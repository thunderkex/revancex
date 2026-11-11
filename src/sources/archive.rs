use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use crate::sources::common::download_and_extract_if_bundle;
use crate::utils::version::is_beta_or_alpha;
use anyhow::Result;
use regex::Regex;
use reqwest::Client;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::OnceLock;
use tracing::info;

pub struct ArchiveSource;

impl ApkSource for ArchiveSource {
    fn name(&self) -> &'static str {
        "archive"
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if app.archive_url.is_none() {
                tracing::info!("archive skipped: not configured");
                return Ok(None);
            }
            let url = app.archive_url.as_deref().unwrap();
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
            fetch_archive_org(
                ctx.client,
                &resolved.url,
                &resolved.arch,
                dest,
                resolved.version_found.as_deref(),
                &[],
            )
            .await?;
            Ok(PathBuf::from(dest))
        })
    }
}

pub(crate) async fn fetch_archive_org(
    client: &Client,
    url: &str,
    arch: &str,
    dest: &str,
    target_version: Option<&str>,
    supported_versions: &[&str],
) -> Result<String> {
    let url_lower = url.to_lowercase();
    if url_lower.ends_with(".apk") || url_lower.ends_with(".apkm") || url_lower.ends_with(".xapk") {
        info!("archive.org: direct file URL, downloading {url}");
        download_and_extract_if_bundle(client, url, dest, None).await?;
        return Ok(dest.to_string());
    }
    let base = if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    };

    info!("archive.org: listing files from {base}");
    let html = client.get(&base).send().await?.text().await?;

    static RE_ARCHIVE: OnceLock<Regex> = OnceLock::new();
    let re = RE_ARCHIVE.get_or_init(|| {
        Regex::new(r#"href="([^"]+\.(?:apk|apkm|xapk))""#).expect("valid archive regex")
    });
    let mut candidates: Vec<String> = Vec::new();

    for cap in re.captures_iter(&html) {
        let file = &cap[1];
        if file.contains('/') || file.starts_with('?') || file.contains('~') {
            continue;
        }
        candidates.push(file.to_string());
    }

    if candidates.is_empty() {
        anyhow::bail!("No APK/APKM found in archive.org directory {base}");
    }

    let allow_beta = target_version.is_some_and(is_beta_or_alpha);
    let clean_candidates: Vec<String> = candidates
        .into_iter()
        .filter(|c| allow_beta || !is_beta_or_alpha(c))
        .collect();

    if clean_candidates.is_empty() {
        anyhow::bail!("No non-beta APK/APKM found in archive.org directory {base}");
    }

    let chosen = if let Some(ver) = target_version {
        let base_ver = ver.split('-').next().unwrap_or(ver);
        let exact = clean_candidates
            .iter()
            .filter(|f| f.contains(ver) || f.contains(base_ver))
            .rfind(|f| {
                f.contains(arch)
                    || f.contains("-all.")
                    || f.contains("_all.")
                    || f.contains("universal")
            })
            .or_else(|| {
                clean_candidates
                    .iter()
                    .rfind(|f| f.contains(ver) || f.contains(base_ver))
            });

        if let Some(m) = exact {
            m
        } else {
            let supported_match = supported_versions
                .iter()
                .rev()
                .filter(|&&sv| sv != ver)
                .find_map(|&sv| {
                    let s_base = sv.split('-').next().unwrap_or(sv);
                    clean_candidates
                        .iter()
                        .filter(|f| f.contains(sv) || f.contains(s_base))
                        .rfind(|f| {
                            f.contains(arch)
                                || f.contains("-all.")
                                || f.contains("_all.")
                                || f.contains("universal")
                        })
                        .or_else(|| {
                            clean_candidates
                                .iter()
                                .rfind(|f| f.contains(sv) || f.contains(s_base))
                        })
                });

            if let Some(sm) = supported_match {
                info!("archive.org: exact version {ver} not found, matched supported version in archive: {sm}");
                sm
            } else {
                anyhow::bail!(
                    "archive.org: exact version {ver} and supported versions not found in archive candidates"
                );
            }
        }
    } else {
        clean_candidates
            .iter()
            .rfind(|f| f.contains(arch))
            .or_else(|| {
                clean_candidates
                    .iter()
                    .rfind(|f| f.contains("-all.") || f.contains("_all."))
            })
            .or_else(|| clean_candidates.iter().rfind(|f| f.contains("universal")))
            .or_else(|| clean_candidates.last())
            .ok_or_else(|| anyhow::anyhow!("No matching file candidate"))?
    };

    let download_url = format!("{base}{chosen}");
    info!("archive.org: downloading {download_url}");
    download_and_extract_if_bundle(client, &download_url, dest, None).await?;
    Ok(dest.to_string())
}
