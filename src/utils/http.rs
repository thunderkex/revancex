use crate::config::Config;
use anyhow::Result;
use reqwest::Client;
use std::path::Path;
use tracing::info;

use super::tools::is_valid_jar_or_bundle_async;

static STATIC_USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0",
];

pub const DEFAULT_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

pub fn random_user_agent() -> &'static str {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let idx = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % STATIC_USER_AGENTS.len();
    STATIC_USER_AGENTS[idx]
}

pub fn make_github_client(cfg: &Config) -> Result<Client> {
    let timeout = std::time::Duration::from_secs(cfg.build.download_timeout_secs);
    let ver = env!("CARGO_PKG_VERSION");
    let client = Client::builder()
        .timeout(timeout)
        .user_agent(format!("revancex/{ver}"))
        .build()?;
    Ok(client)
}

pub fn make_no_redirect_client(timeout_secs: u64) -> Result<Client> {
    Client::builder()
        .http1_only()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(Into::into)
}

pub fn make_browser_client(cfg: &Config) -> Result<Client> {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, USER_AGENT};
    let mut headers = HeaderMap::new();
    let ua = random_user_agent();
    let val = HeaderValue::from_str(ua).unwrap_or_else(|_| HeaderValue::from_static(DEFAULT_UA));
    headers.insert(USER_AGENT, val);
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua"),
        HeaderValue::from_static(
            "\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"",
        ),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-mobile"),
        HeaderValue::from_static("?0"),
    );
    headers.insert(
        HeaderName::from_static("sec-ch-ua-platform"),
        HeaderValue::from_static("\"Windows\""),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-dest"),
        HeaderValue::from_static("document"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-mode"),
        HeaderValue::from_static("navigate"),
    );
    headers.insert(
        HeaderName::from_static("sec-fetch-site"),
        HeaderValue::from_static("none"),
    );
    headers.insert(
        HeaderName::from_static("upgrade-insecure-requests"),
        HeaderValue::from_static("1"),
    );

    let timeout = std::time::Duration::from_secs(cfg.build.download_timeout_secs);
    Client::builder()
        .cookie_store(true)
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(std::time::Duration::from_secs(10))
        .read_timeout(timeout)
        .timeout(timeout)
        .build()
        .map_err(Into::into)
}

pub async fn download_file_with_client(
    client: &Client,
    url: &str,
    dest: &str,
    retries: u32,
    referer: Option<&str>,
) -> Result<String> {
    if let Some(parent) = Path::new(dest).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    for attempt in 0..=retries {
        match try_download(client, url, dest, referer).await {
            Ok(_) => return Ok(dest.to_string()),
            Err(e) if attempt < retries => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("403")
                    || err_str.contains("forbidden")
                    || err_str.contains("turnstile")
                    || err_str.contains("challenge")
                    || err_str.contains("blocked")
                {
                    return Err(e);
                }
                tracing::warn!("Download attempt {attempt} failed: {e}, retrying...");
                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

static ARIA2_AVAILABLE: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();

async fn is_aria2_available() -> bool {
    *ARIA2_AVAILABLE
        .get_or_init(|| async {
            let res = tokio::process::Command::new("aria2c")
                .arg("--version")
                .output()
                .await;
            let available = res.is_ok_and(|o| o.status.success());
            if available {
                info!("Transport: aria2c detected and enabled for multi-connection downloads");
            } else {
                info!("Transport: aria2c not available, falling back to HTTP client");
            }
            available
        })
        .await
}

fn verify_downloaded_zip_header(path: &str) -> Result<()> {
    if !path.ends_with(".apk") && !path.ends_with("-input.apk") {
        return Ok(());
    }
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut header = [0u8; 4];
    if file.read_exact(&mut header).is_err() || header != [0x50, 0x4B, 0x03, 0x04] {
        anyhow::bail!(
            "Download integrity failure: {path} is not a valid ZIP/APK archive (bad magic bytes)"
        );
    }
    Ok(())
}

async fn try_download(client: &Client, url: &str, dest: &str, referer: Option<&str>) -> Result<()> {
    if let Some(file_path) = url.strip_prefix("file://") {
        let clean = file_path.trim_start_matches('/');
        let local_path = if (clean.len() >= 2 && clean.chars().nth(1) == Some(':'))
            || tokio::fs::try_exists(clean).await.unwrap_or(false)
        {
            clean
        } else {
            file_path
        };
        if let Some(parent) = std::path::Path::new(dest).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(local_path, dest).await?;
        return Ok(());
    }

    let path = Path::new(dest);
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download.tmp");
    let ua = random_user_agent();

    if is_aria2_available().await
        && try_aria2_download(url, dest, dir, filename, ua, referer).await?
    {
        return Ok(());
    }

    stream_http_download(client, url, dest, ua, referer).await
}

async fn try_aria2_download(
    url: &str,
    dest: &str,
    dir: &Path,
    filename: &str,
    ua: &str,
    referer: Option<&str>,
) -> Result<bool> {
    let referer_header = referer.map(|r| format!("Referer: {r}"));
    let mut args = vec![
        "-x",
        "16",
        "-s",
        "16",
        "-j",
        "16",
        "-k",
        "1M",
        "-U",
        ua,
        "--file-allocation=none",
        "--allow-overwrite=true",
        "--auto-file-renaming=false",
        "--summary-interval=0",
    ];
    if let Some(ref h) = referer_header {
        args.push("--header");
        args.push(h);
    }
    args.extend(["-d", dir.to_str().unwrap_or("."), "-o", filename, url]);

    let aria_res = tokio::process::Command::new("aria2c")
        .args(&args)
        .output()
        .await;

    if let Ok(out) = aria_res {
        crate::ui::tool_log(
            "downloads",
            &format!("aria2c {url}"),
            &format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        );
        if out.status.success() {
            if let Ok(meta) = tokio::fs::metadata(dest).await {
                if meta.len() > 0 {
                    let d = dest.to_string();
                    tokio::task::spawn_blocking(move || verify_downloaded_zip_header(&d))
                        .await
                        .map_err(|e| anyhow::anyhow!("verify task panicked: {e}"))??;
                    info!("aria2 accelerated download complete: {dest}");
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

async fn stream_http_download(
    client: &Client,
    url: &str,
    dest: &str,
    ua: &str,
    referer: Option<&str>,
) -> Result<()> {
    use futures::StreamExt;
    info!("Downloading {url} → {dest}");
    let send_request = |ref_hdr: Option<&str>| {
        let mut req = client
            .get(url)
            .timeout(std::time::Duration::from_secs(600))
            .header(reqwest::header::USER_AGENT, ua)
            .header(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
            )
            .header(reqwest::header::ACCEPT_LANGUAGE, "en-US,en;q=0.9")
            .header("sec-ch-ua", "\"Chromium\";v=\"130\", \"Google Chrome\";v=\"130\", \"Not?A_Brand\";v=\"99\"")
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", "\"Windows\"")
            .header("sec-fetch-dest", "document")
            .header("sec-fetch-mode", "navigate")
            .header("sec-fetch-site", "cross-site")
            .header("upgrade-insecure-requests", "1");
        if let Some(ref_val) = ref_hdr {
            req = req.header(reqwest::header::REFERER, ref_val);
        }
        req
    };

    let mut resp = send_request(referer).send().await?;
    if resp.status() == reqwest::StatusCode::FORBIDDEN && referer.is_some() {
        tracing::warn!("403 Forbidden with Referer {referer:?} on {url}, retrying without Referer / base origin");
        let fallback_ref = if url.contains("apkpure") {
            Some("https://apkpure.com/")
        } else {
            None
        };
        let retry_resp = send_request(fallback_ref).send().await?;
        if retry_resp.status().is_success() {
            resp = retry_resp;
        } else if fallback_ref.is_some() {
            let bare_resp = send_request(None).send().await?;
            resp = if bare_resp.status().is_success() {
                bare_resp
            } else {
                retry_resp
            };
        } else {
            resp = retry_resp;
        }
    }

    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!(
            "HTTP status client error (403 Forbidden) for url ({url}) (Referer: {referer:?})"
        );
    }
    let resp = resp.error_for_status()?;
    let tmp_dest = format!("{dest}.download.tmp");
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&tmp_dest).await?;
    let stall_timeout = std::time::Duration::from_secs(20);
    loop {
        match tokio::time::timeout(stall_timeout, stream.next()).await {
            Ok(Some(chunk_res)) => {
                let chunk = chunk_res?;
                if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await {
                    let _ = tokio::fs::remove_file(&tmp_dest).await;
                    return Err(e.into());
                }
            }
            Ok(None) => break,
            Err(_) => {
                let _ = tokio::fs::remove_file(&tmp_dest).await;
                anyhow::bail!("Download stream stalled (no data received for >20s) for {url}");
            }
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    drop(file);

    let verify_target = tmp_dest.clone();
    let verify_res =
        tokio::task::spawn_blocking(move || verify_downloaded_zip_header(&verify_target))
            .await
            .map_err(|e| anyhow::anyhow!("verify task panicked: {e}"))?;
    if let Err(e) = verify_res {
        let _ = tokio::fs::remove_file(&tmp_dest).await;
        return Err(e);
    }

    if let Err(e) = tokio::fs::rename(&tmp_dest, dest).await {
        let _ = tokio::fs::remove_file(&tmp_dest).await;
        return Err(e.into());
    }

    Ok(())
}

pub async fn ensure_apkeditor_jar(tools_dir: &str, client: &Client) -> Result<String> {
    let jar_candidates = [
        format!("{tools_dir}/APKEditor.jar"),
        "tools/APKEditor.jar".to_string(),
        "APKEditor.jar".to_string(),
    ];
    for cand in &jar_candidates {
        if tokio::fs::try_exists(cand).await.unwrap_or(false)
            && is_valid_jar_or_bundle_async(cand).await
        {
            return Ok(cand.clone());
        }
    }
    let dest = format!("{tools_dir}/APKEditor.jar");
    if let Some(parent) = Path::new(&dest).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let url = "https://github.com/REAndroid/APKEditor/releases/download/V1.4.9/APKEditor-1.4.9.jar";
    download_file_with_client(client, url, &dest, 2, None).await?;
    Ok(dest)
}
