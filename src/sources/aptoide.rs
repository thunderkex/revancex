use super::{ApkSource, Resolved, SourceCtx};
use crate::config::AppConfig;
use crate::sources::common::{download_and_extract_if_bundle, fetch_page_html};
use anyhow::{Context, Result};
use reqwest::Client;
use std::path::PathBuf;
use std::pin::Pin;
use tracing::info;

pub struct AptoideSource;

impl ApkSource for AptoideSource {
    fn name(&self) -> &'static str {
        "aptoide"
    }

    fn resolve<'a>(
        &'a self,
        _ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            if app.package.is_empty() {
                tracing::info!("aptoide skipped: not configured");
                return Ok(None);
            }
            let pseudo = format!("aptoide://{}", app.package);
            Ok(Some(Resolved::simple(
                pseudo,
                version.map(str::to_string),
                arch,
            )))
        })
    }

    fn fetch<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        resolved: &'a Resolved,
        dest: &'a str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<PathBuf>> + Send + 'a>> {
        Box::pin(async move {
            let pkg = resolved
                .url
                .strip_prefix("aptoide://")
                .unwrap_or(&resolved.url);
            fetch_aptoide(
                ctx.client,
                pkg,
                dest,
                &resolved.arch,
                resolved.version_found.as_deref(),
                ctx.fetcher,
            )
            .await?;
            Ok(PathBuf::from(dest))
        })
    }
}

const APTOIDE_API_BASE: &str = "https://ws75.aptoide.com/api/7/";

fn aptoide_cpu_query_suffix(arch: &str) -> String {
    use crate::config::apps::Arch;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    let raw = match Arch::parse(arch) {
        Arch::Arm64V8a => "myCPU=arm64-v8a,armeabi-v7a,armeabi&leanback=0",
        Arch::ArmeabiV7a => "myCPU=armeabi-v7a,armeabi&leanback=0",
        Arch::X86_64 => "myCPU=x86_64,x86&leanback=0",
        Arch::X86 => "myCPU=x86&leanback=0",
        _ => return String::new(),
    };
    format!("&q={}", URL_SAFE_NO_PAD.encode(raw))
}

async fn try_aptoide_website_version(
    client: &Client,
    package: &str,
    dest: &str,
    arch: &str,
    target_norm: &str,
    fetcher: Option<&crate::net::Fetcher>,
) -> Option<String> {
    let normalize = crate::utils::version::normalize_version;
    let site_url = format!("https://en.aptoide.com/app/{package}");
    let html_res = match fetcher {
        Some(f) => fetch_page_html(client, &site_url, Some(f))
            .await
            .map(|(b, _)| b),
        None => match client.get(&site_url).send().await {
            Ok(resp) if resp.status().is_success() => resp.text().await.map_err(Into::into),
            Ok(resp) => Err(anyhow::anyhow!("status {}", resp.status())),
            Err(e) => Err(e.into()),
        },
    };
    let html = html_res.ok()?;
    let start = html.find("{\"props\"")?;
    let json_str = &html[start..];
    let end = json_str.find("</script>").unwrap_or(json_str.len());
    let props = serde_json::from_str::<serde_json::Value>(&json_str[..end]).ok()?;
    let versions = props["pageProps"]["versions"].as_array()?;

    for ver_entry in versions {
        let vername = ver_entry["vername"].as_str().unwrap_or("");
        if normalize(vername) != target_norm {
            continue;
        }
        let path = ver_entry["file"]["path"].as_str().unwrap_or("");
        if path.is_empty() {
            info!("aptoide website: v{vername} found but no download URL (older version)");
            break;
        }
        info!("aptoide website: found {package} v{vername}, downloading {path}");
        if download_and_extract_if_bundle(client, path, dest, Some("https://aptoide.com/"))
            .await
            .is_err()
        {
            return None;
        }
        if arch != "all" && arch != "universal" && !arch.is_empty() {
            let dest_owned = dest.to_string();
            let arch_owned = arch.to_string();
            let arch_mismatch = tokio::task::spawn_blocking(move || {
                let has_any =
                    crate::utils::apk::apk_has_any_native_libs(&dest_owned).unwrap_or(false);
                let has_target =
                    crate::utils::apk::apk_has_arch(&dest_owned, &arch_owned).unwrap_or(false);
                has_any && !has_target
            })
            .await
            .unwrap_or(false);
            if arch_mismatch {
                info!("aptoide website: APK for v{vername} has native libs but none for '{arch}', checking next entry");
                let _ = tokio::fs::remove_file(dest).await;
                continue;
            }
        }
        return Some(dest.to_string());
    }
    None
}

async fn resolve_aptoide_api_download_path(
    client: &Client,
    package: &str,
    target_version: Option<&str>,
    target_norm: Option<&str>,
    q_suffix: &str,
) -> Result<String> {
    let Some(tgt) = target_norm else {
        let search_url =
            format!("{APTOIDE_API_BASE}apps/search?query={package}&limit=1&trusted=true{q_suffix}");
        info!("aptoide: searching latest via {search_url}");
        let resp: serde_json::Value = client
            .get(&search_url)
            .send()
            .await?
            .json()
            .await
            .context("aptoide: failed to parse search JSON")?;
        return Ok(resp["datalist"]["list"][0]["file"]["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("aptoide: no result for {package}"))?
            .to_string());
    };

    let list_url =
        format!("{APTOIDE_API_BASE}listAppVersions?package_name={package}&limit=100{q_suffix}");
    info!("aptoide: listing versions from {list_url}");
    let resp: serde_json::Value = client
        .get(&list_url)
        .send()
        .await?
        .json()
        .await
        .context("aptoide: failed to parse listAppVersions JSON")?;
    let list = resp["list"]
        .as_array()
        .or_else(|| resp["datalist"]["list"].as_array())
        .ok_or_else(|| anyhow::anyhow!("aptoide: no version list for {package}"))?;
    let vercode = list
        .iter()
        .find(|item| {
            let vn = item["file"]["vername"].as_str().unwrap_or("");
            crate::utils::version::normalize_version(vn) == tgt
        })
        .and_then(|item| item["file"]["vercode"].as_i64())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "aptoide: version {:?} (normalized: {tgt}) not found for {package}",
                target_version
            )
        })?;

    let meta_url =
        format!("{APTOIDE_API_BASE}getAppMeta?package_name={package}&vercode={vercode}{q_suffix}");
    info!("aptoide: fetching meta from {meta_url}");
    let meta: serde_json::Value = client
        .get(&meta_url)
        .send()
        .await?
        .json()
        .await
        .context("aptoide: failed to parse getAppMeta JSON")?;
    Ok(meta["data"]["file"]["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("aptoide: no download path in meta for {package} v{tgt}"))?
        .to_string())
}

pub(crate) async fn fetch_aptoide(
    client: &Client,
    package: &str,
    dest: &str,
    arch: &str,
    target_version: Option<&str>,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<String> {
    let q_suffix = aptoide_cpu_query_suffix(arch);
    let target_norm = target_version.map(crate::utils::version::normalize_version);

    if let Some(ref tgt) = target_norm {
        if let Some(path) =
            try_aptoide_website_version(client, package, dest, arch, tgt, fetcher).await
        {
            return Ok(path);
        }
    }

    let download_path = resolve_aptoide_api_download_path(
        client,
        package,
        target_version,
        target_norm.as_deref(),
        &q_suffix,
    )
    .await?;

    info!("aptoide: downloading from {download_path}");
    download_and_extract_if_bundle(client, &download_path, dest, Some("https://aptoide.com/"))
        .await?;
    Ok(dest.to_string())
}
