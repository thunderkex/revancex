use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    #[cfg(test)]
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AppSourceConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub asset_pattern: Option<String>,
    #[serde(default)]
    pub version_regex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ExpectedCert {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PinPolicy {
    #[default]
    PreferSupported,
    Strict,
    Force,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct AppConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub sources: HashMap<String, AppSourceConfig>,
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
    #[serde(alias = "aptoide-dlurl", default)]
    pub aptoide_url: Option<String>,
    #[serde(default)]
    pub source_priority: Vec<String>,
    #[serde(default)]
    pub strict_sources: bool,
    #[serde(default)]
    pub allow_build_failure: bool,
    #[serde(default)]
    pub expected_cert_sha256: Option<ExpectedCert>,
    #[serde(default)]
    pub on_unknown_compat: Option<String>,
    #[serde(alias = "apk_source", default)]
    pub apk_source: String,
    #[serde(alias = "apk_url", default)]
    pub apk_url: Option<String>,
    #[serde(alias = "arch", default = "default_arch")]
    pub architectures: Vec<String>,
    #[serde(alias = "preserve-all-archs", default)]
    pub preserve_all_archs: bool,
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
    pub version_pin_strict: bool,
    #[serde(default)]
    pub pin_policy: Option<PinPolicy>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub expected_min_patches: Option<usize>,
    #[serde(default)]
    pub patches: Vec<String>,
    #[serde(default)]
    pub keystore: Option<KeystoreRef>,
    #[serde(default)]
    pub module: ModuleFlags,
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
    #[serde(alias = "name", default)]
    pub name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default = "default_bundle_priority")]
    pub bundle_priority: u32,
}

fn default_bundle_priority() -> u32 {
    100
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    #[default]
    Patch,
    Full,
    Lite,
    Root,
    Nonroot,
    Install,
}

impl AppMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" => Self::Full,
            "lite" => Self::Lite,
            "root" => Self::Root,
            "nonroot" => Self::Nonroot,
            "install" => Self::Install,
            _ => Self::Patch,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Full => "full",
            Self::Lite => "lite",
            Self::Root => "root",
            Self::Nonroot => "nonroot",
            Self::Install => "install",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arch {
    Arm64V8a,
    ArmeabiV7a,
    X86,
    X86_64,
    All,
    Universal,
    Other(String),
}

impl Arch {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "arm64-v8a" => Self::Arm64V8a,
            "armeabi-v7a" => Self::ArmeabiV7a,
            "x86" => Self::X86,
            "x86_64" => Self::X86_64,
            "all" | "" => Self::All,
            "universal" => Self::Universal,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Arm64V8a => "arm64-v8a",
            Self::ArmeabiV7a => "armeabi-v7a",
            Self::X86 => "x86",
            Self::X86_64 => "x86_64",
            Self::All => "all",
            Self::Universal => "universal",
            Self::Other(s) => s.as_str(),
        }
    }

    pub fn is_wildcard(&self) -> bool {
        matches!(self, Self::All | Self::Universal)
    }
}

impl AppConfig {
    pub fn resolved_mode(&self, cli_mode: Option<&str>) -> AppMode {
        match cli_mode {
            Some(m) if !m.is_empty() => AppMode::parse(m),
            _ if !self.mode.is_empty() => match AppMode::parse(&self.mode) {
                AppMode::Patch => AppMode::Full,
                other => other,
            },
            _ => AppMode::Full,
        }
    }

    pub fn effective_pin_policy(&self) -> PinPolicy {
        if let Some(p) = self.pin_policy {
            p
        } else if self.version_pin_strict {
            PinPolicy::Strict
        } else {
            PinPolicy::PreferSupported
        }
    }

    pub fn expected_certs(&self) -> Vec<String> {
        match &self.expected_cert_sha256 {
            Some(ExpectedCert::Single(s)) if !s.is_empty() => vec![s.clone()],
            Some(ExpectedCert::Multiple(v)) => v.clone(),
            _ => vec![],
        }
    }

    pub fn display_name(&self, id: &str) -> String {
        if let Some(ref n) = self.name {
            if !n.is_empty() {
                return n.clone();
            }
        }
        if let Some(ref n) = self.display_name {
            if !n.is_empty() {
                return n.clone();
            }
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
    "patch".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
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
    #[serde(default)]
    pub single: bool,
    #[serde(default = "default_true")]
    pub bundle: bool,
}

impl Default for ModuleFlags {
    fn default() -> Self {
        Self {
            single: false,
            bundle: true,
        }
    }
}

fn default_true() -> bool {
    true
}
