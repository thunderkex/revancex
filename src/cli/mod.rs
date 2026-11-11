mod args;
mod handlers;

use crate::{config, orchestrator, ui, utils};
use anyhow::Result;
use args::{Cli, Commands, ReportSubcommand, SourcesSubcommand};
use clap::Parser;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub async fn main() {
    let cli = Cli::parse();
    ui::init(cli.verbose);
    tracing_subscriber::registry().with(ui::UiLayer).init();

    match run(cli).await {
        Ok(()) => ui::flush(),
        Err(err) => {
            ui::fatal(&err);
            ui::exit(1);
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let config = config::load_config(&cli.config_dir)?;

    match cli.command {
        Commands::Apps { json } => orchestrator::list_apps(&config, json).await,

        Commands::Build {
            apps,
            arch,
            mode,
            output,
            force,
            keep_going,
            report,
        } => {
            handlers::handle_build(
                &config,
                handlers::BuildCmdArgs {
                    apps,
                    arch,
                    mode,
                    output,
                    force,
                    keep_going,
                    report,
                },
            )
            .await
        }

        Commands::Module {
            single,
            bundle,
            apps,
            include_webui,
            no_webui,
            output,
            version_code,
            release_tag,
        } => {
            handlers::handle_module(
                &config,
                single,
                bundle,
                apps,
                include_webui,
                no_webui,
                output,
                version_code,
                release_tag,
            )
            .await
        }

        Commands::CheckUpdates { json } => orchestrator::check_updates(&config, json).await,

        Commands::GenerateMatrix { apps, arch, mode } => {
            orchestrator::generate_matrix(&config, &apps, &arch, &mode).await
        }

        Commands::Validate {
            strict,
            online,
            apps,
        } => handlers::handle_validate(&config, strict, online, apps).await,

        Commands::Clean { all, stale_tools } => {
            if stale_tools {
                utils::clean_stale_tools(&config)
            } else {
                utils::cache::clean_cache(&config, all)
            }
        }

        Commands::DownloadTools { apps } => handlers::handle_download_tools(&config, apps).await,

        Commands::GenAppsJson {
            registry,
            report,
            test_report,
            output_dir,
            release_tag,
            out,
            update_dir,
        } => handlers::handle_gen_apps_json(
            &config,
            crate::pages::GenAppsJsonOptions {
                registry_path: Some(registry),
                build_report_path: report,
                test_report_path: test_report,
                output_dir,
                release_tag,
            },
            out,
            update_dir,
        ),

        Commands::FetchApk {
            app,
            arch,
            dry_run,
            force,
        } => handlers::handle_fetch_apk(&config, app, arch, dry_run, force).await,

        Commands::VerifyModule { zip } => {
            crate::module::verify_bundle(&zip)?;
            println!("SUCCESS: Magisk/KernelSU module {zip} is valid and ready for installation!");
            Ok(())
        }

        Commands::Keystore { info } => {
            if info {
                utils::keystore::print_info(&config).await
            } else {
                println!("Use --info to print keystore details.");
                Ok(())
            }
        }

        Commands::Patches { app, mode, json } => {
            handlers::handle_patches(&config, app, mode, json).await
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
            report,
        } => {
            handlers::handle_test(
                &config,
                apps,
                output,
                require_device,
                startup_grace_secs,
                install_timeout_secs,
                json,
                continue_on_error,
                fail_threshold,
                report,
            )
            .await
        }

        Commands::Doctor => utils::doctor::run_doctor(&config).await,

        Commands::Sources { sub } => match sub {
            SourcesSubcommand::Probe { app, arch, version } => {
                handlers::handle_sources_probe(&config, app, arch, version).await
            }
        },

        Commands::Report { sub } => match sub {
            ReportSubcommand::Check {
                path,
                fail_threshold,
                r#continue,
            } => handlers::handle_report_check(&config, path, fail_threshold, r#continue),
        },
    }
}
