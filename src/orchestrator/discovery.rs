use crate::config::Config;
use crate::patcher::metadata;
use crate::utils::cache::{load_version_cache, save_version_cache};
use anyhow::Result;
use std::collections::HashMap;
use tracing::info;

pub fn resolve_app_patch_pairs(
    cfg: &Config,
) -> HashMap<(String, String, Option<String>), Vec<String>> {
    let mut pairs: HashMap<(String, String, Option<String>), Vec<String>> = HashMap::new();

    for (app_name, app) in &cfg.apps {
        if !app.enabled || app.patch_source.is_empty() || app.patch_source == "none" {
            continue;
        }
        let (repo, source_branch) = cfg
            .patch_sources
            .get(&app.patch_source)
            .map(|p| (p.repo.clone(), p.branch.clone()))
            .unwrap_or_else(|| (app.patch_source.clone(), None));

        let effective_branch = app
            .patches_version
            .clone()
            .or(app.patch_branch.clone())
            .or(source_branch);

        pairs
            .entry((app.patch_source.clone(), repo, effective_branch))
            .or_default()
            .push(app_name.clone());
    }

    pairs
}

pub fn detect_changed_apps_from_tags(
    pairs: &HashMap<(String, String, Option<String>), Vec<String>>,
    cached_tags: &HashMap<String, String>,
    latest_tags: &HashMap<(String, Option<String>), String>,
) -> (Vec<String>, Vec<String>, HashMap<String, String>) {
    let mut changed_sources = Vec::new();
    let mut changed_apps = Vec::new();
    let mut updated_cache = cached_tags.clone();

    for ((source_name, _repo, branch), apps) in pairs {
        let cache_key = match branch {
            Some(b) => format!("{source_name}@{b}"),
            None => source_name.clone(),
        };
        let prev_tag = cached_tags
            .get(&cache_key)
            .or_else(|| cached_tags.get(source_name))
            .cloned()
            .unwrap_or_default();

        if let Some(latest_tag) = latest_tags.get(&(source_name.clone(), branch.clone())) {
            let branch_desc = branch.as_deref().unwrap_or("default");
            info!(
                "Patch source {source_name} ({branch_desc}): current={latest_tag}, previous={prev_tag}"
            );
            if latest_tag != &prev_tag {
                if !changed_sources.contains(source_name) {
                    changed_sources.push(source_name.clone());
                }
                updated_cache.insert(cache_key, latest_tag.clone());
                for app in apps {
                    if !changed_apps.contains(app) {
                        changed_apps.push(app.clone());
                    }
                }
            }
        }
    }

    changed_apps.sort();
    changed_sources.sort();
    (changed_sources, changed_apps, updated_cache)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SkipReason {
    NoMppFile(String),
    MetadataLoadFailed(String),
    PackageNotInBundle,
    VersionPinned,
    VersionConflict(Vec<(String, Vec<String>)>),
    Unconstrained,
}

pub async fn check_updates(cfg: &Config, json_output: bool) -> Result<()> {
    let client = crate::utils::make_github_client(cfg)?;

    let cache_path = &cfg.build.version_cache;
    let mut cache = load_version_cache(cache_path);

    let pairs = resolve_app_patch_pairs(cfg);
    let mut latest_tags = HashMap::new();

    for (source_name, repo, branch) in pairs.keys() {
        match crate::utils::github::get_latest_release(&client, repo, branch.as_deref()).await {
            Ok(rel) => {
                latest_tags.insert((source_name.clone(), branch.clone()), rel.tag_name);
            }
            Err(e) => {
                let branch_str = branch.as_deref().unwrap_or("default");
                tracing::warn!("Could not check updates for {source_name} ({branch_str}): {e}");
            }
        }
    }

    let (changed_sources, mut changed_apps, new_cache_tags) =
        detect_changed_apps_from_tags(&pairs, &cache.patch_source_tags, &latest_tags);

    cache.patch_source_tags = new_cache_tags;

    let mut app_version_changed: Vec<String> = Vec::new();
    let mut version_conflicts: Vec<serde_json::Value> = Vec::new();
    let mut unresolved_apps: HashMap<String, SkipReason> = HashMap::new();

    for (app_name, app) in &cfg.apps {
        if !app.enabled || app.patch_source.is_empty() || app.patch_source == "none" {
            continue;
        }
        if app
            .version
            .as_deref()
            .map(|v| !v.eq_ignore_ascii_case("auto"))
            .unwrap_or(false)
        {
            unresolved_apps.insert(app_name.clone(), SkipReason::VersionPinned);
            continue;
        }
        let mpp_path = match metadata::resolve_mpp_path(cfg, app) {
            Some(p) => p,
            None => {
                let target = app
                    .patches_version
                    .as_deref()
                    .or(app.patch_branch.as_deref());
                let repo = cfg
                    .patch_sources
                    .get(&app.patch_source)
                    .map(|p| p.repo.clone())
                    .unwrap_or_else(|| app.patch_source.clone());
                let safe_name = repo.replace('/', "_");
                let expected_path = target
                    .map(|t| format!("{}/patches_{safe_name}_{t}.mpp", cfg.build.tools_dir))
                    .unwrap_or_else(|| format!("{}/patches_{safe_name}.mpp", cfg.build.tools_dir));
                unresolved_apps.insert(app_name.clone(), SkipReason::NoMppFile(expected_path));
                continue;
            }
        };
        let patches = match metadata::load(cfg, &mpp_path).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("{app_name}: could not load patch metadata for version check: {e}");
                unresolved_apps.insert(
                    app_name.clone(),
                    SkipReason::MetadataLoadFailed(e.to_string()),
                );
                continue;
            }
        };

        let pkg_in_bundle = patches
            .iter()
            .any(|p| p.compatible_packages.iter().any(|c| c.name == app.package));
        if !pkg_in_bundle {
            unresolved_apps.insert(app_name.clone(), SkipReason::PackageNotInBundle);
            continue;
        }

        let enabled_patches = if !app.included_patches.is_empty() {
            &app.included_patches
        } else {
            &app.patches
        };

        let supported = metadata::resolve_compatible_versions(
            &patches,
            &app.package,
            enabled_patches,
            &app.excluded_patches,
        );
        let latest_max = match supported {
            metadata::CompatibleVersions::Intersection(ref vs) => {
                crate::utils::semver::max_version(vs)
            }
            metadata::CompatibleVersions::Conflict { ref patch_versions } => {
                unresolved_apps.insert(
                    app_name.clone(),
                    SkipReason::VersionConflict(patch_versions.clone()),
                );
                version_conflicts.push(serde_json::json!({
                    "app": app_name,
                    "conflicts": patch_versions
                }));
                let smallest = patch_versions
                    .iter()
                    .filter(|(_, v)| !v.is_empty())
                    .min_by_key(|(_, v)| v.len())
                    .map(|(_, v)| v.as_slice())
                    .unwrap_or(&[]);
                crate::utils::semver::max_version(smallest)
            }
            metadata::CompatibleVersions::Unconstrained | metadata::CompatibleVersions::Unknown => {
                unresolved_apps.insert(app_name.clone(), SkipReason::Unconstrained);
                None
            }
        };

        let cached_max = cache.app_versions.get(app_name).cloned();
        let latest_str = latest_max.as_deref().unwrap_or("");
        let cached_str = cached_max.as_deref().unwrap_or("");
        if !latest_str.is_empty() && latest_str != cached_str {
            info!("{app_name}: supported app version changed {cached_str:?} → {latest_str:?}");
            if let Some(v) = latest_max {
                cache.app_versions.insert(app_name.clone(), v);
            }
            if !app_version_changed.contains(app_name) {
                app_version_changed.push(app_name.clone());
            }
            if !changed_apps.contains(app_name) {
                changed_apps.push(app_name.clone());
            }
        }
    }
    changed_apps.sort();
    app_version_changed.sort();

    save_version_cache(cache_path, &cache);

    let has_updates = !changed_apps.is_empty();

    if json_output {
        let out = serde_json::json!({
            "has_updates": has_updates,
            "changed_apps": changed_apps,
            "changed_sources": changed_sources,
            "app_version_changed": app_version_changed,
            "version_conflicts": version_conflicts,
            "unresolved_apps": unresolved_apps
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        if has_updates {
            println!(
                "Updates found: {} sources changed, {} apps affected ({} due to app version change).",
                changed_sources.len(),
                changed_apps.len(),
                app_version_changed.len()
            );
        } else {
            println!("No updates detected.");
        }
    }

    Ok(())
}
