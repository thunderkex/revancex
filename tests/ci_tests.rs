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

    // Patch 1 supports 1.0, 2.0
    // Patch 2 supports 2.0, 3.0
    // Patch 3 supports 2.0
    // Patch 4 supports 4.0
    // Global intersection was empty [], but version 2.0 has maximum coverage (3 patches).
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

    // Unordered versions from patch listing
    let versions = vec!["19.05.36", "19.16.39", "19.01.33", "19.11.43", "18.45.43"];
    let max = max_version(versions);
    assert_eq!(max, Some("19.16.39".to_string()));
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
