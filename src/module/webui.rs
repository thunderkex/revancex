use anyhow::Result;
use std::io::Write;
use std::path::Path;
use tracing::info;
use zip::write::SimpleFileOptions;

pub fn inject_webui<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    opts: SimpleFileOptions,
    apps_json: Option<&str>,
) -> Result<()> {
    info!("Injecting WebUI into webroot/...");
    let webui_html = Path::new("webui/index.html");
    if webui_html.exists() {
        let data = std::fs::read(webui_html)?;
        zip.start_file("webroot/index.html", opts)?;
        zip.write_all(&data)?;
    }

    if let Some(json_str) = apps_json {
        zip.start_file("webroot/apps.json", opts)?;
        zip.write_all(json_str.as_bytes())?;
    }
    Ok(())
}
