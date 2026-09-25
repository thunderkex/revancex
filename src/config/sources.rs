use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BreakerRuleConfig {
    #[serde(default = "default_cb_threshold")]
    pub threshold: u32,
    #[serde(default = "default_cb_cooldown_secs")]
    pub cooldown_secs: u64,
    #[serde(default = "default_cb_cooldown_apps")]
    pub cooldown_apps: usize,
}

fn default_cb_threshold() -> u32 {
    3
}

fn default_cb_cooldown_secs() -> u64 {
    60
}

fn default_cb_cooldown_apps() -> usize {
    5
}

impl Default for BreakerRuleConfig {
    fn default() -> Self {
        Self {
            threshold: default_cb_threshold(),
            cooldown_secs: default_cb_cooldown_secs(),
            cooldown_apps: default_cb_cooldown_apps(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CircuitBreakersConfig {
    #[serde(default)]
    pub default: BreakerRuleConfig,
    #[serde(default)]
    pub sources: HashMap<String, BreakerRuleConfig>,
}

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
    #[serde(default)]
    pub acknowledge_dangers: bool,
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
            acknowledge_dangers: false,
        }
    }
}
