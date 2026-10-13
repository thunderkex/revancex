use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use anyhow::Result;
use std::path::PathBuf;
use std::pin::Pin;

pub struct AptoideSource;

impl ApkSource for AptoideSource {
    fn name(&self) -> &'static str {
        "aptoide"
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if app.package.is_empty() {
                return Ok(None);
            }
            let pseudo = format!("aptoide://{}", app.package);
            Ok(Some(Resolved::simple(
                pseudo,
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
            let pkg = resolved
                .url
                .strip_prefix("aptoide://")
                .unwrap_or(&resolved.url);
            crate::builder::fetch_aptoide(
                ctx.client,
                pkg,
                dest,
                &resolved.arch,
                resolved.version_found.as_deref(),
            )
            .await?;
            Ok(PathBuf::from(dest))
        })
    }
}
