use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use revancex::{builder, config, module, orchestrator, pages, patcher, report, testing, utils};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "revancex",
    version,
    about = "Modern patcher with dynamic configuration",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(
        short,
        long,
        alias = "config",
        global = true,
        default_value = "./config"
    )]
    config_dir: String,
}

#[derive(Subcommand)]
enum Commands {
    Apps {
        #[arg(short, long)]
        json: bool,
    },

    Build {
        #[arg(short, long, default_value = "all")]
        apps: String,

        #[arg(long, default_value = "arm64-v8a")]
        arch: String,

        #[arg(short, long)]
        mode: Option<String>,

        #[arg(short, long, default_value = "./output")]
        output: String,

        #[arg(long)]
        force: bool,

        #[arg(long)]
        keep_going: bool,

        #[arg(long)]
        report: Option<String>,
    },

    Module {
        #[arg(long)]
        single: bool,

        #[arg(long)]
        bundle: bool,

        #[arg(short, long, default_value = "all")]
        apps: String,

        #[arg(long, default_value_t = true)]
        include_webui: bool,

        #[arg(short, long, default_value = "./modules")]
        output: String,
    },

    CheckUpdates {
        #[arg(short, long)]
        json: bool,
    },

    GenerateMatrix {
        #[arg(short, long, default_value = "all")]
        apps: String,

        #[arg(long, default_value = "all")]
        arch: String,

        #[arg(short, long, default_value = "auto")]
        mode: String,
    },

    Validate {
        #[arg(short, long)]
        strict: bool,
        #[arg(long)]
        online: bool,
        #[arg(short, long)]
        apps: Option<String>,
    },

    Clean {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        stale_tools: bool,
    },

    DownloadTools {
        #[arg(short, long, default_value = "all")]
        apps: String,
    },

    GenAppsJson {
        #[arg(long, default_value = "config/releases_registry.json")]
        registry: String,
    },

    FetchApk {
        app: String,
        #[arg(long, default_value = "arm64-v8a")]
        arch: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, short)]
        force: bool,
    },

    VerifyModule {
        zip: String,
    },

    Keystore {
        #[arg(long)]
        info: bool,
    },

    Patches {
        app: String,
        #[arg(long, default_value = "nonroot")]
        mode: String,
        #[arg(short, long)]
        json: bool,
    },

    /// Smoke test built APKs.
    ///
    /// Precedence for exit codes:
    /// 1. `--continue`: Always exits 0, allowing callers / release pipelines to inspect the
    ///    report and continue with releasing passing apps without aborting.
    /// 2. `--fail-threshold <percent>`: When `--continue` is NOT passed, exits non-zero only
    ///    if the failure percentage strictly exceeds `<percent>`.
    /// 3. Default: Exits non-zero if any single app fails (strict developer mode).
    Test {
        #[arg(short, long, default_value = "all")]
        apps: String,
        #[arg(short, long, default_value = "./output")]
        output: String,
        #[arg(long)]
        require_device: bool,
        #[arg(long, default_value_t = 8)]
        startup_grace_secs: u64,
        #[arg(long, default_value_t = 180)]
        install_timeout_secs: u64,
        #[arg(short, long)]
        json: bool,
        #[arg(long = "continue")]
        continue_on_error: bool,
        #[arg(long)]
        fail_threshold: Option<u32>,
    },

    Doctor,

    Sources {
        #[command(subcommand)]
        sub: SourcesSubcommand,
    },

    Report {
        #[command(subcommand)]
        sub: ReportSubcommand,
    },
}

#[derive(Subcommand)]
enum ReportSubcommand {
    Check {
        path: std::path::PathBuf,
        #[arg(
            long,
            help = "Maximum allowed failure percentage (0-100) before exiting with error"
        )]
        fail_threshold: Option<u32>,
        #[arg(
            long = "continue",
            help = "Do not exit with error on failures, only warn"
        )]
        r#continue: bool,
    },
}

#[derive(Subcommand)]
enum SourcesSubcommand {
    Probe {
        app: String,
        #[arg(long, default_value = "arm64-v8a")]
        arch: String,
        #[arg(long)]
        version: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(if cli.verbose {
            "debug".parse()?
        } else {
            "info".parse()?
        }))
        .with_writer(std::io::stderr)
        .init();

    let config = config::load_config(&cli.config_dir)?;
    std::env::set_var("REVANCEX_CONFIG_DIR", &cli.config_dir);
    std::env::set_var("REVANCEX_TEMP_DIR", &config.build.temp_dir);

    match cli.command {
        Commands::Apps { json } => {
            orchestrator::list_apps(&config, json).await?;
        }

        Commands::Build {
            apps,
            arch,
            mode,
            output,
            force,
            keep_going,
            report,
        } => {
            utils::check_java_version(config.build.java_min_version)?;
            let output_dir = if output != "./output" {
                output
            } else {
                config.build.output_dir.clone()
            };
            let summary = orchestrator::build_apps(
                &config,
                &apps,
                &arch,
                mode.as_deref(),
                &output_dir,
                force,
            )
            .await?;

            let report_path = report.unwrap_or_else(|| format!("{output_dir}/BUILD_REPORT.json"));
            utils::write_build_report(&summary, &report_path)?;

            let has_failures = summary.failed.iter().any(|(id, _)| {
                !config
                    .apps
                    .get(id)
                    .map(|a| a.allow_build_failure)
                    .unwrap_or(false)
            }) || summary.reports.iter().any(|(id, r)| {
                r.status != "success"
                    && !config
                        .apps
                        .get(id)
                        .map(|a| a.allow_build_failure)
                        .unwrap_or(false)
            });

            if has_failures {
                for (id, err) in &summary.failed {
                    eprintln!("FAILED: {id}: {err}");
                }
                for (id, r) in &summary.reports {
                    if r.status != "success" && !summary.failed.iter().any(|(fid, _)| fid == id) {
                        eprintln!(
                            "FAILED: {id}: {}",
                            r.error.as_deref().unwrap_or("status is not success")
                        );
                    }
                }
                if !keep_going {
                    std::process::exit(1);
                }
            }
        }

        Commands::Module {
            single,
            bundle,
            apps,
            include_webui,
            output,
        } => {
            let config = config::sync_and_reload_modules(&cli.config_dir, config)?;
            if single {
                module::single::build_single_module(&config, &apps, &output).await?;
            }
            if bundle {
                module::bundle::build_bundle_module(&config, &apps, include_webui, &output).await?;
            }
        }

        Commands::CheckUpdates { json } => {
            orchestrator::check_updates(&config, json).await?;
        }

        Commands::GenerateMatrix { apps, arch, mode } => {
            orchestrator::generate_matrix(&config, &apps, &arch, &mode).await?;
        }

        Commands::Validate {
            strict,
            online,
            apps,
        } => {
            config::validate_yaml_duplicates(&cli.config_dir)?;
            let config = config::sync_and_reload_modules(&cli.config_dir, config)?;
            config::validate_config(&config, strict)?;
            let target_apps = apps
                .as_deref()
                .map(|a| orchestrator::resolver::resolve_targets(&config, a))
                .transpose()?;
            let warnings =
                patcher::metadata::validate_patch_compatibilities(&config, target_apps.as_deref())
                    .await;
            for w in &warnings {
                eprintln!("Warning: {w}");
            }
            if strict && !warnings.is_empty() {
                anyhow::bail!("Strict validation failed: {} warning(s)", warnings.len());
            }
            if online {
                let client = utils::make_browser_client(&config)?;
                let mut failures = Vec::new();
                for (id, app) in &config.apps {
                    if !app.enabled {
                        continue;
                    }
                    if let Some(ref targets) = target_apps {
                        if !targets.iter().any(|t| t == id) {
                            continue;
                        }
                    }
                    if let Some(ref url) = app.uptodown_url {
                        println!("Validating uptodown_url for {id}: {url}");
                        match client.head(url).send().await {
                            Ok(resp) => {
                                let st = resp.status();
                                if st == reqwest::StatusCode::NOT_FOUND
                                    || st == reqwest::StatusCode::GONE
                                {
                                    failures.push(format!(
                                        "App '{id}' uptodown_url {url} returned {st}"
                                    ));
                                }
                            }
                            Err(e) => {
                                failures.push(format!(
                                    "App '{id}' uptodown_url {url} request failed: {e}"
                                ));
                            }
                        }
                    }
                    if let Some(ref url) = app.apkmirror_url {
                        println!("Validating apkmirror_url for {id}: {url}");
                        match client.head(url).send().await {
                            Ok(resp) => {
                                let st = resp.status();
                                if st == reqwest::StatusCode::NOT_FOUND
                                    || st == reqwest::StatusCode::GONE
                                {
                                    failures.push(format!(
                                        "App '{id}' apkmirror_url {url} returned {st}"
                                    ));
                                }
                            }
                            Err(e) => {
                                failures.push(format!(
                                    "App '{id}' apkmirror_url {url} request failed: {e}"
                                ));
                            }
                        }
                    }
                }
                if !failures.is_empty() {
                    for f in &failures {
                        eprintln!("Online validation error: {f}");
                    }
                    anyhow::bail!(
                        "Online URL validation failed with {} error(s)",
                        failures.len()
                    );
                }
                println!("Online URL validation passed.");
            }
        }

        Commands::Clean { all, stale_tools } => {
            if stale_tools {
                utils::clean_stale_tools(&config)?;
            } else {
                utils::cache::clean_cache(&config, all)?;
            }
        }

        Commands::DownloadTools { apps } => {
            utils::check_java_version(config.build.java_min_version)?;
            let targets = orchestrator::resolver::resolve_targets(&config, &apps)?;
            let summary = utils::download_tools(&config, Some(&targets)).await?;
            if summary.tools_changed {
                if let Ok(path) = std::env::var("GITHUB_OUTPUT") {
                    if let Ok(mut file) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                    {
                        use std::io::Write;
                        let _ = writeln!(file, "tools_changed=true");
                    }
                }
            }
        }

        Commands::GenAppsJson { registry } => {
            let json = pages::generate_apps_json_with_registry(&config, Some(&registry))?;
            std::fs::create_dir_all("docs")?;
            std::fs::write("docs/apps.json", &json)?;
            println!("docs/apps.json written ({} bytes)", json.len());
        }

        Commands::FetchApk {
            app,
            arch,
            dry_run,
            force,
        } => {
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
            let path = builder::fetch_apk(&config, &app, app_cfg, &arch, force, None, None)
                .await?
                .path;
            let size = std::fs::metadata(&path)?.len();
            println!("SUCCESS: {app} downloaded to {path} ({size} bytes)");
        }

        Commands::VerifyModule { zip } => {
            module::verify_bundle(&zip)?;
            println!("SUCCESS: Magisk/KernelSU module {zip} is valid and ready for installation!");
        }

        Commands::Keystore { info } => {
            if info {
                utils::keystore::print_info(&config).await?;
            } else {
                println!("Use --info to print keystore details.");
            }
        }

        Commands::Patches { app, mode, json } => {
            let app_cfg = config
                .apps
                .get(&app)
                .ok_or_else(|| anyhow::anyhow!("Unknown app: {app}"))?;
            let is_root = mode.eq_ignore_ascii_case("root");
            let mpp_path = patcher::metadata::resolve_mpp_path(&config, app_cfg);
            let meta = if let Some(ref p) = mpp_path {
                patcher::metadata::load(&config, p).await.ok()
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
        }

        Commands::Test {
            apps,
            output,
            require_device,
            startup_grace_secs,
            install_timeout_secs,
            json,
            continue_on_error,
            fail_threshold,
        } => {
            let report = testing::run_device_tests(
                &apps,
                &output,
                require_device,
                startup_grace_secs,
                install_timeout_secs,
            )
            .await?;

            let filter_report = testing::filter_release_artifacts(&output, &report)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Device Smoke Tests: {} passed, {} failed, {} skipped",
                    report.passed, report.failed, report.skipped
                );
                for item in &report.items {
                    let reason = item
                        .reason
                        .as_deref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default();
                    let strat = item
                        .version_strategy
                        .as_deref()
                        .map(|s| format!(" [strategy: {s}]"))
                        .unwrap_or_default();
                    println!("  [{}] {}{}{}", item.status, item.app, reason, strat);
                }

                if !filter_report.excluded_apps.is_empty() {
                    println!("\nExcluded from release:");
                    for excl in &filter_report.excluded_apps {
                        println!("  - {}: {}", excl.app, excl.reason);
                    }
                }
            }

            let total = report.passed + report.failed + report.skipped;
            if testing::should_exit_failure(report.failed, total, continue_on_error, fail_threshold)
            {
                std::process::exit(1);
            }
        }

        Commands::Doctor => {
            utils::doctor::run_doctor(&config).await?;
        }

        Commands::Sources { sub } => match sub {
            SourcesSubcommand::Probe { app, arch, version } => {
                let app_cfg = config
                    .apps
                    .get(&app)
                    .ok_or_else(|| anyhow::anyhow!("App '{app}' not found in configuration"))?;

                let client = utils::make_browser_client(&config)?;
                let ctx = revancex::sources::SourceCtx::new(&client, &config, 5);
                let registry = revancex::sources::default_registry();
                let ordered = revancex::sources::source_order(&registry, app_cfg, &config);

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
                                .and_then(|u| u.host_str().map(|h| h.to_string()))
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
                        Ok(None) => {
                            println!(
                                "{:<14} | {:<8} | {:<16} | {:<25} | {:<6} | not found",
                                src.name(),
                                "miss",
                                "-",
                                "-",
                                elapsed_ms
                            );
                        }
                        Err(e) => {
                            println!(
                                "{:<14} | {:<8} | {:<16} | {:<25} | {:<6} | {e}",
                                src.name(),
                                "error",
                                "-",
                                "-",
                                elapsed_ms
                            );
                        }
                    }
                }

                if !any_resolved {
                    std::process::exit(1);
                }
            }
        },

        Commands::Report { sub } => match sub {
            ReportSubcommand::Check {
                path,
                fail_threshold,
                r#continue,
            } => {
                if !path.exists() {
                    anyhow::bail!("Build report not found: {}", path.display());
                }
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;

                let (schema_version, apps, summary) =
                    if let Ok(br) = serde_json::from_str::<report::BuildReport>(&content) {
                        (Some(br.schema_version), br.apps, Some(br.summary))
                    } else if let Ok(apps_map) = serde_json::from_str::<
                        std::collections::BTreeMap<String, report::AppBuildReport>,
                    >(&content)
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
                    eprintln!(
                        "::warning::Build report contains {} failed or unallowed app(s) ({:.1}% of total): {}",
                        bad.len(),
                        fail_pct,
                        bad.join(", ")
                    );

                    let should_fail = if r#continue {
                        false
                    } else if let Some(threshold) = fail_threshold {
                        fail_pct > (threshold as f64)
                    } else {
                        true
                    };

                    if should_fail {
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
                    println!(
                        "Build report (schema v{ver_str}) validated: {} total, {} success, {} failed, {} skipped.",
                        s.total, s.success, s.failed, s.skipped
                    );
                } else {
                    println!("Build report (schema v{ver_str}) validated: all apps evaluated within threshold.");
                }
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}
