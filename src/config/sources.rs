use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CliSource {
    pub repo: String,
    pub asset_pattern: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ApkSourceEntry {
    pub base_url: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
}
