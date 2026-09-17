use crate::config::Config;
use anyhow::Result;
use std::path::Path;

pub async fn run_doctor(cfg: &Config) -> Result<()> {
    println!("=== ReVanceX Doctor ===");
    println!(
        "OS:             {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH
    );

    print!("Java:           ");
    match crate::utils::check_java_version(cfg.build.java_min_version) {
        Ok(v) => println!("v{v} (meets minimum {})", cfg.build.java_min_version),
        Err(e) => println!("NOT FOUND or TOO OLD: {e}"),
    }

    print!("ADB:            ");
    match crate::testing::device::devices().await {
        Ok(devs) => {
            if devs.is_empty() {
                println!("found, 0 devices connected");
            } else {
                let dev_list = devs
                    .iter()
                    .map(|d| format!("{} ({})", d.serial, d.state))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("found, devices: {dev_list}");
            }
        }
        Err(_) => println!("not found on PATH"),
    }

    check_tool("aria2c", "optional, accelerates downloads");
    check_tool("zipalign", "optional, post-patch APK alignment");
    check_tool("python", "optional, post-patch hooks");
    check_tool("keytool", "required for keystore generation/info");

    println!("\n--- Keystore ---");
    let dummy_app = crate::config::apps::AppConfig {
        enabled: true,
        package: String::new(),
        patch_source: String::new(),
        patch_branch: None,
        cli_source: String::new(),
        uptodown_url: None,
        apkmirror_url: None,
        archive_url: None,
        apkpure_url: None,
        source_priority: vec![],
        apk_source: String::new(),
        apk_url: None,
        architectures: vec![],
        mode: String::new(),
        included_patches: vec![],
        excluded_patches: vec![],
        patcher_args: None,
        patches_version: None,
        version: None,
        patches: vec![],
        keystore: None,
        module: None,
        dependencies: crate::config::apps::AppDependencies::default(),
        description: None,
        min_version: None,
        max_version: None,
        post_patch_hooks: vec![],
        display_name: None,
        bundle_priority: 100,
    };
    match crate::utils::keystore::resolve(cfg, &dummy_app).await {
        Ok(ks) => {
            println!("Resolved file:  {}", ks.file);
            println!("Alias:          {}", ks.alias);
            println!(
                "Status:         {}",
                if Path::new(&ks.file).exists() {
                    "exists on disk"
                } else {
                    "will be generated"
                }
            );
        }
        Err(e) => println!("Keystore error: {e}"),
    }

    println!("\n--- Tools Directory ({}) ---", cfg.build.tools_dir);
    let tools_dir = Path::new(&cfg.build.tools_dir);
    if tools_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(tools_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                println!("  {:36} ({:.1} MB)", name, size as f64 / 1_048_576.0);
            }
        }
    } else {
        println!("  (directory does not exist yet)");
    }

    println!("\n--- Config Summary ---");
    println!("Total apps configured:   {}", cfg.apps.len());
    let enabled_count = cfg.apps.values().filter(|a| a.enabled).count();
    println!("Enabled apps:            {}", enabled_count);
    println!("Patch sources:           {}", cfg.patch_sources.len());
    println!("Modules configured:      {}", cfg.modules.len());

    println!("\nDoctor check complete.");
    Ok(())
}

fn check_tool(name: &str, purpose: &str) {
    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    let found = std::process::Command::new(which_cmd)
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let status = if found { "OK" } else { "MISSING" };
    println!("{:<16}[{}] ({})", name, status, purpose);
}
