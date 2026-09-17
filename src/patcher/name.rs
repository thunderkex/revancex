use serde::{Deserialize, Serialize};

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
}
