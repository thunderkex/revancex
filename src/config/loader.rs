use crate::config::apps::AppConfig;
use crate::config::patches::PatchSourceEntry;
use crate::config::sources::{ApkSourceEntry, CliSource};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TestingConfig {
    #[serde(default = "default_testing_enabled")]
    pub enabled: String,
    #[serde(default = "default_startup_grace")]
    pub startup_grace_secs: u64,
    #[serde(default = "default_install_timeout")]
    pub install_timeout_secs: u64,
    #[serde(default = "default_logcat_lines")]
    pub logcat_lines: u32,
    #[serde(default)]
    pub crash_patterns: Vec<String>,
}

fn default_testing_enabled() -> String {
    "auto".to_string()
}
fn default_startup_grace() -> u64 {
    8
}
fn default_install_timeout() -> u64 {
    180
}
fn default_logcat_lines() -> u32 {
    80
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BuildConfig {
    #[serde(default = "default_java")]
    pub java_min_version: u32,
    #[serde(default = "default_tools")]
    pub tools_dir: String,
    #[serde(default = "default_output")]
    pub output_dir: String,
    #[serde(default = "default_keys")]
    pub keys_dir: String,
    #[serde(default = "default_temp")]
    pub temp_dir: String,
    #[serde(default)]
    pub keystore: KeystoreConfig,
    #[serde(default = "default_workers")]
    pub workers: usize,
    #[serde(default = "default_retries")]
    pub download_retries: u32,
    #[serde(default = "default_timeout")]
    pub download_timeout_secs: u64,
    #[serde(default = "default_cache")]
    pub version_cache: String,
    #[serde(default)]
    pub bundle: BundleConfig,
    #[serde(default)]
    pub patcher: crate::patcher::plan::PatcherConfig,
    #[serde(default)]
    pub testing: TestingConfig,
}

fn default_java() -> u32 {
    21
}
fn default_tools() -> String {
    "./tools".to_string()
}
fn default_output() -> String {
    "./output".to_string()
}
fn default_keys() -> String {
    "./keys".to_string()
}
fn default_temp() -> String {
    "./tmp".to_string()
}
fn default_keystore_validity() -> u32 {
    10000
}
fn default_keystore_algorithm() -> String {
    "RSA".to_string()
}
fn default_keystore_size() -> u32 {
    4096
}
fn default_keystore_sig_alg() -> String {
    "SHA256withRSA".to_string()
}
fn default_keystore_dname() -> String {
    "CN=ReVanceX, OU=Ministry of Silly Builds, O=Thunderkex, L=CyberSpace, ST=Somewhere Over The Galaxy, C=XX".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KeystoreConfig {
    #[serde(default = "default_keystore_validity")]
    pub validity_days: u32,
    #[serde(default = "default_keystore_algorithm")]
    pub key_algorithm: String,
    #[serde(default = "default_keystore_size")]
    pub key_size: u32,
    #[serde(default = "default_keystore_sig_alg")]
    pub sig_algorithm: String,
    #[serde(default = "default_keystore_dname")]
    pub distinguished_name: String,
}

impl Default for KeystoreConfig {
    fn default() -> Self {
        Self {
            validity_days: default_keystore_validity(),
            key_algorithm: default_keystore_algorithm(),
            key_size: default_keystore_size(),
            sig_algorithm: default_keystore_sig_alg(),
            distinguished_name: default_keystore_dname(),
        }
    }
}
fn default_workers() -> usize {
    4
}
fn default_retries() -> u32 {
    3
}
fn default_timeout() -> u64 {
    180
}
fn default_cache() -> String {
    "./tmp/version_cache.json".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BundleConfig {
    #[serde(default = "default_max_bytes")]
    pub max_bytes: u64,
    #[serde(default = "default_true_bool")]
    pub include_stock_apks: bool,
}

fn default_max_bytes() -> u64 {
    1_800_000_000
}
fn default_true_bool() -> bool {
    true
}

impl Default for BundleConfig {
    fn default() -> Self {
        Self {
            max_bytes: default_max_bytes(),
            include_stock_apks: default_true_bool(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ModuleDefinition {
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_apps: Vec<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub module_id: Option<String>,
    #[serde(default)]
    pub zip_name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ModulesFile {
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub single_modules: Vec<String>,
}

fn default_module_id() -> String {
    "revancex".to_string()
}
fn default_module_name() -> String {
    "ReVanceX".to_string()
}
fn default_module_version() -> String {
    "1.0".to_string()
}
fn default_module_author() -> String {
    "Thunderkex".to_string()
}
fn default_module_description() -> String {
    "Patched apps bundle".to_string()
}
fn default_min_magisk() -> u32 {
    20400
}
fn default_min_kernelsu() -> u32 {
    10200
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ModulePropMeta {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, alias = "update_json")]
    pub update_json: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModuleMetaConfig {
    #[serde(default = "default_module_id")]
    pub id: String,
    #[serde(default = "default_module_name")]
    pub name: String,
    #[serde(default = "default_module_version")]
    pub version: String,
    #[serde(default = "default_module_author")]
    pub author: String,
    #[serde(default = "default_module_description")]
    pub description: String,
    #[serde(default, alias = "updateJson")]
    pub update_json: String,
    #[serde(default = "default_min_magisk")]
    pub min_magisk: u32,
    #[serde(default = "default_min_kernelsu")]
    pub min_kernelsu: u32,
    #[serde(default)]
    pub prop: Option<ModulePropMeta>,
}

impl Default for ModuleMetaConfig {
    fn default() -> Self {
        Self {
            id: default_module_id(),
            name: default_module_name(),
            version: default_module_version(),
            author: default_module_author(),
            description: default_module_description(),
            update_json: String::new(),
            min_magisk: default_min_magisk(),
            min_kernelsu: default_min_kernelsu(),
            prop: None,
        }
    }
}

impl ModuleMetaConfig {
    pub fn resolved_id(&self) -> &str {
        self.prop
            .as_ref()
            .and_then(|p| p.id.as_deref())
            .unwrap_or(&self.id)
    }

    pub fn resolved_name(&self) -> &str {
        self.prop
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or(&self.name)
    }

    pub fn resolved_author(&self) -> &str {
        self.prop
            .as_ref()
            .and_then(|p| p.author.as_deref())
            .unwrap_or(&self.author)
    }

    pub fn resolved_description(&self) -> &str {
        self.prop
            .as_ref()
            .and_then(|p| p.description.as_deref())
            .unwrap_or(&self.description)
    }

    pub fn resolved_update_json(&self) -> &str {
        self.prop
            .as_ref()
            .and_then(|p| p.update_json.as_deref())
            .unwrap_or(&self.update_json)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ModuleMetaFile {
    #[serde(default)]
    pub module: ModuleMetaConfig,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub apps: BTreeMap<String, AppConfig>,
    pub modules: BTreeMap<String, ModuleDefinition>,
    pub cli_repo: String,
    pub cli_pattern: String,
    pub patch_sources: BTreeMap<String, PatchSourceEntry>,
    pub apk_sources: BTreeMap<String, ApkSourceEntry>,
    pub build: BuildConfig,
    pub module_meta: ModuleMetaConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct AppsFile {
    apps: BTreeMap<String, AppConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SourcesFile {
    #[serde(default)]
    cli: Option<CliSource>,
    #[serde(default)]
    patch_sources: BTreeMap<String, PatchSourceEntry>,
    #[serde(default)]
    apk_sources: BTreeMap<String, ApkSourceEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BuildFile {
    build: BuildConfig,
}

pub fn sync_modules_file(config_dir: &str, cfg: &Config) -> Result<ModulesFile> {
    let path = format!("{config_dir}/modules.yaml");
    let mut modules_file: ModulesFile = if std::path::Path::new(&path).exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_yaml::from_str(&content).unwrap_or_default()
    } else {
        ModulesFile::default()
    };

    for module in modules_file.modules.values_mut() {
        let mut all_apps = Vec::new();
        for a in &module.apps {
            if !all_apps.contains(a) {
                all_apps.push(a.clone());
            }
        }
        for a in &module.disabled_apps {
            if !all_apps.contains(a) {
                all_apps.push(a.clone());
            }
        }

        let mut active = Vec::new();
        let mut disabled = Vec::new();
        for a in all_apps {
            if let Some(app_cfg) = cfg.apps.get(&a) {
                if app_cfg.enabled {
                    active.push(a);
                } else {
                    disabled.push(a);
                }
            }
        }
        module.apps = active;
        module.disabled_apps = disabled;
    }

    let mut single_mods: Vec<String> = cfg
        .apps
        .iter()
        .filter(|(id, a)| {
            a.enabled
                && !id.eq_ignore_ascii_case("microg")
                && a.module.as_ref().map_or(true, |m| m.single)
        })
        .map(|(id, _)| id.clone())
        .collect();
    single_mods.sort();
    modules_file.single_modules = single_mods;

    let yaml_str = serde_yaml::to_string(&modules_file)?;
    std::fs::write(&path, yaml_str)?;
    Ok(modules_file)
}

pub fn sync_and_reload_modules(config_dir: &str, mut cfg: Config) -> Result<Config> {
    let synced = sync_modules_file(config_dir, &cfg)?;
    cfg.modules = synced.modules;
    Ok(cfg)
}

pub fn load_config(config_dir: &str) -> Result<Config> {
    let read = |name: &str| -> Result<String> {
        let path = format!("{config_dir}/{name}");
        std::fs::read_to_string(&path).with_context(|| format!("Reading {path}"))
    };

    let apps: AppsFile = serde_yaml::from_str(&read("apps.yaml")?)?;
    let sources: SourcesFile = serde_yaml::from_str(&read("sources.yaml")?)?;
    let build_file: BuildFile = serde_yaml::from_str(&read("build.yaml")?)?;
    let modules: ModulesFile = match read("modules.yaml") {
        Ok(content) => serde_yaml::from_str(&content).unwrap_or_default(),
        Err(_) => ModulesFile::default(),
    };
    let module_meta_file: ModuleMetaFile = match read("module.yaml") {
        Ok(content) => serde_yaml::from_str(&content).unwrap_or_default(),
        Err(_) => ModuleMetaFile::default(),
    };

    let (cli_repo, cli_pattern) = match sources.cli {
        Some(c) => (c.repo, c.asset_pattern),
        None => (
            "MorpheApp/morphe-desktop".to_string(),
            "morphe-*-all.jar".to_string(),
        ),
    };

    Ok(Config {
        apps: apps.apps,
        modules: modules.modules,
        cli_repo,
        cli_pattern,
        patch_sources: sources.patch_sources,
        apk_sources: sources.apk_sources,
        build: build_file.build,
        module_meta: module_meta_file.module,
    })
}

pub fn validate_config(cfg: &Config, strict: bool) -> Result<()> {
    for (id, app) in &cfg.apps {
        for dep in app.dependencies.all() {
            if !cfg.apps.contains_key(&dep) {
                anyhow::bail!("App '{id}' has unknown dependency '{dep}'");
            }
        }
        if strict && app.enabled && app.package.is_empty() {
            anyhow::bail!("App '{id}' is enabled but has no package defined");
        }
    }

    if strict {
        for (module_name, module_def) in &cfg.modules {
            let mut pkg_map: HashMap<String, Vec<String>> = HashMap::new();
            for app_id in &module_def.apps {
                let app = cfg.apps.get(app_id).ok_or_else(|| {
                    anyhow::anyhow!("Module '{module_name}' references non-existent app '{app_id}'")
                })?;
                if !app.enabled {
                    anyhow::bail!("Module '{module_name}' references disabled app '{app_id}'");
                }
                pkg_map
                    .entry(app.package.clone())
                    .or_default()
                    .push(app_id.clone());
            }

            for (pkg, app_ids) in pkg_map {
                if app_ids.len() > 1 {
                    anyhow::bail!(
                        "Package collision in module '{module_name}': multiple apps {:?} share package '{pkg}'",
                        app_ids
                    );
                }
            }
        }
    }

    println!("Config valid. Total apps configured: {}", cfg.apps.len());
    Ok(())
}

pub fn check_yaml_duplicate_keys(content: &str) -> Result<()> {
    let mut stack: Vec<(usize, std::collections::HashSet<String>)> = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        if trimmed.starts_with('-') {
            while let Some((top_indent, _)) = stack.last() {
                if *top_indent > indent {
                    stack.pop();
                } else {
                    break;
                }
            }
            continue;
        }

        if let Some(colon_idx) = trimmed.find(':') {
            let key = trimmed[..colon_idx].trim();
            if key.is_empty() || key.starts_with('{') || key.starts_with('[') {
                continue;
            }

            while let Some((top_indent, _)) = stack.last() {
                if *top_indent > indent {
                    stack.pop();
                } else {
                    break;
                }
            }

            if let Some((top_indent, keys)) = stack.last_mut() {
                if *top_indent == indent {
                    if keys.contains(key) {
                        anyhow::bail!("Duplicate key '{key}' found at line {}", line_idx + 1);
                    }
                    keys.insert(key.to_string());
                    continue;
                }
            }

            let mut new_set = std::collections::HashSet::new();
            new_set.insert(key.to_string());
            stack.push((indent, new_set));
        }
    }

    Ok(())
}

pub fn validate_yaml_duplicates(config_dir: &str) -> Result<()> {
    let files = [
        "apps.yaml",
        "sources.yaml",
        "modules.yaml",
        "build.yaml",
        "module.yaml",
    ];
    for file in &files {
        let path = format!("{config_dir}/{file}");
        if std::path::Path::new(&path).exists() {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("Reading {path} for duplicate key validation"))?;
            check_yaml_duplicate_keys(&content)
                .with_context(|| format!("Duplicate key check failed in {path}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate_key_detection() {
        let yaml_valid = r#"
cli:
  repo: MorpheApp/morphe-desktop
patch_sources:
  anddea:
    repo: anddea/revanced-patches
  rushiranpise:
    repo: rushiranpise/morphe-patches
"#;
        assert!(check_yaml_duplicate_keys(yaml_valid).is_ok());

        let yaml_dup = r#"
patch_sources:
  rushiranpise:
    repo: a
  rushiranpise:
    repo: b
"#;
        let err = check_yaml_duplicate_keys(yaml_dup).unwrap_err();
        assert!(err.to_string().contains("Duplicate key 'rushiranpise'"));
    }
}
