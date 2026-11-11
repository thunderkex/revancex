use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

pub const NEGATIVE_CACHE_TTL_SECS: i64 = 8 * 3600; // 8 hours TTL

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug)]
pub struct VersionCache {
    #[serde(default)]
    pub patch_source_tags: HashMap<String, String>,
    #[serde(default)]
    pub app_versions: HashMap<String, String>,
    #[serde(default)]
    pub negative_cache: HashMap<String, i64>,
}

pub fn is_negative_cached(
    cache: &VersionCache,
    source: &str,
    package: &str,
    version: Option<&str>,
) -> bool {
    let key = format!("{}:{}:{}", source, package, version.unwrap_or("latest"));
    if let Some(&ts) = cache.negative_cache.get(&key) {
        let now = chrono::Utc::now().timestamp();
        if now - ts < NEGATIVE_CACHE_TTL_SECS {
            return true;
        }
    }
    false
}

pub fn record_negative_cache(
    cache: &mut VersionCache,
    source: &str,
    package: &str,
    version: Option<&str>,
) {
    let key = format!("{}:{}:{}", source, package, version.unwrap_or("latest"));
    let now = chrono::Utc::now().timestamp();
    cache.negative_cache.insert(key, now);
}

static CACHE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn is_negative_cached_file(
    path: &str,
    source: &str,
    package: &str,
    version: Option<&str>,
) -> bool {
    let _guard = crate::utils::lock_unpoisoned(&CACHE_LOCK);
    let cache = load_version_cache(path);
    is_negative_cached(&cache, source, package, version)
}

pub fn record_negative_cache_file(path: &str, source: &str, package: &str, version: Option<&str>) {
    let _guard = crate::utils::lock_unpoisoned(&CACHE_LOCK);
    let mut cache = load_version_cache(path);
    record_negative_cache(&mut cache, source, package, version);
    save_version_cache(path, &cache);
}

pub fn prune_stale_cache_keys(cache: &mut VersionCache, cfg: &crate::config::Config) {
    let mut pruned = Vec::new();
    cache.patch_source_tags.retain(|key, _| {
        if key == "cli" {
            return true;
        }
        let alias = key.split('@').next().unwrap_or(key);
        let valid = cfg.patch_sources.contains_key(alias);
        if !valid {
            pruned.push(key.clone());
        }
        valid
    });
    for k in pruned {
        tracing::info!("Pruned stale cache key from patch_source_tags: {k}");
    }

    let now = chrono::Utc::now().timestamp();
    cache
        .negative_cache
        .retain(|_, &mut ts| now - ts < NEGATIVE_CACHE_TTL_SECS);
}

pub fn load_version_cache(path: impl AsRef<Path>) -> VersionCache {
    load_version_cache_with_config(path, None)
}

pub fn load_version_cache_with_config(
    path: impl AsRef<Path>,
    cfg: Option<&crate::config::Config>,
) -> VersionCache {
    let mut cache: VersionCache = std::fs::read_to_string(path.as_ref())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    if let Some(c) = cfg {
        prune_stale_cache_keys(&mut cache, c);
    } else {
        let now = chrono::Utc::now().timestamp();
        cache
            .negative_cache
            .retain(|_, &mut ts| now - ts < NEGATIVE_CACHE_TTL_SECS);
    }

    cache
}

pub fn save_version_cache(path: impl AsRef<Path>, cache: &VersionCache) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        crate::utils::warn_io(
            std::fs::create_dir_all(parent),
            &format!("Creating cache parent dir {}", parent.display()),
        );
    }
    if let Ok(json) = serde_json::to_string_pretty(cache) {
        crate::utils::warn_io(
            std::fs::write(path, json),
            &format!("Writing version cache to {}", path.display()),
        );
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
    crate::ui::line("Cache cleaned successfully.");
    Ok(())
}
