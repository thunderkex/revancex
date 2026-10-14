pub mod apkcombo;
pub mod apkeep;
pub mod apkfab;
pub mod apkmirror;
pub mod apkpure;
pub mod aptoide;
pub mod archive;
pub mod codeberg;
pub mod github;
pub mod gitlab;
pub mod uptodown;

use crate::config::{AppConfig, Config};
use anyhow::Result;
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

#[derive(Clone)]
pub struct SourceCtx<'a> {
    pub client: &'a Client,
    pub cfg: &'a Config,
    pub breakers: Arc<HashMap<String, Arc<AtomicU32>>>,
    pub cb_threshold: u32,
    pub fetcher: Option<&'a crate::net::Fetcher>,
}

impl<'a> SourceCtx<'a> {
    pub fn new(client: &'a Client, cfg: &'a Config, cb_threshold: u32) -> Self {
        Self {
            client,
            cfg,
            breakers: Arc::new(HashMap::new()),
            cb_threshold,
            fetcher: None,
        }
    }

    pub fn with_fetcher(
        client: &'a Client,
        cfg: &'a Config,
        cb_threshold: u32,
        fetcher: &'a crate::net::Fetcher,
    ) -> Self {
        Self {
            client,
            cfg,
            breakers: Arc::new(HashMap::new()),
            cb_threshold,
            fetcher: Some(fetcher),
        }
    }

    pub fn breaker(&self, name: &str) -> Arc<AtomicU32> {
        self.breakers
            .get(name)
            .cloned()
            .unwrap_or_else(|| Arc::new(AtomicU32::new(0)))
    }

    pub async fn get_html(&self, url: &str) -> (String, reqwest::header::HeaderMap) {
        if let Some(fetcher) = self.fetcher {
            match fetcher.get(url).await {
                Ok(body) => {
                    return (body, reqwest::header::HeaderMap::new());
                }
                Err(crate::net::FetchError::Http { status, .. }) => {
                    tracing::info!(
                        "fetcher: non-challenge HTTP {} for {} — falling back to direct",
                        status,
                        url
                    );
                }
                Err(e) => {
                    tracing::debug!("fetcher get failed for {url}: {e} — falling back to direct");
                }
            }
        }
        match self.client.get(url).send().await {
            Ok(resp) => {
                let headers = resp.headers().clone();
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if !status.is_success() {
                    tracing::info!(
                        "direct GET {} returned {} ({} bytes)",
                        url,
                        status,
                        body.len()
                    );
                }
                (body, headers)
            }
            Err(e) => {
                tracing::warn!("direct GET failed for {url}: {e}");
                (String::new(), reqwest::header::HeaderMap::new())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    Apk,
    Xapk,
    Apkm,
    Apks,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub url: String,
    pub file_kind: FileKind,
    pub version_found: Option<String>,
    pub arch: String,
    pub extra_headers: Vec<(String, String)>,
    pub trace: Vec<String>,
}

impl Resolved {
    pub fn simple(
        url: impl Into<String>,
        version_found: impl Into<Option<String>>,
        arch: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            file_kind: FileKind::Apk,
            version_found: version_found.into(),
            arch: arch.into(),
            extra_headers: Vec::new(),
            trace: Vec::new(),
        }
    }
}

pub trait ApkSource: Send + Sync {
    fn name(&self) -> &'static str;
    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>>;

    fn fetch<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        resolved: &'a Resolved,
        dest: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>>;
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Hit { version: String, url: String },
    Miss { reason: String },
    Blocked { reason: String },
    SkippedBreaker,
    Skipped { reason: String },
    Error { reason: String },
}

impl Outcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Hit { .. } => "hit",
            Outcome::Miss { .. } => "miss",
            Outcome::Blocked { .. } => "blocked",
            Outcome::SkippedBreaker => "skipped-breaker",
            Outcome::Skipped { .. } => "skipped",
            Outcome::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceAttempt {
    pub source: String,
    pub outcome: Outcome,
    pub trace: Vec<String>,
}

pub async fn default_fetch(
    ctx: &SourceCtx<'_>,
    resolved: &Resolved,
    dest: &str,
) -> Result<PathBuf> {
    use crate::builder::{download_and_extract_if_bundle, is_turnstile_blocked};

    let mut req = ctx.client.get(&resolved.url);
    for (k, v) in &resolved.extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let referer = resolved
        .trace
        .last()
        .map(|s| s.as_str())
        .or(Some("https://apkpure.com/"));

    download_and_extract_if_bundle(ctx.client, &resolved.url, dest, referer).await?;

    if let Ok(meta) = std::fs::metadata(dest) {
        if meta.len() < 1_000_000 {
            if let Ok(snippet) = std::fs::read_to_string(dest) {
                let status = 200u16;
                let empty = reqwest::header::HeaderMap::new();
                if is_turnstile_blocked(status, &empty, &snippet) {
                    let _ = std::fs::remove_file(dest);
                    anyhow::bail!("Cloudflare challenge page saved as APK file — bypass needed");
                }
            }
        }
    }

    Ok(PathBuf::from(dest))
}

pub const DEFAULT_SOURCE_ORDER: &[&str] = &[
    "apkeep",
    "github",
    "codeberg",
    "gitlab",
    "apkpure",
    "apkmirror",
    "uptodown",
    "aptoide",
    "apkfab",
    "apkcombo",
];

pub fn source_order<'a>(
    registry: &'a [Box<dyn ApkSource>],
    app: &AppConfig,
    cfg: &Config,
) -> Vec<&'a dyn ApkSource> {
    let cfg_order: Vec<&str> = if cfg.apk_source_order.is_empty() {
        DEFAULT_SOURCE_ORDER.to_vec()
    } else {
        cfg.apk_source_order.iter().map(|s| s.as_str()).collect()
    };

    let priority: Vec<&str> = if app.strict_sources {
        app.source_priority.iter().map(|s| s.as_str()).collect()
    } else if app.source_priority.is_empty() {
        cfg_order
    } else {
        app.source_priority.iter().map(|s| s.as_str()).collect()
    };

    let mut ordered: Vec<&dyn ApkSource> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in &priority {
        if seen.insert(*name) {
            if let Some(src) = registry.iter().find(|s| s.name() == *name) {
                ordered.push(src.as_ref());
            }
        }
    }
    ordered
}

/// Constructs the default set of all available APK sources.
pub fn default_registry() -> Vec<Box<dyn ApkSource>> {
    vec![
        Box::new(apkeep::ApkeepSource),
        Box::new(apkmirror::ApkmirrorSource),
        Box::new(uptodown::UptodownSource),
        Box::new(apkpure::ApkpureSource),
        Box::new(aptoide::AptoideSource),
        Box::new(archive::ArchiveSource),
        Box::new(github::GithubSource),
        Box::new(codeberg::CodebergSource),
        Box::new(gitlab::GitlabSource),
        Box::new(apkcombo::ApkcomboSource),
        Box::new(apkfab::ApkfabSource),
    ]
}
