use super::{default_fetch, ApkSource, FileKind, Resolved, SourceCtx};
use crate::config::apps::AppConfig;
use crate::utils::version::normalize_version;
use anyhow::Result;
use scraper::{Html, Selector};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

pub struct ApkfabSource;

impl ApkSource for ApkfabSource {
    fn name(&self) -> &'static str {
        "apkfab"
    }

    fn resolve<'a>(
        &'a self,
        ctx: &'a SourceCtx<'a>,
        app: &'a AppConfig,
        version: Option<&'a str>,
        arch: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Resolved>>> + Send + 'a>> {
        Box::pin(async move {
            let mut trace = vec![format!("Resolving from apkfab for {}", app.package)];

            let slug = app
                .sources
                .get("apkfab")
                .and_then(|e| e.slug.as_deref())
                .unwrap_or(&app.package);

            let versions_url = format!("https://apkfab.com/{}/{}/versions", slug, app.package);
            trace.push(format!("GET {}", versions_url));

            let html_text = match ctx.client.get(&versions_url).send().await {
                Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
                _ => return Ok(None),
            };

            let mut matched_sha1: Option<String> = None;
            let mut found_version: Option<String> = None;

            {
                let doc = Html::parse_document(&html_text);
                let item_sel = match Selector::parse(".version_history .list") {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };
                let ver_sel = match Selector::parse(".package_info .title .version") {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };
                let dl_sel = match Selector::parse("a[href*='/download?sha1=']") {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };

                for item in doc.select(&item_sel) {
                    let ver_text = item
                        .select(&ver_sel)
                        .next()
                        .map(|e| e.text().collect::<String>().trim().to_string())
                        .unwrap_or_default();

                    let matches = match version {
                        Some(req_v) => normalize_version(&ver_text) == normalize_version(req_v),
                        None => !ver_text.is_empty(),
                    };

                    if matches {
                        if let Some(dl_elem) = item.select(&dl_sel).next() {
                            if let Some(href) = dl_elem.value().attr("href") {
                                if let Some(pos) = href.find("sha1=") {
                                    matched_sha1 = Some(href[pos + 5..].to_string());
                                    found_version = Some(ver_text);
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            let sha1 = match matched_sha1 {
                Some(s) => s,
                None => return Ok(None),
            };

            trace.push(format!("Resolved sha1: {}", sha1));

            let download_page = format!(
                "https://apkfab.com/{}/{}/download?sha1={}",
                slug, app.package, sha1
            );

            let dl_html = match ctx.client.get(&download_page).send().await {
                Ok(r) if r.status().is_success() => r.text().await.unwrap_or_default(),
                _ => return Ok(None),
            };

            let direct_url = {
                let dl_doc = Html::parse_document(&dl_html);
                let direct_sel = match Selector::parse("a[href*='d.apkfab.com/get-download']") {
                    Ok(s) => s,
                    Err(_) => return Ok(None),
                };

                match dl_doc
                    .select(&direct_sel)
                    .next()
                    .and_then(|e| e.value().attr("href"))
                {
                    Some(u) => u.to_string(),
                    None => return Ok(None),
                }
            };

            trace.push(format!("Signed direct URL resolved: {}", direct_url));

            let mut resolved = Resolved::simple(direct_url, found_version, arch);
            resolved.file_kind = FileKind::Apk;
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
