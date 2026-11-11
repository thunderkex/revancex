use regex::Regex;
use std::cmp::Ordering;
use std::sync::OnceLock;

pub fn check_compatibility(app_version: &str, min: Option<&str>, max: Option<&str>) -> bool {
    let app_v = parse_version_numbers(app_version);
    if let Some(min_v) = min {
        let min_clean = min_v.trim();
        if !min_clean.is_empty() {
            let min_parts = parse_version_numbers(min_clean);
            if compare_version_parts(&app_v, &min_parts) == Ordering::Less {
                return false;
            }
        }
    }
    if let Some(max_v) = max {
        let max_clean = max_v.trim();
        if !max_clean.is_empty() {
            let max_parts = parse_version_numbers(max_clean);
            if compare_version_parts(&app_v, &max_parts) == Ordering::Greater {
                return false;
            }
        }
    }
    true
}

pub fn parse_version_numbers(v: &str) -> Vec<u64> {
    let clean = v.trim_start_matches(|c: char| !c.is_ascii_digit());
    clean
        .split(['.', '-', '_'])
        .take_while(|part| !part.is_empty())
        .filter_map(|part| {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u64>().ok()
        })
        .collect()
}

pub fn compare_version_parts(a: &[u64], b: &[u64]) -> Ordering {
    let max_len = a.len().max(b.len());
    for i in 0..max_len {
        let val_a = a.get(i).copied().unwrap_or(0);
        let val_b = b.get(i).copied().unwrap_or(0);
        match val_a.cmp(&val_b) {
            Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    Ordering::Equal
}

pub fn max_version<I, S>(versions: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    versions
        .into_iter()
        .max_by(|a, b| {
            let pa = parse_version_numbers(a.as_ref());
            let pb = parse_version_numbers(b.as_ref());
            compare_version_parts(&pa, &pb)
        })
        .map(|s| s.as_ref().to_string())
}

pub fn is_beta_or_alpha(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("beta") || lower.contains("alpha")
}

pub fn normalize_version(v: &str) -> String {
    let v = v.trim();
    let v = v.trim_end_matches("...");
    if let (Some(open), Some(close)) = (v.find('('), v.find(')')) {
        if open < close {
            let inner = &v[open + 1..close];
            if inner.contains('.') {
                return normalize_version(inner);
            }
        }
    }
    let mut end = 0;
    let bytes = v.as_bytes();
    let len = bytes.len();
    while end < len {
        let ch = bytes[end] as char;
        if ch.is_ascii_digit() || ch == '.' {
            end += 1;
        } else {
            break;
        }
    }
    let core = v[..end].trim_end_matches('.');
    if core.is_empty() {
        v.to_string()
    } else {
        core.to_string()
    }
}

pub fn versions_match(a: &str, b: &str) -> bool {
    let na = normalize_version(a);
    let nb = normalize_version(b);
    if na == nb {
        return true;
    }
    let pa = parse_numeric(na.as_str());
    let pb = parse_numeric(nb.as_str());
    !pa.is_empty() && pa == pb
}

/// Compare two version strings for functional equality across raw string match,
/// normalized version match, and parsed numeric component match.
pub fn version_eq(a: &str, b: &str) -> bool {
    if a == b || versions_match(a, b) {
        return true;
    }
    let pa = parse_version_numbers(a);
    let pb = parse_version_numbers(b);
    !pa.is_empty() && pa == pb
}

fn parse_numeric(v: &str) -> Vec<u64> {
    v.split('.').filter_map(|s| s.parse::<u64>().ok()).collect()
}

pub fn extract_version_from_url_or_filename(s: &str) -> Option<String> {
    static RE_PREF: OnceLock<Regex> = OnceLock::new();
    static RE_LOOSE: OnceLock<Regex> = OnceLock::new();
    let re = RE_PREF.get_or_init(|| {
        Regex::new(r"(?:[vV]|[\-_/])(\d{1,5}(?:\.\d{1,5}){1,4})").expect("valid prefix regex")
    });
    if let Some(cap) = re.captures(s) {
        return Some(cap[1].to_string());
    }
    let re_loose = RE_LOOSE
        .get_or_init(|| Regex::new(r"(\d{1,5}(?:\.\d{1,5}){1,4})").expect("valid loose regex"));
    re_loose.captures(s).map(|c| c[1].to_string())
}

pub fn extract_version_from_asset_filename(filename: &str) -> Option<String> {
    let stem = filename
        .strip_suffix(".apk")
        .or_else(|| filename.strip_suffix(".zip"))
        .unwrap_or(filename)
        .strip_suffix("-module")
        .unwrap_or(
            filename
                .strip_suffix(".apk")
                .or_else(|| filename.strip_suffix(".zip"))
                .unwrap_or(filename),
        );
    if stem.ends_with("-patched") || stem == "microg" {
        return None;
    }
    let parts: Vec<&str> = stem.split('-').collect();
    for (idx, part) in parts.iter().enumerate() {
        let clean = part.strip_prefix('v').unwrap_or(part);
        if clean.chars().next().is_some_and(|c| c.is_ascii_digit()) && clean.contains('.') {
            let mut ver = clean.to_string();
            if let Some(next) = parts.get(idx + 1) {
                if next.starts_with("prod") || next.starts_with("beta") || next.starts_with("rc") {
                    ver.push('-');
                    ver.push_str(next);
                }
            }
            return Some(ver);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_suffix_variants() {
        assert_eq!(
            normalize_version("18.0.3.954559732-release-arm64-v8a"),
            "18.0.3.954559732"
        );
        assert_eq!(normalize_version("32.30.0(1575420)"), "32.30.0");
        assert_eq!(normalize_version("6.6 build 002"), "6.6");
        assert_eq!(normalize_version("20.12.36"), "20.12.36");
        assert_eq!(normalize_version("88800 (8.8.8)"), "8.8.8");
        assert_eq!(normalize_version("4.5.6..."), "4.5.6");
        assert_eq!(normalize_version("1.0"), "1.0");
    }

    #[test]
    fn versions_match_cases() {
        assert!(versions_match(
            "18.0.3.954559732-release-arm64-v8a",
            "18.0.3.954559732"
        ));
        assert!(versions_match("32.30.0(1575420)", "32.30.0"));
        assert!(versions_match("6.6 build 002", "6.6"));
        assert!(!versions_match("1.2.3", "1.2.4"));
    }

    #[test]
    fn version_eq_cases() {
        assert!(version_eq("1.2.3", "1.2.3"));
        assert!(version_eq("1.2-3", "1.2.3"));
        assert!(version_eq("v20.12.36", "20.12.36"));
        assert!(!version_eq("1.2.3", "1.2.4"));
    }
}
