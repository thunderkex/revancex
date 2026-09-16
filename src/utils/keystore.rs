use crate::config::{AppConfig, Config};
use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

pub struct ResolvedKeystore {
    pub file: String,
    pub alias: String,
    pub password: String,
    pub generated: bool,
}

pub async fn resolve(cfg: &Config, app: &AppConfig) -> Result<ResolvedKeystore> {
    let default_file = format!("{}/revanced.jks", cfg.build.keys_dir);
    let default_alias = "revanced".to_string();
    let password = crate::utils::get_keystore_password();

    if Path::new(&default_file).exists() {
        return Ok(ResolvedKeystore {
            file: default_file,
            alias: default_alias,
            password,
            generated: false,
        });
    }

    if let Some(ks) = &app.keystore {
        if Path::new(&ks.file).exists() {
            return Ok(ResolvedKeystore {
                file: ks.file.clone(),
                alias: ks.alias.clone(),
                password,
                generated: false,
            });
        }
    }

    if let Ok(b64) = std::env::var("KEYSTORE_B64") {
        use std::io::Write;
        if let Some(parent) = Path::new(&default_file).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let decoded = base64_decode(&b64)?;
        let mut f = std::fs::File::create(&default_file)?;
        f.write_all(&decoded)?;
        info!("Materialised keystore from KEYSTORE_B64 to {default_file}");
        return Ok(ResolvedKeystore {
            file: default_file,
            alias: default_alias,
            password,
            generated: false,
        });
    }

    if let Ok(src) = std::env::var("KEYSTORE_FILE") {
        if Path::new(&src).exists() {
            if let Some(parent) = Path::new(&default_file).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&src, &default_file)
                .with_context(|| format!("Copying {src} to {default_file}"))?;
            info!("Materialised keystore from KEYSTORE_FILE={src} to {default_file}");
            return Ok(ResolvedKeystore {
                file: default_file,
                alias: default_alias,
                password,
                generated: false,
            });
        }
    }

    if let Some(parent) = Path::new(&default_file).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let ks_cfg = &cfg.build.keystore;
    info!("Generating new keystore at {default_file}...");
    let status = tokio::process::Command::new("keytool")
        .args([
            "-genkeypair",
            "-keystore",
            &default_file,
            "-alias",
            &default_alias,
            "-keyalg",
            &ks_cfg.key_algorithm,
            "-keysize",
            &ks_cfg.key_size.to_string(),
            "-validity",
            &ks_cfg.validity_days.to_string(),
            "-sigalg",
            &ks_cfg.sig_algorithm,
            "-dname",
            &ks_cfg.distinguished_name,
            "-storepass",
            &password,
            "-keypass",
            &password,
            "-storetype",
            "pkcs12",
        ])
        .status()
        .await
        .context("keytool not found — install JDK")?;

    if !status.success() {
        anyhow::bail!("keytool exited with {status} while generating {default_file}");
    }

    Ok(ResolvedKeystore {
        file: default_file,
        alias: default_alias,
        password,
        generated: true,
    })
}

pub async fn print_info(cfg: &Config) -> Result<()> {
    let app_dummy = crate::config::apps::AppConfig {
        enabled: true,
        package: String::new(),
        patch_source: String::new(),
        patch_branch: None,
        cli_source: String::new(),
        uptodown_url: None,
        apkmirror_url: None,
        archive_url: None,
        apk_source: String::new(),
        apk_url: None,
        architectures: vec![],
        mode: String::new(),
        included_patches: vec![],
        excluded_patches: vec![],
        patcher_args: None,
        patches_version: None,
        version: None,
        patches: vec![],
        keystore: None,
        module: None,
        dependencies: crate::config::apps::AppDependencies::default(),
        description: None,
        min_version: None,
        max_version: None,
        post_patch_hooks: vec![],
        display_name: None,
        bundle_priority: 100,
    };
    let ks = resolve(cfg, &app_dummy).await?;
    println!("Keystore file:  {}", ks.file);
    println!("Alias:          {}", ks.alias);
    println!("Generated:      {}", ks.generated);

    let output = tokio::process::Command::new("keytool")
        .args([
            "-list",
            "-v",
            "-keystore",
            &ks.file,
            "-alias",
            &ks.alias,
            "-storepass",
            &ks.password,
        ])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            for line in text.lines() {
                if line.trim_start().starts_with("SHA256:") {
                    println!("SHA-256:        {}", line.trim());
                    break;
                }
            }
        }
        _ => {
            println!("SHA-256:        (keytool not available)");
        }
    }

    Ok(())
}

fn base64_decode(s: &str) -> Result<Vec<u8>> {
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    let table: [u8; 128] = {
        let mut t = [255u8; 128];
        for (i, &c) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
            .iter()
            .enumerate()
        {
            t[c as usize] = i as u8;
        }
        t
    };
    let bytes = clean.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let b0 = bytes[i] as usize;
        let b1 = bytes[i + 1] as usize;
        let b2 = bytes[i + 2] as usize;
        let b3 = bytes[i + 3] as usize;
        if b0 >= 128 || b1 >= 128 || b2 >= 128 || b3 >= 128 {
            anyhow::bail!("Invalid base64 character");
        }
        let v0 = table[b0];
        let v1 = table[b1];
        let v2 = if bytes[i + 2] == b'=' { 0 } else { table[b2] };
        let v3 = if bytes[i + 3] == b'=' { 0 } else { table[b3] };
        if v0 == 255 || v1 == 255 {
            anyhow::bail!("Invalid base64 character");
        }
        out.push((v0 << 2) | (v1 >> 4));
        if bytes[i + 2] != b'=' {
            out.push(((v1 & 0xF) << 4) | (v2 >> 2));
        }
        if bytes[i + 3] != b'=' {
            out.push(((v2 & 0x3) << 6) | v3);
        }
        i += 4;
    }
    Ok(out)
}
