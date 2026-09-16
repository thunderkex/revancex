use anyhow::{Context, Result};
use std::io::{Read, Write};
use zip::write::SimpleFileOptions;

pub enum EntryAction {
    Keep,
    Drop,
    Recompress(zip::CompressionMethod),
}

pub fn rewrite<F>(src_path: &str, dst_path: &str, mut transform: F) -> Result<()>
where
    F: FnMut(&str) -> EntryAction,
{
    let src_file = std::fs::File::open(src_path).with_context(|| format!("Opening {src_path}"))?;
    let mut zip_in = zip::ZipArchive::new(src_file)?;

    let out_file =
        std::fs::File::create(dst_path).with_context(|| format!("Creating {dst_path}"))?;
    let mut zip_out = zip::ZipWriter::new(out_file);

    for i in 0..zip_in.len() {
        let mut entry = zip_in.by_index(i)?;
        let name = entry.name().to_string();

        let action = transform(&name);

        match action {
            EntryAction::Drop => continue,
            EntryAction::Keep | EntryAction::Recompress(_) => {
                let compression = if name == "resources.arsc" {
                    zip::CompressionMethod::Stored
                } else {
                    match action {
                        EntryAction::Recompress(m) => m,
                        _ => entry.compression(),
                    }
                };
                let opts = SimpleFileOptions::default().compression_method(compression);
                zip_out.start_file(&name, opts)?;
                let mut buf = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buf)?;
                zip_out.write_all(&buf)?;
            }
        }
    }

    zip_out.finish()?;
    Ok(())
}
