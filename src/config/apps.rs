use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HookWhen {
    Root,
    Nonroot,
    #[default]
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostPatchHook {
    pub script: String,
    #[serde(default)]
    pub when: HookWhen,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AppDependencies {
    Flat(Vec<String>),
    ModeAware {
        #[serde(default)]
        non_root: Vec<String>,
        #[serde(default)]
        root: Vec<String>,
    },
}

impl Default for AppDependencies {
    fn default() -> Self {
        AppDependencies::Flat(Vec::new())
    }
}

impl AppDependencies {
    pub fn contains(&self, dep: &str) -> bool {
        match self {
            AppDependencies::Flat(list) => list.iter().any(|d| d == dep),
            AppDependencies::ModeAware { non_root, root } => {
                non_root.iter().any(|d| d == dep) || root.iter().any(|d| d == dep)
            }
        }
    }

    pub fn for_mode(&self, is_root: bool) -> &[String] {
        match self {
            AppDependencies::Flat(list) => list.as_slice(),
            AppDependencies::ModeAware { non_root, root } => {
                if is_root {
                    root.as_slice()
                } else {
                    non_root.as_slice()
                }
            }
        }
    }

    pub fn all(&self) -> Vec<String> {
        match self {
            AppDependencies::Flat(list) => list.clone(),
            AppDependencies::ModeAware { non_root, root } => {
                let mut set = std::collections::HashSet::new();
                for d in non_root.iter().chain(root.iter()) {
                    set.insert(d.clone());
                }
                set.into_iter().collect()
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub enabled: bool,
    #[serde(default)]
    pub package: String,
    #[serde(alias = "patches-source", default)]
    pub patch_source: String,
    #[serde(
        alias = "patch-branch",
        alias = "patch_branch",
        alias = "branch",
        default
    )]
    pub patch_branch: Option<String>,
    #[serde(alias = "cli-source", default = "default_cli")]
    pub cli_source: String,
    #[serde(alias = "uptodown-dlurl", default)]
    pub uptodown_url: Option<String>,
    #[serde(alias = "apkmirror-dlurl", default)]
    pub apkmirror_url: Option<String>,
    #[serde(alias = "archive-dlurl", default)]
    pub archive_url: Option<String>,
    #[serde(alias = "apkpure-dlurl", default)]
    pub apkpure_url: Option<String>,
    #[serde(default)]
    pub source_priority: Vec<String>,
    #[serde(alias = "apk_source", default)]
    pub apk_source: String,
    #[serde(alias = "apk_url", default)]
    pub apk_url: Option<String>,
    #[serde(alias = "arch", default = "default_arch")]
    pub architectures: Vec<String>,
    #[serde(alias = "build-mode", default = "default_mode")]
    pub mode: String,
    #[serde(alias = "included-patches", default)]
    pub included_patches: Vec<String>,
    #[serde(alias = "excluded-patches", default)]
    pub excluded_patches: Vec<String>,
    #[serde(alias = "patcher-args", default)]
    pub patcher_args: Option<String>,
    #[serde(
        alias = "patches-version",
        alias = "patch-version",
        alias = "patch_version",
        alias = "patches_version",
        default
    )]
    pub patches_version: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub patches: Vec<String>,
    #[serde(default)]
    pub keystore: Option<KeystoreRef>,
    #[serde(default)]
    pub module: Option<ModuleFlags>,
    #[serde(default)]
    pub dependencies: AppDependencies,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub min_version: Option<String>,
    #[serde(default)]
    pub max_version: Option<String>,
    #[serde(default)]
    pub post_patch_hooks: Vec<PostPatchHook>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default = "default_bundle_priority")]
    pub bundle_priority: u32,
}

fn default_bundle_priority() -> u32 {
    100
}

impl AppConfig {
    pub fn display_name(&self, id: &str) -> String {
        if let Some(ref n) = self.display_name {
            return n.clone();
        }
        id.split(['_', '-'])
            .map(|s| {
                let mut c = s.chars();
                match c.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn default_cli() -> String {
    "MorpheApp/morphe-desktop".to_string()
}
fn default_arch() -> Vec<String> {
    vec!["arm64-v8a".to_string()]
}
fn default_mode() -> String {
    "full".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct KeystoreRef {
    #[serde(default = "default_alias")]
    pub alias: String,
    #[serde(default = "default_keystore_file")]
    pub file: String,
}

fn default_alias() -> String {
    "revanced".to_string()
}
fn default_keystore_file() -> String {
    "keys/default.jks".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModuleFlags {
    #[serde(default = "default_true")]
    pub single: bool,
    #[serde(default = "default_true")]
    pub bundle: bool,
}

fn default_true() -> bool {
    true
}
