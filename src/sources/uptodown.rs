use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use anyhow::Result;
use std::path::PathBuf;
use std::pin::Pin;

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
            let cb = ctx.breaker("uptodown");
            let cb_count = cb.load(std::sync::atomic::Ordering::SeqCst);
            if cb_count >= ctx.cb_threshold {
                tracing::warn!(
                    "uptodown circuit-breaker open ({cb_count}/{} consecutive TurnstileBlocked) — skipping uptodown",
                    ctx.cb_threshold
                );
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
            let res = crate::builder::fetch_uptodown(
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
                    ctx.breaker("uptodown")
                        .store(0, std::sync::atomic::Ordering::SeqCst);
                    Ok(PathBuf::from(path))
                }
                Err(crate::builder::UptodownError::TurnstileBlocked) => {
                    let cb = ctx.breaker("uptodown");
                    let new_count = cb.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                    tracing::warn!(
                        "uptodown TurnstileBlocked (circuit-breaker {new_count}/{})",
                        ctx.cb_threshold
                    );
                    Err(anyhow::anyhow!("uptodown TurnstileBlocked"))
                }
                Err(e) => Err(anyhow::anyhow!("{e}")),
            }
        })
    }
}
