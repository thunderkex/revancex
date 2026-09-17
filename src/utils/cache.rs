use anyhow::Result;
use std::path::Path;
use tracing::info;

pub fn clean_cache(cfg: &crate::config::Config, all: bool) -> Result<()> {
    info!("Cleaning cache (all={all})...");
    let tmp = &cfg.build.temp_dir;
    if Path::new(tmp).exists() {
        std::fs::remove_dir_all(tmp)?;
        std::fs::create_dir_all(tmp)?;
    }
    if all {
        let tools = &cfg.build.tools_dir;
        if Path::new(tools).exists() {
            std::fs::remove_dir_all(tools)?;
            std::fs::create_dir_all(tools)?;
        }
    }
    println!("Cache cleaned successfully.");
    Ok(())
}
