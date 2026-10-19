use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use anyhow::Result;
use std::path::PathBuf;
use std::pin::Pin;

pub struct ApkmirrorSource;

impl ApkSource for ApkmirrorSource {
    fn name(&self) -> &'static str {
        "apkmirror"
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
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
            crate::builder::fetch_apkmirror(
                ctx.client,
                &resolved.url,
                &resolved.arch,
                dest,
                resolved.version_found.as_deref(),
                ctx.fetcher,
            )
            .await?;
            Ok(PathBuf::from(dest))
        })
    }
}
