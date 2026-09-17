use crate::config::Config;
use anyhow::Result;
use serde_json::json;

pub fn generate_apps_json(cfg: &Config) -> Result<String> {
    let mut entries: Vec<_> = cfg.apps.iter().collect();
    entries.sort_by_key(|(id, _)| *id);
    let apps: Vec<_> = entries
        .into_iter()
        .map(|(id, app)| {
            json!({
                "id": id,
                "name": app.display_name.as_deref().unwrap_or(id),
                "enabled": app.enabled,
                "package": app.package,
                "patch_source": app.patch_source,
                "architectures": app.architectures,
                "mode": app.mode,
                "patches": app.patches,
                "dependencies": app.dependencies.all(),
                "module": {
                    "single": app.module.as_ref().map(|m| m.single).unwrap_or(true),
                    "bundle": app.module.as_ref().map(|m| m.bundle).unwrap_or(true),
                }
            })
        })
        .collect();

    Ok(serde_json::to_string_pretty(&apps)?)
}
