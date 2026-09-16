use crate::config::apps::AppConfig;
use std::path::Path;

pub struct MorpheCliArgs {
    pub cli_jar: String,
    pub patches_arg: String,
    pub output_apk: String,
    pub input_apk: String,
    pub included: Vec<String>,
    pub excluded: Vec<String>,
    pub raw_args: Vec<String>,
    pub keystore: Option<(String, String, String)>,
}

pub fn build_args(
    tools_dir: &str,
    repo_or_alias: &str,
    app: &AppConfig,
    input_apk: &str,
    output_apk: &str,
    is_root: bool,
) -> MorpheCliArgs {
    let cli_jar = format!("{tools_dir}/morphe-cli.jar");
    let safe_name = repo_or_alias.replace('/', "_");
    let target = app
        .patches_version
        .as_deref()
        .or(app.patch_branch.as_deref());
    let versioned_mpp = target.map(|t| format!("{tools_dir}/patches_{safe_name}_{t}.mpp"));
    let patches_mpp = format!("{tools_dir}/patches_{safe_name}.mpp");

    let patches_arg = if let Some(ref vmpp) = versioned_mpp {
        if Path::new(vmpp).exists() {
            vmpp.clone()
        } else if Path::new(&patches_mpp).exists() {
            patches_mpp
        } else {
            String::new()
        }
    } else if Path::new(&patches_mpp).exists() {
        patches_mpp
    } else {
        String::new()
    };

    let patches_arg = if !patches_arg.is_empty() {
        patches_arg
    } else if let Ok(entries) = std::fs::read_dir(tools_dir) {
        entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|ext| ext == "mpp"))
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                if repo_or_alias.contains('/') {
                    format!("https://github.com/{repo_or_alias}")
                } else {
                    "https://github.com/MorpheApp/morphe-patches".to_string()
                }
            })
    } else if repo_or_alias.contains('/') {
        format!("https://github.com/{repo_or_alias}")
    } else {
        "https://github.com/MorpheApp/morphe-patches".to_string()
    };

    let mut included = app.included_patches.clone();
    included.extend(app.patches.clone());

    let is_branding_patch = |name: &str| -> bool {
        let lower = name.to_lowercase().replace(['-', '_'], " ");
        lower.contains("custom branding")
            || lower.contains("change package name")
            || lower.contains("change app icon")
            || lower.contains("premium icon")
    };
    let is_gmscore_patch = |name: &str| -> bool {
        let lower = name.to_lowercase().replace(['-', '_'], " ");
        lower.contains("gmscore") || lower.contains("microg")
    };

    if is_root {
        included.retain(|p| !is_branding_patch(p) && !is_gmscore_patch(p));
    } else {
        included.retain(|p| !is_branding_patch(p));
    }

    let mut excluded = app.excluded_patches.clone();
    let default_excludes = [
        "Custom branding icon for YouTube",
        "Custom branding name for YouTube",
        "Custom branding for YouTube",
        "Custom branding icon for YouTube Music",
        "Custom branding name for YouTube Music",
        "Custom branding for YouTube Music",
        "Custom branding",
        "Change package name",
        "Change app icon",
        "premium-icon-reddit",
    ];
    for b in default_excludes {
        if !excluded.iter().any(|e| e.eq_ignore_ascii_case(b)) {
            excluded.push(b.to_string());
        }
    }

    if is_root {
        if !excluded.iter().any(|e| is_gmscore_patch(e)) {
            excluded.push("GmsCore support".to_string());
        }
        for b in [
            "Custom branding for YouTube",
            "Custom branding for YouTube Music",
            "Custom branding",
            "Change package name",
        ] {
            if !excluded.iter().any(|e| e.eq_ignore_ascii_case(b)) {
                excluded.push(b.to_string());
            }
        }
    } else {
        excluded.retain(|e| !is_gmscore_patch(e));
        if !included.is_empty()
            && app.dependencies.contains("microg")
            && !included.iter().any(|p| is_gmscore_patch(p))
        {
            included.push("GmsCore support".to_string());
        }
    }

    let mut raw_args = Vec::new();
    if let Some(args) = &app.patcher_args {
        for part in args.split_whitespace() {
            raw_args.push(part.trim_matches('\'').to_string());
        }
    }

    let keystore = app.keystore.as_ref().and_then(|ks| {
        if Path::new(&ks.file).exists() {
            let pass = crate::utils::get_keystore_password();
            Some((ks.file.clone(), ks.alias.clone(), pass))
        } else {
            None
        }
    });

    MorpheCliArgs {
        cli_jar,
        patches_arg,
        output_apk: output_apk.to_string(),
        input_apk: input_apk.to_string(),
        included,
        excluded,
        raw_args,
        keystore,
    }
}
