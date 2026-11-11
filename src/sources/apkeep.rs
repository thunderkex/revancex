use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use crate::sources::common::{
    extract_base_apk_from_zip, is_split_bundle, is_zip_file, merge_split_apk_bundle,
    strip_split_requirements_from_apk,
};
use anyhow::{Context, Result};
use reqwest::Client;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::info;

pub struct ApkeepSource;

impl ApkSource for ApkeepSource {
    fn name(&self) -> &'static str {
        "apkeep"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if app.package.is_empty() {
                tracing::info!("apkeep skipped: not configured");
                return Ok(None);
            }
            let apkeep_ack = std::env::var("REVANCEX_APKEEP_ACKNOWLEDGE_DANGERS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or_else(|_| {
                    ctx.cfg
                        .apk_sources
                        .get("apkeep")
                        .map(|e| e.acknowledge_dangers)
                        .unwrap_or(false)
                });
            if !apkeep_ack {
                tracing::info!("apkeep skipped: acknowledge_dangers is false");
                return Ok(None);
            }
            let pseudo_url = format!("apkeep://{}/{}", app.package, version.unwrap_or("latest"));
            Ok(Some(Resolved::simple(
                pseudo_url,
                version.map(str::to_string),
                arch,
            )))
        })
    }

    fn fetch<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        resolved: &'a Resolved,
        dest: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            let version = resolved.version_found.as_deref();
            let pkg = resolved
                .url
                .strip_prefix("apkeep://")
                .and_then(|s| s.split('/').next())
                .unwrap_or("");
            let apkeep_ack = std::env::var("REVANCEX_APKEEP_ACKNOWLEDGE_DANGERS")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or_else(|_| {
                    ctx.cfg
                        .apk_sources
                        .get("apkeep")
                        .map(|e| e.acknowledge_dangers)
                        .unwrap_or(false)
                });
            fetch_apkeep(
                ctx.client,
                pkg,
                dest,
                version,
                Some(&resolved.arch),
                apkeep_ack,
            )
            .await?;
            Ok(PathBuf::from(dest))
        })
    }
}

fn resolve_apkeep_cmd() -> String {
    if crate::utils::is_command_available("apkeep") {
        return "apkeep".to_string();
    }
    let local = if cfg!(windows) {
        "tools/apkeep.exe"
    } else {
        "tools/apkeep"
    };
    if std::path::Path::new(local).exists() {
        return local.to_string();
    }
    "apkeep".to_string()
}

pub(crate) async fn fetch_apkeep(
    client: &Client,
    package: &str,
    dest: &str,
    target_version: Option<&str>,
    arch: Option<&str>,
    acknowledge_dangers: bool,
) -> Result<String> {
    let tmp_dir = tempfile::tempdir()?;
    let out_dir = tmp_dir.path().to_str().unwrap();
    let apkeep_cmd = resolve_apkeep_cmd();

    let opt_arg = if let Some(a) = arch.filter(|a| !a.is_empty() && *a != "all") {
        format!("acknowledge_dangers=true,arch={a}")
    } else {
        "acknowledge_dangers=true".to_string()
    };

    if let Some(ver) = target_version {
        let app_arg = format!("{package}@{ver}");
        info!("apkeep: attempting targeted download {app_arg}");
        let mut cmd = tokio::process::Command::new(&apkeep_cmd);
        cmd.args(["-a", &app_arg, "-d", "apk-pure"]);
        if acknowledge_dangers {
            cmd.args(["-o", &opt_arg]);
        }
        cmd.arg(out_dir);
        let res = cmd.status().await;
        match res {
            Ok(st) if st.success() => {}
            Ok(st) => anyhow::bail!("apkeep: targeted download for {app_arg} failed (exit {st})"),
            Err(e) => anyhow::bail!("apkeep: targeted download for {app_arg} failed to run: {e}"),
        }
    } else {
        info!("apkeep: attempting download for {package} (latest)");
        let mut cmd = tokio::process::Command::new(&apkeep_cmd);
        cmd.args(["-a", package, "-d", "apk-pure"]);
        if acknowledge_dangers {
            cmd.args(["-o", &opt_arg]);
        }
        cmd.arg(out_dir);
        let status = cmd.status().await.context("Running apkeep")?;
        if !status.success() {
            anyhow::bail!("apkeep exited with status {status}");
        }
    }

    let mut rd = tokio::fs::read_dir(out_dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        let p = entry.path();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let p_str = p.to_string_lossy().to_string();
        let is_bundle = {
            let ps = p_str.clone();
            tokio::task::spawn_blocking(move || is_zip_file(&ps) || is_split_bundle(&ps))
                .await
                .unwrap_or(false)
        };
        if ext == "xapk" || ext == "apkm" || is_bundle {
            info!("apkeep: processing bundle file {p_str}");
            let is_split = {
                let ps = p_str.clone();
                tokio::task::spawn_blocking(move || is_split_bundle(&ps))
                    .await
                    .unwrap_or(false)
            };
            if is_split {
                if let Ok(()) = merge_split_apk_bundle(client, &p_str, dest).await {
                    let d = dest.to_string();
                    let _ =
                        tokio::task::spawn_blocking(move || strip_split_requirements_from_apk(&d))
                            .await;
                    return Ok(dest.to_string());
                } else {
                    tracing::warn!(
                        "apkeep: merge_split_apk_bundle failed, attempting base APK extraction"
                    );
                }
            }
            let ps = p_str.clone();
            let d = dest.to_string();
            let extracted = tokio::task::spawn_blocking(move || {
                extract_base_apk_from_zip(&ps, &d)?;
                let _ = strip_split_requirements_from_apk(&d);
                Ok::<(), anyhow::Error>(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("extract task panicked: {e}"))?;
            if extracted.is_ok() {
                return Ok(dest.to_string());
            }
        }
        if ext == "apk" {
            tokio::fs::copy(&p, dest).await?;
            let d = dest.to_string();
            let _ =
                tokio::task::spawn_blocking(move || strip_split_requirements_from_apk(&d)).await;
            return Ok(dest.to_string());
        }
    }

    anyhow::bail!("apkeep finished but produced no valid .apk or .xapk in {out_dir}")
}
