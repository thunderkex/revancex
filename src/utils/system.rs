use crate::orchestrator::BuildSummary;
use anyhow::{Context, Result};
use std::path::Path;
use tracing::{info, warn};

/// Acquire a `Mutex` lock, recovering from poisoning instead of propagating panics.
pub(crate) fn lock_unpoisoned<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| {
        warn!("Mutex was poisoned; recovering inner state");
        poisoned.into_inner()
    })
}

/// Log an I/O error on non-fatal persistence/cleanup operations instead of silently discarding it.
pub(crate) fn warn_io<T>(res: std::result::Result<T, impl std::fmt::Display>, context: &str) {
    if let Err(e) = res {
        warn!("{context}: {e}");
    }
}

pub fn is_command_available(cmd: &str) -> bool {
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let p = dir.join(cmd);
            if p.is_file() {
                return true;
            }
            if cfg!(windows) && dir.join(format!("{cmd}.exe")).is_file() {
                return true;
            }
        }
    }
    false
}

pub fn get_keystore_password() -> String {
    std::env::var("KEYSTORE_PASSWORD")
        .ok()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && p.len() >= 6)
        .unwrap_or_else(|| "revanced".to_string())
}

pub fn check_java_version(min_version: u32) -> Result<u32> {
    let output = std::process::Command::new("java")
        .arg("-version")
        .output()
        .context("Java not found on PATH — please install JDK/JRE (minimum version required)")?;

    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let major = parse_java_major_version(&text).ok_or_else(|| {
        anyhow::anyhow!("Failed to parse Java major version from 'java -version' output:\n{text}")
    })?;

    if major < min_version {
        anyhow::bail!(
            "Java version {major} is below configured minimum of {min_version} — please upgrade JDK"
        );
    }

    Ok(major)
}

pub fn parse_java_major_version(output: &str) -> Option<u32> {
    for line in output.lines() {
        if let Some(idx) = line.find("version \"") {
            let rest = &line[idx + 9..];
            if let Some(end_quote) = rest.find('"') {
                let ver_str = &rest[..end_quote];
                let parts: Vec<&str> = ver_str.split('.').collect();
                if parts[0] == "1" && parts.len() > 1 {
                    return parts[1].parse().ok();
                } else {
                    return parts[0].parse().ok();
                }
            }
        }
    }
    None
}

pub fn write_build_report(summary: &BuildSummary, path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Creating report directory {}", parent.display()))?;
        }
    }
    let report = crate::report::BuildReport::from_summary(summary);
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(path, &json)
        .with_context(|| format!("Writing build report to {}", path.display()))?;
    info!("Build report written to {}", path.display());

    let md_content = crate::report::generate_markdown_report(&report);
    let md_path = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join("BUILD_REPORT.md"),
        _ => std::path::PathBuf::from("BUILD_REPORT.md"),
    };
    warn_io(
        std::fs::write(&md_path, &md_content),
        &format!("Writing Markdown build report {}", md_path.display()),
    );
    info!(
        "Human-readable build report written to {}",
        md_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_java_major_version() {
        assert_eq!(
            parse_java_major_version("openjdk version \"21.0.12.1\" 2026-08-18 LTS"),
            Some(21)
        );
        assert_eq!(
            parse_java_major_version("java version \"17.0.2\" 2022-01-18 LTS"),
            Some(17)
        );
        assert_eq!(
            parse_java_major_version("java version \"1.8.0_312\""),
            Some(8)
        );
        assert_eq!(parse_java_major_version("invalid version string"), None);
    }
}
