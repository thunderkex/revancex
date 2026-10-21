use crate::orchestrator::scheduler::BuildSummary;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureCategory {
    AllSourcesExhausted,
    VersionNotSupported,
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
        if lower.contains("all download sources exhausted") {
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
            block_kind: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AppBuildReport {
    pub status: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_category: Option<FailureCategory>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attempted_sources: Vec<SourceAttemptRecord>,
}

impl AppBuildReport {
    pub fn failed(err: impl Into<String>) -> Self {
        let err_str = err.into();
        let cat = FailureCategory::from_error_message(&err_str);
        Self {
            status: "failed".to_string(),
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
            warnings: vec![],
            error: Some(err_str),
            failure_category: Some(cat),
            attempted_sources: vec![],
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct BuildReportSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
    pub failures_by_category: BTreeMap<String, usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_breakers: Vec<String>,
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
    pub const CURRENT_SCHEMA_VERSION: &'static str = "2.0.0";

    pub fn from_summary(summary: &BuildSummary) -> Self {
        let total = summary.reports.len();
        let mut success = 0;
        let mut failed = 0;
        let mut skipped = 0;
        let mut failures_by_category = BTreeMap::new();

        for rep in summary.reports.values() {
            match rep.status.as_str() {
                "success" => success += 1,
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
                failed,
                skipped,
                failures_by_category,
                open_breakers: summary.open_breakers.clone(),
            },
            apps: summary.reports.clone(),
        }
    }
}
