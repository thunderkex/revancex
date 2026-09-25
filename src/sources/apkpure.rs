use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use anyhow::Result;
use std::path::PathBuf;
use std::pin::Pin;

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
                match crate::builder::resolve_apkpure_api_url(
                    ctx.client,
                    &app.package,
                    arch,
                    version,
                    &api_headers,
                )
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
                crate::builder::download_and_extract_if_bundle(
                    ctx.client,
                    &resolved.url,
                    dest,
                    Some("https://apkpure.com/"),
                )
                .await?;
            } else if resolved.url.starts_with("http") {
                crate::builder::fetch_apkpure_url_pub(
                    ctx.client,
                    &resolved.url,
                    dest,
                    &resolved.arch,
                    resolved.version_found.as_deref(),
                    ctx.fetcher,
                )
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            } else {
                crate::builder::fetch_apkpure_pub(
                    ctx.client,
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
