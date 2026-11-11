use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use crate::sources::common::{download_and_extract_if_bundle, fetch_page_html};
use anyhow::{Context, Result};
use regex::Regex;
use reqwest::Client;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::OnceLock;
use tracing::{info, warn};

#[derive(Debug)]
pub enum ApkpureError {
    Forbidden { url: String },
    Other(anyhow::Error),
}

impl std::fmt::Display for ApkpureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApkpureError::Forbidden { url } => write!(f, "apkpure: 403 on {url}"),
            ApkpureError::Other(e) => write!(f, "apkpure: {e}"),
        }
    }
}

impl std::error::Error for ApkpureError {}

pub struct ApkpureSource;

impl ApkSource for ApkpureSource {
    fn name(&self) -> &'static str {
        "apkpure"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if app.package.is_empty() {
                tracing::info!("apkpure skipped: not configured");
                return Ok(None);
            }

            let api_entry = ctx.cfg.apk_sources.get("apkpure_api");
            let api_enabled = api_entry.map(|e| e.enabled).unwrap_or(true);
            if api_enabled {
                let api_headers = api_entry.map(|e| e.headers.clone()).unwrap_or_default();
                match resolve_apkpure_api_url(ctx.client, &app.package, arch, version, &api_headers)
                    .await
                {
                    Ok(url) => {
                        let mut r = Resolved::simple(url, version.map(str::to_string), arch);
                        r.trace.push("apkpure_api".into());
                        return Ok(Some(r));
                    }
                    Err(e) => {
                        tracing::warn!("apkpure_api resolve failed for {}: {e}", app.package);
                    }
                }
            } else {
                tracing::info!("apkpure_api disabled by config kill switch");
            }

            let page_url = app.apkpure_url.as_deref().unwrap_or(&app.package);
            let mut r = Resolved::simple(page_url.to_string(), version.map(str::to_string), arch);
            r.trace.push("apkpure_page".into());
            Ok(Some(r))
        })
    }

    fn fetch<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        resolved: &'a Resolved,
        dest: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            if resolved.trace.contains(&"apkpure_api".to_string()) {
                download_and_extract_if_bundle(
                    ctx.client,
                    &resolved.url,
                    dest,
                    Some("https://apkpure.com/"),
                )
                .await?;
            } else if resolved.url.starts_with("http") {
                fetch_apkpure_url(
                    ctx.client,
                    ctx.no_redirect_client,
                    &resolved.url,
                    dest,
                    &resolved.arch,
                    resolved.version_found.as_deref(),
                    ctx.fetcher,
                )
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            } else {
                fetch_apkpure(
                    ctx.client,
                    ctx.no_redirect_client,
                    &resolved.url,
                    dest,
                    &resolved.arch,
                    resolved.version_found.as_deref(),
                    ctx.fetcher,
                )
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            Ok(PathBuf::from(dest))
        })
    }
}

pub(crate) async fn resolve_apkpure_api_url(
    client: &Client,
    package: &str,
    arch: &str,
    target_version: Option<&str>,
    extra_headers: &std::collections::HashMap<String, String>,
) -> Result<String> {
    let url =
        format!("https://tapi.pureapk.com/v3/get_app_his_version?package_name={package}&hl=en");

    let mut req = client.get(&url);
    for (k, v) in extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("apkpure_api: request failed for {package}: {e}");
            return Err(e).context("apkpure_api: request failed");
        }
    };

    if !resp.status().is_success() {
        warn!("apkpure_api: HTTP {} for {package}", resp.status());
        anyhow::bail!("apkpure_api: HTTP {} for {package}", resp.status());
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            warn!("apkpure_api: JSON parse failed for {package}: {e}");
            return Err(e).context("apkpure_api: JSON parse failed");
        }
    };

    let version_list = body
        .get("version_list")
        .or_else(|| body.get("data").and_then(|d| d.get("version_list")))
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            warn!("apkpure_api: no version_list for {package}");
            anyhow::anyhow!("apkpure_api: no version_list for {package}")
        })?;

    if version_list.is_empty() {
        warn!("apkpure_api: empty version_list for {package}");
        anyhow::bail!("apkpure_api: empty version_list for {package}");
    }

    let is_hw = |s: &str| s.to_ascii_lowercase().contains("-hw");
    let target_wants_hw = target_version.is_some_and(is_hw);

    let entry = if let Some(ver) = target_version {
        version_list
            .iter()
            .find(|e| {
                let vn = e["version_name"].as_str().unwrap_or("");
                if !target_wants_hw && is_hw(vn) {
                    return false;
                }
                if !crate::utils::version::version_eq(vn, ver) {
                    return false;
                }
                e.get("native_code")
                    .and_then(|v| v.as_array())
                    .map(|nc| nc.iter().any(|c| c.as_str() == Some(arch)))
                    .unwrap_or(false)
            })
            .or_else(|| {
                version_list.iter().find(|e| {
                    let vn = e["version_name"].as_str().unwrap_or("");
                    if !target_wants_hw && is_hw(vn) {
                        return false;
                    }
                    crate::utils::version::version_eq(vn, ver)
                })
            })
    } else {
        version_list
            .iter()
            .find(|e| {
                let vn = e["version_name"].as_str().unwrap_or("");
                if !target_wants_hw && is_hw(vn) {
                    return false;
                }
                e.get("native_code")
                    .and_then(|v| v.as_array())
                    .map(|nc| nc.iter().any(|c| c.as_str() == Some(arch)))
                    .unwrap_or(false)
            })
            .or_else(|| {
                version_list.iter().find(|e| {
                    let vn = e["version_name"].as_str().unwrap_or("");
                    target_wants_hw || !is_hw(vn)
                })
            })
    };
    let entry = entry.ok_or_else(|| {
        anyhow::anyhow!(
            "apkpure_api: version {:?} not in the ~20 newest entries for {package} — miss",
            target_version
        )
    })?;
    let asset_url = entry["asset"]["urls"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|u| u.as_str().map(|s| s.contains(arch)).unwrap_or(false))
                .or_else(|| arr.first())
        })
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("apkpure_api: no asset URL in entry for {package}"))?;
    Ok(asset_url.to_string())
}

pub(crate) async fn fetch_apkpure_url(
    client: &Client,
    no_redirect_client: Option<&Client>,
    base_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
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
    fetch_apkpure(
        client,
        no_redirect_client,
        base_url,
        dest,
        arch,
        target_version,
        fetcher,
    )
    .await
}

pub(crate) async fn resolve_apkpure_cdn_url(
    no_redirect_client: Option<&Client>,
    url: &str,
    referer: Option<&str>,
) -> String {
    if !url.contains("d.apkpure.") {
        return url.to_string();
    }
    static FALLBACK_NO_REDIR: OnceLock<Client> = OnceLock::new();
    let client = match no_redirect_client {
        Some(c) => c,
        None => FALLBACK_NO_REDIR.get_or_init(|| {
            crate::utils::make_no_redirect_client(10).expect("valid fallback no-redirect client")
        }),
    };

    let referers_to_try = [
        referer,
        Some("https://apkpure.com/"),
        Some("https://apkpure.net/"),
        None,
    ];

    for r_opt in referers_to_try {
        let mut req = client
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
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
                        let is_root = loc_str.ends_with("apkpure.com/")
                            || loc_str.ends_with("apkpure.net/")
                            || loc_str == "https://apkpure.com"
                            || loc_str == "https://apkpure.net";
                        if !is_root {
                            info!("apkpure: resolved CDN redirect {url} -> {loc_str}");
                            return loc_str.to_string();
                        }
                    }
                }
            }
        }
    }
    url.to_string()
}

async fn resolve_apkpure_canonical_url(
    client: &Client,
    app_url: &str,
    fetcher: Option<&crate::net::Fetcher>,
) -> Option<String> {
    if let Some(f) = fetcher {
        return match fetch_page_html(client, app_url, Some(f)).await {
            Ok(_) => {
                let u = app_url.trim_end_matches('/').to_string();
                info!("apkpure: canonical app URL is {u} (fetcher)");
                Some(u)
            }
            Err(e) => {
                warn!(
                    "apkpure: app page {app_url} fetcher error: {e}, proceeding direct endpoints"
                );
                None
            }
        };
    }
    match client
        .get(app_url)
        .header("Referer", "https://apkpure.com/")
        .send()
        .await
    {
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
    }
}

async fn fetch_apkpure_dl_page_html(
    client: &Client,
    dl_page: &str,
    canonical_url: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Option<String> {
    if let Some(f) = fetcher {
        return match fetch_page_html(client, dl_page, Some(f)).await {
            Ok((body, _hdrs)) => Some(body),
            Err(e) => {
                warn!("apkpure: download page {dl_page} fetcher error: {e}");
                None
            }
        };
    }
    let referer_header = canonical_url.unwrap_or("https://apkpure.com/");
    let resp = match client
        .get(dl_page)
        .header("Referer", referer_header)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("apkpure: download page {dl_page} error: {e}");
            return None;
        }
    };
    if !resp.status().is_success() {
        warn!(
            "apkpure: download page {dl_page} returned status {}",
            resp.status()
        );
        return None;
    }
    resp.text().await.ok()
}

fn extract_apkpure_links_from_html(
    html: &str,
    package: &str,
    arch: &str,
    target_version: Option<&str>,
    found_dl_links: &mut Vec<String>,
) {
    static RE_WINUDF: OnceLock<Regex> = OnceLock::new();
    static RE_DL_LINK: OnceLock<Regex> = OnceLock::new();
    static RE_VER_ITEM: OnceLock<Regex> = OnceLock::new();
    let re_winudf_link = RE_WINUDF.get_or_init(|| {
        Regex::new(
            r#"href=[\x22\x27](https://(?:download|data)\.winudf\.com/[^\x22\x27]+)[\x22\x27]"#,
        )
        .expect("valid winudf regex")
    });
    let re_dl_link = RE_DL_LINK.get_or_init(|| {
        Regex::new(
            r#"href=[\x22\x27](https://d\.apkpure\.(?:com|net)/b/(?:XAPK|APK)/[^\x22\x27]+)[\x22\x27]"#,
        )
        .expect("valid apkpure link regex")
    });
    let re_version_item = RE_VER_ITEM.get_or_init(|| {
        Regex::new(
            r#"data-dt-version=[\x22\x27]([^\x22\x27]+)[\x22\x27][^>]*data-dt-versioncode=[\x22\x27]([^\x22\x27]+)[\x22\x27]"#,
        )
        .expect("valid apkpure version item regex")
    });

    let push_unique = |list: &mut Vec<String>, link: String| {
        if !list.contains(&link) {
            list.push(link);
        }
    };
    for cap in re_winudf_link.captures_iter(html) {
        push_unique(found_dl_links, cap[1].replace("&amp;", "&"));
    }
    for cap in re_dl_link.captures_iter(html) {
        push_unique(found_dl_links, cap[1].replace("&amp;", "&"));
    }
    for cap in re_version_item.captures_iter(html) {
        let item_ver = &cap[1];
        let item_code = &cap[2];
        let matches = target_version
            .map(|tv| crate::utils::version::version_eq(item_ver, tv))
            .unwrap_or(true);
        if matches {
            let code_link =
                format!("https://d.apkpure.com/b/XAPK/{package}?versionCode={item_code}&nc={arch}");
            push_unique(found_dl_links, code_link);
        }
    }
}

async fn try_apkpure_download_candidates(
    client: &Client,
    no_redirect_client: Option<&Client>,
    package: &str,
    dest: &str,
    last_page_url: &str,
    found_dl_links: &[String],
) -> Result<String, ApkpureError> {
    let mut had_403 = false;
    let mut last_403_url = String::new();
    let referer_candidates = [
        Some(last_page_url),
        Some("https://apkpure.com/"),
        Some("https://apkpure.net/"),
        None,
    ];

    for raw_url in found_dl_links {
        info!("apkpure: attempting download from {raw_url}");
        let resolved_url =
            resolve_apkpure_cdn_url(no_redirect_client, raw_url, Some(last_page_url)).await;
        for ref_opt in referer_candidates {
            match download_and_extract_if_bundle(client, &resolved_url, dest, ref_opt).await {
                Ok(_)
                    if tokio::fs::metadata(dest)
                        .await
                        .map(|m| m.len())
                        .unwrap_or(0)
                        > 1_000_000 =>
                {
                    info!("apkpure: successfully downloaded {package} to {dest}");
                    return Ok(dest.to_string());
                }
                Ok(_) => {
                    warn!("apkpure: downloaded file too small or missing for {resolved_url}");
                }
                Err(e) => {
                    if e.to_string().contains("403") {
                        had_403 = true;
                        last_403_url = resolved_url.clone();
                        warn!(
                            "apkpure: 403 on {resolved_url} (Referer: {ref_opt:?}) — aborting endpoint (no retry)"
                        );
                    } else {
                        warn!("apkpure: failed to download {resolved_url}: {e}");
                    }
                    break;
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

pub(crate) async fn fetch_apkpure(
    client: &Client,
    no_redirect_client: Option<&Client>,
    package_or_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
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

    let canonical_url = resolve_apkpure_canonical_url(client, &app_url, fetcher).await;
    let mut dl_pages = Vec::new();
    if let Some(ref can_url) = canonical_url {
        dl_pages.push(format!("{can_url}/versions"));
        if let Some(ver) = target_version {
            dl_pages.push(format!("{can_url}/download/{}", ver.replace(' ', "-")));
        } else {
            dl_pages.push(format!("{can_url}/download"));
        }
    }

    let mut found_dl_links: Vec<String> = Vec::new();
    let mut last_page_url = canonical_url
        .clone()
        .unwrap_or_else(|| "https://apkpure.com/".to_string());

    for dl_page in &dl_pages {
        info!("apkpure: inspecting download page {dl_page}");
        let Some(html) =
            fetch_apkpure_dl_page_html(client, dl_page, canonical_url.as_deref(), fetcher).await
        else {
            continue;
        };
        last_page_url = dl_page.clone();
        extract_apkpure_links_from_html(&html, &package, arch, target_version, &mut found_dl_links);
        if !found_dl_links.is_empty() {
            break;
        }
    }

    if found_dl_links.is_empty() {
        info!("apkpure: no direct download links found on page, falling back to constructed endpoints");
        let fallback = if let Some(ver) = target_version {
            format!(
                "https://d.apkpure.com/b/XAPK/{package}?versionName={}",
                ver.replace(' ', "+")
            )
        } else {
            format!("https://d.apkpure.com/b/XAPK/{package}?version=latest")
        };
        found_dl_links.push(fallback);
    }

    found_dl_links.sort_by_key(|link| {
        let is_winudf = link.contains("winudf.com");
        let has_arch = link.contains(arch);
        let has_universal = link.contains("universal") || link.contains("all");
        match (is_winudf, has_arch, has_universal) {
            (true, true, _) => 0,
            (true, false, true) => 1,
            (true, false, false) => 2,
            (false, true, _) => 3,
            (false, false, true) => 4,
            (false, false, false) => 5,
        }
    });

    try_apkpure_download_candidates(
        client,
        no_redirect_client,
        &package,
        dest,
        &last_page_url,
        &found_dl_links,
    )
    .await
}
