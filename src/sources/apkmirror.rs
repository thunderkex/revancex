use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use crate::sources::common::{download_and_extract_if_bundle, fetch_page_html};
use crate::utils::version::is_beta_or_alpha;
use anyhow::Result;
use regex::Regex;
use reqwest::header::{HeaderValue, USER_AGENT};
use reqwest::Client;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::OnceLock;
use tracing::info;

pub struct ApkmirrorSource;

impl ApkSource for ApkmirrorSource {
    fn name(&self) -> &'static str {
        "apkmirror"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if let Err((_st, reason)) = ctx.can_try("apkmirror") {
                tracing::warn!("{reason} — skipping apkmirror");
                return Ok(None);
            }
            if app.apkmirror_url.is_none() {
                tracing::info!("apkmirror skipped: not configured");
                return Ok(None);
            }
            let url = app.apkmirror_url.as_deref().unwrap();
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
            match fetch_apkmirror(
                ctx.client,
                ctx.no_redirect_client,
                &resolved.url,
                &resolved.arch,
                dest,
                resolved.version_found.as_deref(),
                ctx.fetcher,
            )
            .await
            {
                Ok(_) => {
                    ctx.record_success("apkmirror");
                    Ok(PathBuf::from(dest))
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    let block_kind = crate::sources::breaker::classify_error(&err_msg);
                    ctx.record_failure("apkmirror", &block_kind);
                    Err(e)
                }
            }
        })
    }
}

pub(crate) async fn fetch_apkmirror(
    client: &Client,
    no_redirect_client: Option<&Client>,
    app_url: &str,
    arch: &str,
    dest: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String> {
    info!("apkmirror: loading app page {app_url}");
    let base_origin = "https://www.apkmirror.com";
    let (app_html, _hdrs) = fetch_page_html(client, app_url, fetcher).await?;

    static RE_REL: OnceLock<Regex> = OnceLock::new();
    let re_rel = RE_REL.get_or_init(|| {
        Regex::new(r#"href="(/apk/[^"]+-release/)""#).expect("valid apkmirror release regex")
    });
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
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "apkmirror: version {ver} not found in recent releases on {app_url}"
                )
            })?
    } else {
        releases.first().unwrap()
    };

    let release_url = format!("{base_origin}{release_path}");
    info!("apkmirror: loading release {release_url}");
    let (rel_html, _hdrs) = fetch_page_html(client, &release_url, fetcher).await?;

    static RE_VAR: OnceLock<Regex> = OnceLock::new();
    let re_var = RE_VAR.get_or_init(|| {
        Regex::new(r#"href="(/apk/[^"]+-android-apk-download/)""#)
            .expect("valid apkmirror variant regex")
    });
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
    let (var_html, _hdrs) = fetch_page_html(client, &variant_url, fetcher).await?;

    static RE_DL: OnceLock<Regex> = OnceLock::new();
    let re_dl = RE_DL.get_or_init(|| {
        Regex::new(r#"href="(/apk/[^"]+/download/\?key=[^"]+)""#)
            .expect("valid apkmirror download regex")
    });
    let dl_path = re_dl
        .captures(&var_html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow::anyhow!("No intermediate download button found on {variant_url}"))?;

    let dl_url = format!("{base_origin}{dl_path}");
    info!("apkmirror: loading download landing page {dl_url}");
    let (dl_html, _hdrs) = fetch_page_html(client, &dl_url, fetcher).await?;

    static RE_PHP: OnceLock<Regex> = OnceLock::new();
    let re_php = RE_PHP.get_or_init(|| {
        Regex::new(r#"href="(/wp-content/themes/APKMirror/download\.php\?[^"]+)""#)
            .expect("valid apkmirror php regex")
    });
    let php_path = re_php
        .captures(&dl_html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow::anyhow!("No download.php link found on {dl_url}"))?;

    let php_url = format!("{base_origin}{php_path}");
    info!("apkmirror: resolving direct CDN location from {php_url}");

    let ua = crate::utils::random_user_agent();
    let val = HeaderValue::from_str(ua)
        .unwrap_or_else(|_| HeaderValue::from_static(crate::utils::DEFAULT_UA));
    static FALLBACK_NO_REDIR: OnceLock<Client> = OnceLock::new();
    let no_redir_client = match no_redirect_client {
        Some(c) => c,
        None => FALLBACK_NO_REDIR.get_or_init(|| {
            crate::utils::make_no_redirect_client(15).expect("valid fallback no-redirect client")
        }),
    };

    let res = no_redir_client
        .get(&php_url)
        .header(USER_AGENT, val)
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
