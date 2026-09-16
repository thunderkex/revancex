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
    if let Some(base) = stem.strip_suffix("-root") {
        (base, 2)
    } else if let Some(base) = stem.strip_suffix("-patched") {
        (base, 1)
    } else {
        (stem, 0)
    }
}

pub async fn build_bundle_module(
    cfg: &Config,
    apps_or_paths: &str,
    include_webui: bool,
    output_dir: &str,
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
                    .filter(|id| cfg.apps.get(*id).is_some_and(|a| a.enabled))
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
                build_custom_module(
                    cfg,
                    &module_apps,
                    include_webui,
                    output_dir,
                    Some(&mod_id),
                    Some(&mod_name),
                    Some(&zip_filename),
                )
                .await?;
            }
        }
        Ok(())
    } else if let Some(module_def) = cfg.modules.get(&lower) {
        let filtered_apps: Vec<String> = module_def
            .apps
            .iter()
            .filter(|id| cfg.apps.get(*id).is_some_and(|a| a.enabled))
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
        build_custom_module(
            cfg,
            &module_apps,
            include_webui,
            output_dir,
            Some(&mod_id),
            Some(&mod_name),
            Some(&zip_filename),
        )
        .await
    } else {
        build_custom_module(
            cfg,
            apps_or_paths,
            include_webui,
            output_dir,
            None,
            None,
            None,
        )
        .await
    }
}

pub async fn build_custom_module(
    cfg: &Config,
    apps_or_paths: &str,
    include_webui: bool,
    output_dir: &str,
    module_id: Option<&str>,
    module_name: Option<&str>,
    zip_filename: Option<&str>,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let resolved_dir = if std::path::Path::new(output_dir).exists() {
        output_dir.to_string()
    } else {
        cfg.build.output_dir.clone()
    };

    let mut apk_map: std::collections::HashMap<String, (String, u8)> =
        std::collections::HashMap::new();

    for token in apps_or_paths
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if token.eq_ignore_ascii_case("microg") {
            continue;
        }
        let p = Path::new(token);
        if p.exists() && p.is_file() {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".apk") && !name.to_lowercase().contains("microg") {
                    let stem = name.trim_end_matches(".apk");
                    let (app_id, prio) = parse_apk_stem(stem);
                    if let Some(app_cfg) = cfg.apps.get(app_id) {
                        if !app_cfg.enabled {
                            continue;
                        }
                    }
                    match apk_map.get_mut(app_id) {
                        Some((existing_path, prev_prio)) => {
                            if prio > *prev_prio {
                                *existing_path = token.to_string();
                                *prev_prio = prio;
                            }
                        }
                        None => {
                            apk_map.insert(app_id.to_string(), (token.to_string(), prio));
                        }
                    }
                }
            }
        } else if token == "all" || token == "all-enabled" {
            if let Ok(entries) = std::fs::read_dir(&resolved_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("apk") {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if !name.to_lowercase().contains("microg") {
                                let stem = name.trim_end_matches(".apk");
                                let (app_id, prio) = parse_apk_stem(stem);
                                if let Some(app_cfg) = cfg.apps.get(app_id) {
                                    if !app_cfg.enabled {
                                        continue;
                                    }
                                }
                                let path_str = path.to_string_lossy().to_string();
                                match apk_map.get_mut(app_id) {
                                    Some((existing_path, prev_prio)) => {
                                        if prio > *prev_prio {
                                            *existing_path = path_str;
                                            *prev_prio = prio;
                                        }
                                    }
                                    None => {
                                        apk_map.insert(app_id.to_string(), (path_str, prio));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            if let Ok(entries) = std::fs::read_dir(&resolved_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".apk") && !name.to_lowercase().contains("microg") {
                            let stem = name.trim_end_matches(".apk");
                            let (app_id, prio) = parse_apk_stem(stem);
                            if app_id == token {
                                if let Some(app_cfg) = cfg.apps.get(app_id) {
                                    if !app_cfg.enabled {
                                        continue;
                                    }
                                }
                                let path_str = path.to_string_lossy().to_string();
                                match apk_map.get_mut(app_id) {
                                    Some((existing_path, prev_prio)) => {
                                        if prio > *prev_prio {
                                            *existing_path = path_str;
                                            *prev_prio = prio;
                                        }
                                    }
                                    None => {
                                        apk_map.insert(app_id.to_string(), (path_str, prio));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut apks: Vec<String> = apk_map.into_values().map(|(p, _)| p).collect();

    let max_bytes = cfg.build.bundle.max_bytes;
    let mut total_bundle_size: u64 = 0;
    let mut selected_apks = Vec::new();

    apks.sort_by(|a, b| {
        let priority_of = |p: &str| -> u32 {
            let stem = std::path::Path::new(p)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(p);
            let (app_id, _) = parse_apk_stem(stem);
            cfg.apps
                .get(app_id)
                .map(|a| a.bundle_priority)
                .unwrap_or(100)
        };
        let pa = priority_of(a);
        let pb = priority_of(b);
        pa.cmp(&pb).then_with(|| a.cmp(b))
    });

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
    let apks = resolve_conflicts_for_bundle(cfg, selected_apks)?;

    if apks.is_empty() {
        tracing::warn!("No APKs found to bundle for '{apps_or_paths}' in '{output_dir}', skipping bundle creation.");
        return Ok(());
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let default_id = cfg.module_meta.resolved_id();
    let default_name = cfg.module_meta.resolved_name();
    let mod_id = module_id.unwrap_or(default_id);
    let mod_name = module_name.unwrap_or(default_name);
    let actual_zip_name = zip_filename.unwrap_or("revancex-bundle.zip");
    let zip_path = format!("{output_dir}/{actual_zip_name}");
    info!(
        "Creating module '{mod_id}': {zip_path} with {} APKs",
        apks.len()
    );

    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("META-INF/com/google/android/update-binary", opts)?;
    zip.write_all(super::UPDATE_BINARY)?;

    zip.start_file("META-INF/com/google/android/updater-script", opts)?;
    zip.write_all(b"#MAGISK\n")?;

    let version = env!("CARGO_PKG_VERSION");
    let author = cfg.module_meta.resolved_author();
    let default_desc = cfg.module_meta.resolved_description();
    let description =
        if mod_name == default_name || mod_name == "ReVanceX" || mod_name == "ReVanceX Bundle" {
            default_desc.to_string()
        } else {
            format!("Patched module for {mod_name} via ReVanceX")
        };
    let update_json = cfg.module_meta.resolved_update_json();
    let min_magisk = cfg.module_meta.min_magisk;
    let min_kernelsu = cfg.module_meta.min_kernelsu;

    zip.start_file("module.prop", opts)?;
    let mut prop_str = format!(
        "id={mod_id}\nname={mod_name}\nversion=v{version}\nversionCode={ts}\nauthor={author}\ndescription={description}\nminMagisk={min_magisk}\nminKernelSU={min_kernelsu}\n"
    );
    if !update_json.is_empty() {
        prop_str.push_str(&format!("updateJson={update_json}\n"));
    }
    zip.write_all(prop_str.as_bytes())?;

    let strip_crlf =
        |data: &[u8]| -> Vec<u8> { data.iter().copied().filter(|&b| b != b'\r').collect() };

    zip.start_file("customize.sh", opts)?;
    zip.write_all(&strip_crlf(super::CUSTOMIZE_SH))?;

    zip.start_file("utils.sh", opts)?;
    zip.write_all(&strip_crlf(super::UTILS_SH))?;

    zip.start_file("service.sh", opts)?;
    zip.write_all(&strip_crlf(super::SERVICE_SH))?;

    zip.start_file("action.sh", opts)?;
    zip.write_all(&strip_crlf(super::ACTION_SH))?;

    zip.start_file("uninstall.sh", opts)?;
    zip.write_all(&strip_crlf(super::UNINSTALL_SH))?;

    let mut apps_list_content = String::new();
    let mut apps_json_array = Vec::new();

    for apk in &apks {
        let file_stem = Path::new(apk)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("app");
        let (app_id, _) = parse_apk_stem(file_stem);

        let (default_pkg, mode, desc) = if let Some(app_cfg) = cfg.apps.get(app_id) {
            (
                app_cfg.package.clone(),
                app_cfg.mode.clone(),
                app_cfg.description.clone(),
            )
        } else {
            (app_id.to_string(), "patch".to_string(), None)
        };

        let pkg = crate::utils::apk::get_apk_package_name(apk).unwrap_or(default_pkg);

        apps_list_content.push_str(&format!("{app_id}:{pkg}:{mode}\n"));

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
        super::webui::inject_webui(&mut zip, opts, json_str.as_deref())?;
    }

    for apk in &apks {
        let name = Path::new(apk)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app.apk");
        let (app_dir, _) = parse_apk_stem(name.trim_end_matches(".apk"));

        let mut apk_file = std::fs::File::open(apk).with_context(|| format!("Opening {apk}"))?;
        let apk_entry = format!("apks/{app_dir}.apk");
        zip.start_file(&apk_entry, opts)?;
        std::io::copy(&mut apk_file, &mut zip)?;

        let stock_candidates = [
            format!("{}/{app_dir}-input.apk", cfg.build.temp_dir),
            format!("tmp/{app_dir}-input.apk"),
            format!("{}/{app_dir}.apk", cfg.build.temp_dir),
        ];
        for stock_cand in &stock_candidates {
            if Path::new(stock_cand).exists() {
                if let Ok(mut sf) = std::fs::File::open(stock_cand) {
                    let stock_entry = format!("stock/{app_dir}.apk");
                    if zip.start_file(&stock_entry, opts).is_ok() {
                        let _ = std::io::copy(&mut sf, &mut zip);
                    }
                }
                break;
            }
        }
    }

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
        let (app_id, _) = parse_apk_stem(file_stem);

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
