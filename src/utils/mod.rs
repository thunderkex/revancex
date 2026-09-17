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

pub async fn download_tools(cfg: &Config, targets: Option<&[String]>) -> Result<()> {
    let client = make_github_client(cfg)?;
    let mut version_cache = cache::load_version_cache(&cfg.build.version_cache);
    let mut cache_modified = false;

    let cli_dest = format!("{}/morphe-cli.jar", cfg.build.tools_dir);
    let cli_tag_res = github::get_latest_release(&client, &cfg.cli_repo, None).await;
    let cli_upstream_tag = cli_tag_res.as_ref().map(|r| r.tag_name.clone()).ok();
    let cached_cli_tag = version_cache.patch_source_tags.get("cli").cloned();

    let cli_needs_download = if !is_valid_jar_or_bundle(&cli_dest) {
        true
    } else if let (Some(ref upstream), Some(ref cached)) = (&cli_upstream_tag, &cached_cli_tag) {
        upstream != cached
    } else {
        false
    };

    if cli_needs_download {
        let url = github::resolve_asset_url(&client, &cfg.cli_repo, &cfg.cli_pattern, None)
            .await
            .with_context(|| format!("fetching CLI from {}", cfg.cli_repo))?;
        download_file_with_client(&client, &url, &cli_dest, cfg.build.download_retries, None)
            .await?;
        if let Some(tag) = cli_upstream_tag {
            version_cache
                .patch_source_tags
                .insert("cli".to_string(), tag);
            cache_modified = true;
        }
    } else {
        info!("CLI up to date, skipping: {cli_dest}");
    }

    let apkeditor_dest = format!("{}/APKEditor.jar", cfg.build.tools_dir);
    if !is_valid_jar_or_bundle(&apkeditor_dest) {
        let url =
            "https://github.com/REAndroid/APKEditor/releases/download/V1.4.9/APKEditor-1.4.9.jar";
        if let Err(e) = download_file_with_client(
            &client,
            url,
            &apkeditor_dest,
            cfg.build.download_retries,
            None,
        )
        .await
        {
            tracing::warn!("APKEditor download failed, split-bundle merging unavailable: {e}");
        }
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
        let primary_dest = versioned_dest.as_ref().unwrap_or(&dest);

        let upstream_rel = github::get_latest_release(&client, &repo, branch.as_deref())
            .await
            .ok();
        let upstream_tag = upstream_rel.as_ref().map(|r| r.tag_name.clone());
        let cached_tag = version_cache.patch_source_tags.get(&repo).cloned();

        let needs_download = if !is_valid_jar_or_bundle(primary_dest) {
            true
        } else if let (Some(ref up_tag), Some(ref c_tag)) = (&upstream_tag, &cached_tag) {
            up_tag != c_tag
        } else {
            false
        };

        if !needs_download {
            if let Some(ref vdest) = versioned_dest {
                if !is_valid_jar_or_bundle(&dest) {
                    let _ = std::fs::copy(vdest, &dest);
                }
            }
            info!("Patches up to date, skipping: {primary_dest}");
            continue;
        }

        match github::resolve_asset_url(&client, &repo, "*.mpp", branch.as_deref()).await {
            Ok(url) => {
                if let Some(ref vdest) = versioned_dest {
                    download_file_with_client(
                        &client,
                        &url,
                        vdest,
                        cfg.build.download_retries,
                        None,
                    )
                    .await?;
                    let _ = std::fs::copy(vdest, &dest);
                } else {
                    download_file_with_client(
                        &client,
                        &url,
                        &dest,
                        cfg.build.download_retries,
                        None,
                    )
                    .await?;
                }
                if let Some(tag) = upstream_tag {
                    version_cache.patch_source_tags.insert(repo.clone(), tag);
                    cache_modified = true;
                }
            }
            Err(e) => {
                tracing::warn!("Could not download patches from {repo}: {e}");
            }
        }
    }

    if cache_modified {
        cache::save_version_cache(&cfg.build.version_cache, &version_cache);
    }

    Ok(())
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
