use crate::config::apps::AppConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MatchRule {
    Equals { equals: String },
    Contains { contains: String },
    Regex { regex: String },
}

pub fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_space = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_was_space = false;
        } else if !last_was_space {
            out.push(' ');
            last_was_space = true;
        }
    }
    out.trim().to_string()
}

pub fn matches(rule: &MatchRule, patch_name: &str) -> bool {
    let norm_name = normalize(patch_name);
    match rule {
        MatchRule::Equals { equals: val } => norm_name == normalize(val),
        MatchRule::Contains { contains: val } => norm_name.contains(&normalize(val)),
        MatchRule::Regex { regex: pattern } => {
            if let Ok(re) = regex::Regex::new(pattern) {
                re.is_match(patch_name) || re.is_match(&norm_name)
            } else {
                false
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Apk,
    Module,
}

pub fn sanitize_app_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "App".to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn output_filename(app: &str, version: &str, kind: ArtifactKind) -> String {
    let clean_app = sanitize_app_name(app);
    let clean_ver = version.trim().trim_start_matches('v');
    match kind {
        ArtifactKind::Apk => format!("{clean_app}-{clean_ver}.apk"),
        ArtifactKind::Module => format!("{clean_app}-{clean_ver}-module.zip"),
    }
}

pub fn resolve_app_id<'a>(
    apps: &'a BTreeMap<String, AppConfig>,
    raw_name_or_stem: &str,
) -> Option<&'a str> {
    let clean = raw_name_or_stem
        .strip_suffix(".apk")
        .unwrap_or(raw_name_or_stem)
        .strip_suffix(".zip")
        .unwrap_or(raw_name_or_stem);

    let clean = clean
        .strip_suffix("-root-payload")
        .or_else(|| clean.strip_suffix("-root-stage"))
        .or_else(|| clean.strip_suffix("-stage"))
        .or_else(|| clean.strip_suffix("-patched"))
        .or_else(|| clean.strip_suffix("-module"))
        .unwrap_or(clean);

    let clean = clean
        .strip_prefix("revancex-module-")
        .or_else(|| clean.strip_prefix("revancex-"))
        .unwrap_or(clean);

    if let Some((k, _)) = apps.iter().find(|(k, _)| k.eq_ignore_ascii_case(clean)) {
        return Some(k.as_str());
    }

    for (id, app) in apps {
        if app
            .name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(clean))
            || app.display_name(id).eq_ignore_ascii_case(clean)
            || sanitize_app_name(&app.display_name(id)).eq_ignore_ascii_case(clean)
        {
            return Some(id.as_str());
        }
    }

    let mut candidates = Vec::new();
    for (id, app) in apps {
        let clean_display = sanitize_app_name(&app.display_name(id));
        let display_prefix = format!("{clean_display}-");
        if clean.starts_with(&display_prefix) {
            candidates.push((display_prefix.len(), id.as_str()));
            continue;
        }
        let id_prefix = format!("{id}-");
        if clean.starts_with(&id_prefix) {
            candidates.push((id_prefix.len(), id.as_str()));
        }
    }

    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
    candidates.first().map(|(_, id)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize() {
        assert_eq!(normalize("Custom branding-icon"), "custom branding icon");
        assert_eq!(normalize("GmsCore_support"), "gmscore support");
        assert_eq!(normalize("Change-Package-Name"), "change package name");
    }

    #[test]
    fn test_matches() {
        let rule = MatchRule::Contains {
            contains: "custom branding".to_string(),
        };
        assert!(matches(&rule, "Custom Branding Icon for YouTube"));
        assert!(matches(&rule, "custom-branding"));
        assert!(!matches(&rule, "hide-shorts"));
    }

    #[test]
    fn test_output_filename() {
        assert_eq!(
            output_filename("YouTube", "19.16.39", ArtifactKind::Apk),
            "YouTube-19.16.39.apk"
        );
        assert_eq!(
            output_filename("Spotify", "9.0.20", ArtifactKind::Apk),
            "Spotify-9.0.20.apk"
        );
        assert_eq!(
            output_filename("YouTube", "19.16.39", ArtifactKind::Module),
            "YouTube-19.16.39-module.zip"
        );
        assert_eq!(
            output_filename("Nova Launcher", "8.0.18", ArtifactKind::Apk),
            "Nova-Launcher-8.0.18.apk"
        );
        assert_eq!(
            output_filename("YouTube Music", "7.25.53", ArtifactKind::Module),
            "YouTube-Music-7.25.53-module.zip"
        );
    }

    #[test]
    fn test_resolve_app_id_youtube_vs_music() {
        let mut apps = BTreeMap::new();
        apps.insert(
            "youtube".to_string(),
            AppConfig {
                name: Some("YouTube".to_string()),
                ..Default::default()
            },
        );
        apps.insert(
            "youtube_music".to_string(),
            AppConfig {
                name: Some("YouTube Music".to_string()),
                ..Default::default()
            },
        );

        assert_eq!(
            resolve_app_id(&apps, "YouTube-Music-9.15.51.apk"),
            Some("youtube_music")
        );
        assert_eq!(
            resolve_app_id(&apps, "YouTube-Music-9.15.51"),
            Some("youtube_music")
        );
        assert_eq!(
            resolve_app_id(&apps, "YouTube-Music-9.15.51-module.zip"),
            Some("youtube_music")
        );
        assert_eq!(
            resolve_app_id(&apps, "youtube_music-root-payload.apk"),
            Some("youtube_music")
        );
        assert_eq!(
            resolve_app_id(&apps, "youtube_music"),
            Some("youtube_music")
        );

        assert_eq!(
            resolve_app_id(&apps, "YouTube-20.05.46.apk"),
            Some("youtube")
        );
        assert_eq!(resolve_app_id(&apps, "YouTube-20.05.46"), Some("youtube"));
        assert_eq!(
            resolve_app_id(&apps, "YouTube-20.05.46-module.zip"),
            Some("youtube")
        );
        assert_eq!(
            resolve_app_id(&apps, "youtube-root-payload.apk"),
            Some("youtube")
        );
        assert_eq!(resolve_app_id(&apps, "youtube"), Some("youtube"));
    }
}
