#[cfg(test)]
use anyhow::Context;
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub target_commitish: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
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
    RateLimited { repo: String },
    NotFound { repo: String },
    Unauthorized { repo: String },
    Other(anyhow::Error),
}

impl std::fmt::Display for FetchReleaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchReleaseError::RateLimited { repo } => write!(
                f,
                "GitHub API rate limit exceeded for {repo}. Set GITHUB_TOKEN to increase the limit."
            ),
            FetchReleaseError::NotFound { repo } => {
                write!(f, "GitHub repository or releases not found: {repo} (404)")
            }
            FetchReleaseError::Unauthorized { repo } => write!(
                f,
                "GitHub authentication failed for {repo} — if external private repo, provide PAT"
            ),
            FetchReleaseError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FetchReleaseError {}

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn github_api_get(
    client: &Client,
    url: &str,
    if_none_match: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut req = client
        .get(url)
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = github_token() {
        req = req.bearer_auth(token);
    }
    if let Some(etag) = if_none_match {
        req = req.header("If-None-Match", etag);
    }
    req
}

async fn send_github_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    url: &str,
    repo: &str,
    if_none_match: Option<&str>,
) -> std::result::Result<Option<(T, Option<String>)>, FetchReleaseError> {
    let resp = github_api_get(client, url, if_none_match)
        .send()
        .await
        .map_err(|e| FetchReleaseError::Other(anyhow::anyhow!("GET {url}: {e}")))?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(FetchReleaseError::NotFound {
            repo: repo.to_string(),
        });
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(FetchReleaseError::Unauthorized {
            repo: repo.to_string(),
        });
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(FetchReleaseError::RateLimited {
            repo: repo.to_string(),
        });
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        let remaining = resp
            .headers()
            .get("X-RateLimit-Remaining")
            .and_then(|v| v.to_str().ok());
        if remaining == Some("0") {
            return Err(FetchReleaseError::RateLimited {
                repo: repo.to_string(),
            });
        }
    }

    let new_etag = resp
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let parsed = resp
        .error_for_status()
        .map_err(|e| FetchReleaseError::Other(anyhow::anyhow!("GitHub API error for {repo}: {e}")))?
        .json::<T>()
        .await
        .map_err(|e| {
            FetchReleaseError::Other(anyhow::anyhow!("Parsing releases JSON for {repo}: {e}"))
        })?;

    Ok(Some((parsed, new_etag)))
}

pub async fn get_latest_release_conditional(
    client: &Client,
    repo: &str,
    branch: Option<&str>,
    if_none_match: Option<&str>,
) -> std::result::Result<ReleaseCheck, FetchReleaseError> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=50");
    let Some((releases, new_etag)) =
        send_github_json::<Vec<GhRelease>>(client, &url, repo, if_none_match).await?
    else {
        return Ok(ReleaseCheck::NotModified);
    };

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
        if let Ok(Some((rel, _))) =
            send_github_json::<GhRelease>(client, &tag_url, repo, None).await
        {
            return Ok(ReleaseCheck::Modified {
                release: rel,
                etag: new_etag,
            });
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
    let (rel, _) = send_github_json::<GhRelease>(client, &url_latest, repo, None)
        .await?
        .ok_or_else(|| {
            FetchReleaseError::Other(anyhow::anyhow!(
                "Unexpected 304 without If-None-Match for {repo}"
            ))
        })?;
    Ok(ReleaseCheck::Modified {
        release: rel,
        etag: new_etag,
    })
}

pub async fn get_release_by_tag(
    client: &Client,
    repo: &str,
    tag: &str,
) -> std::result::Result<GhRelease, FetchReleaseError> {
    let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
    let (rel, _) = send_github_json::<GhRelease>(client, &url, repo, None)
        .await?
        .ok_or_else(|| {
            FetchReleaseError::Other(anyhow::anyhow!(
                "Unexpected 304 without If-None-Match for {repo}"
            ))
        })?;
    Ok(rel)
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
        Err(e) => Err(anyhow::Error::new(e)),
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

#[cfg(test)]
pub(crate) async fn resolve_asset_url(
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
