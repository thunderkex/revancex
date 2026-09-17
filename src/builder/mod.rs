pub mod arch;
pub mod lite;

use crate::config::{AppConfig, Config};
use crate::patcher::metadata;
use crate::utils::semver::{compare_version_parts, parse_version_numbers};
use anyhow::{Context, Result};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use reqwest::Client;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum UptodownError {
    TurnstileBlocked,
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
            UptodownError::TokenNotFound { html_len, snippet } => {
                write!(f, "Uptodown: data-url token not found (page {html_len} bytes); snippet: {snippet}")
            }
            UptodownError::Network(e) => write!(f, "Uptodown: network error: {e}"),
        }
    }
}

impl std::error::Error for UptodownError {}

#[derive(Debug)]
pub enum ApkpureError {
    Forbidden { url: String },
    Other(anyhow::Error),
}

impl std::fmt::Display for ApkpureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApkpureError::Forbidden { url } => {
                write!(
                    f,
                    "apkpure: 403 pada {url} — kemungkinan Referer salah / rate-limit / IP diblokir"
                )
            }
            ApkpureError::Other(e) => write!(f, "apkpure: {e}"),
        }
    }
}

impl std::error::Error for ApkpureError {}

pub fn is_turnstile_blocked(status: u16, headers: &reqwest::header::HeaderMap, body: &str) -> bool {
    if headers.contains_key("cf-mitigated") {
        return true;
    }
    if (status == 403 || status == 503) && headers.contains_key("cf-ray") {
        return true;
    }
    body.contains("cf-turnstile")
        || body.contains("challenges.cloudflare.com")
        || body.contains("cf_chl_opt")
}

fn make_client() -> Result<Client> {
    let mut headers = HeaderMap::new();
    let ua = crate::utils::random_user_agent();
    let val = HeaderValue::from_str(ua)
        .unwrap_or_else(|_| HeaderValue::from_static(crate::utils::DEFAULT_UA));
    headers.insert(USER_AGENT, val);
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::from_static(
            "\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"",
        ),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-mobile"),
        HeaderValue::from_static("?0"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-platform"),
        HeaderValue::from_static("\"Windows\""),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("document"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("navigate"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("upgrade-insecure-requests"),
        HeaderValue::from_static("1"),
    );

    Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(Into::into)
}

pub async fn resolve_all_patch_compatible_versions(cfg: &Config, app: &AppConfig) -> Vec<String> {
    let Some(mpp_path) = metadata::resolve_mpp_path(cfg, app) else {
        return Vec::new();
    };
    let cli_jar = format!("{}/morphe-cli.jar", cfg.build.tools_dir);
    if !Path::new(&cli_jar).exists() {
        return Vec::new();
    }
    let patches = match metadata::load(cfg, &mpp_path).await {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let enabled = app
        .included_patches
        .iter()
        .chain(app.patches.iter())
        .cloned()
        .collect::<Vec<_>>();
    metadata::resolve_compatible_versions(&patches, &app.package, &enabled)
}

pub async fn resolve_patch_compatible_version(cfg: &Config, app: &AppConfig) -> Option<String> {
    crate::utils::semver::max_version(resolve_all_patch_compatible_versions(cfg, app).await)
}

pub async fn fetch_apk(
    cfg: &Config,
    id: &str,
    app: &AppConfig,
    arch: &str,
    force: bool,
    uptodown_cb: Option<Arc<AtomicU32>>,
) -> Result<String> {
    let arch_tag = if arch == "all" || arch.is_empty() {
        "all"
    } else {
        arch
    };
    let dest = format!("{}/{id}-{arch_tag}-input.apk", cfg.build.temp_dir);
    let all_dest = format!("{}/{id}-all-input.apk", cfg.build.temp_dir);
    let legacy_dest = format!("{}/{id}-input.apk", cfg.build.temp_dir);

    if !force {
        if Path::new(&dest).exists() && std::fs::metadata(&dest)?.len() > 1_000_000 {
            info!("{id}: cached input APK exists at {dest}");
            return Ok(dest);
        }
        if Path::new(&all_dest).exists() && std::fs::metadata(&all_dest)?.len() > 1_000_000 {
            info!("{id}: cached universal input APK exists at {all_dest}");
            return Ok(all_dest);
        }
        if Path::new(&legacy_dest).exists() && std::fs::metadata(&legacy_dest)?.len() > 1_000_000 {
            info!("{id}: cached legacy input APK exists at {legacy_dest}");
            return Ok(legacy_dest);
        }
    }
    if force {
        for p in [&dest, &all_dest, &legacy_dest] {
            if Path::new(p).exists() {
                info!("{id}: --force: removing cached input APK {p}");
                let _ = std::fs::remove_file(p);
            }
        }
    }

    let client = make_client()?;

    if let Some(url) = &app.apk_url {
        let final_url = if url.contains("github.com") && url.contains("/releases/") {
            resolve_github_asset_url(&client, url, arch, &cfg.build.asset_exclude_patterns)
                .await
                .unwrap_or_else(|_| url.clone())
        } else {
            url.clone()
        };
        info!("{id}: downloading from direct apk_url {final_url}");
        match download_and_extract_if_bundle(&client, &final_url, &dest, None).await {
            Ok(_) => return Ok(dest),
            Err(e) => warn!("{id}: direct apk_url failed: {e}"),
        }
    }

    let configured_ver = app
        .version
        .as_deref()
        .or(app.max_version.as_deref())
        .or(app.min_version.as_deref());

    let patch_supported_versions = resolve_all_patch_compatible_versions(cfg, app).await;

    let target_ver = if configured_ver.is_none() || configured_ver == Some("auto") {
        let detected = crate::utils::semver::max_version(&patch_supported_versions);
        if let Some(ref v) = detected {
            info!("{id}: resolved auto version from patch: {v}");
        }
        detected
    } else {
        configured_ver.map(String::from)
    };

    let supported_refs: Vec<&str> = patch_supported_versions
        .iter()
        .map(|s| s.as_str())
        .collect();

    if let Some(url) = &app.archive_url {
        info!("{id}: checking archive.org source {url}");
        match fetch_archive_org(
            &client,
            url,
            arch,
            &dest,
            target_ver.as_deref(),
            &supported_refs,
        )
        .await
        {
            Ok(path) => return Ok(path),
            Err(e) => warn!("{id}: archive.org failed: {e}"),
        }
    }

    let default_order: &[&str] = &["apkmirror", "uptodown", "apkpure", "apkeep"];
    let priority: Vec<&str> = if app.source_priority.is_empty() {
        default_order.to_vec()
    } else {
        app.source_priority.iter().map(|s| s.as_str()).collect()
    };

    let cb_threshold: u32 = std::env::var("REVANCEX_UTD_CB_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let local_cb;
    let cb = match &uptodown_cb {
        Some(c) => c.as_ref(),
        None => {
            local_cb = Arc::new(AtomicU32::new(0));
            &local_cb
        }
    };

    let mut attempted: Vec<String> = Vec::new();

    'sources: for source in &priority {
        match *source {
            "apkmirror" => {
                if let Some(url) = &app.apkmirror_url {
                    info!("{id}: checking apkmirror source {url}");
                    attempted.push("apkmirror".into());
                    match fetch_apkmirror(&client, url, arch, &dest, target_ver.as_deref()).await {
                        Ok(path) => {
                            info!("{id}: BUILD_REPORT resolved_source=apkmirror attempted={attempted:?}");
                            return Ok(path);
                        }
                        Err(e) => warn!("{id}: apkmirror failed: {e}"),
                    }
                }
            }
            "uptodown" => {
                if let Some(url) = &app.uptodown_url {
                    let cb_count = cb.load(AtomicOrdering::SeqCst);
                    if cb_count >= cb_threshold {
                        warn!(
                            "{id}: uptodown circuit-breaker open ({cb_count}/{cb_threshold} consecutive TurnstileBlocked) — skipping uptodown for this app"
                        );
                        attempted.push("uptodown(skipped-cb)".into());
                        continue 'sources;
                    }

                    info!("{id}: checking uptodown source {url}");
                    attempted.push("uptodown".into());
                    match fetch_uptodown(&client, url, arch, &dest, target_ver.as_deref()).await {
                        Ok(path) => {
                            cb.store(0, AtomicOrdering::SeqCst);
                            info!("{id}: BUILD_REPORT resolved_source=uptodown attempted={attempted:?}");
                            return Ok(path);
                        }
                        Err(UptodownError::TurnstileBlocked) => {
                            let new_count = cb.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                            warn!(
                                "{id}: uptodown TurnstileBlocked (circuit-breaker {new_count}/{cb_threshold})"
                            );
                            if new_count >= cb_threshold {
                                warn!(
                                    "uptodown appears globally blocked this run, skipping it for remaining apps"
                                );
                            }
                        }
                        Err(e) => warn!("{id}: uptodown failed: {e}"),
                    }
                }
            }
            "apkpure" => {
                if !app.package.is_empty() {
                    info!("{id}: trying apkpure for package {}", app.package);
                    attempted.push("apkpure".into());
                    let result = if let Some(base) = &app.apkpure_url {
                        fetch_apkpure_url(&client, base, &dest, arch, target_ver.as_deref()).await
                    } else {
                        fetch_apkpure(&client, &app.package, &dest, arch, target_ver.as_deref())
                            .await
                    };
                    match result {
                        Ok(path) => {
                            info!("{id}: BUILD_REPORT resolved_source=apkpure attempted={attempted:?}");
                            return Ok(path);
                        }
                        Err(e) => warn!("{id}: apkpure failed: {e}"),
                    }
                }
            }
            "apkeep" => {
                if !app.package.is_empty() {
                    info!("{id}: trying apkeep for package {}", app.package);
                    attempted.push("apkeep".into());
                    match fetch_apkeep(&app.package, &dest, target_ver.as_deref()).await {
                        Ok(path) => {
                            info!(
                                "{id}: BUILD_REPORT resolved_source=apkeep attempted={attempted:?}"
                            );
                            return Ok(path);
                        }
                        Err(e) => warn!("{id}: apkeep failed: {e}"),
                    }
                }
            }
            other => {
                warn!("{id}: unknown source '{other}' in source_priority, skipping");
            }
        }
    }

    anyhow::bail!(
        "{id}: all download sources exhausted for package '{}'",
        app.package
    );
}

fn is_beta_or_alpha(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("beta") || lower.contains("alpha")
}

fn extract_version_from_filename(filename: &str) -> String {
    let stem = filename.trim_end_matches(".apkm").trim_end_matches(".apk");
    let mut parts = stem.split('-');
    let _ = parts.next();
    for part in parts {
        if part.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            let ver: String = part
                .split('-')
                .take_while(|s| s.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .collect::<Vec<_>>()
                .join(".");
            if !ver.is_empty() {
                return ver;
            }
        }
    }
    String::new()
}

async fn fetch_archive_org(
    client: &Client,
    url: &str,
    arch: &str,
    dest: &str,
    target_version: Option<&str>,
    supported_versions: &[&str],
) -> Result<String> {
    let base = if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    };

    info!("archive.org: listing files from {base}");
    let html = client.get(&base).send().await?.text().await?;

    let re = Regex::new(r#"href="([^"]+\.(?:apk|apkm|xapk))""#)?;
    let mut candidates: Vec<String> = Vec::new();

    for cap in re.captures_iter(&html) {
        let file = &cap[1];
        if file.contains('/') || file.starts_with('?') || file.contains('~') {
            continue;
        }
        candidates.push(file.to_string());
    }

    if candidates.is_empty() {
        anyhow::bail!("No APK/APKM found in archive.org directory {base}");
    }

    let allow_beta = target_version.is_some_and(is_beta_or_alpha);
    let clean_candidates: Vec<String> = candidates
        .into_iter()
        .filter(|c| allow_beta || !is_beta_or_alpha(c))
        .collect();

    if clean_candidates.is_empty() {
        anyhow::bail!("No non-beta APK/APKM found in archive.org directory {base}");
    }

    let chosen = if let Some(ver) = target_version {
        let base_ver = ver.split('-').next().unwrap_or(ver);
        let exact = clean_candidates
            .iter()
            .filter(|f| f.contains(ver) || f.contains(base_ver))
            .rfind(|f| {
                f.contains(arch)
                    || f.contains("-all.")
                    || f.contains("_all.")
                    || f.contains("universal")
            })
            .or_else(|| {
                clean_candidates
                    .iter()
                    .rfind(|f| f.contains(ver) || f.contains(base_ver))
            });

        if let Some(m) = exact {
            m
        } else {
            let supported_match = supported_versions
                .iter()
                .rev()
                .filter(|&&sv| sv != ver)
                .find_map(|&sv| {
                    let s_base = sv.split('-').next().unwrap_or(sv);
                    clean_candidates
                        .iter()
                        .filter(|f| f.contains(sv) || f.contains(s_base))
                        .rfind(|f| {
                            f.contains(arch)
                                || f.contains("-all.")
                                || f.contains("_all.")
                                || f.contains("universal")
                        })
                        .or_else(|| {
                            clean_candidates
                                .iter()
                                .rfind(|f| f.contains(sv) || f.contains(s_base))
                        })
                });

            if let Some(sm) = supported_match {
                info!("archive.org: exact version {ver} not found, matched supported version in archive: {sm}");
                sm
            } else if supported_versions.is_empty() {
                let target_parts = parse_version_numbers(ver);
                let mut best: Option<&String> = None;
                let mut best_parts: Vec<u64> = Vec::new();
                for f in &clean_candidates {
                    let file_ver = extract_version_from_filename(f);
                    if file_ver.is_empty() {
                        continue;
                    }
                    let fparts = parse_version_numbers(&file_ver);
                    if compare_version_parts(&fparts, &target_parts) != Ordering::Greater {
                        if compare_version_parts(&fparts, &best_parts) == Ordering::Greater
                            || best.is_none()
                        {
                            best = Some(f);
                            best_parts = fparts;
                        } else if compare_version_parts(&fparts, &best_parts) == Ordering::Equal
                            && (f.contains(arch) || f.contains("-all.") || f.contains("_all."))
                        {
                            best = Some(f);
                        }
                    }
                }
                best.ok_or_else(|| {
                    anyhow::anyhow!(
                        "archive.org: version {ver} not found and no older version available"
                    )
                })?
            } else {
                let fallback = clean_candidates
                    .iter()
                    .rfind(|f| {
                        f.contains(arch)
                            || f.contains("-all.")
                            || f.contains("_all.")
                            || f.contains("universal")
                    })
                    .or_else(|| clean_candidates.last());
                if let Some(fb) = fallback {
                    warn!("archive.org: exact version {ver} and supported versions not found, falling back to newest available candidate in archive: {fb}");
                    fb
                } else {
                    anyhow::bail!("archive.org: version {ver} and supported versions not found in archive candidates");
                }
            }
        }
    } else {
        clean_candidates
            .iter()
            .rfind(|f| f.contains(arch))
            .or_else(|| {
                clean_candidates
                    .iter()
                    .rfind(|f| f.contains("-all.") || f.contains("_all."))
            })
            .or_else(|| clean_candidates.iter().rfind(|f| f.contains("universal")))
            .or_else(|| clean_candidates.last())
            .ok_or_else(|| anyhow::anyhow!("No matching file candidate"))?
    };

    let download_url = format!("{base}{chosen}");
    info!("archive.org: downloading {download_url}");
    download_and_extract_if_bundle(client, &download_url, dest, None).await?;
    Ok(dest.to_string())
}

async fn fetch_apkmirror(
    client: &Client,
    app_url: &str,
    arch: &str,
    dest: &str,
    target_version: Option<&str>,
) -> Result<String> {
    info!("apkmirror: loading app page {app_url}");
    let base_origin = "https://www.apkmirror.com";
    let app_html = client.get(app_url).send().await?.text().await?;

    let re_rel = Regex::new(r#"href="(/apk/[^"]+-release/)""#)?;
    let mut all_releases: Vec<String> = Vec::new();
    for cap in re_rel.captures_iter(&app_html) {
        all_releases.push(cap[1].to_string());
    }

    if all_releases.is_empty() {
        anyhow::bail!("No release links found on {app_url}");
    }

    let allow_beta = target_version.is_some_and(is_beta_or_alpha);
    let releases: Vec<String> = all_releases
        .into_iter()
        .filter(|r| allow_beta || !is_beta_or_alpha(r))
        .collect();

    if releases.is_empty() {
        anyhow::bail!("Only beta/alpha release links found on {app_url}");
    }

    let release_path = if let Some(ver) = target_version {
        let hyphen_ver = ver.replace('.', "-");
        let base_hyphen = ver.split('-').next().unwrap_or(ver).replace('.', "-");
        releases
            .iter()
            .find(|r| r.contains(&hyphen_ver) || r.contains(&base_hyphen) || r.contains(ver))
            .or_else(|| releases.first())
            .unwrap()
    } else {
        releases.first().unwrap()
    };

    let release_url = format!("{base_origin}{release_path}");
    info!("apkmirror: loading release {release_url}");
    let rel_html = client.get(&release_url).send().await?.text().await?;

    let re_var = Regex::new(r#"href="(/apk/[^"]+-android-apk-download/)""#)?;
    let mut variants: Vec<String> = Vec::new();
    for cap in re_var.captures_iter(&rel_html) {
        variants.push(cap[1].to_string());
    }
    if variants.is_empty() {
        anyhow::bail!("No variant links found on {release_url}");
    }

    let variant_path = variants
        .iter()
        .find(|v| v.contains(arch))
        .or_else(|| variants.first())
        .unwrap();

    let variant_url = format!("{base_origin}{variant_path}");
    info!("apkmirror: loading variant {variant_url}");
    let var_html = client.get(&variant_url).send().await?.text().await?;

    let re_dl = Regex::new(r#"href="(/apk/[^"]+/download/\?key=[^"]+)""#)?;
    let dl_path = re_dl
        .captures(&var_html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow::anyhow!("No intermediate download button found on {variant_url}"))?;

    let dl_url = format!("{base_origin}{dl_path}");
    info!("apkmirror: loading download landing page {dl_url}");
    let dl_html = client.get(&dl_url).send().await?.text().await?;

    let re_php = Regex::new(r#"href="(/wp-content/themes/APKMirror/download\.php\?[^"]+)""#)?;
    let php_path = re_php
        .captures(&dl_html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow::anyhow!("No download.php link found on {dl_url}"))?;

    let php_url = format!("{base_origin}{php_path}");
    info!("apkmirror: resolving direct CDN location from {php_url}");

    let mut hdrs = HeaderMap::new();
    let ua = crate::utils::random_user_agent();
    let val = HeaderValue::from_str(ua)
        .unwrap_or_else(|_| HeaderValue::from_static(crate::utils::DEFAULT_UA));
    hdrs.insert(USER_AGENT, val);
    let no_redir_client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(hdrs)
        .build()?;

    let res = no_redir_client
        .get(&php_url)
        .header("Referer", &dl_url)
        .send()
        .await?;

    let cdn_url = if let Some(loc) = res.headers().get("location") {
        loc.to_str()?.to_string()
    } else {
        php_url
    };

    info!("apkmirror: downloading from CDN {cdn_url}");
    download_and_extract_if_bundle(client, &cdn_url, dest, Some(&dl_url)).await?;
    Ok(dest.to_string())
}

async fn fetch_uptodown(
    client: &Client,
    base_url: &str,
    _arch: &str,
    dest: &str,
    target_version: Option<&str>,
) -> Result<String, UptodownError> {
    let versions_url = format!("{base_url}/versions");
    info!("uptodown: checking versions at {versions_url}");

    let versions_resp = client
        .get(&versions_url)
        .send()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;

    let versions_status = versions_resp.status().as_u16();
    let versions_headers = versions_resp.headers().clone();
    let html = versions_resp
        .text()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;

    if is_turnstile_blocked(versions_status, &versions_headers, &html) {
        warn!("uptodown: TurnstileBlocked on versions page (status={versions_status})");
        return Err(UptodownError::TurnstileBlocked);
    }

    let re_code = Regex::new(r#"id="detail-app-name"[^>]*data-code="([0-9]+)""#)
        .map_err(|e| UptodownError::Network(e.into()))?;
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
        data.iter()
            .find(|item| {
                let v = item["version"].as_str().unwrap_or("");
                (allow_beta || !is_beta_or_alpha(v)) && (v.contains(ver) || v.contains(base_ver))
            })
            .or_else(|| {
                data.iter().find(|item| {
                    let v = item["version"].as_str().unwrap_or("");
                    allow_beta || !is_beta_or_alpha(v)
                })
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
    let dl_resp = client
        .get(&dl_page)
        .send()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;

    let dl_status = dl_resp.status().as_u16();
    let dl_headers = dl_resp.headers().clone();
    let dl_html = dl_resp
        .text()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;

    debug!(
        "uptodown: download page {} bytes (status={dl_status})",
        dl_html.len()
    );

    if is_turnstile_blocked(dl_status, &dl_headers, &dl_html) {
        warn!("uptodown: TurnstileBlocked on download page (status={dl_status})");
        return Err(UptodownError::TurnstileBlocked);
    }

    let re_dwn =
        Regex::new(r#"data-url="([^"]+)""#).map_err(|e| UptodownError::Network(e.into()))?;

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

async fn resolve_github_asset_url(
    client: &Client,
    url: &str,
    arch: &str,
    exclude_patterns: &[String],
) -> Result<String> {
    let re = Regex::new(r#"github\.com/([^/]+)/([^/]+)/releases"#)?;
    if let Some(cap) = re.captures(url) {
        let owner = &cap[1];
        let repo = &cap[2];
        let api_url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
        let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
        let mut req = client.get(&api_url);
        if !token.is_empty() {
            req = req.bearer_auth(&token);
        }
        let release: serde_json::Value = req.send().await?.json().await?;
        if let Some(assets) = release["assets"].as_array() {
            let mut candidates: Vec<String> = Vec::new();
            for asset in assets {
                let name = asset["name"].as_str().unwrap_or("");
                let is_excluded = exclude_patterns.iter().any(|pat| name.contains(pat));
                if name.ends_with(".apk") && !is_excluded {
                    if let Some(dl) = asset["browser_download_url"].as_str() {
                        candidates.push(dl.to_string());
                    }
                }
            }
            if let Some(dl) = candidates.iter().find(|u| u.contains(arch)) {
                return Ok(dl.clone());
            }
            if let Some(dl) = candidates
                .iter()
                .find(|u| u.contains("all") || u.contains("universal"))
            {
                return Ok(dl.clone());
            }
            if let Some(dl) = candidates.first() {
                return Ok(dl.clone());
            }
        }
    }
    Ok(url.to_string())
}

async fn fetch_apkeep(package: &str, dest: &str, target_version: Option<&str>) -> Result<String> {
    let tmp_dir = tempfile::tempdir()?;
    let out_dir = tmp_dir.path().to_str().unwrap();

    let mut success = false;
    if let Some(ver) = target_version {
        let app_arg = format!("{package}@{ver}");
        info!("apkeep: attempting targeted download {app_arg}");
        let res = tokio::process::Command::new("apkeep")
            .args(["-a", &app_arg, "-d", "apk-pure", out_dir])
            .status()
            .await;
        if let Ok(st) = res {
            if st.success() {
                success = true;
            } else {
                warn!("apkeep: targeted download for {app_arg} failed (exit {st}), falling back to latest");
            }
        }
    }

    if !success {
        info!("apkeep: attempting download for {package} (latest)");
        let status = tokio::process::Command::new("apkeep")
            .args(["-a", package, "-d", "apk-pure", out_dir])
            .status()
            .await
            .context("Running apkeep")?;
        if !status.success() {
            anyhow::bail!("apkeep exited with status {status}");
        }
    }

    for entry in std::fs::read_dir(out_dir)? {
        let entry = entry?;
        let p = entry.path();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let p_str = p.to_string_lossy().to_string();
        if ext == "xapk" || ext == "apkm" || is_zip_file(&p_str) || is_split_bundle(&p_str) {
            info!("apkeep: processing bundle file {p_str}");
            if is_split_bundle(&p_str) {
                if let Ok(()) = merge_split_apk_bundle(&p_str, dest).await {
                    let _ = strip_split_requirements_from_apk(dest);
                    return Ok(dest.to_string());
                } else {
                    tracing::warn!(
                        "apkeep: merge_split_apk_bundle failed, attempting base APK extraction"
                    );
                }
            }
            if extract_base_apk_from_zip(&p_str, dest).is_ok() {
                let _ = strip_split_requirements_from_apk(dest);
                return Ok(dest.to_string());
            }
        }
        if ext == "apk" {
            std::fs::copy(&p, dest)?;
            let _ = strip_split_requirements_from_apk(dest);
            return Ok(dest.to_string());
        }
    }

    anyhow::bail!("apkeep finished but produced no valid .apk or .xapk in {out_dir}")
}

pub async fn download_and_extract_if_bundle(
    client: &Client,
    url: &str,
    dest_apk_path: &str,
    referer: Option<&str>,
) -> Result<()> {
    if let Some(parent) = Path::new(dest_apk_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temp_dl = format!("{dest_apk_path}.tmp");
    crate::utils::download_file_with_client(client, url, &temp_dl, 3, referer).await?;

    if is_zip_file(&temp_dl) {
        info!("Checking if downloaded file is a ZIP bundle (.apkm/.xapk/split) or APK...");
        if is_split_bundle(&temp_dl) {
            info!("Detected split APK bundle. Merging splits using APKEditor...");
            if let Ok(()) = merge_split_apk_bundle(&temp_dl, dest_apk_path).await {
                let _ = std::fs::remove_file(&temp_dl);
                let _ = strip_split_requirements_from_apk(dest_apk_path);
                return Ok(());
            } else {
                tracing::warn!("APKEditor merge failed, falling back to base APK extraction");
            }
        }
        if extract_base_apk_from_zip(&temp_dl, dest_apk_path).is_ok() {
            let _ = std::fs::remove_file(&temp_dl);
            let _ = strip_split_requirements_from_apk(dest_apk_path);
            return Ok(());
        }
    }

    std::fs::rename(&temp_dl, dest_apk_path)?;
    let _ = strip_split_requirements_from_apk(dest_apk_path);
    Ok(())
}

async fn fetch_apkpure_url(
    client: &Client,
    base_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
) -> Result<String, ApkpureError> {
    if base_url.starts_with("http") && (base_url.ends_with(".apk") || base_url.ends_with(".xapk")) {
        info!("apkpure: using explicit direct URL override {base_url}");
        download_and_extract_if_bundle(client, base_url, dest, Some("https://apkpure.com/"))
            .await
            .map_err(|e| {
                if e.to_string().contains("403") {
                    ApkpureError::Forbidden {
                        url: base_url.to_string(),
                    }
                } else {
                    ApkpureError::Other(e)
                }
            })?;
        return Ok(dest.to_string());
    }
    fetch_apkpure(client, base_url, dest, arch, target_version).await
}

async fn resolve_apkpure_cdn_url(url: &str, referer: Option<&str>) -> String {
    if !url.contains("d.apkpure.") {
        return url.to_string();
    }
    let no_redirect_client = match Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return url.to_string(),
    };

    let referers_to_try = [
        referer,
        Some("https://apkpure.com/"),
        Some("https://apkpure.net/"),
        None,
    ];

    for r_opt in referers_to_try {
        let mut req = no_redirect_client
            .get(url)
            .header(reqwest::header::USER_AGENT, crate::utils::DEFAULT_UA)
            .header(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            );
        if let Some(r) = r_opt {
            req = req.header(reqwest::header::REFERER, r);
        }
        if let Ok(resp) = req.send().await {
            if resp.status().is_redirection() {
                if let Some(loc) = resp.headers().get(reqwest::header::LOCATION) {
                    if let Ok(loc_str) = loc.to_str() {
                        info!("apkpure: resolved CDN redirect {url} -> {loc_str}");
                        return loc_str.to_string();
                    }
                }
            }
        }
    }
    url.to_string()
}

async fn fetch_apkpure(
    client: &Client,
    package_or_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
) -> Result<String, ApkpureError> {
    info!("apkpure: resolving download for {package_or_url}");
    let (app_url, package) = if package_or_url.starts_with("http") {
        let pkg = package_or_url
            .split('/')
            .rfind(|s| !s.is_empty())
            .unwrap_or(package_or_url)
            .to_string();
        (package_or_url.to_string(), pkg)
    } else {
        (
            format!("https://apkpure.com/app/{package_or_url}"),
            package_or_url.to_string(),
        )
    };

    let app_resp = client
        .get(&app_url)
        .header("Referer", "https://apkpure.com/")
        .send()
        .await;

    let canonical_url = match app_resp {
        Ok(r) if r.status().is_success() => {
            let u = r.url().as_str().trim_end_matches('/').to_string();
            info!("apkpure: canonical app URL is {u}");
            Some(u)
        }
        Ok(r) => {
            warn!(
                "apkpure: app page {app_url} returned status {}, proceeding to direct endpoints",
                r.status()
            );
            None
        }
        Err(e) => {
            warn!("apkpure: app page {app_url} request error: {e}, proceeding to direct endpoints");
            None
        }
    };

    let mut dl_pages = Vec::new();
    if let Some(ref can_url) = canonical_url {
        if let Some(ver) = target_version {
            let ver_clean = ver.replace(' ', "-");
            dl_pages.push(format!("{can_url}/download/{ver_clean}"));
        }
        dl_pages.push(format!("{can_url}/download"));
    }

    let re_dl_link = Regex::new(
        r#"href=[\x22\x27](https://d\.apkpure\.(?:com|net)/b/(?:XAPK|APK)/[^\x22\x27]+)[\x22\x27]"#,
    )
    .map_err(|e| ApkpureError::Other(e.into()))?;

    let mut found_dl_links: Vec<String> = Vec::new();
    let mut last_page_url = canonical_url
        .clone()
        .unwrap_or_else(|| "https://apkpure.com/".to_string());

    for dl_page in &dl_pages {
        info!("apkpure: inspecting download page {dl_page}");
        let referer_header = canonical_url.as_deref().unwrap_or("https://apkpure.com/");
        let resp = match client
            .get(dl_page)
            .header("Referer", referer_header)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("apkpure: download page {dl_page} error: {e}");
                continue;
            }
        };

        if !resp.status().is_success() {
            warn!(
                "apkpure: download page {dl_page} returned status {}",
                resp.status()
            );
            continue;
        }

        last_page_url = dl_page.clone();
        let html = match resp.text().await {
            Ok(h) => h,
            Err(_) => continue,
        };

        for cap in re_dl_link.captures_iter(&html) {
            let link = cap[1].replace("&amp;", "&");
            if !found_dl_links.contains(&link) {
                found_dl_links.push(link);
            }
        }

        if !found_dl_links.is_empty() {
            break;
        }
    }

    if found_dl_links.is_empty() {
        info!("apkpure: no direct download links found on page, falling back to constructed endpoints");
        if let Some(ver) = target_version {
            let ver_encoded = ver.replace(' ', "+");
            found_dl_links.push(format!(
                "https://d.apkpure.com/b/XAPK/{package}?versionName={ver_encoded}"
            ));
            found_dl_links.push(format!(
                "https://d.apkpure.com/b/APK/{package}?versionName={ver_encoded}"
            ));
            found_dl_links.push(format!(
                "https://d.apkpure.net/b/XAPK/{package}?versionName={ver_encoded}"
            ));
            found_dl_links.push(format!(
                "https://d.apkpure.net/b/APK/{package}?versionName={ver_encoded}"
            ));
        }
        found_dl_links.push(format!(
            "https://d.apkpure.com/b/XAPK/{package}?version=latest"
        ));
        found_dl_links.push(format!(
            "https://d.apkpure.com/b/APK/{package}?version=latest"
        ));
        found_dl_links.push(format!(
            "https://d.apkpure.net/b/XAPK/{package}?version=latest"
        ));
        found_dl_links.push(format!(
            "https://d.apkpure.net/b/APK/{package}?version=latest"
        ));
    }

    found_dl_links.sort_by_key(|link| {
        if link.contains(arch) {
            0
        } else if link.contains("universal") || link.contains("all") {
            1
        } else {
            2
        }
    });

    let mut had_403 = false;
    let mut last_403_url = String::new();

    for raw_url in &found_dl_links {
        info!("apkpure: attempting download from {raw_url}");
        let resolved_url = resolve_apkpure_cdn_url(raw_url, Some(&last_page_url)).await;
        let referer_candidates = [
            Some(last_page_url.as_str()),
            Some("https://apkpure.com/"),
            Some("https://apkpure.net/"),
            None,
        ];

        for ref_opt in referer_candidates {
            match download_and_extract_if_bundle(client, &resolved_url, dest, ref_opt).await {
                Ok(_)
                    if Path::new(dest).exists()
                        && std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0) > 1_000_000 =>
                {
                    info!("apkpure: successfully downloaded {package} to {dest}");
                    return Ok(dest.to_string());
                }
                Ok(_) => {
                    warn!("apkpure: downloaded file too small or missing for {resolved_url}");
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("403") {
                        had_403 = true;
                        last_403_url = resolved_url.clone();
                        warn!(
                            "apkpure: 403 on {resolved_url} (Referer: {ref_opt:?}) — trying next referer/endpoint"
                        );
                    } else {
                        warn!("apkpure: failed to download {resolved_url}: {e}");
                        break;
                    }
                }
            }
        }
    }

    if had_403 {
        return Err(ApkpureError::Forbidden { url: last_403_url });
    }

    Err(ApkpureError::Other(anyhow::anyhow!(
        "APKPure download failed for {package}"
    )))
}

fn is_zip_file(path: &str) -> bool {
    if let Ok(mut file) = std::fs::File::open(path) {
        use std::io::Read;
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_ok() {
            return magic == [0x50, 0x4B, 0x03, 0x04];
        }
    }
    false
}

fn is_split_bundle(zip_path: &str) -> bool {
    let file = match std::fs::File::open(zip_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut apk_count = 0;
    for i in 0..archive.len() {
        if let Ok(item) = archive.by_index(i) {
            let name = item.name().to_lowercase();
            if name.ends_with(".apk") {
                apk_count += 1;
                if apk_count > 1 || name.starts_with("split_") || name.contains(".config.") {
                    return true;
                }
            }
        }
    }
    false
}

async fn merge_split_apk_bundle(bundle_path: &str, dest_apk: &str) -> Result<()> {
    let jar_candidates = ["tools/APKEditor.jar", "APKEditor.jar"];
    let jar_path = match jar_candidates.iter().map(Path::new).find(|p| p.exists()) {
        Some(p) => p.to_path_buf(),
        None => {
            info!("APKEditor.jar not found, attempting auto-download...");
            match crate::utils::ensure_apkeditor_jar("tools").await {
                Ok(p) => PathBuf::from(p),
                Err(e) => anyhow::bail!("APKEditor.jar not found and could not download: {e}"),
            }
        }
    };

    info!("Merging split APK bundle {bundle_path} into {dest_apk} using APKEditor...");
    let status = std::process::Command::new("java")
        .args([
            "-jar",
            jar_path.to_str().unwrap(),
            "m",
            "-i",
            bundle_path,
            "-o",
            dest_apk,
            "-f",
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!(
            "APKEditor failed to merge split bundle (exit: {:?})",
            status.code()
        );
    }
    Ok(())
}

fn extract_base_apk_from_zip(zip_path: &str, dest: &str) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut best_index: Option<usize> = None;
    let mut max_size: u64 = 0;

    for i in 0..archive.len() {
        let item = archive.by_index(i)?;
        let name = item.name().to_lowercase();
        if name.ends_with(".apk") {
            if name.ends_with("base.apk") {
                best_index = Some(i);
                break;
            }
            if item.size() > max_size {
                max_size = item.size();
                best_index = Some(i);
            }
        }
    }

    let idx = best_index.ok_or_else(|| anyhow::anyhow!("No .apk found inside ZIP bundle"))?;
    let mut entry = archive.by_index(idx)?;
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(&mut entry, &mut out)?;

    info!("Extracted {} bytes to {dest}", entry.size());
    Ok(())
}

pub fn strip_split_requirements_from_apk(apk_path: &str) -> Result<bool> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let src_file = std::fs::File::open(apk_path)?;
    let mut archive = zip::ZipArchive::new(src_file)?;

    let mut manifest_bytes: Option<Vec<u8>> = None;
    let mut other_entries = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;

        let compression = entry.compression();
        if name == "AndroidManifest.xml" {
            manifest_bytes = Some(data);
        } else {
            other_entries.push((name, data, compression));
        }
    }

    let manifest = match manifest_bytes {
        Some(m) => m,
        None => return Ok(false),
    };

    let sanitized_manifest = match sanitize_split_manifest(&manifest) {
        Some(m) => m,
        None => return Ok(false),
    };

    let temp_out = format!("{apk_path}.nosplit.tmp");
    let out_file = std::fs::File::create(&temp_out)?;
    let mut zip_out = zip::ZipWriter::new(out_file);
    let def_opts =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip_out.start_file("AndroidManifest.xml", def_opts)?;
    zip_out.write_all(&sanitized_manifest)?;

    for (name, data, compression) in other_entries {
        let method = if name == "resources.arsc" {
            zip::CompressionMethod::Stored
        } else {
            compression
        };
        let opts = SimpleFileOptions::default().compression_method(method);
        zip_out.start_file(&name, opts)?;
        zip_out.write_all(&data)?;
    }

    zip_out.finish()?;
    std::fs::remove_file(apk_path)?;
    std::fs::rename(&temp_out, apk_path)?;

    info!(
        "{apk_path}: stripped requiredSplitTypes and split requirements from AndroidManifest.xml"
    );
    Ok(true)
}

fn sanitize_split_manifest(manifest: &[u8]) -> Option<Vec<u8>> {
    let target_resid = 0x0101064eu32.to_le_bytes();
    let mut has_resid = false;
    for window in manifest.windows(4) {
        if window == target_resid {
            has_resid = true;
            break;
        }
    }
    if !has_resid {
        return None;
    }

    let new_manifest = manifest.to_vec();

    let mut resmap_pos = None;
    for (i, window) in new_manifest.windows(4).enumerate() {
        if window == [0x80, 0x01, 0x08, 0x00] {
            resmap_pos = Some(i);
            break;
        }
    }

    let pos = resmap_pos?;
    let size = u32::from_le_bytes(new_manifest[pos + 4..pos + 8].try_into().ok()?) as usize;
    let map_slice = &new_manifest[pos + 8..pos + size];
    let mut target_indices = Vec::new();
    for (idx, chunk) in map_slice.chunks_exact(4).enumerate() {
        let resid = u32::from_le_bytes(chunk.try_into().unwrap());
        if resid == 0x0101064e || resid == 0x0101064f {
            target_indices.push(idx);
        }
    }

    if target_indices.is_empty() {
        return None;
    }

    let mut elem_pos = None;
    for (i, window) in new_manifest.windows(4).enumerate() {
        if window == [0x02, 0x01, 0x10, 0x00] && i + 36 <= new_manifest.len() {
            let chunk_size =
                u32::from_le_bytes(new_manifest[i + 4..i + 8].try_into().unwrap()) as usize;
            let attr_count =
                u16::from_le_bytes(new_manifest[i + 28..i + 30].try_into().unwrap()) as usize;
            if attr_count > 0
                && attr_count < 200
                && chunk_size == 36 + attr_count * 20
                && i + chunk_size <= new_manifest.len()
            {
                let attrs_start = i + 36;
                let mut contains_target = false;
                for a in 0..attr_count {
                    let a_off = attrs_start + a * 20;
                    let name_idx =
                        i32::from_le_bytes(new_manifest[a_off + 4..a_off + 8].try_into().unwrap())
                            as usize;
                    if target_indices.contains(&name_idx) {
                        contains_target = true;
                        break;
                    }
                }
                if contains_target {
                    elem_pos = Some((i, chunk_size, attr_count));
                    break;
                }
            }
        }
    }

    let (pos, chunk_size, attr_count) = elem_pos?;
    let attrs_start = pos + 36;
    let mut new_attrs = Vec::new();
    let mut removed = 0;

    for a in 0..attr_count {
        let a_off = attrs_start + a * 20;
        let name_idx =
            i32::from_le_bytes(new_manifest[a_off + 4..a_off + 8].try_into().unwrap()) as usize;
        if target_indices.contains(&name_idx) {
            removed += 1;
        } else {
            new_attrs.extend_from_slice(&new_manifest[a_off..a_off + 20]);
        }
    }

    if removed == 0 {
        return None;
    }

    let mut result = Vec::new();
    result.extend_from_slice(&new_manifest[..pos]);

    let mut elem_hdr = new_manifest[pos..pos + 36].to_vec();
    let new_chunk_size = (36 + (attr_count - removed) * 20) as u32;
    elem_hdr[4..8].copy_from_slice(&new_chunk_size.to_le_bytes());
    elem_hdr[28..30].copy_from_slice(&((attr_count - removed) as u16).to_le_bytes());
    result.extend_from_slice(&elem_hdr);
    result.extend_from_slice(&new_attrs);
    result.extend_from_slice(&new_manifest[pos + chunk_size..]);

    let total_len = result.len() as u32;
    result[4..8].copy_from_slice(&total_len.to_le_bytes());

    let old_s_u16: Vec<u8> = "com.android.vending.splits"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let new_s_u16: Vec<u8> = "com.android.vending.spl_ds"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();

    for pos in 0..result.len().saturating_sub(old_s_u16.len()) {
        if result[pos..pos + old_s_u16.len()] == old_s_u16[..] {
            result[pos..pos + new_s_u16.len()].copy_from_slice(&new_s_u16);
        }
    }

    let old_s_u8 = b"com.android.vending.splits";
    let new_s_u8 = b"com.android.vending.spl_ds";
    for pos in 0..result.len().saturating_sub(old_s_u8.len()) {
        if result[pos..pos + old_s_u8.len()] == old_s_u8[..] {
            result[pos..pos + new_s_u8.len()].copy_from_slice(new_s_u8);
        }
    }

    Some(result)
}
