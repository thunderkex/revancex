pub mod apk;
pub mod cache;
pub mod doctor;
pub mod github;
pub mod keystore;
pub mod semver;
pub mod zip;

use crate::config::Config;
use crate::orchestrator::BuildSummary;
use anyhow::{Context, Result};
use reqwest::Client;
use std::path::Path;
use tracing::info;

pub async fn download_tools(cfg: &Config, targets: Option<&[String]>) -> Result<()> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(
            cfg.build.download_timeout_secs,
        ))
        .user_agent("revancex-builder/666")
        .build()?;

    let cli_dest = format!("{}/morphe-cli.jar", cfg.build.tools_dir);
    if !Path::new(&cli_dest).exists() {
        let url = github::resolve_asset_url(&client, &cfg.cli_repo, &cfg.cli_pattern, None)
            .await
            .with_context(|| format!("fetching CLI from {}", cfg.cli_repo))?;
        download_file_with_client(&client, &url, &cli_dest, cfg.build.download_retries).await?;
    }

    let needs_apkeditor = targets.map_or(true, |tlist| {
        tlist.iter().any(|id| {
            cfg.apps
                .get(id)
                .is_some_and(|a| a.mode == "lite" || a.mode == "both" || a.architectures.len() > 1)
        })
    });

    let apkeditor_dest = format!("{}/APKEditor.jar", cfg.build.tools_dir);
    if needs_apkeditor && !Path::new(&apkeditor_dest).exists() {
        let url =
            "https://github.com/REAndroid/APKEditor/releases/download/V1.4.9/APKEditor-1.4.9.jar";
        let _ =
            download_file_with_client(&client, url, &apkeditor_dest, cfg.build.download_retries)
                .await;
    } else if !needs_apkeditor {
        info!("APKEditor.jar: not needed for this target set, skipping download");
    }

    let mut repo_branches: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();

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

            let branch = app
                .patches_version
                .clone()
                .or(app.patch_branch.clone())
                .or(source_branch);

            match repo_branches.get(&repo) {
                Some(Some(_)) => {}
                _ => {
                    repo_branches.insert(repo, branch);
                }
            }
        }
    }

    for (repo, branch) in repo_branches {
        let safe_name = repo.replace('/', "_");
        let dest = format!("{}/patches_{safe_name}.mpp", cfg.build.tools_dir);
        let versioned_dest = branch
            .as_ref()
            .map(|b| format!("{}/patches_{safe_name}_{b}.mpp", cfg.build.tools_dir));

        let needs_download = if let Some(ref vdest) = versioned_dest {
            !Path::new(vdest).exists()
        } else {
            !Path::new(&dest).exists()
        };

        if !needs_download {
            info!("Patches exist, skipping: {dest}");
            continue;
        }

        match github::resolve_asset_url(&client, &repo, "*.mpp", branch.as_deref()).await {
            Ok(url) => {
                if let Some(ref vdest) = versioned_dest {
                    download_file_with_client(&client, &url, vdest, cfg.build.download_retries)
                        .await?;
                    let _ = std::fs::copy(vdest, &dest);
                } else {
                    download_file_with_client(&client, &url, &dest, cfg.build.download_retries)
                        .await?;
                }
            }
            Err(e) => {
                tracing::warn!("Could not download patches from {repo}: {e}");
            }
        }
    }

    Ok(())
}

pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub async fn download_file_with_client(
    client: &Client,
    url: &str,
    dest: &str,
    retries: u32,
) -> Result<String> {
    if let Some(parent) = Path::new(dest).parent() {
        std::fs::create_dir_all(parent)?;
    }
    for attempt in 0..=retries {
        match try_download(client, url, dest).await {
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

async fn try_download(client: &Client, url: &str, dest: &str) -> Result<()> {
    let path = Path::new(dest);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download.tmp");

    if is_aria2_available().await {
        let aria_res = tokio::process::Command::new("aria2c")
            .args([
                "-x",
                "16",
                "-s",
                "16",
                "-j",
                "16",
                "-k",
                "1M",
                "-U",
                UA,
                "--file-allocation=none",
                "--allow-overwrite=true",
                "--auto-file-renaming=false",
                "--summary-interval=0",
                "-d",
                dir.to_str().unwrap_or("."),
                "-o",
                filename,
                url,
            ])
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
    let resp = client.get(url).send().await?.error_for_status()?;
    let mut stream = resp.bytes_stream();
    let mut file = std::fs::File::create(dest)?;
    while let Some(chunk) = stream.next().await {
        std::io::Write::write_all(&mut file, &chunk?)?;
    }

    verify_apk_integrity(dest)?;
    Ok(())
}

/// Verify that a downloaded file is not an HTML error page masquerading as an APK.
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
    std::env::var("KEYSTORE_PASSWORD").unwrap_or_else(|_| "revanced".to_string())
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
    let ts = chrono::Utc::now().to_rfc3339();
    let version = env!("CARGO_PKG_VERSION");

    let built: Vec<serde_json::Value> = summary
        .built
        .iter()
        .map(|p| {
            serde_json::json!({
                "status": "success",
                "artifact": p
            })
        })
        .collect();

    let failed: Vec<serde_json::Value> = summary
        .failed
        .iter()
        .map(|(id, err)| {
            serde_json::json!({
                "status": "failed",
                "app_id": id,
                "error": err
            })
        })
        .collect();

    let report = serde_json::json!({
        "build_timestamp": ts,
        "revancex_version": version,
        "summary": {
            "built_count": summary.built.len(),
            "failed_count": summary.failed.len()
        },
        "built": built,
        "failed": failed
    });

    let json = serde_json::to_string_pretty(&report)?;
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
