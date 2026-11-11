use crate::config::Config;
use crate::ui;
use crate::utils::github;
use anyhow::Result;
use reqwest::Client;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use super::http::download_file_with_client;
use super::system::{is_command_available, warn_io};

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
    pub patch_results: BTreeMap<String, Result<String, String>>,
}

pub fn compute_file_sha256<P: AsRef<Path>>(path: P) -> Result<String> {
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

pub(crate) async fn save_tool_sidecar_async(file_path: &str, sidecar: &ToolSidecar) -> Result<()> {
    let sidecar_p = format!("{file_path}.meta.json");
    let json = serde_json::to_string_pretty(sidecar)?;
    tokio::fs::write(sidecar_p, json).await?;
    Ok(())
}

pub(crate) async fn check_sidecar_and_integrity_async(
    file_path: &str,
) -> (bool, Option<ToolSidecar>) {
    let p = file_path.to_string();
    tokio::task::spawn_blocking(move || check_sidecar_and_integrity(&p))
        .await
        .unwrap_or((false, None))
}

pub(crate) async fn is_valid_jar_or_bundle_async(path: &str) -> bool {
    let p = path.to_string();
    tokio::task::spawn_blocking(move || is_valid_jar_or_bundle(&p))
        .await
        .unwrap_or(false)
}

pub(crate) async fn compute_file_sha256_async(path: &str) -> Result<String> {
    let p = path.to_string();
    tokio::task::spawn_blocking(move || compute_file_sha256(&p))
        .await
        .map_err(|e| anyhow::anyhow!("sha256 task panicked: {e}"))?
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
                warn_io(
                    std::fs::remove_file(file_path),
                    &format!("Removing mismatched tool {file_path}"),
                );
                warn_io(
                    std::fs::remove_file(format!("{file_path}.meta.json")),
                    &format!("Removing stale sidecar {file_path}.meta.json"),
                );
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
    let tools_dir = Path::new(&cfg.build.tools_dir);
    if !tools_dir.exists() {
        return Ok(());
    }

    let mut referenced_stems: HashSet<String> = HashSet::new();
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
        let p = entry?.path();
        if !p.is_file() {
            continue;
        }
        let Some(fname) = p.file_name().and_then(|f| f.to_str()) else {
            continue;
        };

        if fname.starts_with("patches_") || fname.starts_with("patches-") {
            let stem = [".mpp.meta.json", ".mpp", ".jar.meta.json", ".jar"]
                .iter()
                .find_map(|ext| fname.strip_suffix(ext));
            let Some(stem) = stem else {
                continue;
            };
            if !referenced_stems.contains(stem) {
                tracing::info!("clean: removing stale tool bundle {}", p.display());
                warn_io(
                    std::fs::remove_file(&p),
                    &format!("Removing stale tool {}", p.display()),
                );
            }
        }
    }

    Ok(())
}

fn patch_line(msg: String) {
    let body = msg.strip_prefix("patches: ").unwrap_or(&msg);
    let (target, status) = body.rsplit_once(' ').unwrap_or((body, ""));
    let (repo, tag) = target.split_once('@').unwrap_or((target, ""));
    let st = match status {
        "failed" => ui::Status::Fail,
        s if s.starts_with("skipped") => ui::Status::Skip,
        _ => ui::Status::Ok,
    };
    ui::item(st, repo, tag, status, None);
}

struct BundleCheckStats {
    api_calls: u32,
    downloaded: bool,
}

async fn ensure_cli_tool(
    cfg: &Config,
    tools_dir: &str,
    client: &Client,
) -> Option<BundleCheckStats> {
    if cfg.cli_repo.is_empty() {
        return None;
    }
    let cli_dest = format!("{tools_dir}/morphe-cli.jar");
    let (valid, sidecar) = check_sidecar_and_integrity_async(&cli_dest).await;
    if valid
        && sidecar
            .as_ref()
            .is_some_and(|s| is_sidecar_within_interval(s, cfg.build.tools_check_interval_hours))
    {
        let tag = sidecar
            .as_ref()
            .map(|s| s.tag.as_str())
            .unwrap_or("unknown");
        patch_line(format!(
            "patches: {}@{} skipped(interval)",
            cfg.cli_repo, tag
        ));
        return Some(BundleCheckStats {
            api_calls: 0,
            downloaded: false,
        });
    }

    let if_none_match = sidecar.as_ref().and_then(|s| s.etag.as_deref());
    let mut downloaded = false;
    match github::get_latest_release_conditional(client, &cfg.cli_repo, None, if_none_match).await {
        Ok(github::ReleaseCheck::NotModified) => {
            let tag = sidecar
                .as_ref()
                .map(|s| s.tag.as_str())
                .unwrap_or("unknown");
            patch_line(format!("patches: {}@{} cached", cfg.cli_repo, tag));
            if let Some(mut sc) = sidecar {
                sc.fetched_at = chrono::Utc::now().to_rfc3339();
                warn_io(
                    save_tool_sidecar_async(&cli_dest, &sc).await,
                    &format!("Saving sidecar for {cli_dest}"),
                );
            }
        }
        Ok(github::ReleaseCheck::Modified { release, etag }) => {
            if let Some(asset) = github::pick_asset(&release, &cfg.cli_pattern) {
                let is_same_tag =
                    valid && sidecar.as_ref().map(|s| &s.tag) == Some(&release.tag_name);
                if is_same_tag {
                    patch_line(format!(
                        "patches: {}@{} cached",
                        cfg.cli_repo, release.tag_name
                    ));
                    if let Some(mut sc) = sidecar {
                        sc.etag = etag;
                        sc.fetched_at = chrono::Utc::now().to_rfc3339();
                        warn_io(
                            save_tool_sidecar_async(&cli_dest, &sc).await,
                            &format!("Saving sidecar for {cli_dest}"),
                        );
                    }
                } else {
                    match download_file_with_client(
                        client,
                        &asset.browser_download_url,
                        &cli_dest,
                        cfg.build.download_retries,
                        None,
                    )
                    .await
                    {
                        Ok(_) if is_valid_jar_or_bundle_async(&cli_dest).await => {
                            if let Ok(sha256) = compute_file_sha256_async(&cli_dest).await {
                                let sc = ToolSidecar {
                                    repo: cfg.cli_repo.clone(),
                                    branch: None,
                                    tag: release.tag_name.clone(),
                                    asset_name: asset.name.clone(),
                                    sha256,
                                    etag,
                                    fetched_at: chrono::Utc::now().to_rfc3339(),
                                };
                                warn_io(
                                    save_tool_sidecar_async(&cli_dest, &sc).await,
                                    &format!("Saving sidecar for {cli_dest}"),
                                );
                                downloaded = true;
                                patch_line(format!(
                                    "patches: {}@{} downloaded",
                                    cfg.cli_repo, release.tag_name
                                ));
                            } else {
                                patch_line(format!(
                                    "patches: {}@{} failed",
                                    cfg.cli_repo, release.tag_name
                                ));
                            }
                        }
                        Ok(_) => {
                            let _ = tokio::fs::remove_file(&cli_dest).await;
                            patch_line(format!(
                                "patches: {}@{} failed",
                                cfg.cli_repo, release.tag_name
                            ));
                        }
                        Err(_) => {
                            patch_line(format!(
                                "patches: {}@{} failed",
                                cfg.cli_repo, release.tag_name
                            ));
                        }
                    }
                }
            } else {
                patch_line(format!(
                    "patches: {}@{} failed",
                    cfg.cli_repo, release.tag_name
                ));
            }
        }
        Err(err @ github::FetchReleaseError::RateLimited { .. }) => {
            ui::warn_once(&err.to_string());
            let tag = sidecar
                .as_ref()
                .map(|s| s.tag.as_str())
                .unwrap_or("unknown");
            patch_line(if valid {
                format!("patches: {}@{} cached", cfg.cli_repo, tag)
            } else {
                format!("patches: {}@unknown failed", cfg.cli_repo)
            });
        }
        Err(_) => {
            let tag = sidecar
                .as_ref()
                .map(|s| s.tag.as_str())
                .unwrap_or("unknown");
            patch_line(if valid {
                format!("patches: {}@{} cached", cfg.cli_repo, tag)
            } else {
                format!("patches: {}@unknown failed", cfg.cli_repo)
            });
        }
    }

    Some(BundleCheckStats {
        api_calls: 1,
        downloaded,
    })
}

async fn ensure_apkeditor_present(cfg: &Config, tools_dir: &str, client: &Client) -> bool {
    let apkeditor_dest = format!("{tools_dir}/APKEditor.jar");
    if is_valid_jar_or_bundle_async(&apkeditor_dest).await {
        return false;
    }
    let url = "https://github.com/REAndroid/APKEditor/releases/download/V1.4.9/APKEditor-1.4.9.jar";
    if download_file_with_client(
        client,
        url,
        &apkeditor_dest,
        cfg.build.download_retries,
        None,
    )
    .await
    .is_ok()
        && is_valid_jar_or_bundle_async(&apkeditor_dest).await
    {
        if let Ok(sha256) = compute_file_sha256_async(&apkeditor_dest).await {
            let sc = ToolSidecar {
                repo: "REAndroid/APKEditor".to_string(),
                branch: None,
                tag: "V1.4.9".to_string(),
                asset_name: "APKEditor-1.4.9.jar".to_string(),
                sha256,
                etag: None,
                fetched_at: chrono::Utc::now().to_rfc3339(),
            };
            warn_io(
                save_tool_sidecar_async(&apkeditor_dest, &sc).await,
                &format!("Saving sidecar for {apkeditor_dest}"),
            );
            return true;
        }
    }
    false
}

async fn ensure_apkeep_present(cfg: &Config, tools_dir: &str, client: &Client) -> Option<bool> {
    let apkeep_filename = if cfg!(windows) {
        "apkeep.exe"
    } else {
        "apkeep"
    };
    let apkeep_dest = format!("{tools_dir}/{apkeep_filename}");
    if is_command_available("apkeep") || tokio::fs::metadata(&apkeep_dest).await.is_ok() {
        return None;
    }
    let apkeep_url = if cfg!(windows) {
        "https://github.com/EFForg/apkeep/releases/download/1.1.0/apkeep-x86_64-pc-windows-msvc.exe"
    } else {
        "https://github.com/EFForg/apkeep/releases/download/1.1.0/apkeep-x86_64-unknown-linux-gnu"
    };
    let ok = download_file_with_client(
        client,
        apkeep_url,
        &apkeep_dest,
        cfg.build.download_retries,
        None,
    )
    .await
    .is_ok();
    if ok {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            warn_io(
                tokio::fs::set_permissions(&apkeep_dest, std::fs::Permissions::from_mode(0o755))
                    .await,
                &format!("Setting executable permissions on {apkeep_dest}"),
            );
        }
    }
    Some(ok)
}

fn collect_repo_branches(
    cfg: &Config,
    targets: Option<&[String]>,
) -> BTreeMap<(String, Option<String>), String> {
    let mut repo_branches: BTreeMap<(String, Option<String>), String> = BTreeMap::new();
    let target_apps: Vec<&crate::config::AppConfig> = match targets {
        Some(tlist) => tlist.iter().filter_map(|id| cfg.apps.get(id)).collect(),
        None => cfg.apps.values().collect(),
    };
    for app in target_apps {
        if !app.enabled || app.patch_source.is_empty() || app.patch_source == "none" {
            continue;
        }
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
    repo_branches
}

async fn sync_versioned_copy(versioned_dest: Option<&str>, dest: &str) {
    if let Some(vdest) = versioned_dest {
        if !is_valid_jar_or_bundle_async(dest).await {
            warn_io(
                tokio::fs::copy(vdest, dest).await,
                &format!("Copying {vdest} -> {dest}"),
            );
        }
    }
}

struct PatchBundlePaths<'a> {
    repo: &'a str,
    branch: Option<&'a str>,
    source_alias: &'a str,
    primary_dest: &'a str,
    versioned_dest: Option<&'a str>,
    dest: &'a str,
}

async fn download_and_store_patch_bundle(
    cfg: &Config,
    client: &Client,
    paths: &PatchBundlePaths<'_>,
    tag_name: &str,
    asset: &github::GhAsset,
    etag: Option<String>,
) -> (Result<String, String>, bool) {
    match download_file_with_client(
        client,
        &asset.browser_download_url,
        paths.primary_dest,
        cfg.build.download_retries,
        None,
    )
    .await
    {
        Ok(_) if is_valid_jar_or_bundle_async(paths.primary_dest).await => {
            match compute_file_sha256_async(paths.primary_dest).await {
                Ok(sha256) => {
                    let sc = ToolSidecar {
                        repo: paths.repo.to_string(),
                        branch: paths.branch.map(str::to_string),
                        tag: tag_name.to_string(),
                        asset_name: asset.name.clone(),
                        sha256,
                        etag,
                        fetched_at: chrono::Utc::now().to_rfc3339(),
                    };
                    warn_io(
                        save_tool_sidecar_async(paths.primary_dest, &sc).await,
                        &format!("Saving sidecar for {}", paths.primary_dest),
                    );
                    if let Some(vdest) = paths.versioned_dest {
                        warn_io(
                            tokio::fs::copy(vdest, paths.dest).await,
                            &format!("Copying {vdest} -> {}", paths.dest),
                        );
                    }
                    patch_line(format!("patches: {}@{tag_name} downloaded", paths.repo));
                    (Ok(paths.primary_dest.to_string()), true)
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(paths.primary_dest).await;
                    patch_line(format!("patches: {}@{tag_name} failed", paths.repo));
                    (Err(format!("sha256 calculation failed: {e}")), false)
                }
            }
        }
        Ok(_) => {
            let _ = tokio::fs::remove_file(paths.primary_dest).await;
            patch_line(format!("patches: {}@{tag_name} failed", paths.repo));
            (
                Err("Downloaded file is not a valid JAR/bundle".to_string()),
                false,
            )
        }
        Err(e) => {
            patch_line(format!("patches: {}@{tag_name} failed", paths.repo));
            (Err(format!("Download failed: {e}")), false)
        }
    }
}

async fn fallback_cached_bundle(
    paths: &PatchBundlePaths<'_>,
    sidecar: Option<&ToolSidecar>,
    valid: bool,
    err_msg: String,
) -> Result<String, String> {
    if valid {
        let tag = sidecar.map(|s| s.tag.as_str()).unwrap_or("unknown");
        patch_line(format!("patches: {}@{tag} cached", paths.repo));
        sync_versioned_copy(paths.versioned_dest, paths.dest).await;
        Ok(paths.primary_dest.to_string())
    } else {
        patch_line(format!("patches: {}@unknown failed", paths.repo));
        Err(err_msg)
    }
}

async fn handle_modified_patch_release(
    cfg: &Config,
    client: &Client,
    paths: &PatchBundlePaths<'_>,
    valid: bool,
    sidecar: Option<ToolSidecar>,
    release: github::GhRelease,
    etag: Option<String>,
) -> (Result<String, String>, bool) {
    let pattern = cfg
        .patch_sources
        .get(paths.source_alias)
        .map(|p| p.asset_pattern.as_str())
        .unwrap_or("*.mpp");
    let Some(asset) = github::pick_asset(&release, pattern) else {
        patch_line(format!(
            "patches: {}@{} failed",
            paths.repo, release.tag_name
        ));
        return (
            Err(format!(
                "No asset matching pattern '{pattern}' found in release"
            )),
            false,
        );
    };

    if valid && sidecar.as_ref().map(|s| &s.tag) == Some(&release.tag_name) {
        patch_line(format!(
            "patches: {}@{} cached",
            paths.repo, release.tag_name
        ));
        if let Some(mut sc) = sidecar {
            sc.etag = etag;
            sc.fetched_at = chrono::Utc::now().to_rfc3339();
            warn_io(
                save_tool_sidecar_async(paths.primary_dest, &sc).await,
                &format!("Saving sidecar for {}", paths.primary_dest),
            );
        }
        sync_versioned_copy(paths.versioned_dest, paths.dest).await;
        return (Ok(paths.primary_dest.to_string()), false);
    }

    download_and_store_patch_bundle(cfg, client, paths, &release.tag_name, asset, etag).await
}

async fn ensure_patch_bundle(
    cfg: &Config,
    tools_dir: &str,
    repo: &str,
    branch: Option<&str>,
    source_alias: &str,
    client: &Client,
) -> (Result<String, String>, BundleCheckStats) {
    let safe_name = repo.replace('/', "_");
    let dest = format!("{tools_dir}/patches_{safe_name}.mpp");
    let versioned_dest = branch.map(|b| format!("{tools_dir}/patches_{safe_name}_{b}.mpp"));
    let primary_dest = versioned_dest.as_deref().unwrap_or(&dest);
    let paths = PatchBundlePaths {
        repo,
        branch,
        source_alias,
        primary_dest,
        versioned_dest: versioned_dest.as_deref(),
        dest: &dest,
    };

    let (valid, sidecar) = check_sidecar_and_integrity_async(primary_dest).await;
    if valid
        && sidecar
            .as_ref()
            .is_some_and(|s| is_sidecar_within_interval(s, cfg.build.tools_check_interval_hours))
    {
        let tag = sidecar
            .as_ref()
            .map(|s| s.tag.as_str())
            .unwrap_or("unknown");
        patch_line(format!("patches: {repo}@{tag} skipped(interval)"));
        sync_versioned_copy(paths.versioned_dest, &dest).await;
        return (
            Ok(primary_dest.to_string()),
            BundleCheckStats {
                api_calls: 0,
                downloaded: false,
            },
        );
    }

    let if_none_match = sidecar.as_ref().and_then(|s| s.etag.as_deref());
    let rel_check =
        github::get_latest_release_conditional(client, repo, branch, if_none_match).await;
    let mut downloaded = false;
    let res = match rel_check {
        Ok(github::ReleaseCheck::NotModified) => {
            let tag = sidecar
                .as_ref()
                .map(|s| s.tag.as_str())
                .unwrap_or("unknown");
            patch_line(format!("patches: {repo}@{tag} cached"));
            if let Some(mut sc) = sidecar {
                sc.fetched_at = chrono::Utc::now().to_rfc3339();
                warn_io(
                    save_tool_sidecar_async(primary_dest, &sc).await,
                    &format!("Saving sidecar for {primary_dest}"),
                );
            }
            sync_versioned_copy(paths.versioned_dest, &dest).await;
            Ok(primary_dest.to_string())
        }
        Ok(github::ReleaseCheck::Modified { release, etag }) => {
            let (r, dl) =
                handle_modified_patch_release(cfg, client, &paths, valid, sidecar, release, etag)
                    .await;
            downloaded = dl;
            r
        }
        Err(err @ github::FetchReleaseError::RateLimited { .. }) => {
            ui::warn_once(&err.to_string());
            fallback_cached_bundle(
                &paths,
                sidecar.as_ref(),
                valid,
                "Rate limited and no valid cached bundle".to_string(),
            )
            .await
        }
        Err(e) => {
            fallback_cached_bundle(
                &paths,
                sidecar.as_ref(),
                valid,
                format!("GitHub release error: {e}"),
            )
            .await
        }
    };

    (
        res,
        BundleCheckStats {
            api_calls: 1,
            downloaded,
        },
    )
}

pub async fn download_tools(
    cfg: &Config,
    targets: Option<&[String]>,
    client: &Client,
) -> Result<DownloadToolsSummary> {
    let tools_dir = &cfg.build.tools_dir;
    let mut total_needed = 0usize;
    let mut total_downloaded = 0usize;
    let mut api_calls = 0u32;
    let mut tools_changed = false;
    let mut patch_results: BTreeMap<String, Result<String, String>> = BTreeMap::new();

    if let Some(stats) = ensure_cli_tool(cfg, tools_dir, client).await {
        total_needed += 1;
        api_calls += stats.api_calls;
        if stats.downloaded {
            total_downloaded += 1;
            tools_changed = true;
        }
    }

    if !cfg.cli_repo.is_empty() {
        total_needed += 1;
        if ensure_apkeditor_present(cfg, tools_dir, client).await {
            total_downloaded += 1;
            tools_changed = true;
        }
    }

    if let Some(downloaded_apkeep) = ensure_apkeep_present(cfg, tools_dir, client).await {
        total_needed += 1;
        if downloaded_apkeep {
            total_downloaded += 1;
            tools_changed = true;
        }
    }

    for ((repo, branch), source_alias) in collect_repo_branches(cfg, targets) {
        total_needed += 1;
        let (res, stats) = ensure_patch_bundle(
            cfg,
            tools_dir,
            &repo,
            branch.as_deref(),
            &source_alias,
            client,
        )
        .await;
        api_calls += stats.api_calls;
        if stats.downloaded {
            total_downloaded += 1;
            tools_changed = true;
        }
        patch_results.insert(source_alias, res);
    }

    ui::item(
        ui::Status::Info,
        "tools",
        "",
        &format!("{total_needed} needed, {total_downloaded} downloaded, {api_calls} api calls"),
        None,
    );

    Ok(DownloadToolsSummary {
        tools_changed,
        patch_results,
    })
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
