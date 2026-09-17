use crate::config::Config;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::OnceCell;

#[derive(Debug, Clone)]
pub struct PatchMeta {
    pub name: String,
    pub description: String,
    pub compatible_packages: Vec<PackageCompat>,
}

#[derive(Debug, Clone)]
pub struct PackageCompat {
    pub name: String,
    pub versions: Vec<String>,
}

static CACHE: OnceCell<Mutex<HashMap<PathBuf, Arc<Vec<PatchMeta>>>>> = OnceCell::const_new();

async fn get_cache() -> &'static Mutex<HashMap<PathBuf, Arc<Vec<PatchMeta>>>> {
    CACHE
        .get_or_init(|| async { Mutex::new(HashMap::new()) })
        .await
}

pub async fn load(cfg: &Config, mpp_path: &str) -> Result<Arc<Vec<PatchMeta>>> {
    let key = PathBuf::from(mpp_path);
    {
        let cache = get_cache().await.lock().unwrap();
        if let Some(cached) = cache.get(&key) {
            return Ok(cached.clone());
        }
    }

    let cli_jar = format!("{}/morphe-cli.jar", cfg.build.tools_dir);
    let output = tokio::process::Command::new("java")
        .args([
            "-jar",
            &cli_jar,
            "list-patches",
            &format!("--patches={mpp_path}"),
            "-p",
            "-v",
        ])
        .output()
        .await?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let patches = parse_list_patches_output(&text);
    let arc = Arc::new(patches);

    {
        let mut cache = get_cache().await.lock().unwrap();
        cache.insert(key, arc.clone());
    }

    Ok(arc)
}

pub fn parse_list_patches_output(text: &str) -> Vec<PatchMeta> {
    let mut patches = Vec::new();
    let mut current_name = String::new();
    let mut current_desc = String::new();
    let mut current_packages: Vec<PackageCompat> = Vec::new();
    let mut current_pkg_name = String::new();
    let mut current_pkg_versions: Vec<String> = Vec::new();
    let mut in_compatible = false;

    let flush_pkg = |pkg_name: &mut String,
                     pkg_versions: &mut Vec<String>,
                     packages: &mut Vec<PackageCompat>| {
        if !pkg_name.is_empty() {
            packages.push(PackageCompat {
                name: std::mem::take(pkg_name),
                versions: std::mem::take(pkg_versions),
            });
        }
    };

    let flush_patch = |name: &mut String,
                       desc: &mut String,
                       pkg_name: &mut String,
                       pkg_versions: &mut Vec<String>,
                       packages: &mut Vec<PackageCompat>,
                       patches: &mut Vec<PatchMeta>| {
        flush_pkg(pkg_name, pkg_versions, packages);
        if !name.is_empty() {
            patches.push(PatchMeta {
                name: std::mem::take(name),
                description: std::mem::take(desc),
                compatible_packages: std::mem::take(packages),
            });
        }
    };

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("Name:") {
            flush_patch(
                &mut current_name,
                &mut current_desc,
                &mut current_pkg_name,
                &mut current_pkg_versions,
                &mut current_packages,
                &mut patches,
            );
            current_name = name.trim().to_string();
            in_compatible = false;
        } else if let Some(desc) = trimmed.strip_prefix("Description:") {
            current_desc = desc.trim().to_string();
        } else if let Some(pkg) = trimmed.strip_prefix("Package name:") {
            flush_pkg(
                &mut current_pkg_name,
                &mut current_pkg_versions,
                &mut current_packages,
            );
            current_pkg_name = pkg.trim().to_string();
            in_compatible = false;
        } else if trimmed.starts_with("Compatible versions:") {
            in_compatible = true;
        } else if in_compatible {
            if trimmed.is_empty()
                || trimmed.starts_with("Version codes:")
                || trimmed.starts_with("Index:")
                || trimmed.starts_with("Name:")
                || trimmed.starts_with("Package name:")
            {
                in_compatible = false;
            } else if !current_pkg_versions.contains(&trimmed.to_string()) {
                current_pkg_versions.push(trimmed.to_string());
            }
        }
    }

    flush_patch(
        &mut current_name,
        &mut current_desc,
        &mut current_pkg_name,
        &mut current_pkg_versions,
        &mut current_packages,
        &mut patches,
    );

    patches
}

pub fn resolve_compatible_versions(
    patches: &[PatchMeta],
    package: &str,
    enabled_patches: &[String],
) -> Vec<String> {
    let mut version_counts: HashMap<String, usize> = HashMap::new();

    for patch in patches {
        if !enabled_patches.is_empty()
            && !enabled_patches
                .iter()
                .any(|e| e.eq_ignore_ascii_case(&patch.name))
        {
            continue;
        }
        for compat in &patch.compatible_packages {
            if compat.name == package {
                for v in &compat.versions {
                    *version_counts.entry(v.clone()).or_insert(0) += 1;
                }
            }
        }
    }

    if version_counts.is_empty() {
        return Vec::new();
    }

    let max_count = version_counts.values().copied().max().unwrap_or(0);
    if max_count == 0 {
        return Vec::new();
    }

    let mut result: Vec<String> = version_counts
        .into_iter()
        .filter(|(_, count)| *count == max_count)
        .map(|(v, _)| v)
        .collect();

    result.sort_by(|a, b| {
        let pa = crate::utils::semver::parse_version_numbers(a);
        let pb = crate::utils::semver::parse_version_numbers(b);
        crate::utils::semver::compare_version_parts(&pa, &pb)
    });

    result
}

pub fn resolve_mpp_path(cfg: &Config, app: &crate::config::AppConfig) -> Option<String> {
    let repo = cfg
        .patch_sources
        .get(&app.patch_source)
        .map(|p| p.repo.clone())
        .unwrap_or_else(|| app.patch_source.clone());

    let safe_name = repo.replace('/', "_");
    let target = app
        .patches_version
        .as_deref()
        .or(app.patch_branch.as_deref());
    let versioned_mpp =
        target.map(|t| format!("{}/patches_{safe_name}_{t}.mpp", cfg.build.tools_dir));
    let patches_mpp = format!("{}/patches_{safe_name}.mpp", cfg.build.tools_dir);

    if let Some(ref vmpp) = versioned_mpp {
        if std::path::Path::new(vmpp).exists() {
            return Some(vmpp.clone());
        }
    }
    if std::path::Path::new(&patches_mpp).exists() {
        return Some(patches_mpp);
    }
    None
}
