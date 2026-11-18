use crate::config::Config;
use anyhow::{Context, Result};
use reqwest::Client;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::Mutex;
use tracing::info;
use zip::ZipArchive;

const HUSHFACEBOOK_ARCHIVE_URL: &str =
    "https://github.com/SysAdminDoc/HushFacebook/archive/refs/heads/main.zip";
const METADATA_ENTRY: &str = "assets/secondary-program-dex-jars/metadata.txt";
const ARM64_SUPERPACK_LIB: &str = "lib/arm64-v8a/libsuperpack-jni.so";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SuperpackInfo {
    pub store_entry: String,
    pub has_arm64_lib: bool,
}

fn build_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Inspects an APK/ZIP for a single Superpack secondary DEX store that has not yet been unpacked.
pub(crate) fn detect_superpack(apk_path: &str) -> Result<Option<SuperpackInfo>> {
    let file = File::open(apk_path).with_context(|| format!("Opening {apk_path}"))?;
    let mut archive = ZipArchive::new(file).with_context(|| format!("Reading ZIP {apk_path}"))?;

    let metadata_text = {
        let Ok(mut entry) = archive.by_name(METADATA_ENTRY) else {
            return Ok(None);
        };
        let mut buf = String::new();
        entry
            .read_to_string(&mut buf)
            .context("Reading Superpack metadata.txt")?;
        buf
    };

    let mut ext: Option<String> = None;
    for raw_line in metadata_text.lines() {
        let line = raw_line.trim();
        if line == ".root_relative" {
            return Ok(None);
        }
        if let Some(rest) = line.strip_prefix(".superpack_extension") {
            let candidate = rest.trim();
            if !candidate.is_empty()
                && candidate.len() <= 8
                && candidate.chars().all(|c| c.is_ascii_alphanumeric())
            {
                ext = Some(candidate.to_string());
            }
        }
    }

    let Some(ext) = ext else {
        return Ok(None);
    };
    let store_entry = format!("assets/secondary-program-dex-jars/store-0.dex.{ext}");
    if archive.by_name(&store_entry).is_err() {
        return Ok(None);
    }
    let has_arm64_lib = archive.by_name(ARM64_SUPERPACK_LIB).is_ok();
    Ok(Some(SuperpackInfo {
        store_entry,
        has_arm64_lib,
    }))
}

fn extract_zip_entry(apk_path: &str, entry_name: &str, dest: &Path) -> Result<()> {
    let file = File::open(apk_path).with_context(|| format!("Opening {apk_path}"))?;
    let mut archive = ZipArchive::new(file)?;
    let mut entry = archive
        .by_name(entry_name)
        .with_context(|| format!("Missing ZIP entry {entry_name} in {apk_path}"))?;
    let mut out = File::create(dest).with_context(|| format!("Creating {}", dest.display()))?;
    io::copy(&mut entry, &mut out)?;
    Ok(())
}

fn extract_archive_bytes(bytes: &[u8], dest_dir: &Path) -> Result<PathBuf> {
    let cursor = io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).context("Opening HushFacebook source archive")?;
    let mut top_root: Option<PathBuf> = None;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel_path) = entry.enclosed_name() else {
            continue;
        };
        if top_root.is_none() {
            if let Some(first) = rel_path.components().next() {
                top_root = Some(dest_dir.join(first.as_os_str()));
            }
        }
        let out_path = dest_dir.join(&rel_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&out_path)?;
            io::copy(&mut entry, &mut out)?;
        }
    }

    top_root.context("HushFacebook source archive was empty")
}

async fn ensure_superpack_desktop(client: &Client, tools_dir: &str) -> Result<PathBuf> {
    let base_dir = Path::new(tools_dir).join("superpack-desktop");
    let lib_dir = base_dir.join("lib");
    let ready_marker = lib_dir.join(".ready");
    let main_jar = lib_dir.join("superpack-desktop.jar");

    if ready_marker.is_file() && main_jar.is_file() {
        return Ok(lib_dir);
    }

    let _guard = build_lock().lock().await;
    if ready_marker.is_file() && main_jar.is_file() {
        return Ok(lib_dir);
    }

    info!("superpack: downloading and building HushFacebook superpack-desktop decoder...");
    let resp = client
        .get(HUSHFACEBOOK_ARCHIVE_URL)
        .send()
        .await
        .context("Downloading HushFacebook source archive")?
        .error_for_status()
        .context("HushFacebook source archive HTTP error")?;
    let bytes = resp.bytes().await?;

    let tmp_dir = tempfile::tempdir().context("Creating temp dir for superpack-desktop build")?;
    let tmp_path = tmp_dir.path().to_path_buf();
    let repo_root =
        tokio::task::spawn_blocking(move || extract_archive_bytes(&bytes, &tmp_path)).await??;

    let project_dir = repo_root.join("scripts").join("superpack-desktop");
    if !project_dir.join("build.gradle.kts").is_file() {
        anyhow::bail!(
            "scripts/superpack-desktop/build.gradle.kts not found in HushFacebook archive"
        );
    }

    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("cmd");
        c.args(["/C", "gradlew.bat"]);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("gradlew");
        c
    };
    cmd.current_dir(&repo_root)
        .arg("-p")
        .arg(&project_dir)
        .args([
            "--no-daemon",
            "--quiet",
            "--dependency-verification=off",
            "installDist",
        ]);

    let output = cmd
        .output()
        .await
        .context("Running Gradle for superpack-desktop")?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Gradle installDist failed for superpack-desktop ({}): {stdout}\n{stderr}",
            output.status
        );
    }

    let built_lib = project_dir
        .join("build")
        .join("install")
        .join("superpack-desktop")
        .join("lib");
    if !built_lib.join("superpack-desktop.jar").is_file() {
        anyhow::bail!(
            "superpack-desktop.jar missing after Gradle build at {}",
            built_lib.display()
        );
    }

    tokio::fs::create_dir_all(&lib_dir).await?;
    let mut rd = tokio::fs::read_dir(&built_lib).await?;
    while let Some(entry) = rd.next_entry().await? {
        let src = entry.path();
        if src.extension().and_then(|e| e.to_str()) == Some("jar") {
            let dst = lib_dir.join(entry.file_name());
            tokio::fs::copy(&src, &dst).await?;
        }
    }
    tokio::fs::write(&ready_marker, b"ok\n").await?;
    info!(
        "superpack: superpack-desktop decoder cached at {}",
        lib_dir.display()
    );
    Ok(lib_dir)
}

fn collect_classpath(lib_dir: &Path) -> Result<String> {
    let mut jars = Vec::new();
    for entry in std::fs::read_dir(lib_dir)? {
        let p = entry?.path();
        if p.extension().and_then(|e| e.to_str()) == Some("jar") {
            jars.push(p.to_string_lossy().to_string());
        }
    }
    jars.sort();
    if jars.is_empty() {
        anyhow::bail!("No JARs found in {}", lib_dir.display());
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    Ok(jars.join(sep))
}

async fn supports_enable_native_access() -> bool {
    tokio::process::Command::new("java")
        .args(["--enable-native-access=ALL-UNNAMED", "-version"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Unpacks Superpack-compressed secondary DEXes in-place if the APK contains `store-0.dex.<ext>`.
pub(crate) async fn unpack_if_needed(
    cfg: &Config,
    id: &str,
    apk_path: &str,
    client: &Client,
) -> Result<()> {
    let apk_owned = apk_path.to_string();
    let Some(info) = tokio::task::spawn_blocking(move || detect_superpack(&apk_owned)).await??
    else {
        return Ok(());
    };

    if !info.has_arm64_lib {
        anyhow::bail!(
            "{id}: APK contains Superpack store ({}) without {ARM64_SUPERPACK_LIB}; desktop unpacking requires arm64-v8a",
            info.store_entry
        );
    }

    info!(
        "{id}: detected Superpack store ({}); unpacking DEX files before patching...",
        info.store_entry
    );

    let lib_dir = ensure_superpack_desktop(client, &cfg.build.tools_dir).await?;
    let classpath = collect_classpath(&lib_dir)?;

    let work_dir = tempfile::tempdir().context("Creating Superpack work directory")?;
    let lib_file = work_dir.path().join("libsuperpack-jni.so");
    let store_file = work_dir.path().join("store.dex.spo");
    let meta_file = work_dir.path().join("metadata.txt");
    let decoded_dir = work_dir.path().join("decoded");
    tokio::fs::create_dir_all(&decoded_dir).await?;

    let apk_clone = apk_path.to_string();
    let store_entry = info.store_entry.clone();
    let lib_c = lib_file.clone();
    let store_c = store_file.clone();
    let meta_c = meta_file.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        extract_zip_entry(&apk_clone, ARM64_SUPERPACK_LIB, &lib_c)?;
        extract_zip_entry(&apk_clone, &store_entry, &store_c)?;
        extract_zip_entry(&apk_clone, METADATA_ENTRY, &meta_c)?;
        Ok(())
    })
    .await??;

    let mut decode_cmd = tokio::process::Command::new("java");
    decode_cmd.arg("-Xmx1g");
    if supports_enable_native_access().await {
        decode_cmd.arg("--enable-native-access=ALL-UNNAMED");
    }
    decode_cmd
        .args([
            "-cp",
            &classpath,
            "hushfacebook.superpack.desktop.DesktopDecode",
        ])
        .arg(&lib_file)
        .arg(&store_file)
        .arg(&meta_file)
        .arg(&decoded_dir);

    let decode_out = decode_cmd.output().await.context("Running DesktopDecode")?;
    let decode_stdout = String::from_utf8_lossy(&decode_out.stdout);
    let decode_stderr = String::from_utf8_lossy(&decode_out.stderr);
    let verified_line = decode_stdout
        .lines()
        .map(str::trim)
        .rfind(|l| l.starts_with("VERIFIED ") || l.starts_with("REFUSED "));

    if !decode_out.status.success() || !verified_line.is_some_and(|l| l.starts_with("VERIFIED ")) {
        anyhow::bail!(
            "{id}: Superpack DesktopDecode failed ({}): {}\n{decode_stdout}\n{decode_stderr}",
            decode_out.status,
            verified_line.unwrap_or("no verdict")
        );
    }
    if let Some(line) = verified_line {
        info!("{id}: superpack {line}");
    }

    let storeless_tmp = format!("{apk_path}.storeless.tmp");
    let storeless_out = tokio::process::Command::new("java")
        .args([
            "-Xmx2g",
            "-cp",
            &classpath,
            "hushfacebook.superpack.StorelessApk",
        ])
        .arg(apk_path)
        .arg(&decoded_dir)
        .arg(&storeless_tmp)
        .output()
        .await
        .context("Running StorelessApk")?;

    if !storeless_out.status.success() {
        let _ = tokio::fs::remove_file(&storeless_tmp).await;
        let stdout = String::from_utf8_lossy(&storeless_out.stdout);
        let stderr = String::from_utf8_lossy(&storeless_out.stderr);
        anyhow::bail!(
            "{id}: StorelessApk failed ({}): {stdout}\n{stderr}",
            storeless_out.status
        );
    }

    let stdout = String::from_utf8_lossy(&storeless_out.stdout);
    for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        info!("{id}: {line}");
    }

    tokio::fs::rename(&storeless_tmp, apk_path)
        .await
        .with_context(|| format!("Replacing {apk_path} with storeless APK"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn write_test_apk(entries: &[(&str, &[u8])]) -> tempfile::NamedTempFile {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let mut zw = ZipWriter::new(tmp.reopen().unwrap());
        let opts = SimpleFileOptions::default();
        for (name, data) in entries {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(data).unwrap();
        }
        zw.finish().unwrap();
        tmp
    }

    #[test]
    fn detect_superpack_ignores_normal_apk() {
        let apk = write_test_apk(&[("classes.dex", b"dex")]);
        let res = detect_superpack(apk.path().to_str().unwrap()).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn detect_superpack_ignores_root_relative_apk() {
        let apk = write_test_apk(&[
            (METADATA_ENTRY, b".root_relative\nsecondary-1.dex 0000 0\n"),
            ("assets/secondary-program-dex-jars/store-0.dex.spo", b"spo"),
        ]);
        let res = detect_superpack(apk.path().to_str().unwrap()).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn detect_superpack_finds_store_and_arm64_lib() {
        let apk = write_test_apk(&[
            (
                METADATA_ENTRY,
                b".superpack_extension spo\nsecondary-1.dex 0000 0\n",
            ),
            ("assets/secondary-program-dex-jars/store-0.dex.spo", b"spo"),
            (ARM64_SUPERPACK_LIB, b"elf"),
        ]);
        let res = detect_superpack(apk.path().to_str().unwrap()).unwrap();
        assert_eq!(
            res,
            Some(SuperpackInfo {
                store_entry: "assets/secondary-program-dex-jars/store-0.dex.spo".to_string(),
                has_arm64_lib: true,
            })
        );
    }

    #[test]
    fn detect_superpack_reports_missing_arm64_lib() {
        let apk = write_test_apk(&[
            (
                METADATA_ENTRY,
                b".superpack_extension spo\nsecondary-1.dex 0000 0\n",
            ),
            ("assets/secondary-program-dex-jars/store-0.dex.spo", b"spo"),
        ]);
        let res = detect_superpack(apk.path().to_str().unwrap()).unwrap();
        assert_eq!(
            res,
            Some(SuperpackInfo {
                store_entry: "assets/secondary-program-dex-jars/store-0.dex.spo".to_string(),
                has_arm64_lib: false,
            })
        );
    }
}
