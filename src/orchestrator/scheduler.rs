use crate::builder;
use crate::config::{AppConfig, Config};
use crate::patcher;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn};

pub use crate::report::{AppBuildReport, FailureCategory, SourceAttemptRecord};

pub struct BuildSummary {
    pub built: Vec<String>,
    pub failed: Vec<(String, String)>, // (app_id, error_message)
    pub reports: std::collections::BTreeMap<String, AppBuildReport>,
    pub open_breakers: Vec<String>,
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
    let breaker_registry = Arc::new(crate::sources::breaker::BreakerRegistry::new(
        &cfg.circuit_breakers,
    ));
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
            let breaker_reg = breaker_registry.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let res = build_single(
                    &cfg,
                    &id,
                    &app,
                    &arch,
                    cli_mode.as_deref(),
                    force,
                    &output_dir,
                    Some(cb),
                    Some(breaker_reg.clone()),
                )
                .await;
                breaker_reg.record_app_completed();
                res
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

    let open_breakers = breaker_registry.open_breakers();
    Ok(BuildSummary {
        built,
        failed,
        reports,
        open_breakers,
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
    breaker_registry: Option<Arc<crate::sources::breaker::BreakerRegistry>>,
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
    let patch_supported_list = patch_supported.to_vec();
    let target_version_reason = if target_ver.is_some() {
        None
    } else if effective_mode == "install"
        || app.patch_source == "none"
        || app.patch_source.is_empty()
    {
        Some("install mode (no patches)".to_string())
    } else if patch_supported_list.is_empty() {
        Some("unconstrained / universal (no version constraints in patch metadata)".to_string())
    } else {
        Some("no compatible target version resolved".to_string())
    };

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
                target_version: target_ver.clone(),
                target_version_reason: target_version_reason.clone(),
                patch_supported_versions: patch_supported_list,
                fetched_version: None,
                resolved_version: target_ver,
                resolved_source: None,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: vec![msg.clone()],
                error: Some(msg),
                failure_category: Some(FailureCategory::CompatibilityUnknown),
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
                    let err_msg = format!("{id}: {err}");
                    return (
                        None,
                        AppBuildReport {
                            status: "failed".to_string(),
                            target_version: target_ver.clone(),
                            target_version_reason: target_version_reason.clone(),
                            patch_supported_versions: patch_supported_list,
                            fetched_version: None,
                            resolved_version: target_ver,
                            resolved_source: None,
                            resolution_strategy: Some(strategy_str),
                            version_conflict,
                            patches_included: vec![],
                            patches_auto_disabled: vec![],
                            warnings: vec![err.clone()],
                            error: Some(err_msg.clone()),
                            failure_category: Some(FailureCategory::from_error_message(&err_msg)),
                            attempted_sources: vec![],
                        },
                    );
                } else {
                    tracing::warn!("{id}: {err} (auto-correcting to compatible version)");
                }
            }
        }
    }

    let (raw_apk, fetch_attempts, fetched_version, fetch_warnings) =
        match builder::fetch_apk(cfg, id, app, arch, force, uptodown_cb, breaker_registry).await {
            Ok(succ) => (succ.path, succ.attempts, succ.final_version, succ.warnings),
            Err(e) => {
                let (attempts, warnings) =
                    if let Some(fetch_err) = e.downcast_ref::<builder::FetchApkError>() {
                        (fetch_err.attempts.clone(), fetch_err.warnings.clone())
                    } else {
                        (vec![], vec![])
                    };
                let err_msg = format!("{id}: fetch APK failed: {e}");
                let cat = FailureCategory::from_error_message(&err_msg);
                return (
                    None,
                    AppBuildReport {
                        status: "failed".to_string(),
                        target_version: target_ver.clone(),
                        target_version_reason: target_version_reason.clone(),
                        patch_supported_versions: patch_supported_list,
                        fetched_version: None,
                        resolved_version: target_ver,
                        resolved_source: None,
                        resolution_strategy: Some(strategy_str),
                        version_conflict,
                        patches_included: vec![],
                        patches_auto_disabled: vec![],
                        warnings,
                        error: Some(err_msg),
                        failure_category: Some(cat),
                        attempted_sources: attempts,
                    },
                );
            }
        };

    let resolved_source = fetch_attempts
        .iter()
        .find(|a| a.outcome == "hit")
        .map(|a| a.source.clone())
        .or_else(|| {
            if app.apk_url.is_some() {
                Some("direct_url".to_string())
            } else {
                None
            }
        });
    let resolved_ver = fetched_version.clone().or_else(|| target_ver.clone());
    let effective_target_ver = fetched_version.clone().or(target_ver);

    // Check APK integrity after download
    if let Err(integrity_err) = crate::utils::apk::verify_apk_integrity(&raw_apk) {
        warn!("{id}: downloaded APK failed integrity check: {integrity_err}");
        let err_msg = format!("{id}: downloaded APK failed integrity check: {integrity_err}");
        return (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                target_version: effective_target_ver,
                target_version_reason: target_version_reason.clone(),
                patch_supported_versions: patch_supported_list,
                fetched_version: fetched_version.clone(),
                resolved_version: resolved_ver,
                resolved_source,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: vec![],
                error: Some(err_msg),
                failure_category: Some(FailureCategory::IntegrityCheckFailed),
                attempted_sources: fetch_attempts,
            },
        );
    }

    if effective_mode == "install" || app.patch_source == "none" || app.patch_source.is_empty() {
        let dest = format!("{output_dir}/{id}.apk");
        if let Err(e) = std::fs::copy(&raw_apk, &dest) {
            let err_msg = e.to_string();
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    target_version: effective_target_ver,
                    target_version_reason: target_version_reason.clone(),
                    patch_supported_versions: vec![],
                    fetched_version: fetched_version.clone(),
                    resolved_version: resolved_ver,
                    resolved_source,
                    resolution_strategy: Some("install_mode".to_string()),
                    version_conflict,
                    patches_included: vec![],
                    patches_auto_disabled: vec![],
                    warnings: vec![],
                    error: Some(err_msg.clone()),
                    failure_category: Some(FailureCategory::from_error_message(&err_msg)),
                    attempted_sources: fetch_attempts,
                },
            );
        }
        info!("{id}: copied unpatched official APK to {dest}");
        return (
            Some(dest),
            AppBuildReport {
                status: "success".to_string(),
                target_version: effective_target_ver,
                target_version_reason: Some("install mode (no patches)".to_string()),
                patch_supported_versions: vec![],
                fetched_version: fetched_version.clone(),
                resolved_version: resolved_ver,
                resolved_source,
                resolution_strategy: Some("install_mode".to_string()),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: fetch_warnings.clone(),
                error: None,
                failure_category: None,
                attempted_sources: fetch_attempts,
            },
        );
    }

    let apk_path = if arch != "all" && arch != "universal" {
        let work_apk = format!("{}/{id}-{arch}-work.apk", cfg.build.temp_dir);
        if let Ok(false) = builder::arch::apk_has_arch(&raw_apk, arch) {
            if builder::arch::apk_has_any_native_libs(&raw_apk).unwrap_or(false) {
                let err_msg = format!("{id}: input APK has native libraries for foreign architectures but none for '{arch}'");
                tracing::error!("{err_msg}");
                return (
                    None,
                    AppBuildReport {
                        status: "failed".to_string(),
                        target_version: effective_target_ver,
                        target_version_reason: target_version_reason.clone(),
                        patch_supported_versions: patch_supported_list,
                        fetched_version: fetched_version.clone(),
                        resolved_version: resolved_ver,
                        resolved_source,
                        resolution_strategy: Some(strategy_str),
                        version_conflict,
                        patches_included: vec![],
                        patches_auto_disabled: vec![],
                        warnings: fetch_warnings.clone(),
                        error: Some(err_msg),
                        failure_category: Some(FailureCategory::ArchUnsupported),
                        attempted_sources: fetch_attempts,
                    },
                );
            }
            info!(
                "{id}: input APK has no native libs for '{arch}', using unstripped universal APK"
            );
            raw_apk
        } else if let Err(e) = builder::arch::strip_unsupported_archs_to(&raw_apk, &work_apk, arch)
        {
            warn!("{id}: arch stripping note: {e}");
            raw_apk
        } else if let Err(e) = builder::arch::verify_arch_native_libs(&raw_apk, &work_apk, arch) {
            let err_msg = format!("{id}: arch verification failed: {e}");
            tracing::error!("{err_msg}");
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    target_version: effective_target_ver,
                    target_version_reason: target_version_reason.clone(),
                    patch_supported_versions: patch_supported_list,
                    fetched_version: fetched_version.clone(),
                    resolved_version: resolved_ver,
                    resolved_source,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    patches_included: vec![],
                    patches_auto_disabled: vec![],
                    warnings: fetch_warnings.clone(),
                    error: Some(err_msg),
                    failure_category: Some(FailureCategory::ArchUnsupported),
                    attempted_sources: fetch_attempts,
                },
            );
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
        Ok((out_path, plan)) => {
            let mut warnings = plan.warnings;
            warnings.extend(fetch_warnings.clone());
            let status = if effective_mode != "install"
                && app.patch_source != "none"
                && !app.patch_source.is_empty()
                && plan.included.is_empty()
            {
                warnings.push(format!("{id}: app configured with mode='patch' but 0 patches were applied; output APK is signed stock"));
                "success_unpatched".to_string()
            } else {
                "success".to_string()
            };
            (
                Some(out_path),
                AppBuildReport {
                    status,
                    target_version: effective_target_ver,
                    target_version_reason: target_version_reason.clone(),
                    patch_supported_versions: patch_supported_list,
                    fetched_version: fetched_version.clone(),
                    resolved_version: resolved_ver,
                    resolved_source,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    patches_included: plan.included,
                    patches_auto_disabled: plan.auto_disabled,
                    warnings,
                    error: None,
                    failure_category: None,
                    attempted_sources: fetch_attempts,
                },
            )
        }
        Err(e) => {
            let err_msg = e.to_string();
            let cat = FailureCategory::from_error_message(&err_msg);
            (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    target_version: effective_target_ver,
                    target_version_reason: target_version_reason.clone(),
                    patch_supported_versions: patch_supported_list,
                    fetched_version: fetched_version.clone(),
                    resolved_version: resolved_ver,
                    resolved_source,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    patches_included: vec![],
                    patches_auto_disabled: vec![],
                    warnings: fetch_warnings,
                    error: Some(err_msg),
                    failure_category: Some(cat),
                    attempted_sources: fetch_attempts,
                },
            )
        }
    }
}
