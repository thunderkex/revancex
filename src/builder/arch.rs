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
    strip_unsupported_archs_to(apk_path, apk_path, target_arch)
}

pub fn strip_unsupported_archs_to(src_apk: &str, dst_apk: &str, target_arch: &str) -> Result<u64> {
    let initial_size = std::fs::metadata(src_apk)?.len();
    let temp_out = if src_apk == dst_apk {
        format!("{src_apk}.arch_stripped.tmp")
    } else {
        dst_apk.to_string()
    };

    info!("Stripping native libs for arch '{target_arch}' from {src_apk} to {dst_apk}...");

    let mut stripped_files = 0u32;
    let mut stripped_bytes = 0u64;

    {
        let src = std::fs::File::open(src_apk)?;
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
    rewrite(src_apk, &temp_out, move |name| {
        if should_strip(name, &arch) {
            EntryAction::Drop
        } else {
            EntryAction::Keep
        }
    })?;

    if src_apk == dst_apk {
        std::fs::remove_file(src_apk)?;
        std::fs::rename(&temp_out, src_apk)?;
    }

    let final_size = std::fs::metadata(dst_apk)?.len();
    let saved = initial_size.saturating_sub(final_size);

    info!(
        "Arch stripping complete: removed {stripped_files} libs ({stripped_bytes} uncompressed bytes). \
         {initial_size} → {final_size} (saved {saved} bytes)."
    );

    Ok(saved)
}

pub fn verify_arch_native_libs(
    original_apk: &str,
    stripped_apk: &str,
    target_arch: &str,
) -> Result<()> {
    let original_has_native_libs = {
        let file = std::fs::File::open(original_apk)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut has_lib = false;
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            if entry.name().starts_with("lib/") && entry.name().ends_with(".so") {
                has_lib = true;
                break;
            }
        }
        has_lib
    };

    if !original_has_native_libs {
        return Ok(());
    }

    let target_prefix = format!("lib/{target_arch}/");
    let file = std::fs::File::open(stripped_apk)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut has_target_lib = false;

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.name().starts_with(&target_prefix) && entry.name().ends_with(".so") {
            has_target_lib = true;
            break;
        }
    }

    if !has_target_lib {
        anyhow::bail!(
            "no native libs for {target_arch} after arch-stripping — check the input APK / cache"
        );
    }

    Ok(())
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
