use crate::config::Config;
use crate::report::BuildReport;
use crate::testing::TestReport;
use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const APPS_JSON_SCHEMA_VERSION: &str = "2.0.0";

const ALLOWED_CATALOG_ARCHS: &[&str] = &["arm64-v8a", "armeabi-v7a", "x86_64", "all"];

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BundlePayloadEntry {
    pub app: String,
    pub package: String,
    pub version: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleaseAssetInfo {
    pub filename: String,
    pub url: String,
    pub tag: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_archs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimization_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Vec<BundlePayloadEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildHealthSummary {
    pub release_tag: Option<String>,
    pub cli_version: String,
    pub built_at: Option<String>,
    pub pass_rate: Option<f64>,
    pub active_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleasesRegistry {
    #[serde(default)]
    pub stock_apps: BTreeMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub root_apps: BTreeMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub single_modules: BTreeMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub bundles: BTreeMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub custom_bundles: BTreeMap<String, ReleaseAssetInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_health: Option<BuildHealthSummary>,
}

#[derive(Debug, Clone, Default)]
pub struct GenAppsJsonOptions {
    pub registry_path: Option<String>,
    pub build_report_path: Option<String>,
    pub test_report_path: Option<String>,
    pub output_dir: Option<String>,
    pub release_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct EnrichedReleaseAsset {
    filename: String,
    url: String,
    download_url: String,
    tag: String,
    release_tag: String,
    updated_at: Option<String>,
    source_repo: Option<String>,
    asset_pattern: Option<String>,
    version: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    supported_archs: Vec<String>,
    optimization_mode: String,
    validation_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AppReleases {
    stock: Option<EnrichedReleaseAsset>,
    module: Option<EnrichedReleaseAsset>,
}

#[derive(Debug, Clone, Serialize)]
struct ModuleFlagsEntry {
    single: bool,
    bundle: bool,
}

#[derive(Debug, Clone, Serialize)]
struct EnrichedAppEntry {
    id: String,
    name: String,
    enabled: bool,
    package: String,
    patch_source: String,
    source_repo: Option<String>,
    asset_pattern: Option<String>,
    categories: Vec<String>,
    architectures: Vec<String>,
    supported_archs: Vec<String>,
    mode: String,
    optimization_mode: String,
    patches: Vec<String>,
    dependencies: Vec<String>,
    module: ModuleFlagsEntry,
    version: Option<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    validation_status: String,
    download_url: Option<String>,
    tag: Option<String>,
    release_tag: Option<String>,
    updated_at: Option<String>,
    releases: AppReleases,
}

#[derive(Debug, Clone, Serialize)]
struct EnrichedBundleEntry {
    id: String,
    module_id: String,
    name: String,
    category: String,
    filename: String,
    source_repo: Option<String>,
    asset_pattern: Option<String>,
    sources: Vec<String>,
    sha256: Option<String>,
    size_bytes: Option<u64>,
    release_tag: Option<String>,
    tag: Option<String>,
    updated_at: Option<String>,
    supported_archs: Vec<String>,
    optimization_mode: String,
    download_url: Option<String>,
    validation_status: String,
    payload: Vec<BundlePayloadEntry>,
}

#[derive(Debug, Clone, Serialize)]
struct AppsCatalogDocument {
    schema_version: String,
    build_health: BuildHealthSummary,
    apps: Vec<EnrichedAppEntry>,
    bundles: Vec<EnrichedBundleEntry>,
    custom_bundles: Vec<EnrichedBundleEntry>,
}

pub fn load_registry(path: &str) -> ReleasesRegistry {
    if Path::new(path).exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(reg) = serde_json::from_str::<ReleasesRegistry>(&content) {
                return reg;
            }
        }
    }
    ReleasesRegistry::default()
}

pub fn normalize_iso8601_utc(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(
            dt.with_timezone(&Utc)
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        );
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S UTC",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%SZ",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Some(
                DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string(),
            );
        }
    }
    None
}

pub fn resolve_provenance(
    cfg: &Config,
    app: &crate::config::AppConfig,
) -> (Option<String>, Option<String>) {
    if !app.patch_source.is_empty() && app.patch_source != "none" {
        if let Some(ps) = cfg.patch_sources.get(&app.patch_source) {
            return (Some(ps.repo.clone()), Some(ps.asset_pattern.clone()));
        }
    }
    if let Some(ref url) = app.apk_url {
        if let Some(rest) = url.strip_prefix("https://github.com/") {
            let mut parts = rest.split('/').filter(|s| !s.is_empty());
            if let (Some(owner), Some(repo)) = (parts.next(), parts.next()) {
                return (Some(format!("{owner}/{repo}")), Some("*.apk".to_string()));
            }
        }
    }
    (None, None)
}

pub fn normalize_supported_archs(raw: &[String]) -> Vec<String> {
    let mut set = BTreeSet::new();
    for item in raw {
        let trimmed = item.trim();
        if trimmed.eq_ignore_ascii_case("universal") || trimmed.eq_ignore_ascii_case("all") {
            set.insert("all");
        } else if ALLOWED_CATALOG_ARCHS.contains(&trimmed) {
            set.insert(trimmed);
        }
    }
    ALLOWED_CATALOG_ARCHS
        .iter()
        .filter(|a| set.contains(**a))
        .map(|a| (*a).to_string())
        .collect()
}

pub fn infer_archs_from_filename(filename: &str) -> Option<Vec<String>> {
    for arch in ["arm64-v8a", "armeabi-v7a", "x86_64"] {
        if filename.contains(arch) {
            return Some(vec![arch.to_string()]);
        }
    }
    None
}

pub fn extract_version_from_asset_filename(filename: &str) -> Option<String> {
    let stem = filename
        .strip_suffix(".apk")
        .or_else(|| filename.strip_suffix(".zip"))
        .unwrap_or(filename)
        .strip_suffix("-module")
        .unwrap_or(
            filename
                .strip_suffix(".apk")
                .or_else(|| filename.strip_suffix(".zip"))
                .unwrap_or(filename),
        );
    if stem.ends_with("-patched") || stem == "microg" {
        return None;
    }
    let parts: Vec<&str> = stem.split('-').collect();
    for (idx, part) in parts.iter().enumerate() {
        let clean = part.strip_prefix('v').unwrap_or(part);
        if clean.chars().next().is_some_and(|c| c.is_ascii_digit()) && clean.contains('.') {
            let mut ver = clean.to_string();
            if let Some(next) = parts.get(idx + 1) {
                if next.starts_with("prod") || next.starts_with("beta") || next.starts_with("rc") {
                    ver.push('-');
                    ver.push_str(next);
                }
            }
            return Some(ver);
        }
    }
    None
}

/// Optimization mode semantics in revancex:
/// - "full": standard patched/signed APK keeping all original resources and metadata.
/// - "lite": runs `builder::lite::optimize_apk` post-patch to strip stale signature files,
///   `*.properties`, `assets/dexopt/*`, and `kotlin-tooling-metadata.json`.
pub fn normalize_optimization_mode(raw: &str) -> String {
    if raw.eq_ignore_ascii_case("lite") {
        "lite".to_string()
    } else {
        "full".to_string()
    }
}

fn resolve_app_categories(cfg: &Config, app_id: &str) -> Vec<String> {
    let mut cats = BTreeSet::new();
    for (key, def) in &cfg.modules {
        if key == "all" {
            continue;
        }
        if def.apps.iter().any(|a| a == app_id) || def.disabled_apps.iter().any(|a| a == app_id) {
            cats.insert(key.clone());
        }
    }
    cats.into_iter().collect()
}

fn resolve_validation_status(
    app_id: &str,
    build_rep: Option<&crate::report::AppBuildReport>,
    test_item: Option<&crate::testing::TestReportItem>,
    reg_asset: Option<&ReleaseAssetInfo>,
) -> String {
    if let Some(ti) = test_item {
        return match ti.status.as_str() {
            "passed" => {
                if build_rep.is_some_and(|r| {
                    r.status == "degraded"
                        || r.status == "success_unpatched"
                        || !r.warnings.is_empty()
                }) {
                    "warning".to_string()
                } else {
                    "passed".to_string()
                }
            }
            "failed" => "failed".to_string(),
            _ => "unknown".to_string(),
        };
    }

    if let Some(rep) = build_rep {
        if let Some(ref vs) = rep.validation_status {
            return vs.clone();
        }
        return match rep.status.as_str() {
            "success" => {
                if rep
                    .integrity
                    .as_ref()
                    .is_some_and(|i| !i.package_ok || !i.version_ok || !i.cert_ok)
                {
                    "warning".to_string()
                } else {
                    "passed".to_string()
                }
            }
            "degraded" | "success_unpatched" => "warning".to_string(),
            "failed" => "failed".to_string(),
            _ => "unknown".to_string(),
        };
    }

    if let Some(asset) = reg_asset {
        if let Some(ref vs) = asset.validation_status {
            return vs.clone();
        }
    }

    let _ = app_id;
    "unknown".to_string()
}

struct ReleaseEnrichCtx<'a> {
    source_repo: Option<String>,
    asset_pattern: Option<String>,
    default_archs: &'a [String],
    default_opt_mode: &'a str,
    validation_status: String,
    output_dir: Option<&'a str>,
    module_id: Option<String>,
}

fn build_enriched_release(
    rel: &ReleaseAssetInfo,
    ctx: ReleaseEnrichCtx<'_>,
) -> EnrichedReleaseAsset {
    let mut sha256 = rel.sha256.clone();
    let mut size_bytes = rel.size_bytes;
    let mut file_archs = rel.supported_archs.clone();

    if let Some(dir) = ctx.output_dir {
        let candidate = Path::new(dir).join(&rel.filename);
        if candidate.is_file() {
            if sha256.is_none() {
                sha256 = crate::utils::apk::compute_file_sha256(&candidate).ok();
            }
            if size_bytes.is_none() {
                size_bytes = std::fs::metadata(&candidate).ok().map(|m| m.len());
            }
            if file_archs.is_none() && rel.filename.ends_with(".apk") {
                let p_str = candidate.to_string_lossy();
                let mut found = Vec::new();
                if crate::builder::arch::apk_has_any_native_libs(&p_str).unwrap_or(true) {
                    for arch in ["arm64-v8a", "armeabi-v7a", "x86_64"] {
                        if crate::builder::arch::apk_has_arch(&p_str, arch).unwrap_or(false) {
                            found.push(arch.to_string());
                        }
                    }
                } else {
                    found.push("all".to_string());
                }
                if !found.is_empty() {
                    file_archs = Some(found);
                }
            }
        }
    }

    let supported_archs = file_archs
        .or_else(|| infer_archs_from_filename(&rel.filename))
        .map(|v| normalize_supported_archs(&v))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| normalize_supported_archs(ctx.default_archs));

    let optimization_mode = rel
        .optimization_mode
        .as_deref()
        .map(normalize_optimization_mode)
        .unwrap_or_else(|| ctx.default_opt_mode.to_string());

    let version = rel
        .version
        .clone()
        .or_else(|| extract_version_from_asset_filename(&rel.filename));

    EnrichedReleaseAsset {
        filename: rel.filename.clone(),
        url: rel.url.clone(),
        download_url: rel.url.clone(),
        tag: rel.tag.clone(),
        release_tag: rel.tag.clone(),
        updated_at: normalize_iso8601_utc(&rel.updated_at),
        source_repo: rel.source_repo.clone().or(ctx.source_repo),
        asset_pattern: rel.asset_pattern.clone().or(ctx.asset_pattern),
        version,
        sha256,
        size_bytes,
        supported_archs,
        optimization_mode,
        validation_status: ctx.validation_status,
        module_id: ctx.module_id,
    }
}

fn bundle_display_name(cfg: &Config, key: &str) -> String {
    if let Some(module_def) = cfg.modules.get(key) {
        if let Some(ref n) = module_def.name {
            return n.clone();
        }
    }
    let title = key
        .split(['_', '-'])
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("ReVanceX {title} Bundle")
}

fn aggregate_validation_status<'a>(statuses: impl Iterator<Item = &'a str>) -> String {
    let mut any = false;
    let mut has_warn = false;
    let mut all_passed = true;
    for st in statuses {
        any = true;
        match st {
            "failed" => return "failed".to_string(),
            "warning" => {
                has_warn = true;
                all_passed = false;
            }
            "passed" => {}
            _ => all_passed = false,
        }
    }
    if !any {
        "unknown".to_string()
    } else if has_warn {
        "warning".to_string()
    } else if all_passed {
        "passed".to_string()
    } else {
        "unknown".to_string()
    }
}

fn inspect_bundle_zip_payload(
    cfg: &Config,
    zip_path: &Path,
    app_versions: &BTreeMap<String, Option<String>>,
) -> Option<Vec<BundlePayloadEntry>> {
    let file = std::fs::File::open(zip_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entries = BTreeMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).ok()?;
        let name = entry.name().ok()?.to_string();
        if let Some(rest) = name.strip_prefix("apks/") {
            if let Some(stem) = rest.strip_suffix(".apk") {
                let app_id = cfg.resolve_app_id(stem).unwrap_or(stem).to_string();
                let pkg = cfg
                    .apps
                    .get(&app_id)
                    .map(|a| a.package.clone())
                    .unwrap_or_default();
                let ver = app_versions.get(&app_id).cloned().flatten();
                entries.insert(
                    app_id.clone(),
                    BundlePayloadEntry {
                        app: app_id,
                        package: pkg,
                        version: ver,
                        path: name,
                    },
                );
            }
        }
    }
    Some(entries.into_values().collect())
}

fn compute_build_health(
    cfg: &Config,
    registry: &ReleasesRegistry,
    build_report: Option<&BuildReport>,
    test_report: Option<&TestReport>,
    release_tag: Option<&str>,
) -> BuildHealthSummary {
    let mut active_sources_set = BTreeSet::new();
    for app in cfg.apps.values() {
        if app.enabled && !app.patch_source.is_empty() && app.patch_source != "none" {
            if let Some(ps) = cfg.patch_sources.get(&app.patch_source) {
                active_sources_set.insert(ps.repo.clone());
            } else {
                active_sources_set.insert(app.patch_source.clone());
            }
        }
    }
    if let Some(ref rh) = registry.build_health {
        for s in &rh.active_sources {
            if !s.is_empty() {
                active_sources_set.insert(s.clone());
            }
        }
    }

    let latest_auto_asset = registry
        .stock_apps
        .values()
        .chain(registry.single_modules.values())
        .chain(registry.bundles.values())
        .filter(|a| a.tag.starts_with("auto-v"))
        .max_by(|a, b| {
            normalize_iso8601_utc(&a.updated_at)
                .cmp(&normalize_iso8601_utc(&b.updated_at))
                .then_with(|| a.tag.cmp(&b.tag))
        });

    let resolved_tag = release_tag
        .filter(|t| t.starts_with("auto-v"))
        .map(String::from)
        .or_else(|| {
            registry
                .build_health
                .as_ref()
                .and_then(|h| h.release_tag.clone())
        })
        .or_else(|| latest_auto_asset.map(|a| a.tag.clone()));

    let cli_version = build_report
        .map(|r| r.revancex_version.clone())
        .or_else(|| {
            registry
                .build_health
                .as_ref()
                .map(|h| h.cli_version.clone())
        })
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    let built_at = build_report
        .and_then(|r| normalize_iso8601_utc(&r.build_timestamp))
        .or_else(|| {
            registry
                .build_health
                .as_ref()
                .and_then(|h| h.built_at.as_deref().and_then(normalize_iso8601_utc))
        })
        .or_else(|| latest_auto_asset.and_then(|a| normalize_iso8601_utc(&a.updated_at)));

    let pass_rate = if let Some(tr) = test_report {
        let total = tr.passed + tr.failed + tr.skipped;
        if total > 0 {
            Some(((tr.passed as f64 / total as f64) * 10000.0).round() / 10000.0)
        } else {
            None
        }
    } else if let Some(br) = build_report {
        let total = br.summary.total;
        if total > 0 {
            let ok = br.summary.success + br.summary.degraded;
            Some(((ok as f64 / total as f64) * 10000.0).round() / 10000.0)
        } else {
            None
        }
    } else {
        registry.build_health.as_ref().and_then(|h| h.pass_rate)
    };

    BuildHealthSummary {
        release_tag: resolved_tag,
        cli_version,
        built_at,
        pass_rate,
        active_sources: active_sources_set.into_iter().collect(),
    }
}

pub fn generate_apps_json(cfg: &Config) -> Result<String> {
    generate_apps_json_with_registry(cfg, Some("config/releases_registry.json"))
}

pub fn generate_apps_json_with_registry(
    cfg: &Config,
    registry_path: Option<&str>,
) -> Result<String> {
    generate_apps_json_with_options(
        cfg,
        &GenAppsJsonOptions {
            registry_path: registry_path.map(String::from),
            ..Default::default()
        },
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OtaUpdateManifest {
    pub version: String,
    #[serde(rename = "versionCode")]
    pub version_code: i32,
    #[serde(rename = "zipUrl")]
    pub zip_url: String,
    pub changelog: String,
}

fn read_zip_module_version_code(zip_path: &Path) -> Option<i32> {
    use std::io::Read;
    let file = std::fs::File::open(zip_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut prop = archive.by_name("module.prop").ok()?;
    let mut content = String::new();
    prop.read_to_string(&mut content).ok()?;
    for line in content.lines() {
        if let Some(val) = line.strip_prefix("versionCode=") {
            if let Ok(vc) = val.trim().parse::<i32>() {
                if vc > 0 {
                    return Some(vc);
                }
            }
        }
    }
    None
}

fn resolve_ota_version_code(
    rel_version_code: Option<i32>,
    filename: &str,
    updated_at: Option<&str>,
    output_dir: Option<&str>,
) -> i32 {
    if let Some(vc) = rel_version_code.filter(|&v| v > 0) {
        return vc;
    }
    if let Some(dir) = output_dir {
        let p = Path::new(dir).join(filename);
        if p.is_file() {
            if let Some(vc) = read_zip_module_version_code(&p) {
                return vc;
            }
        }
    }
    if let Some(ts) = updated_at {
        if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
            if let Ok(vc) = crate::module::compute_version_code(dt.date_naive(), 1) {
                return vc;
            }
        }
    }
    2026010101
}

pub fn generate_apps_json_with_options(cfg: &Config, opts: &GenAppsJsonOptions) -> Result<String> {
    let (doc, _) = build_catalog_document(cfg, opts)?;
    Ok(serde_json::to_string_pretty(&doc)?)
}

pub fn generate_ota_endpoints_with_options(
    cfg: &Config,
    opts: &GenAppsJsonOptions,
    update_dir: &str,
) -> Result<Vec<String>> {
    let (doc, registry) = build_catalog_document(cfg, opts)?;
    std::fs::create_dir_all(update_dir)?;

    let mut written = Vec::new();
    let app_by_id: BTreeMap<&str, &EnrichedAppEntry> =
        doc.apps.iter().map(|a| (a.id.as_str(), a)).collect();

    // 1. Single-app root modules
    for app in &doc.apps {
        if !app.enabled || !app.module.single || app.id.eq_ignore_ascii_case("microg") {
            continue;
        }
        let Some(ref mod_rel) = app.releases.module else {
            continue;
        };
        if !mod_rel.download_url.starts_with("http") || mod_rel.release_tag.is_empty() {
            continue;
        }
        let module_id = mod_rel
            .module_id
            .clone()
            .unwrap_or_else(|| format!("revancex-{}", app.id));
        let reg_vc = registry
            .single_modules
            .get(&app.id)
            .and_then(|r| r.version_code);
        let version_code = resolve_ota_version_code(
            reg_vc,
            &mod_rel.filename,
            mod_rel.updated_at.as_deref(),
            opts.output_dir.as_deref(),
        );

        let manifest = OtaUpdateManifest {
            version: mod_rel.release_tag.clone(),
            version_code,
            zip_url: mod_rel.download_url.clone(),
            changelog: crate::module::ota_changelog_url(&module_id),
        };
        let json_path = Path::new(update_dir).join(format!("{module_id}.json"));
        std::fs::write(&json_path, serde_json::to_string_pretty(&manifest)?)?;
        written.push(json_path.to_string_lossy().to_string());

        let md_path = Path::new(update_dir).join(format!("{module_id}-changelog.md"));
        let mut md = String::new();
        md.push_str(&format!("# ReVanceX — {} (`{module_id}`)\n\n", app.name));
        md.push_str(&format!("- **Release Tag**: `{}`\n", mod_rel.release_tag));
        md.push_str(&format!("- **Version Code**: `{version_code}`\n"));
        md.push_str(&format!(
            "- **Updated**: {}\n",
            mod_rel.updated_at.as_deref().unwrap_or("unknown")
        ));
        md.push_str(&format!(
            "- **Download**: [{}]({})\n\n",
            mod_rel.filename, mod_rel.download_url
        ));
        md.push_str("## Included App & Upstream Patch Source\n\n");
        md.push_str("| App | Package | Version | Payload Path | Upstream Patch Source |\n");
        md.push_str("| --- | --- | --- | --- | --- |\n");
        md.push_str(&format!(
            "| {} | `{}` | {} | `apks/{}.apk` | {} |\n",
            app.name,
            app.package,
            mod_rel
                .version
                .as_deref()
                .or(app.version.as_deref())
                .unwrap_or("latest"),
            app.id,
            app.source_repo.as_deref().unwrap_or(&app.patch_source),
        ));
        std::fs::write(&md_path, md)?;
        written.push(md_path.to_string_lossy().to_string());
    }

    // 2. Multi-app bundle modules
    for bundle in &doc.bundles {
        let (Some(ref url), Some(ref tag)) = (&bundle.download_url, &bundle.release_tag) else {
            continue;
        };
        if !url.starts_with("http") || tag.is_empty() {
            continue;
        }
        let reg_vc = registry
            .bundles
            .get(&bundle.filename)
            .and_then(|r| r.version_code);
        let version_code = resolve_ota_version_code(
            reg_vc,
            &bundle.filename,
            bundle.updated_at.as_deref(),
            opts.output_dir.as_deref(),
        );
        let manifest = OtaUpdateManifest {
            version: tag.clone(),
            version_code,
            zip_url: url.clone(),
            changelog: crate::module::ota_changelog_url(&bundle.module_id),
        };
        let json_path = Path::new(update_dir).join(format!("{}.json", bundle.module_id));
        std::fs::write(&json_path, serde_json::to_string_pretty(&manifest)?)?;
        written.push(json_path.to_string_lossy().to_string());

        let md_path = Path::new(update_dir).join(format!("{}-changelog.md", bundle.module_id));
        let mut md = String::new();
        md.push_str(&format!("# {} (`{}`)\n\n", bundle.name, bundle.module_id));
        md.push_str(&format!("- **Release Tag**: `{tag}`\n"));
        md.push_str(&format!("- **Version Code**: `{version_code}`\n"));
        md.push_str(&format!(
            "- **Updated**: {}\n",
            bundle.updated_at.as_deref().unwrap_or("unknown")
        ));
        md.push_str(&format!("- **Download**: [{}]({url})\n", bundle.filename));
        if !bundle.sources.is_empty() {
            md.push_str(&format!(
                "- **Upstream Patch Sources**: {}\n",
                bundle
                    .sources
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        md.push_str("\n## Payload APKs & App Versions\n\n");
        md.push_str("| App | Package | Version | Payload Path | Upstream Patch Source |\n");
        md.push_str("| --- | --- | --- | --- | --- |\n");
        for item in &bundle.payload {
            let app_entry = app_by_id.get(item.app.as_str());
            let app_name = app_entry
                .map(|a| a.name.as_str())
                .unwrap_or(item.app.as_str());
            let src = app_entry
                .and_then(|a| a.source_repo.as_deref())
                .unwrap_or("-");
            let ver = item.version.as_deref().unwrap_or("latest");
            md.push_str(&format!(
                "| {app_name} | `{}` | {ver} | `{}` | {src} |\n",
                item.package, item.path
            ));
        }
        std::fs::write(&md_path, md)?;
        written.push(md_path.to_string_lossy().to_string());
    }

    written.sort();
    Ok(written)
}

fn build_catalog_document(
    cfg: &Config,
    opts: &GenAppsJsonOptions,
) -> Result<(AppsCatalogDocument, ReleasesRegistry)> {
    let registry = opts
        .registry_path
        .as_deref()
        .map(load_registry)
        .unwrap_or_default();

    let build_report: Option<BuildReport> = opts
        .build_report_path
        .as_deref()
        .filter(|p| Path::new(p).exists())
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok());

    let test_report: Option<TestReport> = opts
        .test_report_path
        .as_deref()
        .filter(|p| Path::new(p).exists())
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok());

    let test_by_app: BTreeMap<String, crate::testing::TestReportItem> = test_report
        .as_ref()
        .map(|tr| {
            tr.items
                .iter()
                .map(|it| (it.app.clone(), it.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut app_versions: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut app_validations: BTreeMap<String, String> = BTreeMap::new();
    let mut app_sources: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut app_archs: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let mut entries: Vec<_> = cfg.apps.iter().collect();
    entries.sort_by_key(|(id, _)| *id);

    let apps: Vec<EnrichedAppEntry> = entries
        .into_iter()
        .map(|(id, app)| {
            let stock_rel = registry.stock_apps.get(id);
            let mod_rel = registry.single_modules.get(id);
            let build_rep = build_report.as_ref().and_then(|br| br.apps.get(id));
            let test_item = test_by_app.get(id);

            let (source_repo, asset_pattern) = resolve_provenance(cfg, app);
            let categories = resolve_app_categories(cfg, id);
            let default_opt = build_rep
                .and_then(|r| r.optimization_mode.as_deref())
                .map(normalize_optimization_mode)
                .unwrap_or_else(|| normalize_optimization_mode(&app.mode));

            let default_archs = {
                let mut archs = if let Some(rep) = build_rep {
                    if !rep.supported_archs.is_empty() {
                        normalize_supported_archs(&rep.supported_archs)
                    } else if let Some(ref am) = rep.arch_mode {
                        normalize_supported_archs(std::slice::from_ref(am))
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };
                if archs.is_empty() {
                    archs = normalize_supported_archs(&app.architectures);
                }
                archs
            };

            let stock_val = resolve_validation_status(id, build_rep, test_item, stock_rel);
            let mod_val = resolve_validation_status(id, build_rep, test_item, mod_rel);

            let enriched_stock = stock_rel.map(|rel| {
                build_enriched_release(
                    rel,
                    ReleaseEnrichCtx {
                        source_repo: source_repo.clone(),
                        asset_pattern: asset_pattern.clone(),
                        default_archs: &default_archs,
                        default_opt_mode: &default_opt,
                        validation_status: stock_val.clone(),
                        output_dir: opts.output_dir.as_deref(),
                        module_id: None,
                    },
                )
            });

            let enriched_mod = mod_rel.map(|rel| {
                build_enriched_release(
                    rel,
                    ReleaseEnrichCtx {
                        source_repo: source_repo.clone(),
                        asset_pattern: asset_pattern.clone(),
                        default_archs: &default_archs,
                        default_opt_mode: &default_opt,
                        validation_status: mod_val.clone(),
                        output_dir: opts.output_dir.as_deref(),
                        module_id: Some(format!("revancex-{id}")),
                    },
                )
            });

            let primary = enriched_stock.as_ref().or(enriched_mod.as_ref());
            let version = primary
                .and_then(|p| p.version.clone())
                .or_else(|| build_rep.and_then(|r| r.resolved_version.clone()))
                .or_else(|| {
                    app.version
                        .clone()
                        .filter(|v| !v.eq_ignore_ascii_case("auto"))
                });

            let sha256 = primary
                .and_then(|p| p.sha256.clone())
                .or_else(|| build_rep.and_then(|r| r.sha256.clone()));
            let size_bytes = primary
                .and_then(|p| p.size_bytes)
                .or_else(|| build_rep.and_then(|r| r.size_bytes));
            let supported_archs = primary
                .map(|p| p.supported_archs.clone())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| default_archs.clone());
            let optimization_mode = primary
                .map(|p| p.optimization_mode.clone())
                .unwrap_or_else(|| default_opt.clone());
            let validation_status = primary
                .map(|p| p.validation_status.clone())
                .unwrap_or_else(|| stock_val.clone());

            let download_url = primary.map(|p| p.download_url.clone());
            let tag = primary.map(|p| p.tag.clone());
            let updated_at = primary.and_then(|p| p.updated_at.clone());

            let mut dependencies = app.dependencies.all();
            dependencies.sort();
            dependencies.dedup();

            let mut patches = app.patches.clone();
            patches.sort();

            app_versions.insert(id.clone(), version.clone());
            app_validations.insert(id.clone(), validation_status.clone());
            app_sources.insert(id.clone(), source_repo.clone());
            app_archs.insert(id.clone(), supported_archs.clone());

            EnrichedAppEntry {
                id: id.clone(),
                name: app.display_name(id),
                enabled: app.enabled,
                package: app.package.clone(),
                patch_source: app.patch_source.clone(),
                source_repo,
                asset_pattern,
                categories,
                architectures: app.architectures.clone(),
                supported_archs,
                mode: app.mode.clone(),
                optimization_mode,
                patches,
                dependencies,
                module: ModuleFlagsEntry {
                    single: app.module.as_ref().is_none_or(|m| m.single),
                    bundle: app.module.as_ref().is_none_or(|m| m.bundle),
                },
                version,
                sha256,
                size_bytes,
                validation_status,
                download_url,
                tag: tag.clone(),
                release_tag: tag,
                updated_at,
                releases: AppReleases {
                    stock: enriched_stock,
                    module: enriched_mod,
                },
            }
        })
        .collect();

    let mut bundles = Vec::new();
    let mut seen_bundle_files = BTreeSet::new();
    for (key, module_def) in &cfg.modules {
        let filename = module_def
            .zip_name
            .clone()
            .unwrap_or_else(|| format!("revancex-bundle-{key}.zip"));
        seen_bundle_files.insert(filename.clone());

        let reg_entry = registry.bundles.get(&filename).or_else(|| {
            if key == "core" {
                registry.bundles.get("revancex-bundle.zip")
            } else {
                None
            }
        });
        if let Some(re) = reg_entry {
            seen_bundle_files.insert(re.filename.clone());
        }

        let module_id = module_def
            .module_id
            .clone()
            .unwrap_or_else(|| format!("revancex-{key}"));
        let name = bundle_display_name(cfg, key);

        let mut payload = if let Some(dir) = opts.output_dir.as_deref() {
            let p = Path::new(dir).join(&filename);
            if p.is_file() {
                inspect_bundle_zip_payload(cfg, &p, &app_versions)
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| reg_entry.and_then(|r| r.payload.clone()))
        .unwrap_or_else(|| {
            let mut items = Vec::new();
            for app_id in &module_def.apps {
                if let Some(app_cfg) = cfg.apps.get(app_id) {
                    if !app_cfg.enabled || !app_cfg.module.as_ref().is_none_or(|m| m.bundle) {
                        continue;
                    }
                    items.push(BundlePayloadEntry {
                        app: app_id.clone(),
                        package: app_cfg.package.clone(),
                        version: app_versions.get(app_id).cloned().flatten(),
                        path: format!("apks/{app_id}.apk"),
                    });
                }
            }
            items
        });
        payload.sort_by(|a, b| a.app.cmp(&b.app));

        let mut sources_set = BTreeSet::new();
        let mut archs_set = BTreeSet::new();
        let mut val_statuses = Vec::new();
        for item in &payload {
            if let Some(Some(src)) = app_sources.get(&item.app) {
                sources_set.insert(src.clone());
            }
            if let Some(archs) = app_archs.get(&item.app) {
                for a in archs {
                    archs_set.insert(a.clone());
                }
            }
            if let Some(vs) = app_validations.get(&item.app) {
                val_statuses.push(vs.as_str());
            }
        }

        let default_bundle_archs: Vec<String> = if archs_set.is_empty() {
            vec!["arm64-v8a".to_string()]
        } else {
            normalize_supported_archs(&archs_set.into_iter().collect::<Vec<_>>())
        };

        let validation_status = reg_entry
            .and_then(|r| r.validation_status.clone())
            .unwrap_or_else(|| aggregate_validation_status(val_statuses.into_iter()));

        let enriched = reg_entry.map(|re| {
            build_enriched_release(
                re,
                ReleaseEnrichCtx {
                    source_repo: None,
                    asset_pattern: None,
                    default_archs: &default_bundle_archs,
                    default_opt_mode: "full",
                    validation_status: validation_status.clone(),
                    output_dir: opts.output_dir.as_deref(),
                    module_id: Some(module_id.clone()),
                },
            )
        });

        bundles.push(EnrichedBundleEntry {
            id: key.clone(),
            module_id,
            name,
            category: key.clone(),
            filename: enriched
                .as_ref()
                .map(|e| e.filename.clone())
                .unwrap_or(filename),
            source_repo: None,
            asset_pattern: None,
            sources: sources_set.into_iter().collect(),
            sha256: enriched.as_ref().and_then(|e| e.sha256.clone()),
            size_bytes: enriched.as_ref().and_then(|e| e.size_bytes),
            release_tag: enriched.as_ref().map(|e| e.release_tag.clone()),
            tag: enriched.as_ref().map(|e| e.tag.clone()),
            updated_at: enriched.as_ref().and_then(|e| e.updated_at.clone()),
            supported_archs: enriched
                .as_ref()
                .map(|e| e.supported_archs.clone())
                .unwrap_or(default_bundle_archs),
            optimization_mode: enriched
                .as_ref()
                .map(|e| e.optimization_mode.clone())
                .unwrap_or_else(|| "full".to_string()),
            download_url: enriched.as_ref().map(|e| e.download_url.clone()),
            validation_status,
            payload,
        });
    }
    bundles.sort_by(|a, b| a.id.cmp(&b.id));

    let custom_bundles = registry
        .custom_bundles
        .iter()
        .map(|(fname, rel)| {
            let mut payload = rel.payload.clone().unwrap_or_default();
            payload.sort_by(|a, b| a.app.cmp(&b.app));
            let val = rel
                .validation_status
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let enriched = build_enriched_release(
                rel,
                ReleaseEnrichCtx {
                    source_repo: None,
                    asset_pattern: None,
                    default_archs: &["arm64-v8a".to_string()],
                    default_opt_mode: "full",
                    validation_status: val.clone(),
                    output_dir: opts.output_dir.as_deref(),
                    module_id: None,
                },
            );
            let id = fname.trim_end_matches(".zip").to_string();
            EnrichedBundleEntry {
                id: id.clone(),
                module_id: format!("revancex-{id}"),
                name: fname.clone(),
                category: "custom".to_string(),
                filename: enriched.filename,
                source_repo: enriched.source_repo,
                asset_pattern: enriched.asset_pattern,
                sources: vec![],
                sha256: enriched.sha256,
                size_bytes: enriched.size_bytes,
                release_tag: Some(enriched.release_tag),
                tag: Some(enriched.tag),
                updated_at: enriched.updated_at,
                supported_archs: enriched.supported_archs,
                optimization_mode: enriched.optimization_mode,
                download_url: Some(enriched.download_url),
                validation_status: val,
                payload,
            }
        })
        .collect();

    let build_health = compute_build_health(
        cfg,
        &registry,
        build_report.as_ref(),
        test_report.as_ref(),
        opts.release_tag.as_deref(),
    );

    Ok((
        AppsCatalogDocument {
            schema_version: APPS_JSON_SCHEMA_VERSION.to_string(),
            build_health,
            apps,
            bundles,
            custom_bundles,
        },
        registry,
    ))
}
