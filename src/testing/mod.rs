pub mod device;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Passed,
    Failed,
    #[default]
    Skipped,
}

impl TestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

impl std::fmt::Display for TestStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<&str> for TestStatus {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestReportItem {
    pub app: String,
    pub package: String,
    pub status: TestStatus,
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
    temp_dir: Option<&Path>,
) -> Option<String> {
    let tmp_dir = temp_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("./tmp"));
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
    crate::report::failure_threshold_exceeded(failed, total, continue_on_error, fail_threshold)
        .is_some()
}

pub fn filter_release_artifacts(
    output_dir: impl AsRef<Path>,
    report: &TestReport,
) -> Result<ReleaseFilterReport> {
    let output_dir = output_dir.as_ref();
    let mut included_apps = Vec::new();
    let mut excluded_apps = Vec::new();

    let out_p = output_dir;

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
                            let matches_app = name == item.app
                                || name.strip_suffix("-patched") == Some(&item.app)
                                || name.starts_with(&format!("{}-", item.app))
                                || crate::utils::apk::get_apk_package_name(&path)
                                    .map(|p| p == item.package)
                                    .unwrap_or(false);
                            if matches_app {
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
        let json_path = out_p.join("release_exclusions.json");
        let json = serde_json::to_string_pretty(&filter_report)?;
        crate::utils::warn_io(
            std::fs::write(&json_path, json),
            &format!(
                "Writing release exclusions report to {}",
                json_path.display()
            ),
        );
    }

    info!(
        "Release filter summary: {} app(s) marked for release, {} app(s) excluded",
        filter_report.included_apps.len(),
        filter_report.excluded_apps.len()
    );

    Ok(filter_report)
}

fn record_test_result(report: &mut TestReport, item: TestReportItem) {
    match item.status {
        TestStatus::Passed => report.passed += 1,
        TestStatus::Failed => report.failed += 1,
        TestStatus::Skipped => report.skipped += 1,
    }
    report.items.push(item);
}

fn collect_output_test_apks(
    out_p: &Path,
    apps_input: &str,
    cfg: Option<&crate::config::Config>,
) -> Vec<(String, std::path::PathBuf)> {
    let mut apks = Vec::new();
    let Ok(entries) = std::fs::read_dir(out_p) else {
        return apks;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("apk") {
            continue;
        }
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let app_id = cfg
            .and_then(|c| c.resolve_app_id(name).map(|s| s.to_string()))
            .unwrap_or_else(|| name.strip_suffix("-patched").unwrap_or(name).to_string());
        if apps_input == "all" || apps_input.split(',').any(|a| a.trim() == app_id) {
            apks.push((app_id, path));
        }
    }
    apks
}

fn find_input_apk_for_static_test(app_id: &str, tmp_dir: &Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(tmp_dir).ok()?;
    entries.flatten().find_map(|entry| {
        let p = entry.path();
        let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let matches = fname == format!("{app_id}-input.apk")
            || fname == format!("{app_id}.apk")
            || (fname.starts_with(&format!("{app_id}-"))
                && (fname.ends_with("-input.apk") || fname.ends_with(".apk")));
        matches.then_some(p)
    })
}

fn verify_static_apk_item(
    app_id: &str,
    path: &Path,
    app_cfg: Option<&crate::config::AppConfig>,
    tmp_dir: &Path,
) -> TestReportItem {
    let strategy = resolve_test_item_strategy(app_id, app_cfg, Some(tmp_dir));
    let pkg = crate::utils::apk::get_apk_package_name(path)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| app_cfg.map(|a| a.package.clone()))
        .unwrap_or_default();

    let make_item = |pkg: String, status: TestStatus, reason: &str| TestReportItem {
        app: app_id.to_string(),
        package: pkg,
        status,
        install_ms: None,
        launch_ms: None,
        reason: Some(reason.to_string()),
        version_strategy: strategy.clone(),
    };

    if pkg.is_empty() {
        return make_item(
            String::new(),
            TestStatus::Failed,
            "could not resolve package name from manifest",
        );
    }

    let Some(out_dex) = crate::utils::apk::dex_entries_crc(path)
        .ok()
        .filter(|d| !d.is_empty())
    else {
        return make_item(
            pkg,
            TestStatus::Failed,
            "corrupt APK or missing classes.dex",
        );
    };

    let is_unpatched = app_cfg
        .map(|a| a.mode == "install" || a.patch_source == "none" || a.patch_source.is_empty())
        .unwrap_or(app_id == "microg");

    if !is_unpatched {
        if let Some(input_p) = find_input_apk_for_static_test(app_id, tmp_dir) {
            if crate::utils::apk::dex_entries_crc(&input_p).ok().as_ref() == Some(&out_dex) {
                return make_item(
                    pkg,
                    TestStatus::Failed,
                    "output APK dex is identical to unpatched input APK — 0 patches applied",
                );
            }
        }
    }

    make_item(
        pkg,
        TestStatus::Passed,
        "static APK & dex verification passed",
    )
}

fn run_static_smoke_tests(
    apps_input: &str,
    out_p: &Path,
    paths: &crate::config::AppPaths,
) -> TestReport {
    let mut report = TestReport::default();
    if !out_p.exists() {
        return report;
    }
    let cfg = crate::config::load_config(&paths.config_dir).ok();
    let tmp_dir = cfg
        .as_ref()
        .map(|c| std::path::PathBuf::from(&c.build.temp_dir))
        .unwrap_or_else(|| paths.temp_dir.clone());
    for (app_id, path) in collect_output_test_apks(out_p, apps_input, cfg.as_ref()) {
        let app_cfg = cfg.as_ref().and_then(|c| c.apps.get(&app_id));
        record_test_result(
            &mut report,
            verify_static_apk_item(&app_id, &path, app_cfg, &tmp_dir),
        );
    }
    report
}

async fn run_single_device_smoke_test(
    serial: &str,
    app_id: String,
    apk_path: &Path,
    app_cfg: Option<&crate::config::AppConfig>,
    temp_dir: &Path,
    startup_grace_secs: u64,
    install_timeout_secs: u64,
) -> TestReportItem {
    let strategy = resolve_test_item_strategy(&app_id, app_cfg, Some(temp_dir));
    let pkg = crate::utils::apk::get_apk_package_name(apk_path)
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| app_cfg.map(|a| a.package.clone()))
        .unwrap_or_default();
    if pkg.is_empty() {
        return TestReportItem {
            app: app_id,
            package: String::new(),
            status: TestStatus::Skipped,
            install_ms: None,
            launch_ms: None,
            reason: Some("could not resolve package name from manifest".to_string()),
            version_strategy: strategy,
        };
    }

    let apk_str = apk_path.to_string_lossy();
    let t0 = std::time::Instant::now();
    let install_res = device::install(serial, &apk_str, install_timeout_secs).await;
    let install_ms = t0.elapsed().as_millis() as u64;
    if let Err(e) = install_res {
        warn!("{app_id}: install failed: {e}");
        return TestReportItem {
            app: app_id,
            package: pkg,
            status: TestStatus::Failed,
            install_ms: Some(install_ms),
            launch_ms: None,
            reason: Some(e.to_string()),
            version_strategy: strategy,
        };
    }

    let t1 = std::time::Instant::now();
    let component = format!("{pkg}/.MainActivity");
    let launch_res = device::start_activity(serial, &component).await;
    let launch_ms = t1.elapsed().as_millis() as u64;

    let mut alive = false;
    for _ in 0..startup_grace_secs {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Ok(Some(_)) = device::pid_of(serial, &pkg).await {
            alive = true;
            break;
        }
    }
    let _ = device::uninstall(serial, &pkg).await;

    if launch_res.is_ok() && alive {
        info!("{app_id}: smoke test passed (install {install_ms}ms, launch {launch_ms}ms)");
        TestReportItem {
            app: app_id,
            package: pkg,
            status: TestStatus::Passed,
            install_ms: Some(install_ms),
            launch_ms: Some(launch_ms),
            reason: None,
            version_strategy: strategy,
        }
    } else {
        let reason = if !alive {
            "process died within startup grace period"
        } else {
            "activity launch failed"
        };
        warn!("{app_id}: {reason}");
        TestReportItem {
            app: app_id,
            package: pkg,
            status: TestStatus::Failed,
            install_ms: Some(install_ms),
            launch_ms: Some(launch_ms),
            reason: Some(reason.to_string()),
            version_strategy: strategy,
        }
    }
}

pub async fn run_device_tests(
    apps_input: &str,
    output_dir: &str,
    require_device: bool,
    startup_grace_secs: u64,
    install_timeout_secs: u64,
    paths: &crate::config::AppPaths,
) -> Result<TestReport> {
    let static_mode = std::env::var("REVANCEX_STATIC_TEST").is_ok();
    let devices = if static_mode {
        Vec::new()
    } else {
        device::devices().await.unwrap_or_default()
    };

    let out_p = Path::new(output_dir);
    let Some(device) = devices.into_iter().find(|d| d.state == "device") else {
        if require_device {
            anyhow::bail!("No adb device connected and --require-device was specified");
        }
        info!("No connected adb device detected — falling back to static APK smoke tests");
        return Ok(run_static_smoke_tests(apps_input, out_p, paths));
    };

    info!("Using adb device: {}", device.serial);
    let mut report = TestReport::default();
    if !out_p.exists() {
        return Ok(report);
    }

    let cfg = crate::config::load_config(&paths.config_dir).ok();
    for (app_id, apk_path) in collect_output_test_apks(out_p, apps_input, cfg.as_ref()) {
        let app_cfg = cfg.as_ref().and_then(|c| c.apps.get(&app_id));
        let item = run_single_device_smoke_test(
            &device.serial,
            app_id,
            &apk_path,
            app_cfg,
            &paths.temp_dir,
            startup_grace_secs,
            install_timeout_secs,
        )
        .await;
        record_test_result(&mut report, item);
    }

    Ok(report)
}
