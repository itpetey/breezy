use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CHANGE_TEMPLATE: &str = "$TITLE";

#[derive(Debug, Clone)]
pub struct ReleaseCategory {
    pub title: String,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ReleaseConfig {
    pub language: Option<String>,
    pub tag_template: Option<String>,
    pub name_template: Option<String>,
    pub categories: Vec<ReleaseCategory>,
    pub exclude_labels: Vec<String>,
    pub change_template: String,
    pub template: Option<String>,
}

#[derive(Deserialize)]
struct RawConfig {
    language: Option<String>,
    #[serde(rename = "tag-template")]
    tag_template: Option<String>,
    #[serde(rename = "name-template")]
    name_template: Option<String>,
    categories: Option<Vec<RawCategory>>,
    #[serde(rename = "exclude-labels")]
    exclude_labels: Option<Vec<String>>,
    #[serde(rename = "change-template")]
    change_template: Option<String>,
    template: Option<String>,
}

#[derive(Deserialize)]
struct RawCategory {
    title: String,
    labels: Option<Vec<String>>,
    label: Option<String>,
}

impl ReleaseConfig {
    fn from_raw(raw: RawConfig) -> Self {
        let categories = raw
            .categories
            .unwrap_or_default()
            .into_iter()
            .map(|category| {
                let mut labels = Vec::new();
                if let Some(list) = category.labels {
                    labels.extend(list);
                }
                if let Some(label) = category.label {
                    labels.push(label);
                }
                ReleaseCategory {
                    title: category.title,
                    labels: normalize_labels(labels),
                }
            })
            .collect();

        ReleaseConfig {
            language: raw.language.map(|value| value.trim().to_lowercase()),
            tag_template: raw.tag_template.map(|value| value.trim().to_string()),
            name_template: raw.name_template.map(|value| value.trim().to_string()),
            categories,
            exclude_labels: normalize_labels(raw.exclude_labels.unwrap_or_default()),
            change_template: raw
                .change_template
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_CHANGE_TEMPLATE.to_string()),
            template: raw.template.map(|value| value.trim().to_string()),
        }
    }
}

pub fn load_config(input: Option<String>, cwd: &Path) -> Result<Option<ReleaseConfig>> {
    if let Some(raw_path) = input.filter(|value| !value.trim().is_empty()) {
        let path = resolve_path(&raw_path, cwd)?;
        if !path.exists() {
            bail!("Config file not found: {}", path.display());
        }
        return Ok(Some(read_config(&path)?));
    }

    if let Some(home) = std::env::var("HOME").ok().map(PathBuf::from) {
        let home_path = home.join(".github").join("breezy.yml");
        if home_path.exists() {
            return Ok(Some(read_config(&home_path)?));
        }
    }

    let repo_path = cwd.join(".github").join("breezy.yml");
    if repo_path.exists() {
        return Ok(Some(read_config(&repo_path)?));
    }

    Ok(None)
}

fn resolve_path(input: &str, cwd: &Path) -> Result<PathBuf> {
    if let Some(stripped) = input.strip_prefix("~/") {
        let home = std::env::var("HOME").context("HOME is not set.")?;
        return Ok(PathBuf::from(home).join(stripped));
    }
    if input == "~" {
        let home = std::env::var("HOME").context("HOME is not set.")?;
        return Ok(PathBuf::from(home));
    }

    let path = PathBuf::from(input);
    if path.is_absolute() {
        return Ok(path);
    }

    Ok(cwd.join(path))
}

fn read_config(path: &Path) -> Result<ReleaseConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    let raw: RawConfig =
        serde_yaml::from_str(&content).map_err(|error| anyhow!("Invalid config YAML: {error}"))?;
    Ok(ReleaseConfig::from_raw(raw))
}

fn normalize_labels(labels: Vec<String>) -> Vec<String> {
    labels
        .into_iter()
        .map(|label| label.trim().to_lowercase())
        .filter(|label| !label.is_empty())
        .collect()
}
