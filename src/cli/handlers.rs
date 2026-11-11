use crate::config::{self, Config};
use crate::testing::TestStatus;
use crate::{builder, module, orchestrator, pages, patcher, report, testing, ui, utils};
use anyhow::{Context, Result};
use std::path::Path;

pub struct BuildCmdArgs {
    pub apps: String,
    pub arch: String,
    pub mode: Option<String>,
    pub output: String,
    pub force: bool,
    pub keep_going: bool,
    pub report: Option<String>,
}

pub async fn handle_build(config: &Config, args: BuildCmdArgs) -> Result<()> {
    utils::check_java_version(config.build.java_min_version)?;
    let output_dir = if args.output != "./output" {
        args.output
    } else {
        config.build.output_dir.clone()
    };
    let summary = orchestrator::build_apps(
        config,
        &args.apps,
        &args.arch,
        args.mode.as_deref(),
        &output_dir,
        args.force,
    )
    .await?;

    let report_path = args
        .report
        .unwrap_or_else(|| format!("{output_dir}/BUILD_REPORT.json"));
    utils::write_build_report(&summary, &report_path)?;

    let allow_fail = |id: &str| {
        config
            .apps
            .get(id)
            .map(|a| a.allow_build_failure)
            .unwrap_or(false)
    };
    let has_failures = summary.failed.iter().any(|(id, _)| !allow_fail(id))
        || summary
            .reports
            .iter()
            .any(|(id, r)| r.status != "success" && !allow_fail(id));

    orchestrator::print_summary(config, &summary);
    if has_failures && !args.keep_going {
        ui::exit(1);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_module(
    config: &Config,
    single: bool,
    bundle: bool,
    apps: String,
    include_webui: bool,
    no_webui: bool,
    output: String,
    version_code: Option<i32>,
    release_tag: Option<String>,
) -> Result<()> {
    let effective_webui = include_webui && !no_webui;
    let build_opts = module::ModuleBuildOptions {
        version_code,
        release_tag,
        include_webui: effective_webui,
    };
    if single {
        module::single::build_single_module_with_options(config, &apps, &output, &build_opts)
            .await?;
    }
    if bundle {
        module::bundle::build_bundle_module_with_options(
            config,
            &apps,
            effective_webui,
            &output,
            &build_opts,
        )
        .await?;
    }
    Ok(())
}

async fn check_online_url(
    client: &reqwest::Client,
    id: &str,
    field: &str,
    url: &str,
) -> Option<String> {
    ui::info(&format!("validating {field} for {id}: {url}"));
    match client.head(url).send().await {
        Ok(resp) => {
            let st = resp.status();
            if st == reqwest::StatusCode::NOT_FOUND || st == reqwest::StatusCode::GONE {
                Some(format!("App '{id}' {field} {url} returned {st}"))
            } else {
                None
            }
        }
        Err(e) => Some(format!("App '{id}' {field} {url} request failed: {e}")),
    }
}

pub async fn handle_validate(
    config: &Config,
    strict: bool,
    online: bool,
    apps: Option<String>,
) -> Result<()> {
    config::validate_yaml_duplicates(&config.paths.config_dir)?;
    config::validate_config(config, strict)?;
    let target_apps = apps
        .as_deref()
        .map(|a| orchestrator::resolver::resolve_targets(config, a))
        .transpose()?;
    let warnings =
        patcher::metadata::validate_patch_compatibilities(config, target_apps.as_deref()).await;
    for w in &warnings {
        ui::warn_once(w);
    }
    if strict && !warnings.is_empty() {
        anyhow::bail!("Strict validation failed: {} warning(s)", warnings.len());
    }
    if online {
        let client = utils::make_browser_client(config)?;
        let mut failures = Vec::new();
        for (id, app) in &config.apps {
            if !app.enabled
                || target_apps
                    .as_ref()
                    .is_some_and(|targets| !targets.iter().any(|t| t == id))
            {
                continue;
            }
            for (field, url_opt) in [
                ("uptodown_url", app.uptodown_url.as_deref()),
                ("apkmirror_url", app.apkmirror_url.as_deref()),
            ] {
                if let Some(url) = url_opt {
                    if let Some(err) = check_online_url(&client, id, field, url).await {
                        failures.push(err);
                    }
                }
            }
        }
        if !failures.is_empty() {
            for f in &failures {
                ui::item(ui::Status::Fail, "validate", "", f, None);
            }
            anyhow::bail!(
                "Online URL validation failed with {} error(s)",
                failures.len()
            );
        }
        ui::item_ok("validate", "", "online URL validation passed", None);
    }
    Ok(())
}

pub async fn handle_download_tools(config: &Config, apps: String) -> Result<()> {
    utils::check_java_version(config.build.java_min_version)?;
    let targets = orchestrator::resolver::resolve_targets(config, &apps)?;
    let client = utils::make_github_client(config)?;
    let summary = utils::download_tools(config, Some(&targets), &client).await?;
    if summary.tools_changed {
        if let Ok(path) = std::env::var("GITHUB_OUTPUT") {
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                use std::io::Write;
                utils::warn_io(
                    writeln!(file, "tools_changed=true"),
                    &format!("Writing tools_changed to GITHUB_OUTPUT ({path})"),
                );
            }
        }
    }
    Ok(())
}

pub fn handle_gen_apps_json(
    config: &Config,
    opts: pages::GenAppsJsonOptions,
    out: String,
    update_dir: String,
) -> Result<()> {
    let json = pages::generate_apps_json_with_options(config, &opts)?;
    if let Some(parent) = Path::new(&out).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&out, &json)?;
    println!("{out} written ({} bytes)", json.len());
    if !update_dir.trim().is_empty() {
        let files = pages::generate_ota_endpoints_with_options(config, &opts, &update_dir)?;
        println!("{update_dir} updated ({} file(s))", files.len());
    }
    Ok(())
}

pub async fn handle_fetch_apk(
    config: &Config,
    app: String,
    arch: String,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    let app_cfg = config
        .apps
        .get(&app)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app}"))?;
    if dry_run {
        println!("Dry run resolution for {app}:");
        println!("  Package:        {}", app_cfg.package);
        println!("  Patch source:   {}", app_cfg.patch_source);
        println!("  Target arch:    {arch}");
        println!("  Direct APK URL: {:?}", app_cfg.apk_url);
        println!("  Archive URL:    {:?}", app_cfg.archive_url);
        println!("  APKMirror URL:  {:?}", app_cfg.apkmirror_url);
        println!("  Uptodown URL:   {:?}", app_cfg.uptodown_url);
        return Ok(());
    }
    std::fs::create_dir_all(&config.build.temp_dir)?;
    let net = crate::net::SharedNet::new(config)?;
    let path = builder::fetch_apk(config, &app, app_cfg, &arch, force, None, &net)
        .await?
        .path;
    let size = std::fs::metadata(&path)?.len();
    println!("SUCCESS: {app} downloaded to {path} ({size} bytes)");
    Ok(())
}

pub async fn handle_patches(config: &Config, app: String, mode: String, json: bool) -> Result<()> {
    let app_cfg = config
        .apps
        .get(&app)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app}"))?;
    let is_root = mode.eq_ignore_ascii_case("root");
    let mpp_path = patcher::metadata::resolve_mpp_path(config, app_cfg);
    let meta = if let Some(ref p) = mpp_path {
        patcher::metadata::load(config, p).await.ok()
    } else {
        None
    };
    let plan = patcher::plan::resolve(
        app_cfg,
        is_root,
        meta.as_ref(),
        None,
        &config.build.patcher.rules,
        &config.build.patcher.auto_disable,
    );
    if json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!("Plan for {app} (mode: {mode}):");
        println!("  Included ({}): {:?}", plan.included.len(), plan.included);
        println!("  Excluded ({}): {:?}", plan.excluded.len(), plan.excluded);
        println!("  Auto-disabled: {}", plan.auto_disabled.len());
        for d in &plan.auto_disabled {
            println!("    - {}: {:?}", d.name, d.reason);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_test(
    config: &Config,
    apps: String,
    output: String,
    require_device: bool,
    startup_grace_secs: u64,
    install_timeout_secs: u64,
    json: bool,
    continue_on_error: bool,
    fail_threshold: Option<u32>,
    test_report_out: Option<String>,
) -> Result<()> {
    let report = testing::run_device_tests(
        &apps,
        &output,
        require_device,
        startup_grace_secs,
        install_timeout_secs,
        &config.paths,
    )
    .await?;

    let filter_report = testing::filter_release_artifacts(&output, &report)?;
    let report_path = test_report_out.or_else(|| {
        Path::new(&output)
            .exists()
            .then(|| format!("{output}/TEST_REPORT.json"))
    });
    if let Some(ref rp) = report_path {
        if let Some(parent) = Path::new(rp).parent() {
            if !parent.as_os_str().is_empty() {
                utils::warn_io(
                    std::fs::create_dir_all(parent),
                    &format!("Creating test report directory {}", parent.display()),
                );
            }
        }
        if let Ok(serialized) = serde_json::to_string_pretty(&report) {
            utils::warn_io(
                std::fs::write(rp, serialized),
                &format!("Writing test report to {rp}"),
            );
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        ui::phase("Smoke test");
        for item in &report.items {
            let reason = item.reason.as_deref().unwrap_or("");
            let strat = item
                .version_strategy
                .as_deref()
                .map(|s| format!(" [strategy: {s}]"))
                .unwrap_or_default();
            let status = match item.status {
                TestStatus::Passed => ui::Status::Ok,
                TestStatus::Failed => ui::Status::Fail,
                TestStatus::Skipped => ui::Status::Skip,
            };
            let suffix = if reason.is_empty() {
                String::new()
            } else {
                format!(" — {reason}")
            };
            ui::item(
                status,
                &item.app,
                "",
                &format!("{}{strat}{suffix}", item.status),
                None,
            );
        }
        for excl in &filter_report.excluded_apps {
            ui::warn_once(&format!(
                "excluded from release: {}: {}",
                excl.app, excl.reason
            ));
        }
        ui::item(
            ui::Status::Info,
            "smoke-test",
            "",
            &format!(
                "{} passed, {} failed, {} skipped",
                report.passed, report.failed, report.skipped
            ),
            None,
        );
        ui::end_phase();
    }

    let total = report.passed + report.failed + report.skipped;
    if testing::should_exit_failure(report.failed, total, continue_on_error, fail_threshold) {
        ui::exit(1);
    }
    Ok(())
}

pub async fn handle_sources_probe(
    config: &Config,
    app: String,
    arch: String,
    version: Option<String>,
) -> Result<()> {
    let app_cfg = config
        .apps
        .get(&app)
        .ok_or_else(|| anyhow::anyhow!("App '{app}' not found in configuration"))?;

    let net = crate::net::SharedNet::new(config)?;
    let mut ctx = crate::sources::SourceCtx::new(&net.browser_client, config);
    ctx.no_redirect_client = Some(&net.no_redirect_client);
    ctx.fetcher = Some(&net.fetcher);
    let registry = crate::sources::default_registry();
    let ordered = crate::sources::source_order(&registry, app_cfg, config);

    println!(
        "{:<14} | {:<8} | {:<16} | {:<25} | {:<6} | REASON/TRACE",
        "SOURCE", "OUTCOME", "VERSION", "HOST", "MS"
    );
    println!(
        "{:-<14}-+-{:-<8}-+-{:-<16}-+-{:-<25}-+-{:-<6}-+-----------------------",
        "", "", "", "", ""
    );

    let mut any_resolved = false;
    for src in ordered {
        let start = std::time::Instant::now();
        let outcome = src.resolve(&ctx, app_cfg, version.as_deref(), &arch).await;
        let elapsed_ms = start.elapsed().as_millis();

        match outcome {
            Ok(Some(res)) => {
                any_resolved = true;
                let host = reqwest::Url::parse(&res.url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_else(|| "-".to_string());
                let ver = res.version_found.unwrap_or_else(|| "-".to_string());
                println!(
                    "{:<14} | {:<8} | {:<16} | {:<25} | {:<6} | OK",
                    src.name(),
                    "hit",
                    ver,
                    host,
                    elapsed_ms
                );
            }
            Ok(None) => println!(
                "{:<14} | {:<8} | {:<16} | {:<25} | {:<6} | not found",
                src.name(),
                "miss",
                "-",
                "-",
                elapsed_ms
            ),
            Err(e) => println!(
                "{:<14} | {:<8} | {:<16} | {:<25} | {:<6} | {e}",
                src.name(),
                "error",
                "-",
                "-",
                elapsed_ms
            ),
        }
    }

    if !any_resolved {
        ui::exit(1);
    }
    Ok(())
}

pub fn handle_report_check(
    config: &Config,
    path: std::path::PathBuf,
    fail_threshold: Option<u32>,
    r#continue: bool,
) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Build report not found: {}", path.display());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let (schema_version, apps, summary) = if let Ok(br) =
        serde_json::from_str::<report::BuildReport>(&content)
    {
        (Some(br.schema_version), br.apps, Some(br.summary))
    } else if let Ok(apps_map) =
        serde_json::from_str::<std::collections::BTreeMap<String, report::AppBuildReport>>(&content)
    {
        (None, apps_map, None)
    } else {
        anyhow::bail!(
            "Failed to parse build report in {} as valid BuildReport",
            path.display()
        );
    };

    let mut bad = Vec::new();
    for (app, rep) in &apps {
        let allow_failure = config.apps.get(app).is_some_and(|a| a.allow_build_failure);
        let has_conflict = rep.version_conflict.is_some();
        let is_failed = rep.status != "success";
        if (is_failed || has_conflict) && !allow_failure {
            bad.push(format!(
                "{app} (status={}, conflict={has_conflict})",
                rep.status
            ));
        }
    }

    if !bad.is_empty() {
        let total = apps.len().max(1);
        let fail_pct = (bad.len() as f64 / total as f64) * 100.0;
        ui::warn_once(&format!(
            "Build report contains {} failed or unallowed app(s) ({:.1}% of total): {}",
            bad.len(),
            fail_pct,
            bad.join(", ")
        ));
        if report::failure_threshold_exceeded(bad.len(), total, r#continue, fail_threshold)
            .is_some()
        {
            anyhow::bail!(
                "Build report failure threshold exceeded ({:.1}% > {:?}%): {}",
                fail_pct,
                fail_threshold,
                bad.join(", ")
            );
        }
    }

    let ver_str = schema_version.as_deref().unwrap_or("legacy");
    if let Some(s) = summary {
        ui::item_ok(
            "report",
            "",
            &format!(
                "schema v{ver_str} valid: {} total, {} success, {} failed, {} skipped",
                s.total, s.success, s.failed, s.skipped
            ),
            None,
        );
    } else {
        ui::item_ok(
            "report",
            "",
            &format!("schema v{ver_str} valid: all apps within threshold"),
            None,
        );
    }
    Ok(())
}
