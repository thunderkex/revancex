use crate::config::Config;
use anyhow::Result;
use tracing::info;

async fn build_single_app(
    cfg: &Config,
    id: &str,
    output_dir: &str,
    build_opts: &super::ModuleBuildOptions,
) -> Result<()> {
    if id.eq_ignore_ascii_case("microg") {
        return Ok(());
    }

    let resolved_id = match cfg.resolve_app_id(id) {
        Some(canonical) => canonical,
        None => return Ok(()),
    };
    let (id, app_cfg) = match cfg.apps.get(resolved_id) {
        Some(app) => (resolved_id, app),
        None => return Ok(()),
    };

    if !app_cfg.enabled {
        info!("Skipping single module for {id}: app is disabled");
        return Ok(());
    }
    if !app_cfg.module.single {
        info!("Skipping single module for {id}: module.single is disabled");
        return Ok(());
    }

    let resolved_dir = if std::path::Path::new(output_dir).exists() {
        output_dir.to_string()
    } else {
        cfg.build.output_dir.clone()
    };

    let manifest_path = format!("{resolved_dir}/BUILD_REPORT.json");
    let mut payload_path: Option<String> = None;
    let mut detected_version: Option<String> = None;

    if let Ok(manifest_content) = std::fs::read_to_string(&manifest_path) {
        if let Ok(manifest) = serde_json::from_str::<crate::report::BuildReport>(&manifest_content)
        {
            if let Some(app_rep) = manifest.apps.get(id) {
                detected_version = app_rep
                    .resolved_version
                    .clone()
                    .or_else(|| app_rep.fetched_version.clone());
                if let Some(ref root_p) = app_rep.root_payload_path {
                    if std::path::Path::new(root_p).exists() {
                        payload_path = Some(root_p.clone());
                    }
                }
                if payload_path.is_none() {
                    if let Some(ref out_p) = app_rep.output_path {
                        if std::path::Path::new(out_p).exists() {
                            payload_path = Some(out_p.clone());
                        }
                    }
                }
            }
        }
    }

    if payload_path.is_none() {
        if let Ok(entries) = std::fs::read_dir(&resolved_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("apk") {
                    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if cfg.resolve_app_id(stem) == Some(id) {
                        payload_path = Some(p.to_string_lossy().to_string());
                        break;
                    }
                }
            }
        }
    }

    if payload_path.is_none()
        && (resolved_dir == cfg.build.output_dir || !std::path::Path::new(&resolved_dir).exists())
    {
        let temp_root = format!("{}/{id}-root-payload.apk", cfg.build.temp_dir);
        if std::path::Path::new(&temp_root).exists() {
            payload_path = Some(temp_root);
        }
    }

    let found = payload_path.ok_or_else(|| {
        anyhow::anyhow!("No APK payload found for app '{id}' in '{resolved_dir}'")
    })?;

    let version = detected_version
        .or_else(|| crate::utils::apk::get_apk_version_name(&found).ok())
        .or_else(|| app_cfg.version.clone())
        .unwrap_or_else(|| "1.0.0".to_string());

    let mod_id = format!("revancex-{id}");
    let clean_name = app_cfg.display_name(id);
    let mod_name = format!("ReVanceX - {clean_name}");
    let zip_name = crate::patcher::name::output_filename(
        &clean_name,
        &version,
        crate::patcher::name::ArtifactKind::Module,
    );

    super::bundle::build_custom_module_with_options(
        cfg,
        &found,
        build_opts.include_webui,
        output_dir,
        Some(&mod_id),
        Some(&mod_name),
        Some(&zip_name),
        build_opts,
    )
    .await
}

#[cfg(test)]
pub async fn build_single_module(cfg: &Config, apps_input: &str, output_dir: &str) -> Result<()> {
    build_single_module_with_options(
        cfg,
        apps_input,
        output_dir,
        &super::ModuleBuildOptions::default(),
    )
    .await
}

pub async fn build_single_module_with_options(
    cfg: &Config,
    apps_input: &str,
    output_dir: &str,
    build_opts: &super::ModuleBuildOptions,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    if apps_input == "all" || apps_input == "all-enabled" {
        let mut app_ids: Vec<_> = cfg.apps.keys().cloned().collect();
        app_ids.sort();
        for id in app_ids {
            if let Some(app) = cfg.apps.get(&id) {
                if !app.enabled || id.eq_ignore_ascii_case("microg") || !app.module.single {
                    continue;
                }
                info!("Building single module for {id} in {output_dir}...");
                if let Err(e) = build_single_app(cfg, &id, output_dir, build_opts).await {
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
            if let Some(app) = cfg.apps.get(id) {
                if !app.enabled {
                    info!("Skipping single module for {id}: app is disabled");
                    continue;
                }
                if !app.module.single {
                    info!("Skipping single module for {id}: module.single is disabled");
                    continue;
                }
            }
            info!("Building single module for {id} in {output_dir}...");
            build_single_app(cfg, id, output_dir, build_opts).await?;
        }
        Ok(())
    }
}
