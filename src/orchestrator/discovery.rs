use crate::config::Config;
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

    let (changed_sources, changed_apps, new_cache_tags) =
        detect_changed_apps_from_tags(&pairs, &cache.patch_source_tags, &latest_tags);

    cache.patch_source_tags = new_cache_tags;
    save_version_cache(cache_path, &cache);

    let has_updates = !changed_apps.is_empty();

    if json_output {
        let out = serde_json::json!({
            "has_updates": has_updates,
            "changed_apps": changed_apps,
            "changed_sources": changed_sources
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        if has_updates {
            println!(
                "Updates found: {} sources changed, {} apps affected.",
                changed_sources.len(),
                changed_apps.len()
            );
        } else {
            println!("No updates detected.");
        }
    }

    Ok(())
}
