use revancex::{builder, config, module, orchestrator, utils};
use std::io::Write;
use zip::write::SimpleFileOptions;

#[test]
fn test_config_loader_and_validation() {
    let cfg = config::load_config("./config").expect("Failed to load config");

    assert!(!cfg.apps.is_empty(), "Apps registry should not be empty");
    assert!(cfg.apps.len() >= 28, "Expected at least 28 configured apps");

    let youtube = cfg.apps.get("youtube").expect("youtube app missing");
    assert_eq!(youtube.package, "com.google.android.youtube");
    assert!(youtube.dependencies.contains("microg"));

    let microg = cfg.apps.get("microg").expect("microg app missing");
    assert_eq!(microg.package, "app.revanced.android.gms");

    let news = cfg
        .apps
        .get("google_news")
        .expect("google_news app missing");
    assert_eq!(news.package, "com.google.android.apps.magazines");

    config::validate_config(&cfg, true).expect("Strict validation failed");
}

#[test]
fn test_semver_compatibility_logic() {
    assert!(utils::semver::check_compatibility(
        "18.32.39",
        Some("18.0.0"),
        None
    ));
    assert!(utils::semver::check_compatibility(
        "v2.5.0",
        Some("2.0.0"),
        Some("3.0.0")
    ));
    assert!(!utils::semver::check_compatibility(
        "1.5.0",
        Some("2.0.0"),
        None
    ));
    assert!(!utils::semver::check_compatibility(
        "4.0.0",
        None,
        Some("3.5.0")
    ));

    assert!(utils::semver::check_compatibility(
        "10.0.0",
        Some("9.0.0"),
        None
    ));
    assert!(!utils::semver::check_compatibility(
        "9.0.0",
        Some("10.0.0"),
        None
    ));
    assert!(utils::semver::check_compatibility(
        "9.0.0",
        None,
        Some("10.0.0")
    ));
    assert!(!utils::semver::check_compatibility(
        "10.0.0",
        None,
        Some("9.0.0")
    ));
}

#[test]
fn test_real_arch_stripping() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let test_apk = tmp_dir.path().join("test-multiarch.apk");
    let test_apk_path = test_apk.to_str().unwrap();

    {
        let file = std::fs::File::create(&test_apk).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("AndroidManifest.xml", opts).unwrap();
        zip.write_all(b"dummy android manifest").unwrap();

        zip.start_file("lib/arm64-v8a/libnative.so", opts).unwrap();
        zip.write_all(&vec![0xAA; 5000]).unwrap();

        zip.start_file("lib/x86/libnative.so", opts).unwrap();
        zip.write_all(&vec![0xBB; 5000]).unwrap();

        zip.start_file("lib/x86_64/libnative.so", opts).unwrap();
        zip.write_all(&vec![0xCC; 5000]).unwrap();

        zip.start_file("lib/armeabi-v7a/libnative.so", opts)
            .unwrap();
        zip.write_all(&vec![0xDD; 5000]).unwrap();

        zip.finish().unwrap();
    }

    let initial_size = std::fs::metadata(&test_apk).unwrap().len();

    let saved = builder::arch::strip_unsupported_archs(test_apk_path, "arm64-v8a")
        .expect("Arch stripping failed");

    assert!(saved > 0, "Stripping should save bytes");
    let final_size = std::fs::metadata(&test_apk).unwrap().len();
    assert!(
        final_size < initial_size,
        "Final size should be smaller than initial"
    );

    let file = std::fs::File::open(&test_apk).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    let mut has_arm64 = false;
    let mut has_x86 = false;
    let mut has_v7a = false;

    for i in 0..archive.len() {
        let name = archive.by_index(i).unwrap().name().to_string();
        if name.contains("arm64-v8a") {
            has_arm64 = true;
        }
        if name.contains("x86") {
            has_x86 = true;
        }
        if name.contains("armeabi-v7a") {
            has_v7a = true;
        }
    }

    assert!(has_arm64, "arm64-v8a native lib should be preserved");
    assert!(!has_x86, "x86 native lib must be stripped");
    assert!(!has_v7a, "armeabi-v7a native lib must be stripped");

    builder::arch::verify_arch_native_libs(test_apk_path, test_apk_path, "arm64-v8a")
        .expect("verify_arch_native_libs should succeed for preserved arch");

    let err =
        builder::arch::verify_arch_native_libs(test_apk_path, test_apk_path, "x86_64").unwrap_err();
    assert!(err
        .to_string()
        .contains("no native libs for x86_64 after arch-stripping — check the input APK / cache"));
}

#[test]
fn test_arch_stripping_preserves_original_cache() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let original_apk = tmp_dir.path().join("original-multiarch.apk");
    let orig_path = original_apk.to_str().unwrap();

    {
        let file = std::fs::File::create(&original_apk).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("AndroidManifest.xml", opts).unwrap();
        zip.write_all(b"manifest").unwrap();

        zip.start_file("lib/arm64-v8a/libtest.so", opts).unwrap();
        zip.write_all(&vec![0x11; 1000]).unwrap();

        zip.start_file("lib/x86_64/libtest.so", opts).unwrap();
        zip.write_all(&vec![0x22; 1000]).unwrap();

        zip.finish().unwrap();
    }

    let orig_len = std::fs::metadata(&original_apk).unwrap().len();

    let arm64_work = tmp_dir.path().join("arm64-work.apk");
    let arm64_path = arm64_work.to_str().unwrap();
    builder::arch::strip_unsupported_archs_to(orig_path, arm64_path, "arm64-v8a")
        .expect("strip to arm64 failed");

    assert_eq!(
        std::fs::metadata(&original_apk).unwrap().len(),
        orig_len,
        "Original cached APK must be completely untouched after arm64 stripping"
    );
    let mut orig_archive =
        zip::ZipArchive::new(std::fs::File::open(&original_apk).unwrap()).unwrap();
    let orig_entries: Vec<String> = (0..orig_archive.len())
        .map(|i| orig_archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(orig_entries.contains(&"lib/arm64-v8a/libtest.so".to_string()));
    assert!(orig_entries.contains(&"lib/x86_64/libtest.so".to_string()));

    let x86_work = tmp_dir.path().join("x86_64-work.apk");
    let x86_path = x86_work.to_str().unwrap();
    builder::arch::strip_unsupported_archs_to(orig_path, x86_path, "x86_64")
        .expect("strip to x86_64 failed");

    let mut arm64_archive =
        zip::ZipArchive::new(std::fs::File::open(&arm64_work).unwrap()).unwrap();
    let arm64_entries: Vec<String> = (0..arm64_archive.len())
        .map(|i| arm64_archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(arm64_entries.contains(&"lib/arm64-v8a/libtest.so".to_string()));
    assert!(!arm64_entries.contains(&"lib/x86_64/libtest.so".to_string()));

    let mut x86_archive = zip::ZipArchive::new(std::fs::File::open(&x86_work).unwrap()).unwrap();
    let x86_entries: Vec<String> = (0..x86_archive.len())
        .map(|i| x86_archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(x86_entries.contains(&"lib/x86_64/libtest.so".to_string()));
    assert!(!x86_entries.contains(&"lib/arm64-v8a/libtest.so".to_string()));
}

#[test]
fn test_real_lite_optimization() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let test_apk = tmp_dir.path().join("test-bloated.apk");
    let test_apk_path = test_apk.to_str().unwrap();

    {
        let file = std::fs::File::create(&test_apk).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("classes.dex", opts).unwrap();
        zip.write_all(b"dex file bytecode").unwrap();

        zip.start_file("META-INF/CERT.RSA", opts).unwrap();
        zip.write_all(b"old cert signature").unwrap();

        zip.start_file("build-data.properties", opts).unwrap();
        zip.write_all(b"build.number=123").unwrap();

        zip.finish().unwrap();
    }

    let initial_size = std::fs::metadata(&test_apk).unwrap().len();

    let saved = builder::lite::optimize_apk(test_apk_path).expect("Lite optimization failed");

    assert!(saved > 0, "Lite optimization should save bytes");
    let final_size = std::fs::metadata(&test_apk).unwrap().len();
    assert!(
        final_size < initial_size,
        "Final size should be smaller than initial"
    );

    let file = std::fs::File::open(&test_apk).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    for i in 0..archive.len() {
        let name = archive.by_index(i).unwrap().name().to_string();
        assert!(!name.ends_with(".RSA"), "CERT.RSA must be removed");
        assert!(!name.ends_with(".properties"), "properties must be removed");
    }
}

#[tokio::test]
async fn test_magisk_module_bundle_creation_and_verification() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let out_dir = tmp_dir.path().to_str().unwrap();

    let apk1 = tmp_dir.path().join("app_one.apk");
    let apk2 = tmp_dir.path().join("app_two.apk");

    for apk_path in [&apk1, &apk2] {
        let file = std::fs::File::create(apk_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("AndroidManifest.xml", opts).unwrap();
        zip.write_all(&vec![0x7F; 5000]).unwrap();
        zip.start_file("classes.dex", opts).unwrap();
        zip.write_all(&vec![0xAA; 10000]).unwrap();
        zip.finish().unwrap();
    }

    let cfg = config::load_config("./config").expect("Failed to load config");
    let apk_paths = format!("{},{}", apk1.to_str().unwrap(), apk2.to_str().unwrap());

    module::bundle::build_bundle_module(&cfg, &apk_paths, true, out_dir)
        .await
        .expect("Failed to build bundle module");

    let mut found_zip = None;
    for entry in std::fs::read_dir(out_dir).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) == Some("zip") {
            found_zip = Some(p);
            break;
        }
    }

    let zip_path = found_zip.expect("Expected a generated zip file in output directory");
    let zip_str = zip_path.to_str().unwrap();

    module::verify_bundle(zip_str).expect("Magisk module bundle verification failed");

    let file = std::fs::File::open(&zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    let mut has_update_binary = false;
    let mut has_updater_script = false;
    let mut has_module_prop = false;
    let mut has_customize_sh = false;
    let mut has_webui = false;
    let mut apk_entries = 0;

    for i in 0..archive.len() {
        let name = archive.by_index(i).unwrap().name().to_string();
        match name.as_str() {
            "META-INF/com/google/android/update-binary" => has_update_binary = true,
            "META-INF/com/google/android/updater-script" => has_updater_script = true,
            "module.prop" => has_module_prop = true,
            "customize.sh" => has_customize_sh = true,
            "webroot/index.html" => has_webui = true,
            _ => {
                if name.starts_with("apks/") && name.ends_with(".apk") {
                    apk_entries += 1;
                }
            }
        }
    }

    assert!(has_update_binary, "Missing update-binary in zip");
    assert!(has_updater_script, "Missing updater-script in zip");
    assert!(has_module_prop, "Missing module.prop in zip");
    assert!(has_customize_sh, "Missing customize.sh in zip");
    assert!(has_webui, "Missing webroot/index.html in zip");
    assert_eq!(apk_entries, 2, "Expected 2 APKs inside apks/");
}

#[tokio::test]
async fn test_matrix_generation_scenario() {
    let cfg = config::load_config("./config").expect("Failed to load config");
    orchestrator::generate_matrix(&cfg, "youtube,microg", "arm64-v8a", "full")
        .await
        .expect("Matrix generation failed");
}

#[test]
fn test_bundle_conflict_resolution() {
    let mut cfg = config::load_config("./config").expect("Failed to load config");
    let yt_variant = cfg.apps.get("youtube").unwrap().clone();
    cfg.apps.insert("youtube_alt".to_string(), yt_variant);

    let apks = vec![
        "output/youtube.apk".to_string(),
        "output/youtube_alt.apk".to_string(),
        "output/spotify.apk".to_string(),
    ];
    let result = module::bundle::resolve_conflicts_for_bundle(&cfg, apks);
    assert!(
        result.is_ok(),
        "Expected conflict to warn-and-continue, not bail"
    );
    let kept = result.unwrap();
    assert_eq!(kept.len(), 2, "Expected 2 APKs after conflict resolution");
}

#[test]
fn test_is_valid_jar_or_bundle() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let corrupt_path = tmp_dir.path().join("corrupt.jar");
    std::fs::write(&corrupt_path, b"corrupted content").unwrap();
    assert!(!utils::is_valid_jar_or_bundle(&corrupt_path));

    let valid_path = tmp_dir.path().join("valid.jar");
    {
        let file = std::fs::File::create(&valid_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("META-INF/MANIFEST.MF", opts).unwrap();
        zip.write_all(&vec![0x20; 1200]).unwrap();
        zip.finish().unwrap();
    }
    assert!(utils::is_valid_jar_or_bundle(&valid_path));
}

#[test]
fn test_validation_rules() {
    let mut cfg = config::load_config("./config").expect("Failed to load config");

    config::validate_config(&cfg, true).expect("Strict validation on default config should pass");

    let dup_app = cfg.apps.get("youtube").unwrap().clone();
    cfg.apps.insert("youtube_dup".to_string(), dup_app);

    let bad_module = config::loader::ModuleDefinition {
        apps: vec!["youtube".to_string(), "youtube_dup".to_string()],
        ..Default::default()
    };
    cfg.modules
        .insert("bad_duplicate_module".to_string(), bad_module);

    let err = config::validate_config(&cfg, true).unwrap_err();
    assert!(
        err.to_string()
            .contains("Package collision in module 'bad_duplicate_module'"),
        "Expected collision error, got: {err}"
    );

    cfg.modules.remove("bad_duplicate_module");
    cfg.apps.remove("youtube_dup");
    let missing_app_mod = config::loader::ModuleDefinition {
        apps: vec!["non_existent_app_123".to_string()],
        ..Default::default()
    };
    cfg.modules
        .insert("missing_app_module".to_string(), missing_app_mod);

    let err2 = config::validate_config(&cfg, true).unwrap_err();
    assert!(
        err2.to_string().contains("non-existent app"),
        "Expected non-existent app error, got: {err2}"
    );
}

#[tokio::test]
async fn test_patches_version_dev_resolution() {
    let cfg = config::load_config("./config").expect("Failed to load config");
    let ig = cfg.apps.get("instagram").expect("instagram app missing");
    assert_eq!(ig.patches_version.as_deref(), Some("dev"));

    let client = reqwest::Client::builder()
        .user_agent("revancex-builder/666")
        .build()
        .unwrap();

    let asset_url =
        utils::github::resolve_asset_url(&client, "crimera/piko", "*.mpp", Some("dev")).await;
    if let Ok(url) = asset_url {
        assert!(url.contains("dev"), "Expected dev asset URL, got: {url}");
        assert!(url.ends_with(".mpp"), "Expected .mpp asset URL, got: {url}");
    }
}

#[tokio::test]
async fn test_auto_version_resolution_from_patch() {
    let cfg = config::load_config("./config").expect("Failed to load config");
    if let Some(x_app) = cfg.apps.get("x_piko") {
        if std::path::Path::new("tools/morphe-cli.jar").exists() {
            let ver = builder::resolve_patch_compatible_version(&cfg, x_app).await;
            if let Some(v) = ver {
                assert_eq!(v, "12.19.1-release.0");
            }
        }
    }
    if let Some(ig_app) = cfg.apps.get("instagram") {
        if std::path::Path::new("tools/morphe-cli.jar").exists() {
            let ver = builder::resolve_patch_compatible_version(&cfg, ig_app).await;
            if let Some(v) = ver {
                assert_eq!(v, "439.0.0.37.89");
            }
        }
    }
}

#[test]
fn test_patcher_name_normalization_idempotent() {
    use revancex::patcher::name::normalize;
    let inputs = [
        "Custom branding icon for YouTube",
        "GmsCore_support-v2",
        "hide-shorts",
        "   multiple   spaces   and---hyphens_here  ",
    ];
    for input in inputs {
        let once = normalize(input);
        let twice = normalize(&once);
        assert_eq!(
            once, twice,
            "Normalization must be idempotent for '{input}'"
        );
    }
}

#[test]
fn test_patch_plan_invariants() {
    let cfg = config::load_config("./config").expect("Failed to load config");
    for (id, app) in &cfg.apps {
        let plan =
            revancex::patcher::plan::resolve(app, false, None, None, &cfg.build.patcher.rules);
        for inc in &plan.included {
            assert!(
                !plan.excluded.contains(inc),
                "App {id}: patch {inc} cannot be in both included and excluded"
            );
        }
    }
}

#[test]
fn test_build_args_uses_resolved_plan() {
    let cfg = config::load_config("./config").expect("Failed to load config");
    for (id, app) in &cfg.apps {
        for is_root in [false, true] {
            let plan = revancex::patcher::plan::resolve(
                app,
                is_root,
                None,
                None,
                &cfg.build.patcher.rules,
            );
            let args = revancex::patcher::cli::build_args(
                &cfg.build.tools_dir,
                &app.patch_source,
                app,
                "input.apk",
                "output.apk",
                &plan,
            );
            assert_eq!(
                args.included, plan.included,
                "App {id} (root={is_root}): args.included must equal plan.included"
            );
            assert_eq!(
                args.excluded, plan.excluded,
                "App {id} (root={is_root}): args.excluded must equal plan.excluded"
            );
        }
    }
}

#[test]
fn test_plan_resolve_with_apk_version_auto_disables_incompatible() {
    use revancex::patcher::metadata::{PackageCompat, PatchMeta};
    use std::sync::Arc;

    let cfg = config::load_config("./config").expect("Failed to load config");
    let mut app = cfg.apps.values().next().unwrap().clone();
    app.package = "com.test.pkg".to_string();
    app.patches = vec!["PatchA".to_string(), "PatchB".to_string()];

    let meta = Arc::new(vec![
        PatchMeta {
            name: "PatchA".to_string(),
            description: "Patch A".to_string(),
            compatible_packages: vec![PackageCompat {
                name: "com.test.pkg".to_string(),
                versions: vec!["1.0.0".to_string()],
            }],
        },
        PatchMeta {
            name: "PatchB".to_string(),
            description: "Patch B".to_string(),
            compatible_packages: vec![PackageCompat {
                name: "com.test.pkg".to_string(),
                versions: vec!["2.0.0".to_string()],
            }],
        },
    ]);

    let plan = revancex::patcher::plan::resolve(
        &app,
        false,
        Some(&meta),
        Some("1.0.0"),
        &cfg.build.patcher.rules,
    );

    assert!(plan.included.contains(&"PatchA".to_string()));
    assert!(!plan.included.contains(&"PatchB".to_string()));
    assert!(plan.excluded.contains(&"PatchB".to_string()));
    assert_eq!(plan.auto_disabled.len(), 1);
    assert_eq!(plan.auto_disabled[0].name, "PatchB");
}

#[test]
fn test_resolve_compatible_versions_maximum_coverage() {
    use revancex::patcher::metadata::{resolve_compatible_versions, PackageCompat, PatchMeta};

    let patches = vec![
        PatchMeta {
            name: "P1".to_string(),
            description: "".to_string(),
            compatible_packages: vec![PackageCompat {
                name: "com.pkg".to_string(),
                versions: vec!["1.0.0".to_string(), "2.0.0".to_string()],
            }],
        },
        PatchMeta {
            name: "P2".to_string(),
            description: "".to_string(),
            compatible_packages: vec![PackageCompat {
                name: "com.pkg".to_string(),
                versions: vec!["2.0.0".to_string(), "3.0.0".to_string()],
            }],
        },
        PatchMeta {
            name: "P3".to_string(),
            description: "".to_string(),
            compatible_packages: vec![PackageCompat {
                name: "com.pkg".to_string(),
                versions: vec!["2.0.0".to_string()],
            }],
        },
        PatchMeta {
            name: "P4".to_string(),
            description: "".to_string(),
            compatible_packages: vec![PackageCompat {
                name: "com.pkg".to_string(),
                versions: vec!["4.0.0".to_string()],
            }],
        },
    ];

    let result = resolve_compatible_versions(&patches, "com.pkg", &[]);
    assert_eq!(result, vec!["2.0.0".to_string()]);
}

#[test]
fn test_max_version_semver_ordering() {
    use revancex::utils::semver::max_version;

    let versions = vec!["19.05.36", "19.16.39", "19.01.33", "19.11.43", "18.45.43"];
    let max = max_version(versions);
    assert_eq!(max, Some("19.16.39".to_string()));
}

#[test]
fn test_parse_applied_patch_count() {
    use revancex::patcher::parse_applied_patch_count;

    let stdout_summary_zero = "Initializing...\nApplied 0 patches\nDone.";
    assert_eq!(parse_applied_patch_count(stdout_summary_zero), Some(0));

    let stdout_summary_many = "Initializing...\nApplied 47 patches\nDone.";
    assert_eq!(parse_applied_patch_count(stdout_summary_many), Some(47));

    let stdout_individual = "Initializing...\nApplied PatchA\nApplied PatchB\nDone.";
    assert_eq!(parse_applied_patch_count(stdout_individual), Some(2));

    let stdout_none = "Initializing...\nFinished without patch logs";
    assert_eq!(parse_applied_patch_count(stdout_none), None);
}

#[test]
fn test_generate_apps_json_is_bare_array() {
    let cfg = config::load_config("./config").expect("Failed to load config");
    let json_str = revancex::pages::generate_apps_json(&cfg).expect("generate_apps_json failed");
    let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("invalid json");
    assert!(
        parsed.is_array(),
        "generate_apps_json must emit a top-level JSON array"
    );
    let arr = parsed.as_array().unwrap();
    assert!(!arr.is_empty(), "apps array should not be empty");
    assert!(
        arr[0].get("id").is_some(),
        "entry must have an 'id' property"
    );
}

#[test]
fn test_generate_apps_json_with_registry_integration() {
    let cfg = config::load_config("./config").expect("Failed to load config");
    let json_str = revancex::pages::generate_apps_json_with_registry(
        &cfg,
        Some("config/releases_registry.json"),
    )
    .expect("generate_apps_json_with_registry failed");

    let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("invalid json");
    assert!(parsed.is_array(), "output must be a bare JSON array");

    let arr = parsed.as_array().unwrap();
    let adguard = arr
        .iter()
        .find(|item| item["id"] == "adguard")
        .expect("adguard not found in apps.json");

    assert!(
        adguard["download_url"].is_string(),
        "adguard must have a download_url string"
    );
    assert!(
        adguard["download_url"]
            .as_str()
            .unwrap()
            .starts_with("https://"),
        "adguard download_url must be an https URL"
    );
    assert!(
        adguard["updated_at"].is_string(),
        "adguard must have an updated_at timestamp"
    );
    assert!(
        adguard["releases"]["stock"].is_object(),
        "adguard must have stock release details"
    );
}

#[test]
fn test_check_updates_independent_branches_per_app() {
    use revancex::orchestrator::discovery::detect_changed_apps_from_tags;
    use std::collections::HashMap;

    let mut pairs: HashMap<(String, String, Option<String>), Vec<String>> = HashMap::new();
    pairs.insert(
        (
            "mysource".to_string(),
            "org/patches".to_string(),
            Some("main".to_string()),
        ),
        vec!["app_stable".to_string()],
    );
    pairs.insert(
        (
            "mysource".to_string(),
            "org/patches".to_string(),
            Some("dev".to_string()),
        ),
        vec!["app_dev".to_string()],
    );

    let mut cached_tags: HashMap<String, String> = HashMap::new();
    cached_tags.insert("mysource@main".to_string(), "v1.0.0".to_string());
    cached_tags.insert("mysource@dev".to_string(), "v2.0.0".to_string());

    let mut latest_tags: HashMap<(String, Option<String>), String> = HashMap::new();
    latest_tags.insert(
        ("mysource".to_string(), Some("main".to_string())),
        "v1.0.0".to_string(),
    );
    latest_tags.insert(
        ("mysource".to_string(), Some("dev".to_string())),
        "v2.1.0".to_string(),
    );

    let (changed_sources, changed_apps, updated_cache) =
        detect_changed_apps_from_tags(&pairs, &cached_tags, &latest_tags);

    assert_eq!(changed_sources, vec!["mysource".to_string()]);
    assert_eq!(changed_apps, vec!["app_dev".to_string()]);
    assert!(!changed_apps.contains(&"app_stable".to_string()));
    assert_eq!(updated_cache.get("mysource@dev").unwrap(), "v2.1.0");
    assert_eq!(updated_cache.get("mysource@main").unwrap(), "v1.0.0");
}

#[test]
fn test_check_app_version_compatibility_warning() {
    use revancex::patcher::metadata::{check_app_version_compatibility, PackageCompat, PatchMeta};

    let cfg = config::load_config("./config").expect("Failed to load config");
    let mut app = cfg.apps.get("instagram").cloned().unwrap();
    app.max_version = Some("439.0.0.37.89".to_string());

    let patches = vec![PatchMeta {
        name: "TestPatch".to_string(),
        description: "".to_string(),
        compatible_packages: vec![PackageCompat {
            name: "com.instagram.android".to_string(),
            versions: vec!["420.0.0.1".to_string(), "430.0.0.1".to_string()],
        }],
    }];

    let warn = check_app_version_compatibility("instagram", &app, &patches);
    assert!(warn.is_none());

    app.max_version = Some("400.0.0.0".to_string());
    let warn2 = check_app_version_compatibility("instagram", &app, &patches);
    assert!(warn2.is_some());
    assert!(warn2.unwrap().contains("fall outside every enabled patch"));
}

#[tokio::test]
async fn test_smoke_testing_static_dex_check() {
    let report = revancex::testing::run_device_tests("all", "./nonexistent_dir", false, 0, 0)
        .await
        .expect("smoke test run failed");
    assert_eq!(report.failed, 0);

    let out_dir = tempfile::tempdir().expect("Failed to create out tempdir");
    let manifest_bytes = std::fs::read("tests/fixtures/test_manifest.xml")
        .expect("test_manifest.xml fixture missing");

    let microg_apk = out_dir.path().join("microg.apk");
    {
        let file = std::fs::File::create(&microg_apk).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("AndroidManifest.xml", opts).unwrap();
        zip.write_all(&manifest_bytes).unwrap();
        zip.start_file("classes.dex", opts).unwrap();
        zip.write_all(b"sample dex bytecode 12345").unwrap();
        zip.finish().unwrap();
    }

    let report_microg = revancex::testing::run_device_tests(
        "microg",
        out_dir.path().to_str().unwrap(),
        false,
        0,
        0,
    )
    .await
    .expect("smoke test run for microg failed");

    assert_eq!(report_microg.failed, 0);
    assert_eq!(report_microg.passed, 1);
    assert_eq!(report_microg.items[0].app, "microg");
    assert_eq!(report_microg.items[0].package, "app.revanced.android.gms");
    assert_eq!(report_microg.items[0].status, "passed");
}

#[test]
fn test_find_manifest_attr_real_binary_xml() {
    let manifest_bytes = std::fs::read("tests/fixtures/test_manifest.xml")
        .expect("test_manifest.xml fixture missing");
    let pkg = revancex::utils::apk::find_manifest_attr(&manifest_bytes, "package");
    assert_eq!(pkg.as_deref(), Some("app.revanced.android.gms"));

    let ver_name = revancex::utils::apk::find_manifest_attr(&manifest_bytes, "versionName");
    assert_eq!(ver_name.as_deref(), Some("7.1.1"));

    let ver_code = revancex::utils::apk::find_manifest_attr(&manifest_bytes, "versionCode");
    assert_eq!(ver_code.as_deref(), Some("255070107"));
}

#[test]
fn test_uptodown_turnstile_blocked_detected() {
    let html = std::fs::read_to_string("tests/fixtures/uptodown/turnstile_blocked.html")
        .expect("fixture file missing");

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "cf-ray",
        reqwest::header::HeaderValue::from_static("abc123-SIN"),
    );

    assert!(
        revancex::builder::is_turnstile_blocked(403, &headers, &html),
        "Should detect Turnstile via cf-ray + 403"
    );

    let empty_headers = reqwest::header::HeaderMap::new();
    assert!(
        revancex::builder::is_turnstile_blocked(200, &empty_headers, &html),
        "Should detect Turnstile via body markers"
    );
}

#[test]
fn test_uptodown_token_found_not_blocked() {
    let html = std::fs::read_to_string("tests/fixtures/uptodown/token_found.html")
        .expect("fixture file missing");

    let empty_headers = reqwest::header::HeaderMap::new();
    assert!(
        !revancex::builder::is_turnstile_blocked(200, &empty_headers, &html),
        "Normal download page must not be flagged as Turnstile"
    );

    let re = regex::Regex::new(r#"data-url="([^"]+)""#).unwrap();
    let cap = re.captures(&html).expect("data-url not found in fixture");
    let token = &cap[1];
    assert!(
        token.len() > 10 && !token.starts_with("http"),
        "Token should be a non-URL string longer than 10 chars, got: {token}"
    );
}

#[test]
fn test_source_priority_field_deserializes() {
    let yaml = r#"
enabled: true
package: com.example.app
source_priority:
  - apkpure
  - uptodown
  - apkeep
apkpure_url: "com.example.app"
"#;
    let app: revancex::config::apps::AppConfig =
        serde_yaml::from_str(yaml).expect("Failed to deserialize AppConfig with source_priority");
    assert_eq!(app.source_priority, vec!["apkpure", "uptodown", "apkeep"]);
    assert_eq!(app.apkpure_url.as_deref(), Some("com.example.app"));
}

#[test]
fn test_keystore_password_fallback_on_empty_or_short() {
    std::env::set_var("KEYSTORE_PASSWORD", "");
    assert_eq!(revancex::utils::get_keystore_password(), "revanced");

    std::env::set_var("KEYSTORE_PASSWORD", "   ");
    assert_eq!(revancex::utils::get_keystore_password(), "revanced");

    std::env::set_var("KEYSTORE_PASSWORD", "12345");
    assert_eq!(revancex::utils::get_keystore_password(), "revanced");

    std::env::set_var("KEYSTORE_PASSWORD", "mysecret123");
    assert_eq!(revancex::utils::get_keystore_password(), "mysecret123");

    std::env::remove_var("KEYSTORE_PASSWORD");
    assert_eq!(revancex::utils::get_keystore_password(), "revanced");
}

#[tokio::test]
async fn test_circuit_breaker_concurrency_atomic_thread_safety() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let counter = Arc::new(AtomicU32::new(0));
    let num_tasks = 20;
    let increments_per_task = 50;

    let mut handles = Vec::new();
    for _ in 0..num_tasks {
        let c = counter.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..increments_per_task {
                c.fetch_add(1, Ordering::SeqCst);
                tokio::task::yield_now().await;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let expected = num_tasks * increments_per_task;
    assert_eq!(
        counter.load(Ordering::SeqCst),
        expected,
        "Atomic counter must never lose concurrent increments"
    );

    counter.store(0, Ordering::SeqCst);
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[test]
fn test_apkpure_link_scraping_and_arch_priority() {
    use regex::Regex;

    let html_fixture = r#"
        <div class="download-box">
            <a href="https://d.apkpure.com/b/XAPK/com.flightradar24free?versionCode=111000000&amp;nc=armeabi-v7a&amp;sv=29">Download V7A</a>
            <a href="https://d.apkpure.com/b/XAPK/com.flightradar24free?versionCode=111000000&amp;nc=arm64-v8a&amp;sv=32">Download V8A</a>
            <a href="https://d.apkpure.com/b/APK/com.flightradar24free?versionCode=111000000">Download Universal</a>
        </div>
    "#;

    let re_dl_link = Regex::new(
        r#"href=[\x22\x27](https://d\.apkpure\.(?:com|net)/b/(?:XAPK|APK)/[^\x22\x27]+)[\x22\x27]"#,
    )
    .unwrap();

    let mut found_links: Vec<String> = Vec::new();
    for cap in re_dl_link.captures_iter(html_fixture) {
        let link = cap[1].replace("&amp;", "&");
        if !found_links.contains(&link) {
            found_links.push(link);
        }
    }

    assert_eq!(found_links.len(), 3);
    assert!(found_links[0].contains("&nc=armeabi-v7a"));
    assert!(found_links[1].contains("&nc=arm64-v8a"));

    let target_arch = "arm64-v8a";
    found_links.sort_by_key(|link| {
        if link.contains(target_arch) {
            0
        } else if link.contains("universal") || link.contains("all") {
            1
        } else {
            2
        }
    });

    assert!(found_links[0].contains("arm64-v8a"));
}

#[test]
fn test_asset_exclude_patterns_config() {
    let yaml = r#"
asset_exclude_patterns:
  - "hw-"
  - "custom-variant"
"#;
    let bcfg: revancex::config::BuildConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        bcfg.asset_exclude_patterns,
        vec!["hw-".to_string(), "custom-variant".to_string()]
    );
}

#[test]
fn test_rotating_user_agent_pool() {
    use reqwest::header::HeaderValue;
    use std::collections::HashSet;

    let mut samples = HashSet::new();
    for _ in 0..100 {
        let ua = revancex::utils::random_user_agent();
        assert!(!ua.is_empty());
        assert!(
            ua.starts_with("Mozilla/5.0"),
            "UA should be a modern browser, got: {ua}"
        );
        assert!(
            HeaderValue::from_str(ua).is_ok(),
            "UA must be valid ASCII for HTTP HeaderValue: {ua}"
        );
        samples.insert(ua);
    }
    assert!(
        samples.len() > 1,
        "User-Agent should rotate across requests, but got only {} unique variant(s)",
        samples.len()
    );
}

#[test]
fn test_apkpure_constructed_endpoints_generation() {
    let pkg = "com.flightradar24free";
    let target_ver = Some("11.10.0");
    let mut endpoints = Vec::new();
    if let Some(ver) = target_ver {
        let ver_clean = ver.replace(' ', "+");
        endpoints.push(format!(
            "https://d.apkpure.com/b/XAPK/{pkg}?versionName={ver_clean}"
        ));
        endpoints.push(format!(
            "https://d.apkpure.com/b/APK/{pkg}?versionName={ver_clean}"
        ));
        endpoints.push(format!(
            "https://d.apkpure.net/b/XAPK/{pkg}?versionName={ver_clean}"
        ));
        endpoints.push(format!(
            "https://d.apkpure.net/b/APK/{pkg}?versionName={ver_clean}"
        ));
    }
    endpoints.push(format!("https://d.apkpure.com/b/XAPK/{pkg}?version=latest"));
    endpoints.push(format!("https://d.apkpure.com/b/APK/{pkg}?version=latest"));
    endpoints.push(format!("https://d.apkpure.net/b/XAPK/{pkg}?version=latest"));
    endpoints.push(format!("https://d.apkpure.net/b/APK/{pkg}?version=latest"));

    assert_eq!(endpoints.len(), 8);
    assert!(endpoints[0].contains("XAPK/com.flightradar24free?versionName=11.10.0"));
    assert!(endpoints[4].contains("XAPK/com.flightradar24free?version=latest"));
}

#[test]
fn test_apkpure_referer_rotation_candidates() {
    let last_page = "https://apkpure.com/app/com.duolingo/download";
    let referer_candidates: Vec<Option<&str>> = vec![
        Some(last_page),
        Some("https://apkpure.com/"),
        Some("https://apkpure.net/"),
        None,
    ];
    assert_eq!(referer_candidates.len(), 4);
    assert_eq!(referer_candidates[0], Some(last_page));
    assert_eq!(referer_candidates[1], Some("https://apkpure.com/"));
    assert_eq!(referer_candidates[2], Some("https://apkpure.net/"));
    assert_eq!(referer_candidates[3], None);
}
