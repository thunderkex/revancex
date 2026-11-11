use crate::config::apps::AppConfig;
use crate::patcher::metadata::PatchMeta;
use crate::patcher::name::{matches, MatchRule};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatcherConfig {
    #[serde(default = "default_max_retries")]
    pub max_patch_retries: u32,
    #[serde(default = "default_auto_disable")]
    pub auto_disable: String, // "off" | "incompatible" | "aggressive"
    #[serde(default)]
    pub rules: PatcherRules,
    #[serde(default = "default_failure_patterns")]
    pub failure_patterns: Vec<String>,
}

fn default_max_retries() -> u32 {
    2
}
fn default_auto_disable() -> String {
    "incompatible".to_string()
}
fn default_failure_patterns() -> Vec<String> {
    vec![
        r"Failed to apply patch\s+([A-Za-z0-9_-]+)".to_string(),
        r"([A-Za-z0-9_-]+):\s*Exception".to_string(),
        r"Exception in patch\s+([A-Za-z0-9_-]+)".to_string(),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatcherRules {
    #[serde(default)]
    pub root: RuleSet,
    #[serde(default)]
    pub nonroot: RuleSet,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleSet {
    #[serde(default)]
    pub exclude: Vec<MatchRule>,
    #[serde(default)]
    pub include_if_dependency: std::collections::HashMap<String, Vec<MatchRule>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DisableReason {
    Incompatible {
        supported: Vec<String>,
        actual: String,
    },
    RuleExcluded {
        rule: String,
    },
    FailedAtRuntime {
        round: u32,
        message: String,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisabledPatch {
    pub name: String,
    pub reason: DisableReason,
    #[serde(default)]
    pub critical: bool,
}

pub fn is_critical_patch(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("gmscore")
        || lower.contains("microg")
        || lower.contains("signature")
        || lower.contains("spoof-signature")
        || lower.contains("spoof signature")
        || lower.contains("spoof app signature")
        || lower.contains("firebase")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchPlan {
    pub included: Vec<String>,
    pub excluded: Vec<String>,
    pub auto_disabled: Vec<DisabledPatch>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub critical_disabled: Vec<String>,
}

fn canonicalize_patch_name(patch: &str, patch_meta: Option<&Arc<Vec<PatchMeta>>>) -> String {
    let Some(patches) = patch_meta else {
        return patch.to_string();
    };
    patches
        .iter()
        .find(|p| {
            p.name.eq_ignore_ascii_case(patch)
                || p.name.replace(' ', "-").eq_ignore_ascii_case(patch)
                || p.name.replace(['-', '_'], " ").eq_ignore_ascii_case(patch)
        })
        .map(|p| p.name.clone())
        .unwrap_or_else(|| patch.to_string())
}

fn push_unique_exclude(plan: &mut PatchPlan, name: &str) {
    if !plan.excluded.iter().any(|e| e.eq_ignore_ascii_case(name)) {
        plan.excluded.push(name.to_string());
    }
}

fn apply_dependency_includes(
    app: &AppConfig,
    is_root: bool,
    patch_meta: Option<&Arc<Vec<PatchMeta>>>,
    mode_rules: &RuleSet,
    plan: &mut PatchPlan,
) {
    let deps = app.dependencies.for_mode(is_root);
    let Some(patches) = patch_meta else {
        return;
    };
    for (dep_name, inc_rules) in &mode_rules.include_if_dependency {
        if !deps.iter().any(|d| d == dep_name) {
            continue;
        }
        for patch in patches.iter() {
            let matched = inc_rules.iter().any(|r| matches(r, &patch.name));
            if matched
                && !plan
                    .included
                    .iter()
                    .any(|i| i.eq_ignore_ascii_case(&patch.name))
            {
                plan.included.push(patch.name.clone());
            }
        }
    }
}

fn apply_mode_and_root_excludes(
    is_root: bool,
    patch_meta: Option<&Arc<Vec<PatchMeta>>>,
    mode_rules: &RuleSet,
    plan: &mut PatchPlan,
) {
    for rule in &mode_rules.exclude {
        if let MatchRule::Equals { equals: name } | MatchRule::Contains { contains: name } = rule {
            push_unique_exclude(plan, name);
        }
    }
    if let Some(patches) = patch_meta {
        for patch in patches.iter() {
            if mode_rules.exclude.iter().any(|r| matches(r, &patch.name)) {
                push_unique_exclude(plan, &patch.name);
                plan.included
                    .retain(|i| !i.eq_ignore_ascii_case(&patch.name));
            }
        }
    }
    if is_root {
        const ROOT_MUST_EXCLUDE: [&str; 9] = [
            "GmsCore support",
            "Change package name",
            "MicroG support",
            "Change app icon",
            "Custom branding",
            "Custom branding icon for YouTube",
            "Custom branding name for YouTube",
            "Custom branding icon for YouTube Music",
            "Custom branding name for YouTube Music",
        ];
        for p in ROOT_MUST_EXCLUDE {
            push_unique_exclude(plan, p);
            plan.included.retain(|i| !i.eq_ignore_ascii_case(p));
        }
    }
}

fn auto_disable_incompatible(
    app: &AppConfig,
    patches: &[PatchMeta],
    apk_ver: &str,
    plan: &mut PatchPlan,
) {
    for patch in patches {
        let is_included = plan
            .included
            .iter()
            .any(|i| i.eq_ignore_ascii_case(&patch.name));
        if !is_included && !plan.included.is_empty() {
            continue;
        }
        for compat in &patch.compatible_packages {
            if compat.name != app.package
                || compat.versions.is_empty()
                || compat
                    .versions
                    .iter()
                    .any(|v| crate::utils::version::version_eq(v, apk_ver))
            {
                continue;
            }
            let critical = is_critical_patch(&patch.name);
            if critical
                && !plan
                    .critical_disabled
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(&patch.name))
            {
                plan.critical_disabled.push(patch.name.clone());
            }
            plan.auto_disabled.push(DisabledPatch {
                name: patch.name.clone(),
                reason: DisableReason::Incompatible {
                    supported: compat.versions.clone(),
                    actual: apk_ver.to_string(),
                },
                critical,
            });
            push_unique_exclude(plan, &patch.name);
            plan.included
                .retain(|i| !i.eq_ignore_ascii_case(&patch.name));
        }
    }
}

pub fn resolve(
    app: &AppConfig,
    is_root: bool,
    patch_meta: Option<&Arc<Vec<PatchMeta>>>,
    apk_version: Option<&str>,
    rules: &PatcherRules,
    auto_disable: &str,
) -> PatchPlan {
    let mut plan = PatchPlan {
        excluded: app.excluded_patches.clone(),
        ..Default::default()
    };
    let mode_rules = if is_root { &rules.root } else { &rules.nonroot };

    for patch in app.included_patches.iter().chain(app.patches.iter()) {
        let canonical = canonicalize_patch_name(patch, patch_meta);
        let matched_rule = mode_rules
            .exclude
            .iter()
            .find(|rule| matches(rule, &canonical))
            .map(|rule| format!("{rule:?}"));
        if let Some(rule_desc) = matched_rule {
            let critical = is_critical_patch(&canonical);
            plan.auto_disabled.push(DisabledPatch {
                name: canonical,
                reason: DisableReason::RuleExcluded { rule: rule_desc },
                critical,
            });
        } else {
            plan.included.push(canonical);
        }
    }

    apply_dependency_includes(app, is_root, patch_meta, mode_rules, &mut plan);
    apply_mode_and_root_excludes(is_root, patch_meta, mode_rules, &mut plan);

    if !auto_disable.eq_ignore_ascii_case("off") && !app.version_pin_strict {
        if let (Some(patches), Some(apk_ver)) = (patch_meta, apk_version) {
            auto_disable_incompatible(app, patches, apk_ver, &mut plan);
        }
    }

    plan
}
