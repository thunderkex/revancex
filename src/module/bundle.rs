use crate::config::Config;
use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use tracing::info;
use zip::write::SimpleFileOptions;

fn bundle_display_name(cfg: &Config, key: &str) -> String {
    if let Some(module_def) = cfg.modules.get(key) {
        if let Some(ref n) = module_def.name {
            return n.clone();
        }
    }
    let title = key
        .split(['_', '-'])
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("ReVanceX {title} Bundle")
}

pub(crate) fn parse_apk_stem(stem: &str) -> (&str, u8) {
    if let Some(base) = stem.strip_suffix("-root-payload") {
        (base, 2)
    } else if let Some(base) = stem.strip_suffix("-root") {
        (base, 2)
    } else if let Some(base) = stem.strip_suffix("-patched") {
        (base, 1)
    } else {
        (stem, 0)
    }
}

#[cfg(test)]
pub async fn build_bundle_module(
    cfg: &Config,
    apps_or_paths: &str,
    include_webui: bool,
    output_dir: &str,
) -> Result<()> {
    build_bundle_module_with_options(
        cfg,
        apps_or_paths,
        include_webui,
        output_dir,
        &super::ModuleBuildOptions::default(),
    )
    .await
}

pub async fn build_bundle_module_with_options(
    cfg: &Config,
    apps_or_paths: &str,
    include_webui: bool,
    output_dir: &str,
    build_opts: &super::ModuleBuildOptions,
) -> Result<()> {
    let lower = apps_or_paths.to_lowercase().trim().to_string();

    if lower == "all" || lower == "all-enabled" || lower == "categories" {
        let mut sorted_keys: Vec<_> = cfg.modules.keys().cloned().collect();
        sorted_keys.sort();
        for key in sorted_keys {
            if let Some(module_def) = cfg.modules.get(&key) {
                let filtered_apps: Vec<String> = module_def
                    .apps
                    .iter()
                    .filter(|id| {
                        cfg.apps
                            .get(*id)
                            .is_some_and(|a| a.enabled && a.module.bundle)
                    })
                    .cloned()
                    .collect();
                if filtered_apps.is_empty() {
                    continue;
                }
                let module_apps = filtered_apps.join(",");
                let mod_id = cfg
                    .modules
                    .get(&key)
                    .and_then(|m| m.module_id.clone())
                    .unwrap_or_else(|| format!("revancex-{key}"));
                let mod_name = bundle_display_name(cfg, &key);
                let zip_filename = cfg
                    .modules
                    .get(&key)
                    .and_then(|m| m.zip_name.clone())
                    .unwrap_or_else(|| format!("revancex-bundle-{key}.zip"));
                build_custom_module_with_options(
                    cfg,
                    &module_apps,
                    include_webui,
                    output_dir,
                    Some(&mod_id),
                    Some(&mod_name),
                    Some(&zip_filename),
                    build_opts,
                )
                .await?;
            }
        }
        Ok(())
    } else if let Some(module_def) = cfg.modules.get(&lower) {
        let filtered_apps: Vec<String> = module_def
            .apps
            .iter()
            .filter(|id| {
                cfg.apps
                    .get(*id)
                    .is_some_and(|a| a.enabled && a.module.bundle)
            })
            .cloned()
            .collect();
        let module_apps = filtered_apps.join(",");
        let mod_id = module_def
            .module_id
            .clone()
            .unwrap_or_else(|| format!("revancex-{lower}"));
        let mod_name = bundle_display_name(cfg, &lower);
        let zip_filename = module_def
            .zip_name
            .clone()
            .unwrap_or_else(|| format!("revancex-bundle-{lower}.zip"));
        build_custom_module_with_options(
            cfg,
            &module_apps,
            include_webui,
            output_dir,
            Some(&mod_id),
            Some(&mod_name),
            Some(&zip_filename),
            build_opts,
        )
        .await
    } else {
        build_custom_module_with_options(
            cfg,
            apps_or_paths,
            include_webui,
            output_dir,
            None,
            None,
            None,
            build_opts,
        )
        .await
    }
}

fn insert_bundle_apk(
    apk_map: &mut std::collections::HashMap<String, (String, u8)>,
    app_id: &str,
    path: String,
    prio: u8,
) {
    match apk_map.get_mut(app_id) {
        Some((existing_path, prev_prio)) if prio > *prev_prio => {
            *existing_path = path;
            *prev_prio = prio;
        }
        Some(_) => {}
        None => {
            apk_map.insert(app_id.to_string(), (path, prio));
        }
    }
}

fn scan_output_dir_apks(
    cfg: &Config,
    resolved_dir: &str,
    filter_token: Option<&str>,
    apk_map: &mut std::collections::HashMap<String, (String, u8)>,
) {
    let Ok(entries) = std::fs::read_dir(resolved_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("apk") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.to_lowercase().contains("microg") {
            continue;
        }
        let stem = name.trim_end_matches(".apk");
        let (_, prio) = parse_apk_stem(stem);
        let Some(app_id) = cfg.resolve_app_id(stem) else {
            continue;
        };
        if filter_token.is_some_and(|t| t != app_id) {
            continue;
        }
        if cfg
            .apps
            .get(app_id)
            .is_some_and(|a| !a.enabled || !a.module.bundle)
        {
            continue;
        }
        insert_bundle_apk(apk_map, app_id, path.to_string_lossy().to_string(), prio);
    }
}

fn collect_apks_for_tokens(cfg: &Config, apps_or_paths: &str, resolved_dir: &str) -> Vec<String> {
    let mut apk_map = std::collections::HashMap::new();
    for token in apps_or_paths
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("microg"))
    {
        let p = Path::new(token);
        if p.exists() && p.is_file() {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".apk") && !name.to_lowercase().contains("microg") {
                    let stem = name.trim_end_matches(".apk");
                    let (_, prio) = parse_apk_stem(stem);
                    let app_id = cfg
                        .resolve_app_id(stem)
                        .unwrap_or_else(|| parse_apk_stem(stem).0);
                    if !cfg
                        .apps
                        .get(app_id)
                        .is_some_and(|a| !a.enabled || !a.module.bundle)
                    {
                        insert_bundle_apk(&mut apk_map, app_id, token.to_string(), prio);
                    }
                }
            }
        } else if token == "all" || token == "all-enabled" {
            for (app_id, app_cfg) in &cfg.apps {
                if app_cfg.enabled && app_cfg.module.bundle {
                    let temp_root = format!("{}/{app_id}-root-payload.apk", cfg.build.temp_dir);
                    if Path::new(&temp_root).exists() {
                        apk_map.insert(app_id.clone(), (temp_root, 2));
                    }
                }
            }
            scan_output_dir_apks(cfg, resolved_dir, None, &mut apk_map);
        } else {
            let temp_root = format!("{}/{token}-root-payload.apk", cfg.build.temp_dir);
            if Path::new(&temp_root).exists()
                && cfg
                    .apps
                    .get(token)
                    .is_some_and(|a| a.enabled && a.module.bundle)
            {
                apk_map.insert(token.to_string(), (temp_root, 2));
            }
            scan_output_dir_apks(cfg, resolved_dir, Some(token), &mut apk_map);
        }
    }
    apk_map.into_values().map(|(p, _)| p).collect()
}

fn select_apks_for_bundle(cfg: &Config, mut apks: Vec<String>) -> Result<Vec<String>> {
    apks.sort_by(|a, b| {
        let priority_of = |p: &str| -> u32 {
            let stem = Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(p);
            let fallback = parse_apk_stem(stem).0;
            let app_id = cfg.resolve_app_id(stem).unwrap_or(fallback);
            cfg.apps
                .get(app_id)
                .map(|a| a.bundle_priority)
                .unwrap_or(100)
        };
        priority_of(a).cmp(&priority_of(b)).then_with(|| a.cmp(b))
    });

    let max_bytes = cfg.build.bundle.max_bytes;
    let mut total_bundle_size: u64 = 0;
    let mut selected_apks = Vec::new();
    for apk in apks {
        let size = std::fs::metadata(&apk).map(|m| m.len()).unwrap_or(0);
        if total_bundle_size + size > max_bytes {
            tracing::warn!(
                "Bundle size limit (1.8 GiB) reached while adding '{}'. Skipping further APKs for this bundle.",
                apk
            );
            break;
        }
        total_bundle_size += size;
        selected_apks.push(apk);
    }
    resolve_conflicts_for_bundle(cfg, selected_apks)
}

struct ModuleHeaderSpec<'a> {
    mod_id: &'a str,
    mod_name: &'a str,
    version_code: i32,
    override_module_id: bool,
    release_tag: Option<&'a str>,
}

fn write_module_header_and_scripts(
    zip: &mut zip::ZipWriter<std::fs::File>,
    opts: SimpleFileOptions,
    cfg: &Config,
    spec: &ModuleHeaderSpec<'_>,
) -> Result<()> {
    zip.start_file("META-INF/com/google/android/update-binary", opts)?;
    zip.write_all(super::UPDATE_BINARY)?;
    zip.start_file("META-INF/com/google/android/updater-script", opts)?;
    zip.write_all(b"#MAGISK\n")?;

    let env_tag = std::env::var("REVANCEX_RELEASE_TAG")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let version = spec
        .release_tag
        .or(env_tag.as_deref())
        .map(String::from)
        .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));
    let default_name = cfg.module_meta.resolved_name();
    let description = if spec.mod_name == default_name
        || spec.mod_name == "ReVanceX"
        || spec.mod_name == "ReVanceX Bundle"
    {
        cfg.module_meta.resolved_description().to_string()
    } else {
        format!("Patched module for {} via ReVanceX", spec.mod_name)
    };
    let configured_update = cfg.module_meta.resolved_update_json();
    let update_json = if configured_update.is_empty() || spec.override_module_id {
        super::ota_update_json_url(spec.mod_id)
    } else {
        configured_update.to_string()
    };

    zip.start_file("module.prop", opts)?;
    let prop_str = super::render_module_prop(
        spec.mod_id,
        spec.mod_name,
        &version,
        spec.version_code,
        cfg.module_meta.resolved_author(),
        &description,
        cfg.module_meta.min_magisk,
        cfg.module_meta.min_kernelsu,
        &update_json,
    );
    zip.write_all(prop_str.as_bytes())?;

    let strip_crlf =
        |data: &[u8]| -> Vec<u8> { data.iter().copied().filter(|&b| b != b'\r').collect() };
    for (name, bytes) in [
        ("customize.sh", super::CUSTOMIZE_SH),
        ("utils.sh", super::UTILS_SH),
        ("service.sh", super::SERVICE_SH),
        ("action.sh", super::ACTION_SH),
        ("uninstall.sh", super::UNINSTALL_SH),
    ] {
        zip.start_file(name, opts)?;
        zip.write_all(&strip_crlf(bytes))?;
    }
    Ok(())
}

fn write_apps_metadata_and_webui(
    zip: &mut zip::ZipWriter<std::fs::File>,
    opts: SimpleFileOptions,
    cfg: &Config,
    apks: &[String],
    include_webui: bool,
) -> Result<()> {
    let mut apps_list_content = String::new();
    let mut apps_json_array = Vec::new();

    for apk in apks {
        let file_stem = Path::new(apk)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("app");
        let fallback = parse_apk_stem(file_stem).0;
        let app_id = cfg.resolve_app_id(file_stem).unwrap_or(fallback);

        let (default_pkg, mode, desc) = if let Some(app_cfg) = cfg.apps.get(app_id) {
            (
                app_cfg.package.clone(),
                app_cfg.mode.clone(),
                app_cfg.description.clone(),
            )
        } else {
            (app_id.to_string(), "patch".to_string(), None)
        };

        let pkg = match crate::utils::apk::get_apk_package_name(apk).ok() {
            Some(p) if p != default_pkg => {
                tracing::warn!(
                    "{app_id}: module APK package '{p}' differs from original '{default_pkg}'. Enforcing original package '{default_pkg}' for root module."
                );
                default_pkg
            }
            _ => default_pkg,
        };

        let patched_version: String = crate::utils::apk::get_apk_version_name(apk)
            .ok()
            .or_else(|| {
                cfg.apps
                    .get(app_id)
                    .and_then(|a| a.version.clone())
                    .filter(|v| !v.eq_ignore_ascii_case("auto"))
            })
            .unwrap_or_default()
            .chars()
            .filter(|c| !matches!(c, ':' | '\r' | '\n'))
            .collect();

        apps_list_content.push_str(&format!("{app_id}:{pkg}:{mode}:{patched_version}\n"));
        let clean_name = app_id
            .replace('_', " ")
            .replace("youtube", "YouTube")
            .replace("microg", "MicroG");
        apps_json_array.push(serde_json::json!({
            "id": app_id,
            "name": clean_name,
            "package": pkg,
            "mode": mode,
            "description": desc
        }));
    }

    zip.start_file("apps.list", opts)?;
    zip.write_all(apps_list_content.as_bytes())?;
    if include_webui {
        let json_str = serde_json::to_string_pretty(&apps_json_array).ok();
        super::webui::inject_webui(zip, opts, json_str.as_deref())?;
    }
    Ok(())
}

fn write_bundle_apk_payloads(
    zip: &mut zip::ZipWriter<std::fs::File>,
    opts: SimpleFileOptions,
    cfg: &Config,
    apks: &[String],
) -> Result<()> {
    for apk in apks {
        let name = Path::new(apk)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app.apk");
        let (app_dir, _) = parse_apk_stem(name.trim_end_matches(".apk"));

        let mut apk_file = std::fs::File::open(apk).with_context(|| format!("Opening {apk}"))?;
        let apk_entry = format!("apks/{app_dir}.apk");
        zip.start_file(&apk_entry, opts)?;
        std::io::copy(&mut apk_file, zip)?;

        let stock_file_path = std::fs::read_dir(&cfg.build.temp_dir)
            .ok()
            .and_then(|entries| {
                entries.flatten().find_map(|entry| {
                    let p = entry.path();
                    let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if fname.starts_with(app_dir) && fname.ends_with("-input.apk") {
                        Some(p)
                    } else {
                        None
                    }
                })
            });
        let fallback_candidates = [
            format!("{}/{app_dir}-all-input.apk", cfg.build.temp_dir),
            format!("{}/{app_dir}-input.apk", cfg.build.temp_dir),
            format!("tmp/{app_dir}-input.apk"),
            format!("{}/{app_dir}.apk", cfg.build.temp_dir),
        ];
        let chosen_stock = stock_file_path.or_else(|| {
            fallback_candidates
                .into_iter()
                .map(std::path::PathBuf::from)
                .find(|p| p.exists())
        });
        if let Some(sp) = chosen_stock {
            if let Ok(mut sf) = std::fs::File::open(sp) {
                let stock_entry = format!("stock/{app_dir}.apk");
                if zip.start_file(&stock_entry, opts).is_ok() {
                    crate::utils::warn_io(
                        std::io::copy(&mut sf, zip),
                        &format!("Writing stock APK {stock_entry} into bundle"),
                    );
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn build_custom_module_with_options(
    cfg: &Config,
    apps_or_paths: &str,
    include_webui: bool,
    output_dir: &str,
    module_id: Option<&str>,
    module_name: Option<&str>,
    zip_filename: Option<&str>,
    build_opts: &super::ModuleBuildOptions,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let resolved_dir = if Path::new(output_dir).exists() {
        output_dir.to_string()
    } else {
        cfg.build.output_dir.clone()
    };

    let raw_apks = collect_apks_for_tokens(cfg, apps_or_paths, &resolved_dir);
    let apks = select_apks_for_bundle(cfg, raw_apks)?;
    if apks.is_empty() {
        tracing::warn!("No APKs found to bundle for '{apps_or_paths}' in '{output_dir}', skipping bundle creation.");
        return Ok(());
    }

    let version_code = super::resolve_module_version_code(
        build_opts.version_code,
        Some("config/releases_registry.json"),
    );
    let mod_id = module_id.unwrap_or_else(|| cfg.module_meta.resolved_id());
    let mod_name = module_name.unwrap_or_else(|| cfg.module_meta.resolved_name());
    let actual_zip_name = zip_filename.unwrap_or("revancex-bundle.zip");
    let zip_path = format!("{output_dir}/{actual_zip_name}");
    info!(
        "Creating module '{mod_id}': {zip_path} with {} APKs (versionCode={version_code})",
        apks.len()
    );

    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let header_spec = ModuleHeaderSpec {
        mod_id,
        mod_name,
        version_code,
        override_module_id: module_id.is_some(),
        release_tag: build_opts.release_tag.as_deref(),
    };
    write_module_header_and_scripts(&mut zip, opts, cfg, &header_spec)?;
    write_apps_metadata_and_webui(&mut zip, opts, cfg, &apks, include_webui)?;
    write_bundle_apk_payloads(&mut zip, opts, cfg, &apks)?;

    zip.finish()?;
    info!("Bundle complete: {zip_path}");
    super::verify_bundle(&zip_path)?;
    Ok(())
}

pub fn resolve_conflicts_for_bundle(cfg: &Config, apks: Vec<String>) -> Result<Vec<String>> {
    use std::collections::HashMap;

    let mut pkg_map: HashMap<String, Vec<(String, String)>> = HashMap::new(); // pkg -> [(app_id, apk_path)]

    for apk in &apks {
        let file_stem = Path::new(apk)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("app");
        let fallback = parse_apk_stem(file_stem).0;
        let app_id = cfg.resolve_app_id(file_stem).unwrap_or(fallback);

        let pkg = if let Some(app_cfg) = cfg.apps.get(app_id) {
            if !app_cfg.package.is_empty() {
                app_cfg.package.clone()
            } else {
                app_id.to_string()
            }
        } else {
            app_id.to_string()
        };

        pkg_map
            .entry(pkg)
            .or_default()
            .push((app_id.to_string(), apk.clone()));
    }

    let mut allowed_apks = Vec::new();

    for (pkg, entries) in pkg_map {
        if entries.len() == 1 {
            allowed_apks.push(entries[0].1.clone());
        } else {
            let app_ids: Vec<String> = entries.iter().map(|(id, _)| id.clone()).collect();
            tracing::warn!(
                "Bundle conflict: multiple apps {:?} share package '{}'. Keeping '{}', dropping the rest.",
                app_ids,
                pkg,
                app_ids[0]
            );
            allowed_apks.push(entries[0].1.clone());
        }
    }

    Ok(allowed_apks)
}
