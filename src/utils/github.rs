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

const COMPARE_FILES_LIMIT: usize = 300;

#[derive(Deserialize)]
struct GhCompare {
    #[serde(default)]
    files: Vec<GhCompareFile>,
    #[serde(default)]
    merge_base_commit: Option<GhCompareCommit>,
}

#[derive(Deserialize)]
struct GhCompareCommit {
    #[serde(default)]
    commit: Option<GhCompareCommitInner>,
}

#[derive(Deserialize)]
struct GhCompareCommitInner {
    #[serde(default)]
    tree: Option<GhTreeRef>,
}

#[derive(Deserialize)]
struct GhTreeRef {
    sha: String,
}

#[derive(Deserialize)]
struct GhCompareFile {
    filename: String,
    #[serde(default)]
    previous_filename: Option<String>,
}

#[derive(Deserialize)]
struct GhGitTree {
    #[serde(default)]
    tree: Vec<GhGitTreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct GhGitTreeEntry {
    path: String,
    sha: String,
    #[serde(rename = "type")]
    entry_type: String,
}

fn diff_git_trees(base: &[GhGitTreeEntry], head: &[GhGitTreeEntry]) -> Vec<String> {
    let base_map: std::collections::HashMap<&str, &str> = base
        .iter()
        .filter(|e| e.entry_type != "tree")
        .map(|e| (e.path.as_str(), e.sha.as_str()))
        .collect();
    let head_map: std::collections::HashMap<&str, &str> = head
        .iter()
        .filter(|e| e.entry_type != "tree")
        .map(|e| (e.path.as_str(), e.sha.as_str()))
        .collect();

    let mut changed = Vec::new();
    for (&path, &sha) in &head_map {
        if base_map.get(path) != Some(&sha) {
            changed.push(path.to_string());
        }
    }
    for &path in base_map.keys() {
        if !head_map.contains_key(path) {
            changed.push(path.to_string());
        }
    }
    changed.sort();
    changed
}

async fn fetch_recursive_tree(
    client: &Client,
    repo: &str,
    tree_ref: &str,
) -> std::result::Result<Option<GhGitTree>, FetchReleaseError> {
    let url = format!("https://api.github.com/repos/{repo}/git/trees/{tree_ref}?recursive=1");
    let Some((tree, _)) = send_github_json::<GhGitTree>(client, &url, repo, None).await? else {
        return Ok(None);
    };
    Ok(Some(tree))
}

async fn get_changed_files_via_trees(
    client: &Client,
    repo: &str,
    base_ref: &str,
    head_ref: &str,
) -> std::result::Result<Option<Vec<String>>, FetchReleaseError> {
    let (base_tree, head_tree) = tokio::try_join!(
        fetch_recursive_tree(client, repo, base_ref),
        fetch_recursive_tree(client, repo, head_ref),
    )?;
    let (Some(base_tree), Some(head_tree)) = (base_tree, head_tree) else {
        return Ok(None);
    };
    if base_tree.truncated || head_tree.truncated {
        return get_changed_files_via_level_walk(client, repo, base_ref, head_ref).await;
    }
    Ok(Some(diff_git_trees(&base_tree.tree, &head_tree.tree)))
}

const TREE_WALK_REQUEST_LIMIT: usize = 400;

async fn get_changed_files_via_level_walk(
    client: &Client,
    repo: &str,
    base_ref: &str,
    head_ref: &str,
) -> std::result::Result<Option<Vec<String>>, FetchReleaseError> {
    async fn fetch(
        client: &Client,
        repo: &str,
        sha: &str,
    ) -> std::result::Result<Option<GhGitTree>, FetchReleaseError> {
        let url = format!("https://api.github.com/repos/{repo}/git/trees/{sha}");
        Ok(send_github_json::<GhGitTree>(client, &url, repo, None)
            .await?
            .map(|(t, _)| t))
    }

    let mut changed: Vec<String> = Vec::new();
    let mut level: Vec<(String, String, String)> =
        vec![(String::new(), base_ref.to_string(), head_ref.to_string())];
    let mut requests = 0usize;

    while !level.is_empty() {
        requests += level.len() * 2;
        if requests > TREE_WALK_REQUEST_LIMIT {
            return Ok(None);
        }
        let fetched = futures::future::join_all(level.iter().map(|(_, b, h)| async move {
            tokio::try_join!(fetch(client, repo, b), fetch(client, repo, h))
        }))
        .await;

        let mut next = Vec::new();
        for ((prefix, _, _), res) in level.iter().zip(fetched) {
            let (Some(base), Some(head)) = res? else {
                return Ok(None);
            };
            if base.truncated || head.truncated {
                return Ok(None);
            }
            let base_map: std::collections::HashMap<&str, &GhGitTreeEntry> =
                base.tree.iter().map(|e| (e.path.as_str(), e)).collect();
            let full = |name: &str| format!("{prefix}{name}");
            for e in &head.tree {
                match base_map.get(e.path.as_str()) {
                    Some(b) if b.sha == e.sha => {}
                    Some(b) if b.entry_type == "tree" && e.entry_type == "tree" => {
                        next.push((format!("{prefix}{}/", e.path), b.sha.clone(), e.sha.clone()));
                    }
                    Some(_) => changed.push(full(&e.path)),
                    None if e.entry_type == "tree" => changed.push(format!("{}/", full(&e.path))),
                    None => changed.push(full(&e.path)),
                }
            }
            let head_names: std::collections::HashSet<&str> =
                head.tree.iter().map(|e| e.path.as_str()).collect();
            for b in &base.tree {
                if !head_names.contains(b.path.as_str()) {
                    if b.entry_type == "tree" {
                        changed.push(format!("{}/", full(&b.path)));
                    } else {
                        changed.push(full(&b.path));
                    }
                }
            }
        }
        level = next;
    }
    changed.sort();
    Ok(Some(changed))
}

pub async fn get_changed_files(
    client: &Client,
    repo: &str,
    base: &str,
    head: &str,
) -> std::result::Result<Option<Vec<String>>, FetchReleaseError> {
    let url = format!("https://api.github.com/repos/{repo}/compare/{base}...{head}?per_page=1");
    match send_github_json::<GhCompare>(client, &url, repo, None).await {
        Ok(Some((cmp, _))) if cmp.files.len() < COMPARE_FILES_LIMIT => {
            let mut out = Vec::with_capacity(cmp.files.len());
            for f in cmp.files {
                out.push(f.filename);
                if let Some(p) = f.previous_filename {
                    out.push(p);
                }
            }
            Ok(Some(out))
        }
        Ok(Some((cmp, _))) => {
            let merge_base_tree = cmp
                .merge_base_commit
                .as_ref()
                .and_then(|c| c.commit.as_ref())
                .and_then(|c| c.tree.as_ref())
                .map(|t| t.sha.as_str())
                .unwrap_or(base);
            get_changed_files_via_trees(client, repo, merge_base_tree, head).await
        }
        Ok(None) => Ok(None),
        Err(
            e @ (FetchReleaseError::RateLimited { .. }
            | FetchReleaseError::NotFound { .. }
            | FetchReleaseError::Unauthorized { .. }),
        ) => Err(e),
        Err(FetchReleaseError::Other(_)) => {
            get_changed_files_via_trees(client, repo, base, head).await
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, sha: &str, entry_type: &str) -> GhGitTreeEntry {
        GhGitTreeEntry {
            path: path.to_string(),
            sha: sha.to_string(),
            entry_type: entry_type.to_string(),
        }
    }

    #[test]
    fn diff_git_trees_detects_added_modified_deleted_and_ignores_dirs() {
        let base = vec![
            entry("src", "tree_sha_old", "tree"),
            entry("src/unchanged.rs", "aaa", "blob"),
            entry("src/modified.rs", "bbb1", "blob"),
            entry("src/deleted.rs", "ccc", "blob"),
            entry("submod", "sub1", "commit"),
        ];
        let head = vec![
            entry("src", "tree_sha_new", "tree"),
            entry("src/unchanged.rs", "aaa", "blob"),
            entry("src/modified.rs", "bbb2", "blob"),
            entry("src/added.rs", "ddd", "blob"),
            entry("submod", "sub2", "commit"),
        ];

        let changed = diff_git_trees(&base, &head);
        assert_eq!(
            changed,
            vec![
                "src/added.rs",
                "src/deleted.rs",
                "src/modified.rs",
                "submod"
            ]
        );
    }

    #[test]
    fn diff_git_trees_identical_returns_empty() {
        let tree = vec![entry("a.txt", "111", "blob"), entry("dir", "222", "tree")];
        assert!(diff_git_trees(&tree, &tree).is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live GitHub API access"]
    async fn live_get_changed_files_handles_large_and_medium_diffs() {
        let client = reqwest::Client::builder()
            .user_agent("revancex-test")
            .build()
            .unwrap();

        let cases = [
            ("hxreborn/morphe-patches", "v1.80.0", "v1.82.0", 150),
            ("SysAdminDoc/HushFeed", "v0.82.0", "v0.83.0", 600),
            ("SysAdminDoc/Hushfacebook", "v0.22.0", "v0.23.0", 400),
            ("anddea/revanced-patches", "v4.4.0-dev.7", "v4.4.0-dev.8", 1),
            ("SysAdminDoc/HushFeed", "v0.83.0", "v0.84.0", 1),
        ];

        for (repo, base, head, min_expected) in cases {
            let files = get_changed_files(&client, repo, base, head)
                .await
                .unwrap_or_else(|e| panic!("{repo} {base}...{head} failed: {e}"))
                .unwrap_or_else(|| panic!("{repo} {base}...{head} returned None"));
            assert!(
                files.len() >= min_expected,
                "{repo} {base}...{head}: expected >= {min_expected} files, got {}",
                files.len()
            );
            println!("{repo} {base}...{head}: {} changed files", files.len());
        }
    }
}
