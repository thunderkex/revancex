use crate::config::Config;
use anyhow::Result;
use tracing::info;

async fn build_single_app(cfg: &Config, id: &str, output_dir: &str) -> Result<()> {
    if id.eq_ignore_ascii_case("microg") {
        return Ok(());
    }

    let resolved_dir = if std::path::Path::new(output_dir).exists() {
        output_dir.to_string()
    } else {
        cfg.build.output_dir.clone()
    };

    let mut selected_apk: Option<(String, u8)> = None;

    if let Ok(entries) = std::fs::read_dir(&resolved_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".apk") {
                    let stem = name.trim_end_matches(".apk");
                    let is_root = stem.ends_with("-root");
                    let is_patched = stem.ends_with("-patched");
                    let candidate_id = stem.trim_end_matches("-root").trim_end_matches("-patched");
                    if candidate_id == id {
                        let prio = if is_root {
                            2
                        } else if is_patched {
                            1
                        } else {
                            0
                        };
                        match &selected_apk {
                            Some((_, prev_prio)) => {
                                if prio > *prev_prio {
                                    selected_apk = Some((p.to_string_lossy().to_string(), prio));
                                }
                            }
                            None => {
                                selected_apk = Some((p.to_string_lossy().to_string(), prio));
                            }
                        }
                    }
                }
            }
        }
    }

    let found = selected_apk
        .map(|(path, _)| path)
        .ok_or_else(|| anyhow::anyhow!("No APK found for app '{id}' in '{resolved_dir}'"))?;

    let mod_id = format!("revancex-{id}");
    let clean_name = cfg
        .apps
        .get(id)
        .map(|a| a.display_name(id))
        .unwrap_or_else(|| id.replace('_', " "));
    let mod_name = format!("ReVanceX - {clean_name}");
    let zip_name = format!("revancex-module-{id}.zip");

    super::bundle::build_custom_module(
        cfg,
        &found,
        true,
        output_dir,
        Some(&mod_id),
        Some(&mod_name),
        Some(&zip_name),
    )
    .await
}

pub async fn build_single_module(cfg: &Config, apps_input: &str, output_dir: &str) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    if apps_input == "all" || apps_input == "all-enabled" {
        let mut app_ids: Vec<_> = cfg.apps.keys().cloned().collect();
        app_ids.sort();
        for id in app_ids {
            if let Some(app) = cfg.apps.get(&id) {
                if !app.enabled || id.eq_ignore_ascii_case("microg") {
                    continue;
                }
                if let Some(m) = &app.module {
                    if !m.single {
                        continue;
                    }
                }
                info!("Building single module for {id} in {output_dir}...");
                if let Err(e) = build_single_app(cfg, &id, output_dir).await {
                    tracing::warn!("Failed building single module for {id}: {e}");
                }
            }
        }
        Ok(())
    } else {
        for id in apps_input
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if id.eq_ignore_ascii_case("microg") {
                continue;
            }
            info!("Building single module for {id} in {output_dir}...");
            build_single_app(cfg, id, output_dir).await?;
        }
        Ok(())
    }
}
