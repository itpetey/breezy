use anyhow::{Result, anyhow, bail};
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub struct VersionInfo {
    pub version: String,
}

fn parse_cargo_version(content: &str) -> Option<String> {
    let mut in_package = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = trimmed == "[package]";
            continue;
        }

        if !in_package {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let value = rest.trim_start();
                let quote = value.chars().next();
                if let Some(quote_char) = quote {
                    if quote_char == '"' || quote_char == '\'' {
                        let remainder = &value[quote_char.len_utf8()..];
                        if let Some(end) = remainder.find(quote_char) {
                            return Some(remainder[..end].to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

fn resolve_rust_version(cwd: &Path) -> Result<Option<VersionInfo>> {
    let file = cwd.join("Cargo.toml");
    if !file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&file)?;
    let version = parse_cargo_version(&content)
        .ok_or_else(|| anyhow!("Cargo.toml does not declare a [package] version."))?;

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
