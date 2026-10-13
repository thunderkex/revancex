// Codeberg release asset APK source — derived from RookieEnough/Morphe-AutoBuilds (GPL-3.0).
// https://github.com/RookieEnough/Morphe-AutoBuilds

use super::{default_fetch, ApkSource, FileKind, Resolved, SourceCtx};
use crate::config::apps::AppConfig;
use crate::utils::forge::{self, Forge, ReleaseSelector};
use anyhow::Result;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub struct CodebergSource;

impl ApkSource for CodebergSource {
    fn name(&self) -> &'static str {
        "codeberg"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            let mut trace = vec![format!("Resolving from codeberg for {}", app.package)];

            let cb_entry = app.sources.get("codeberg");
            let repo = match cb_entry.and_then(|e| e.repo.as_deref()) {
                Some(r) => r,
                None => return Ok(None),
            };

            let domain = cb_entry
                .and_then(|e| e.domain.as_deref())
                .unwrap_or("codeberg.org")
                .to_string();

            let keyword = cb_entry.and_then(|e| e.keyword.as_deref());

            let selector = match version {
                Some(v) => {
                    if let Some(tag) = cb_entry.and_then(|e| e.tag.as_deref()) {
                        ReleaseSelector::Exact(tag.to_string())
                    } else {
                        ReleaseSelector::Exact(v.to_string())
                    }
                }
                None => ReleaseSelector::Latest,
            };

            let forge = Forge::Codeberg { domain };
            let rel = match forge::release(ctx.client, &forge, repo, &selector).await {
                Ok(r) => r,
                Err(e) => {
                    trace.push(format!("Lookup failed: {e}"));
                    return Ok(None);
                }
            };

            trace.push(format!("Found release tag {}", rel.tag));

            let mut candidates: Vec<_> = rel
                .assets
                .into_iter()
                .filter(|a| {
                    let n = &a.name;
                    let valid_ext = n.ends_with(".apk")
                        || n.ends_with(".apkm")
                        || n.ends_with(".xapk")
                        || n.ends_with(".apks");
                    let kw_match = keyword.map_or(true, |k| n.contains(k));
                    valid_ext && kw_match
                })
                .collect();

            if candidates.is_empty() {
                return Ok(None);
            }

            candidates.sort_by_key(|a| if a.name.contains(arch) { 0 } else { 10 });
            let chosen = candidates.remove(0);

            trace.push(format!("Selected asset: {}", chosen.name));

            let kind = if chosen.name.ends_with(".apkm") {
                FileKind::Apkm
            } else if chosen.name.ends_with(".xapk") {
                FileKind::Xapk
            } else if chosen.name.ends_with(".apks") {
                FileKind::Apks
            } else {
                FileKind::Apk
            };

            let mut resolved = Resolved::simple(chosen.url, Some(rel.tag), arch);
            resolved.file_kind = kind;
            resolved.trace = trace;

            Ok(Some(resolved))
        })
    }

    fn fetch<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        resolved: &'a Resolved,
        dest: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<PathBuf>> + Send + 'a>> {
        let dest = dest.to_string();
        Box::pin(async move { default_fetch(ctx, resolved, &dest).await })
    }
}
