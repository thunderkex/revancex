use anyhow::Result;
use clap::{Parser, Subcommand};
use revancex::{builder, config, module, orchestrator, pages, patcher, testing, utils};
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

    #[arg(short, long, global = true, default_value = "./config")]
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
    },

    Clean {
        #[arg(long)]
        all: bool,
    },

    DownloadTools,

    GenAppsJson,

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
    },

    Doctor,
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

            let report_path = report.unwrap_or_else(|| "BUILD_REPORT.json".to_string());
            utils::write_build_report(&summary, &report_path)?;

            if !summary.failed.is_empty() {
                for (id, err) in &summary.failed {
                    eprintln!("FAILED: {id}: {err}");
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

        Commands::Validate { strict } => {
            config::validate_yaml_duplicates(&cli.config_dir)?;
            let config = config::sync_and_reload_modules(&cli.config_dir, config)?;
            config::validate_config(&config, strict)?;
            let warnings = patcher::metadata::validate_patch_compatibilities(&config).await;
            for w in warnings {
                eprintln!("Warning: {w}");
            }
        }

        Commands::Clean { all } => {
            utils::cache::clean_cache(&config, all)?;
        }

        Commands::DownloadTools => {
            utils::check_java_version(config.build.java_min_version)?;
            utils::download_tools(&config, None).await?;
        }

        Commands::GenAppsJson => {
            let json = pages::generate_apps_json(&config)?;
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
            let path = builder::fetch_apk(&config, &app, app_cfg, &arch, force, None).await?;
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
        } => {
            let report = testing::run_device_tests(
                &apps,
                &output,
                require_device,
                startup_grace_secs,
                install_timeout_secs,
            )
            .await?;

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
                    println!("  [{}] {}{}", item.status, item.app, reason);
                }
            }

            if report.failed > 0 {
                std::process::exit(1);
            }
        }

        Commands::Doctor => {
            utils::doctor::run_doctor(&config).await?;
        }
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
