use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CliSource {
    pub repo: String,
    pub asset_pattern: String,
    #[serde(default = "default_provider")]
    pub provider: String,
}

fn default_provider() -> String {
    "github".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FlareSolverrConfig {
    #[serde(default = "default_auto")]
    pub enabled: String,
    #[serde(default = "default_flaresolverr_url")]
    pub url: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_auto() -> String {
    "auto".to_string()
}

fn default_flaresolverr_url() -> String {
    "http://localhost:8191/v1".to_string()
}

fn default_timeout_secs() -> u64 {
    60
}

impl Default for FlareSolverrConfig {
    fn default() -> Self {
        Self {
            enabled: default_auto(),
            url: default_flaresolverr_url(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TrawlConfig {
    #[serde(default = "default_auto")]
    pub enabled: String,
    #[serde(default)]
    pub url: String,
}

impl Default for TrawlConfig {
    fn default() -> Self {
        Self {
            enabled: default_auto(),
            url: String::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FetchConfig {
    #[serde(default)]
    pub flaresolverr: FlareSolverrConfig,
    #[serde(default)]
    pub trawl: TrawlConfig,
    #[serde(default = "default_challenge_markers")]
    pub challenge_markers: Vec<String>,
    #[serde(default)]
    pub per_host_delay_ms: HashMap<String, u64>,
}

fn default_challenge_markers() -> Vec<String> {
    vec![
        "Just a moment...".to_string(),
        "cf-chl-bypass".to_string(),
        "Checking your browser".to_string(),
        "cloudflare-static".to_string(),
        "Attention Required".to_string(),
        "cf-turnstile".to_string(),
        "verify you are human".to_string(),
    ]
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            flaresolverr: FlareSolverrConfig::default(),
            trawl: TrawlConfig::default(),
            challenge_markers: default_challenge_markers(),
            per_host_delay_ms: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ApkSourceEntry {
    pub base_url: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

impl Default for ApkSourceEntry {
    fn default() -> Self {
        Self {
            base_url: None,
            pattern: None,
            enabled: true,
            headers: HashMap::new(),
        }
    }
}
