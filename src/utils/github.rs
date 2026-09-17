use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub target_commitish: String,
    #[serde(default)]
    pub prerelease: bool,
    pub assets: Vec<GhAsset>,
}

#[derive(Deserialize, Clone)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
}

pub async fn get_latest_release(
    client: &Client,
    repo: &str,
    branch: Option<&str>,
) -> Result<GhRelease> {
    let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=50");

    let resp = {
        let mut req = client
            .get(&url)
            .header("Accept", "application/vnd.github+json");
        if !token.is_empty() {
            req = req.bearer_auth(&token);
        }
        req.send().await.with_context(|| format!("GET {url}"))?
    };

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("GitHub repo {repo} not found (404)");
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        let remaining = resp
            .headers()
            .get("X-RateLimit-Remaining")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?");
        if remaining == "0" {
            anyhow::bail!(
                "GitHub API rate limit exceeded for {repo}. Set GITHUB_TOKEN to increase the limit."
            );
        }
    }

    let releases: Vec<GhRelease> = resp
        .error_for_status()
        .with_context(|| format!("GitHub API error for {repo}"))?
        .json()
        .await
        .with_context(|| format!("Parsing releases JSON for {repo}"))?;

    if let Some(target_b) = branch {
        let tb_lower = target_b.to_lowercase();
        if tb_lower == "dev" || tb_lower.contains("dev") {
            if let Some(rel) = releases.iter().find(|r| {
                r.target_commitish.eq_ignore_ascii_case("dev")
                    || r.tag_name.to_lowercase().contains("dev")
                    || r.prerelease
            }) {
                return Ok(rel.clone());
            }
        } else {
            let clean_ver = target_b.trim_start_matches('v');
            if let Some(rel) = releases.iter().find(|r| {
                r.tag_name == target_b
                    || r.tag_name.trim_start_matches('v') == clean_ver
                    || r.tag_name.contains(target_b)
                    || r.target_commitish.eq_ignore_ascii_case(target_b)
            }) {
                return Ok(rel.clone());
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
                    return Ok(rel);
                }
            }
        }
    } else {
        if let Some(stable) = releases.iter().find(|r| !r.prerelease) {
            return Ok(stable.clone());
        }
        if let Some(first) = releases.into_iter().next() {
            return Ok(first);
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
        .await?
        .error_for_status()
        .with_context(|| format!("GitHub /releases/latest for {repo}"))?
        .json()
        .await?;
    Ok(rel)
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

    let prefix = pattern.split('*').next().unwrap_or("");
    let suffix = pattern.rsplit('*').next().unwrap_or("");

    release
        .assets
        .into_iter()
        .find(|a| a.name.starts_with(prefix) && a.name.ends_with(suffix))
        .map(|a| a.browser_download_url)
        .ok_or_else(|| anyhow::anyhow!("No asset matching '{pattern}' in {repo}"))
}
