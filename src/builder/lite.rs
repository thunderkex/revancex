use anyhow::Result;
use tracing::info;

use crate::utils::zip::{rewrite, EntryAction};

pub fn optimize_apk(apk_path: &str) -> Result<u64> {
    let initial_size = std::fs::metadata(apk_path)?.len();
    let temp_out = format!("{apk_path}.lite.tmp");

    info!("Running lite optimization on {apk_path}...");

    let mut removed_count = 0u32;
    let mut removed_bytes = 0u64;

    {
        let src = std::fs::File::open(apk_path)?;
        let mut zip_in = zip::ZipArchive::new(src)?;
        for i in 0..zip_in.len() {
            let entry = zip_in.by_index(i)?;
            let name = entry.name();
            if should_drop(name) {
                removed_count += 1;
                removed_bytes += entry.size();
            }
        }
    }

    rewrite(apk_path, &temp_out, |name| {
        if should_drop(name) {
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
        "Lite optimization complete: stripped {removed_count} entries ({removed_bytes} raw bytes). \
         {initial_size} → {final_size} (saved {saved} bytes)."
    );

    Ok(saved)
}

fn should_drop(name: &str) -> bool {
    let is_stale_sig = name.starts_with("META-INF/")
        && (name.ends_with(".SF")
            || name.ends_with(".RSA")
            || name.ends_with(".DSA")
            || name.ends_with(".EC"));
    let is_metadata = name.ends_with(".properties")
        || name.starts_with("assets/dexopt/")
        || name.contains("kotlin-tooling-metadata.json");
    is_stale_sig || is_metadata
}
