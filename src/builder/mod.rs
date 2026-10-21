pub mod arch;
pub mod lite;

use crate::config::{AppConfig, Config};
use crate::patcher::metadata;
use anyhow::{Context, Result};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub enum UptodownError {
    TurnstileBlocked,
    IpReputationBlocked,
    TokenNotFound { html_len: usize, snippet: String },
    Network(anyhow::Error),
}

impl std::fmt::Display for UptodownError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UptodownError::TurnstileBlocked => {
                write!(
                    f,
                    "Uptodown: Cloudflare Turnstile challenge detected (domain globally blocked)"
                )
            }
            UptodownError::IpReputationBlocked => {
                write!(f, "Uptodown: IP reputation block (403/1020) detected")
            }
            UptodownError::TokenNotFound { html_len, snippet } => {
                write!(f, "Uptodown: data-url token not found (page {html_len} bytes); snippet: {snippet}")
            }
            UptodownError::Network(e) => write!(f, "Uptodown: network error: {e}"),
        }
    }
}

impl std::error::Error for UptodownError {}

#[derive(Debug)]
pub enum ApkpureError {
    Forbidden { url: String },
    Other(anyhow::Error),
}

impl std::fmt::Display for ApkpureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApkpureError::Forbidden { url } => {
                write!(f, "apkpure: 403 pada {url}")
            }
            ApkpureError::Other(e) => write!(f, "apkpure: {e}"),
        }
    }
}

impl std::error::Error for ApkpureError {}

pub fn is_ip_reputation_blocked(status: u16, body: &str) -> bool {
    (status == 403 && !is_turnstile_blocked(status, &reqwest::header::HeaderMap::new(), body))
        || body.contains("1020")
        || body.contains("error code: 1020")
        || body.contains("Access denied")
}

pub fn is_turnstile_blocked(status: u16, headers: &reqwest::header::HeaderMap, body: &str) -> bool {
    let _ = status;
    if headers
        .get("cf-mitigated")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("challenge"))
        .unwrap_or(false)
    {
        return true;
    }
    let snippet = &body[..body.len().min(5120)];
    snippet.contains("cf-turnstile")
        || snippet.contains("challenges.cloudflare.com")
        || snippet.contains("cf_chl_opt")
        || snippet.contains("Just a moment...")
        || snippet.contains("cf-chl-bypass")
        || snippet.contains("Checking your browser")
        || snippet.contains("cloudflare-static")
        || snippet.contains("verify you are human")
}

pub(crate) async fn fetch_page_html(
    client: &Client,
    url: &str,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<(String, reqwest::header::HeaderMap)> {
    if let Some(f) = fetcher {
        match f.get(url).await {
            Ok(body) => {
                return Ok((body, reqwest::header::HeaderMap::new()));
            }
            Err(crate::net::FetchError::Http { status, .. }) => {
                tracing::debug!("fetcher: non-challenge HTTP {status} for {url} — direct fallback");
            }
            Err(e) => {
                tracing::debug!("fetcher get failed for {url}: {e} — direct fallback");
            }
        }
    }
    let resp = client.get(url).send().await?;
    let headers = resp.headers().clone();
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        tracing::info!("direct GET {url} -> {status} ({} bytes)", body.len());
    }
    Ok((body, headers))
}

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

pub async fn resolve_patch_compatible_version(cfg: &Config, app: &AppConfig) -> Option<String> {
    match resolve_all_patch_compatible_versions(cfg, app).await {
        metadata::CompatibleVersions::Intersection(v) => crate::utils::semver::max_version(&v),
        metadata::CompatibleVersions::Conflict { patch_versions } => {
            let smallest = patch_versions
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .min_by_key(|(_, v)| v.len())
                .map(|(_, v)| v.as_slice())
                .unwrap_or(&[]);
            crate::utils::semver::max_version(smallest)
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
        } else {
            false
        };

        if is_stale {
            if app.version_pin_strict || available_versions.is_empty() {
                warn!(
                    "{id}: pinned version v{pin} falls outside compatible patch range, but version_pin_strict=true (or no compatible versions available) — honoring pin"
                );
                (
                    Some(pin.to_string()),
                    vec![pin.to_string()],
                    "strict-honored-despite-stale".to_string(),
                )
            } else {
                warn!(
                    "{id}: pinned version v{pin} falls outside compatible patch range and version_pin_strict=false — auto-correcting to compatible version"
                );
                let auto_ver = crate::utils::semver::max_version(&available_versions);
                let mut candidates: Vec<String> = available_versions;
                candidates.reverse();
                (
                    auto_ver,
                    candidates,
                    "auto-corrected-from-stale-pin".to_string(),
                )
            }
        } else {
            (
                Some(pin.to_string()),
                vec![pin.to_string()],
                "pinned".to_string(),
            )
        }
    } else {
        let auto_ver = crate::utils::semver::max_version(&available_versions);
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
}

#[derive(Debug)]
pub struct FetchApkError {
    pub message: String,
    pub attempts: Vec<crate::orchestrator::scheduler::SourceAttemptRecord>,
    pub warnings: Vec<String>,
}

impl std::fmt::Display for FetchApkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for FetchApkError {}

pub async fn fetch_apk(
    cfg: &Config,
    id: &str,
    app: &AppConfig,
    arch: &str,
    force: bool,
    uptodown_cb: Option<Arc<AtomicU32>>,
    breaker_registry: Option<Arc<crate::sources::breaker::BreakerRegistry>>,
) -> Result<FetchApkSuccess> {
    let arch_tag = if arch == "all" || arch.is_empty() {
        "all"
    } else {
        arch
    };
    let dest = format!("{}/{id}-{arch_tag}-input.apk", cfg.build.temp_dir);
    let all_dest = format!("{}/{id}-all-input.apk", cfg.build.temp_dir);
    let legacy_dest = format!("{}/{id}-input.apk", cfg.build.temp_dir);

    let patch_compat = resolve_all_patch_compatible_versions(cfg, app).await;
    let patch_supported_versions = patch_compat.to_vec();
    let mpp_path = metadata::resolve_mpp_path(cfg, app);
    let patch_meta = if let Some(ref p) = mpp_path {
        metadata::load(cfg, p).await.ok()
    } else {
        None
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

    let cache_candidates = [&dest[..], &all_dest[..], &legacy_dest[..]];
    let cache_result = check_cached_input_apk(id, &cache_candidates, target_ver.as_deref(), force);
    if let CacheCheckResult::Hit(cached_path) = cache_result {
        let _ = std::fs::write(format!("{cached_path}.strategy"), &strategy_str);
        return Ok(FetchApkSuccess {
            path: cached_path,
            final_version: target_ver.clone(),
            attempts: vec![],
            warnings: vec![],
        });
    }

    let client = crate::utils::make_browser_client(cfg)?;

    let fetcher = Arc::new(crate::net::Fetcher::new(client.clone(), cfg.fetch.clone()));
    if let Err(e) = fetcher.ensure_probed().await {
        anyhow::bail!("{id}: FlareSolverr health check failed: {e}");
    }

    let mut warnings = Vec::new();
    if fetcher.is_flaresolverr_unavailable() {
        warnings.push("flaresolverr_unavailable".to_string());
    }

    let mut all_attempts: Vec<crate::orchestrator::scheduler::SourceAttemptRecord> = Vec::new();

    if let Some(url) = &app.apk_url {
        let t0 = std::time::Instant::now();
        let final_url = if url.contains("github.com") && url.contains("/releases/") {
            resolve_github_asset_url(&client, url, arch, &cfg.build.asset_exclude_patterns)
                .await
                .unwrap_or_else(|_| url.clone())
        } else {
            url.clone()
        };
        info!("{id}: downloading from direct apk_url {final_url}");
        match download_and_extract_if_bundle(&client, &final_url, &dest, None).await {
            Ok(_) => {
                let dur = t0.elapsed().as_millis() as u64;
                match crate::utils::apk::verify_apk_integrity(&dest) {
                    Ok(_) => {
                        let actual_ver_res = crate::utils::apk::get_apk_version_name(&dest);
                        let is_acceptable = match actual_ver_res {
                            Ok(ref actual_ver) => {
                                let actual_parts =
                                    crate::utils::semver::parse_version_numbers(actual_ver);
                                let version_matches_target =
                                    target_ver.as_ref().is_some_and(|target| {
                                        let target_parts =
                                            crate::utils::semver::parse_version_numbers(target);
                                        actual_ver == target
                                            || (!actual_parts.is_empty()
                                                && actual_parts == target_parts)
                                    });
                                let version_in_patch_compat = !patch_supported_versions.is_empty()
                                    && patch_supported_versions.iter().any(|v| {
                                        v == actual_ver
                                            || (!actual_parts.is_empty()
                                                && crate::utils::semver::parse_version_numbers(v)
                                                    == actual_parts)
                                    });

                                if is_strictly_pinned {
                                    version_matches_target
                                } else {
                                    version_matches_target || version_in_patch_compat
                                }
                            }
                            Err(_) => false,
                        };

                        if is_acceptable {
                            let actual_ver = actual_ver_res.ok();
                            let v_write = actual_ver.clone().or_else(|| target_ver.clone());
                            if let Some(ref v) = v_write {
                                let _ = std::fs::write(format!("{dest}.version"), v);
                            }
                            let _ = std::fs::write(format!("{dest}.strategy"), &strategy_str);
                            all_attempts.push(
                                crate::orchestrator::scheduler::SourceAttemptRecord {
                                    source: "direct".to_string(),
                                    outcome: "hit".to_string(),
                                    version: actual_ver.clone(),
                                    url: Some(final_url),
                                    reason: "downloaded successfully via direct apk_url"
                                        .to_string(),
                                    duration_ms: dur,
                                    breaker_state: None,
                                    block_kind: None,
                                },
                            );
                            return Ok(FetchApkSuccess {
                                path: dest,
                                final_version: actual_ver,
                                attempts: all_attempts,
                                warnings: warnings.clone(),
                            });
                        } else {
                            let act_str = actual_ver_res.unwrap_or_else(|_| "unknown".to_string());
                            warn!(
                                "{id}: direct apk_url gave version v{act_str} which is not compatible with target {:?} or patches — discarding and trying next sources",
                                target_ver
                            );
                            let _ = std::fs::remove_file(&dest);
                            all_attempts.push(
                                crate::orchestrator::scheduler::SourceAttemptRecord {
                                    source: "direct".to_string(),
                                    outcome: "version_mismatch".to_string(),
                                    version: Some(act_str.clone()),
                                    url: Some(final_url),
                                    reason: format!(
                                        "downloaded v{act_str} does not match target {:?} and is not in patch supported versions",
                                        target_ver
                                    ),
                                    duration_ms: dur,
                                    breaker_state: None,
                                    block_kind: None,
                                },
                            );
                        }
                    }
                    Err(integrity_err) => {
                        warn!("{id}: direct apk_url file failed integrity check: {integrity_err}");
                        let _ = std::fs::remove_file(&dest);
                        all_attempts.push(crate::orchestrator::scheduler::SourceAttemptRecord {
                            source: "direct".to_string(),
                            outcome: "bad_zip".to_string(),
                            version: None,
                            url: Some(final_url),
                            reason: integrity_err.to_string(),
                            duration_ms: dur,
                            breaker_state: None,
                            block_kind: None,
                        });
                    }
                }
            }
            Err(e) => {
                warn!("{id}: direct apk_url failed: {e}");
                all_attempts.push(crate::orchestrator::scheduler::SourceAttemptRecord {
                    source: "direct".to_string(),
                    outcome: "error".to_string(),
                    version: None,
                    url: Some(final_url),
                    reason: e.to_string(),
                    duration_ms: t0.elapsed().as_millis() as u64,
                    breaker_state: None,
                    block_kind: None,
                });
            }
        }
    }

    let cb_threshold: u32 = std::env::var("REVANCEX_UTD_CB_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let registry = crate::sources::default_registry();
    let ordered = crate::sources::source_order(&registry, app, cfg);

    let candidates_to_try: Vec<Option<String>> = if candidate_versions.is_empty() {
        vec![target_ver.clone()]
    } else {
        candidate_versions.into_iter().map(Some).collect()
    };

    let shared_attempts: Arc<
        std::sync::Mutex<Vec<crate::orchestrator::scheduler::SourceAttemptRecord>>,
    > = Arc::new(std::sync::Mutex::new(all_attempts));
    let attempts_clone = shared_attempts.clone();

    let result = download_with_fallback(
        id,
        &app.package,
        &candidates_to_try,
        cfg.build.version_fallback_attempts,
        |cur_target| {
            let client = &client;
            let dest = &dest;
            let ordered = &ordered;
            let fetcher = fetcher.clone();
            let uptodown_cb = uptodown_cb.clone();
            let breaker_registry = breaker_registry.clone();
            let patch_supported_versions = patch_supported_versions.clone();
            let shared_attempts = attempts_clone.clone();
            async move {
                let mut breakers = std::collections::HashMap::new();
                if let Some(ref cb) = uptodown_cb {
                    breakers.insert("uptodown".to_string(), cb.clone());
                }
                let ctx = crate::sources::SourceCtx {
                    client,
                    cfg,
                    breakers: std::sync::Arc::new(breakers),
                    cb_threshold,
                    fetcher: Some(&fetcher),
                    breaker_registry: breaker_registry.clone(),
                    app_id: Some(id.to_string()),
                };

                for src in ordered {
                    let sname = src.name();
                    let t_start = std::time::Instant::now();
                    info!("{id}: checking source {sname}");

                    let (cur_state, allowed) = match ctx.can_try(sname) {
                        Ok(st) => (st, true),
                        Err((st, reason)) => {
                            shared_attempts.lock().unwrap().push(
                                crate::orchestrator::scheduler::SourceAttemptRecord {
                                    source: sname.to_string(),
                                    outcome: "skipped-breaker".to_string(),
                                    version: cur_target.clone(),
                                    url: None,
                                    reason,
                                    duration_ms: t_start.elapsed().as_millis() as u64,
                                    breaker_state: Some(st.to_string()),
                                    block_kind: None,
                                },
                            );
                            (st, false)
                        }
                    };

                    if !allowed {
                        continue;
                    }

                    match src.resolve(&ctx, app, cur_target.as_deref(), arch).await {
                        Ok(Some(resolved)) => {
                            let r_url = resolved.url.clone();
                            match src.fetch(&ctx, &resolved, dest).await {
                                Ok(path) => {
                                    let dur = t_start.elapsed().as_millis() as u64;
                                    let p_str = path.to_string_lossy();

                                    // Verify APK integrity
                                    if let Err(integrity_err) =
                                        crate::utils::apk::verify_apk_integrity(&path)
                                    {
                                        warn!(
                                            "{id}: APK from {sname} failed integrity check: {integrity_err}"
                                        );
                                        let _ = std::fs::remove_file(dest);
                                        shared_attempts.lock().unwrap().push(
                                            crate::orchestrator::scheduler::SourceAttemptRecord {
                                                source: sname.to_string(),
                                                outcome: "bad_zip".to_string(),
                                                version: cur_target.clone(),
                                                url: Some(r_url),
                                                reason: integrity_err.to_string(),
                                                duration_ms: dur,
                                                breaker_state: Some(cur_state.to_string()),
                                                block_kind: None,
                                            },
                                        );
                                        continue;
                                    }

                                    // Check architecture
                                    if arch != "all" && arch != "universal" && !arch.is_empty() {
                                        let has_any = arch::apk_has_any_native_libs(&p_str)
                                            .unwrap_or(false);
                                        let has_target =
                                            arch::apk_has_arch(&p_str, arch).unwrap_or(false);
                                        if has_any && !has_target {
                                            warn!("{id}: APK from {sname} has native libs but none for requested arch '{arch}', discarding and trying next source");
                                            let _ = std::fs::remove_file(dest);
                                            shared_attempts.lock().unwrap().push(
                                                crate::orchestrator::scheduler::SourceAttemptRecord {
                                                    source: sname.to_string(),
                                                    outcome: "not_found".to_string(),
                                                    version: cur_target.clone(),
                                                    url: Some(r_url),
                                                    reason: format!(
                                                        "native libs missing for arch '{arch}'"
                                                    ),
                                                    duration_ms: dur,
                                                    breaker_state: Some(cur_state.to_string()),
                                                    block_kind: None,
                                                },
                                            );
                                            continue;
                                        }
                                    }

                                    // Check certificate SHA-256
                                    if let Some(ref expected_cert) = app.expected_cert_sha256 {
                                        match crate::utils::keystore::verify_apk_cert_sha256(
                                            &p_str,
                                            expected_cert,
                                        ) {
                                            Ok(true) => {
                                                info!(
                                                    "{id}: APK cert SHA-256 matches expected: {expected_cert}"
                                                );
                                            }
                                            Ok(false) => {
                                                warn!("{id}: APK cert SHA-256 mismatch for source {sname}, discarding and trying next source");
                                                let _ = std::fs::remove_file(dest);
                                                shared_attempts.lock().unwrap().push(
                                                    crate::orchestrator::scheduler::SourceAttemptRecord {
                                                        source: sname.to_string(),
                                                        outcome: "error".to_string(),
                                                        version: cur_target.clone(),
                                                        url: Some(r_url),
                                                        reason: "cert SHA-256 mismatch".to_string(),
                                                        duration_ms: dur,
                                                        breaker_state: Some(cur_state.to_string()),
                                                        block_kind: None,
                                                    },
                                                );
                                                continue;
                                            }
                                            Err(e) => {
                                                warn!("{id}: failed to verify APK cert for {sname}: {e}, trying next source");
                                                let _ = std::fs::remove_file(dest);
                                                shared_attempts.lock().unwrap().push(
                                                    crate::orchestrator::scheduler::SourceAttemptRecord {
                                                        source: sname.to_string(),
                                                        outcome: "error".to_string(),
                                                        version: cur_target.clone(),
                                                        url: Some(r_url),
                                                        reason: format!(
                                                            "failed to verify cert: {e}"
                                                        ),
                                                        duration_ms: dur,
                                                        breaker_state: Some(cur_state.to_string()),
                                                        block_kind: None,
                                                    },
                                                );
                                                continue;
                                            }
                                        }
                                    }

                                    // Check version match
                                    let actual_ver =
                                        match crate::utils::apk::get_apk_version_name(&path) {
                                            Ok(v) => v,
                                            Err(e) => {
                                                let _ = std::fs::remove_file(dest);
                                                shared_attempts.lock().unwrap().push(
                                                    crate::orchestrator::scheduler::SourceAttemptRecord {
                                                        source: sname.to_string(),
                                                        outcome: "error".to_string(),
                                                        version: cur_target.clone(),
                                                        url: Some(r_url),
                                                        reason: format!(
                                                            "failed to read APK version name: {e}"
                                                        ),
                                                        duration_ms: dur,
                                                        breaker_state: Some(cur_state.to_string()),
                                                        block_kind: None,
                                                    },
                                                );
                                                continue;
                                            }
                                        };

                                    let actual_parts = crate::utils::semver::parse_version_numbers(
                                        &actual_ver,
                                    );
                                    let matches_cur = cur_target.as_ref().is_none_or(|tgt| {
                                        let tgt_parts =
                                            crate::utils::semver::parse_version_numbers(tgt);
                                        &actual_ver == tgt
                                            || (!actual_parts.is_empty()
                                                && actual_parts == tgt_parts)
                                    });

                                    let in_patch_compat = !patch_supported_versions.is_empty()
                                        && patch_supported_versions.iter().any(|v| {
                                            v == &actual_ver
                                                || (!actual_parts.is_empty()
                                                    && crate::utils::semver::parse_version_numbers(
                                                        v,
                                                    ) == actual_parts)
                                        });

                                    if matches_cur || (!is_strictly_pinned && in_patch_compat) {
                                        ctx.record_success(sname);
                                        shared_attempts.lock().unwrap().push(
                                            crate::orchestrator::scheduler::SourceAttemptRecord {
                                                source: sname.to_string(),
                                                outcome: "hit".to_string(),
                                                version: Some(actual_ver.clone()),
                                                url: Some(r_url),
                                                reason: "downloaded and verified successfully"
                                                    .to_string(),
                                                duration_ms: dur,
                                                breaker_state: Some(cur_state.to_string()),
                                                block_kind: None,
                                            },
                                        );
                                        info!(
                                            "{id}: BUILD_REPORT resolved_source={sname} actual_ver={actual_ver}"
                                        );
                                        return Ok(path.to_string_lossy().to_string());
                                    } else {
                                        let _ = std::fs::remove_file(dest);
                                        let tgt_name = cur_target.as_deref().unwrap_or("auto");
                                        shared_attempts.lock().unwrap().push(
                                            crate::orchestrator::scheduler::SourceAttemptRecord {
                                                source: sname.to_string(),
                                                outcome: "version_mismatch".to_string(),
                                                version: Some(actual_ver.clone()),
                                                url: Some(r_url),
                                                reason: format!(
                                                    "downloaded v{actual_ver} does not match requested v{tgt_name} and is not in patch supported versions"
                                                ),
                                                duration_ms: dur,
                                                breaker_state: Some(cur_state.to_string()),
                                                block_kind: None,
                                            },
                                        );
                                        continue;
                                    }
                                }
                                Err(e) => {
                                    let dur = t_start.elapsed().as_millis() as u64;
                                    let err_msg = e.to_string();
                                    let block_kind = crate::sources::breaker::classify_error(&err_msg);
                                    ctx.record_failure(sname, &block_kind);
                                    let outcome = match block_kind {
                                        crate::sources::breaker::BlockKind::TurnstileChallenge
                                        | crate::sources::breaker::BlockKind::IpReputationBlock => "blocked",
                                        crate::sources::breaker::BlockKind::RateLimited => "rate_limited",
                                        crate::sources::breaker::BlockKind::NotFound => "not_found",
                                        _ => "error",
                                    };
                                    shared_attempts.lock().unwrap().push(
                                        crate::orchestrator::scheduler::SourceAttemptRecord {
                                            source: sname.to_string(),
                                            outcome: outcome.to_string(),
                                            version: cur_target.clone(),
                                            url: Some(r_url),
                                            reason: err_msg.clone(),
                                            duration_ms: dur,
                                            breaker_state: Some(cur_state.to_string()),
                                            block_kind: Some(block_kind),
                                        },
                                    );
                                    warn!("{id}: {sname} fetch failed: {err_msg}");
                                }
                            }
                        }
                        Ok(None) => {
                            let dur = t_start.elapsed().as_millis() as u64;
                            shared_attempts.lock().unwrap().push(
                                crate::orchestrator::scheduler::SourceAttemptRecord {
                                    source: sname.to_string(),
                                    outcome: "not_found".to_string(),
                                    version: cur_target.clone(),
                                    url: None,
                                    reason: "source not configured or release not found"
                                        .to_string(),
                                    duration_ms: dur,
                                    breaker_state: Some(cur_state.to_string()),
                                    block_kind: Some(crate::sources::breaker::BlockKind::NotFound),
                                },
                            );
                        }
                        Err(e) => {
                            let dur = t_start.elapsed().as_millis() as u64;
                            let err_msg = e.to_string();
                            let block_kind = crate::sources::breaker::classify_error(&err_msg);
                            ctx.record_failure(sname, &block_kind);
                            let outcome = match block_kind {
                                crate::sources::breaker::BlockKind::TurnstileChallenge
                                | crate::sources::breaker::BlockKind::IpReputationBlock => "blocked",
                                crate::sources::breaker::BlockKind::RateLimited => "rate_limited",
                                crate::sources::breaker::BlockKind::NotFound => "not_found",
                                _ => "error",
                            };
                            shared_attempts.lock().unwrap().push(
                                crate::orchestrator::scheduler::SourceAttemptRecord {
                                    source: sname.to_string(),
                                    outcome: outcome.to_string(),
                                    version: cur_target.clone(),
                                    url: None,
                                    reason: err_msg.clone(),
                                    duration_ms: dur,
                                    breaker_state: Some(cur_state.to_string()),
                                    block_kind: Some(block_kind),
                                },
                            );
                            warn!("{id}: {sname} resolve failed: {err_msg}");
                        }
                    }
                }

                let all_open = ordered.iter().all(|s| ctx.can_try(s.name()).is_err());
                if all_open {
                    anyhow::bail!("{id}: all sources failed for candidate {:?} (all_candidate_sources_open_breakers)", cur_target);
                }
                anyhow::bail!("{id}: all sources failed for candidate {:?}", cur_target);
            }
        },
    )
    .await;

    let result = match result {
        Ok(path) => path,
        Err(e) => {
            let final_attempts = shared_attempts.lock().unwrap().clone();
            return Err(anyhow::Error::new(FetchApkError {
                message: e.to_string(),
                attempts: final_attempts,
                warnings: warnings.clone(),
            }));
        }
    };

    let actual_ver = crate::utils::apk::get_apk_version_name(&dest).ok();
    let actual_parts = actual_ver
        .as_ref()
        .map(|v| crate::utils::semver::parse_version_numbers(v))
        .unwrap_or_default();

    let version_matches_target = target_ver.as_ref().is_some_and(|target| {
        let target_parts = crate::utils::semver::parse_version_numbers(target);
        actual_ver.as_ref().map(|v| v == target).unwrap_or(false)
            || (!actual_parts.is_empty() && actual_parts == target_parts)
    });

    let version_in_patch_compat = !patch_supported_versions.is_empty()
        && actual_ver.as_ref().is_some_and(|v| {
            patch_supported_versions.iter().any(|pv| {
                pv == v
                    || (!actual_parts.is_empty()
                        && crate::utils::semver::parse_version_numbers(pv) == actual_parts)
            })
        });

    if is_strictly_pinned {
        if !version_matches_target {
            let _ = std::fs::remove_file(&dest);
            let _ = std::fs::remove_file(format!("{dest}.version"));
            let _ = std::fs::remove_file(format!("{dest}.strategy"));
            let act_str = actual_ver.unwrap_or_else(|| "unknown".to_string());
            let final_attempts = shared_attempts.lock().unwrap().clone();
            return Err(anyhow::Error::new(FetchApkError {
                message: format!(
                    "{id}: downloaded APK version v{act_str} does not match strictly pinned target v{} — discarded",
                    target_ver.as_deref().unwrap_or("unknown")
                ),
                attempts: final_attempts,
                warnings: warnings.clone(),
            }));
        }
    } else if !version_matches_target && !version_in_patch_compat && target_ver.is_some() {
        let _ = std::fs::remove_file(&dest);
        let _ = std::fs::remove_file(format!("{dest}.version"));
        let _ = std::fs::remove_file(format!("{dest}.strategy"));
        let act_str = actual_ver.unwrap_or_else(|| "unknown".to_string());
        let final_attempts = shared_attempts.lock().unwrap().clone();
        return Err(anyhow::Error::new(FetchApkError {
            message: format!(
                "{id}: downloaded APK version v{act_str} does not match target v{} and is not compatible with any enabled patch (supported: {:?}) — discarded",
                target_ver.as_deref().unwrap_or("unknown"),
                patch_supported_versions
            ),
            attempts: final_attempts,
            warnings: warnings.clone(),
        }));
    } else if !version_matches_target && version_in_patch_compat {
        let act_str = actual_ver.as_deref().unwrap_or("unknown");
        info!(
            "{id}: target was v{}, but mirror provided compatible version v{act_str} supported by patch — accepting fallback",
            target_ver.as_deref().unwrap_or("auto")
        );
    }

    let final_ver = actual_ver.or_else(|| target_ver.clone());
    if let Some(ref v) = final_ver {
        let _ = std::fs::write(format!("{dest}.version"), v);
    }
    let _ = std::fs::write(format!("{dest}.strategy"), &strategy_str);

    let final_attempts = shared_attempts.lock().unwrap().clone();
    Ok(FetchApkSuccess {
        path: result,
        final_version: final_ver,
        attempts: final_attempts,
        warnings,
    })
}

fn is_beta_or_alpha(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("beta") || lower.contains("alpha")
}

pub(crate) async fn fetch_archive_org(
    client: &Client,
    url: &str,
    arch: &str,
    dest: &str,
    target_version: Option<&str>,
    supported_versions: &[&str],
) -> Result<String> {
    let base = if url.ends_with('/') {
        url.to_string()
    } else {
        format!("{url}/")
    };

    info!("archive.org: listing files from {base}");
    let html = client.get(&base).send().await?.text().await?;

    let re = Regex::new(r#"href="([^"]+\.(?:apk|apkm|xapk))""#)?;
    let mut candidates: Vec<String> = Vec::new();

    for cap in re.captures_iter(&html) {
        let file = &cap[1];
        if file.contains('/') || file.starts_with('?') || file.contains('~') {
            continue;
        }
        candidates.push(file.to_string());
    }

    if candidates.is_empty() {
        anyhow::bail!("No APK/APKM found in archive.org directory {base}");
    }

    let allow_beta = target_version.is_some_and(is_beta_or_alpha);
    let clean_candidates: Vec<String> = candidates
        .into_iter()
        .filter(|c| allow_beta || !is_beta_or_alpha(c))
        .collect();

    if clean_candidates.is_empty() {
        anyhow::bail!("No non-beta APK/APKM found in archive.org directory {base}");
    }

    let chosen = if let Some(ver) = target_version {
        let base_ver = ver.split('-').next().unwrap_or(ver);
        let exact = clean_candidates
            .iter()
            .filter(|f| f.contains(ver) || f.contains(base_ver))
            .rfind(|f| {
                f.contains(arch)
                    || f.contains("-all.")
                    || f.contains("_all.")
                    || f.contains("universal")
            })
            .or_else(|| {
                clean_candidates
                    .iter()
                    .rfind(|f| f.contains(ver) || f.contains(base_ver))
            });

        if let Some(m) = exact {
            m
        } else {
            let supported_match = supported_versions
                .iter()
                .rev()
                .filter(|&&sv| sv != ver)
                .find_map(|&sv| {
                    let s_base = sv.split('-').next().unwrap_or(sv);
                    clean_candidates
                        .iter()
                        .filter(|f| f.contains(sv) || f.contains(s_base))
                        .rfind(|f| {
                            f.contains(arch)
                                || f.contains("-all.")
                                || f.contains("_all.")
                                || f.contains("universal")
                        })
                        .or_else(|| {
                            clean_candidates
                                .iter()
                                .rfind(|f| f.contains(sv) || f.contains(s_base))
                        })
                });

            if let Some(sm) = supported_match {
                info!("archive.org: exact version {ver} not found, matched supported version in archive: {sm}");
                sm
            } else {
                anyhow::bail!(
                    "archive.org: exact version {ver} and supported versions not found in archive candidates"
                );
            }
        }
    } else {
        clean_candidates
            .iter()
            .rfind(|f| f.contains(arch))
            .or_else(|| {
                clean_candidates
                    .iter()
                    .rfind(|f| f.contains("-all.") || f.contains("_all."))
            })
            .or_else(|| clean_candidates.iter().rfind(|f| f.contains("universal")))
            .or_else(|| clean_candidates.last())
            .ok_or_else(|| anyhow::anyhow!("No matching file candidate"))?
    };

    let download_url = format!("{base}{chosen}");
    info!("archive.org: downloading {download_url}");
    download_and_extract_if_bundle(client, &download_url, dest, None).await?;
    Ok(dest.to_string())
}

pub(crate) async fn fetch_apkmirror(
    client: &Client,
    app_url: &str,
    arch: &str,
    dest: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String> {
    info!("apkmirror: loading app page {app_url}");
    let base_origin = "https://www.apkmirror.com";
    let (app_html, _hdrs) = fetch_page_html(client, app_url, fetcher).await?;

    let re_rel = Regex::new(r#"href="(/apk/[^"]+-release/)""#)?;
    let mut all_releases: Vec<String> = Vec::new();
    for cap in re_rel.captures_iter(&app_html) {
        all_releases.push(cap[1].to_string());
    }

    if all_releases.is_empty() {
        anyhow::bail!("No release links found on {app_url}");
    }

    let allow_beta = target_version.is_some_and(is_beta_or_alpha);
    let releases: Vec<String> = all_releases
        .into_iter()
        .filter(|r| allow_beta || !is_beta_or_alpha(r))
        .collect();

    if releases.is_empty() {
        anyhow::bail!("Only beta/alpha release links found on {app_url}");
    }

    let release_path = if let Some(ver) = target_version {
        let hyphen_ver = ver.replace('.', "-");
        let base_hyphen = ver.split('-').next().unwrap_or(ver).replace('.', "-");
        releases
            .iter()
            .find(|r| r.contains(&hyphen_ver) || r.contains(&base_hyphen) || r.contains(ver))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "apkmirror: version {ver} not found in recent releases on {app_url}"
                )
            })?
    } else {
        releases.first().unwrap()
    };

    let release_url = format!("{base_origin}{release_path}");
    info!("apkmirror: loading release {release_url}");
    let (rel_html, _hdrs) = fetch_page_html(client, &release_url, fetcher).await?;

    let re_var = Regex::new(r#"href="(/apk/[^"]+-android-apk-download/)""#)?;
    let mut variants: Vec<String> = Vec::new();
    for cap in re_var.captures_iter(&rel_html) {
        variants.push(cap[1].to_string());
    }
    if variants.is_empty() {
        anyhow::bail!("No variant links found on {release_url}");
    }

    let variant_path = variants
        .iter()
        .find(|v| v.contains(arch))
        .or_else(|| variants.first())
        .unwrap();

    let variant_url = format!("{base_origin}{variant_path}");
    info!("apkmirror: loading variant {variant_url}");
    let (var_html, _hdrs) = fetch_page_html(client, &variant_url, fetcher).await?;

    let re_dl = Regex::new(r#"href="(/apk/[^"]+/download/\?key=[^"]+)""#)?;
    let dl_path = re_dl
        .captures(&var_html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow::anyhow!("No intermediate download button found on {variant_url}"))?;

    let dl_url = format!("{base_origin}{dl_path}");
    info!("apkmirror: loading download landing page {dl_url}");
    let (dl_html, _hdrs) = fetch_page_html(client, &dl_url, fetcher).await?;

    let re_php = Regex::new(r#"href="(/wp-content/themes/APKMirror/download\.php\?[^"]+)""#)?;
    let php_path = re_php
        .captures(&dl_html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| anyhow::anyhow!("No download.php link found on {dl_url}"))?;

    let php_url = format!("{base_origin}{php_path}");
    info!("apkmirror: resolving direct CDN location from {php_url}");

    let mut hdrs = HeaderMap::new();
    let ua = crate::utils::random_user_agent();
    let val = HeaderValue::from_str(ua)
        .unwrap_or_else(|_| HeaderValue::from_static(crate::utils::DEFAULT_UA));
    hdrs.insert(USER_AGENT, val);
    let no_redir_client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(hdrs)
        .build()?;

    let res = no_redir_client
        .get(&php_url)
        .header("Referer", &dl_url)
        .send()
        .await?;

    let cdn_url = if let Some(loc) = res.headers().get("location") {
        loc.to_str()?.to_string()
    } else {
        php_url
    };

    info!("apkmirror: downloading from CDN {cdn_url}");
    download_and_extract_if_bundle(client, &cdn_url, dest, Some(&dl_url)).await?;
    Ok(dest.to_string())
}

pub(crate) async fn fetch_uptodown(
    client: &Client,
    base_url: &str,
    _arch: &str,
    dest: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String, UptodownError> {
    let versions_url = format!("{base_url}/versions");
    info!("uptodown: checking versions at {versions_url}");

    let (html_bytes, versions_headers) = match fetcher {
        Some(f) => match crate::builder::fetch_page_html(client, &versions_url, Some(f)).await {
            Ok((body, hdrs)) => (body, hdrs),
            Err(e) => return Err(UptodownError::Network(e)),
        },
        None => {
            let resp = client
                .get(&versions_url)
                .send()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            let hdrs = resp.headers().clone();
            let body = resp
                .text()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            (body, hdrs)
        }
    };
    let versions_status = 200u16;
    let _ = versions_headers;
    let html = html_bytes;

    if is_ip_reputation_blocked(versions_status, &html) {
        warn!("uptodown: IpReputationBlocked on versions page (status={versions_status})");
        return Err(UptodownError::IpReputationBlocked);
    }

    if is_turnstile_blocked(versions_status, &versions_headers, &html) {
        warn!("uptodown: TurnstileBlocked on versions page (status={versions_status})");
        return Err(UptodownError::TurnstileBlocked);
    }

    let re_code = Regex::new(r#"id="detail-app-name"[^>]*data-code="([0-9]+)""#)
        .map_err(|e| UptodownError::Network(e.into()))?;
    let data_code = re_code
        .captures(&html)
        .map(|c| c[1].to_string())
        .ok_or_else(|| {
            let snippet = html.chars().take(200).collect::<String>();
            UptodownError::TokenNotFound {
                html_len: html.len(),
                snippet,
            }
        })?;

    let api_url = format!("{base_url}/apps/{data_code}/versions/1");
    info!("uptodown: querying version api {api_url}");

    let res = client
        .get(&api_url)
        .header("X-Requested-With", "XMLHttpRequest")
        .send()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| UptodownError::Network(e.into()))?;
    let data = json["data"].as_array().ok_or_else(|| {
        UptodownError::Network(anyhow::anyhow!(
            "Unexpected JSON from uptodown versions API"
        ))
    })?;

    if data.is_empty() {
        return Err(UptodownError::Network(anyhow::anyhow!(
            "No versions returned by uptodown API"
        )));
    }

    let allow_beta = target_version.is_some_and(is_beta_or_alpha);

    let chosen_item = if let Some(ver) = target_version {
        let base_ver = ver.split('-').next().unwrap_or(ver);
        data.iter().find(|item| {
            let v = item["version"].as_str().unwrap_or("");
            (allow_beta || !is_beta_or_alpha(v)) && (v.contains(ver) || v.contains(base_ver))
        })
    } else {
        data.iter().find(|item| {
            let v = item["version"].as_str().unwrap_or("");
            allow_beta || !is_beta_or_alpha(v)
        })
    };

    let item = chosen_item.ok_or_else(|| {
        UptodownError::Network(anyhow::anyhow!(
            "No suitable non-beta versions from uptodown API"
        ))
    })?;
    let file_id = item["fileID"].as_i64().unwrap_or(0);
    info!(
        "uptodown: selected version {} (fileID: {file_id})",
        item["version"].as_str().unwrap_or("unknown")
    );

    let dl_page = format!("{base_url}/download/{file_id}-x");
    let (dl_html, dl_headers) = match fetcher {
        Some(f) => match crate::builder::fetch_page_html(client, &dl_page, Some(f)).await {
            Ok((body, hdrs)) => (body, hdrs),
            Err(e) => return Err(UptodownError::Network(e)),
        },
        None => {
            let resp = client
                .get(&dl_page)
                .send()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            let hdrs = resp.headers().clone();
            let body = resp
                .text()
                .await
                .map_err(|e| UptodownError::Network(e.into()))?;
            (body, hdrs)
        }
    };
    let dl_status = 200u16;
    let _ = dl_status;

    debug!("uptodown: download page {} bytes", dl_html.len());

    if is_ip_reputation_blocked(dl_status, &dl_html) {
        warn!("uptodown: IpReputationBlocked on download page (status={dl_status})");
        return Err(UptodownError::IpReputationBlocked);
    }

    if is_turnstile_blocked(dl_status, &dl_headers, &dl_html) {
        warn!("uptodown: TurnstileBlocked on download page (status={dl_status})");
        return Err(UptodownError::TurnstileBlocked);
    }

    let re_dwn =
        Regex::new(r#"data-url="([^"]+)""#).map_err(|e| UptodownError::Network(e.into()))?;

    if let Some(cap) = re_dwn.captures(&dl_html) {
        let token = &cap[1];
        if !token.starts_with("http") && token.len() > 10 {
            let direct = format!("https://dw.uptodown.com/dwn/{token}");
            info!("uptodown: downloading from direct token {direct}");
            return download_and_extract_if_bundle(client, &direct, dest, Some(base_url))
                .await
                .map(|_| dest.to_string())
                .map_err(UptodownError::Network);
        }
    }

    let snippet = dl_html.chars().take(300).collect::<String>();
    debug!("uptodown: data-url not found; page snippet: {snippet}");
    Err(UptodownError::TokenNotFound {
        html_len: dl_html.len(),
        snippet,
    })
}

async fn resolve_github_asset_url(
    client: &Client,
    url: &str,
    arch: &str,
    exclude_patterns: &[String],
) -> Result<String> {
    let re = Regex::new(r#"github\.com/([^/]+)/([^/]+)/releases"#)?;
    if let Some(cap) = re.captures(url) {
        let owner = &cap[1];
        let repo = &cap[2];
        let api_url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
        let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
        let mut req = client.get(&api_url);
        if !token.is_empty() {
            req = req.bearer_auth(&token);
        }
        let release: serde_json::Value = req.send().await?.json().await?;
        if let Some(assets) = release["assets"].as_array() {
            let mut candidates: Vec<String> = Vec::new();
            for asset in assets {
                let name = asset["name"].as_str().unwrap_or("");
                let is_excluded = exclude_patterns.iter().any(|pat| name.contains(pat));
                if name.ends_with(".apk") && !is_excluded {
                    if let Some(dl) = asset["browser_download_url"].as_str() {
                        candidates.push(dl.to_string());
                    }
                }
            }
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
    }
    Ok(url.to_string())
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

    for entry in std::fs::read_dir(out_dir)? {
        let entry = entry?;
        let p = entry.path();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let p_str = p.to_string_lossy().to_string();
        if ext == "xapk" || ext == "apkm" || is_zip_file(&p_str) || is_split_bundle(&p_str) {
            info!("apkeep: processing bundle file {p_str}");
            if is_split_bundle(&p_str) {
                if let Ok(()) = merge_split_apk_bundle(&p_str, dest).await {
                    let _ = strip_split_requirements_from_apk(dest);
                    return Ok(dest.to_string());
                } else {
                    tracing::warn!(
                        "apkeep: merge_split_apk_bundle failed, attempting base APK extraction"
                    );
                }
            }
            if extract_base_apk_from_zip(&p_str, dest).is_ok() {
                let _ = strip_split_requirements_from_apk(dest);
                return Ok(dest.to_string());
            }
        }
        if ext == "apk" {
            std::fs::copy(&p, dest)?;
            let _ = strip_split_requirements_from_apk(dest);
            return Ok(dest.to_string());
        }
    }

    anyhow::bail!("apkeep finished but produced no valid .apk or .xapk in {out_dir}")
}

pub async fn download_and_extract_if_bundle(
    client: &Client,
    url: &str,
    dest_apk_path: &str,
    referer: Option<&str>,
) -> Result<()> {
    if let Some(parent) = Path::new(dest_apk_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temp_dl = format!("{dest_apk_path}.tmp");
    crate::utils::download_file_with_client(client, url, &temp_dl, 3, referer).await?;

    if is_zip_file(&temp_dl) {
        info!("Checking if downloaded file is a ZIP bundle (.apkm/.xapk/split) or APK...");
        if is_split_bundle(&temp_dl) {
            info!("Detected split APK bundle. Merging splits using APKEditor...");
            if let Ok(()) = merge_split_apk_bundle(&temp_dl, dest_apk_path).await {
                let _ = std::fs::remove_file(&temp_dl);
                let _ = strip_split_requirements_from_apk(dest_apk_path);
                return Ok(());
            } else {
                tracing::warn!("APKEditor merge failed, falling back to base APK extraction");
            }
        }
        if extract_base_apk_from_zip(&temp_dl, dest_apk_path).is_ok() {
            let _ = std::fs::remove_file(&temp_dl);
            let _ = strip_split_requirements_from_apk(dest_apk_path);
            return Ok(());
        }
    }

    std::fs::rename(&temp_dl, dest_apk_path)?;
    let _ = strip_split_requirements_from_apk(dest_apk_path);
    Ok(())
}

pub(crate) async fn fetch_aptoide(
    client: &Client,
    package: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    const BASE: &str = "https://ws75.aptoide.com/api/7/";

    let q_suffix = match arch {
        "arm64-v8a" => {
            let raw = "myCPU=arm64-v8a,armeabi-v7a,armeabi&leanback=0";
            format!("&q={}", STANDARD.encode(raw))
        }
        "armeabi-v7a" => {
            let raw = "myCPU=armeabi-v7a,armeabi&leanback=0";
            format!("&q={}", STANDARD.encode(raw))
        }
        "x86_64" => {
            let raw = "myCPU=x86_64,x86&leanback=0";
            format!("&q={}", STANDARD.encode(raw))
        }
        "x86" => {
            let raw = "myCPU=x86&leanback=0";
            format!("&q={}", STANDARD.encode(raw))
        }
        _ => String::new(),
    };

    let normalize = crate::utils::version::normalize_version;

    let target_norm = target_version.map(normalize);

    if let Some(ref tgt) = target_norm {
        let site_url = format!("https://en.aptoide.com/app/{package}");
        let html_res = match fetcher {
            Some(f) => fetch_page_html(client, &site_url, Some(f))
                .await
                .map(|(b, _)| b),
            None => match client.get(&site_url).send().await {
                Ok(resp) if resp.status().is_success() => resp.text().await.map_err(Into::into),
                Ok(resp) => anyhow::bail!("status {}", resp.status()),
                Err(e) => Err(e.into()),
            },
        };
        if let Ok(html) = html_res {
            if let Some(start) = html.find("{\"props\"") {
                let json_str = &html[start..];
                let end = json_str.find("</script>").unwrap_or(json_str.len());
                if let Ok(props) = serde_json::from_str::<serde_json::Value>(&json_str[..end]) {
                    if let Some(versions) = props["pageProps"]["versions"].as_array() {
                        for ver_entry in versions {
                            let vername = ver_entry["vername"].as_str().unwrap_or("");
                            if normalize(vername) == *tgt {
                                if let Some(path) = ver_entry["file"]["path"].as_str() {
                                    if !path.is_empty() {
                                        info!("aptoide website: found {package} v{vername}, downloading {path}");
                                        download_and_extract_if_bundle(
                                            client,
                                            path,
                                            dest,
                                            Some("https://aptoide.com/"),
                                        )
                                        .await?;
                                        if arch != "all" && arch != "universal" && !arch.is_empty()
                                        {
                                            let has_any = arch::apk_has_any_native_libs(dest)
                                                .unwrap_or(false);
                                            let has_target =
                                                arch::apk_has_arch(dest, arch).unwrap_or(false);
                                            if has_any && !has_target {
                                                info!("aptoide website: APK for v{vername} has native libs but none for '{arch}', checking next entry");
                                                let _ = std::fs::remove_file(dest);
                                                continue;
                                            }
                                        }
                                        return Ok(dest.to_string());
                                    }
                                }
                                info!("aptoide website: v{vername} found but no download URL (older version)");
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let download_path = if let Some(ref tgt) = target_norm {
        let list_url = format!("{BASE}listAppVersions?package_name={package}&limit=100{q_suffix}");
        info!("aptoide: listing versions from {list_url}");
        let resp: serde_json::Value = client
            .get(&list_url)
            .send()
            .await?
            .json()
            .await
            .context("aptoide: failed to parse listAppVersions JSON")?;

        let list = resp["datalist"]["list"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("aptoide: no version list for {package}"))?;

        let entry = list.iter().find(|item| {
            let vn = item["file"]["vername"].as_str().unwrap_or("");
            normalize(vn) == *tgt
        });

        let vercode = entry
            .and_then(|item| item["file"]["vercode"].as_i64())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "aptoide: version {:?} (normalized: {tgt}) not found for {package}",
                    target_version
                )
            })?;

        let meta_url =
            format!("{BASE}getAppMeta?package_name={package}&vercode={vercode}{q_suffix}");
        info!("aptoide: fetching meta from {meta_url}");
        let meta: serde_json::Value = client
            .get(&meta_url)
            .send()
            .await?
            .json()
            .await
            .context("aptoide: failed to parse getAppMeta JSON")?;

        meta["data"]["file"]["path"]
            .as_str()
            .ok_or_else(|| {
                anyhow::anyhow!("aptoide: no download path in meta for {package} v{tgt}")
            })?
            .to_string()
    } else {
        // Latest — API search (no specific version requested).
        let search_url =
            format!("{BASE}apps/search?query={package}&limit=1&trusted=true{q_suffix}");
        info!("aptoide: searching latest via {search_url}");
        let resp: serde_json::Value = client
            .get(&search_url)
            .send()
            .await?
            .json()
            .await
            .context("aptoide: failed to parse search JSON")?;

        resp["datalist"]["list"][0]["file"]["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("aptoide: no result for {package}"))?
            .to_string()
    };

    info!("aptoide: downloading from {download_path}");
    download_and_extract_if_bundle(client, &download_path, dest, Some("https://aptoide.com/"))
        .await?;
    Ok(dest.to_string())
}

/// Resolve-only variant: returns the asset URL without downloading.
/// Used by `src/sources/apkpure.rs` to populate `Resolved` ahead of time.
pub(crate) async fn resolve_apkpure_api_url(
    client: &Client,
    package: &str,
    arch: &str,
    target_version: Option<&str>,
    extra_headers: &std::collections::HashMap<String, String>,
) -> Result<String> {
    let url =
        format!("https://tapi.pureapk.com/v3/get_app_his_version?package_name={package}&hl=en");

    let mut req = client.get(&url);
    for (k, v) in extra_headers {
        req = req.header(k.as_str(), v.as_str());
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("apkpure_api: request failed for {package}: {e}");
            return Err(e).context("apkpure_api: request failed");
        }
    };

    if !resp.status().is_success() {
        warn!("apkpure_api: HTTP {} for {package}", resp.status());
        anyhow::bail!("apkpure_api: HTTP {} for {package}", resp.status());
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            warn!("apkpure_api: JSON parse failed for {package}: {e}");
            return Err(e).context("apkpure_api: JSON parse failed");
        }
    };

    let version_list = body
        .get("version_list")
        .or_else(|| body.get("data").and_then(|d| d.get("version_list")))
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            warn!("apkpure_api: no version_list for {package}");
            anyhow::anyhow!("apkpure_api: no version_list for {package}")
        })?;

    if version_list.is_empty() {
        warn!("apkpure_api: empty version_list for {package}");
        anyhow::bail!("apkpure_api: empty version_list for {package}");
    }
    let entry = if let Some(ver) = target_version {
        version_list.iter().find(|e| {
            let vn = e["version_name"].as_str().unwrap_or("");
            vn == ver || crate::utils::version::versions_match(vn, ver)
        })
    } else {
        version_list.first()
    };
    let entry = entry.ok_or_else(|| {
        anyhow::anyhow!(
            "apkpure_api: version {:?} not in the ~20 newest entries for {package} — miss",
            target_version
        )
    })?;
    let asset_url = entry["asset"]["urls"]
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|u| u.as_str().map(|s| s.contains(arch)).unwrap_or(false))
                .or_else(|| arr.first())
        })
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("apkpure_api: no asset URL in entry for {package}"))?;
    Ok(asset_url.to_string())
}

pub(crate) async fn fetch_apkpure_url_pub(
    client: &Client,
    base_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String, ApkpureError> {
    fetch_apkpure_url(client, base_url, dest, arch, target_version, fetcher).await
}

pub(crate) async fn fetch_apkpure_pub(
    client: &Client,
    package_or_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String, ApkpureError> {
    fetch_apkpure(client, package_or_url, dest, arch, target_version, fetcher).await
}

pub(crate) async fn fetch_apkpure_url(
    client: &Client,
    base_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String, ApkpureError> {
    if base_url.starts_with("http") && (base_url.ends_with(".apk") || base_url.ends_with(".xapk")) {
        info!("apkpure: using explicit direct URL override {base_url}");
        download_and_extract_if_bundle(client, base_url, dest, Some("https://apkpure.com/"))
            .await
            .map_err(|e| {
                if e.to_string().contains("403") {
                    ApkpureError::Forbidden {
                        url: base_url.to_string(),
                    }
                } else {
                    ApkpureError::Other(e)
                }
            })?;
        return Ok(dest.to_string());
    }
    fetch_apkpure(client, base_url, dest, arch, target_version, fetcher).await
}

async fn resolve_apkpure_cdn_url(url: &str, referer: Option<&str>) -> String {
    if !url.contains("d.apkpure.") {
        return url.to_string();
    }
    let no_redirect_client = match Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return url.to_string(),
    };

    let referers_to_try = [
        referer,
        Some("https://apkpure.com/"),
        Some("https://apkpure.net/"),
        None,
    ];

    for r_opt in referers_to_try {
        let mut req = no_redirect_client
            .get(url)
            .header(reqwest::header::USER_AGENT, crate::utils::DEFAULT_UA)
            .header(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            );
        if let Some(r) = r_opt {
            req = req.header(reqwest::header::REFERER, r);
        }
        if let Ok(resp) = req.send().await {
            if resp.status().is_redirection() {
                if let Some(loc) = resp.headers().get(reqwest::header::LOCATION) {
                    if let Ok(loc_str) = loc.to_str() {
                        info!("apkpure: resolved CDN redirect {url} -> {loc_str}");
                        return loc_str.to_string();
                    }
                }
            }
        }
    }
    url.to_string()
}

pub(crate) async fn fetch_apkpure(
    client: &Client,
    package_or_url: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String, ApkpureError> {
    info!("apkpure: resolving download for {package_or_url}");
    let (app_url, package) = if package_or_url.starts_with("http") {
        let pkg = package_or_url
            .split('/')
            .rfind(|s| !s.is_empty())
            .unwrap_or(package_or_url)
            .to_string();
        (package_or_url.to_string(), pkg)
    } else {
        (
            format!("https://apkpure.com/app/{package_or_url}"),
            package_or_url.to_string(),
        )
    };

    let canonical_url: Option<String> = if let Some(f) = fetcher {
        match crate::builder::fetch_page_html(client, &app_url, Some(f)).await {
            Ok((_body, _hdrs)) => {
                let u = app_url.trim_end_matches('/').to_string();
                info!("apkpure: canonical app URL is {u} (fetcher)");
                Some(u)
            }
            Err(e) => {
                warn!(
                    "apkpure: app page {app_url} fetcher error: {e}, proceeding direct endpoints"
                );
                None
            }
        }
    } else {
        let app_resp = client
            .get(&app_url)
            .header("Referer", "https://apkpure.com/")
            .send()
            .await;
        match app_resp {
            Ok(r) if r.status().is_success() => {
                let u = r.url().as_str().trim_end_matches('/').to_string();
                info!("apkpure: canonical app URL is {u}");
                Some(u)
            }
            Ok(r) => {
                warn!(
                    "apkpure: app page {app_url} returned status {}, proceeding to direct endpoints",
                    r.status()
                );
                None
            }
            Err(e) => {
                warn!("apkpure: app page {app_url} request error: {e}, proceeding to direct endpoints");
                None
            }
        }
    };

    let mut dl_pages = Vec::new();
    if let Some(ref can_url) = canonical_url {
        if let Some(ver) = target_version {
            let ver_clean = ver.replace(' ', "-");
            dl_pages.push(format!("{can_url}/download/{ver_clean}"));
        } else {
            dl_pages.push(format!("{can_url}/download"));
        }
    }

    let re_dl_link = Regex::new(
        r#"href=[\x22\x27](https://d\.apkpure\.(?:com|net)/b/(?:XAPK|APK)/[^\x22\x27]+)[\x22\x27]"#,
    )
    .map_err(|e| ApkpureError::Other(e.into()))?;

    let mut found_dl_links: Vec<String> = Vec::new();
    let mut last_page_url = canonical_url
        .clone()
        .unwrap_or_else(|| "https://apkpure.com/".to_string());

    for dl_page in &dl_pages {
        info!("apkpure: inspecting download page {dl_page}");
        let html = if let Some(f) = fetcher {
            match crate::builder::fetch_page_html(client, dl_page, Some(f)).await {
                Ok((body, _hdrs)) => body,
                Err(e) => {
                    warn!("apkpure: download page {dl_page} fetcher error: {e}");
                    continue;
                }
            }
        } else {
            let referer_header = canonical_url.as_deref().unwrap_or("https://apkpure.com/");
            let resp = match client
                .get(dl_page)
                .header("Referer", referer_header)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!("apkpure: download page {dl_page} error: {e}");
                    continue;
                }
            };
            if !resp.status().is_success() {
                warn!(
                    "apkpure: download page {dl_page} returned status {}",
                    resp.status()
                );
                continue;
            }
            match resp.text().await {
                Ok(h) => h,
                Err(_) => continue,
            }
        };

        last_page_url = dl_page.clone();

        for cap in re_dl_link.captures_iter(&html) {
            let link = cap[1].replace("&amp;", "&");
            if !found_dl_links.contains(&link) {
                found_dl_links.push(link);
            }
        }

        if !found_dl_links.is_empty() {
            break;
        }
    }

    if found_dl_links.is_empty() {
        info!("apkpure: no direct download links found on page, falling back to constructed endpoints");
        if let Some(ver) = target_version {
            let ver_encoded = ver.replace(' ', "+");
            found_dl_links.push(format!(
                "https://d.apkpure.com/b/XAPK/{package}?versionName={ver_encoded}"
            ));
        } else {
            found_dl_links.push(format!(
                "https://d.apkpure.com/b/XAPK/{package}?version=latest"
            ));
        }
    }

    found_dl_links.sort_by_key(|link| {
        if link.contains(arch) {
            0
        } else if link.contains("universal") || link.contains("all") {
            1
        } else {
            2
        }
    });

    let mut had_403 = false;
    let mut last_403_url = String::new();

    for raw_url in &found_dl_links {
        info!("apkpure: attempting download from {raw_url}");
        let resolved_url = resolve_apkpure_cdn_url(raw_url, Some(&last_page_url)).await;
        let referer_candidates = [
            Some(last_page_url.as_str()),
            Some("https://apkpure.com/"),
            Some("https://apkpure.net/"),
            None,
        ];

        for ref_opt in referer_candidates {
            match download_and_extract_if_bundle(client, &resolved_url, dest, ref_opt).await {
                Ok(_)
                    if Path::new(dest).exists()
                        && std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0) > 1_000_000 =>
                {
                    info!("apkpure: successfully downloaded {package} to {dest}");
                    return Ok(dest.to_string());
                }
                Ok(_) => {
                    warn!("apkpure: downloaded file too small or missing for {resolved_url}");
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    if err_msg.contains("403") {
                        had_403 = true;
                        last_403_url = resolved_url.clone();
                        warn!(
                            "apkpure: 403 on {resolved_url} (Referer: {ref_opt:?}) — trying next referer/endpoint"
                        );
                    } else {
                        warn!("apkpure: failed to download {resolved_url}: {e}");
                        break;
                    }
                }
            }
        }
    }

    if had_403 {
        return Err(ApkpureError::Forbidden { url: last_403_url });
    }

    Err(ApkpureError::Other(anyhow::anyhow!(
        "APKPure download failed for {package}"
    )))
}

fn is_zip_file(path: &str) -> bool {
    if let Ok(mut file) = std::fs::File::open(path) {
        use std::io::Read;
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_ok() {
            return magic == [0x50, 0x4B, 0x03, 0x04];
        }
    }
    false
}

fn is_split_bundle(zip_path: &str) -> bool {
    let file = match std::fs::File::open(zip_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut apk_count = 0;
    for i in 0..archive.len() {
        if let Ok(item) = archive.by_index(i) {
            if let Ok(name_cow) = item.name() {
                let name = name_cow.to_lowercase();
                if name.ends_with(".apk") {
                    apk_count += 1;
                    if apk_count > 1 || name.starts_with("split_") || name.contains(".config.") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

async fn merge_split_apk_bundle(bundle_path: &str, dest_apk: &str) -> Result<()> {
    let jar_candidates = ["tools/APKEditor.jar", "APKEditor.jar"];
    let jar_path = match jar_candidates.iter().map(Path::new).find(|p| p.exists()) {
        Some(p) => p.to_path_buf(),
        None => {
            info!("APKEditor.jar not found, attempting auto-download...");
            match crate::utils::ensure_apkeditor_jar("tools").await {
                Ok(p) => PathBuf::from(p),
                Err(e) => anyhow::bail!("APKEditor.jar not found and could not download: {e}"),
            }
        }
    };

    info!("Merging split APK bundle {bundle_path} into {dest_apk} using APKEditor...");
    let status = std::process::Command::new("java")
        .args([
            "-jar",
            jar_path.to_str().unwrap(),
            "m",
            "-i",
            bundle_path,
            "-o",
            dest_apk,
            "-f",
        ])
        .status()?;

    if !status.success() {
        anyhow::bail!(
            "APKEditor failed to merge split bundle (exit: {:?})",
            status.code()
        );
    }
    Ok(())
}

fn extract_base_apk_from_zip(zip_path: &str, dest: &str) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut best_index: Option<usize> = None;
    let mut max_size: u64 = 0;

    for i in 0..archive.len() {
        let item = archive.by_index(i)?;
        let name = item.name()?.to_lowercase();
        if name.ends_with(".apk") {
            if name.ends_with("base.apk") {
                best_index = Some(i);
                break;
            }
            if item.size() > max_size {
                max_size = item.size();
                best_index = Some(i);
            }
        }
    }

    let idx = best_index.ok_or_else(|| anyhow::anyhow!("No .apk found inside ZIP bundle"))?;
    let mut entry = archive.by_index(idx)?;
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(&mut entry, &mut out)?;

    info!("Extracted {} bytes to {dest}", entry.size());
    Ok(())
}

pub fn strip_split_requirements_from_apk(apk_path: &str) -> Result<bool> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let src_file = std::fs::File::open(apk_path)?;
    let mut archive = zip::ZipArchive::new(src_file)?;

    let mut manifest_bytes: Option<Vec<u8>> = None;
    let mut other_entries: Vec<(String, Vec<u8>, zip::CompressionMethod)> = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.to_string();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;

        let compression = entry.compression();
        if name == "AndroidManifest.xml" {
            manifest_bytes = Some(data);
        } else {
            other_entries.push((name, data, compression));
        }
    }

    let manifest = match manifest_bytes {
        Some(m) => m,
        None => return Ok(false),
    };

    let sanitized_manifest = match sanitize_split_manifest(&manifest) {
        Some(m) => m,
        None => return Ok(false),
    };

    let temp_out = format!("{apk_path}.nosplit.tmp");
    let out_file = std::fs::File::create(&temp_out)?;
    let mut zip_out = zip::ZipWriter::new(out_file);
    let def_opts =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip_out.start_file("AndroidManifest.xml", def_opts)?;
    zip_out.write_all(&sanitized_manifest)?;

    for (name, data, compression) in other_entries {
        let method = if name == "resources.arsc" {
            zip::CompressionMethod::Stored
        } else {
            compression
        };
        let opts = SimpleFileOptions::default().compression_method(method);
        zip_out.start_file(&name, opts)?;
        zip_out.write_all(&data)?;
    }

    zip_out.finish()?;
    std::fs::remove_file(apk_path)?;
    std::fs::rename(&temp_out, apk_path)?;

    info!(
        "{apk_path}: stripped requiredSplitTypes and split requirements from AndroidManifest.xml"
    );
    Ok(true)
}

fn sanitize_split_manifest(manifest: &[u8]) -> Option<Vec<u8>> {
    let target_resid = 0x0101064eu32.to_le_bytes();
    let mut has_resid = false;
    for window in manifest.windows(4) {
        if window == target_resid {
            has_resid = true;
            break;
        }
    }
    if !has_resid {
        return None;
    }

    let new_manifest = manifest.to_vec();

    let mut resmap_pos = None;
    for (i, window) in new_manifest.windows(4).enumerate() {
        if window == [0x80, 0x01, 0x08, 0x00] {
            resmap_pos = Some(i);
            break;
        }
    }

    let pos = resmap_pos?;
    let size = u32::from_le_bytes(new_manifest[pos + 4..pos + 8].try_into().ok()?) as usize;
    let map_slice = &new_manifest[pos + 8..pos + size];
    let mut target_indices = Vec::new();
    for (idx, chunk) in map_slice.chunks_exact(4).enumerate() {
        let resid = u32::from_le_bytes(chunk.try_into().unwrap());
        if resid == 0x0101064e || resid == 0x0101064f {
            target_indices.push(idx);
        }
    }

    if target_indices.is_empty() {
        return None;
    }

    let mut elem_pos = None;
    for (i, window) in new_manifest.windows(4).enumerate() {
        if window == [0x02, 0x01, 0x10, 0x00] && i + 36 <= new_manifest.len() {
            let chunk_size =
                u32::from_le_bytes(new_manifest[i + 4..i + 8].try_into().unwrap()) as usize;
            let attr_count =
                u16::from_le_bytes(new_manifest[i + 28..i + 30].try_into().unwrap()) as usize;
            if attr_count > 0
                && attr_count < 200
                && chunk_size == 36 + attr_count * 20
                && i + chunk_size <= new_manifest.len()
            {
                let attrs_start = i + 36;
                let mut contains_target = false;
                for a in 0..attr_count {
                    let a_off = attrs_start + a * 20;
                    let name_idx =
                        i32::from_le_bytes(new_manifest[a_off + 4..a_off + 8].try_into().unwrap())
                            as usize;
                    if target_indices.contains(&name_idx) {
                        contains_target = true;
                        break;
                    }
                }
                if contains_target {
                    elem_pos = Some((i, chunk_size, attr_count));
                    break;
                }
            }
        }
    }

    let (pos, chunk_size, attr_count) = elem_pos?;
    let attrs_start = pos + 36;
    let mut new_attrs = Vec::new();
    let mut removed = 0;

    for a in 0..attr_count {
        let a_off = attrs_start + a * 20;
        let name_idx =
            i32::from_le_bytes(new_manifest[a_off + 4..a_off + 8].try_into().unwrap()) as usize;
        if target_indices.contains(&name_idx) {
            removed += 1;
        } else {
            new_attrs.extend_from_slice(&new_manifest[a_off..a_off + 20]);
        }
    }

    if removed == 0 {
        return None;
    }

    let mut result = Vec::new();
    result.extend_from_slice(&new_manifest[..pos]);

    let mut elem_hdr = new_manifest[pos..pos + 36].to_vec();
    let new_chunk_size = (36 + (attr_count - removed) * 20) as u32;
    elem_hdr[4..8].copy_from_slice(&new_chunk_size.to_le_bytes());
    elem_hdr[28..30].copy_from_slice(&((attr_count - removed) as u16).to_le_bytes());
    result.extend_from_slice(&elem_hdr);
    result.extend_from_slice(&new_attrs);
    result.extend_from_slice(&new_manifest[pos + chunk_size..]);

    let total_len = result.len() as u32;
    result[4..8].copy_from_slice(&total_len.to_le_bytes());

    let old_s_u16: Vec<u8> = "com.android.vending.splits"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let new_s_u16: Vec<u8> = "com.android.vending.spl_ds"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();

    for pos in 0..result.len().saturating_sub(old_s_u16.len()) {
        if result[pos..pos + old_s_u16.len()] == old_s_u16[..] {
            result[pos..pos + new_s_u16.len()].copy_from_slice(&new_s_u16);
        }
    }

    let old_s_u8 = b"com.android.vending.splits";
    let new_s_u8 = b"com.android.vending.spl_ds";
    for pos in 0..result.len().saturating_sub(old_s_u8.len()) {
        if result[pos..pos + old_s_u8.len()] == old_s_u8[..] {
            result[pos..pos + new_s_u8.len()].copy_from_slice(new_s_u8);
        }
    }

    Some(result)
}
