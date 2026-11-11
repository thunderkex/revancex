use anyhow::Result;
use reqwest::Client;
use std::path::{Path, PathBuf};
use tracing::info;

pub fn is_ip_reputation_blocked(status: u16, body: &str) -> bool {
    (status == 403 && !is_turnstile_blocked(status, &reqwest::header::HeaderMap::new(), body))
        || body.contains("error code: 1020")
        || body.contains("Error 1020")
        || body.contains("Access denied |")
        || body.contains("<title>Access denied")
}

pub fn is_turnstile_blocked(status: u16, headers: &reqwest::header::HeaderMap, body: &str) -> bool {
    let _ = status;
    if headers
        .get("cf-mitigated")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("challenge"))
        .unwrap_or(false)
    {
        return true;
    }
    let snippet = &body[..body.len().min(5120)];
    snippet.contains("cf-turnstile")
        || snippet.contains("challenges.cloudflare.com")
        || snippet.contains("cf_chl_opt")
        || snippet.contains("Just a moment...")
        || snippet.contains("cf-chl-bypass")
        || snippet.contains("Checking your browser")
        || snippet.contains("cloudflare-static")
        || snippet.contains("verify you are human")
}

pub(crate) async fn fetch_page_html(
    client: &Client,
    url: &str,
    fetcher: Option<&crate::net::Fetcher>,
) -> Result<(String, reqwest::header::HeaderMap)> {
    if let Some(f) = fetcher {
        match f.get(url).await {
            Ok(body) => {
                return Ok((body, reqwest::header::HeaderMap::new()));
            }
            Err(crate::net::FetchError::Blocked { host, kind }) => {
                anyhow::bail!("Blocked by Cloudflare/403 ({kind}) on {host} for {url}");
            }
            Err(crate::net::FetchError::Http { status, .. })
                if status == reqwest::StatusCode::FORBIDDEN =>
            {
                anyhow::bail!("HTTP 403 Forbidden on {url}");
            }
            Err(crate::net::FetchError::Http { status, .. }) => {
                tracing::debug!("fetcher: non-challenge HTTP {status} for {url} — direct fallback");
            }
            Err(e) => {
                tracing::debug!("fetcher get failed for {url}: {e} — direct fallback");
            }
        }
    }
    let resp = client
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await?;
    let headers = resp.headers().clone();
    let status = resp.status();
    if status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("HTTP 403 Forbidden on {url}");
    }
    let body = resp.text().await?;
    if !status.is_success() {
        tracing::info!("direct GET {url} -> {status} ({} bytes)", body.len());
    }
    Ok((body, headers))
}

pub async fn download_and_extract_if_bundle(
    client: &Client,
    url: &str,
    dest_apk_path: &str,
    referer: Option<&str>,
) -> Result<()> {
    if let Some(parent) = Path::new(dest_apk_path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let temp_dl = format!("{dest_apk_path}.tmp");
    crate::utils::download_file_with_client(client, url, &temp_dl, 3, referer).await?;

    let td = temp_dl.clone();
    let (is_zip, is_split) = tokio::task::spawn_blocking(move || {
        let z = is_zip_file(&td);
        let s = if z { is_split_bundle(&td) } else { false };
        (z, s)
    })
    .await
    .unwrap_or((false, false));

    if is_zip {
        info!("Checking if downloaded file is a ZIP bundle (.apkm/.xapk/split) or APK...");
        if is_split {
            info!("Detected split APK bundle. Merging splits using APKEditor...");
            if let Ok(()) = merge_split_apk_bundle(client, &temp_dl, dest_apk_path).await {
                let _ = tokio::fs::remove_file(&temp_dl).await;
                let d = dest_apk_path.to_string();
                let _ = tokio::task::spawn_blocking(move || strip_split_requirements_from_apk(&d))
                    .await;
                return Ok(());
            } else {
                tracing::warn!("APKEditor merge failed, falling back to base APK extraction");
            }
        }
        let td2 = temp_dl.clone();
        let d2 = dest_apk_path.to_string();
        let base_ok = tokio::task::spawn_blocking(move || extract_base_apk_from_zip(&td2, &d2))
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false);
        if base_ok {
            let _ = tokio::fs::remove_file(&temp_dl).await;
            let d = dest_apk_path.to_string();
            let _ =
                tokio::task::spawn_blocking(move || strip_split_requirements_from_apk(&d)).await;
            return Ok(());
        }
    }

    tokio::fs::rename(&temp_dl, dest_apk_path).await?;
    let d = dest_apk_path.to_string();
    let _ = tokio::task::spawn_blocking(move || strip_split_requirements_from_apk(&d)).await;
    Ok(())
}

pub(crate) fn is_zip_file(path: &str) -> bool {
    if let Ok(mut file) = std::fs::File::open(path) {
        use std::io::Read;
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_ok() {
            return magic == [0x50, 0x4B, 0x03, 0x04];
        }
    }
    false
}

pub(crate) fn is_split_bundle(zip_path: &str) -> bool {
    let file = match std::fs::File::open(zip_path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(_) => return false,
    };
    let mut apk_count = 0;
    for i in 0..archive.len() {
        if let Ok(item) = archive.by_index(i) {
            if let Ok(name_cow) = item.name() {
                let name = name_cow.to_lowercase();
                if name.ends_with(".apk") {
                    apk_count += 1;
                    if apk_count > 1 || name.starts_with("split_") || name.contains(".config.") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub(crate) async fn merge_split_apk_bundle(
    client: &Client,
    bundle_path: &str,
    dest_apk: &str,
) -> Result<()> {
    let jar_candidates = ["tools/APKEditor.jar", "APKEditor.jar"];
    let mut found_jar = None;
    for c in jar_candidates {
        if tokio::fs::try_exists(c).await.unwrap_or(false) {
            found_jar = Some(PathBuf::from(c));
            break;
        }
    }
    let jar_path = match found_jar {
        Some(p) => p,
        None => {
            info!("APKEditor.jar not found, attempting auto-download...");
            match crate::utils::ensure_apkeditor_jar("tools", client).await {
                Ok(p) => PathBuf::from(p),
                Err(e) => anyhow::bail!("APKEditor.jar not found and could not download: {e}"),
            }
        }
    };

    info!("Merging split APK bundle {bundle_path} into {dest_apk} using APKEditor...");
    let status = tokio::process::Command::new("java")
        .args([
            "-jar",
            jar_path.to_str().unwrap_or("tools/APKEditor.jar"),
            "m",
            "-i",
            bundle_path,
            "-o",
            dest_apk,
            "-f",
        ])
        .status()
        .await?;

    if !status.success() {
        anyhow::bail!(
            "APKEditor failed to merge split bundle (exit: {:?})",
            status.code()
        );
    }
    Ok(())
}

pub(crate) fn extract_base_apk_from_zip(zip_path: &str, dest: &str) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut best_index: Option<usize> = None;
    let mut max_size: u64 = 0;

    for i in 0..archive.len() {
        let item = archive.by_index(i)?;
        let name = item.name()?.to_lowercase();
        if name.ends_with(".apk") {
            if name.ends_with("base.apk") {
                best_index = Some(i);
                break;
            }
            if item.size() > max_size {
                max_size = item.size();
                best_index = Some(i);
            }
        }
    }

    let idx = best_index.ok_or_else(|| anyhow::anyhow!("No .apk found inside ZIP bundle"))?;
    let mut entry = archive.by_index(idx)?;
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(&mut entry, &mut out)?;

    info!("Extracted {} bytes to {dest}", entry.size());
    Ok(())
}

pub fn strip_split_requirements_from_apk(apk_path: &str) -> Result<bool> {
    use std::io::{Read, Write};
    use zip::write::SimpleFileOptions;

    let src_file = std::fs::File::open(apk_path)?;
    let mut archive = zip::ZipArchive::new(src_file)?;

    let mut manifest_bytes: Option<Vec<u8>> = None;
    let mut other_entries: Vec<(String, Vec<u8>, zip::CompressionMethod)> = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.to_string();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;

        let compression = entry.compression();
        if name == "AndroidManifest.xml" {
            manifest_bytes = Some(data);
        } else {
            other_entries.push((name, data, compression));
        }
    }

    let manifest = match manifest_bytes {
        Some(m) => m,
        None => return Ok(false),
    };

    let sanitized_manifest = match sanitize_split_manifest(&manifest) {
        Some(m) => m,
        None => return Ok(false),
    };

    let temp_out = format!("{apk_path}.nosplit.tmp");
    let out_file = std::fs::File::create(&temp_out)?;
    let mut zip_out = zip::ZipWriter::new(out_file);
    let def_opts =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip_out.start_file("AndroidManifest.xml", def_opts)?;
    zip_out.write_all(&sanitized_manifest)?;

    for (name, data, compression) in other_entries {
        let method = if name == "resources.arsc" {
            zip::CompressionMethod::Stored
        } else {
            compression
        };
        let opts = SimpleFileOptions::default().compression_method(method);
        zip_out.start_file(&name, opts)?;
        zip_out.write_all(&data)?;
    }

    zip_out.finish()?;
    std::fs::remove_file(apk_path)?;
    std::fs::rename(&temp_out, apk_path)?;

    info!(
        "{apk_path}: stripped requiredSplitTypes and split requirements from AndroidManifest.xml"
    );
    Ok(true)
}

fn sanitize_split_manifest(manifest: &[u8]) -> Option<Vec<u8>> {
    let target_resid = 0x0101064eu32.to_le_bytes();
    let mut has_resid = false;
    for window in manifest.windows(4) {
        if window == target_resid {
            has_resid = true;
            break;
        }
    }
    if !has_resid {
        return None;
    }

    let new_manifest = manifest.to_vec();

    let mut resmap_pos = None;
    for (i, window) in new_manifest.windows(4).enumerate() {
        if window == [0x80, 0x01, 0x08, 0x00] {
            resmap_pos = Some(i);
            break;
        }
    }

    let pos = resmap_pos?;
    let size = u32::from_le_bytes(new_manifest[pos + 4..pos + 8].try_into().ok()?) as usize;
    let map_slice = &new_manifest[pos + 8..pos + size];
    let mut target_indices = Vec::new();
    for (idx, chunk) in map_slice.chunks_exact(4).enumerate() {
        let resid = u32::from_le_bytes(chunk.try_into().unwrap());
        if resid == 0x0101064e || resid == 0x0101064f {
            target_indices.push(idx);
        }
    }

    if target_indices.is_empty() {
        return None;
    }

    let mut elem_pos = None;
    for (i, window) in new_manifest.windows(4).enumerate() {
        if window == [0x02, 0x01, 0x10, 0x00] && i + 36 <= new_manifest.len() {
            let chunk_size =
                u32::from_le_bytes(new_manifest[i + 4..i + 8].try_into().unwrap()) as usize;
            let attr_count =
                u16::from_le_bytes(new_manifest[i + 28..i + 30].try_into().unwrap()) as usize;
            if attr_count > 0
                && attr_count < 200
                && chunk_size == 36 + attr_count * 20
                && i + chunk_size <= new_manifest.len()
            {
                let attrs_start = i + 36;
                let mut contains_target = false;
                for a in 0..attr_count {
                    let a_off = attrs_start + a * 20;
                    let name_idx =
                        i32::from_le_bytes(new_manifest[a_off + 4..a_off + 8].try_into().unwrap())
                            as usize;
                    if target_indices.contains(&name_idx) {
                        contains_target = true;
                        break;
                    }
                }
                if contains_target {
                    elem_pos = Some((i, chunk_size, attr_count));
                    break;
                }
            }
        }
    }

    let (pos, chunk_size, attr_count) = elem_pos?;
    let attrs_start = pos + 36;
    let mut new_attrs = Vec::new();
    let mut removed = 0;

    for a in 0..attr_count {
        let a_off = attrs_start + a * 20;
        let name_idx =
            i32::from_le_bytes(new_manifest[a_off + 4..a_off + 8].try_into().unwrap()) as usize;
        if target_indices.contains(&name_idx) {
            removed += 1;
        } else {
            new_attrs.extend_from_slice(&new_manifest[a_off..a_off + 20]);
        }
    }

    if removed == 0 {
        return None;
    }

    let mut result = Vec::new();
    result.extend_from_slice(&new_manifest[..pos]);

    let mut elem_hdr = new_manifest[pos..pos + 36].to_vec();
    let new_chunk_size = (36 + (attr_count - removed) * 20) as u32;
    elem_hdr[4..8].copy_from_slice(&new_chunk_size.to_le_bytes());
    elem_hdr[28..30].copy_from_slice(&((attr_count - removed) as u16).to_le_bytes());
    result.extend_from_slice(&elem_hdr);
    result.extend_from_slice(&new_attrs);
    result.extend_from_slice(&new_manifest[pos + chunk_size..]);

    let total_len = result.len() as u32;
    result[4..8].copy_from_slice(&total_len.to_le_bytes());

    let old_s_u16: Vec<u8> = "com.android.vending.splits"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let new_s_u16: Vec<u8> = "com.android.vending.spl_ds"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();

    for pos in 0..result.len().saturating_sub(old_s_u16.len()) {
        if result[pos..pos + old_s_u16.len()] == old_s_u16[..] {
            result[pos..pos + new_s_u16.len()].copy_from_slice(&new_s_u16);
        }
    }

    let old_s_u8 = b"com.android.vending.splits";
    let new_s_u8 = b"com.android.vending.spl_ds";
    for pos in 0..result.len().saturating_sub(old_s_u8.len()) {
        if result[pos..pos + old_s_u8.len()] == old_s_u8[..] {
            result[pos..pos + new_s_u8.len()].copy_from_slice(new_s_u8);
        }
    }

    Some(result)
}
