use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use anyhow::Result;
use std::path::PathBuf;
use std::pin::Pin;

pub struct ApkeepSource;

impl ApkSource for ApkeepSource {
    fn name(&self) -> &'static str {
        "apkeep"
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
                tracing::info!("apkeep skipped: not configured");
                return Ok(None);
            }
            let apkeep_ack = std::env::var("REVANCEX_APKEEP_ACKNOWLEDGE_DANGERS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or_else(|_| {
                    ctx.cfg
                        .apk_sources
                        .get("apkeep")
                        .map(|e| e.acknowledge_dangers)
                        .unwrap_or(false)
                });
            if !apkeep_ack {
                tracing::info!("apkeep skipped: acknowledge_dangers is false");
                return Ok(None);
            }
            let pseudo_url = format!("apkeep://{}/{}", app.package, version.unwrap_or("latest"));
            Ok(Some(Resolved::simple(
                pseudo_url,
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
            let version = resolved.version_found.as_deref();
            let pkg = resolved
                .url
                .strip_prefix("apkeep://")
                .and_then(|s| s.split('/').next())
                .unwrap_or("");
            let apkeep_ack = std::env::var("REVANCEX_APKEEP_ACKNOWLEDGE_DANGERS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or_else(|_| {
                    ctx.cfg
                        .apk_sources
                        .get("apkeep")
                        .map(|e| e.acknowledge_dangers)
                        .unwrap_or(false)
                });
            crate::builder::fetch_apkeep(pkg, dest, version, Some(&resolved.arch), apkeep_ack)
                .await?;
            Ok(PathBuf::from(dest))
        })
    }
}
