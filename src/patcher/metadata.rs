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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompatibleVersions {
    Unconstrained,
    Intersection(Vec<String>),
    Conflict {
        patch_versions: Vec<(String, Vec<String>)>,
    },
    Unknown,
}

impl CompatibleVersions {
    pub fn is_empty(&self) -> bool {
        match self {
            CompatibleVersions::Unconstrained | CompatibleVersions::Unknown => true,
            CompatibleVersions::Intersection(v) => v.is_empty(),
            CompatibleVersions::Conflict { .. } => true,
        }
    }

    pub fn to_vec(&self) -> Vec<String> {
        match self {
            CompatibleVersions::Intersection(v) => v.clone(),
            _ => Vec::new(),
        }
    }
}

pub fn resolve_compatible_versions(
    patches: &[PatchMeta],
    package: &str,
    enabled_patches: &[String],
) -> CompatibleVersions {
    let active_patches: Vec<&PatchMeta> = patches
        .iter()
        .filter(|patch| {
            if enabled_patches.is_empty() {
                true
            } else {
                enabled_patches.iter().any(|e| {
                    e.eq_ignore_ascii_case(&patch.name)
                        || crate::patcher::name::normalize(e)
                            == crate::patcher::name::normalize(&patch.name)
                })
            }
        })
        .collect();

    let mut constraining: Vec<(String, Vec<String>)> = Vec::new();

    for patch in active_patches {
        if let Some(compat) = patch.compatible_packages.iter().find(|c| c.name == package) {
            if !compat.versions.is_empty() {
                constraining.push((patch.name.clone(), compat.versions.clone()));
            }
        }
    }

    if constraining.is_empty() {
        return CompatibleVersions::Unconstrained;
    }

    let first_versions = &constraining[0].1;
    let mut intersection: std::collections::HashSet<&str> =
        first_versions.iter().map(|s| s.as_str()).collect();

    for (_, versions) in &constraining[1..] {
        let set: std::collections::HashSet<&str> = versions.iter().map(|s| s.as_str()).collect();
        intersection = intersection.intersection(&set).copied().collect();
    }

    if intersection.is_empty() {
        return CompatibleVersions::Conflict {
            patch_versions: constraining,
        };
    }

    let mut result: Vec<String> = intersection.into_iter().map(String::from).collect();
    result.sort_by(|a, b| {
        let pa = crate::utils::semver::parse_version_numbers(a);
        let pb = crate::utils::semver::parse_version_numbers(b);
        crate::utils::semver::compare_version_parts(&pa, &pb)
    });

    CompatibleVersions::Intersection(result)
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

pub fn check_app_version_compatibility(
    id: &str,
    app: &crate::config::AppConfig,
    patches: &[PatchMeta],
) -> Option<String> {
    let min_c = app.version.as_deref().or(app.min_version.as_deref());
    let max_c = app.version.as_deref().or(app.max_version.as_deref());
    if min_c.is_none() && max_c.is_none() {
        return None;
    }
    if min_c == Some("auto") || max_c == Some("auto") {
        return None;
    }

    let mut has_versioned_patch = false;
    let mut any_compatible = false;

    let enabled: Vec<&str> = if !app.included_patches.is_empty() {
        app.included_patches.iter().map(|s| s.as_str()).collect()
    } else {
        app.patches.iter().map(|s| s.as_str()).collect()
    };

    for patch in patches {
        if !enabled.is_empty() && !enabled.iter().any(|p| p.eq_ignore_ascii_case(&patch.name)) {
            continue;
        }
        for compat in &patch.compatible_packages {
            if compat.name == app.package {
                if compat.versions.is_empty() {
                    any_compatible = true;
                    break;
                }
                has_versioned_patch = true;
                for v in &compat.versions {
                    if crate::utils::semver::check_compatibility(v, min_c, max_c) {
                        any_compatible = true;
                        break;
                    }
                }
            }
        }
        if any_compatible {
            break;
        }
    }

    if has_versioned_patch && !any_compatible {
        Some(format!(
            "App '{id}': pinned version constraints (min={min_c:?}, max={max_c:?}) fall outside every enabled patch's compatible range"
        ))
    } else {
        None
    }
}

pub async fn validate_patch_compatibilities(
    cfg: &Config,
    targets: Option<&[String]>,
) -> Vec<String> {
    let has_any_mpp = std::path::Path::new(&cfg.build.tools_dir)
        .read_dir()
        .map(|entries| {
            entries.filter_map(Result::ok).any(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "mpp")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    if !has_any_mpp {
        return Vec::new();
    }

    let mut warnings = Vec::new();
    for (id, app) in &cfg.apps {
        if !app.enabled || app.patch_source == "none" || app.patch_source.is_empty() {
            continue;
        }
        if let Some(target_list) = targets {
            if !target_list.iter().any(|t| t == id) {
                continue;
            }
        }
        let mpp_path = resolve_mpp_path(cfg, app);
        let is_auto = app
            .version
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case("auto"))
            .unwrap_or(true);

        if let Some(ref mpp) = mpp_path {
            if let Ok(meta) = load(cfg, mpp).await {
                if let Some(warn) = check_app_version_compatibility(id, app, &meta) {
                    warnings.push(warn);
                } else if is_auto {
                    let pkg_in_bundle = meta
                        .iter()
                        .any(|p| p.compatible_packages.iter().any(|c| c.name == app.package));
                    if !pkg_in_bundle {
                        warnings.push(format!(
                            "App '{id}': enabled with version=auto but package '{}' was not found in patch bundle",
                            app.package
                        ));
                    } else {
                        let enabled = if !app.included_patches.is_empty() {
                            &app.included_patches
                        } else {
                            &app.patches
                        };
                        let compat = resolve_compatible_versions(&meta, &app.package, enabled);
                        if let CompatibleVersions::Conflict { patch_versions } = compat {
                            let conflict_desc = patch_versions
                                .iter()
                                .map(|(p, vs)| format!("{p}: [{}]", vs.join(", ")))
                                .collect::<Vec<_>>()
                                .join("; ");
                            warnings.push(format!(
                                "App '{id}': version conflict between enabled patches: {conflict_desc}"
                            ));
                        }
                    }
                }
            } else {
                warnings.push(format!(
                    "App '{id}': could not load patch metadata from {mpp}"
                ));
            }
        } else if is_auto {
            warnings.push(format!(
                "App '{id}': enabled with version=auto but no patch bundle (.mpp) found on disk"
            ));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_patches_same_compat_versions_returns_intersection() {
        let patches = vec![
            PatchMeta {
                name: "PatchA".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["1.0.0".to_string(), "2.0.0".to_string()],
                }],
            },
            PatchMeta {
                name: "PatchB".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["2.0.0".to_string(), "3.0.0".to_string()],
                }],
            },
        ];
        let res = resolve_compatible_versions(&patches, "com.test", &[]);
        assert_eq!(
            res,
            CompatibleVersions::Intersection(vec!["2.0.0".to_string()])
        );
    }

    #[test]
    fn test_two_patches_disjoint_compat_versions_returns_conflict() {
        let patches = vec![
            PatchMeta {
                name: "PatchA".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["1.0.0".to_string()],
                }],
            },
            PatchMeta {
                name: "PatchB".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["2.0.0".to_string()],
                }],
            },
        ];
        let res = resolve_compatible_versions(&patches, "com.test", &[]);
        match res {
            CompatibleVersions::Conflict { patch_versions } => {
                assert_eq!(patch_versions.len(), 2);
                assert_eq!(patch_versions[0].0, "PatchA");
                assert_eq!(patch_versions[1].0, "PatchB");
            }
            _ => panic!("Expected Conflict, got {:?}", res),
        }
    }

    #[test]
    fn test_universal_patch_and_constrained_patch_returns_constrained_range() {
        let patches = vec![
            PatchMeta {
                name: "Universal".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec![],
                }],
            },
            PatchMeta {
                name: "Constrained".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["2.0.0".to_string(), "2.1.0".to_string()],
                }],
            },
        ];
        let res = resolve_compatible_versions(&patches, "com.test", &[]);
        assert_eq!(
            res,
            CompatibleVersions::Intersection(vec!["2.0.0".to_string(), "2.1.0".to_string()])
        );
    }

    #[test]
    fn test_irrelevant_patch_does_not_affect_result() {
        let patches = vec![
            PatchMeta {
                name: "Irrelevant".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.other".to_string(),
                    versions: vec!["9.9.9".to_string()],
                }],
            },
            PatchMeta {
                name: "Relevant".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["1.2.3".to_string()],
                }],
            },
        ];
        let res = resolve_compatible_versions(&patches, "com.test", &[]);
        assert_eq!(
            res,
            CompatibleVersions::Intersection(vec!["1.2.3".to_string()])
        );
    }

    #[test]
    fn test_empty_enabled_patches_uses_every_patch() {
        let patches = vec![
            PatchMeta {
                name: "P1".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["1.0.0".to_string(), "2.0.0".to_string()],
                }],
            },
            PatchMeta {
                name: "P2".to_string(),
                description: "".to_string(),
                compatible_packages: vec![PackageCompat {
                    name: "com.test".to_string(),
                    versions: vec!["2.0.0".to_string(), "3.0.0".to_string()],
                }],
            },
        ];
        let res_empty = resolve_compatible_versions(&patches, "com.test", &[]);
        assert_eq!(
            res_empty,
            CompatibleVersions::Intersection(vec!["2.0.0".to_string()])
        );

        let res_explicit = resolve_compatible_versions(&patches, "com.test", &["P1".to_string()]);
        assert_eq!(
            res_explicit,
            CompatibleVersions::Intersection(vec!["1.0.0".to_string(), "2.0.0".to_string()])
        );
    }
}
