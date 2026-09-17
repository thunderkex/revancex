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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestReport {
    pub items: Vec<TestReportItem>,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

pub async fn run_device_tests(
    apps_input: &str,
    output_dir: &str,
    require_device: bool,
    startup_grace_secs: u64,
    install_timeout_secs: u64,
) -> Result<TestReport> {
    let mut report = TestReport::default();

    let devices = device::devices().await.unwrap_or_default();

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
                if let Ok(entries) = std::fs::read_dir(out_p) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("apk") {
                            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                            let app_id =
                                name.trim_end_matches("-root").trim_end_matches("-patched");
                            if apps_input != "all"
                                && !apps_input.split(',').any(|a| a.trim() == app_id)
                            {
                                continue;
                            }

                            let pkg =
                                crate::utils::apk::get_apk_package_name(&path).unwrap_or_default();
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
                                    });
                                    report.failed += 1;
                                    continue;
                                }
                            };

                            let mut input_found = None;
                            if let Ok(tmp_entries) = std::fs::read_dir("./tmp") {
                                for tmp_entry in tmp_entries.flatten() {
                                    let p = tmp_entry.path();
                                    let fname =
                                        p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                    if fname.starts_with(app_id) && fname.ends_with("-input.apk") {
                                        input_found = Some(p);
                                        break;
                                    }
                                }
                            }

                            if let Some(input_p) = input_found {
                                if let Ok(in_dex) = crate::utils::apk::dex_entries_crc(&input_p) {
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
                                        });
                                        report.failed += 1;
                                        continue;
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
    if let Ok(entries) = std::fs::read_dir(out_p) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("apk") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let app_id = name.trim_end_matches("-root").trim_end_matches("-patched");
                if apps_input == "all" || apps_input.split(',').any(|a| a.trim() == app_id) {
                    apks.push((app_id.to_string(), path.to_string_lossy().to_string()));
                }
            }
        }
    }

    for (app_id, apk_path) in apks {
        let pkg = crate::utils::apk::get_apk_package_name(&apk_path).unwrap_or_default();
        if pkg.is_empty() {
            report.items.push(TestReportItem {
                app: app_id.clone(),
                package: String::new(),
                status: "skipped".to_string(),
                install_ms: None,
                launch_ms: None,
                reason: Some("could not resolve package name from manifest".to_string()),
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
            });
            report.failed += 1;
        }
    }

    Ok(report)
}
