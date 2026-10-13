// Forge-agnostic release lookup — derived from RookieEnough/Morphe-AutoBuilds (GPL-3.0).
// https://github.com/RookieEnough/Morphe-AutoBuilds

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forge {
    Github,
    Gitlab,
    Codeberg { domain: String },
}

impl Default for Forge {
    fn default() -> Self {
        Forge::Github
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseSelector {
    Latest,
    MostRecent,
    Dev,
    Prerelease,
    Exact(String),
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub published_at: Option<String>,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    prerelease: bool,
    created_at: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GlAssetLink {
    name: String,
    url: String,
    #[serde(default)]
    direct_asset_url: Option<String>,
}

#[derive(Deserialize, Default)]
struct GlReleaseAssets {
    #[serde(default)]
    links: Vec<GlAssetLink>,
}

#[derive(Deserialize)]
struct GlRelease {
    tag_name: String,
    released_at: Option<String>,
    #[serde(default)]
    assets: GlReleaseAssets,
}

#[derive(Deserialize)]
struct CbAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct CbRelease {
    tag_name: String,
    prerelease: bool,
    created_at: Option<String>,
    #[serde(default)]
    assets: Vec<CbAsset>,
}

pub async fn release(
    client: &Client,
    forge: &Forge,
    repo_or_project: &str,
    selector: &ReleaseSelector,
) -> Result<Release> {
    match forge {
        Forge::Github => fetch_github_release(client, repo_or_project, selector).await,
        Forge::Gitlab => fetch_gitlab_release(client, repo_or_project, selector).await,
        Forge::Codeberg { domain } => {
            fetch_codeberg_release(client, domain, repo_or_project, selector).await
        }
    }
}

async fn fetch_github_release(
    client: &Client,
    repo: &str,
    selector: &ReleaseSelector,
) -> Result<Release> {
    let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
    let base_url = format!("https://api.github.com/repos/{repo}/releases");

    let mut req = client.get(&base_url);
    if !token.is_empty() {
        req = req.bearer_auth(&token);
    }

    let resp = req
        .send()
        .await
        .with_context(|| format!("GitHub API request failed for {repo}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        bail!("GitHub API rate limited for {repo} — set GITHUB_TOKEN to increase limits");
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        bail!("GitHub repository or releases not found: {repo}");
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        bail!("GitHub authentication failed for {repo} — if external private repo, provide PAT");
    }
    if !status.is_success() {
        bail!("GitHub API returned {status} for {repo}");
    }

    let releases: Vec<GhRelease> = resp.json().await?;
    if releases.is_empty() {
        bail!("No releases found for GitHub repo {repo}");
    }

    let chosen = match selector {
        ReleaseSelector::Latest => releases.iter().find(|r| !r.prerelease),
        ReleaseSelector::MostRecent => releases.first(),
        ReleaseSelector::Dev => releases.iter().find(|r| r.tag_name.contains("dev")),
        ReleaseSelector::Prerelease => releases.iter().find(|r| r.prerelease),
        ReleaseSelector::Exact(tag) => releases.iter().find(|r| &r.tag_name == tag),
    };

    let r = chosen.ok_or_else(|| {
        anyhow::anyhow!("No matching release found for {repo} with selector {selector:?}")
    })?;

    Ok(Release {
        tag: r.tag_name.clone(),
        published_at: r.published_at.clone().or_else(|| r.created_at.clone()),
        assets: r
            .assets
            .iter()
            .map(|a| Asset {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
            })
            .collect(),
    })
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

async fn fetch_gitlab_release(
    client: &Client,
    project: &str,
    selector: &ReleaseSelector,
) -> Result<Release> {
    let token = std::env::var("GITLAB_TOKEN").unwrap_or_default();
    let encoded_project = percent_encode(project);
    let base_url = format!("https://gitlab.com/api/v4/projects/{encoded_project}/releases");

    let mut req = client.get(&base_url);
    if !token.is_empty() {
        req = req.header("PRIVATE-TOKEN", &token);
    }

    let resp = req
        .send()
        .await
        .with_context(|| format!("GitLab API request failed for {project}"))?;

    let status = resp.status();
    if !status.is_success() {
        bail!("GitLab API returned {status} for {project}");
    }

    let releases: Vec<GlRelease> = resp.json().await?;
    if releases.is_empty() {
        bail!("No releases found for GitLab project {project}");
    }

    let chosen = match selector {
        ReleaseSelector::Latest | ReleaseSelector::MostRecent => releases.first(),
        ReleaseSelector::Dev => releases.iter().find(|r| r.tag_name.contains("dev")),
        ReleaseSelector::Prerelease => releases.iter().find(|r| r.tag_name.contains("pre")),
        ReleaseSelector::Exact(tag) => releases.iter().find(|r| &r.tag_name == tag),
    };

    let r = chosen.ok_or_else(|| {
        anyhow::anyhow!("No matching release found for {project} with selector {selector:?}")
    })?;

    Ok(Release {
        tag: r.tag_name.clone(),
        published_at: r.released_at.clone(),
        assets: r
            .assets
            .links
            .iter()
            .map(|l| Asset {
                name: l.name.clone(),
                url: l.direct_asset_url.clone().unwrap_or_else(|| l.url.clone()),
            })
            .collect(),
    })
}

async fn fetch_codeberg_release(
    client: &Client,
    domain: &str,
    repo: &str,
    selector: &ReleaseSelector,
) -> Result<Release> {
    let base_url = format!("https://{domain}/api/v1/repos/{repo}/releases");

    let resp = client
        .get(&base_url)
        .send()
        .await
        .with_context(|| format!("Codeberg API request failed for {repo} on {domain}"))?;

    let status = resp.status();
    if !status.is_success() {
        bail!("Codeberg API returned {status} for {repo} on {domain}");
    }

    let releases: Vec<CbRelease> = resp.json().await?;
    if releases.is_empty() {
        bail!("No releases found for Codeberg repo {repo}");
    }

    let chosen = match selector {
        ReleaseSelector::Latest => releases.iter().find(|r| !r.prerelease),
        ReleaseSelector::MostRecent => releases.first(),
        ReleaseSelector::Dev => releases.iter().find(|r| r.tag_name.contains("dev")),
        ReleaseSelector::Prerelease => releases.iter().find(|r| r.prerelease),
        ReleaseSelector::Exact(tag) => releases.iter().find(|r| &r.tag_name == tag),
    };

    let r = chosen.ok_or_else(|| {
        anyhow::anyhow!("No matching release found for {repo} with selector {selector:?}")
    })?;

    Ok(Release {
        tag: r.tag_name.clone(),
        published_at: r.created_at.clone(),
        assets: r
            .assets
            .iter()
            .map(|a| Asset {
                name: a.name.clone(),
                url: a.browser_download_url.clone(),
            })
            .collect(),
    })
}
