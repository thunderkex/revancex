use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use crate::sources::common::{
    download_and_extract_if_bundle, fetch_page_html, is_ip_reputation_blocked, is_turnstile_blocked,
};
use crate::utils::version::is_beta_or_alpha;
use anyhow::Result;
use regex::Regex;
use reqwest::Client;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum UptodownError {
    TurnstileBlocked,
    IpReputationBlocked,
    TokenNotFound { html_len: usize, snippet: String },
    Network(anyhow::Error),
}

impl std::fmt::Display for UptodownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UptodownError::TurnstileBlocked => {
                write!(
                    f,
                    "Uptodown: Cloudflare Turnstile challenge detected (domain globally blocked)"
                )
            }
            UptodownError::IpReputationBlocked => {
                write!(f, "Uptodown: IP reputation block (403/1020) detected")
            }
            UptodownError::TokenNotFound { html_len, snippet } => {
                write!(
                    f,
                    "Uptodown: data-url token not found (page {html_len} bytes); snippet: {snippet}"
                )
            }
            UptodownError::Network(e) => write!(f, "Uptodown: network error: {e}"),
        }
    }
}

impl std::error::Error for UptodownError {}

pub struct UptodownSource;

impl ApkSource for UptodownSource {
    fn name(&self) -> &'static str {
        "uptodown"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if let Err((_st, reason)) = ctx.can_try("uptodown") {
                tracing::warn!("{reason} — skipping uptodown");
                return Ok(None);
            }
            if app.uptodown_url.is_none() {
                tracing::info!("uptodown skipped: not configured");
                return Ok(None);
            }
            let url = app.uptodown_url.as_deref().unwrap();
            Ok(Some(Resolved::simple(
                url.to_string(),
                version.map(str::to_string),
                arch,
            )))
        })
    }

    fn fetch<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        resolved: &'a Resolved,
        dest: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            let res = fetch_uptodown(
                ctx.client,
                &resolved.url,
                &resolved.arch,
                dest,
                resolved.version_found.as_deref(),
                ctx.fetcher,
            )
            .await;
            match res {
                Ok(path) => {
                    ctx.record_success("uptodown");
                    Ok(PathBuf::from(path))
                }
                Err(UptodownError::TurnstileBlocked) => {
                    ctx.record_failure(
                        "uptodown",
                        &crate::sources::breaker::BlockKind::TurnstileChallenge,
                    );
                    tracing::warn!("uptodown TurnstileBlocked");
                    Err(anyhow::anyhow!("uptodown TurnstileBlocked"))
                }
                Err(UptodownError::IpReputationBlocked) => {
                    ctx.record_failure(
                        "uptodown",
                        &crate::sources::breaker::BlockKind::IpReputationBlock,
                    );
                    tracing::warn!("uptodown IpReputationBlocked (403/1020)");
                    Err(anyhow::anyhow!("uptodown IpReputationBlocked (403/1020)"))
                }
                Err(e) => Err(anyhow::anyhow!("{e}")),
            }
        })
    }
}

pub(crate) async fn fetch_uptodown(
    client: &Client,
    base_url: &str,
    _arch: &str,
    dest: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String, UptodownError> {
    let versions_url = format!("{base_url}/versions");
    info!("uptodown: checking versions at {versions_url}");

    let (html_bytes, versions_headers) = match fetcher {
        Some(f) => match fetch_page_html(client, &versions_url, Some(f)).await {
            Ok((body, hdrs)) => (body, hdrs),
            Err(e) => return Err(UptodownError::Network(e)),
        },
        None => {
            let resp = client
                .get(&versions_url)
                .send()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            let hdrs = resp.headers().clone();
            let body = resp
                .text()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            (body, hdrs)
        }
    };
    let versions_status = 200u16;
    let html = html_bytes;

    if is_ip_reputation_blocked(versions_status, &html) {
        warn!("uptodown: IpReputationBlocked on versions page (status={versions_status})");
        return Err(UptodownError::IpReputationBlocked);
    }

    if is_turnstile_blocked(versions_status, &versions_headers, &html) {
        warn!("uptodown: TurnstileBlocked on versions page (status={versions_status})");
        return Err(UptodownError::TurnstileBlocked);
    }

    static RE_CODE: OnceLock<Regex> = OnceLock::new();
    let re_code = RE_CODE.get_or_init(|| {
        Regex::new(r#"id="detail-app-name"[^>]*data-code="([0-9]+)""#)
            .expect("valid uptodown code regex")
    });
    let data_code = re_code
        .captures(&html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| {
            let snippet = html.chars().take(200).collect::<String>();
            UptodownError::TokenNotFound {
                html_len: html.len(),
                snippet,
            }
        })?;

    let api_url = format!("{base_url}/apps/{data_code}/versions/1");
    info!("uptodown: querying version api {api_url}");

    let res = client
        .get(&api_url)
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;
    let data = json["data"].as_array().ok_or_else(|| {
        UptodownError::Network(anyhow::anyhow!(
            "Unexpected JSON from uptodown versions API"
        ))
    })?;

    if data.is_empty() {
        return Err(UptodownError::Network(anyhow::anyhow!(
            "No versions returned by uptodown API"
        )));
    }

    let allow_beta = target_version.is_some_and(is_beta_or_alpha);

    let chosen_item = if let Some(ver) = target_version {
        let base_ver = ver.split('-').next().unwrap_or(ver);
        data.iter().find(|item| {
            let v = item["version"].as_str().unwrap_or("");
            (allow_beta || !is_beta_or_alpha(v)) && (v.contains(ver) || v.contains(base_ver))
        })
    } else {
        data.iter().find(|item| {
            let v = item["version"].as_str().unwrap_or("");
            allow_beta || !is_beta_or_alpha(v)
        })
    };

    let item = chosen_item.ok_or_else(|| {
        UptodownError::Network(anyhow::anyhow!(
            "No suitable non-beta versions from uptodown API"
        ))
    })?;
    let file_id = item["fileID"].as_i64().unwrap_or(0);
    info!(
        "uptodown: selected version {} (fileID: {file_id})",
        item["version"].as_str().unwrap_or("unknown")
    );

    let dl_page = format!("{base_url}/download/{file_id}-x");
    let (dl_html, dl_headers) = match fetcher {
        Some(f) => match fetch_page_html(client, &dl_page, Some(f)).await {
            Ok((body, hdrs)) => (body, hdrs),
            Err(e) => return Err(UptodownError::Network(e)),
        },
        None => {
            let resp = client
                .get(&dl_page)
                .send()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            let hdrs = resp.headers().clone();
            let body = resp
                .text()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            (body, hdrs)
        }
    };
    let dl_status = 200u16;

    debug!("uptodown: download page {} bytes", dl_html.len());

    if is_ip_reputation_blocked(dl_status, &dl_html) {
        warn!("uptodown: IpReputationBlocked on download page (status={dl_status})");
        return Err(UptodownError::IpReputationBlocked);
    }

    if is_turnstile_blocked(dl_status, &dl_headers, &dl_html) {
        warn!("uptodown: TurnstileBlocked on download page (status={dl_status})");
        return Err(UptodownError::TurnstileBlocked);
    }

    static RE_DWN: OnceLock<Regex> = OnceLock::new();
    let re_dwn = RE_DWN
        .get_or_init(|| Regex::new(r#"data-url="([^"]+)""#).expect("valid uptodown dwn regex"));

    if let Some(cap) = re_dwn.captures(&dl_html) {
        let token = &cap[1];
        if !token.starts_with("http") && token.len() > 10 {
            let direct = format!("https://dw.uptodown.com/dwn/{token}");
            info!("uptodown: downloading from direct token {direct}");
            return download_and_extract_if_bundle(client, &direct, dest, Some(base_url))
                .await
                .map(|_| dest.to_string())
                .map_err(UptodownError::Network);
        }
    }

    let snippet = dl_html.chars().take(300).collect::<String>();
    debug!("uptodown: data-url not found; page snippet: {snippet}");
    Err(UptodownError::TokenNotFound {
        html_len: dl_html.len(),
        snippet,
    })
}
