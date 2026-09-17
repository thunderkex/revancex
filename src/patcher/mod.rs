pub mod cli;
pub mod metadata;
pub mod name;
pub mod plan;

use crate::config::apps::HookWhen;
use crate::config::{AppConfig, Config};
use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

pub fn parse_applied_patch_count(stdout: &str) -> Option<usize> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Applied ") {
            if let Some(num_str) = rest.split_whitespace().next() {
                if let Ok(n) = num_str.parse::<usize>() {
                    if rest.contains("patch") {
                        return Some(n);
                    }
                }
            }
        }
    }
    let mut count = 0;
    let mut found_individual = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if (trimmed.starts_with("Applied ") || trimmed.starts_with("Applying "))
            && !trimmed.contains(" patches")
            && !trimmed.contains(" patch")
        {
            count += 1;
            found_individual = true;
        }
    }
    if found_individual {
        return Some(count);
    }
    None
}

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
    let apk_version = crate::utils::apk::get_apk_version_name(apk_path).ok();
    if let Some(ref ver) = apk_version {
        info!("{id}: parsed actual APK version {ver} from {apk_path}");
    }
    let plan = plan::resolve(
        app,
        is_root,
        meta.as_ref(),
        apk_version.as_deref(),
        &cfg.build.patcher.rules,
    );
    if !plan.auto_disabled.is_empty() {
        info!(
            "{id}: {} patch(es) auto-disabled due to version/rules",
            plan.auto_disabled.len()
        );
    }

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
    let output = cmd
        .output()
        .await
        .context("java not found — install JDK 21+")?;
    if !output.status.success() {
        anyhow::bail!("{id}: morphe-cli exited with {}", output.status);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        print!("{stdout}");
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }

    let dex_changed = match (
        crate::utils::apk::dex_entries_crc(apk_path),
        crate::utils::apk::dex_entries_crc(&output_apk),
    ) {
        (Ok(in_crc), Ok(out_crc)) => in_crc != out_crc,
        _ => true,
    };

    let zero_applied = if let Some(count) = parse_applied_patch_count(&stdout) {
        count == 0
    } else {
        !dex_changed
    };

    if zero_applied {
        let ver_info = apk_version
            .as_deref()
            .map(|v| format!(" — version {v} not covered by any enabled patch"))
            .unwrap_or_default();
        let plan_count = plan.included.len();
        anyhow::bail!("{id}: 0/{plan_count} patches applied{ver_info}");
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
