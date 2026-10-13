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
        _ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if app.package.is_empty() {
                return Ok(None);
            }
            let pseudo_url = format!("apkeep://{}/{}", app.package, version.unwrap_or("latest"));
            Ok(Some(Resolved::simple(pseudo_url, version.map(str::to_string), arch)))
        })
    }

    fn fetch<'a>(
        &'a self,
        _ctx: &'a SourceCtx<'a>,
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
            crate::builder::fetch_apkeep(pkg, dest, version).await?;
            Ok(PathBuf::from(dest))
        })
    }
}
