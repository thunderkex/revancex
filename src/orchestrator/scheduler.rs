use crate::builder;
use crate::config::{AppConfig, Config};
use crate::patcher;
use crate::ui;
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
    pub skipped_unconfigured: Vec<String>,
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
                        ui::item_fail(id, &app.display_name(id), "", &err_msg, None);
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

            ui::register_item(&id);
            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let started = std::time::Instant::now();
                let budget_secs = app.timeout_secs.unwrap_or(cfg.build.app_timeout_secs);
                let app_fut = build_single(
                    &cfg,
                    &id,
                    &app,
                    &arch,
                    cli_mode.as_deref(),
                    force,
                    &output_dir,
                    Some(cb),
                    Some(breaker_reg.clone()),
                );
                let res = match tokio::time::timeout(
                    std::time::Duration::from_secs(budget_secs),
                    app_fut,
                )
                .await
                {
                    Ok(r) => r,
                    Err(_) => {
                        let err_msg =
                            format!("{id}: per-app time budget exceeded ({budget_secs}s)");
                        tracing::error!("{err_msg}");
                        (
                            None,
                            AppBuildReport {
                                status: "failed".to_string(),
                                target_version: app.version.clone(),
                                target_version_reason: Some(format!(
                                    "time budget exceeded ({budget_secs}s)"
                                )),
                                error: Some(err_msg.clone()),
                                failure_category: Some(FailureCategory::Other),
                                warnings: vec![err_msg],
                                attempted_sources: vec![],
                                ..Default::default()
                            },
                        )
                    }
                };
                breaker_reg.record_app_completed();
                let elapsed = started.elapsed();
                let mut res = res;
                res.1.total_duration_ms = elapsed.as_millis() as u64;
                ui::record_duration(&id, elapsed);
                report_item(&id, &app, &res.0, &res.1, elapsed);
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
                info!("Build failed for {id}: {err_str}");
                failed.push((id.clone(), err_str));
                if let Some(ref path) = report.output_path {
                    let _ = std::fs::remove_file(path);
                    let _ = std::fs::remove_file(format!("{path}.idsig"));
                }
                let _ = std::fs::remove_file(format!("{output_dir}/{id}.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk.idsig"));
                reports.insert(id.clone(), report);
            }
            Err(e) => {
                let err_str = format!("task panicked: {e}");
                ui::item_fail(&id, &id, "", &err_str, None);
                failed.push((id.clone(), err_str.clone()));
                reports.insert(id.clone(), AppBuildReport::failed(err_str));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk"));
                let _ = std::fs::remove_file(format!("{output_dir}/{id}-patched.apk.idsig"));
            }
        }
    }

    let open_breakers = breaker_registry.open_breakers();
    let unconfigured_candidates = ["github", "gitlab", "codeberg", "apkeep"];
    let mut skipped_unconfigured = Vec::new();
    for s in unconfigured_candidates {
        let any_configured =
            cfg.apps.values().any(|a| a.sources.contains_key(s)) || cfg.apk_sources.contains_key(s);
        if !any_configured {
            skipped_unconfigured.push(s.to_string());
        }
    }

    Ok(BuildSummary {
        built,
        failed,
        reports,
        open_breakers,
        skipped_unconfigured,
    })
}

/// One result line per app, printed as soon as the app finishes.
fn report_item(
    id: &str,
    app: &AppConfig,
    path: &Option<String>,
    report: &AppBuildReport,
    elapsed: std::time::Duration,
) {
    let name = app.display_name(id);
    let version = report
        .resolved_version
        .clone()
        .or_else(|| report.fetched_version.clone())
        .or_else(|| report.target_version.clone())
        .unwrap_or_default();
    if path.is_some()
        && matches!(
            report.status.as_str(),
            "success" | "degraded" | "success_unpatched"
        )
    {
        let detail = format!(
            "patches: {} applied, {} skipped",
            report.patches_included.len(),
            report.patches_auto_disabled.len()
        );
        match report.status.as_str() {
            "success" => ui::item_ok(&name, &version, &detail, Some(elapsed)),
            other => {
                let tag = if other == "degraded" {
                    "degraded"
                } else {
                    "unpatched"
                };
                ui::item(
                    ui::Status::Warn,
                    &name,
                    &version,
                    &format!("{detail} ({tag})"),
                    Some(elapsed),
                );
                for w in &report.warnings {
                    ui::warn_once(w);
                }
            }
        }
    } else {
        let err = report.error.as_deref().unwrap_or("build failed");
        ui::item_fail(id, &name, &version, err, Some(elapsed));
        if app.allow_build_failure {
            ui::warn_once(&format!(
                "{id}: build failed but allow_build_failure is set"
            ));
        }
    }
}

pub(crate) fn normalize_optimization_mode(effective_mode: &str) -> &'static str {
    if effective_mode.eq_ignore_ascii_case("lite") {
        "lite"
    } else {
        "full"
    }
}

fn detect_supported_archs(apk_path: &str, arch_mode: &str) -> Vec<String> {
    const CATALOG_ARCHS: &[&str] = &["arm64-v8a", "armeabi-v7a", "x86_64"];
    if !builder::arch::apk_has_any_native_libs(apk_path).unwrap_or(true) {
        return vec!["all".to_string()];
    }
    let detected: Vec<String> = CATALOG_ARCHS
        .iter()
        .filter(|&&a| builder::arch::apk_has_arch(apk_path, a).unwrap_or(false))
        .map(|&a| a.to_string())
        .collect();
    if !detected.is_empty() {
        return detected;
    }
    if CATALOG_ARCHS.contains(&arch_mode) {
        vec![arch_mode.to_string()]
    } else {
        vec!["all".to_string()]
    }
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
                ..Default::default()
            },
        );
    }

    let pin_val = app
        .version
        .as_deref()
        .filter(|v| !v.eq_ignore_ascii_case("auto"));
    let is_pin_unsupported = pin_val.is_some_and(|pin| {
        !patch_supported_list.is_empty()
            && !patch_supported_list
                .iter()
                .any(|v| v == pin || crate::utils::version::versions_match(v, pin))
    });

    if effective_mode != "install" && app.patch_source != "none" && !app.patch_source.is_empty() {
        let explicit_enabled: Vec<String> = app
            .included_patches
            .iter()
            .chain(app.patches.iter())
            .cloned()
            .collect();
        if let Some(ref meta_slice) = meta {
            if !meta_slice.is_empty()
                && crate::patcher::metadata::count_applicable_patches(
                    meta_slice,
                    &app.package,
                    &explicit_enabled,
                    &app.excluded_patches,
                ) == 0
            {
                let err_msg = format!(
                    "{id}: 0 enabled patches applicable for '{}' (mode='{effective_mode}')",
                    app.package
                );
                return (
                    None,
                    AppBuildReport {
                        status: "failed".to_string(),
                        target_version: target_ver.clone(),
                        target_version_reason: Some("no enabled patches applicable".to_string()),
                        patch_supported_versions: patch_supported_list,
                        fetched_version: None,
                        resolved_version: target_ver,
                        resolved_source: None,
                        resolution_strategy: Some(strategy_str),
                        version_conflict,
                        patches_included: vec![],
                        patches_auto_disabled: vec![],
                        patches_auto_disabled_count: 0,
                        critical_patches_disabled: vec![],
                        warnings: vec![err_msg.clone()],
                        error: Some(err_msg),
                        failure_category: Some(FailureCategory::NoEnabledPatches),
                        attempted_sources: vec![],
                        ..Default::default()
                    },
                );
            }
        }

        if matches!(
            patch_supported,
            crate::patcher::metadata::CompatibleVersions::Intersection(ref v) if v.is_empty()
        ) && target_ver.is_none()
        {
            let err_msg = format!(
                "{id}: version not supported — no enabled patches support package '{}' or compatible version set is empty",
                app.package
            );
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    target_version: target_ver.clone(),
                    target_version_reason: Some("no compatible version resolved".to_string()),
                    patch_supported_versions: patch_supported_list,
                    fetched_version: None,
                    resolved_version: None,
                    resolved_source: None,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    patches_included: vec![],
                    patches_auto_disabled: vec![],
                    patches_auto_disabled_count: 0,
                    critical_patches_disabled: vec![],
                    warnings: vec![err_msg.clone()],
                    error: Some(err_msg),
                    failure_category: Some(FailureCategory::VersionNotSupported),
                    attempted_sources: vec![],
                    ..Default::default()
                },
            );
        }

        if app.effective_pin_policy() == crate::config::apps::PinPolicy::Strict
            && is_pin_unsupported
        {
            let pin = pin_val.unwrap();
            let err_msg = format!(
                "{id}: pinned version v{pin} is not supported by patches and pin_policy is strict"
            );
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
                    patches_auto_disabled_count: 0,
                    critical_patches_disabled: vec![],
                    warnings: vec![err_msg.clone()],
                    error: Some(err_msg),
                    failure_category: Some(FailureCategory::VersionNotSupported),
                    attempted_sources: vec![],
                    ..Default::default()
                },
            );
        }

        if let Some(ref meta_slice) = meta {
            if let Some(err) =
                crate::patcher::metadata::check_app_version_compatibility(id, app, meta_slice)
            {
                if !app.version_pin_strict
                    && matches!(
                        patch_supported,
                        crate::patcher::metadata::CompatibleVersions::Intersection(ref v)
                            if v.is_empty()
                    )
                {
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
                            ..Default::default()
                        },
                    );
                } else if !app.version_pin_strict {
                    tracing::warn!("{id}: {err} (auto-correcting to compatible version)");
                } else {
                    tracing::warn!("{id}: {err} (honoring strict pin)");
                }
            }
        }
    }

    let (
        raw_apk,
        fetch_attempts,
        fetched_version,
        mut fetch_warnings,
        requested_version,
        candidates_tried,
    ) = match builder::fetch_apk(cfg, id, app, arch, force, uptodown_cb, breaker_registry).await {
        Ok(succ) => (
            succ.path,
            succ.attempts,
            succ.final_version,
            succ.warnings,
            succ.requested_version,
            succ.candidates_tried,
        ),
        Err(e) => {
            let (attempts, warnings, req_v, cand_t) =
                if let Some(fetch_err) = e.downcast_ref::<builder::FetchApkError>() {
                    (
                        fetch_err.attempts.clone(),
                        fetch_err.warnings.clone(),
                        fetch_err.requested_version.clone(),
                        fetch_err.candidates_tried.clone(),
                    )
                } else {
                    (vec![], vec![], target_ver.clone(), vec![])
                };
            let err_msg = format!("{id}: fetch APK failed: {e}");
            let cat = FailureCategory::from_error_message(&err_msg);
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    requested_version: req_v,
                    version_candidates_tried: cand_t,
                    version_downgraded: false,
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
                    ..Default::default()
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

    let newest_supported = if !patch_supported_list.is_empty() {
        crate::utils::semver::max_version(&patch_supported_list)
    } else {
        None
    };

    let is_downgraded = if let (Some(ref req), Some(ref res)) = (&requested_version, &resolved_ver)
    {
        req != res && !crate::utils::version::versions_match(req, res)
    } else if let (Some(ref newest), Some(ref res)) = (&newest_supported, &resolved_ver) {
        newest != res && !crate::utils::version::versions_match(newest, res)
    } else {
        false
    };

    // Check APK integrity after download
    if let Err(integrity_err) = crate::utils::apk::verify_apk_integrity(&raw_apk) {
        warn!("{id}: downloaded APK failed integrity check: {integrity_err}");
        let err_msg = format!("{id}: downloaded APK failed integrity check: {integrity_err}");
        return (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                requested_version: requested_version.clone(),
                version_candidates_tried: candidates_tried.clone(),
                version_downgraded: is_downgraded,
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
                ..Default::default()
            },
        );
    }

    let file_sha256 = crate::utils::apk::compute_file_sha256(&raw_apk).ok();
    let cert_sha = crate::utils::keystore::get_apk_cert_sha256(&raw_apk)
        .ok()
        .flatten();

    // Verify package name equals expected package
    let actual_pkg_res = crate::utils::apk::get_apk_package_name(&raw_apk);
    let package_ok = match actual_pkg_res {
        Ok(ref pkg) => app.package.is_empty() || pkg == &app.package,
        Err(_) => false,
    };
    if !package_ok {
        let err_msg = format!(
            "{id}: downloaded APK package '{:?}' does not match expected '{}'",
            actual_pkg_res, app.package
        );
        warn!("{err_msg}");
        return (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                requested_version: requested_version.clone(),
                version_candidates_tried: candidates_tried.clone(),
                version_downgraded: is_downgraded,
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
                failure_category: Some(FailureCategory::IntegrityCheckFailed),
                attempted_sources: fetch_attempts,
                sha256: file_sha256,
                cert_sha256: cert_sha,
                integrity: Some(crate::report::IntegrityStatus {
                    package_ok: false,
                    version_ok: true,
                    cert_ok: false,
                }),
                ..Default::default()
            },
        );
    }

    // Verify downloaded versionName equals expected one before patching
    let actual_ver_res = crate::utils::apk::get_apk_version_name(&raw_apk);
    let version_ok = if let Some(ref exp_v) = effective_target_ver {
        if !exp_v.eq_ignore_ascii_case("auto") {
            actual_ver_res
                .as_ref()
                .map(|v| crate::utils::version::versions_match(v, exp_v))
                .unwrap_or(false)
        } else {
            true
        }
    } else {
        true
    };

    if !version_ok {
        let act_v = actual_ver_res.unwrap_or_else(|_| "unknown".to_string());
        let exp_v = effective_target_ver.as_deref().unwrap_or("unknown");
        let err_msg = format!(
            "{id}: downloaded APK version v{act_v} does not match expected target v{exp_v}"
        );
        warn!("{err_msg}");
        return (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                requested_version: requested_version.clone(),
                version_candidates_tried: candidates_tried.clone(),
                version_downgraded: is_downgraded,
                target_version: effective_target_ver,
                target_version_reason: target_version_reason.clone(),
                patch_supported_versions: patch_supported_list,
                fetched_version: Some(act_v),
                resolved_version: resolved_ver,
                resolved_source,
                resolution_strategy: Some(strategy_str),
                version_conflict,
                patches_included: vec![],
                patches_auto_disabled: vec![],
                warnings: fetch_warnings,
                error: Some(err_msg),
                failure_category: Some(FailureCategory::IntegrityCheckFailed),
                attempted_sources: fetch_attempts,
                sha256: file_sha256,
                cert_sha256: cert_sha,
                integrity: Some(crate::report::IntegrityStatus {
                    package_ok: true,
                    version_ok: false,
                    cert_ok: false,
                }),
                ..Default::default()
            },
        );
    }

    let exp_certs = app.expected_certs();
    let cert_matched = if let Some(ref actual_cert) = cert_sha {
        exp_certs
            .iter()
            .any(|c| c.eq_ignore_ascii_case(actual_cert))
    } else {
        false
    };

    if exp_certs.is_empty() {
        fetch_warnings.push(format!("{id}: no expected_cert_sha256 configured for app"));
    } else if !cert_matched {
        let err_msg = format!(
            "{id}: downloaded APK cert SHA-256 '{cert_sha:?}' does not match any expected cert fingerprint"
        );
        warn!("{err_msg}");
        return (
            None,
            AppBuildReport {
                status: "failed".to_string(),
                requested_version: requested_version.clone(),
                version_candidates_tried: candidates_tried.clone(),
                version_downgraded: is_downgraded,
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
                failure_category: Some(FailureCategory::IntegrityCheckFailed),
                attempted_sources: fetch_attempts,
                sha256: file_sha256,
                cert_sha256: cert_sha,
                integrity: Some(crate::report::IntegrityStatus {
                    package_ok: true,
                    version_ok: true,
                    cert_ok: false,
                }),
                ..Default::default()
            },
        );
    }

    let is_mirror_degraded = if resolved_source.as_deref() == Some("archive") && !cert_matched {
        fetch_warnings.push(format!(
            "{id}: downloaded from third-party mirror archive.org without verified certificate match — building degraded"
        ));
        true
    } else {
        false
    };

    let integrity_status = Some(crate::report::IntegrityStatus {
        package_ok: true,
        version_ok: true,
        cert_ok: if exp_certs.is_empty() {
            true
        } else {
            cert_matched
        },
    });

    if effective_mode == "install" || app.patch_source == "none" || app.patch_source.is_empty() {
        let dest = format!("{output_dir}/{id}.apk");
        if let Err(e) = std::fs::copy(&raw_apk, &dest) {
            let err_msg = e.to_string();
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    requested_version: requested_version.clone(),
                    version_candidates_tried: candidates_tried.clone(),
                    version_downgraded: is_downgraded,
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
                    ..Default::default()
                },
            );
        }
        info!("{id}: copied unpatched official APK to {dest}");
        let out_sha256 = crate::utils::apk::compute_file_sha256(&dest)
            .ok()
            .or(file_sha256);
        let out_size = std::fs::metadata(&dest).ok().map(|m| m.len());
        let supported_archs = detect_supported_archs(&dest, arch);
        let validation_status = if integrity_status
            .as_ref()
            .is_some_and(|i| i.package_ok && i.version_ok && i.cert_ok)
        {
            Some("passed".to_string())
        } else {
            Some("warning".to_string())
        };
        return (
            Some(dest.clone()),
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
                output_file: std::path::Path::new(&dest)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(String::from),
                output_path: Some(dest),
                sha256: out_sha256,
                size_bytes: out_size,
                optimization_mode: Some("full".to_string()),
                supported_archs,
                validation_status,
                cert_sha256: cert_sha,
                integrity: integrity_status,
                ..Default::default()
            },
        );
    }

    let (apk_path, arch_mode) = if arch != "all" && arch != "universal" {
        let work_apk = format!("{}/{id}-{arch}-work.apk", cfg.build.temp_dir);
        let has_target_arch = builder::arch::apk_has_arch(&raw_apk, arch).unwrap_or(false);
        if !has_target_arch {
            info!(
                "{id}: input APK has no native libs for '{arch}', falling back to unstripped universal APK"
            );
            (raw_apk, "universal".to_string())
        } else if let Err(e) = builder::arch::strip_unsupported_archs_to(&raw_apk, &work_apk, arch)
        {
            warn!("{id}: arch stripping note: {e}");
            (raw_apk, "universal".to_string())
        } else if let Err(e) = builder::arch::verify_arch_native_libs(&raw_apk, &work_apk, arch) {
            let err_msg = format!("{id}: arch verification failed: {e}");
            info!("{err_msg}");
            return (
                None,
                AppBuildReport {
                    status: "failed".to_string(),
                    requested_version: requested_version.clone(),
                    version_candidates_tried: candidates_tried.clone(),
                    version_downgraded: is_downgraded,
                    target_version: effective_target_ver,
                    target_version_reason: target_version_reason.clone(),
                    patch_supported_versions: patch_supported_list,
                    fetched_version: fetched_version.clone(),
                    resolved_version: resolved_ver,
                    resolved_source,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    warnings: fetch_warnings.clone(),
                    error: Some(err_msg),
                    failure_category: Some(FailureCategory::ArchUnsupported),
                    attempted_sources: fetch_attempts,
                    arch_mode: Some(arch.to_string()),
                    ..Default::default()
                },
            );
        } else {
            (work_apk, arch.to_string())
        }
    } else {
        (raw_apk, "universal".to_string())
    };

    let needs_module_root =
        app.module.as_ref().is_some_and(|m| m.single || m.bundle) || effective_mode == "both";

    let (patch_res, root_payload_path) = if effective_mode == "both" || needs_module_root {
        match patcher::patch(cfg, id, app, &apk_path, output_dir, false).await {
            Ok((non_root, plan)) => {
                if effective_mode == "lite" {
                    if let Err(e) = builder::lite::optimize_apk(&non_root) {
                        warn!("{id}: lite optimization note: {e}");
                    }
                }
                let root_res = patcher::patch(cfg, id, app, &apk_path, output_dir, true).await;
                let root_payload = match root_res {
                    Ok((r_path, _)) => Some(r_path),
                    Err(e) => {
                        warn!("{id}: root payload patch failed: {e}");
                        None
                    }
                };
                (Ok((non_root, plan)), root_payload)
            }
            Err(e) => (Err(e), None),
        }
    } else {
        let is_root = effective_mode == "module" || effective_mode == "root";
        let res = patcher::patch(cfg, id, app, &apk_path, output_dir, is_root).await;
        match res {
            Ok((patched, plan)) => {
                if effective_mode == "lite" {
                    if let Err(e) = builder::lite::optimize_apk(&patched) {
                        warn!("{id}: lite optimization note: {e}");
                    }
                }
                if is_root {
                    (Ok((patched.clone(), plan)), Some(patched))
                } else {
                    (Ok((patched, plan)), None)
                }
            }
            Err(e) => (Err(e), None),
        }
    };

    match patch_res {
        Ok((out_path, plan)) => {
            let mut warnings = plan.warnings;
            warnings.extend(fetch_warnings.clone());

            if !plan.critical_disabled.is_empty() {
                let crit_list = plan.critical_disabled.join(", ");
                let err_msg = format!("{id}: critical patch(es) [{crit_list}] were auto-disabled as incompatible; refusing degraded build");
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
                        patches_included: plan.included,
                        patches_auto_disabled_count: plan.auto_disabled.len(),
                        critical_patches_disabled: plan.critical_disabled,
                        patches_auto_disabled: plan.auto_disabled,
                        warnings,
                        error: Some(err_msg),
                        failure_category: Some(FailureCategory::VersionNotSupported),
                        attempted_sources: fetch_attempts,
                        arch_mode: Some(arch_mode),
                        ..Default::default()
                    },
                );
            }

            let is_resolved_unsupported = resolved_ver.as_ref().is_some_and(|v| {
                !patch_supported_list.is_empty()
                    && !patch_supported_list
                        .iter()
                        .any(|s| s == v || crate::utils::version::versions_match(s, v))
            });
            if is_resolved_unsupported {
                warnings.push(format!(
                    "{id}: resolved version v{} is not in patch-supported set {:?}",
                    resolved_ver.as_deref().unwrap_or(""),
                    patch_supported_list
                ));
            }

            if let (Some(ref newest), Some(ref res)) = (&newest_supported, &resolved_ver) {
                if newest != res && !crate::utils::version::versions_match(newest, res) {
                    warnings.push(format!(
                        "{id}: resolved version v{res} is not the newest supported version v{newest}"
                    ));
                }
            }

            if let Some(min_p) = app.expected_min_patches {
                if plan.included.len() < min_p {
                    let warn_msg = format!(
                        "{id}: included patches count ({}) is below expected minimum ({min_p})",
                        plan.included.len()
                    );
                    tracing::warn!("{warn_msg}");
                    warnings.push(warn_msg);
                }
            }

            let status = if effective_mode != "install"
                && app.patch_source != "none"
                && !app.patch_source.is_empty()
                && plan.included.is_empty()
            {
                warnings.push(format!("{id}: app configured with mode='patch' but 0 patches were applied; output APK is signed stock"));
                "success_unpatched".to_string()
            } else if is_resolved_unsupported
                || !plan.auto_disabled.is_empty()
                || is_mirror_degraded
            {
                "degraded".to_string()
            } else {
                "success".to_string()
            };
            let output_file = std::path::Path::new(&out_path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from);
            let out_sha256 = crate::utils::apk::compute_file_sha256(&out_path)
                .ok()
                .or(file_sha256);
            let out_size = std::fs::metadata(&out_path).ok().map(|m| m.len());
            let supported_archs = detect_supported_archs(&out_path, &arch_mode);
            let opt_mode = normalize_optimization_mode(effective_mode).to_string();
            let validation_status = match status.as_str() {
                "success" => {
                    if integrity_status
                        .as_ref()
                        .is_none_or(|i| i.package_ok && i.version_ok && i.cert_ok)
                    {
                        "passed"
                    } else {
                        "warning"
                    }
                }
                "degraded" | "success_unpatched" => "warning",
                "failed" => "failed",
                _ => "unknown",
            }
            .to_string();
            (
                Some(out_path.clone()),
                AppBuildReport {
                    status,
                    requested_version: requested_version.clone(),
                    version_candidates_tried: candidates_tried.clone(),
                    version_downgraded: is_downgraded,
                    target_version: effective_target_ver,
                    target_version_reason: target_version_reason.clone(),
                    patch_supported_versions: patch_supported_list,
                    fetched_version: fetched_version.clone(),
                    resolved_version: resolved_ver,
                    resolved_source,
                    resolution_strategy: Some(strategy_str),
                    version_conflict,
                    patches_included: plan.included,
                    patches_auto_disabled_count: plan.auto_disabled.len(),
                    critical_patches_disabled: plan.critical_disabled,
                    patches_auto_disabled: plan.auto_disabled,
                    warnings,
                    error: None,
                    failure_category: None,
                    attempted_sources: fetch_attempts,
                    arch_mode: Some(arch_mode),
                    output_file,
                    output_path: Some(out_path),
                    root_payload_path,
                    module_file: None,
                    runtime_test: None,
                    sha256: out_sha256,
                    size_bytes: out_size,
                    optimization_mode: Some(opt_mode),
                    supported_archs,
                    validation_status: Some(validation_status),
                    cert_sha256: cert_sha.clone(),
                    integrity: integrity_status.clone(),
                    ..Default::default()
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
                    requested_version: requested_version.clone(),
                    version_candidates_tried: candidates_tried.clone(),
                    version_downgraded: is_downgraded,
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
                    patches_auto_disabled_count: 0,
                    critical_patches_disabled: vec![],
                    warnings: fetch_warnings,
                    error: Some(err_msg),
                    failure_category: Some(cat),
                    attempted_sources: fetch_attempts,
                    arch_mode: Some(arch_mode),
                    output_file: None,
                    output_path: None,
                    root_payload_path: None,
                    module_file: None,
                    runtime_test: None,
                    sha256: file_sha256,
                    cert_sha256: cert_sha,
                    integrity: integrity_status,
                    ..Default::default()
                },
            )
        }
    }
}
