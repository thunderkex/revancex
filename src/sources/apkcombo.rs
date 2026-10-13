use super::{default_fetch, ApkSource, FileKind, Resolved, SourceCtx};
use crate::config::apps::AppConfig;
use crate::utils::version::normalize_version;
use anyhow::Result;
use scraper::{Html, Selector};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub struct ApkcomboSource;

impl ApkSource for ApkcomboSource {
    fn name(&self) -> &'static str {
        "apkcombo"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            let mut trace = vec![format!("Resolving from apkcombo for {}", app.package)];

            let search_url = format!("https://apkcombo.com/search/{}/download", app.package);
            trace.push(format!("GET {}", search_url));

            let html_text = match ctx.client.get(&search_url).send().await {
                Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
                _ => return Ok(None),
            };

            let mut found_url: Option<String> = None;
            let mut found_version: Option<String> = None;
            let mut file_kind = FileKind::Apk;

            {
                let doc = Html::parse_document(&html_text);
                let link_sel = match Selector::parse("a[href]") {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };

                let re_ver = regex::Regex::new(r"phone-(.*?)-(apk|xapk|apks)").unwrap();

                for element in doc.select(&link_sel) {
                    if let Some(href) = element.value().attr("href") {
                        if let Some(caps) = re_ver.captures(href) {
                            let ver_str = &caps[1];
                            let ext = &caps[2];

                            let matches = match version {
                                Some(req_v) => normalize_version(ver_str) == normalize_version(req_v),
                                None => true,
                            };

                            if matches {
                                let full_href = if href.starts_with("http") {
                                    href.to_string()
                                } else {
                                    format!("https://apkcombo.com{}", href)
                                };

                                found_url = Some(full_href);
                                found_version = Some(ver_str.to_string());
                                file_kind = match ext {
                                    "xapk" => FileKind::Xapk,
                                    "apks" => FileKind::Apks,
                                    _ => FileKind::Apk,
                                };
                                break;
                            }
                        }
                    }
                }
            }

            let dl_page = match found_url {
                Some(p) => p,
                None => return Ok(None),
            };

            trace.push(format!("Fetching download page {}", dl_page));

            let dl_html = match ctx.client.get(&dl_page).send().await {
                Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
                _ => return Ok(None),
            };

            let mut final_url = None;
            {
                let dl_doc = Html::parse_document(&dl_html);
                let variant_sel = match Selector::parse("a.variant[href]") {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };

                for v_elem in dl_doc.select(&variant_sel) {
                    if let Some(href) = v_elem.value().attr("href") {
                        if href.contains("/r2") && href.contains("u=") {
                            if let Some(pos) = href.find("u=") {
                                let raw_u = &href[pos + 2..];
                                let end_pos = raw_u.find('&').unwrap_or(raw_u.len());
                                let decoded = urlencoding_decode(&raw_u[..end_pos]);
                                final_url = Some(decoded);
                                break;
                            }
                        } else if href.starts_with("http") {
                            final_url = Some(href.to_string());
                            break;
                        }
                    }
                }
            }

            let direct_link = final_url.unwrap_or(dl_page);
            trace.push(format!("Direct link resolved: {}", direct_link));

            let mut resolved = Resolved::simple(direct_link, found_version, arch);
            resolved.file_kind = file_kind;
            resolved.trace = trace;

            Ok(Some(resolved))
        })
    }

    fn fetch<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        resolved: &'a Resolved,
        dest: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<PathBuf>> + Send + 'a>> {
        let dest = dest.to_string();
        Box::pin(async move { default_fetch(ctx, resolved, &dest).await })
    }
}

fn urlencoding_decode(s: &str) -> String {
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}
