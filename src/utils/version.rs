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

fn parse_numeric(v: &str) -> Vec<u64> {
    v.split('.').filter_map(|s| s.parse::<u64>().ok()).collect()
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
}
