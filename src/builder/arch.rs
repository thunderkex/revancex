use anyhow::Result;
use tracing::info;

use crate::utils::zip::{rewrite, EntryAction};

pub const ANDROID_ABIS: &[&str] = &[
    "arm64-v8a",
    "armeabi-v7a",
    "x86",
    "x86_64",
    "armeabi",
    "mips",
    "mips64",
];

pub fn strip_unsupported_archs(apk_path: &str, target_arch: &str) -> Result<u64> {
    let initial_size = std::fs::metadata(apk_path)?.len();
    let temp_out = format!("{apk_path}.arch_stripped.tmp");

    info!("Stripping native libs for arch '{target_arch}' from {apk_path}...");

    let mut stripped_files = 0u32;
    let mut stripped_bytes = 0u64;

    {
        let src = std::fs::File::open(apk_path)?;
        let mut zip_in = zip::ZipArchive::new(src)?;
        for i in 0..zip_in.len() {
            let entry = zip_in.by_index(i)?;
            let name = entry.name();
            if should_strip(name, target_arch) {
                stripped_files += 1;
                stripped_bytes += entry.size();
            }
        }
    }

    let arch = target_arch.to_string();
    rewrite(apk_path, &temp_out, move |name| {
        if should_strip(name, &arch) {
            EntryAction::Drop
        } else {
            EntryAction::Keep
        }
    })?;

    std::fs::remove_file(apk_path)?;
    std::fs::rename(&temp_out, apk_path)?;

    let final_size = std::fs::metadata(apk_path)?.len();
    let saved = initial_size.saturating_sub(final_size);

    info!(
        "Arch stripping complete: removed {stripped_files} libs ({stripped_bytes} uncompressed bytes). \
         {initial_size} → {final_size} (saved {saved} bytes)."
    );

    Ok(saved)
}

fn should_strip(name: &str, target_arch: &str) -> bool {
    if name.starts_with("lib/") {
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() >= 2 {
            let abi = parts[1];
            return ANDROID_ABIS.contains(&abi) && abi != target_arch;
        }
    }
    false
}
