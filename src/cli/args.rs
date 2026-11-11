use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "revancex",
    version,
    about = "Modern patcher with dynamic configuration",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[arg(
        short,
        long,
        alias = "config",
        global = true,
        default_value = "./config"
    )]
    pub config_dir: String,
}

#[derive(Subcommand)]
pub enum Commands {
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

        #[arg(long, conflicts_with = "include_webui")]
        no_webui: bool,

        #[arg(short, long, default_value = "./modules")]
        output: String,

        #[arg(long)]
        version_code: Option<i32>,

        #[arg(long)]
        release_tag: Option<String>,
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
        #[arg(long)]
        report: Option<String>,
        #[arg(long)]
        test_report: Option<String>,
        #[arg(long)]
        output_dir: Option<String>,
        #[arg(long)]
        release_tag: Option<String>,
        #[arg(long, default_value = "docs/apps.json")]
        out: String,
        #[arg(long, default_value = "docs/update")]
        update_dir: String,
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
        #[arg(long)]
        report: Option<String>,
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
pub enum ReportSubcommand {
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
pub enum SourcesSubcommand {
    Probe {
        app: String,
        #[arg(long, default_value = "arm64-v8a")]
        arch: String,
        #[arg(long)]
        version: Option<String>,
    },
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
