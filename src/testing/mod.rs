pub mod device;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReportItem {
    pub app: String,
    pub package: String,
    pub status: String, // "passed" | "failed" | "skipped"
    pub install_ms: Option<u64>,
    pub launch_ms: Option<u64>,
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestReport {
    pub items: Vec<TestReportItem>,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseExclusion {
    pub app: String,
    pub package: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReleaseFilterReport {
    pub included_apps: Vec<String>,
    pub excluded_apps: Vec<ReleaseExclusion>,
}

pub fn resolve_test_item_strategy(
    app_id: &str,
    app_cfg: Option<&crate::config::apps::AppConfig>,
) -> Option<String> {
    let tmp_dir = std::env::var("REVANCEX_TEMP_DIR").unwrap_or_else(|_| "./tmp".to_string());
    if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if fname == format!("{app_id}.strategy")
                || (fname.starts_with(&format!("{app_id}-")) && fname.ends_with(".strategy"))
            {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    let trimmed = s.trim().to_string();
                    if !trimmed.is_empty() {
                        return Some(trimmed);
                    }
                }
            }
        }
    }

    if let Some(cfg) = app_cfg {
        if let Some(_pin) = cfg
            .version
            .as_deref()
            .filter(|v| !v.eq_ignore_ascii_case("auto"))
        {
            if cfg.version_pin_strict {
                return Some("strict-honored-despite-stale".to_string());
            } else {
                return Some("pinned".to_string());
            }
        } else {
            return Some("auto".to_string());
        }
    }

    None
}

pub fn should_exit_failure(
    failed: usize,
    total: usize,
    continue_on_error: bool,
    fail_threshold: Option<u32>,
) -> bool {
    if continue_on_error {
        return false;
    }
    if failed == 0 {
        return false;
    }
    if let Some(threshold) = fail_threshold {
        if total == 0 {
            return false;
        }
        let fail_pct = (failed as f64 / total as f64) * 100.0;
        fail_pct > (threshold as f64)
    } else {
        true
    }
}

pub fn filter_release_artifacts(
    output_dir: &str,
    report: &TestReport,
) -> Result<ReleaseFilterReport> {
    let mut included_apps = Vec::new();
    let mut excluded_apps = Vec::new();

    let out_p = Path::new(output_dir);

    for item in &report.items {
        if item.status == "failed" {
            let reason = item
                .reason
                .clone()
                .unwrap_or_else(|| "smoke test failed".to_string());
            warn!("{}: excluded from release ({reason})", item.app);

            if out_p.exists() {
                if let Ok(entries) = std::fs::read_dir(out_p) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("apk") {
                            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                            let app_id = name
                                .strip_suffix("-root")
                                .or_else(|| name.strip_suffix("-patched"))
                                .unwrap_or(name);
                            if app_id == item.app {
                                info!(
                                    "Removing failed artifact from release output: {}",
                                    path.display()
                                );
                                let _ = std::fs::remove_file(&path);
                            }
                        }
                    }
                }
            }

            excluded_apps.push(ReleaseExclusion {
                app: item.app.clone(),
                package: item.package.clone(),
                reason,
            });
        } else {
            if !included_apps.contains(&item.app) {
                included_apps.push(item.app.clone());
            }
        }
    }

    let filter_report = ReleaseFilterReport {
        included_apps,
        excluded_apps,
    };

    if out_p.exists() {
        let json_path = format!("{output_dir}/release_exclusions.json");
        let json = serde_json::to_string_pretty(&filter_report)?;
        let _ = std::fs::write(&json_path, json);
    }

    info!(
        "Release filter summary: {} app(s) marked for release, {} app(s) excluded",
        filter_report.included_apps.len(),
        filter_report.excluded_apps.len()
    );

    Ok(filter_report)
}

pub async fn run_device_tests(
    apps_input: &str,
    output_dir: &str,
    require_device: bool,
    startup_grace_secs: u64,
    install_timeout_secs: u64,
) -> Result<TestReport> {
    let mut report = TestReport::default();

    let static_mode = std::env::var("REVANCEX_STATIC_TEST").is_ok();
    let devices = if static_mode {
        Vec::new()
    } else {
        device::devices().await.unwrap_or_default()
    };

    let active_device = devices.into_iter().find(|d| d.state == "device");

    let serial = match active_device {
        Some(d) => d.serial,
        None => {
            if require_device {
                anyhow::bail!("No adb device connected and --require-device was specified");
            }
            info!("No connected adb device detected — falling back to static APK smoke tests");
            let out_p = Path::new(output_dir);
            if out_p.exists() {
                let config_dir =
                    std::env::var("REVANCEX_CONFIG_DIR").unwrap_or_else(|_| "./config".to_string());
                let cfg = crate::config::load_config(&config_dir).ok();
                let tmp_dir = cfg
                    .as_ref()
                    .map(|c| c.build.temp_dir.clone())
                    .or_else(|| std::env::var("REVANCEX_TEMP_DIR").ok())
                    .unwrap_or_else(|| "./tmp".to_string());
                if let Ok(entries) = std::fs::read_dir(out_p) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("apk") {
                            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                            let app_id = name
                                .strip_suffix("-root")
                                .or_else(|| name.strip_suffix("-patched"))
                                .unwrap_or(name);
                            if apps_input != "all"
                                && !apps_input.split(',').any(|a| {
                                    let clean = a.trim();
                                    clean == app_id || clean.trim_end_matches("-root") == app_id
                                })
                            {
                                continue;
                            }

                            let app_cfg = cfg.as_ref().and_then(|c| c.apps.get(app_id));
                            let pkg = crate::utils::apk::get_apk_package_name(&path)
                                .ok()
                                .filter(|s| !s.is_empty())
                                .or_else(|| app_cfg.map(|a| a.package.clone()))
                                .unwrap_or_default();

                            if pkg.is_empty() {
                                report.items.push(TestReportItem {
                                    app: app_id.to_string(),
                                    package: String::new(),
                                    status: "failed".to_string(),
                                    install_ms: None,
                                    launch_ms: None,
                                    reason: Some(
                                        "could not resolve package name from manifest".to_string(),
                                    ),
                                    version_strategy: resolve_test_item_strategy(app_id, app_cfg),
                                });
                                report.failed += 1;
                                continue;
                            }

                            let out_dex = match crate::utils::apk::dex_entries_crc(&path) {
                                Ok(dex) if !dex.is_empty() => dex,
                                _ => {
                                    report.items.push(TestReportItem {
                                        app: app_id.to_string(),
                                        package: pkg,
                                        status: "failed".to_string(),
                                        install_ms: None,
                                        launch_ms: None,
                                        reason: Some(
                                            "corrupt APK or missing classes.dex".to_string(),
                                        ),
                                        version_strategy: resolve_test_item_strategy(
                                            app_id, app_cfg,
                                        ),
                                    });
                                    report.failed += 1;
                                    continue;
                                }
                            };

                            let is_unpatched_app = app_cfg
                                .map(|a| {
                                    a.mode == "install"
                                        || a.patch_source == "none"
                                        || a.patch_source.is_empty()
                                })
                                .unwrap_or(app_id == "microg");

                            if !is_unpatched_app {
                                let mut input_found = None;
                                if let Ok(tmp_entries) = std::fs::read_dir(&tmp_dir) {
                                    for tmp_entry in tmp_entries.flatten() {
                                        let p = tmp_entry.path();
                                        let fname =
                                            p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                        if fname == format!("{app_id}-input.apk")
                                            || fname == format!("{app_id}.apk")
                                            || (fname.starts_with(&format!("{app_id}-"))
                                                && (fname.ends_with("-input.apk")
                                                    || fname.ends_with(".apk")))
                                        {
                                            input_found = Some(p);
                                            break;
                                        }
                                    }
                                }

                                if let Some(input_p) = input_found {
                                    if let Ok(in_dex) = crate::utils::apk::dex_entries_crc(&input_p)
                                    {
                                        if in_dex == out_dex {
                                            report.items.push(TestReportItem {
                                                app: app_id.to_string(),
                                                package: pkg,
                                                status: "failed".to_string(),
                                                install_ms: None,
                                                launch_ms: None,
                                                reason: Some(
                                                    "output APK dex is identical to unpatched input APK — 0 patches applied".to_string(),
                                                ),
                                                version_strategy: resolve_test_item_strategy(app_id, app_cfg),
                                            });
                                            report.failed += 1;
                                            continue;
                                        }
                                    }
                                }
                            }

                            report.items.push(TestReportItem {
                                app: app_id.to_string(),
                                package: pkg,
                                status: "passed".to_string(),
                                install_ms: None,
                                launch_ms: None,
                                reason: Some("static APK & dex verification passed".to_string()),
                                version_strategy: resolve_test_item_strategy(app_id, app_cfg),
                            });
                            report.passed += 1;
                        }
                    }
                }
            }
            return Ok(report);
        }
    };

    info!("Using adb device: {serial}");

    let out_p = Path::new(output_dir);
    if !out_p.exists() {
        return Ok(report);
    }

    let mut apks = Vec::new();
    let cfg = crate::config::load_config("./config").ok();
    if let Ok(entries) = std::fs::read_dir(out_p) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("apk") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let app_id = name
                    .strip_suffix("-root")
                    .or_else(|| name.strip_suffix("-patched"))
                    .unwrap_or(&name);
                if apps_input == "all"
                    || apps_input.split(',').any(|a| {
                        let clean = a.trim();
                        clean == app_id || clean.trim_end_matches("-root") == app_id
                    })
                {
                    apks.push((app_id.to_string(), path.to_string_lossy().to_string()));
                }
            }
        }
    }

    for (app_id, apk_path) in apks {
        let app_cfg = cfg.as_ref().and_then(|c| c.apps.get(&app_id));
        let pkg = crate::utils::apk::get_apk_package_name(&apk_path)
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| app_cfg.map(|a| a.package.clone()))
            .unwrap_or_default();
        if pkg.is_empty() {
            report.items.push(TestReportItem {
                app: app_id.clone(),
                package: String::new(),
                status: "skipped".to_string(),
                install_ms: None,
                launch_ms: None,
                reason: Some("could not resolve package name from manifest".to_string()),
                version_strategy: resolve_test_item_strategy(&app_id, app_cfg),
            });
            report.skipped += 1;
            continue;
        }

        let t0 = std::time::Instant::now();
        let install_res = device::install(&serial, &apk_path, install_timeout_secs).await;
        let install_ms = t0.elapsed().as_millis() as u64;

        if let Err(e) = install_res {
            warn!("{app_id}: install failed: {e}");
            report.items.push(TestReportItem {
                app: app_id.clone(),
                package: pkg.clone(),
                status: "failed".to_string(),
                install_ms: Some(install_ms),
                launch_ms: None,
                reason: Some(e.to_string()),
                version_strategy: resolve_test_item_strategy(&app_id, app_cfg),
            });
            report.failed += 1;
            continue;
        }

        let t1 = std::time::Instant::now();
        let component = format!("{pkg}/.MainActivity");
        let launch_res = device::start_activity(&serial, &component).await;
        let launch_ms = t1.elapsed().as_millis() as u64;

        let mut alive = false;
        for _ in 0..startup_grace_secs {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if let Ok(Some(_)) = device::pid_of(&serial, &pkg).await {
                alive = true;
                break;
            }
        }

        let _ = device::uninstall(&serial, &pkg).await;

        if launch_res.is_ok() && alive {
            info!("{app_id}: smoke test passed (install {install_ms}ms, launch {launch_ms}ms)");
            report.items.push(TestReportItem {
                app: app_id.clone(),
                package: pkg.clone(),
                status: "passed".to_string(),
                install_ms: Some(install_ms),
                launch_ms: Some(launch_ms),
                reason: None,
                version_strategy: resolve_test_item_strategy(&app_id, app_cfg),
            });
            report.passed += 1;
        } else {
            let reason = if !alive {
                "process died within startup grace period"
            } else {
                "activity launch failed"
            };
            warn!("{app_id}: {reason}");
            report.items.push(TestReportItem {
                app: app_id.clone(),
                package: pkg.clone(),
                status: "failed".to_string(),
                install_ms: Some(install_ms),
                launch_ms: Some(launch_ms),
                reason: Some(reason.to_string()),
                version_strategy: resolve_test_item_strategy(&app_id, app_cfg),
            });
            report.failed += 1;
        }
    }

    Ok(report)
}
