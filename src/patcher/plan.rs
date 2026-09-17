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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisabledPatch {
    pub name: String,
    pub reason: DisableReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchPlan {
    pub included: Vec<String>,
    pub excluded: Vec<String>,
    pub auto_disabled: Vec<DisabledPatch>,
    pub warnings: Vec<String>,
}

pub fn resolve(
    app: &AppConfig,
    is_root: bool,
    patch_meta: Option<&Arc<Vec<PatchMeta>>>,
    apk_version: Option<&str>,
    rules: &PatcherRules,
) -> PatchPlan {
    let mut plan = PatchPlan::default();

    let mut raw_included = app.included_patches.clone();
    raw_included.extend(app.patches.clone());

    let mode_rules = if is_root { &rules.root } else { &rules.nonroot };

    for patch in &raw_included {
        let mut matched_rule = None;
        for rule in &mode_rules.exclude {
            if matches(rule, patch) {
                matched_rule = Some(format!("{:?}", rule));
                break;
            }
        }
        if let Some(rule_desc) = matched_rule {
            plan.auto_disabled.push(DisabledPatch {
                name: patch.clone(),
                reason: DisableReason::RuleExcluded { rule: rule_desc },
            });
        } else {
            plan.included.push(patch.clone());
        }
    }

    for (dep_name, inc_rules) in &mode_rules.include_if_dependency {
        if app.dependencies.contains(dep_name) {
            if let Some(patches) = patch_meta {
                for patch in patches.iter() {
                    for rule in inc_rules {
                        if matches(rule, &patch.name)
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
        }
    }

    plan.excluded = app.excluded_patches.clone();
    for rule in &mode_rules.exclude {
        match rule {
            MatchRule::Equals { equals: name } | MatchRule::Contains { contains: name } => {
                if !plan.excluded.iter().any(|e| e.eq_ignore_ascii_case(name)) {
                    plan.excluded.push(name.clone());
                }
            }
            MatchRule::Regex { .. } => {}
        }
    }

    if let (Some(patches), Some(apk_ver)) = (patch_meta, apk_version) {
        for patch in patches.iter() {
            let is_included = plan
                .included
                .iter()
                .any(|i| i.eq_ignore_ascii_case(&patch.name));
            if !is_included && !plan.included.is_empty() {
                continue;
            }
            for compat in &patch.compatible_packages {
                if compat.name == app.package
                    && !compat.versions.is_empty()
                    && !compat.versions.iter().any(|v| v == apk_ver)
                {
                    plan.auto_disabled.push(DisabledPatch {
                        name: patch.name.clone(),
                        reason: DisableReason::Incompatible {
                            supported: compat.versions.clone(),
                            actual: apk_ver.to_string(),
                        },
                    });
                    if !plan
                        .excluded
                        .iter()
                        .any(|e| e.eq_ignore_ascii_case(&patch.name))
                    {
                        plan.excluded.push(patch.name.clone());
                    }
                    plan.included
                        .retain(|i| !i.eq_ignore_ascii_case(&patch.name));
                }
            }
        }
    }

    plan
}
