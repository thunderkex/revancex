use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub target_commitish: String,
    #[serde(default)]
    pub prerelease: bool,
    pub assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug)]
pub enum ReleaseCheck {
    NotModified,
    Modified {
        release: GhRelease,
        etag: Option<String>,
    },
}

#[derive(Debug)]
pub enum FetchReleaseError {
    RateLimited,
    NotFound,
    Other(anyhow::Error),
}

impl std::fmt::Display for FetchReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchReleaseError::RateLimited => write!(f, "GitHub API rate limit exceeded"),
            FetchReleaseError::NotFound => write!(f, "GitHub release or repo not found (404)"),
            FetchReleaseError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FetchReleaseError {}

pub async fn get_latest_release_conditional(
    client: &Client,
    repo: &str,
    branch: Option<&str>,
    if_none_match: Option<&str>,
) -> std::result::Result<ReleaseCheck, FetchReleaseError> {
    let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=50");

    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github+json");
    if !token.is_empty() {
        req = req.bearer_auth(&token);
    }
    if let Some(etag) = if_none_match {
        req = req.header("If-None-Match", etag);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| FetchReleaseError::Other(anyhow::anyhow!("GET {url}: {e}")))?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(ReleaseCheck::NotModified);
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(FetchReleaseError::NotFound);
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        let remaining = resp
            .headers()
            .get("X-RateLimit-Remaining")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        if remaining == "0" {
            return Err(FetchReleaseError::RateLimited);
        }
    }

    let new_etag = resp
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let releases: Vec<GhRelease> = resp
        .error_for_status()
        .map_err(|e| FetchReleaseError::Other(anyhow::anyhow!("GitHub API error for {repo}: {e}")))?
        .json()
        .await
        .map_err(|e| {
            FetchReleaseError::Other(anyhow::anyhow!("Parsing releases JSON for {repo}: {e}"))
        })?;

    if let Some(target_b) = branch {
        let tb_lower = target_b.to_lowercase();
        if tb_lower == "dev" || tb_lower.contains("dev") {
            if let Some(rel) = releases.iter().find(|r| {
                r.target_commitish.eq_ignore_ascii_case("dev")
                    || r.tag_name.to_lowercase().contains("dev")
                    || r.prerelease
            }) {
                return Ok(ReleaseCheck::Modified {
                    release: rel.clone(),
                    etag: new_etag,
                });
            }
        } else {
            let clean_ver = target_b.trim_start_matches('v');
            if let Some(rel) = releases.iter().find(|r| {
                r.tag_name == target_b
                    || r.tag_name.trim_start_matches('v') == clean_ver
                    || r.tag_name.contains(target_b)
                    || r.target_commitish.eq_ignore_ascii_case(target_b)
            }) {
                return Ok(ReleaseCheck::Modified {
                    release: rel.clone(),
                    etag: new_etag,
                });
            }
        }
        let tag_name = if target_b.starts_with('v') {
            target_b.to_string()
        } else {
            format!("v{target_b}")
        };
        let tag_url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag_name}");
        let mut req_tag = client
            .get(&tag_url)
            .header("Accept", "application/vnd.github+json");
        if !token.is_empty() {
            req_tag = req_tag.bearer_auth(&token);
        }
        if let Ok(resp) = req_tag.send().await {
            if resp.status().is_success() {
                if let Ok(rel) = resp.json::<GhRelease>().await {
                    return Ok(ReleaseCheck::Modified {
                        release: rel,
                        etag: new_etag,
                    });
                }
            }
        }
    } else {
        if let Some(stable) = releases.iter().find(|r| !r.prerelease) {
            return Ok(ReleaseCheck::Modified {
                release: stable.clone(),
                etag: new_etag,
            });
        }
        if let Some(first) = releases.into_iter().next() {
            return Ok(ReleaseCheck::Modified {
                release: first,
                etag: new_etag,
            });
        }
    }

    let url_latest = format!("https://api.github.com/repos/{repo}/releases/latest");
    let mut req_latest = client
        .get(&url_latest)
        .header("Accept", "application/vnd.github+json");
    if !token.is_empty() {
        req_latest = req_latest.bearer_auth(&token);
    }
    let rel = req_latest
        .send()
        .await
        .map_err(|e| FetchReleaseError::Other(e.into()))?
        .error_for_status()
        .map_err(|e| {
            FetchReleaseError::Other(anyhow::anyhow!("GitHub /releases/latest for {repo}: {e}"))
        })?
        .json()
        .await
        .map_err(|e| FetchReleaseError::Other(e.into()))?;
    Ok(ReleaseCheck::Modified {
        release: rel,
        etag: new_etag,
    })
}

pub async fn get_latest_release(
    client: &Client,
    repo: &str,
    branch: Option<&str>,
) -> Result<GhRelease> {
    match get_latest_release_conditional(client, repo, branch, None).await {
        Ok(ReleaseCheck::Modified { release, .. }) => Ok(release),
        Ok(ReleaseCheck::NotModified) => {
            anyhow::bail!("Unexpected 304 without If-None-Match for {repo}")
        }
        Err(FetchReleaseError::RateLimited) => anyhow::bail!(
            "GitHub API rate limit exceeded for {repo}. Set GITHUB_TOKEN to increase the limit."
        ),
        Err(FetchReleaseError::NotFound) => anyhow::bail!("GitHub repo {repo} not found (404)"),
        Err(FetchReleaseError::Other(e)) => Err(e),
    }
}

pub fn pick_asset<'a>(release: &'a GhRelease, pattern: &str) -> Option<&'a GhAsset> {
    let prefix = pattern.split('*').next().unwrap_or("");
    let suffix = pattern.rsplit('*').next().unwrap_or("");

    release
        .assets
        .iter()
        .find(|a| a.name.starts_with(prefix) && a.name.ends_with(suffix))
}

pub async fn resolve_asset_url(
    client: &Client,
    repo: &str,
    pattern: &str,
    branch: Option<&str>,
) -> Result<String> {
    let release = get_latest_release(client, repo, branch)
        .await
        .with_context(|| format!("Fetching release for {repo}"))?;

    pick_asset(&release, pattern)
        .map(|a| a.browser_download_url.clone())
        .ok_or_else(|| anyhow::anyhow!("No asset matching '{pattern}' in {repo}"))
}
