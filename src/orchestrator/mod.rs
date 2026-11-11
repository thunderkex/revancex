pub mod discovery;
pub mod resolver;
pub mod scheduler;

use crate::config::Config;
use crate::ui;
use crate::utils;
use anyhow::Result;
use tracing::info;

pub async fn list_apps(cfg: &Config, json: bool) -> Result<()> {
    if json {
        let list: Vec<_> = cfg
            .apps
            .iter()
            .map(|(id, a)| {
                serde_json::json!({
                    "id": id,
                    "enabled": a.enabled,
                    "package": a.package,
                    "patch_source": a.patch_source,
                    "mode": a.mode,
                    "architectures": a.architectures,
                    "description": a.description
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
    } else {
        for (id, app) in &cfg.apps {
            ui::line(&format!(
                "{:24} enabled={:5} pkg={}",
                id, app.enabled, app.package
            ));
        }
    }
    Ok(())
}

pub use scheduler::BuildSummary;

pub async fn build_apps(
    cfg: &Config,
    apps_input: &str,
    arch: &str,
    mode: Option<&str>,
    output_dir: &str,
    force: bool,
) -> Result<BuildSummary> {
    std::fs::create_dir_all(output_dir)?;
    std::fs::create_dir_all(&cfg.build.temp_dir)?;
    std::fs::create_dir_all(&cfg.build.tools_dir)?;
    std::fs::create_dir_all(&cfg.build.keys_dir)?;

    let targets = resolver::resolve_targets(cfg, apps_input)?;
    ui::banner(&[
        ("apps", format!("{} target(s)", targets.len())),
        ("arch", arch.to_string()),
        ("mode", mode.unwrap_or("per-app").to_string()),
        (
            "config",
            format!(
                "workers={} retries={} force={force}",
                cfg.build.workers, cfg.build.download_retries
            ),
        ),
        ("output", output_dir.to_string()),
        (
            "logs",
            format!(
                "{}{}",
                ui::log_dir().display(),
                if ui::is_verbose() { " (verbose)" } else { "" }
            ),
        ),
    ]);
    info!("targets: {:?}", targets);
    let shared_net = std::sync::Arc::new(crate::net::SharedNet::new(cfg)?);
    shared_net.fetcher.ensure_probed().await?;
    ui::phase("Fetch");
    let tools_summary =
        utils::download_tools(cfg, Some(&targets), &shared_net.github_client).await?;
    ui::phase("Build (resolve → patch → sign → package)");

    let summary = scheduler::run_pipeline(
        cfg,
        &targets,
        arch,
        mode,
        force,
        output_dir,
        Some(&tools_summary.patch_results),
        shared_net,
    )
    .await?;
    ui::end_phase();
    info!(
        "Build complete. {} built, {} failed.",
        summary.built.len(),
        summary.failed.len()
    );
    Ok(summary)
}

/// Print the final one-block summary for a finished build.
pub fn print_summary(cfg: &Config, summary: &BuildSummary) {
    let mut rows = Vec::new();
    for (id, rep) in &summary.reports {
        let (status, fail) = match rep.status {
            crate::report::BuildStatus::Success => (ui::Status::Ok, Some(0)),
            crate::report::BuildStatus::Degraded | crate::report::BuildStatus::SuccessUnpatched => {
                (ui::Status::Warn, Some(0))
            }
            crate::report::BuildStatus::Skipped => (ui::Status::Skip, None),
            crate::report::BuildStatus::Failed => (ui::Status::Fail, None),
        };
        let name = cfg
            .apps
            .get(id)
            .map(|a| a.display_name(id))
            .unwrap_or_else(|| id.clone());
        let size = rep
            .output_path
            .as_deref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len());
        rows.push(ui::Row {
            app: name,
            version: rep
                .resolved_version
                .clone()
                .or_else(|| rep.fetched_version.clone())
                .or_else(|| rep.target_version.clone())
                .unwrap_or_default(),
            ok: rep.patches_included.len(),
            skip: rep.patches_auto_disabled.len(),
            fail,
            size,
            dur: ui::duration_of(id),
            status,
        });
    }
    for id in &summary.open_breakers {
        ui::warn_once(&format!("circuit breaker open for source '{id}'"));
    }
    ui::summary(&rows, &summary.built);
}

pub async fn check_updates(cfg: &Config, json: bool) -> Result<()> {
    discovery::check_updates(cfg, json).await
}

pub async fn generate_matrix(cfg: &Config, apps_input: &str, arch: &str, mode: &str) -> Result<()> {
    let targets = resolver::resolve_targets(cfg, apps_input)?;
    let archs: Vec<&str> = if arch == "all" {
        vec!["arm64-v8a", "armeabi-v7a", "x86_64"]
    } else {
        vec![arch]
    };

    let mut entries = Vec::new();
    for app in targets {
        for a in &archs {
            entries.push(serde_json::json!({
                "app": app,
                "arch": a,
                "mode": mode,
            }));
        }
    }

    let matrix = serde_json::json!({ "include": entries });
    println!("{}", serde_json::to_string(&matrix)?);
    Ok(())
}
