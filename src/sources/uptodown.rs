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
                    ctx.record_success("uptodown");
                    Ok(PathBuf::from(path))
                }
                Err(crate::builder::UptodownError::TurnstileBlocked) => {
                    ctx.record_failure(
                        "uptodown",
                        &crate::sources::breaker::BlockKind::TurnstileChallenge,
                    );
                    tracing::warn!("uptodown TurnstileBlocked");
                    Err(anyhow::anyhow!("uptodown TurnstileBlocked"))
                }
                Err(crate::builder::UptodownError::IpReputationBlocked) => {
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
