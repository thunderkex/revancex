use anyhow::Result;
use std::fs::File;
use std::io::Read;
use std::path::Path;

pub fn get_apk_version_name<P: AsRef<Path>>(apk_path: P) -> Result<String> {
    let data = read_manifest(apk_path)?;
    find_manifest_attr(&data, "versionName")
        .ok_or_else(|| anyhow::anyhow!("versionName not found in AndroidManifest.xml"))
}

pub fn get_apk_package_name<P: AsRef<Path>>(apk_path: P) -> Result<String> {
    let p = apk_path.as_ref();
    if let Ok(output) = std::process::Command::new("aapt")
        .args(["dump", "badging", p.to_str().unwrap_or_default()])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if line.starts_with("package: name='") {
                    if let Some(pkg) = line
                        .strip_prefix("package: name='")
                        .and_then(|s| s.split('\'').next())
                    {
                        return Ok(pkg.to_string());
                    }
                }
            }
        }
    }

    let data = read_manifest(p)?;
    find_manifest_attr(&data, "package")
        .ok_or_else(|| anyhow::anyhow!("package name not found in AndroidManifest.xml"))
}

fn read_manifest<P: AsRef<Path>>(apk_path: P) -> Result<Vec<u8>> {
    let file = File::open(apk_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut manifest_file = archive.by_name("AndroidManifest.xml")?;
    let mut data = Vec::new();
    manifest_file.read_to_end(&mut data)?;
    Ok(data)
}

pub fn find_manifest_attr(data: &[u8], attr: &str) -> Option<String> {
    if data.len() < 36 {
        return None;
    }
    let magic = u16::from_le_bytes([data[0], data[1]]);
    if magic != 0x0003 {
        return None;
    }
    let chunk_type = u16::from_le_bytes([data[8], data[9]]);
    if chunk_type != 0x0001 {
        return None;
    }

    let chunk_sz = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let str_count = u32::from_le_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let flags = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);
    let str_start = u32::from_le_bytes([data[32], data[33], data[34], data[35]]) as usize;

    let is_utf8 = (flags & (1 << 8)) != 0;
    let offset_table_start = 36;
    let offset_table_end = offset_table_start + str_count * 4;

    if data.len() < offset_table_end {
        return None;
    }

    let pool_start = 8 + str_start;
    if data.len() < pool_start {
        return None;
    }

    let mut strings: Vec<String> = Vec::with_capacity(str_count);

    for i in 0..str_count {
        let pos = offset_table_start + i * 4;
        let str_off =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let mut idx = pool_start + str_off;

        if idx >= data.len() {
            strings.push(String::new());
            continue;
        }

        if is_utf8 {
            let u16len = data[idx] as usize;
            idx += 1;
            if (u16len & 0x80) != 0 && idx < data.len() {
                idx += 1;
            }
            if idx >= data.len() {
                strings.push(String::new());
                continue;
            }
            let mut u8len = data[idx] as usize;
            idx += 1;
            if (u8len & 0x80) != 0 && idx < data.len() {
                u8len = ((u8len & 0x7F) << 8) | (data[idx] as usize);
                idx += 1;
            }
            if idx + u8len <= data.len() {
                strings.push(String::from_utf8_lossy(&data[idx..idx + u8len]).to_string());
            } else {
                strings.push(String::new());
            }
        } else {
            if idx + 2 > data.len() {
                strings.push(String::new());
                continue;
            }
            let mut u16len = u16::from_le_bytes([data[idx], data[idx + 1]]) as usize;
            idx += 2;
            if (u16len & 0x8000) != 0 && idx + 2 <= data.len() {
                u16len = ((u16len & 0x7FFF) << 16)
                    | (u16::from_le_bytes([data[idx], data[idx + 1]]) as usize);
                idx += 2;
            }
            let byte_len = u16len * 2;
            if idx + byte_len <= data.len() {
                let u16_slice: Vec<u16> = data[idx..idx + byte_len]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                strings.push(String::from_utf16_lossy(&u16_slice));
            } else {
                strings.push(String::new());
            }
        }
    }

    let target_idx = strings.iter().position(|s| s == attr)?;

    let search_start = 8 + chunk_sz;
    if search_start + 8 <= data.len() {
        let mut i = search_start;
        while i + 8 <= data.len() {
            let name_ref =
                u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
            let raw_ref =
                u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as usize;

            if name_ref == target_idx && raw_ref < strings.len() && raw_ref != 0xFFFFFFFF {
                let val = strings[raw_ref].trim().to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
            i += 4;
        }
    }

    None
}
