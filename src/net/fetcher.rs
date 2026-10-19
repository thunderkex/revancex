// HTTP Fetcher with Cloudflare bypass chain — derived from RookieEnough/Morphe-AutoBuilds (GPL-3.0).
// Adapted from vietanhbui2000/morphe-apps-builder's HttpClient
// https://github.com/RookieEnough/Morphe-AutoBuilds

use crate::config::sources::FetchConfig;
use anyhow::{bail, Context, Result};
use reqwest::header::{HeaderMap, USER_AGENT};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum FetchError {
    Blocked { host: String },
    ChallengeFailed { host: String, reason: String },
    Http { status: StatusCode, url: String },
    Network(anyhow::Error),
}

impl std::error::Error for FetchError {}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blocked { host } => {
                write!(
                    f,
                    "Host {host} is blocked by Cloudflare (all bypass stages failed)"
                )
            }
            Self::ChallengeFailed { host, reason } => {
                write!(f, "Challenge bypass failed for {host}: {reason}")
            }
            Self::Http { status, url } => {
                write!(f, "HTTP error ({status}): {url}")
            }
            Self::Network(e) => write!(f, "{e}"),
        }
    }
}

impl From<anyhow::Error> for FetchError {
    fn from(e: anyhow::Error) -> Self {
        Self::Network(e)
    }
}

#[derive(Debug, Clone)]
pub struct SolvedSolution {
    pub html: String,
    pub cookies: HashMap<String, String>,
    pub user_agent: String,
}

#[derive(Serialize)]
struct FlareSolverrRequest<'a> {
    cmd: &'a str,
    url: &'a str,
    #[serde(rename = "maxTimeout")]
    max_timeout: u64,
}

#[derive(Deserialize)]
struct FlareSolverrCookie {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct FlareSolverrSolution {
    #[serde(default)]
    response: String,
    #[serde(default)]
    cookies: Vec<FlareSolverrCookie>,
    #[serde(rename = "userAgent", default)]
    user_agent: String,
}

#[derive(Deserialize)]
struct FlareSolverrResponse {
    status: String,
    message: Option<String>,
    solution: Option<FlareSolverrSolution>,
}

#[derive(Serialize)]
struct TrawlRequest<'a> {
    url: &'a str,
    #[serde(rename = "maxTimeout")]
    max_timeout: u64,
    #[serde(rename = "skipHttp")]
    skip_http: bool,
    headers: HashMap<&'a str, &'a str>,
}

#[derive(Deserialize)]
struct TrawlResponse {
    #[serde(rename = "statusCode", default)]
    status_code: u16,
    #[serde(default)]
    html: String,
    #[serde(default)]
    cookies: Vec<FlareSolverrCookie>,
    #[serde(rename = "userAgent", default)]
    user_agent: String,
}

pub struct Fetcher {
    client: Client,
    config: FetchConfig,
    flaresolverr_available: AtomicBool,
    trawl_available: AtomicBool,
    probed: AtomicBool,
    host_state: Arc<Mutex<HashMap<String, HostState>>>,
}

#[derive(Default, Clone)]
struct HostState {
    cookies: HashMap<String, String>,
    user_agent: Option<String>,
    sticky_browser: bool,
    blocked: bool,
}

impl Fetcher {
    pub fn new(client: Client, config: FetchConfig) -> Self {
        let fs_enabled = config.flaresolverr.enabled != "false";
        let trawl_enabled = config.trawl.enabled != "false"
            && (!config.trawl.url.is_empty() || std::env::var("TRAWL_URL").is_ok());

        Self {
            client,
            config,
            flaresolverr_available: AtomicBool::new(fs_enabled),
            trawl_available: AtomicBool::new(trawl_enabled),
            probed: AtomicBool::new(false),
            host_state: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub async fn ensure_probed(&self) {
        if self.probed.swap(true, Ordering::SeqCst) {
            return;
        }

        let fs_url = std::env::var("FLARESOLVERR_URL")
            .unwrap_or_else(|_| self.config.flaresolverr.url.clone());
        if self.config.flaresolverr.enabled != "false" {
            let probe_res = self
                .client
                .get(&fs_url)
                .timeout(Duration::from_secs(3))
                .send()
                .await;
            match probe_res {
                Ok(r)
                    if r.status().is_success() || r.status() == StatusCode::METHOD_NOT_ALLOWED =>
                {
                    info!("FlareSolverr available at {fs_url}");
                    self.flaresolverr_available.store(true, Ordering::SeqCst);
                }
                _ => {
                    debug!("FlareSolverr probe failed or absent at {fs_url}");
                    self.flaresolverr_available.store(false, Ordering::SeqCst);
                }
            }
        }

        let trawl_url =
            std::env::var("TRAWL_URL").unwrap_or_else(|_| self.config.trawl.url.clone());
        if !trawl_url.is_empty() && self.config.trawl.enabled != "false" {
            let probe_res = self
                .client
                .get(&trawl_url)
                .timeout(Duration::from_secs(3))
                .send()
                .await;
            match probe_res {
                Ok(r)
                    if r.status().is_success() || r.status() == StatusCode::METHOD_NOT_ALLOWED =>
                {
                    info!("Trawl available at {trawl_url}");
                    self.trawl_available.store(true, Ordering::SeqCst);
                }
                _ => {
                    debug!("Trawl probe failed or absent at {trawl_url}");
                    self.trawl_available.store(false, Ordering::SeqCst);
                }
            }
        }
    }

    pub fn is_challenge(&self, status: StatusCode, headers: &HeaderMap, body: &str) -> bool {
        if let Some(cf_mitigated) = headers.get("cf-mitigated") {
            if cf_mitigated
                .to_str()
                .unwrap_or_default()
                .eq_ignore_ascii_case("challenge")
            {
                return true;
            }
        }

        let is_cf = headers
            .get("server")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("cloudflare"))
            .unwrap_or(false)
            || headers.contains_key("cf-ray");

        if is_cf && (status == StatusCode::FORBIDDEN || status == StatusCode::SERVICE_UNAVAILABLE) {
            return true;
        }

        let sample_len = body.len().min(5120);
        let sample = &body[..sample_len];

        for marker in &self.config.challenge_markers {
            if sample.contains(marker) {
                return true;
            }
        }

        false
    }

    pub fn get_host_credentials(&self, host: &str) -> (Option<String>, HashMap<String, String>) {
        let lock = self.host_state.lock().unwrap();
        if let Some(st) = lock.get(host) {
            (st.user_agent.clone(), st.cookies.clone())
        } else {
            (None, HashMap::new())
        }
    }

    pub async fn get(&self, url: &str) -> Result<String, FetchError> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|e| FetchError::Network(anyhow::anyhow!("Invalid URL {url}: {e}")))?;
        let host = parsed.host_str().unwrap_or("").to_string();

        {
            let lock = self.host_state.lock().unwrap();
            if let Some(st) = lock.get(&host) {
                if st.blocked {
                    return Err(FetchError::Blocked { host });
                }
            }
        }

        self.ensure_probed().await;

        if let Some(&delay_ms) = self.config.per_host_delay_ms.get(&host) {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        let (_sticky, saved_ua, saved_cookies) = {
            let lock = self.host_state.lock().unwrap();
            if let Some(st) = lock.get(&host) {
                (st.sticky_browser, st.user_agent.clone(), st.cookies.clone())
            } else {
                (false, None, HashMap::new())
            }
        };

        let mut req = self.client.get(url);
        if let Some(ua) = &saved_ua {
            req = req.header(USER_AGENT, ua);
        }
        if !saved_cookies.is_empty() {
            let cookie_header = saved_cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            req = req.header("Cookie", cookie_header);
        }

        let direct_res = req.send().await;
        match direct_res {
            Ok(resp) => {
                let status = resp.status();
                let headers = resp.headers().clone();
                let body = resp
                    .text()
                    .await
                    .map_err(|e| FetchError::Network(e.into()))?;

                if self.is_challenge(status, &headers, &body) {
                    let server = headers
                        .get("server")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    let cf_ray = headers
                        .get("cf-ray")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    let cf_mitigated = headers
                        .get("cf-mitigated")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    let body_snippet: String = body.chars().take(200).collect();
                    info!(
                        "challenge detected on {url} (host={host}, status={status}, server={server}, cf-ray={cf_ray}, cf-mitigated={cf_mitigated})"
                    );
                    info!("challenge body snippet: {body_snippet}");
                    if status == StatusCode::FORBIDDEN {
                        info!(
                            "403 details — status={status}, server={server}, cf-ray={cf_ray}, cf-mitigated={cf_mitigated}, body[:200]={body_snippet}"
                        );
                    }
                    // Proceed to browser bypass stages below
                } else if status.is_success() {
                    return Ok(body);
                } else {
                    if status == StatusCode::FORBIDDEN {
                        let server = headers
                            .get("server")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("-");
                        let cf_ray = headers
                            .get("cf-ray")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("-");
                        let cf_mitigated = headers
                            .get("cf-mitigated")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("-");
                        let body_snippet: String = body.chars().take(200).collect();
                        info!(
                            "403 details — status={status}, server={server}, cf-ray={cf_ray}, cf-mitigated={cf_mitigated}, body[:200]={body_snippet}"
                        );
                    }
                    return Err(FetchError::Http {
                        status,
                        url: url.to_string(),
                    });
                }
            }
            Err(e) => {
                debug!("Direct request failed for {url}: {e}");
            }
        }

        if self.flaresolverr_available.load(Ordering::SeqCst) {
            let stage_start = std::time::Instant::now();
            info!("bypass stage starting: host={host}, stage=flaresolverr");
            match self.solve_with_flaresolverr(url, &host).await {
                Ok(sol) => {
                    info!(
                        "bypass stage completed: host={host}, stage=flaresolverr, ms={} — ok",
                        stage_start.elapsed().as_millis()
                    );
                    self.record_solved_host(&host, &sol);
                    return Ok(sol.html);
                }
                Err(e) => {
                    info!(
                        "bypass stage completed: host={host}, stage=flaresolverr, ms={} — failed",
                        stage_start.elapsed().as_millis()
                    );
                    warn!("FlareSolverr bypass failed for {url}: {e}");
                }
            }
        }

        if self.trawl_available.load(Ordering::SeqCst) {
            let stage_start = std::time::Instant::now();
            info!("bypass stage starting: host={host}, stage=trawl");
            match self.solve_with_trawl(url, &host).await {
                Ok(sol) => {
                    info!(
                        "bypass stage completed: host={host}, stage=trawl, ms={} — ok",
                        stage_start.elapsed().as_millis()
                    );
                    self.record_solved_host(&host, &sol);
                    return Ok(sol.html);
                }
                Err(e) => {
                    info!(
                        "bypass stage completed: host={host}, stage=trawl, ms={} — failed",
                        stage_start.elapsed().as_millis()
                    );
                    warn!("Trawl bypass failed for {url}: {e}");
                }
            }
        }

        {
            let mut lock = self.host_state.lock().unwrap();
            let entry = lock.entry(host.clone()).or_default();
            entry.blocked = true;
        }

        Err(FetchError::Blocked { host })
    }

    fn record_solved_host(&self, host: &str, sol: &SolvedSolution) {
        let mut lock = self.host_state.lock().unwrap();
        let entry = lock.entry(host.to_string()).or_default();
        entry.sticky_browser = true;
        entry.user_agent = Some(sol.user_agent.clone());
        for (k, v) in &sol.cookies {
            entry.cookies.insert(k.clone(), v.clone());
        }
    }

    async fn solve_with_flaresolverr(&self, url: &str, _host: &str) -> Result<SolvedSolution> {
        let fs_url = std::env::var("FLARESOLVERR_URL")
            .unwrap_or_else(|_| self.config.flaresolverr.url.clone());
        let endpoint = if fs_url.ends_with("/v1") {
            fs_url.clone()
        } else {
            format!("{}/v1", fs_url.trim_end_matches('/'))
        };

        let req_body = FlareSolverrRequest {
            cmd: "request.get",
            url,
            max_timeout: self.config.flaresolverr.timeout_secs * 1000,
        };

        let resp = self
            .client
            .post(&endpoint)
            .json(&req_body)
            .timeout(Duration::from_secs(
                self.config.flaresolverr.timeout_secs + 10,
            ))
            .send()
            .await
            .with_context(|| format!("Failed to connect to FlareSolverr at {endpoint}"))?;

        if !resp.status().is_success() {
            bail!("FlareSolverr HTTP status {}", resp.status());
        }

        let parsed: FlareSolverrResponse = resp
            .json()
            .await
            .with_context(|| "Failed to parse FlareSolverr response")?;

        if parsed.status != "ok" {
            bail!(
                "FlareSolverr returned status '{}': {}",
                parsed.status,
                parsed.message.unwrap_or_default()
            );
        }

        let sol = parsed
            .solution
            .ok_or_else(|| anyhow::anyhow!("FlareSolverr returned no solution"))?;

        let mut cookies = HashMap::new();
        for c in sol.cookies {
            cookies.insert(c.name, c.value);
        }

        Ok(SolvedSolution {
            html: sol.response,
            cookies,
            user_agent: sol.user_agent,
        })
    }

    async fn solve_with_trawl(&self, url: &str, _host: &str) -> Result<SolvedSolution> {
        let trawl_url =
            std::env::var("TRAWL_URL").unwrap_or_else(|_| self.config.trawl.url.clone());
        let endpoint = if trawl_url.ends_with("/scrape") {
            trawl_url.clone()
        } else {
            format!("{}/scrape", trawl_url.trim_end_matches('/'))
        };

        let mut headers = HashMap::new();
        headers.insert("Referer", url);

        let req_body = TrawlRequest {
            url,
            max_timeout: 60000,
            skip_http: true,
            headers,
        };

        let resp = self
            .client
            .post(&endpoint)
            .json(&req_body)
            .timeout(Duration::from_secs(70))
            .send()
            .await
            .with_context(|| format!("Failed to connect to Trawl at {endpoint}"))?;

        if !resp.status().is_success() {
            bail!("Trawl HTTP status {}", resp.status());
        }

        let parsed: TrawlResponse = resp
            .json()
            .await
            .with_context(|| "Failed to parse Trawl response")?;

        if parsed.status_code != 200 || parsed.html.is_empty() {
            bail!("Trawl returned statusCode {}", parsed.status_code);
        }

        for marker in &self.config.challenge_markers {
            if parsed.html.contains(marker) {
                bail!("Trawl returned challenged page containing marker '{marker}'");
            }
        }

        let mut cookies = HashMap::new();
        for c in parsed.cookies {
            cookies.insert(c.name, c.value);
        }

        Ok(SolvedSolution {
            html: parsed.html,
            cookies,
            user_agent: parsed.user_agent,
        })
    }
}
