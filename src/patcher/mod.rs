pub mod cli;
pub mod metadata;
pub mod name;
pub mod plan;

use crate::config::apps::HookWhen;
use crate::config::{AppConfig, Config};
use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

pub async fn patch(
    cfg: &Config,
    id: &str,
    app: &AppConfig,
    apk_path: &str,
    output_dir: &str,
    is_root: bool,
) -> Result<String> {
    let ks = crate::utils::keystore::resolve(cfg, app).await?;
    if ks.generated {
        tracing::warn!("{id}: generated new keystore at {} (first run)", ks.file);
    };

    let repo = cfg
        .patch_sources
        .get(&app.patch_source)
        .map(|p| p.repo.clone())
        .unwrap_or_else(|| app.patch_source.clone());

    let suffix = if is_root && app.mode == "both" {
        "-root.apk"
    } else {
        "-patched.apk"
    };
    let output_apk = format!("{output_dir}/{id}{suffix}");

    let mpp_path = metadata::resolve_mpp_path(cfg, app);
    let meta = if let Some(ref p) = mpp_path {
        metadata::load(cfg, p).await.ok()
    } else {
        None
    };
    let plan = plan::resolve(app, is_root, meta.as_ref(), None, &cfg.build.patcher.rules);

    let args = cli::build_args(
        &cfg.build.tools_dir,
        &repo,
        app,
        apk_path,
        &output_apk,
        &plan,
    );

    let mut cmd = tokio::process::Command::new("java");
    cmd.args(["-jar", &args.cli_jar, "patch"])
        .arg(format!("--patches={}", args.patches_arg))
        .arg("--continue-on-error")
        .arg("-o")
        .arg(&args.output_apk);

    if !args.included.is_empty() {
        cmd.arg("--exclusive");
    }

    if let Some((file, alias, pass)) = &args.keystore {
        cmd.arg(format!("--keystore={file}"))
            .arg(format!("--keystore-entry-alias={alias}"))
            .arg(format!("--keystore-password={pass}"))
            .arg(format!("--keystore-entry-password={pass}"));
    } else {
        cmd.arg(format!("--keystore={}", ks.file))
            .arg(format!("--keystore-entry-alias={}", ks.alias))
            .arg(format!("--keystore-password={}", ks.password))
            .arg(format!("--keystore-entry-password={}", ks.password));
    }

    for patch in &args.included {
        cmd.args(["-e", patch]);
    }
    for patch in &args.excluded {
        cmd.args(["-d", patch]);
    }
    for raw in &args.raw_args {
        cmd.arg(raw);
    }

    cmd.arg(&args.input_apk);

    info!("{id}: patching with morphe-cli...");
    let status = cmd
        .status()
        .await
        .context("java not found — install JDK 21+")?;
    if !status.success() {
        anyhow::bail!("{id}: morphe-cli exited with {status}");
    }

    info!("{id}: patched → {output_apk}");

    for hook in &app.post_patch_hooks {
        let should_run = match hook.when {
            HookWhen::Root => is_root,
            HookWhen::Nonroot => !is_root,
            HookWhen::Always => true,
        };
        if !should_run {
            continue;
        }
        if !Path::new(&hook.script).exists() {
            if hook.optional {
                tracing::warn!("{id}: optional hook script not found: {}", hook.script);
                continue;
            } else {
                anyhow::bail!("{id}: declared hook script not found: {}", hook.script);
            }
        }
        info!("{id}: running post-patch hook: {}", hook.script);
        let status = tokio::process::Command::new("python")
            .arg(&hook.script)
            .arg(&output_apk)
            .arg(&ks.file)
            .arg(&ks.alias)
            .arg(&ks.password)
            .status()
            .await;
        match status {
            Ok(st) if st.success() => {
                info!("{id}: hook {} succeeded", hook.script);
            }
            Ok(st) => {
                if hook.optional {
                    tracing::warn!("{id}: optional hook {} exited with {st}", hook.script);
                } else {
                    anyhow::bail!("{id}: hook {} exited with {st}", hook.script);
                }
            }
            Err(e) => {
                if hook.optional {
                    tracing::warn!("{id}: optional hook {} failed to launch: {e}", hook.script);
                } else {
                    anyhow::bail!("{id}: hook {} failed to launch: {e}", hook.script);
                }
            }
        }
    }

    Ok(output_apk)
}
