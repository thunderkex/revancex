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

#[allow(clippy::too_many_arguments)]
pub async fn run_pipeline(
    cfg: &Config,
    targets: &[String],
    arch: &str,
    mode: Option<&str>,
    force: bool,
    output_dir: &str,
    patch_results: Option<&std::collections::BTreeMap<String, Result<String, String>>>,
    net: Arc<crate::net::SharedNet>,
) -> Result<BuildSummary> {
    let semaphore = Arc::new(Semaphore::new(cfg.build.workers));
    let cfg_arc = Arc::new(cfg.clone());
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
            let breaker_reg = breaker_registry.clone();
            let net_clone = net.clone();

            ui::register_item(&id);
            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let started = std::time::Instant::now();
                let budget_secs = app.timeout_secs.unwrap_or(cfg.build.app_timeout_secs);
                let req = BuildSingleReq {
                    cfg: &cfg,
                    id: &id,
                    app: &app,
                    arch: &arch,
                    cli_mode: cli_mode.as_deref(),
                    force,
                    output_dir: &output_dir,
                    breaker_registry: breaker_reg.clone(),
                    net: &net_clone,
                };
                let app_fut = build_single(&req);
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
                                status: crate::report::BuildStatus::Failed,
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
struct BuildCtx<'a> {
    id: &'a str,
    app: &'a AppConfig,
    effective_mode: &'static str,
    patch_supported: crate::patcher::metadata::CompatibleVersions,
    meta: Option<Arc<Vec<crate::patcher::metadata::PatchMeta>>>,
    target_ver: Option<String>,
    target_version_reason: Option<String>,
    patch_supported_list: Vec<String>,
    version_conflict: Option<Vec<(String, Vec<String>)>>,
    strategy_str: String,
    newest_supported: Option<String>,
}

struct FetchMeta {
    raw_apk: String,
    fetch_attempts: Vec<SourceAttemptRecord>,
    fetched_version: Option<String>,
    fetch_warnings: Vec<String>,
    requested_version: Option<String>,
    candidates_tried: Vec<String>,
    resolved_source: Option<String>,
    resolved_ver: Option<String>,
    effective_target_ver: Option<String>,
    is_downgraded: bool,
    file_sha256: Option<String>,
    cert_sha: Option<String>,
}

impl<'a> BuildCtx<'a> {
    fn base_report(&self, fetch: Option<&FetchMeta>) -> AppBuildReport {
        AppBuildReport {
            status: crate::report::BuildStatus::Failed,
            requested_version: fetch.and_then(|f| f.requested_version.clone()),
            version_candidates_tried: fetch
                .map(|f| f.candidates_tried.clone())
                .unwrap_or_default(),
            version_downgraded: fetch.is_some_and(|f| f.is_downgraded),
            target_version: fetch
                .and_then(|f| f.effective_target_ver.clone())
                .or_else(|| self.target_ver.clone()),
            target_version_reason: self.target_version_reason.clone(),
            patch_supported_versions: self.patch_supported_list.clone(),
            fetched_version: fetch.and_then(|f| f.fetched_version.clone()),
            resolved_version: fetch
                .map(|f| f.resolved_ver.clone())
                .unwrap_or_else(|| self.target_ver.clone()),
            resolved_source: fetch.and_then(|f| f.resolved_source.clone()),
            resolution_strategy: Some(self.strategy_str.clone()),
            version_conflict: self.version_conflict.clone(),
            attempted_sources: fetch.map(|f| f.fetch_attempts.clone()).unwrap_or_default(),
            sha256: fetch.and_then(|f| f.file_sha256.clone()),
            cert_sha256: fetch.and_then(|f| f.cert_sha.clone()),
            ..Default::default()
        }
    }

    fn fail_preflight(
        &self,
        err_msg: String,
        cat: FailureCategory,
        reason_override: Option<&str>,
        clear_resolved_ver: bool,
    ) -> (Option<String>, AppBuildReport) {
        let mut rep = self.base_report(None);
        if let Some(r) = reason_override {
            rep.target_version_reason = Some(r.to_string());
        }
        if clear_resolved_ver {
            rep.resolved_version = None;
        }
        rep.warnings = vec![err_msg.clone()];
        rep.error = Some(err_msg);
        rep.failure_category = Some(cat);
        (None, rep)
    }

    fn fail_with_fetch(
        &self,
        fetch: &FetchMeta,
        err_msg: String,
        cat: FailureCategory,
        integrity: Option<crate::report::IntegrityStatus>,
        arch_mode: Option<String>,
        warnings: Option<Vec<String>>,
    ) -> (Option<String>, AppBuildReport) {
        let mut rep = self.base_report(Some(fetch));
        rep.warnings = warnings.unwrap_or_else(|| fetch.fetch_warnings.clone());
        rep.error = Some(err_msg);
        rep.failure_category = Some(cat);
        rep.integrity = integrity;
        rep.arch_mode = arch_mode;
        (None, rep)
    }
}

async fn resolve_build_ctx<'a>(
    cfg: &Config,
    id: &'a str,
    app: &'a AppConfig,
    cli_mode: Option<&str>,
) -> BuildCtx<'a> {
    let effective_mode = app.resolved_mode(cli_mode).as_str();
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
    let meta = match crate::patcher::metadata::resolve_mpp_path(cfg, app) {
        Some(ref p) => crate::patcher::metadata::load(cfg, p).await.ok(),
        None => None,
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
    let newest_supported = (!patch_supported_list.is_empty())
        .then(|| crate::utils::version::max_version(&patch_supported_list))
        .flatten();

    BuildCtx {
        id,
        app,
        effective_mode,
        patch_supported,
        meta,
        target_ver,
        target_version_reason,
        patch_supported_list,
        version_conflict,
        strategy_str,
        newest_supported,
    }
}

fn preflight_patch_checks(ctx: &BuildCtx<'_>) -> Option<(Option<String>, AppBuildReport)> {
    if ctx.effective_mode == "install"
        || ctx.app.patch_source == "none"
        || ctx.app.patch_source.is_empty()
    {
        return None;
    }

    if matches!(
        ctx.patch_supported,
        crate::patcher::metadata::CompatibleVersions::Unknown
    ) && ctx
        .app
        .version
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("auto"))
        .unwrap_or(true)
        && ctx.app.on_unknown_compat.as_deref().unwrap_or("fail") == "fail"
    {
        let msg = format!(
            "{}: refusing to build latest, patch compatibility unknown",
            ctx.id
        );
        warn!("{msg}");
        return Some(ctx.fail_preflight(msg, FailureCategory::CompatibilityUnknown, None, false));
    }

    if let Some(ref meta_slice) = ctx.meta {
        let explicit_enabled: Vec<String> = ctx
            .app
            .included_patches
            .iter()
            .chain(ctx.app.patches.iter())
            .cloned()
            .collect();
        if !meta_slice.is_empty()
            && crate::patcher::metadata::count_applicable_patches(
                meta_slice,
                &ctx.app.package,
                &explicit_enabled,
                &ctx.app.excluded_patches,
            ) == 0
        {
            let err_msg = format!(
                "{}: 0 enabled patches applicable for '{}' (mode='{}')",
                ctx.id, ctx.app.package, ctx.effective_mode
            );
            return Some(ctx.fail_preflight(
                err_msg,
                FailureCategory::NoEnabledPatches,
                Some("no enabled patches applicable"),
                false,
            ));
        }
    }

    if matches!(
        ctx.patch_supported,
        crate::patcher::metadata::CompatibleVersions::Intersection(ref v) if v.is_empty()
    ) && ctx.target_ver.is_none()
    {
        let err_msg = format!(
            "{}: version not supported — no enabled patches support package '{}' or compatible version set is empty",
            ctx.id, ctx.app.package
        );
        return Some(ctx.fail_preflight(
            err_msg,
            FailureCategory::VersionNotSupported,
            Some("no compatible version resolved"),
            true,
        ));
    }

    let pin_val = ctx
        .app
        .version
        .as_deref()
        .filter(|v| !v.eq_ignore_ascii_case("auto"));
    let is_pin_unsupported = pin_val.is_some_and(|pin| {
        !ctx.patch_supported_list.is_empty()
            && !ctx
                .patch_supported_list
                .iter()
                .any(|v| crate::utils::version::version_eq(v, pin))
    });
    if ctx.app.effective_pin_policy() == crate::config::apps::PinPolicy::Strict
        && is_pin_unsupported
    {
        let pin = pin_val.unwrap();
        let err_msg = format!(
            "{}: pinned version v{pin} is not supported by patches and pin_policy is strict",
            ctx.id
        );
        return Some(ctx.fail_preflight(
            err_msg,
            FailureCategory::VersionNotSupported,
            None,
            false,
        ));
    }

    if let Some(ref meta_slice) = ctx.meta {
        if let Some(err) =
            crate::patcher::metadata::check_app_version_compatibility(ctx.id, ctx.app, meta_slice)
        {
            if !ctx.app.version_pin_strict
                && matches!(
                    ctx.patch_supported,
                    crate::patcher::metadata::CompatibleVersions::Intersection(ref v) if v.is_empty()
                )
            {
                let err_msg = format!("{}: {err}", ctx.id);
                let cat = FailureCategory::from_error_message(&err_msg);
                let mut res = ctx.fail_preflight(err_msg, cat, None, false);
                res.1.warnings = vec![err];
                return Some(res);
            } else if !ctx.app.version_pin_strict {
                tracing::warn!("{}: {err} (auto-correcting to compatible version)", ctx.id);
            } else {
                tracing::warn!("{}: {err} (honoring strict pin)", ctx.id);
            }
        }
    }
    None
}

async fn fetch_build_input_apk(
    cfg: &Config,
    ctx: &BuildCtx<'_>,
    arch: &str,
    force: bool,
    breaker_registry: Arc<crate::sources::breaker::BreakerRegistry>,
    net: &crate::net::SharedNet,
) -> std::result::Result<FetchMeta, Box<AppBuildReport>> {
    let succ = builder::fetch_apk(
        cfg,
        ctx.id,
        ctx.app,
        arch,
        force,
        Some(breaker_registry),
        net,
    )
    .await
    .map_err(|e| {
        let (attempts, warnings, req_v, cand_t) =
            if let Some(fetch_err) = e.downcast_ref::<builder::FetchApkError>() {
                (
                    fetch_err.attempts.clone(),
                    fetch_err.warnings.clone(),
                    fetch_err.requested_version.clone(),
                    fetch_err.candidates_tried.clone(),
                )
            } else {
                (vec![], vec![], ctx.target_ver.clone(), vec![])
            };
        let err_msg = format!("{}: fetch APK failed: {e}", ctx.id);
        let cat = FailureCategory::from_error_message(&err_msg);
        let mut rep = ctx.base_report(None);
        rep.requested_version = req_v;
        rep.version_candidates_tried = cand_t;
        rep.warnings = warnings;
        rep.error = Some(err_msg);
        rep.failure_category = Some(cat);
        rep.attempted_sources = attempts;
        Box::new(rep)
    })?;

    let resolved_source = succ
        .attempts
        .iter()
        .find(|a| a.outcome == "hit")
        .map(|a| a.source.clone())
        .or_else(|| ctx.app.apk_url.is_some().then(|| "direct_url".to_string()));
    let resolved_ver = succ
        .final_version
        .clone()
        .or_else(|| ctx.target_ver.clone());
    let effective_target_ver = succ
        .final_version
        .clone()
        .or_else(|| ctx.target_ver.clone());
    let is_downgraded = match (
        &succ.requested_version,
        &ctx.newest_supported,
        &resolved_ver,
    ) {
        (Some(req), _, Some(res)) => !crate::utils::version::version_eq(req, res),
        (None, Some(newest), Some(res)) => !crate::utils::version::version_eq(newest, res),
        _ => false,
    };

    Ok(FetchMeta {
        raw_apk: succ.path,
        fetch_attempts: succ.attempts,
        fetched_version: succ.final_version,
        fetch_warnings: succ.warnings,
        requested_version: succ.requested_version,
        candidates_tried: succ.candidates_tried,
        resolved_source,
        resolved_ver,
        effective_target_ver,
        is_downgraded,
        file_sha256: None,
        cert_sha: None,
    })
}

fn verify_fetched_raw_apk(
    ctx: &BuildCtx<'_>,
    fetch: &mut FetchMeta,
) -> std::result::Result<(Option<crate::report::IntegrityStatus>, bool), Box<AppBuildReport>> {
    if let Err(integrity_err) = crate::utils::apk::verify_apk_integrity(&fetch.raw_apk) {
        warn!(
            "{}: downloaded APK failed integrity check: {integrity_err}",
            ctx.id
        );
        let err_msg = format!(
            "{}: downloaded APK failed integrity check: {integrity_err}",
            ctx.id
        );
        return Err(Box::new(
            ctx.fail_with_fetch(
                fetch,
                err_msg,
                FailureCategory::IntegrityCheckFailed,
                None,
                None,
                Some(vec![]),
            )
            .1,
        ));
    }

    fetch.file_sha256 = crate::utils::apk::compute_file_sha256(&fetch.raw_apk).ok();
    fetch.cert_sha = crate::utils::keystore::get_apk_cert_sha256(&fetch.raw_apk)
        .ok()
        .flatten();

    let actual_pkg_res = crate::utils::apk::get_apk_package_name(&fetch.raw_apk);
    let package_ok = actual_pkg_res
        .as_ref()
        .is_ok_and(|pkg| ctx.app.package.is_empty() || pkg == &ctx.app.package);
    if !package_ok {
        let err_msg = format!(
            "{}: downloaded APK package '{:?}' does not match expected '{}'",
            ctx.id, actual_pkg_res, ctx.app.package
        );
        warn!("{err_msg}");
        return Err(Box::new(
            ctx.fail_with_fetch(
                fetch,
                err_msg,
                FailureCategory::IntegrityCheckFailed,
                Some(crate::report::IntegrityStatus {
                    package_ok: false,
                    version_ok: true,
                    cert_ok: false,
                }),
                None,
                None,
            )
            .1,
        ));
    }

    let actual_ver_res = crate::utils::apk::get_apk_version_name(&fetch.raw_apk);
    let version_ok = match fetch.effective_target_ver {
        Some(ref exp_v) if !exp_v.eq_ignore_ascii_case("auto") => actual_ver_res
            .as_ref()
            .is_ok_and(|v| crate::utils::version::version_eq(v, exp_v)),
        _ => true,
    };
    if !version_ok {
        let act_v = actual_ver_res.unwrap_or_else(|_| "unknown".to_string());
        let exp_v = fetch.effective_target_ver.as_deref().unwrap_or("unknown");
        let err_msg = format!(
            "{}: downloaded APK version v{act_v} does not match expected target v{exp_v}",
            ctx.id
        );
        warn!("{err_msg}");
        let mut rep = ctx
            .fail_with_fetch(
                fetch,
                err_msg,
                FailureCategory::IntegrityCheckFailed,
                Some(crate::report::IntegrityStatus {
                    package_ok: true,
                    version_ok: false,
                    cert_ok: false,
                }),
                None,
                None,
            )
            .1;
        rep.fetched_version = Some(act_v);
        return Err(Box::new(rep));
    }

    let exp_certs = ctx.app.expected_certs();
    let cert_matched = fetch.cert_sha.as_ref().is_some_and(|actual_cert| {
        exp_certs
            .iter()
            .any(|c| c.eq_ignore_ascii_case(actual_cert))
    });
    if exp_certs.is_empty() {
        fetch.fetch_warnings.push(format!(
            "{}: no expected_cert_sha256 configured for app",
            ctx.id
        ));
    } else if !cert_matched {
        let err_msg = format!(
            "{}: downloaded APK cert SHA-256 '{:?}' does not match any expected cert fingerprint",
            ctx.id, fetch.cert_sha
        );
        warn!("{err_msg}");
        return Err(Box::new(
            ctx.fail_with_fetch(
                fetch,
                err_msg,
                FailureCategory::IntegrityCheckFailed,
                Some(crate::report::IntegrityStatus {
                    package_ok: true,
                    version_ok: true,
                    cert_ok: false,
                }),
                None,
                None,
            )
            .1,
        ));
    }

    let is_mirror_degraded = if fetch.resolved_source.as_deref() == Some("archive") && !cert_matched
    {
        fetch.fetch_warnings.push(format!(
            "{}: downloaded from third-party mirror archive.org without verified certificate match — building degraded",
            ctx.id
        ));
        true
    } else {
        false
    };
    let integrity_status = Some(crate::report::IntegrityStatus {
        package_ok: true,
        version_ok: true,
        cert_ok: exp_certs.is_empty() || cert_matched,
    });
    Ok((integrity_status, is_mirror_degraded))
}

fn finalize_install_mode_build(
    ctx: &BuildCtx<'_>,
    fetch: &FetchMeta,
    integrity_status: Option<crate::report::IntegrityStatus>,
    arch: &str,
    output_dir: &str,
) -> (Option<String>, AppBuildReport) {
    let dest = format!("{output_dir}/{}.apk", ctx.id);
    if let Err(e) = std::fs::copy(&fetch.raw_apk, &dest) {
        let err_msg = e.to_string();
        let cat = FailureCategory::from_error_message(&err_msg);
        let mut rep = ctx.fail_with_fetch(fetch, err_msg, cat, None, None, Some(vec![]));
        rep.1.patch_supported_versions = vec![];
        rep.1.resolution_strategy = Some("install_mode".to_string());
        rep.1.sha256 = None;
        rep.1.cert_sha256 = None;
        return rep;
    }
    info!("{}: copied unpatched official APK to {dest}", ctx.id);
    let out_sha256 = crate::utils::apk::compute_file_sha256(&dest)
        .ok()
        .or_else(|| fetch.file_sha256.clone());
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
    let mut rep = ctx.base_report(Some(fetch));
    rep.status = crate::report::BuildStatus::Success;
    rep.target_version_reason = Some("install mode (no patches)".to_string());
    rep.patch_supported_versions = vec![];
    rep.resolution_strategy = Some("install_mode".to_string());
    rep.warnings = fetch.fetch_warnings.clone();
    rep.output_file = std::path::Path::new(&dest)
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);
    rep.output_path = Some(dest.clone());
    rep.sha256 = out_sha256;
    rep.size_bytes = out_size;
    rep.optimization_mode = Some("full".to_string());
    rep.supported_archs = supported_archs;
    rep.validation_status = validation_status;
    rep.integrity = integrity_status;
    (Some(dest), rep)
}

async fn execute_patch_variants<'a>(
    cfg: &Config,
    ctx: &BuildCtx<'_>,
    arch: &'a str,
    output_dir: &str,
    raw_apk: &str,
) -> (
    String,
    Option<&'a str>,
    anyhow::Result<(String, crate::patcher::plan::PatchPlan)>,
    Option<String>,
) {
    let (apk_path, arch_mode, strip_arch) = if ctx.app.preserve_all_archs {
        if arch != "all" && arch != "universal" {
            info!(
                "{}: preserve_all_archs is enabled; keeping all native architectures instead of stripping to '{arch}'",
                ctx.id
            );
        }
        (raw_apk.to_string(), "universal".to_string(), None)
    } else if arch != "all" && arch != "universal" {
        if !builder::arch::apk_has_arch(raw_apk, arch).unwrap_or(false) {
            info!(
                "{}: input APK has no native libs for '{arch}', falling back to unstripped universal APK",
                ctx.id
            );
            (raw_apk.to_string(), "universal".to_string(), None)
        } else {
            (raw_apk.to_string(), arch.to_string(), Some(arch))
        }
    } else {
        (raw_apk.to_string(), "universal".to_string(), None)
    };

    let needs_module_root =
        ctx.app.module.single || ctx.app.module.bundle || ctx.effective_mode == "both";
    if ctx.effective_mode == "both" || needs_module_root {
        match patcher::patch_with_arch(
            cfg, ctx.id, ctx.app, &apk_path, output_dir, false, strip_arch,
        )
        .await
        {
            Ok((non_root, plan)) => {
                if ctx.effective_mode == "lite" {
                    if let Err(e) = builder::lite::optimize_apk(&non_root) {
                        warn!("{}: lite optimization note: {e}", ctx.id);
                    }
                }
                let root_payload = match patcher::patch_with_arch(
                    cfg, ctx.id, ctx.app, &apk_path, output_dir, true, strip_arch,
                )
                .await
                {
                    Ok((r_path, _)) => Some(r_path),
                    Err(e) => {
                        warn!("{}: root payload patch failed: {e}", ctx.id);
                        None
                    }
                };
                (arch_mode, strip_arch, Ok((non_root, plan)), root_payload)
            }
            Err(e) => (arch_mode, strip_arch, Err(e), None),
        }
    } else {
        let is_root = ctx.effective_mode == "module" || ctx.effective_mode == "root";
        match patcher::patch_with_arch(
            cfg, ctx.id, ctx.app, &apk_path, output_dir, is_root, strip_arch,
        )
        .await
        {
            Ok((patched, plan)) => {
                if ctx.effective_mode == "lite" {
                    if let Err(e) = builder::lite::optimize_apk(&patched) {
                        warn!("{}: lite optimization note: {e}", ctx.id);
                    }
                }
                let root_payload = is_root.then(|| patched.clone());
                (arch_mode, strip_arch, Ok((patched, plan)), root_payload)
            }
            Err(e) => (arch_mode, strip_arch, Err(e), None),
        }
    }
}

struct PatchRunOutcome<'a> {
    integrity_status: Option<crate::report::IntegrityStatus>,
    is_mirror_degraded: bool,
    arch: &'a str,
    arch_mode: String,
    strip_arch: Option<&'a str>,
    patch_res: anyhow::Result<(String, crate::patcher::plan::PatchPlan)>,
    root_payload_path: Option<String>,
}

struct BuildSingleReq<'a> {
    cfg: &'a Config,
    id: &'a str,
    app: &'a AppConfig,
    arch: &'a str,
    cli_mode: Option<&'a str>,
    force: bool,
    output_dir: &'a str,
    breaker_registry: Arc<crate::sources::breaker::BreakerRegistry>,
    net: &'a crate::net::SharedNet,
}

fn finalize_patched_build(
    ctx: &BuildCtx<'_>,
    fetch: &FetchMeta,
    outcome: PatchRunOutcome<'_>,
) -> (Option<String>, AppBuildReport) {
    let PatchRunOutcome {
        integrity_status,
        is_mirror_degraded,
        arch,
        arch_mode,
        strip_arch,
        patch_res,
        root_payload_path,
    } = outcome;
    let (out_path, plan) = match patch_res {
        Ok(res) => res,
        Err(e) => {
            let err_msg = e.to_string();
            let cat = FailureCategory::from_error_message(&err_msg);
            return ctx.fail_with_fetch(
                fetch,
                err_msg,
                cat,
                integrity_status,
                Some(arch_mode),
                None,
            );
        }
    };

    if let Some(target_arch) = strip_arch {
        if let Err(e) =
            builder::arch::verify_arch_native_libs(&fetch.raw_apk, &out_path, target_arch)
        {
            let err_msg = format!("{}: arch verification failed: {e}", ctx.id);
            info!("{err_msg}");
            return ctx.fail_with_fetch(
                fetch,
                err_msg,
                FailureCategory::ArchUnsupported,
                None,
                Some(arch.to_string()),
                None,
            );
        }
    }

    let mut warnings = plan.warnings;
    warnings.extend(fetch.fetch_warnings.clone());

    if !plan.critical_disabled.is_empty() {
        let crit_list = plan.critical_disabled.join(", ");
        let err_msg = format!(
            "{}: critical patch(es) [{crit_list}] were auto-disabled as incompatible; refusing degraded build",
            ctx.id
        );
        let mut rep = ctx.base_report(Some(fetch));
        rep.patches_included = plan.included;
        rep.patches_auto_disabled_count = plan.auto_disabled.len();
        rep.critical_patches_disabled = plan.critical_disabled;
        rep.patches_auto_disabled = plan.auto_disabled;
        rep.warnings = warnings;
        rep.error = Some(err_msg);
        rep.failure_category = Some(FailureCategory::VersionNotSupported);
        rep.arch_mode = Some(arch_mode);
        return (None, rep);
    }

    let is_resolved_unsupported = fetch.resolved_ver.as_ref().is_some_and(|v| {
        !ctx.patch_supported_list.is_empty()
            && !ctx
                .patch_supported_list
                .iter()
                .any(|s| crate::utils::version::version_eq(s, v))
    });
    if is_resolved_unsupported {
        warnings.push(format!(
            "{}: resolved version v{} is not in patch-supported set {:?}",
            ctx.id,
            fetch.resolved_ver.as_deref().unwrap_or(""),
            ctx.patch_supported_list
        ));
    }
    if let (Some(ref newest), Some(ref res)) = (&ctx.newest_supported, &fetch.resolved_ver) {
        if !crate::utils::version::version_eq(newest, res) {
            warnings.push(format!(
                "{}: resolved version v{res} is not the newest supported version v{newest}",
                ctx.id
            ));
        }
    }
    if let Some(min_p) = ctx.app.expected_min_patches {
        if plan.included.len() < min_p {
            let warn_msg = format!(
                "{}: included patches count ({}) is below expected minimum ({min_p})",
                ctx.id,
                plan.included.len()
            );
            tracing::warn!("{warn_msg}");
            warnings.push(warn_msg);
        }
    }

    let status = if ctx.effective_mode != "install"
        && ctx.app.patch_source != "none"
        && !ctx.app.patch_source.is_empty()
        && plan.included.is_empty()
    {
        warnings.push(format!(
            "{}: app configured with mode='patch' but 0 patches were applied; output APK is signed stock",
            ctx.id
        ));
        crate::report::BuildStatus::SuccessUnpatched
    } else if is_resolved_unsupported || !plan.auto_disabled.is_empty() || is_mirror_degraded {
        crate::report::BuildStatus::Degraded
    } else {
        crate::report::BuildStatus::Success
    };

    let validation_status = match status {
        crate::report::BuildStatus::Success => {
            if integrity_status
                .as_ref()
                .is_none_or(|i| i.package_ok && i.version_ok && i.cert_ok)
            {
                "passed"
            } else {
                "warning"
            }
        }
        crate::report::BuildStatus::Degraded | crate::report::BuildStatus::SuccessUnpatched => {
            "warning"
        }
        crate::report::BuildStatus::Failed => "failed",
        crate::report::BuildStatus::Skipped => "unknown",
    }
    .to_string();

    let mut rep = ctx.base_report(Some(fetch));
    rep.status = status;
    rep.patches_auto_disabled_count = plan.auto_disabled.len();
    rep.patches_included = plan.included;
    rep.critical_patches_disabled = plan.critical_disabled;
    rep.patches_auto_disabled = plan.auto_disabled;
    rep.warnings = warnings;
    rep.arch_mode = Some(arch_mode.clone());
    rep.output_file = std::path::Path::new(&out_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);
    rep.output_path = Some(out_path.clone());
    rep.root_payload_path = root_payload_path;
    rep.sha256 = crate::utils::apk::compute_file_sha256(&out_path)
        .ok()
        .or_else(|| fetch.file_sha256.clone());
    rep.size_bytes = std::fs::metadata(&out_path).ok().map(|m| m.len());
    rep.optimization_mode = Some(normalize_optimization_mode(ctx.effective_mode).to_string());
    rep.supported_archs = detect_supported_archs(&out_path, &arch_mode);
    rep.validation_status = Some(validation_status);
    rep.integrity = integrity_status;
    (Some(out_path), rep)
}

async fn build_single(req: &BuildSingleReq<'_>) -> (Option<String>, AppBuildReport) {
    let ctx = resolve_build_ctx(req.cfg, req.id, req.app, req.cli_mode).await;
    if let Some(early) = preflight_patch_checks(&ctx) {
        return early;
    }

    let mut fetch = match fetch_build_input_apk(
        req.cfg,
        &ctx,
        req.arch,
        req.force,
        req.breaker_registry.clone(),
        req.net,
    )
    .await
    {
        Ok(f) => f,
        Err(rep) => return (None, *rep),
    };

    let (integrity_status, is_mirror_degraded) = match verify_fetched_raw_apk(&ctx, &mut fetch) {
        Ok(v) => v,
        Err(rep) => return (None, *rep),
    };

    if ctx.effective_mode == "install"
        || req.app.patch_source == "none"
        || req.app.patch_source.is_empty()
    {
        return finalize_install_mode_build(
            &ctx,
            &fetch,
            integrity_status,
            req.arch,
            req.output_dir,
        );
    }

    let (arch_mode, strip_arch, patch_res, root_payload_path) =
        execute_patch_variants(req.cfg, &ctx, req.arch, req.output_dir, &fetch.raw_apk).await;

    finalize_patched_build(
        &ctx,
        &fetch,
        PatchRunOutcome {
            integrity_status,
            is_mirror_degraded,
            arch: req.arch,
            arch_mode,
            strip_arch,
            patch_res,
            root_payload_path,
        },
    )
}
