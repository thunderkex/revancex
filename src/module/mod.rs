pub mod bundle;
pub mod single;
pub mod webui;

pub use bundle::build_bundle_module;
pub use single::build_single_module;

use anyhow::{Context, Result};
use tracing::info;

pub fn verify_bundle(zip_path: &str) -> Result<()> {
    let file = std::fs::File::open(zip_path)
        .with_context(|| format!("Failed to open module zip at {zip_path}"))?;
    let mut archive = zip::ZipArchive::new(file).with_context(|| "Failed to read zip archive")?;

    let mut has_update_binary = false;
    let mut has_updater_script = false;
    let mut has_module_prop = false;
    let mut has_customize_sh = false;
    let mut has_service_sh = false;
    let mut has_utils_sh = false;
    let mut apk_count = 0;
    let id_re = regex::Regex::new(r"^[a-zA-Z][a-zA-Z0-9._-]+$")?;

    for i in 0..archive.len() {
        let mut item = archive.by_index(i)?;
        let name = item.name()?.to_string();

        match name.as_str() {
            "META-INF/com/google/android/update-binary" => {
                if item.size() > 0 {
                    has_update_binary = true;
                }
            }
            "META-INF/com/google/android/updater-script" => {
                use std::io::Read;
                let mut content = String::new();
                item.read_to_string(&mut content)?;
                if content.contains("#MAGISK") {
                    has_updater_script = true;
                }
            }
            "module.prop" => {
                use std::io::Read;
                let mut content = String::new();
                item.read_to_string(&mut content)?;
                let mut id_valid = false;
                let mut version_code_valid = false;
                for line in content.lines() {
                    if let Some(val) = line.strip_prefix("id=") {
                        let id = val.trim();
                        if id_re.is_match(id) {
                            id_valid = true;
                        }
                    }
                    if let Some(val) = line.strip_prefix("versionCode=") {
                        if val.trim().parse::<u64>().is_ok_and(|v| v > 0) {
                            version_code_valid = true;
                        }
                    }
                }
                if id_valid
                    && version_code_valid
                    && content.contains("name=")
                    && content.contains("version=")
                {
                    has_module_prop = true;
                }
            }
            "customize.sh" => {
                if item.size() > 0 {
                    has_customize_sh = true;
                }
            }
            "service.sh" => {
                if item.size() > 0 {
                    has_service_sh = true;
                }
            }
            "utils.sh" => {
                if item.size() > 0 {
                    has_utils_sh = true;
                }
            }
            _ => {
                if name.starts_with("apks/") && name.ends_with(".apk") && item.size() > 50 {
                    apk_count += 1;
                }
            }
        }
    }

    if !has_update_binary {
        anyhow::bail!("Verification failed: missing or empty update-binary");
    }
    if !has_updater_script {
        anyhow::bail!("Verification failed: updater-script does not contain #MAGISK");
    }
    if !has_module_prop {
        anyhow::bail!("Verification failed: missing or invalid module.prop");
    }
    if !has_customize_sh {
        anyhow::bail!("Verification failed: missing or empty customize.sh");
    }
    if !has_service_sh {
        anyhow::bail!("Verification failed: missing or empty service.sh");
    }
    if !has_utils_sh {
        anyhow::bail!("Verification failed: missing or empty utils.sh");
    }
    if apk_count == 0 {
        anyhow::bail!("Verification failed: no valid APKs found under apks/");
    }

    info!("Magisk module verification passed! {apk_count} APKs bundled.");
    Ok(())
}

pub const UPDATE_BINARY: &[u8] = include_bytes!("../../templates/update-binary");
pub const CUSTOMIZE_SH: &[u8] = include_bytes!("../../templates/customize.sh");
pub const UTILS_SH: &[u8] = include_bytes!("../../templates/utils.sh");
pub const SERVICE_SH: &[u8] = include_bytes!("../../templates/service.sh.tpl");
pub const ACTION_SH: &[u8] = include_bytes!("../../templates/action.sh");
pub const UNINSTALL_SH: &[u8] = include_bytes!("../../templates/uninstall.sh");
pub const MODULE_PROP_TPL: &str = include_str!("../../templates/module.prop.tpl");

pub const OTA_BASE_URL: &str = "https://thunderkex.github.io/revancex/update";

#[derive(Debug, Clone)]
pub struct ModuleBuildOptions {
    pub version_code: Option<i32>,
    pub release_tag: Option<String>,
    pub include_webui: bool,
}

impl Default for ModuleBuildOptions {
    fn default() -> Self {
        Self {
            version_code: None,
            release_tag: None,
            include_webui: true,
        }
    }
}

pub fn ota_update_json_url(module_id: &str) -> String {
    format!("{OTA_BASE_URL}/{module_id}.json")
}

pub fn ota_changelog_url(module_id: &str) -> String {
    format!("{OTA_BASE_URL}/{module_id}-changelog.md")
}

pub fn compute_version_code(date: chrono::NaiveDate, same_day_counter: u8) -> Result<i32> {
    use chrono::Datelike;
    anyhow::ensure!(
        same_day_counter <= 99,
        "same_day_counter must be in 0..=99, got {same_day_counter}"
    );
    let code: i64 = (date.year() as i64) * 1_000_000
        + (date.month() as i64) * 10_000
        + (date.day() as i64) * 100
        + (same_day_counter as i64);
    i32::try_from(code).context("versionCode YYYYMMDDbb overflows int32")
}

pub fn resolve_module_version_code(explicit: Option<i32>, registry_path: Option<&str>) -> i32 {
    if let Some(vc) = explicit.filter(|&v| v > 0) {
        return vc;
    }
    if let Ok(env_vc) = std::env::var("REVANCEX_VERSION_CODE") {
        if let Ok(parsed) = env_vc.trim().parse::<i32>() {
            if parsed > 0 {
                return parsed;
            }
        }
    }
    let today = chrono::Utc::now().date_naive();
    let day_base = compute_version_code(today, 0).unwrap_or(2026010100);
    let day_max = day_base + 99;
    let mut max_bb = 0i32;

    if let Some(path) = registry_path {
        let reg = crate::pages::load_registry(path);
        for item in reg
            .single_modules
            .values()
            .chain(reg.bundles.values())
            .chain(reg.custom_bundles.values())
        {
            if let Some(vc) = item.version_code {
                if vc >= day_base && vc <= day_max {
                    max_bb = max_bb.max(vc - day_base);
                }
            }
        }
    }
    let next_bb = (max_bb + 1).clamp(1, 99) as u8;
    compute_version_code(today, next_bb).unwrap_or(day_base + 1)
}

#[allow(clippy::too_many_arguments)]
pub fn render_module_prop(
    id: &str,
    name: &str,
    version: &str,
    version_code: i32,
    author: &str,
    description: &str,
    min_magisk: u32,
    min_kernelsu: u32,
    update_json: &str,
) -> String {
    MODULE_PROP_TPL
        .replace("{{ID}}", id)
        .replace("{{NAME}}", name)
        .replace("{{VERSION}}", version)
        .replace("{{VERSION_CODE}}", &version_code.to_string())
        .replace("{{AUTHOR}}", author)
        .replace("{{DESCRIPTION}}", description)
        .replace("{{MIN_MAGISK}}", &min_magisk.to_string())
        .replace("{{MIN_KERNELSU}}", &min_kernelsu.to_string())
        .replace("{{UPDATE_JSON}}", update_json)
}
