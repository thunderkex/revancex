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

pub fn dex_entries_crc<P: AsRef<Path>>(
    apk_path: P,
) -> Result<std::collections::BTreeMap<String, u32>> {
    let file = File::open(apk_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut dex_map = std::collections::BTreeMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name.ends_with(".dex") {
            dex_map.insert(name, entry.crc32());
        }
    }
    Ok(dex_map)
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

    let c_hdr_sz = u16::from_le_bytes([data[10], data[11]]) as usize;
    let chunk_sz = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
    let str_count = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let _style_count = u32::from_le_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let flags = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
    let str_start = u32::from_le_bytes([data[28], data[29], data[30], data[31]]) as usize;

    let is_utf8 = (flags & (1 << 8)) != 0;
    let offset_table_start = 8 + c_hdr_sz;
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

    let target_idx = strings.iter().position(|s| s == attr)? as u32;

    let chunks_start = 8 + chunk_sz;
    let mut cur = chunks_start;
    while cur + 8 <= data.len() {
        let chunk_type = u16::from_le_bytes([data[cur], data[cur + 1]]);
        let chunk_hdr_sz = u16::from_le_bytes([data[cur + 2], data[cur + 3]]) as usize;
        let node_chunk_sz =
            u32::from_le_bytes([data[cur + 4], data[cur + 5], data[cur + 6], data[cur + 7]]) as usize;
        if node_chunk_sz < 8 || cur + node_chunk_sz > data.len() {
            break;
        }

        if chunk_type == 0x0102 && cur + 36 <= data.len() {
            let attr_start = u16::from_le_bytes([data[cur + 24], data[cur + 25]]) as usize;
            let attr_sz = u16::from_le_bytes([data[cur + 26], data[cur + 27]]) as usize;
            let attr_cnt = u16::from_le_bytes([data[cur + 28], data[cur + 29]]) as usize;
            let first_attr = cur + chunk_hdr_sz + attr_start;

            if attr_sz >= 20 {
                for a in 0..attr_cnt {
                    let a_off = first_attr + a * attr_sz;
                    if a_off + 20 <= data.len() {
                        let a_name = u32::from_le_bytes([
                            data[a_off + 4],
                            data[a_off + 5],
                            data[a_off + 6],
                            data[a_off + 7],
                        ]);
                        if a_name == target_idx {
                            let a_raw = u32::from_le_bytes([
                                data[a_off + 8],
                                data[a_off + 9],
                                data[a_off + 10],
                                data[a_off + 11],
                            ]);
                            if a_raw != 0xFFFFFFFF && (a_raw as usize) < strings.len() {
                                let val = strings[a_raw as usize].trim().to_string();
                                if !val.is_empty() {
                                    return Some(val);
                                }
                            }
                            let val_type = data[a_off + 15];
                            let val_data = u32::from_le_bytes([
                                data[a_off + 16],
                                data[a_off + 17],
                                data[a_off + 18],
                                data[a_off + 19],
                            ]);
                            if val_type == 0x03 && (val_data as usize) < strings.len() {
                                let val = strings[val_data as usize].trim().to_string();
                                if !val.is_empty() {
                                    return Some(val);
                                }
                            }
                            if val_type == 0x10 || val_type == 0x11 {
                                return Some(val_data.to_string());
                            }
                        }
                    }
                }
            }
        }
        cur += node_chunk_sz;
    }

    let mut i = chunks_start;
    while i + 20 <= data.len() {
        let name_ref =
            u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]);
        if name_ref == target_idx {
            let raw_ref =
                u32::from_le_bytes([data[i + 8], data[i + 9], data[i + 10], data[i + 11]]);
            if raw_ref != 0xFFFFFFFF && (raw_ref as usize) < strings.len() {
                let val = strings[raw_ref as usize].trim().to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
            let val_type = data[i + 15];
            let val_data = u32::from_le_bytes([
                data[i + 16],
                data[i + 17],
                data[i + 18],
                data[i + 19],
            ]);
            if val_type == 0x03 && (val_data as usize) < strings.len() {
                let val = strings[val_data as usize].trim().to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
        i += 4;
    }

    None
}
