use crate::config::Config;
use anyhow::Result;
use std::path::Path;

pub async fn run_doctor(cfg: &Config) -> Result<()> {
    crate::ui::line("=== ReVanceX Doctor ===");
    crate::ui::line(&format!(
        "OS:             {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));

    let java_line = match crate::utils::check_java_version(cfg.build.java_min_version) {
        Ok(v) => format!("v{v} (meets minimum {})", cfg.build.java_min_version),
        Err(e) => format!("NOT FOUND or TOO OLD: {e}"),
    };
    crate::ui::line(&format!("Java:           {java_line}"));

    let adb_line = match crate::testing::device::devices().await {
        Ok(devs) if devs.is_empty() => "found, 0 devices connected".to_string(),
        Ok(devs) => {
            let dev_list = devs
                .iter()
                .map(|d| format!("{} ({})", d.serial, d.state))
                .collect::<Vec<_>>()
                .join(", ");
            format!("found, devices: {dev_list}")
        }
        Err(_) => "not found on PATH".to_string(),
    };
    crate::ui::line(&format!("ADB:            {adb_line}"));

    check_tool("aria2c", "optional, accelerates downloads");
    check_tool("apkeep", "optional, direct playstore/apkeep download");
    check_tool("zipalign", "optional, post-patch APK alignment");
    check_tool("python", "optional, post-patch hooks");
    check_tool("keytool", "required for keystore generation/info");

    crate::ui::line("\n--- Bypass & Network Services ---");
    let http_client = crate::utils::make_browser_client(cfg)?;
    let fs_url =
        std::env::var("FLARESOLVERR_URL").unwrap_or_else(|_| cfg.fetch.flaresolverr.url.clone());
    check_http_service(&http_client, "FlareSolverr", &fs_url).await;

    let trawl_url = std::env::var("TRAWL_URL").unwrap_or_else(|_| cfg.fetch.trawl.url.clone());
    if !trawl_url.is_empty() {
        check_http_service(&http_client, "Trawl", &trawl_url).await;
    } else {
        crate::ui::line(&format!(
            "{:<16}[UNCONFIGURED] (TRAWL_URL not set)",
            "Trawl"
        ));
    }

    let token_status = match std::env::var("GITHUB_TOKEN") {
        Ok(t) if !t.is_empty() => {
            let masked = if t.len() > 8 {
                format!("{}...{}", &t[..4], &t[t.len() - 4..])
            } else {
                "PRESENT".to_string()
            };
            format!("[OK] ({masked})")
        }
        _ => "[MISSING] (Rate limit: 60 req/hr unauthenticated)".to_string(),
    };
    crate::ui::line(&format!("{:<16}{token_status}", "GITHUB_TOKEN"));

    crate::ui::line("\n--- Dependency Graph ---");
    if Path::new("Cargo.lock").exists() {
        crate::ui::line("Cargo.lock:     PRESENT and tracked (reproducible dependency graph)");
    } else {
        crate::ui::line("Cargo.lock:     MISSING (builds may not be reproducible)");
    }

    crate::ui::line("\n--- Keystore ---");
    let dummy_app = crate::config::apps::AppConfig {
        package: "org.revancex.doctor".to_string(),
        bundle_priority: 100,
        ..Default::default()
    };
    match crate::utils::keystore::resolve(cfg, &dummy_app).await {
        Ok(ks) => {
            crate::ui::line(&format!("Resolved file:  {}", ks.file));
            crate::ui::line(&format!("Alias:          {}", ks.alias));
            crate::ui::line(&format!(
                "Status:         {}",
                if Path::new(&ks.file).exists() {
                    "exists on disk"
                } else {
                    "will be generated"
                }
            ));
        }
        Err(e) => crate::ui::line(&format!("Keystore error: {e}")),
    }

    crate::ui::line(&format!(
        "\n--- Tools Directory ({}) ---",
        cfg.build.tools_dir
    ));
    let tools_dir = Path::new(&cfg.build.tools_dir);
    if tools_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(tools_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                crate::ui::line(&format!(
                    "  {:36} ({:.1} MB)",
                    name,
                    size as f64 / 1_048_576.0
                ));
            }
        }
    } else {
        crate::ui::line("  (directory does not exist yet)");
    }

    crate::ui::line("\n--- Config Summary ---");
    crate::ui::line(&format!("Total apps configured:   {}", cfg.apps.len()));
    let enabled_count = cfg.apps.values().filter(|a| a.enabled).count();
    crate::ui::line(&format!("Enabled apps:            {}", enabled_count));
    crate::ui::line(&format!(
        "Patch sources:           {}",
        cfg.patch_sources.len()
    ));
    crate::ui::line(&format!("Modules configured:      {}", cfg.modules.len()));

    crate::ui::line("\nDoctor check complete.");
    Ok(())
}

async fn check_http_service(client: &reqwest::Client, name: &str, url: &str) {
    match client
        .get(url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(r) => {
            crate::ui::line(&format!(
                "{:<16}[OK] (Status: {}, URL: {})",
                name,
                r.status(),
                url
            ));
        }
        Err(_) => {
            crate::ui::line(&format!("{:<16}[UNREACHABLE] (URL: {})", name, url));
        }
    }
}

fn check_tool(name: &str, purpose: &str) {
    let found = crate::utils::is_command_available(name);
    let status = if found { "OK" } else { "MISSING" };
    crate::ui::line(&format!("{:<16}[{}] ({})", name, status, purpose));
}
