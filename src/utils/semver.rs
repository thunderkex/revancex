use std::cmp::Ordering;

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
