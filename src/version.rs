use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct VersionInfo {
    pub version: String,
}

pub fn is_prerelease_version(version: &str) -> bool {
    let trimmed = version.trim();
    if trimmed.is_empty() {
        return false;
    }

    let core = trimmed.splitn(2, '+').next().unwrap_or("");
    let mut core_parts = core.splitn(2, '-');
    let core_version = core_parts.next().unwrap_or("");
    let prerelease = core_parts.next().unwrap_or("");
    if prerelease.is_empty() {
        return false;
    }

    let mut numeric_parts = core_version.split('.');
    let major = numeric_parts.next().unwrap_or("");
    let minor = numeric_parts.next().unwrap_or("");
    let patch = numeric_parts.next().unwrap_or("");
    if major.is_empty()
        || minor.is_empty()
        || patch.is_empty()
        || numeric_parts.next().is_some()
    {
        return false;
    }

    if !major.chars().all(|c| c.is_ascii_digit())
        || !minor.chars().all(|c| c.is_ascii_digit())
        || !patch.chars().all(|c| c.is_ascii_digit())
    {
        return false;
    }

    true
}

fn parse_cargo_version(content: &str) -> Option<String> {
    let mut in_package = false;
    let mut in_workspace_package = false;
    let mut package_version = None;
    let mut workspace_package_version = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = trimmed == "[package]";
            in_workspace_package = trimmed == "[workspace.package]";
            continue;
        }

        if !in_package && !in_workspace_package {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let value = rest.trim_start();
                let quote = value.chars().next();
                if let Some(quote_char) = quote
                    && (quote_char == '"' || quote_char == '\'')
                {
                    let remainder = &value[quote_char.len_utf8()..];
                    if let Some(end) = remainder.find(quote_char) {
                        let parsed = Some(remainder[..end].to_string());
                        if in_package {
                            package_version = parsed;
                        } else if in_workspace_package {
                            workspace_package_version = parsed;
                        }
                    }
                }
            }
        }
    }

    package_version.or(workspace_package_version)
}

fn resolve_rust_version(cwd: &Path) -> Result<Option<VersionInfo>> {
    let file = cwd.join("Cargo.toml");
    if !file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&file)?;
    let version = parse_cargo_version(&content).ok_or_else(|| {
        anyhow!("Cargo.toml does not declare a [package] or [workspace.package] version.")
    })?;

    Ok(Some(VersionInfo { version }))
}

fn resolve_node_version(cwd: &Path) -> Result<Option<VersionInfo>> {
    let file = cwd.join("package.json");
    if !file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&file)?;
    let json: Value = serde_json::from_str(&content)?;
    let version = json
        .get("version")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow!("package.json does not declare a version field."))?;

    Ok(Some(VersionInfo {
        version: version.to_string(),
    }))
}

pub fn parse_languages(input: &str) -> Vec<String> {
    input
        .split(|c: char| c.is_whitespace() || c == ',' || c == '+')
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

pub fn resolve_version(cwd: &Path, languages: &[String]) -> Result<VersionInfo> {
    let mut unknown = Vec::new();
    for language in languages {
        if !matches!(language.as_str(), "rust" | "node") {
            unknown.push(language.clone());
        }
    }

    if !unknown.is_empty() {
        bail!("Unknown language archetype(s): {}", unknown.join(", "));
    }

    let mut attempted = Vec::new();

    for language in languages {
        let result = match language.as_str() {
            "rust" => resolve_rust_version(cwd)?,
            "node" => resolve_node_version(cwd)?,
            _ => None,
        };

        if let Some(info) = result {
            return Ok(info);
        }

        attempted.push(language.clone());
    }

    bail!(
        "Unable to determine version from {}. Ensure the expected version file exists.",
        attempted.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::{is_prerelease_version, parse_cargo_version};

    #[test]
    fn parse_package_version() {
        let content = r#"
[package]
name = "demo"
version = "0.1.0"
"#;
        assert_eq!(parse_cargo_version(content), Some("0.1.0".to_string()));
    }

    #[test]
    fn parse_workspace_package_version() {
        let content = r#"
[workspace]
members = ["crate-a"]

[workspace.package]
version = "1.0.0-alpha.1"
"#;
        assert_eq!(
            parse_cargo_version(content),
            Some("1.0.0-alpha.1".to_string())
        );
    }

    #[test]
    fn prefer_package_over_workspace_package() {
        let content = r#"
[workspace]
members = ["crate-a"]

[workspace.package]
version = "2.0.0"

[package]
name = "demo"
version = "3.1.4"
"#;
        assert_eq!(parse_cargo_version(content), Some("3.1.4".to_string()));
    }

    #[test]
    fn prerelease_detection() {
        assert!(is_prerelease_version("0.1.0-a.1"));
        assert!(is_prerelease_version("5.9.0-beta.3"));
        assert!(is_prerelease_version("1.2.3-rc.1+build.7"));
        assert!(!is_prerelease_version("1.2.3"));
        assert!(!is_prerelease_version("1.2.3+build.7"));
        assert!(!is_prerelease_version("1.2"));
    }
}
