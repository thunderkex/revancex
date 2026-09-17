use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug)]
pub struct VersionCache {
    #[serde(default)]
    pub patch_source_tags: HashMap<String, String>,
}

pub fn load_version_cache(path: &str) -> VersionCache {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_version_cache(path: &str, cache: &VersionCache) {
    if let Some(parent) = Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(path, json);
    }
}

pub fn clean_cache(cfg: &crate::config::Config, all: bool) -> Result<()> {
    info!("Cleaning cache (all={all})...");
    let tmp = &cfg.build.temp_dir;
    if Path::new(tmp).exists() {
        std::fs::remove_dir_all(tmp)?;
        std::fs::create_dir_all(tmp)?;
    }
    if all {
        let tools = &cfg.build.tools_dir;
        if Path::new(tools).exists() {
            std::fs::remove_dir_all(tools)?;
            std::fs::create_dir_all(tools)?;
        }
    }
    println!("Cache cleaned successfully.");
    Ok(())
}
