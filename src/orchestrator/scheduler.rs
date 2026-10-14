use crate::builder;
use crate::config::{AppConfig, Config};
use crate::patcher;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppBuildReport {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_strategy: Option<String>,
    pub version_conflict: Option<Vec<(String, Vec<String>)>>,
    pub patches_included: Vec<String>,
    pub patches_auto_disabled: Vec<crate::patcher::plan::DisabledPatch>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attempted_sources: Vec<SourceAttemptRecord>,
}

impl AppBuildReport {
    pub fn failed(err: impl Into<String>) -> Self {
        Self {
            status: "failed".to_string(),
            target_version: None,
            resolution_strategy: None,
            version_conflict: None,
            patches_included: vec![],
            patches_auto_disabled: vec![],
            warnings: vec![],
            error: Some(err.into()),
            attempted_sources: vec![],
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceAttemptRecord {
    pub source: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub reason: String,
}

pub struct BuildSummary {
    pub built: Vec<String>,
    pub failed: Vec<(String, String)>, // (app_id, error_message)
    pub reports: std::collections::BTreeMap<String, AppBuildReport>,
}

pub async fn run_pipeline(
    cfg: &Config,
    targets: &[String],
    arch: &str,
    mode: Option<&str>,
    force: bool,
    output_dir: &str,
    patch_results: Option<&std::collections::BTreeMap<String, Result<String, String>>>,
) -> Result<BuildSummary> {
    let semaphore = Arc::new(Semaphore::new(cfg.build.workers));
    let cfg_arc = Arc::new(cfg.clone());
    let uptodown_cb = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let mut handles: Vec<(String, _)> = Vec::new();
    let mut built = Vec::new();
    let mut failed = Vec::new();
    let mut reports = std::collections::BTreeMap::new();

    for id in targets {
        if let Some(app) = cfg.apps.get(id) {
            if let Some(res_map) = patch_results {
                if !app.patch_source.is_empty() && app.patch_source != "none" {
                    if let Some(Err(reason)) = res_map.get(&app.patch_source) {
                        let err_msg = format!(
                            "Patch bundle download failed for source '{}': {}",
                            app.patch_source, reason
                        );
                        warn!("Skipping build for {id}: {err_msg}");
                        failed.push((id.clone(), err_msg.clone()));
                        reports.insert(id.clone(), AppBuildReport::failed(err_msg));
                        continue;
                    }
                }
            }

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
    for (id, handle) in handles {
        match handle.await {
            Ok((Some(path), report)) => {
                reports.insert(id.clone(), report);
                built.push(path);
            }
            Ok((None, report)) => {
                let err_str = report
                    .error
                    .clone()
                    .unwrap_or_else(|| "build failed".to_string());
                warn!("Build failed for {id}: {err_str}");
                failed.push((id.clone(), err_str));
                reports.insert(id.clone(), report);
                let _ = std::fs::remove_file(format!("{output_dir}/{id}.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk.idsig"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk.idsig"));
            }
            Err(e) => {
                warn!("Task panicked for {id}: {e}");
                let err_str = format!("task panicked: {e}");
                failed.push((id.clone(), err_str.clone()));
                reports.insert(id.clone(), AppBuildReport::failed(err_str));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk.idsig"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-root.apk.idsig"));
            }
        }
    }

    Ok(BuildSummary {
        built,
        failed,
        reports,
    })
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
) -> (Option<String>, AppBuildReport) {
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

    let patch_supported = crate::builder::resolve_all_patch_compatible_versions(cfg, app).await;
    let version_conflict = match &patch_supported {
        crate::patcher::metadata::CompatibleVersions::Conflict { patch_versions } => {
            Some(patch_versions.clone())
        }
        _ => None,
    };
    let mpp_path = crate::patcher::metadata::resolve_mpp_path(cfg, app);
    let meta = if let Some(ref p) = mpp_path {
        crate::patcher::metadata::load(cfg, p).await.ok()
    } else {
        None
    };
    let (target_ver, _, strategy_str) = crate::builder::resolve_app_version_and_strategy(
        id,
        app,
        meta.as_deref().map(|v| v.as_slice()),
        &patch_supported,
    );

    if effective_mode != "install"
        && app.patch_source != "none"
        && !app.patch_source.is_empty()
        && matches!(
            patch_supported,
            crate::patcher::metadata::CompatibleVersions::Unknown
        )
        && app
            .version
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case("auto"))
            .unwrap_or(true)
        && app.on_unknown_compat.as_deref().unwrap_or("fail") == "fail"
    {
        let msg = format!("{id}: refusing to build latest, patch compatibility unknown");
        warn!("{msg}");
        return (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                target_version: target_ver,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: vec![msg.clone()],
                error: Some(msg),
                attempted_sources: vec![],
            },
        );
    }

    if effective_mode != "install" && app.patch_source != "none" && !app.patch_source.is_empty() {
        if let Some(ref meta_slice) = meta {
            if let Some(err) =
                crate::patcher::metadata::check_app_version_compatibility(id, app, meta_slice)
            {
                if app.version_pin_strict || patch_supported.is_empty() {
                    return (
                        None,
                        AppBuildReport {
                            status: "failed".to_string(),
                            target_version: target_ver,
                            resolution_strategy: Some(strategy_str),
                            version_conflict,
                            patches_included: vec![],
                            patches_auto_disabled: vec![],
                            warnings: vec![err.clone()],
                            error: Some(format!("{id}: {err}")),
                            attempted_sources: vec![],
                        },
                    );
                } else {
                    tracing::warn!("{id}: {err} (auto-correcting to compatible version)");
                }
            }
        }
    }

    let raw_apk = match builder::fetch_apk(cfg, id, app, arch, force, uptodown_cb).await {
        Ok(a) => a,
        Err(e) => {
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    target_version: target_ver,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    patches_included: vec![],
                    patches_auto_disabled: vec![],
                    warnings: vec![],
                    error: Some(format!("{id}: fetch APK failed: {e}")),
                    attempted_sources: vec![],
                },
            );
        }
    };

    // Check APK integrity after download
    if !crate::utils::apk::check_apk_integrity(&raw_apk) {
        return (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                target_version: target_ver,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: vec![],
                error: Some(format!("{id}: downloaded APK failed integrity check")),
                attempted_sources: vec![],
            },
        );
    }

    if effective_mode == "install" || app.patch_source == "none" || app.patch_source.is_empty() {
        let dest = format!("{output_dir}/{id}.apk");
        if let Err(e) = std::fs::copy(&raw_apk, &dest) {
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    target_version: target_ver,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    patches_included: vec![],
                    patches_auto_disabled: vec![],
                    warnings: vec![],
                    error: Some(e.to_string()),
                    attempted_sources: vec![],
                },
            );
        }
        info!("{id}: copied unpatched official APK to {dest}");
        return (
            Some(dest),
            AppBuildReport {
                status: "success".to_string(),
                target_version: target_ver,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: vec![],
                error: None,
                attempted_sources: vec![],
            },
        );
    }

    let apk_path = if arch != "all" && arch != "universal" {
        let work_apk = format!("{}/{id}-{arch}-work.apk", cfg.build.temp_dir);
        if let Ok(false) = builder::arch::apk_has_arch(&raw_apk, arch) {
            info!(
                "{id}: input APK has no native libs for '{arch}', using unstripped universal APK"
            );
            raw_apk
        } else if let Err(e) = builder::arch::strip_unsupported_archs_to(&raw_apk, &work_apk, arch)
        {
            warn!("{id}: arch stripping note: {e}");
            raw_apk
        } else if let Err(e) = builder::arch::verify_arch_native_libs(&raw_apk, &work_apk, arch) {
            warn!("{id}: arch verification note: {e}; falling back to universal unstripped APK");
            raw_apk
        } else {
            work_apk
        }
    } else {
        raw_apk
    };

    let patch_res = if effective_mode == "both" {
        match patcher::patch(cfg, id, app, &apk_path, output_dir, false).await {
            Ok((non_root, plan)) => {
                let _ = patcher::patch(cfg, id, app, &apk_path, output_dir, true).await;
                Ok((non_root, plan))
            }
            Err(e) => Err(e),
        }
    } else {
        let is_root = effective_mode == "module" || effective_mode == "root";
        match patcher::patch(cfg, id, app, &apk_path, output_dir, is_root).await {
            Ok((patched, plan)) => {
                if effective_mode == "lite" {
                    if let Err(e) = builder::lite::optimize_apk(&patched) {
                        warn!("{id}: lite optimization note: {e}");
                    }
                }
                Ok((patched, plan))
            }
            Err(e) => Err(e),
        }
    };

    match patch_res {
        Ok((out_path, plan)) => (
            Some(out_path),
            AppBuildReport {
                status: "success".to_string(),
                target_version: target_ver,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: plan.included,
                patches_auto_disabled: plan.auto_disabled,
                warnings: plan.warnings,
                error: None,
                attempted_sources: vec![],
            },
        ),
        Err(e) => (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                target_version: target_ver,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: vec![],
                error: Some(e.to_string()),
                attempted_sources: vec![],
            },
        ),
    }
}
