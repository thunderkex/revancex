use crate::config::Config;
use anyhow::Result;
use std::collections::HashMap;
use tracing::info;

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct VersionCache {
    patch_source_tags: HashMap<String, String>,
}

fn load_version_cache(path: &str) -> VersionCache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_version_cache(path: &str, cache: &VersionCache) {
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(path, json);
    }
}

pub async fn check_updates(cfg: &Config, json_output: bool) -> Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("revancex/1.0")
        .build()?;

    let cache_path = &cfg.build.version_cache;
    let mut cache = load_version_cache(cache_path);
    let mut changed_sources = Vec::new();
    let mut changed_apps = Vec::new();

    for (name, source) in &cfg.patch_sources {
        let app_branch = cfg
            .apps
            .values()
            .find(|a| &a.patch_source == name)
            .and_then(|a| a.patch_branch.as_deref());
        let branch = app_branch.or(source.branch.as_deref());

        match crate::utils::github::get_latest_release(&client, &source.repo, branch).await {
            Ok(rel) => {
                let prev_tag = cache
                    .patch_source_tags
                    .get(name)
                    .cloned()
                    .unwrap_or_default();
                info!(
                    "Patch source {name}: current={}, previous={prev_tag}",
                    rel.tag_name
                );
                if rel.tag_name != prev_tag {
                    changed_sources.push(name.clone());
                    cache
                        .patch_source_tags
                        .insert(name.clone(), rel.tag_name.clone());
                    for (app_name, app) in &cfg.apps {
                        if app.enabled
                            && &app.patch_source == name
                            && !changed_apps.contains(app_name)
                        {
                            changed_apps.push(app_name.clone());
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Could not check updates for source {name}: {e}");
            }
        }
    }

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
