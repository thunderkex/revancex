pub mod apk;
pub mod cache;
pub mod doctor;
pub mod forge;
pub mod github;
pub mod keystore;
pub mod semver;
pub mod version;
pub mod zip;

use crate::config::Config;
use crate::orchestrator::BuildSummary;
use anyhow::{Context, Result};
use reqwest::Client;
use std::path::Path;
use tracing::info;

pub fn make_github_client(cfg: &Config) -> Result<Client> {
    let timeout = std::time::Duration::from_secs(cfg.build.download_timeout_secs);
    let ver = env!("CARGO_PKG_VERSION");
    let client = Client::builder()
        .timeout(timeout)
        .user_agent(format!("revancex/{ver}"))
        .build()?;
    Ok(client)
}

pub fn make_browser_client(cfg: &Config) -> Result<Client> {
    use reqwest::header::{
        HeaderMap, HeaderName, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT,
    };
    let mut headers = HeaderMap::new();
    let ua = random_user_agent();
    let val = HeaderValue::from_str(ua).unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_UA));
    headers.insert(USER_AGENT, val);
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::from_static(
            "\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"",
        ),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-mobile"),
        HeaderValue::from_static("?0"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-platform"),
        HeaderValue::from_static("\"Windows\""),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("document"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("navigate"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        HeaderName::from_static("upgrade-insecure-requests"),
        HeaderValue::from_static("1"),
    );

    let timeout = std::time::Duration::from_secs(cfg.build.download_timeout_secs);
    Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(timeout)
        .build()
        .map_err(Into::into)
}

pub fn is_command_available(cmd: &str) -> bool {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let p = dir.join(cmd);
            if p.is_file() {
                return true;
            }
            if cfg!(windows) && dir.join(format!("{cmd}.exe")).is_file() {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSidecar {
    pub repo: String,
    pub branch: Option<String>,
    pub tag: String,
    pub asset_name: String,
    pub sha256: String,
    pub etag: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadToolsSummary {
    pub tools_changed: bool,
    pub needed: usize,
    pub downloaded: usize,
    pub api_calls: u32,
    pub patch_results: std::collections::BTreeMap<String, Result<String, String>>,
}

pub fn compute_file_sha256<P: AsRef<std::path::Path>>(path: P) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let hash = ring::digest::digest(&ring::digest::SHA256, &bytes);
    let mut s = String::with_capacity(64);
    for b in hash.as_ref() {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
    }
    Ok(s)
}

pub fn load_tool_sidecar(file_path: &str) -> Option<ToolSidecar> {
    let sidecar_p = format!("{file_path}.meta.json");
    let content = std::fs::read_to_string(sidecar_p).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_tool_sidecar(file_path: &str, sidecar: &ToolSidecar) -> Result<()> {
    let sidecar_p = format!("{file_path}.meta.json");
    let json = serde_json::to_string_pretty(sidecar)?;
    std::fs::write(sidecar_p, json)?;
    Ok(())
}

pub fn check_sidecar_and_integrity(file_path: &str) -> (bool, Option<ToolSidecar>) {
    let sidecar = load_tool_sidecar(file_path);
    if !is_valid_jar_or_bundle(file_path) {
        let _ = std::fs::remove_file(file_path);
        let _ = std::fs::remove_file(format!("{file_path}.meta.json"));
        return (false, None);
    }
    if let Some(ref sc) = sidecar {
        if let Ok(actual_sha) = compute_file_sha256(file_path) {
            if actual_sha != sc.sha256 {
                tracing::warn!(
                    "SHA-256 mismatch for {file_path} against sidecar; removing file and sidecar"
                );
                let _ = std::fs::remove_file(file_path);
                let _ = std::fs::remove_file(format!("{file_path}.meta.json"));
                return (false, None);
            }
        } else {
            return (false, None);
        }
    }
    (true, sidecar)
}

pub fn is_sidecar_within_interval(sidecar: &ToolSidecar, interval_hours: u64) -> bool {
    if interval_hours == 0 {
        return false;
    }
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&sidecar.fetched_at) {
        let age_hours = (chrono::Utc::now() - parsed.with_timezone(&chrono::Utc)).num_hours();
        age_hours >= 0 && (age_hours as u64) < interval_hours
    } else {
        false
    }
}

pub fn clean_stale_tools(cfg: &Config) -> Result<()> {
    let tools_dir = std::path::Path::new(&cfg.build.tools_dir);
    if !tools_dir.exists() {
        return Ok(());
    }

    let mut referenced_stems: std::collections::HashSet<String> = std::collections::HashSet::new();
    for app in cfg.apps.values() {
        if app.enabled && !app.patch_source.is_empty() && app.patch_source != "none" {
            let (repo, source_branch) = cfg
                .patch_sources
                .get(&app.patch_source)
                .map(|p| (p.repo.clone(), p.branch.clone()))
                .unwrap_or_else(|| (app.patch_source.clone(), None));

            if repo.is_empty() {
                continue;
            }
            let safe_name = repo.replace('/', "_");
            let branch = app
                .patches_version
                .clone()
                .or(app.patch_branch.clone())
                .or(source_branch);

            referenced_stems.insert(format!("patches_{safe_name}"));
            if let Some(ref b) = branch {
                referenced_stems.insert(format!("patches_{safe_name}_{b}"));
            }
        }
    }

    for entry in std::fs::read_dir(tools_dir)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let fname = match p.file_name().and_then(|f| f.to_str()) {
            Some(f) => f,
            None => continue,
        };

        if fname.starts_with("patches_") || fname.starts_with("patches-") {
            let stem = if fname.ends_with(".mpp.meta.json") {
                fname.strip_suffix(".mpp.meta.json").unwrap()
            } else if fname.ends_with(".mpp") {
                fname.strip_suffix(".mpp").unwrap()
            } else if fname.ends_with(".jar.meta.json") {
                fname.strip_suffix(".jar.meta.json").unwrap()
            } else if fname.ends_with(".jar") {
                fname.strip_suffix(".jar").unwrap()
            } else {
                continue;
            };

            if !referenced_stems.contains(stem) {
                tracing::info!("clean: removing stale tool bundle {}", p.display());
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    Ok(())
}

pub async fn download_tools(
    cfg: &Config,
    targets: Option<&[String]>,
) -> Result<DownloadToolsSummary> {
    let client = make_github_client(cfg)?;
    let mut total_needed = 0usize;
    let mut total_downloaded = 0usize;
    let mut api_calls = 0u32;
    let mut tools_changed = false;
    let mut patch_results: std::collections::BTreeMap<String, Result<String, String>> =
        std::collections::BTreeMap::new();

    let cli_dest = format!("{}/morphe-cli.jar", cfg.build.tools_dir);
    if !cfg.cli_repo.is_empty() {
        total_needed += 1;
        let (valid, sidecar) = check_sidecar_and_integrity(&cli_dest);
        let within_interval = valid
            && sidecar
                .as_ref()
                .map(|s| is_sidecar_within_interval(s, cfg.build.tools_check_interval_hours))
                .unwrap_or(false);

        if within_interval {
            let tag = sidecar
                .as_ref()
                .map(|s| s.tag.as_str())
                .unwrap_or("unknown");
            println!("patches: {}@{} skipped(interval)", cfg.cli_repo, tag);
        } else {
            api_calls += 1;
            let if_none_match = sidecar.as_ref().and_then(|s| s.etag.as_deref());
            let rel_check =
                github::get_latest_release_conditional(&client, &cfg.cli_repo, None, if_none_match)
                    .await;
            match rel_check {
                Ok(github::ReleaseCheck::NotModified) => {
                    let tag = sidecar
                        .as_ref()
                        .map(|s| s.tag.as_str())
                        .unwrap_or("unknown");
                    println!("patches: {}@{} cached", cfg.cli_repo, tag);
                    if let Some(mut sc) = sidecar {
                        sc.fetched_at = chrono::Utc::now().to_rfc3339();
                        let _ = save_tool_sidecar(&cli_dest, &sc);
                    }
                }
                Ok(github::ReleaseCheck::Modified { release, etag }) => {
                    if let Some(asset) = github::pick_asset(&release, &cfg.cli_pattern) {
                        let is_same_tag =
                            valid && sidecar.as_ref().map(|s| &s.tag) == Some(&release.tag_name);
                        if is_same_tag {
                            println!("patches: {}@{} cached", cfg.cli_repo, release.tag_name);
                            if let Some(mut sc) = sidecar {
                                sc.etag = etag;
                                sc.fetched_at = chrono::Utc::now().to_rfc3339();
                                let _ = save_tool_sidecar(&cli_dest, &sc);
                            }
                        } else {
                            match download_file_with_client(
                                &client,
                                &asset.browser_download_url,
                                &cli_dest,
                                cfg.build.download_retries,
                                None,
                            )
                            .await
                            {
                                Ok(_) if is_valid_jar_or_bundle(&cli_dest) => {
                                    if let Ok(sha256) = compute_file_sha256(&cli_dest) {
                                        let sc = ToolSidecar {
                                            repo: cfg.cli_repo.clone(),
                                            branch: None,
                                            tag: release.tag_name.clone(),
                                            asset_name: asset.name.clone(),
                                            sha256,
                                            etag,
                                            fetched_at: chrono::Utc::now().to_rfc3339(),
                                        };
                                        let _ = save_tool_sidecar(&cli_dest, &sc);
                                        total_downloaded += 1;
                                        tools_changed = true;
                                        println!(
                                            "patches: {}@{} downloaded",
                                            cfg.cli_repo, release.tag_name
                                        );
                                    } else {
                                        println!(
                                            "patches: {}@{} failed",
                                            cfg.cli_repo, release.tag_name
                                        );
                                    }
                                }
                                Ok(_) => {
                                    let _ = std::fs::remove_file(&cli_dest);
                                    println!(
                                        "patches: {}@{} failed",
                                        cfg.cli_repo, release.tag_name
                                    );
                                }
                                Err(_) => {
                                    println!(
                                        "patches: {}@{} failed",
                                        cfg.cli_repo, release.tag_name
                                    );
                                }
                            }
                        }
                    } else {
                        println!("patches: {}@{} failed", cfg.cli_repo, release.tag_name);
                    }
                }
                Err(github::FetchReleaseError::RateLimited) => {
                    eprintln!(
                        "GitHub API rate limit exceeded for {}. Set GITHUB_TOKEN to increase the limit.",
                        cfg.cli_repo
                    );
                    if valid {
                        let tag = sidecar
                            .as_ref()
                            .map(|s| s.tag.as_str())
                            .unwrap_or("unknown");
                        println!("patches: {}@{} cached", cfg.cli_repo, tag);
                    } else {
                        println!("patches: {}@unknown failed", cfg.cli_repo);
                    }
                }
                Err(_) => {
                    if valid {
                        let tag = sidecar
                            .as_ref()
                            .map(|s| s.tag.as_str())
                            .unwrap_or("unknown");
                        println!("patches: {}@{} cached", cfg.cli_repo, tag);
                    } else {
                        println!("patches: {}@unknown failed", cfg.cli_repo);
                    }
                }
            }
        }
    }

    let apkeditor_dest = format!("{}/APKEditor.jar", cfg.build.tools_dir);
    if !cfg.cli_repo.is_empty() {
        total_needed += 1;
        if !is_valid_jar_or_bundle(&apkeditor_dest) {
            let url =
                "https://github.com/REAndroid/APKEditor/releases/download/V1.4.9/APKEditor-1.4.9.jar";
            if download_file_with_client(
                &client,
                url,
                &apkeditor_dest,
                cfg.build.download_retries,
                None,
            )
            .await
            .is_ok()
                && is_valid_jar_or_bundle(&apkeditor_dest)
            {
                if let Ok(sha256) = compute_file_sha256(&apkeditor_dest) {
                    let sc = ToolSidecar {
                        repo: "REAndroid/APKEditor".to_string(),
                        branch: None,
                        tag: "V1.4.9".to_string(),
                        asset_name: "APKEditor-1.4.9.jar".to_string(),
                        sha256,
                        etag: None,
                        fetched_at: chrono::Utc::now().to_rfc3339(),
                    };
                    let _ = save_tool_sidecar(&apkeditor_dest, &sc);
                    total_downloaded += 1;
                    tools_changed = true;
                }
            }
        }
    }

    let apkeep_filename = if cfg!(windows) {
        "apkeep.exe"
    } else {
        "apkeep"
    };
    let apkeep_dest = format!("{}/{apkeep_filename}", cfg.build.tools_dir);
    if !is_command_available("apkeep") && !std::path::Path::new(&apkeep_dest).exists() {
        total_needed += 1;
        let apkeep_url = if cfg!(windows) {
            "https://github.com/EFForg/apkeep/releases/download/1.1.0/apkeep-x86_64-pc-windows-msvc.exe"
        } else {
            "https://github.com/EFForg/apkeep/releases/download/1.1.0/apkeep-x86_64-unknown-linux-gnu"
        };
        if download_file_with_client(
            &client,
            apkeep_url,
            &apkeep_dest,
            cfg.build.download_retries,
            None,
        )
        .await
        .is_ok()
        {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&apkeep_dest, std::fs::Permissions::from_mode(0o755));
            }
            total_downloaded += 1;
            tools_changed = true;
        }
    }

    let mut repo_branches: std::collections::BTreeMap<(String, Option<String>), String> =
        std::collections::BTreeMap::new();

    let target_apps: Vec<&crate::config::AppConfig> = match targets {
        Some(tlist) => tlist.iter().filter_map(|id| cfg.apps.get(id)).collect(),
        None => cfg.apps.values().collect(),
    };

    for app in target_apps {
        if !app.patch_source.is_empty() && app.patch_source != "none" {
            let (repo, source_branch) = cfg
                .patch_sources
                .get(&app.patch_source)
                .map(|p| (p.repo.clone(), p.branch.clone()))
                .unwrap_or_else(|| (app.patch_source.clone(), None));

            if repo.is_empty() {
                continue;
            }

            let branch = app
                .patches_version
                .clone()
                .or(app.patch_branch.clone())
                .or(source_branch);

            repo_branches
                .entry((repo, branch))
                .or_insert_with(|| app.patch_source.clone());
        }
    }

    for ((repo, branch), source_alias) in repo_branches {
        total_needed += 1;
        let safe_name = repo.replace('/', "_");
        let dest = format!("{}/patches_{safe_name}.mpp", cfg.build.tools_dir);
        let versioned_dest = branch
            .as_ref()
            .map(|b| format!("{}/patches_{safe_name}_{b}.mpp", cfg.build.tools_dir));
        let primary_dest = versioned_dest.as_ref().unwrap_or(&dest);

        let (valid, sidecar) = check_sidecar_and_integrity(primary_dest);
        let within_interval = valid
            && sidecar
                .as_ref()
                .map(|s| is_sidecar_within_interval(s, cfg.build.tools_check_interval_hours))
                .unwrap_or(false);

        if within_interval {
            let tag = sidecar
                .as_ref()
                .map(|s| s.tag.as_str())
                .unwrap_or("unknown");
            println!("patches: {repo}@{tag} skipped(interval)");
            if let Some(ref vdest) = versioned_dest {
                if !is_valid_jar_or_bundle(&dest) {
                    let _ = std::fs::copy(vdest, &dest);
                }
            }
            patch_results.insert(source_alias, Ok(primary_dest.to_string()));
            continue;
        }

        api_calls += 1;
        let if_none_match = sidecar.as_ref().and_then(|s| s.etag.as_deref());
        let rel_check = github::get_latest_release_conditional(
            &client,
            &repo,
            branch.as_deref(),
            if_none_match,
        )
        .await;

        match rel_check {
            Ok(github::ReleaseCheck::NotModified) => {
                let tag = sidecar
                    .as_ref()
                    .map(|s| s.tag.as_str())
                    .unwrap_or("unknown");
                println!("patches: {repo}@{tag} cached");
                if let Some(mut sc) = sidecar {
                    sc.fetched_at = chrono::Utc::now().to_rfc3339();
                    let _ = save_tool_sidecar(primary_dest, &sc);
                }
                if let Some(ref vdest) = versioned_dest {
                    if !is_valid_jar_or_bundle(&dest) {
                        let _ = std::fs::copy(vdest, &dest);
                    }
                }
                patch_results.insert(source_alias, Ok(primary_dest.to_string()));
            }
            Ok(github::ReleaseCheck::Modified { release, etag }) => {
                let pattern = cfg
                    .patch_sources
                    .get(&source_alias)
                    .map(|p| p.asset_pattern.as_str())
                    .unwrap_or("*.mpp");
                if let Some(asset) = github::pick_asset(&release, pattern) {
                    let is_same_tag =
                        valid && sidecar.as_ref().map(|s| &s.tag) == Some(&release.tag_name);
                    if is_same_tag {
                        println!("patches: {repo}@{} cached", release.tag_name);
                        if let Some(mut sc) = sidecar {
                            sc.etag = etag;
                            sc.fetched_at = chrono::Utc::now().to_rfc3339();
                            let _ = save_tool_sidecar(primary_dest, &sc);
                        }
                        if let Some(ref vdest) = versioned_dest {
                            if !is_valid_jar_or_bundle(&dest) {
                                let _ = std::fs::copy(vdest, &dest);
                            }
                        }
                        patch_results.insert(source_alias, Ok(primary_dest.to_string()));
                    } else {
                        match download_file_with_client(
                            &client,
                            &asset.browser_download_url,
                            primary_dest,
                            cfg.build.download_retries,
                            None,
                        )
                        .await
                        {
                            Ok(_) if is_valid_jar_or_bundle(primary_dest) => {
                                match compute_file_sha256(primary_dest) {
                                    Ok(sha256) => {
                                        let sc = ToolSidecar {
                                            repo: repo.clone(),
                                            branch: branch.clone(),
                                            tag: release.tag_name.clone(),
                                            asset_name: asset.name.clone(),
                                            sha256,
                                            etag,
                                            fetched_at: chrono::Utc::now().to_rfc3339(),
                                        };
                                        let _ = save_tool_sidecar(primary_dest, &sc);
                                        if let Some(ref vdest) = versioned_dest {
                                            let _ = std::fs::copy(vdest, &dest);
                                        }
                                        total_downloaded += 1;
                                        tools_changed = true;
                                        println!("patches: {repo}@{} downloaded", release.tag_name);
                                        patch_results
                                            .insert(source_alias, Ok(primary_dest.to_string()));
                                    }
                                    Err(e) => {
                                        let _ = std::fs::remove_file(primary_dest);
                                        println!("patches: {repo}@{} failed", release.tag_name);
                                        patch_results.insert(
                                            source_alias,
                                            Err(format!("sha256 calculation failed: {e}")),
                                        );
                                    }
                                }
                            }
                            Ok(_) => {
                                let _ = std::fs::remove_file(primary_dest);
                                println!("patches: {repo}@{} failed", release.tag_name);
                                patch_results.insert(
                                    source_alias,
                                    Err("Downloaded file is not a valid JAR/bundle".to_string()),
                                );
                            }
                            Err(e) => {
                                println!("patches: {repo}@{} failed", release.tag_name);
                                patch_results
                                    .insert(source_alias, Err(format!("Download failed: {e}")));
                            }
                        }
                    }
                } else {
                    println!("patches: {repo}@{} failed", release.tag_name);
                    patch_results.insert(
                        source_alias,
                        Err(format!(
                            "No asset matching pattern '{pattern}' found in release"
                        )),
                    );
                }
            }
            Err(github::FetchReleaseError::RateLimited) => {
                eprintln!(
                    "GitHub API rate limit exceeded for {repo}. Set GITHUB_TOKEN to increase the limit."
                );
                if valid {
                    let tag = sidecar
                        .as_ref()
                        .map(|s| s.tag.as_str())
                        .unwrap_or("unknown");
                    println!("patches: {repo}@{tag} cached");
                    if let Some(ref vdest) = versioned_dest {
                        if !is_valid_jar_or_bundle(&dest) {
                            let _ = std::fs::copy(vdest, &dest);
                        }
                    }
                    patch_results.insert(source_alias, Ok(primary_dest.to_string()));
                } else {
                    println!("patches: {repo}@unknown failed");
                    patch_results.insert(
                        source_alias,
                        Err("Rate limited and no valid cached bundle".to_string()),
                    );
                }
            }
            Err(e) => {
                if valid {
                    let tag = sidecar
                        .as_ref()
                        .map(|s| s.tag.as_str())
                        .unwrap_or("unknown");
                    println!("patches: {repo}@{tag} cached");
                    if let Some(ref vdest) = versioned_dest {
                        if !is_valid_jar_or_bundle(&dest) {
                            let _ = std::fs::copy(vdest, &dest);
                        }
                    }
                    patch_results.insert(source_alias, Ok(primary_dest.to_string()));
                } else {
                    println!("patches: {repo}@unknown failed");
                    patch_results.insert(source_alias, Err(format!("GitHub release error: {e}")));
                }
            }
        }
    }

    println!(
        "tools: {} needed, {} downloaded, {} api calls",
        total_needed, total_downloaded, api_calls
    );

    Ok(DownloadToolsSummary {
        tools_changed,
        needed: total_needed,
        downloaded: total_downloaded,
        api_calls,
        patch_results,
    })
}

pub fn random_user_agent() -> &'static str {
    ua_generator::ua::spoof_ua()
}

pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

pub const UA: &str = DEFAULT_UA;

pub async fn download_file_with_client(
    client: &Client,
    url: &str,
    dest: &str,
    retries: u32,
    referer: Option<&str>,
) -> Result<String> {
    if let Some(parent) = Path::new(dest).parent() {
        std::fs::create_dir_all(parent)?;
    }
    for attempt in 0..=retries {
        match try_download(client, url, dest, referer).await {
            Ok(_) => return Ok(dest.to_string()),
            Err(e) if attempt < retries => {
                tracing::warn!("Download attempt {attempt} failed: {e}, retrying...");
                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

static ARIA2_AVAILABLE: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();

async fn is_aria2_available() -> bool {
    *ARIA2_AVAILABLE
        .get_or_init(|| async {
            let res = tokio::process::Command::new("aria2c")
                .arg("--version")
                .output()
                .await;
            let available = res.is_ok_and(|o| o.status.success());
            if available {
                info!("Transport: aria2c detected and enabled for multi-connection downloads");
            } else {
                info!("Transport: aria2c not available, falling back to HTTP client");
            }
            available
        })
        .await
}

async fn try_download(client: &Client, url: &str, dest: &str, referer: Option<&str>) -> Result<()> {
    if let Some(file_path) = url.strip_prefix("file://") {
        let clean = file_path.trim_start_matches('/');
        let local_path = if (clean.len() >= 2 && clean.chars().nth(1) == Some(':'))
            || std::path::Path::new(clean).exists()
        {
            clean
        } else {
            file_path
        };
        if let Some(parent) = std::path::Path::new(dest).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::copy(local_path, dest)?;
        return Ok(());
    }

    let path = Path::new(dest);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download.tmp");

    let ua = random_user_agent();

    if is_aria2_available().await {
        let referer_header = referer.map(|r| format!("Referer: {r}"));
        let mut args = vec![
            "-x",
            "16",
            "-s",
            "16",
            "-j",
            "16",
            "-k",
            "1M",
            "-U",
            ua,
            "--file-allocation=none",
            "--allow-overwrite=true",
            "--auto-file-renaming=false",
            "--summary-interval=0",
        ];
        if let Some(ref h) = referer_header {
            args.push("--header");
            args.push(h);
        }
        args.extend(["-d", dir.to_str().unwrap_or("."), "-o", filename, url]);

        let aria_res = tokio::process::Command::new("aria2c")
            .args(&args)
            .status()
            .await;

        if let Ok(status) = aria_res {
            if status.success() && Path::new(dest).exists() && std::fs::metadata(dest)?.len() > 0 {
                verify_apk_integrity(dest)?;
                info!("aria2 accelerated download complete: {dest}");
                return Ok(());
            }
        }
    }

    use futures::StreamExt;
    info!("Downloading {url} → {dest}");
    let send_request = |ref_hdr: Option<&str>| {
        let mut req = client
            .get(url)
            .header(reqwest::header::USER_AGENT, ua)
            .header(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
            )
            .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
            .header("sec-ch-ua", "\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"")
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", "\"Windows\"")
            .header("sec-fetch-dest", "document")
            .header("sec-fetch-mode", "navigate")
            .header("sec-fetch-site", "cross-site")
            .header("upgrade-insecure-requests", "1");
        if let Some(ref_val) = ref_hdr {
            req = req.header(reqwest::header::REFERER, ref_val);
        }
        req
    };

    let mut resp = send_request(referer).send().await?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN && referer.is_some() {
        tracing::warn!("403 Forbidden with Referer {referer:?} on {url}, retrying without Referer / base origin");
        let fallback_ref = if url.contains("apkpure") {
            Some("https://apkpure.com/")
        } else {
            None
        };
        let retry_resp = send_request(fallback_ref).send().await?;
        if retry_resp.status().is_success() {
            resp = retry_resp;
        } else if fallback_ref.is_some() {
            let bare_resp = send_request(None).send().await?;
            if bare_resp.status().is_success() {
                resp = bare_resp;
            } else {
                resp = retry_resp;
            }
        } else {
            resp = retry_resp;
        }
    }

    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!(
            "HTTP status client error (403 Forbidden) for url ({url}) (Referer: {referer:?})"
        );
    }
    let resp = resp.error_for_status()?;
    let tmp_dest = format!("{dest}.download.tmp");
    let mut stream = resp.bytes_stream();
    let mut file = std::fs::File::create(&tmp_dest)?;
    while let Some(chunk) = stream.next().await {
        if let Err(e) = std::io::Write::write_all(&mut file, &chunk?) {
            let _ = std::fs::remove_file(&tmp_dest);
            return Err(e.into());
        }
    }
    drop(file);

    if let Err(e) = verify_apk_integrity(&tmp_dest) {
        let _ = std::fs::remove_file(&tmp_dest);
        return Err(e);
    }

    if let Err(e) = std::fs::rename(&tmp_dest, dest) {
        let _ = std::fs::remove_file(&tmp_dest);
        return Err(e.into());
    }

    Ok(())
}

pub async fn ensure_apkeditor_jar(tools_dir: &str) -> Result<String> {
    let jar_candidates = [
        format!("{tools_dir}/APKEditor.jar"),
        "tools/APKEditor.jar".to_string(),
        "APKEditor.jar".to_string(),
    ];
    for cand in &jar_candidates {
        if Path::new(cand).exists() && is_valid_jar_or_bundle(cand) {
            return Ok(cand.clone());
        }
    }
    let dest = format!("{tools_dir}/APKEditor.jar");
    if let Some(parent) = Path::new(&dest).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let url = "https://github.com/REAndroid/APKEditor/releases/download/V1.4.9/APKEditor-1.4.9.jar";
    download_file_with_client(&client, url, &dest, 2, None).await?;
    Ok(dest)
}

pub fn is_valid_jar_or_bundle<P: AsRef<Path>>(path: P) -> bool {
    let p = path.as_ref();
    if !p.is_file() {
        return false;
    }
    let metadata = match std::fs::metadata(p) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if metadata.len() < 1024 {
        return false;
    }
    let file = match std::fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return false,
    };
    ::zip::ZipArchive::new(file).is_ok()
}

fn verify_apk_integrity(path: &str) -> Result<()> {
    if !path.ends_with(".apk") && !path.ends_with("-input.apk") {
        return Ok(());
    }
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 4];
    if file.read_exact(&mut header).is_err() || header != [0x50, 0x4B, 0x03, 0x04] {
        anyhow::bail!(
            "Download integrity failure: {path} is not a valid ZIP/APK archive (bad magic bytes)"
        );
    }
    Ok(())
}

pub fn get_keystore_password() -> String {
    std::env::var("KEYSTORE_PASSWORD")
        .ok()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && p.len() >= 6)
        .unwrap_or_else(|| "revanced".to_string())
}

pub fn check_java_version(min_version: u32) -> Result<u32> {
    let output = std::process::Command::new("java")
        .arg("-version")
        .output()
        .context("Java not found on PATH — please install JDK/JRE (minimum version required)")?;

    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let major = parse_java_major_version(&text).ok_or_else(|| {
        anyhow::anyhow!("Failed to parse Java major version from 'java -version' output:\n{text}")
    })?;

    if major < min_version {
        anyhow::bail!(
            "Java version {major} is below configured minimum of {min_version} — please upgrade JDK"
        );
    }

    Ok(major)
}

pub fn parse_java_major_version(output: &str) -> Option<u32> {
    for line in output.lines() {
        if let Some(idx) = line.find("version \"") {
            let rest = &line[idx + 9..];
            if let Some(end_quote) = rest.find('"') {
                let ver_str = &rest[..end_quote];
                let parts: Vec<&str> = ver_str.split('.').collect();
                if parts[0] == "1" && parts.len() > 1 {
                    return parts[1].parse().ok();
                } else {
                    return parts[0].parse().ok();
                }
            }
        }
    }
    None
}

pub fn write_build_report(summary: &BuildSummary, path: &str) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(&summary.reports)?;
    std::fs::write(path, &json).with_context(|| format!("Writing build report to {path}"))?;
    info!("Build report written to {path}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_java_major_version() {
        assert_eq!(
            parse_java_major_version("openjdk version \"21.0.12.1\" 2026-08-18 LTS"),
            Some(21)
        );
        assert_eq!(
            parse_java_major_version("java version \"17.0.2\" 2022-01-18 LTS"),
            Some(17)
        );
        assert_eq!(
            parse_java_major_version("java version \"1.8.0_312\""),
            Some(8)
        );
        assert_eq!(parse_java_major_version("invalid version string"), None);
    }
}
