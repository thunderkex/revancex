// GitHub release asset APK source — derived from RookieEnough/Morphe-AutoBuilds (GPL-3.0).
// https://github.com/RookieEnough/Morphe-AutoBuilds

use super::{default_fetch, ApkSource, FileKind, Resolved, SourceCtx};
use crate::config::apps::AppConfig;
use crate::utils::forge::{self, Forge, ReleaseSelector};
use anyhow::Result;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub struct GithubSource;

impl ApkSource for GithubSource {
    fn name(&self) -> &'static str {
        "github"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            let mut trace = vec![format!("Resolving from github for {}", app.package)];

            let gh_entry = app.sources.get("github");
            let repo = match gh_entry.and_then(|e| e.repo.as_deref()) {
                Some(r) => r,
                None => return Ok(None),
            };

            let asset_pattern = gh_entry
                .and_then(|e| e.asset_pattern.as_deref())
                .unwrap_or("{arch}*.apk");
            let target_pattern = asset_pattern.replace("{arch}", arch);

            let selector = match version {
                Some(v) => {
                    if let Some(tag) = gh_entry.and_then(|e| e.tag.as_deref()) {
                        ReleaseSelector::Exact(tag.to_string())
                    } else {
                        ReleaseSelector::Exact(v.to_string())
                    }
                }
                None => ReleaseSelector::Latest,
            };

            let rel = match forge::release(ctx.client, &Forge::Github, repo, &selector).await {
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
                    (n.ends_with(".apk")
                        || n.ends_with(".apkm")
                        || n.ends_with(".xapk")
                        || n.ends_with(".apks"))
                        && (n.contains(arch)
                            || target_pattern.is_empty()
                            || n.contains(&target_pattern))
                })
                .collect();

            if candidates.is_empty() {
                return Ok(None);
            }

            candidates.sort_by_key(|a| if a.name.contains(arch) { 0 } else { 1 });
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
