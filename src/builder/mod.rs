pub mod arch;
pub mod lite;

use crate::config::{AppConfig, Config};
use crate::patcher::metadata;
use crate::sources::apkpure::resolve_apkpure_cdn_url;
use crate::sources::common::download_and_extract_if_bundle;
use anyhow::Result;
use regex::Regex;
use reqwest::Client;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tracing::{debug, info, warn};

pub async fn resolve_all_patch_compatible_versions(
    cfg: &Config,
    app: &AppConfig,
) -> metadata::CompatibleVersions {
    let Some(mpp_path) = metadata::resolve_mpp_path(cfg, app) else {
        return metadata::CompatibleVersions::Unknown;
    };
    let cli_jar = format!("{}/morphe-cli.jar", cfg.build.tools_dir);
    if !Path::new(&cli_jar).exists() {
        return metadata::CompatibleVersions::Unknown;
    }
    let patches = match metadata::load(cfg, &mpp_path).await {
        Ok(p) => p,
        Err(_) => return metadata::CompatibleVersions::Unknown,
    };
    let enabled = app
        .included_patches
        .iter()
        .chain(app.patches.iter())
        .cloned()
        .collect::<Vec<_>>();
    metadata::resolve_compatible_versions(&patches, &app.package, &enabled, &app.excluded_patches)
}

#[cfg(test)]
pub(crate) async fn resolve_patch_compatible_version(
    cfg: &Config,
    app: &AppConfig,
) -> Option<String> {
    match resolve_all_patch_compatible_versions(cfg, app).await {
        metadata::CompatibleVersions::Intersection(v) => crate::utils::version::max_version(&v),
        metadata::CompatibleVersions::Conflict { patch_versions } => {
            let smallest = patch_versions
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .min_by_key(|(_, v)| v.len())
                .map(|(_, v)| v.as_slice())
                .unwrap_or(&[]);
            crate::utils::version::max_version(smallest)
        }
        metadata::CompatibleVersions::Unconstrained | metadata::CompatibleVersions::Unknown => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum CacheCheckResult {
    Hit(String),
    Stale { cached: String, target: String },
    Miss,
}

pub fn check_cached_input_apk(
    id: &str,
    dest_candidates: &[&str],
    target_ver: Option<&str>,
    force: bool,
) -> CacheCheckResult {
    if force {
        for p in dest_candidates {
            if Path::new(p).exists() {
                info!("{id}: --force: removing cached input APK {p}");
                let _ = std::fs::remove_file(p);
            }
            let _ = std::fs::remove_file(format!("{p}.version"));
            let _ = std::fs::remove_file(format!("{p}.strategy"));
        }
        return CacheCheckResult::Miss;
    }

    for p in dest_candidates {
        if Path::new(p).exists() {
            if let Ok(meta) = std::fs::metadata(p) {
                if meta.len() > 1_000_000 {
                    let sidecar = format!("{p}.version");
                    let cached_ver = if let Ok(s) = std::fs::read_to_string(&sidecar) {
                        let trimmed = s.trim().to_string();
                        if !trimmed.is_empty() {
                            Some(trimmed)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let cached_ver = match cached_ver {
                        Some(v) => Some(v),
                        None => {
                            let extracted = crate::utils::apk::get_apk_version_name(p).ok();
                            if let Some(ref v) = extracted {
                                let _ = std::fs::write(&sidecar, v);
                            }
                            extracted
                        }
                    };

                    match target_ver {
                        None => {
                            info!("{id}: cached input APK exists at {p}");
                            return CacheCheckResult::Hit(p.to_string());
                        }
                        Some(target) => {
                            if let Some(ref cached) = cached_ver {
                                if cached == target {
                                    info!("{id}: cached input APK version v{cached} matches target v{target} at {p}");
                                    return CacheCheckResult::Hit(p.to_string());
                                } else {
                                    info!("{id}: cached input is v{cached}, target is v{target} — cache is stale, refetching");
                                    for clean_p in dest_candidates {
                                        if Path::new(clean_p).exists() {
                                            let _ = std::fs::remove_file(clean_p);
                                        }
                                        let _ = std::fs::remove_file(format!("{clean_p}.version"));
                                        let _ = std::fs::remove_file(format!("{clean_p}.strategy"));
                                    }
                                    return CacheCheckResult::Stale {
                                        cached: cached.clone(),
                                        target: target.to_string(),
                                    };
                                }
                            } else {
                                info!("{id}: cached input APK at {p} has unreadable version — treating as stale");
                                for clean_p in dest_candidates {
                                    if Path::new(clean_p).exists() {
                                        let _ = std::fs::remove_file(clean_p);
                                    }
                                    let _ = std::fs::remove_file(format!("{clean_p}.version"));
                                    let _ = std::fs::remove_file(format!("{clean_p}.strategy"));
                                }
                                return CacheCheckResult::Stale {
                                    cached: "unknown".to_string(),
                                    target: target.to_string(),
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    CacheCheckResult::Miss
}

pub fn resolve_app_version_and_strategy(
    id: &str,
    app: &AppConfig,
    patch_meta: Option<&[metadata::PatchMeta]>,
    patch_supported: &metadata::CompatibleVersions,
) -> (Option<String>, Vec<String>, String) {
    let configured_pin = app
        .version
        .as_deref()
        .filter(|v| !v.eq_ignore_ascii_case("auto"))
        .or_else(|| {
            app.max_version
                .as_deref()
                .filter(|v| !v.eq_ignore_ascii_case("auto"))
        })
        .or_else(|| {
            app.min_version
                .as_deref()
                .filter(|v| !v.eq_ignore_ascii_case("auto"))
        });

    let available_versions = match patch_supported {
        metadata::CompatibleVersions::Intersection(v) => v.clone(),
        metadata::CompatibleVersions::Conflict { patch_versions } => {
            let conflict_str = patch_versions
                .iter()
                .map(|(p, vs)| format!("{p}: [{}]", vs.join(", ")))
                .collect::<Vec<_>>()
                .join("; ");
            warn!("{id}: conflicting patch version constraints: {conflict_str}");
            patch_versions
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .min_by_key(|(_, v)| v.len())
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
        metadata::CompatibleVersions::Unconstrained | metadata::CompatibleVersions::Unknown => {
            Vec::new()
        }
    };

    if let Some(pin) = configured_pin {
        let is_stale = if let Some(meta) = patch_meta {
            metadata::check_app_version_compatibility(id, app, meta).is_some()
                || (!available_versions.is_empty()
                    && !available_versions
                        .iter()
                        .any(|v| crate::utils::version::version_eq(v, pin)))
        } else if !available_versions.is_empty() {
            !available_versions
                .iter()
                .any(|v| crate::utils::version::version_eq(v, pin))
        } else {
            false
        };

        let policy = app.effective_pin_policy();
        if is_stale {
            match policy {
                crate::config::apps::PinPolicy::Strict => {
                    warn!(
                        "{id}: pinned version v{pin} falls outside compatible patch range, but pin_policy=strict — honoring pin to validate/fail"
                    );
                    (
                        Some(pin.to_string()),
                        vec![pin.to_string()],
                        "strict-honored-despite-stale".to_string(),
                    )
                }
                crate::config::apps::PinPolicy::PreferSupported => {
                    if available_versions.is_empty() {
                        warn!(
                            "{id}: pinned version v{pin} falls outside compatible patch range, but no compatible versions available — building degraded"
                        );
                        (
                            Some(pin.to_string()),
                            vec![pin.to_string()],
                            "pinned-degraded".to_string(),
                        )
                    } else {
                        warn!(
                            "{id}: pinned version v{pin} falls outside compatible patch range and pin_policy=prefer_supported — auto-correcting to compatible version"
                        );
                        let auto_ver = crate::utils::version::max_version(&available_versions);
                        let mut candidates: Vec<String> = available_versions;
                        candidates.reverse();
                        (
                            auto_ver,
                            candidates,
                            "auto-corrected-from-stale-pin".to_string(),
                        )
                    }
                }
                crate::config::apps::PinPolicy::Force => {
                    warn!(
                        "{id}: pinned version v{pin} falls outside compatible patch range, but pin_policy=force — forcing pinned version as degraded"
                    );
                    (
                        Some(pin.to_string()),
                        vec![pin.to_string()],
                        "forced-pinned".to_string(),
                    )
                }
            }
        } else {
            let mut candidates = vec![pin.to_string()];
            if policy != crate::config::apps::PinPolicy::Strict {
                for v in available_versions.into_iter().rev() {
                    if v != pin && !candidates.contains(&v) {
                        candidates.push(v);
                    }
                }
            }
            (Some(pin.to_string()), candidates, "pinned".to_string())
        }
    } else {
        let auto_ver = crate::utils::version::max_version(&available_versions);
        let mut candidates: Vec<String> = available_versions;
        candidates.reverse();
        (auto_ver, candidates, "auto".to_string())
    }
}

pub async fn download_with_fallback<F, Fut>(
    id: &str,
    pkg: &str,
    candidates: &[Option<String>],
    max_attempts: usize,
    mut try_fetch: F,
) -> Result<String>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<String>>,
{
    let attempts: Vec<&Option<String>> = candidates.iter().take(max_attempts.max(1)).collect();
    let total = attempts.len();
    let mut last_attempted: Option<String> = None;

    for (idx, cand) in attempts.iter().enumerate() {
        let cur = cand.as_deref();
        if idx > 0 {
            let prev_str = last_attempted.as_deref().unwrap_or("unknown");
            let cur_str = cur.unwrap_or("auto");
            info!("{id}: v{prev_str} unavailable on any source, trying v{cur_str}...");
        }
        last_attempted = (*cand).clone();

        match try_fetch((*cand).clone()).await {
            Ok(path) => return Ok(path),
            Err(e) => {
                debug!("{id}: version {:?} failed on all sources: {e}", cur);
                if e.to_string()
                    .contains("all_candidate_sources_open_breakers")
                {
                    info!("{id}: all candidate sources have open circuit breakers — skipping remaining candidate versions");
                    break;
                }
            }
        }
    }

    anyhow::bail!(
        "{id}: all download sources exhausted for package '{pkg}' across {total} candidate version(s)"
    );
}

#[derive(Debug, Clone)]
pub struct FetchApkSuccess {
    pub path: String,
    pub final_version: Option<String>,
    pub attempts: Vec<crate::orchestrator::scheduler::SourceAttemptRecord>,
    pub warnings: Vec<String>,
    pub requested_version: Option<String>,
    pub candidates_tried: Vec<String>,
}

#[derive(Debug)]
pub struct FetchApkError {
    pub message: String,
    pub attempts: Vec<crate::orchestrator::scheduler::SourceAttemptRecord>,
    pub warnings: Vec<String>,
    pub requested_version: Option<String>,
    pub candidates_tried: Vec<String>,
}

impl std::fmt::Display for FetchApkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FetchApkError {}

type SharedAttempts =
    Arc<std::sync::Mutex<Vec<crate::orchestrator::scheduler::SourceAttemptRecord>>>;

struct AttemptArgs<'a> {
    source: &'a str,
    outcome: &'a str,
    version: Option<String>,
    url: Option<String>,
    reason: String,
    duration_ms: u64,
    breaker_state: Option<String>,
    block_kind: Option<crate::sources::breaker::BlockKind>,
}

fn make_attempt(args: AttemptArgs<'_>) -> crate::orchestrator::scheduler::SourceAttemptRecord {
    crate::orchestrator::scheduler::SourceAttemptRecord {
        source: args.source.to_string(),
        outcome: args.outcome.to_string(),
        version: args.version,
        url: args.url,
        reason: args.reason,
        duration_ms: args.duration_ms,
        breaker_state_at_attempt: args.breaker_state.clone(),
        breaker_state: args.breaker_state,
        breaker_final_state: None,
        block_kind: args.block_kind,
    }
}

fn finalize_attempts(
    effective_registry: &crate::sources::breaker::BreakerRegistry,
    mut atts: Vec<crate::orchestrator::scheduler::SourceAttemptRecord>,
) -> Vec<crate::orchestrator::scheduler::SourceAttemptRecord> {
    for att in &mut atts {
        if att.breaker_state_at_attempt.is_none() {
            att.breaker_state_at_attempt = att.breaker_state.clone();
        }
        att.breaker_final_state = Some(effective_registry.get(&att.source).state().to_string());
    }
    atts
}

fn version_acceptable_for_app(
    actual_ver: &str,
    target_ver: Option<&str>,
    patch_supported_versions: &[String],
    is_strictly_pinned: bool,
) -> bool {
    let matches_target =
        target_ver.is_some_and(|t| crate::utils::version::version_eq(actual_ver, t));
    let in_patch_compat = !patch_supported_versions.is_empty()
        && patch_supported_versions
            .iter()
            .any(|v| crate::utils::version::version_eq(actual_ver, v));
    if is_strictly_pinned {
        matches_target
    } else {
        matches_target || in_patch_compat
    }
}

struct DirectUrlCtx<'a> {
    cfg: &'a Config,
    id: &'a str,
    app: &'a AppConfig,
    arch: &'a str,
    dest: &'a str,
    target_ver: Option<&'a str>,
    patch_supported_versions: &'a [String],
    is_strictly_pinned: bool,
    strategy_str: &'a str,
    client: &'a Client,
    no_redirect_client: &'a Client,
    warnings: &'a [String],
}

async fn verify_direct_downloaded_apk(
    dctx: &DirectUrlCtx<'_>,
    final_url: String,
    dur: u64,
    all_attempts: &mut Vec<crate::orchestrator::scheduler::SourceAttemptRecord>,
) -> Option<FetchApkSuccess> {
    let dest_inspect = dctx.dest.to_string();
    let (integrity_res, actual_ver_res, actual_pkg_res) = tokio::task::spawn_blocking(move || {
        let integrity = crate::utils::apk::verify_apk_integrity(&dest_inspect);
        if integrity.is_err() {
            return (
                integrity,
                Err(anyhow::anyhow!("skipped")),
                Err(anyhow::anyhow!("skipped")),
            );
        }
        (
            integrity,
            crate::utils::apk::get_apk_version_name(&dest_inspect),
            crate::utils::apk::get_apk_package_name(&dest_inspect),
        )
    })
    .await
    .ok()?;

    if let Err(integrity_err) = integrity_res {
        warn!(
            "{}: direct apk_url file failed integrity check: {integrity_err}",
            dctx.id
        );
        let _ = tokio::fs::remove_file(dctx.dest).await;
        all_attempts.push(make_attempt(AttemptArgs {
            source: "direct",
            outcome: "bad_zip",
            version: None,
            url: Some(final_url),
            reason: integrity_err.to_string(),
            duration_ms: dur,
            breaker_state: None,
            block_kind: None,
        }));
        return None;
    }

    let package_matches = actual_pkg_res
        .as_ref()
        .map(|pkg| dctx.app.package.is_empty() || pkg == &dctx.app.package)
        .unwrap_or(true);
    let is_install_mode = dctx.target_ver.is_none()
        && (dctx.app.mode == "install"
            || dctx.app.patch_source == "none"
            || dctx.app.patch_source.is_empty()
            || dctx.patch_supported_versions.is_empty());
    let is_acceptable = package_matches
        && actual_ver_res.as_ref().is_ok_and(|actual_ver| {
            is_install_mode
                || version_acceptable_for_app(
                    actual_ver,
                    dctx.target_ver,
                    dctx.patch_supported_versions,
                    dctx.is_strictly_pinned,
                )
        });

    if is_acceptable {
        let actual_ver = actual_ver_res.ok();
        let v_write = actual_ver
            .clone()
            .or_else(|| dctx.target_ver.map(str::to_string));
        if let Some(ref v) = v_write {
            crate::utils::warn_io(
                tokio::fs::write(format!("{}.version", dctx.dest), v).await,
                &format!("Writing version sidecar for {}", dctx.dest),
            );
        }
        crate::utils::warn_io(
            tokio::fs::write(format!("{}.strategy", dctx.dest), dctx.strategy_str).await,
            &format!("Writing strategy sidecar for {}", dctx.dest),
        );
        all_attempts.push(make_attempt(AttemptArgs {
            source: "direct",
            outcome: "hit",
            version: actual_ver.clone(),
            url: Some(final_url),
            reason: "downloaded successfully via direct apk_url".to_string(),
            duration_ms: dur,
            breaker_state: None,
            block_kind: None,
        }));
        return Some(FetchApkSuccess {
            path: dctx.dest.to_string(),
            final_version: actual_ver,
            attempts: all_attempts.clone(),
            warnings: dctx.warnings.to_vec(),
            requested_version: dctx.target_ver.map(str::to_string),
            candidates_tried: dctx
                .target_ver
                .map(|v| vec![v.to_string()])
                .unwrap_or_default(),
        });
    }

    let act_str = actual_ver_res.unwrap_or_else(|_| "unknown".to_string());
    let (outcome_str, reason_str) = if !package_matches {
        let act_pkg = actual_pkg_res.unwrap_or_else(|_| "unknown".to_string());
        warn!(
            "{}: direct apk_url gave package '{act_pkg}' which does not match expected '{}' — discarding",
            dctx.id, dctx.app.package
        );
        (
            "package_mismatch",
            format!(
                "downloaded package '{act_pkg}' does not match expected '{}'",
                dctx.app.package
            ),
        )
    } else {
        warn!(
            "{}: direct apk_url gave version v{act_str} which is not compatible with target {:?} or patches — discarding and trying next sources",
            dctx.id, dctx.target_ver
        );
        (
            "version_mismatch",
            format!(
                "downloaded v{act_str} does not match target {:?} and is not in patch supported versions",
                dctx.target_ver
            ),
        )
    };
    let _ = tokio::fs::remove_file(dctx.dest).await;
    all_attempts.push(make_attempt(AttemptArgs {
        source: "direct",
        outcome: outcome_str,
        version: Some(act_str),
        url: Some(final_url),
        reason: reason_str,
        duration_ms: dur,
        breaker_state: None,
        block_kind: None,
    }));
    None
}

async fn try_direct_apk_url(
    dctx: &DirectUrlCtx<'_>,
    url: &str,
    all_attempts: &mut Vec<crate::orchestrator::scheduler::SourceAttemptRecord>,
) -> Option<FetchApkSuccess> {
    let t0 = std::time::Instant::now();
    let final_url = if url.contains("github.com") && url.contains("/releases/") {
        resolve_github_asset_url(
            dctx.client,
            url,
            dctx.arch,
            &dctx.cfg.build.asset_exclude_patterns,
        )
        .await
        .unwrap_or_else(|_| url.to_string())
    } else if url.contains("d.apkpure.") {
        resolve_apkpure_cdn_url(
            Some(dctx.no_redirect_client),
            url,
            Some("https://apkpure.com/"),
        )
        .await
    } else {
        url.to_string()
    };
    let referer = url.contains("apkpure.").then_some("https://apkpure.com/");

    if let Some(ref pv) = crate::utils::version::extract_version_from_url_or_filename(&final_url) {
        let acceptable = dctx.target_ver.is_none()
            || version_acceptable_for_app(
                pv,
                dctx.target_ver,
                dctx.patch_supported_versions,
                dctx.is_strictly_pinned,
            );
        if !acceptable {
            warn!(
                "{}: direct apk_url points to v{pv} which is not compatible with target {:?} or patches — skipping direct download to avoid version mismatch",
                dctx.id, dctx.target_ver
            );
            all_attempts.push(make_attempt(AttemptArgs {
                source: "direct",
                outcome: "version_mismatch",
                version: Some(pv.clone()),
                url: Some(final_url),
                reason: format!(
                    "probed url v{pv} does not match target {:?} and is not in patch supported versions",
                    dctx.target_ver
                ),
                duration_ms: 0,
                breaker_state: None,
                block_kind: None,
            }));
            return None;
        }
    }

    info!("{}: downloading from direct apk_url {final_url}", dctx.id);
    if let Err(e) =
        download_and_extract_if_bundle(dctx.client, &final_url, dctx.dest, referer).await
    {
        warn!("{}: direct apk_url failed: {e}", dctx.id);
        all_attempts.push(make_attempt(AttemptArgs {
            source: "direct",
            outcome: "error",
            version: None,
            url: Some(final_url),
            reason: e.to_string(),
            duration_ms: t0.elapsed().as_millis() as u64,
            breaker_state: None,
            block_kind: None,
        }));
        return None;
    }

    let dur = t0.elapsed().as_millis() as u64;
    verify_direct_downloaded_apk(dctx, final_url, dur, all_attempts).await
}

struct FetchRunCtx<'a> {
    cfg: &'a Config,
    id: &'a str,
    app: &'a AppConfig,
    arch: &'a str,
    dest: &'a str,
    target_ver: Option<&'a str>,
    patch_supported_versions: &'a [String],
    is_strictly_pinned: bool,
    strategy_str: &'a str,
    client: &'a Client,
    no_redirect_client: &'a Client,
    fetcher: &'a crate::net::Fetcher,
    effective_registry: &'a Arc<crate::sources::breaker::BreakerRegistry>,
    shared_attempts: &'a SharedAttempts,
    candidates_tried: &'a Arc<std::sync::Mutex<Vec<String>>>,
}

async fn probe_candidate_versions(
    rctx: &FetchRunCtx<'_>,
    ordered: &[&dyn crate::sources::ApkSource],
    mut candidates_to_try: Vec<Option<String>>,
) -> Vec<Option<String>> {
    if candidates_to_try.len() <= 1 {
        return candidates_to_try;
    }
    let probe_ctx = crate::sources::SourceCtx {
        client: rctx.client,
        no_redirect_client: Some(rctx.no_redirect_client),
        cfg: rctx.cfg,
        fetcher: Some(rctx.fetcher),
        breaker_registry: rctx.effective_registry.clone(),
        app_id: Some(rctx.id.to_string()),
    };
    let probe_futs = candidates_to_try.iter().map(|cand| {
        let c_val = cand.clone();
        let c_ctx = &probe_ctx;
        async move {
            for src in ordered {
                if let Ok(st) = c_ctx.can_try(src.name()) {
                    if st != crate::sources::breaker::BreakerState::Open
                        && src
                            .resolve(c_ctx, rctx.app, c_val.as_deref(), rctx.arch)
                            .await
                            .is_ok_and(|o| o.is_some())
                    {
                        return (c_val, true);
                    }
                }
            }
            (c_val, false)
        }
    });
    let available: Vec<Option<String>> = futures::future::join_all(probe_futs)
        .await
        .into_iter()
        .filter_map(|(cand, found)| found.then_some(cand))
        .collect();
    if !available.is_empty() {
        info!(
            "{}: parallel candidate availability probe found {} available version(s)",
            rctx.id,
            available.len()
        );
        let mut reordered = available;
        for c in &candidates_to_try {
            if !reordered.contains(c) {
                reordered.push(c.clone());
            }
        }
        candidates_to_try = reordered;
    }
    candidates_to_try
}

fn outcome_for_block_kind(block_kind: &crate::sources::breaker::BlockKind) -> &'static str {
    match block_kind {
        crate::sources::breaker::BlockKind::TurnstileChallenge
        | crate::sources::breaker::BlockKind::IpReputationBlock => "blocked",
        crate::sources::breaker::BlockKind::RateLimited => "rate_limited",
        crate::sources::breaker::BlockKind::NotFound => "not_found",
        _ => "error",
    }
}

async fn inspect_downloaded_source_apk(
    path: std::path::PathBuf,
    arch: String,
    exp_certs: Vec<String>,
) -> Result<Result<String, (&'static str, String)>> {
    tokio::task::spawn_blocking(move || {
        if let Err(integrity_err) = crate::utils::apk::verify_apk_integrity(&path) {
            return Err(("bad_zip", integrity_err.to_string()));
        }
        let p_str = path.to_string_lossy();
        if !crate::config::apps::Arch::parse(&arch).is_wildcard() {
            let has_any = arch::apk_has_any_native_libs(&p_str).unwrap_or(false);
            let has_target = arch::apk_has_arch(&p_str, &arch).unwrap_or(false);
            if has_any && !has_target {
                return Err((
                    "not_found",
                    format!("native libs missing for arch '{arch}'"),
                ));
            }
        }
        if !exp_certs.is_empty() {
            let matched = exp_certs.iter().any(|expected_cert| {
                crate::utils::keystore::verify_apk_cert_sha256(&p_str, expected_cert)
                    .unwrap_or(false)
            });
            if !matched {
                return Err(("error", "cert SHA-256 mismatch".to_string()));
            }
        }
        crate::utils::apk::get_apk_version_name(&path)
            .map_err(|e| ("error", format!("failed to read APK version name: {e}")))
    })
    .await
    .map_err(|e| anyhow::anyhow!("apk inspect task panicked: {e}"))
}

struct SourceAttemptMeta<'a> {
    sname: &'a str,
    cur_state: crate::sources::breaker::BreakerState,
    cur_target: Option<&'a str>,
    dur: u64,
}

fn record_source_failure_attempt(
    ctx: &crate::sources::SourceCtx<'_>,
    rctx: &FetchRunCtx<'_>,
    meta: &SourceAttemptMeta<'_>,
    url: Option<String>,
    err: anyhow::Error,
    phase: &str,
) {
    let err_msg = err.to_string();
    let block_kind = crate::sources::breaker::classify_error(&err_msg);
    ctx.record_failure(meta.sname, &block_kind);
    if block_kind == crate::sources::breaker::BlockKind::NotFound {
        crate::utils::cache::record_negative_cache_file(
            &rctx.cfg.build.version_cache,
            meta.sname,
            &rctx.app.package,
            meta.cur_target,
        );
    }
    crate::utils::lock_unpoisoned(rctx.shared_attempts).push(make_attempt(AttemptArgs {
        source: meta.sname,
        outcome: outcome_for_block_kind(&block_kind),
        version: meta.cur_target.map(str::to_string),
        url,
        reason: err_msg.clone(),
        duration_ms: meta.dur,
        breaker_state: Some(meta.cur_state.to_string()),
        block_kind: Some(block_kind),
    }));
    warn!("{}: {} {phase} failed: {err_msg}", rctx.id, meta.sname);
}

async fn verify_and_record_source_apk(
    ctx: &crate::sources::SourceCtx<'_>,
    rctx: &FetchRunCtx<'_>,
    meta: &SourceAttemptMeta<'_>,
    r_url: String,
    path: std::path::PathBuf,
) -> Result<Option<String>> {
    let actual_ver = match inspect_downloaded_source_apk(
        path.clone(),
        rctx.arch.to_string(),
        rctx.app.expected_certs(),
    )
    .await?
    {
        Ok(ver) => ver,
        Err((outcome, reason)) => {
            if outcome == "bad_zip" {
                warn!(
                    "{}: APK from {} failed integrity check: {reason}",
                    rctx.id, meta.sname
                );
            } else if outcome == "not_found" {
                warn!(
                    "{}: APK from {} {reason}, discarding and trying next source",
                    rctx.id, meta.sname
                );
            } else {
                warn!(
                    "{}: {reason} for source {}, discarding and trying next source",
                    rctx.id, meta.sname
                );
            }
            let _ = tokio::fs::remove_file(rctx.dest).await;
            crate::utils::lock_unpoisoned(rctx.shared_attempts).push(make_attempt(AttemptArgs {
                source: meta.sname,
                outcome,
                version: meta.cur_target.map(str::to_string),
                url: Some(r_url),
                reason,
                duration_ms: meta.dur,
                breaker_state: Some(meta.cur_state.to_string()),
                block_kind: None,
            }));
            return Ok(None);
        }
    };

    let matches_cur = meta
        .cur_target
        .is_none_or(|tgt| crate::utils::version::version_eq(&actual_ver, tgt));
    let in_patch_compat = !rctx.patch_supported_versions.is_empty()
        && rctx
            .patch_supported_versions
            .iter()
            .any(|v| crate::utils::version::version_eq(&actual_ver, v));

    if matches_cur || (!rctx.is_strictly_pinned && in_patch_compat) {
        ctx.record_success(meta.sname);
        crate::utils::lock_unpoisoned(rctx.shared_attempts).push(make_attempt(AttemptArgs {
            source: meta.sname,
            outcome: "hit",
            version: Some(actual_ver.clone()),
            url: Some(r_url),
            reason: "downloaded and verified successfully".to_string(),
            duration_ms: meta.dur,
            breaker_state: Some(meta.cur_state.to_string()),
            block_kind: None,
        }));
        info!(
            "{}: BUILD_REPORT resolved_source={} actual_ver={actual_ver}",
            rctx.id, meta.sname
        );
        return Ok(Some(path.to_string_lossy().to_string()));
    }

    let _ = tokio::fs::remove_file(rctx.dest).await;
    let tgt_name = meta.cur_target.unwrap_or("auto");
    crate::utils::cache::record_negative_cache_file(
        &rctx.cfg.build.version_cache,
        meta.sname,
        &rctx.app.package,
        meta.cur_target,
    );
    crate::utils::lock_unpoisoned(rctx.shared_attempts).push(make_attempt(AttemptArgs {
        source: meta.sname,
        outcome: "version_mismatch",
        version: Some(actual_ver.clone()),
        url: Some(r_url),
        reason: format!(
            "downloaded v{actual_ver} does not match requested v{tgt_name} and is not in patch supported versions"
        ),
        duration_ms: meta.dur,
        breaker_state: Some(meta.cur_state.to_string()),
        block_kind: None,
    }));
    Ok(None)
}

async fn try_source_for_candidate(
    ctx: &crate::sources::SourceCtx<'_>,
    rctx: &FetchRunCtx<'_>,
    src: &dyn crate::sources::ApkSource,
    cur_target: Option<&str>,
) -> Result<Option<String>> {
    let sname = src.name();
    let t_start = std::time::Instant::now();
    info!("{}: checking source {sname}", rctx.id);

    if crate::utils::cache::is_negative_cached_file(
        &rctx.cfg.build.version_cache,
        sname,
        &rctx.app.package,
        cur_target,
    ) {
        crate::utils::lock_unpoisoned(rctx.shared_attempts).push(make_attempt(AttemptArgs {
            source: sname,
            outcome: "skipped-negative-cache",
            version: cur_target.map(str::to_string),
            url: None,
            reason: "negative cache hit: previous miss recorded".to_string(),
            duration_ms: 0,
            breaker_state: None,
            block_kind: Some(crate::sources::breaker::BlockKind::NotFound),
        }));
        return Ok(None);
    }

    let cur_state = match ctx.can_try(sname) {
        Ok(st) => st,
        Err((st, reason)) => {
            crate::utils::lock_unpoisoned(rctx.shared_attempts).push(make_attempt(AttemptArgs {
                source: sname,
                outcome: "skipped-breaker",
                version: cur_target.map(str::to_string),
                url: None,
                reason,
                duration_ms: t_start.elapsed().as_millis() as u64,
                breaker_state: Some(st.to_string()),
                block_kind: None,
            }));
            return Ok(None);
        }
    };

    let resolved = match src.resolve(ctx, rctx.app, cur_target, rctx.arch).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            if !matches!(
                sname,
                "apkeep" | "apkmirror" | "uptodown" | "archive" | "aptoide"
            ) {
                crate::utils::cache::record_negative_cache_file(
                    &rctx.cfg.build.version_cache,
                    sname,
                    &rctx.app.package,
                    cur_target,
                );
            }
            crate::utils::lock_unpoisoned(rctx.shared_attempts).push(make_attempt(AttemptArgs {
                source: sname,
                outcome: "not_found",
                version: cur_target.map(str::to_string),
                url: None,
                reason: "source not configured or release not found".to_string(),
                duration_ms: t_start.elapsed().as_millis() as u64,
                breaker_state: Some(cur_state.to_string()),
                block_kind: Some(crate::sources::breaker::BlockKind::NotFound),
            }));
            return Ok(None);
        }
        Err(e) => {
            let meta = SourceAttemptMeta {
                sname,
                cur_state,
                cur_target,
                dur: t_start.elapsed().as_millis() as u64,
            };
            record_source_failure_attempt(ctx, rctx, &meta, None, e, "resolve");
            return Ok(None);
        }
    };

    let r_url = resolved.url.clone();
    let path = match src.fetch(ctx, &resolved, rctx.dest).await {
        Ok(p) => p,
        Err(e) => {
            let meta = SourceAttemptMeta {
                sname,
                cur_state,
                cur_target,
                dur: t_start.elapsed().as_millis() as u64,
            };
            record_source_failure_attempt(ctx, rctx, &meta, Some(r_url), e, "fetch");
            return Ok(None);
        }
    };

    let meta = SourceAttemptMeta {
        sname,
        cur_state,
        cur_target,
        dur: t_start.elapsed().as_millis() as u64,
    };
    verify_and_record_source_apk(ctx, rctx, &meta, r_url, path).await
}

async fn validate_and_finalize_fetched_apk(
    rctx: &FetchRunCtx<'_>,
    result_path: String,
    warnings: Vec<String>,
) -> Result<FetchApkSuccess> {
    let dest_ver = rctx.dest.to_string();
    let actual_ver = tokio::task::spawn_blocking(move || {
        crate::utils::apk::get_apk_version_name(&dest_ver).ok()
    })
    .await
    .unwrap_or(None);

    let version_matches_target = rctx.target_ver.is_some_and(|target| {
        actual_ver
            .as_ref()
            .is_some_and(|v| crate::utils::version::version_eq(v, target))
    });
    let version_in_patch_compat = !rctx.patch_supported_versions.is_empty()
        && actual_ver.as_ref().is_some_and(|v| {
            rctx.patch_supported_versions
                .iter()
                .any(|pv| crate::utils::version::version_eq(v, pv))
        });

    let discard_with_error = |message: String| async {
        let _ = tokio::fs::remove_file(rctx.dest).await;
        let _ = tokio::fs::remove_file(format!("{}.version", rctx.dest)).await;
        let _ = tokio::fs::remove_file(format!("{}.strategy", rctx.dest)).await;
        Err(anyhow::Error::new(FetchApkError {
            message,
            attempts: finalize_attempts(
                rctx.effective_registry,
                crate::utils::lock_unpoisoned(rctx.shared_attempts).clone(),
            ),
            warnings: warnings.clone(),
            requested_version: rctx.target_ver.map(str::to_string),
            candidates_tried: crate::utils::lock_unpoisoned(rctx.candidates_tried).clone(),
        }))
    };

    if rctx.is_strictly_pinned && !version_matches_target {
        let act_str = actual_ver.unwrap_or_else(|| "unknown".to_string());
        return discard_with_error(format!(
            "{}: downloaded APK version v{act_str} does not match strictly pinned target v{} — discarded",
            rctx.id,
            rctx.target_ver.unwrap_or("unknown")
        ))
        .await;
    }
    if !rctx.is_strictly_pinned
        && !version_matches_target
        && !version_in_patch_compat
        && rctx.target_ver.is_some()
    {
        let act_str = actual_ver.unwrap_or_else(|| "unknown".to_string());
        return discard_with_error(format!(
            "{}: downloaded APK version v{act_str} does not match target v{} and is not compatible with any enabled patch (supported: {:?}) — discarded",
            rctx.id,
            rctx.target_ver.unwrap_or("unknown"),
            rctx.patch_supported_versions
        ))
        .await;
    }
    if !version_matches_target && version_in_patch_compat {
        let act_str = actual_ver.as_deref().unwrap_or("unknown");
        info!(
            "{}: target was v{}, but mirror provided compatible version v{act_str} supported by patch — accepting fallback",
            rctx.id,
            rctx.target_ver.unwrap_or("auto")
        );
    }

    let final_ver = actual_ver.or_else(|| rctx.target_ver.map(str::to_string));
    if let Some(ref v) = final_ver {
        crate::utils::warn_io(
            tokio::fs::write(format!("{}.version", rctx.dest), v).await,
            &format!("Writing version sidecar for {}", rctx.dest),
        );
    }
    crate::utils::warn_io(
        tokio::fs::write(format!("{}.strategy", rctx.dest), rctx.strategy_str).await,
        &format!("Writing strategy sidecar for {}", rctx.dest),
    );

    Ok(FetchApkSuccess {
        path: result_path,
        final_version: final_ver,
        attempts: finalize_attempts(
            rctx.effective_registry,
            crate::utils::lock_unpoisoned(rctx.shared_attempts).clone(),
        ),
        warnings,
        requested_version: rctx.target_ver.map(str::to_string),
        candidates_tried: crate::utils::lock_unpoisoned(rctx.candidates_tried).clone(),
    })
}

async fn resolve_setup_and_check_cache(
    cfg: &Config,
    id: &str,
    app: &AppConfig,
    dest: &str,
    force: bool,
) -> Result<(
    Option<FetchApkSuccess>,
    Vec<String>,
    Option<String>,
    Vec<String>,
    String,
    bool,
)> {
    let cache_candidates = [
        dest.to_string(),
        format!("{}/{id}-all-input.apk", cfg.build.temp_dir),
        format!("{}/{id}-input.apk", cfg.build.temp_dir),
    ];

    let patch_compat = resolve_all_patch_compatible_versions(cfg, app).await;
    let patch_supported_versions = patch_compat.to_vec();
    let patch_meta = match metadata::resolve_mpp_path(cfg, app) {
        Some(ref p) => metadata::load(cfg, p).await.ok(),
        None => None,
    };
    let (target_ver, candidate_versions, strategy_str) = resolve_app_version_and_strategy(
        id,
        app,
        patch_meta.as_deref().map(|v| v.as_slice()),
        &patch_compat,
    );
    let is_strictly_pinned = app
        .version
        .as_deref()
        .is_some_and(|v| !v.eq_ignore_ascii_case("auto"))
        && app.version_pin_strict;

    let (id_owned, tv_owned) = (id.to_string(), target_ver.clone());
    let cache_result = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = cache_candidates.iter().map(|s| s.as_str()).collect();
        check_cached_input_apk(&id_owned, &refs, tv_owned.as_deref(), force)
    })
    .await
    .map_err(|e| anyhow::anyhow!("cache check task panicked: {e}"))?;

    if let CacheCheckResult::Hit(cached_path) = cache_result {
        crate::utils::warn_io(
            tokio::fs::write(format!("{cached_path}.strategy"), &strategy_str).await,
            &format!("Writing strategy sidecar for {cached_path}"),
        );
        return Ok((
            Some(FetchApkSuccess {
                path: cached_path,
                final_version: target_ver.clone(),
                attempts: vec![],
                warnings: vec![],
                requested_version: target_ver.clone(),
                candidates_tried: target_ver.clone().map(|v| vec![v]).unwrap_or_default(),
            }),
            patch_supported_versions,
            target_ver,
            candidate_versions,
            strategy_str,
            is_strictly_pinned,
        ));
    }

    Ok((
        None,
        patch_supported_versions,
        target_ver,
        candidate_versions,
        strategy_str,
        is_strictly_pinned,
    ))
}

async fn run_fallback_download_loop(
    rctx: &FetchRunCtx<'_>,
    ordered: &[&dyn crate::sources::ApkSource],
    candidates_to_try: &[Option<String>],
) -> Result<String> {
    download_with_fallback(
        rctx.id,
        &rctx.app.package,
        candidates_to_try,
        rctx.cfg.build.version_fallback_attempts,
        |cur_target| {
            if let Some(ref v) = cur_target {
                let mut lock = crate::utils::lock_unpoisoned(rctx.candidates_tried);
                if !lock.contains(v) {
                    lock.push(v.clone());
                }
            }
            async move {
                let ctx = crate::sources::SourceCtx {
                    client: rctx.client,
                    no_redirect_client: Some(rctx.no_redirect_client),
                    cfg: rctx.cfg,
                    fetcher: Some(rctx.fetcher),
                    breaker_registry: rctx.effective_registry.clone(),
                    app_id: Some(rctx.id.to_string()),
                };
                for &src in ordered {
                    if let Some(path) =
                        try_source_for_candidate(&ctx, rctx, src, cur_target.as_deref()).await?
                    {
                        return Ok(path);
                    }
                }
                let all_open = ordered.iter().all(|s| ctx.can_try(s.name()).is_err());
                if all_open {
                    anyhow::bail!(
                        "{}: all sources failed for candidate {cur_target:?} (all_candidate_sources_open_breakers)",
                        rctx.id
                    );
                }
                anyhow::bail!("{}: all sources failed for candidate {cur_target:?}", rctx.id);
            }
        },
    )
    .await
}

pub async fn fetch_apk(
    cfg: &Config,
    id: &str,
    app: &AppConfig,
    arch: &str,
    force: bool,
    breaker_registry: Option<Arc<crate::sources::breaker::BreakerRegistry>>,
    net: &crate::net::SharedNet,
) -> Result<FetchApkSuccess> {
    let parsed_arch = crate::config::apps::Arch::parse(arch);
    let dest = format!(
        "{}/{id}-{}-input.apk",
        cfg.build.temp_dir,
        parsed_arch.as_str()
    );

    let (
        cached_hit,
        patch_supported_versions,
        target_ver,
        candidate_versions,
        strategy_str,
        is_strictly_pinned,
    ) = resolve_setup_and_check_cache(cfg, id, app, &dest, force).await?;
    if let Some(hit) = cached_hit {
        return Ok(hit);
    }

    let (client, no_redirect_client, fetcher) = (
        net.browser_client.clone(),
        net.no_redirect_client.clone(),
        net.fetcher.clone(),
    );
    if let Err(e) = fetcher.ensure_probed().await {
        anyhow::bail!("{id}: FlareSolverr health check failed: {e}");
    }
    let mut warnings = Vec::new();
    if fetcher.is_flaresolverr_unavailable() {
        warnings.push("flaresolverr_unavailable".to_string());
    }

    let mut all_attempts = Vec::new();
    if let Some(url) = &app.apk_url {
        let dctx = DirectUrlCtx {
            cfg,
            id,
            app,
            arch,
            dest: &dest,
            target_ver: target_ver.as_deref(),
            patch_supported_versions: &patch_supported_versions,
            is_strictly_pinned,
            strategy_str: &strategy_str,
            client: &client,
            no_redirect_client: &no_redirect_client,
            warnings: &warnings,
        };
        if let Some(hit) = try_direct_apk_url(&dctx, url, &mut all_attempts).await {
            return Ok(hit);
        }
    }

    let registry = crate::sources::default_registry();
    let ordered = crate::sources::source_order(&registry, app, cfg);
    let effective_registry = breaker_registry.unwrap_or_else(|| {
        Arc::new(crate::sources::breaker::BreakerRegistry::new(
            &cfg.circuit_breakers,
        ))
    });
    let shared_attempts: SharedAttempts = Arc::new(std::sync::Mutex::new(all_attempts));
    let candidates_tried: Arc<std::sync::Mutex<Vec<String>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));
    let rctx = FetchRunCtx {
        cfg,
        id,
        app,
        arch,
        dest: &dest,
        target_ver: target_ver.as_deref(),
        patch_supported_versions: &patch_supported_versions,
        is_strictly_pinned,
        strategy_str: &strategy_str,
        client: &client,
        no_redirect_client: &no_redirect_client,
        fetcher: &fetcher,
        effective_registry: &effective_registry,
        shared_attempts: &shared_attempts,
        candidates_tried: &candidates_tried,
    };

    let raw_candidates: Vec<Option<String>> = if candidate_versions.is_empty() {
        vec![target_ver.clone()]
    } else {
        candidate_versions.into_iter().map(Some).collect()
    };
    let candidates_to_try = probe_candidate_versions(&rctx, &ordered, raw_candidates).await;

    let result_path = match run_fallback_download_loop(&rctx, &ordered, &candidates_to_try).await {
        Ok(path) => path,
        Err(e) => {
            return Err(anyhow::Error::new(FetchApkError {
                message: e.to_string(),
                attempts: finalize_attempts(
                    &effective_registry,
                    crate::utils::lock_unpoisoned(&shared_attempts).clone(),
                ),
                warnings,
                requested_version: target_ver.clone(),
                candidates_tried: crate::utils::lock_unpoisoned(&candidates_tried).clone(),
            }));
        }
    };

    validate_and_finalize_fetched_apk(&rctx, result_path, warnings).await
}

async fn resolve_github_asset_url(
    client: &Client,
    url: &str,
    arch: &str,
    exclude_patterns: &[String],
) -> Result<String> {
    static RE_GH: OnceLock<Regex> = OnceLock::new();
    let re = RE_GH.get_or_init(|| {
        Regex::new(r#"github\.com/([^/]+)/([^/]+)/releases"#).expect("valid github url regex")
    });
    if let Some(cap) = re.captures(url) {
        let slug = format!("{}/{}", &cap[1], &cap[2]);
        let release = crate::utils::github::get_latest_release(client, &slug, None).await?;
        let candidates: Vec<String> = release
            .assets
            .into_iter()
            .filter(|a| {
                a.name.ends_with(".apk") && !exclude_patterns.iter().any(|pat| a.name.contains(pat))
            })
            .map(|a| a.browser_download_url)
            .collect();
        if let Some(dl) = candidates.iter().find(|u| u.contains(arch)) {
            return Ok(dl.clone());
        }
        if let Some(dl) = candidates
            .iter()
            .find(|u| u.contains("all") || u.contains("universal"))
        {
            return Ok(dl.clone());
        }
        if let Some(dl) = candidates.first() {
            return Ok(dl.clone());
        }
    }
    Ok(url.to_string())
}
