use crate::config::Config;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleaseAssetInfo {
    pub filename: String,
    pub url: String,
    pub tag: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleasesRegistry {
    #[serde(default)]
    pub stock_apps: HashMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub root_apps: HashMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub single_modules: HashMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub bundles: HashMap<String, ReleaseAssetInfo>,
    #[serde(default)]
    pub custom_bundles: HashMap<String, ReleaseAssetInfo>,
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

pub fn generate_apps_json(cfg: &Config) -> Result<String> {
    generate_apps_json_with_registry(cfg, Some("config/releases_registry.json"))
}

pub fn generate_apps_json_with_registry(
    cfg: &Config,
    registry_path: Option<&str>,
) -> Result<String> {
    let registry = registry_path.map(load_registry).unwrap_or_default();

    let mut entries: Vec<_> = cfg.apps.iter().collect();
    entries.sort_by_key(|(id, _)| *id);
    let apps: Vec<_> = entries
        .into_iter()
        .map(|(id, app)| {
            let stock_rel = registry.stock_apps.get(id);
            let root_rel = registry.root_apps.get(id);
            let mod_rel = registry.single_modules.get(id);

            let primary_rel = stock_rel.or(root_rel).or(mod_rel);
            let download_url = primary_rel.map(|r| r.url.as_str());
            let tag = primary_rel.map(|r| r.tag.as_str());
            let updated_at = primary_rel.map(|r| r.updated_at.as_str());

            json!({
                "id": id,
                "name": app.display_name.as_deref().unwrap_or(id),
                "enabled": app.enabled,
                "package": app.package,
                "patch_source": app.patch_source,
                "architectures": app.architectures,
                "mode": app.mode,
                "patches": app.patches,
                "dependencies": app.dependencies.all(),
                "module": {
                    "single": app.module.as_ref().map(|m| m.single).unwrap_or(true),
                    "bundle": app.module.as_ref().map(|m| m.bundle).unwrap_or(true),
                },
                "download_url": download_url,
                "tag": tag,
                "updated_at": updated_at,
                "releases": {
                    "stock": stock_rel,
                    "root": root_rel,
                    "module": mod_rel,
                }
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&apps)?)
}
