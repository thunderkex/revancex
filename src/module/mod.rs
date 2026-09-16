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
        let name = item.name().to_string();

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
