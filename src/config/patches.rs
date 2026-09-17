use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PatchSourceEntry {
    pub repo: String,
    #[serde(default = "default_mpp_pattern")]
    pub asset_pattern: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

fn default_mpp_pattern() -> String {
    "*.mpp".to_string()
}
