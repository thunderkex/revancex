use crate::orchestrator::scheduler::BuildSummary;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    AllSourcesExhausted,
    VersionNotSupported,
    NoEnabledPatches,
    IntegrityCheckFailed,
    PatcherError,
    VersionConflict,
    CompatibilityUnknown,
    ArchUnsupported,
    Other,
}

impl FailureCategory {
    pub fn from_error_message(err: &str) -> Self {
        let lower = err.to_lowercase();
        if lower.contains("no enabled patches") || lower.contains("0 enabled patches") {
            FailureCategory::NoEnabledPatches
        } else if lower.contains("all download sources exhausted") {
            FailureCategory::AllSourcesExhausted
        } else if lower.contains("not covered by any enabled patch")
            || lower.contains("version mismatch")
            || lower.contains("does not match target")
            || lower.contains("outside compatible patch range")
            || lower.contains("version not supported")
        {
            FailureCategory::VersionNotSupported
        } else if lower.contains("integrity check")
            || lower.contains("notzip")
            || lower.contains("corruptedzip")
            || lower.contains("notapk")
            || lower.contains("not a zip")
        {
            FailureCategory::IntegrityCheckFailed
        } else if lower.contains("version conflict") {
            FailureCategory::VersionConflict
        } else if lower.contains("patch compatibility unknown")
            || lower.contains("refusing to build latest")
        {
            FailureCategory::CompatibilityUnknown
        } else if lower.contains("foreign architectures")
            || lower.contains("arch verification failed")
            || lower.contains("arch stripping")
        {
            FailureCategory::ArchUnsupported
        } else if lower.contains("morphe-cli exited with")
            || lower.contains("patching aborted")
            || lower.contains("fingerprint")
            || lower.contains("patcher targeted")
        {
            FailureCategory::PatcherError
        } else {
            FailureCategory::Other
        }
    }

    pub fn suggested_action(&self) -> &'static str {
        match self {
            FailureCategory::NoEnabledPatches => {
                "patch metadata has no enabled patches. Check patch bundle or app config"
            }
            FailureCategory::AllSourcesExhausted => {
                "all download sources exhausted. Check pin or upstream mirror availability"
            }
            FailureCategory::VersionNotSupported => {
                "target version not supported by enabled patches. Check patch compatibility list or update pin"
            }
            FailureCategory::IntegrityCheckFailed => {
                "downloaded file failed integrity check or cert SHA-256 mismatch. Verify upstream integrity"
            }
            FailureCategory::ArchUnsupported => {
                "APK does not contain native libraries for target architecture"
            }
            FailureCategory::VersionConflict => {
                "conflicting version constraints across enabled patches. Check patch bundle"
            }
            FailureCategory::CompatibilityUnknown => {
                "patch compatibility unknown for latest version. Pinned version recommended"
            }
            FailureCategory::PatcherError => {
                "morphe-cli patching failed. Check CLI logs and patch compatibility"
            }
            FailureCategory::Other => "review error message and upstream sources",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct SourceAttemptRecord {
    pub source: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub reason: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breaker_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breaker_state_at_attempt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub breaker_final_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_kind: Option<crate::sources::breaker::BlockKind>,
}

impl SourceAttemptRecord {
    pub fn new(
        source: impl Into<String>,
        outcome: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            source: source.into(),
            outcome: outcome.into(),
            version: None,
            url: None,
            reason: reason.into(),
            duration_ms: 0,
            breaker_state: None,
            breaker_state_at_attempt: None,
            breaker_final_state: None,
            block_kind: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct IntegrityStatus {
    pub package_ok: bool,
    pub version_ok: bool,
    pub cert_ok: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct AppBuildReport {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub version_candidates_tried: Vec<String>,
    #[serde(default)]
    pub version_downgraded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patch_supported_versions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_conflict: Option<Vec<(String, Vec<String>)>>,
    #[serde(default)]
    pub patches_included: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patches_auto_disabled: Vec<crate::patcher::plan::DisabledPatch>,
    #[serde(default)]
    pub patches_auto_disabled_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub critical_patches_disabled: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_category: Option<FailureCategory>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attempted_sources: Vec<SourceAttemptRecord>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub arch_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub root_payload_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub module_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub runtime_test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub optimization_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_archs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub validation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity: Option<IntegrityStatus>,
    #[serde(default)]
    pub total_duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
}

impl AppBuildReport {
    pub fn failed(err: impl Into<String>) -> Self {
        let err_str = err.into();
        let cat = FailureCategory::from_error_message(&err_str);
        let action = cat.suggested_action().to_string();
        Self {
            status: "failed".to_string(),
            requested_version: None,
            version_candidates_tried: vec![],
            version_downgraded: false,
            target_version: None,
            target_version_reason: None,
            patch_supported_versions: vec![],
            fetched_version: None,
            resolved_version: None,
            resolved_source: None,
            resolution_strategy: None,
            version_conflict: None,
            patches_included: vec![],
            patches_auto_disabled: vec![],
            patches_auto_disabled_count: 0,
            critical_patches_disabled: vec![],
            warnings: vec![],
            error: Some(err_str),
            failure_category: Some(cat),
            attempted_sources: vec![],
            arch_mode: None,
            output_file: None,
            output_path: None,
            root_payload_path: None,
            module_file: None,
            runtime_test: None,
            sha256: None,
            size_bytes: None,
            optimization_mode: None,
            supported_archs: vec![],
            validation_status: None,
            cert_sha256: None,
            integrity: None,
            total_duration_ms: 0,
            suggested_action: Some(action),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct BuildReportSummary {
    pub total: usize,
    pub success: usize,
    #[serde(default)]
    pub degraded: usize,
    pub failed: usize,
    pub skipped: usize,
    #[serde(default)]
    pub warnings_total: usize,
    pub failures_by_category: BTreeMap<String, usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_breakers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_unconfigured: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct BuildReport {
    pub schema_version: String,
    pub build_timestamp: String,
    pub revancex_version: String,
    pub summary: BuildReportSummary,
    pub apps: BTreeMap<String, AppBuildReport>,
}

impl BuildReport {
    pub const CURRENT_SCHEMA_VERSION: &'static str = "2.1.0";

    pub fn from_summary(summary: &BuildSummary) -> Self {
        let total = summary.reports.len();
        let mut success = 0;
        let mut degraded = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut warnings_total = 0;
        let mut failures_by_category = BTreeMap::new();
        let mut apps = summary.reports.clone();

        for rep in apps.values_mut() {
            warnings_total += rep.warnings.len();

            // Redact URLs in attempted_sources
            for att in &mut rep.attempted_sources {
                if let Some(ref u) = att.url {
                    att.url = Some(redact_url(u));
                }
            }

            match rep.status.as_str() {
                "success" => success += 1,
                "degraded" => degraded += 1,
                "failed" => {
                    failed += 1;
                    let cat = rep
                        .failure_category
                        .clone()
                        .or_else(|| {
                            rep.error
                                .as_deref()
                                .map(FailureCategory::from_error_message)
                        })
                        .unwrap_or(FailureCategory::Other);

                    if rep.suggested_action.is_none() {
                        rep.suggested_action = Some(cat.suggested_action().to_string());
                    }

                    let cat_name = serde_json::to_value(&cat)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_else(|| "other".to_string());
                    *failures_by_category.entry(cat_name).or_insert(0) += 1;
                }
                "skipped" | "success_unpatched" => skipped += 1,
                _ => {}
            }
        }

        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION.to_string(),
            build_timestamp: chrono::Utc::now().to_rfc3339(),
            revancex_version: env!("CARGO_PKG_VERSION").to_string(),
            summary: BuildReportSummary {
                total,
                success,
                degraded,
                failed,
                skipped,
                warnings_total,
                failures_by_category,
                open_breakers: summary.open_breakers.clone(),
                skipped_unconfigured: summary.skipped_unconfigured.clone(),
            },
            apps,
        }
    }
}

pub fn redact_url(url: &str) -> String {
    if let Ok(mut parsed) = reqwest::Url::parse(url) {
        let mut has_secret = false;
        let mut retained_pairs = Vec::new();
        for (k, v) in parsed.query_pairs() {
            let k_lower = k.to_lowercase();
            if k_lower.contains("token")
                || k_lower == "k"
                || k_lower.contains("secret")
                || k_lower.contains("auth")
                || k_lower.contains("signature")
                || k_lower.contains("sig")
            {
                has_secret = true;
            } else {
                retained_pairs.push((k.to_string(), v.to_string()));
            }
        }
        if has_secret {
            parsed.set_query(None);
            if !retained_pairs.is_empty() {
                for (k, v) in retained_pairs {
                    parsed.query_pairs_mut().append_pair(&k, &v);
                }
            }
            return parsed.to_string();
        }
    }
    url.to_string()
}

pub fn generate_markdown_report(report: &BuildReport) -> String {
    let mut md = String::new();
    md.push_str("# Build Report\n\n");
    md.push_str(&format!("- **Timestamp**: {}\n", report.build_timestamp));
    md.push_str(&format!(
        "- **Schema Version**: {}\n",
        report.schema_version
    ));
    md.push_str(&format!("- **Total Apps**: {}\n", report.summary.total));
    md.push_str(&format!("- **Success**: {}\n", report.summary.success));
    md.push_str(&format!("- **Degraded**: {}\n", report.summary.degraded));
    md.push_str(&format!("- **Failed**: {}\n", report.summary.failed));
    md.push_str(&format!("- **Skipped**: {}\n", report.summary.skipped));
    md.push_str(&format!(
        "- **Total Warnings**: {}\n\n",
        report.summary.warnings_total
    ));

    md.push_str("| App | Status | Requested | Resolved | Source | Warnings | Duration |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");

    for (app_id, rep) in &report.apps {
        let req = rep.requested_version.as_deref().unwrap_or("-");
        let res = rep.resolved_version.as_deref().unwrap_or("-");
        let src = rep.resolved_source.as_deref().unwrap_or("-");
        let dur = format!("{:.1}s", (rep.total_duration_ms as f64) / 1000.0);
        let warn_cnt = rep.warnings.len();
        md.push_str(&format!(
            "| `{app_id}` | `{}` | {req} | {res} | {src} | {warn_cnt} | {dur} |\n",
            rep.status
        ));
    }

    md
}
