use crate::builder;
use crate::config::{AppConfig, Config};
use crate::patcher;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

pub struct BuildSummary {
    pub built: Vec<String>,
    pub failed: Vec<(String, String)>, // (app_id, error_message)
}

pub async fn run_pipeline(
    cfg: &Config,
    targets: &[String],
    arch: &str,
    mode: Option<&str>,
    force: bool,
    output_dir: &str,
) -> Result<BuildSummary> {
    let semaphore = Arc::new(Semaphore::new(cfg.build.workers));
    let cfg_arc = Arc::new(cfg.clone());
    let uptodown_cb = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let mut handles: Vec<(String, _)> = Vec::new();

    for id in targets {
        if let Some(app) = cfg.apps.get(id) {
            let id_key = id.clone();
            let id = id.clone();
            let app = app.clone();
            let cfg = cfg_arc.clone();
            let arch = arch.to_string();
            let cli_mode = mode.map(|m| m.to_string());
            let output_dir = output_dir.to_string();
            let sem = semaphore.clone();
            let cb = uptodown_cb.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                build_single(
                    &cfg,
                    &id,
                    &app,
                    &arch,
                    cli_mode.as_deref(),
                    force,
                    &output_dir,
                    Some(cb),
                )
                .await
            });
            handles.push((id_key, handle));
        }
    }

    let mut built = Vec::new();
    let mut failed = Vec::new();
    for (id, handle) in handles {
        match handle.await {
            Ok(Ok(path)) => built.push(path),
            Ok(Err(e)) => {
                warn!("Build failed for {id}: {e}");
                failed.push((id.clone(), e.to_string()));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk.idsig"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk.idsig"));
            }
            Err(e) => {
                warn!("Task panicked for {id}: {e}");
                failed.push((id.clone(), format!("task panicked: {e}")));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk.idsig"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk.idsig"));
            }
        }
    }

    Ok(BuildSummary { built, failed })
}

#[allow(clippy::too_many_arguments)]
async fn build_single(
    cfg: &Config,
    id: &str,
    app: &AppConfig,
    arch: &str,
    cli_mode: Option<&str>,
    force: bool,
    output_dir: &str,
    uptodown_cb: Option<Arc<std::sync::atomic::AtomicU32>>,
) -> Result<String> {
    let effective_mode = match cli_mode {
        Some(m) if !m.is_empty() => m,
        _ => {
            if !app.mode.is_empty() {
                if app.mode == "patch" {
                    "full"
                } else {
                    app.mode.as_str()
                }
            } else {
                "full"
            }
        }
    };

    info!(
        "Processing {id} (mode={effective_mode}, patch_source={})...",
        app.patch_source
    );

    if effective_mode != "install" && app.patch_source != "none" && !app.patch_source.is_empty() {
        let mpp_path = crate::patcher::metadata::resolve_mpp_path(cfg, app);
        if let Some(ref p) = mpp_path {
            if let Ok(meta) = crate::patcher::metadata::load(cfg, p).await {
                if let Some(err) =
                    crate::patcher::metadata::check_app_version_compatibility(id, app, &meta)
                {
                    let patch_supported =
                        crate::builder::resolve_all_patch_compatible_versions(cfg, app).await;
                    if app.version_pin_strict || patch_supported.is_empty() {
                        anyhow::bail!("{id}: {err}");
                    } else {
                        tracing::warn!("{id}: {err} (auto-correcting to compatible version)");
                    }
                }
            }
        }
    }

    let raw_apk = builder::fetch_apk(cfg, id, app, arch, force, uptodown_cb).await?;

    // Check APK integrity after download
    if !crate::utils::apk::check_apk_integrity(&raw_apk) {
        anyhow::bail!("{id}: downloaded APK failed integrity check");
    }

    if effective_mode == "install" || app.patch_source == "none" || app.patch_source.is_empty() {
        let dest = format!("{output_dir}/{id}.apk");
        std::fs::copy(&raw_apk, &dest)?;
        info!("{id}: copied unpatched official APK to {dest}");
        return Ok(dest);
    }

    let apk_path = if arch != "all" {
        let work_apk = format!("{}/{id}-{arch}-work.apk", cfg.build.temp_dir);
        if let Err(e) = builder::arch::strip_unsupported_archs_to(&raw_apk, &work_apk, arch) {
            warn!("{id}: arch stripping note: {e}");
            raw_apk
        } else {
            builder::arch::verify_arch_native_libs(&raw_apk, &work_apk, arch)?;
            work_apk
        }
    } else {
        raw_apk
    };

    if effective_mode == "both" {
        let non_root = patcher::patch(cfg, id, app, &apk_path, output_dir, false).await?;
        patcher::patch(cfg, id, app, &apk_path, output_dir, true).await?;
        Ok(non_root)
    } else {
        let is_root = effective_mode == "module" || effective_mode == "root";
        let patched = patcher::patch(cfg, id, app, &apk_path, output_dir, is_root).await?;
        if effective_mode == "lite" {
            if let Err(e) = builder::lite::optimize_apk(&patched) {
                warn!("{id}: lite optimization note: {e}");
            }
        }
        Ok(patched)
    }
}
