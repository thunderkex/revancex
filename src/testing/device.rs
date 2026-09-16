use anyhow::{Context, Result};
use std::time::Duration;
use tracing::info;

pub struct AdbDevice {
    pub serial: String,
    pub state: String,
}

pub async fn devices() -> Result<Vec<AdbDevice>> {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::process::Command::new("adb")
            .args(["devices"])
            .output(),
    )
    .await
    .context("adb devices timed out")?
    .context("adb not found on PATH")?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            result.push(AdbDevice {
                serial: parts[0].to_string(),
                state: parts[1].to_string(),
            });
        }
    }
    Ok(result)
}

pub async fn install(serial: &str, apk_path: &str, timeout_secs: u64) -> Result<()> {
    info!("Installing {apk_path} on {serial}...");
    let output = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        tokio::process::Command::new("adb")
            .args([
                "-s",
                serial,
                "install",
                "-r",
                "-g",
                "--no-streaming",
                apk_path,
            ])
            .output(),
    )
    .await
    .context("adb install timed out")?
    .context("adb install failed to launch")?;

    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if !output.status.success() || text.contains("Failure [") {
        let reason = text
            .lines()
            .find(|l| l.contains("Failure [") || l.contains("INSTALL_FAILED_"))
            .unwrap_or(&text)
            .trim();
        anyhow::bail!("Install failed: {reason}");
    }

    Ok(())
}

pub async fn uninstall(serial: &str, package: &str) -> Result<()> {
    let _ = tokio::process::Command::new("adb")
        .args(["-s", serial, "uninstall", package])
        .output()
        .await;
    Ok(())
}

pub async fn start_activity(serial: &str, component: &str) -> Result<()> {
    let output = tokio::process::Command::new("adb")
        .args(["-s", serial, "shell", "am", "start", "-n", component])
        .output()
        .await
        .context("adb shell am start failed")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("am start failed: {err}");
    }
    Ok(())
}

pub async fn pid_of(serial: &str, package: &str) -> Result<Option<u32>> {
    let output = tokio::process::Command::new("adb")
        .args(["-s", serial, "shell", "pidof", package])
        .output()
        .await
        .context("adb shell pidof failed")?;

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(text.split_whitespace().next().and_then(|p| p.parse().ok()))
    }
}
