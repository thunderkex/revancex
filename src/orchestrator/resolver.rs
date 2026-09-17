use crate::config::Config;
use anyhow::Result;
use std::collections::HashSet;

pub fn resolve_targets(cfg: &Config, apps_input: &str) -> Result<Vec<String>> {
    let raw_ids: Vec<String> = if apps_input == "all" || apps_input == "all-enabled" {
        cfg.apps
            .iter()
            .filter(|(_, a)| a.enabled)
            .map(|(id, _)| id.clone())
            .collect()
    } else {
        apps_input
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    };

    let mut resolved = HashSet::new();
    for id in &raw_ids {
        resolved.insert(id.clone());
        if let Some(app) = cfg.apps.get(id) {
            let is_root = app.mode == "root" || app.mode == "module";
            for dep in app.dependencies.for_mode(is_root) {
                resolved.insert(dep.clone());
            }
        }
    }

    Ok(resolved.into_iter().collect())
}
