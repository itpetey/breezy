mod config;
mod github;
mod release_notes;
mod version;

use anyhow::{Context, Result, anyhow, bail};
use config::ReleaseConfig;
use github::ReleaseInfo;
use release_notes::{build_release_notes, release_marker};
use std::env;
use version::{parse_languages, resolve_version};

const MAX_PER_PAGE: u32 = 100;

struct DraftSelection {
    primary: Option<u64>,
    extras: Vec<u64>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let branch = resolve_branch()?;
    let tag_prefix = read_input("tag-prefix").unwrap_or_else(|| "v".to_string());
    let token = read_input("github-token")
        .or_else(|| env::var("GITHUB_TOKEN").ok())
        .unwrap_or_default();

    if token.trim().is_empty() {
        bail!("Missing GitHub token. Set the github-token input or GITHUB_TOKEN env.");
    }

    let cwd = env::current_dir().context("Unable to resolve current working directory.")?;
    let config = config::load_config(read_input("config-file"), &cwd)?;
    let language_input = read_input("language").unwrap_or_default();
    let language_source = resolve_language(&language_input, config.as_ref())?;
    let languages = parse_languages(&language_source);
    if languages.is_empty() {
        bail!("No language archetypes provided.");
    }

    let version_info = resolve_version(&cwd, &languages)?;

    let tag_name = resolve_tag_name(&version_info.version, &tag_prefix, config.as_ref());
    let release_name =
        resolve_release_name(&version_info.version, &tag_name, &branch, config.as_ref());
    let marker = release_marker(&branch);

    let (owner, repo) = parse_repository()?;
    let client = github::GitHubClient::new(&token, &owner, &repo)?;

    let releases = client.list_all_releases(MAX_PER_PAGE)?;
    let selection = select_draft_releases(&releases, &marker);

    for release_id in selection.extras {
        client.delete_release(release_id)?;
        println!("Deleted extra draft release {release_id} for {branch}");
    }

    let since = select_latest_published_release(&releases, &branch)
        .map(|release| {
            release
                .published_at
                .as_deref()
                .unwrap_or(&release.created_at)
        })
        .map(|value| value.to_string());

    let pull_requests =
        client.fetch_merged_pull_requests(&branch, since.as_deref(), MAX_PER_PAGE)?;
    let release_notes = build_release_notes(&marker, &pull_requests, config.as_ref());

    if let Some(release_id) = selection.primary {
        client.update_release(
            release_id,
            &tag_name,
            &release_name,
            &release_notes,
            &branch,
        )?;
        println!("Updated draft release {release_id} for {branch}");
    } else {
        client.create_release(&tag_name, &release_name, &release_notes, &branch)?;
        println!("Created draft release for {branch}");
    }

    Ok(())
}

fn input_key(name: &str) -> String {
    format!("INPUT_{}", name.replace(' ', "_").to_uppercase())
}

fn read_input(name: &str) -> Option<String> {
    let key = input_key(name);
    if let Ok(value) = env::var(&key) {
        return Some(value);
    }

    let alternate = key.replace('-', "_");
    if alternate != key {
        if let Ok(value) = env::var(&alternate) {
            return Some(value);
        }
    }

    None
}

fn resolve_language(input: &str, config: Option<&ReleaseConfig>) -> Result<String> {
    if !input.trim().is_empty() {
        return Ok(input.trim().to_string());
    }
    if let Some(config) = config {
        if let Some(language) = &config.language {
            if !language.trim().is_empty() {
                return Ok(language.trim().to_string());
            }
        }
    }
    bail!("Missing required input: language");
}

fn resolve_tag_name(version: &str, tag_prefix: &str, config: Option<&ReleaseConfig>) -> String {
    if let Some(config) = config {
        if let Some(template) = &config.tag_template {
            return template.replace("$VERSION", version);
        }
    }
    format!("{}{}", tag_prefix.trim(), version)
}

fn resolve_release_name(
    version: &str,
    tag_name: &str,
    branch: &str,
    config: Option<&ReleaseConfig>,
) -> String {
    if let Some(config) = config {
        if let Some(template) = &config.name_template {
            return template.replace("$VERSION", version);
        }
    }
    format!("{tag_name} ({branch})")
}

fn parse_repository() -> Result<(String, String)> {
    let repository =
        env::var("GITHUB_REPOSITORY").context("Missing GITHUB_REPOSITORY environment variable.")?;
    let mut parts = repository.splitn(2, '/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if owner.is_empty() || repo.is_empty() {
        return Err(anyhow!(
            "Invalid GITHUB_REPOSITORY value; expected owner/repo."
        ));
    }

    Ok((owner.to_string(), repo.to_string()))
}

fn resolve_branch() -> Result<String> {
    if let Ok(value) = env::var("GITHUB_HEAD_REF") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Ok(value) = env::var("GITHUB_REF_NAME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    if let Ok(value) = env::var("GITHUB_REF") {
        let trimmed = value.trim();
        if let Some(stripped) = trimmed.strip_prefix("refs/heads/") {
            return Ok(stripped.to_string());
        }
        if trimmed.starts_with("refs/pull/") {
            if let Ok(head) = env::var("GITHUB_HEAD_REF") {
                let head = head.trim();
                if !head.is_empty() {
                    return Ok(head.to_string());
                }
            }
        }
    }

    bail!("Unable to determine branch name from GitHub environment.");
}

fn select_draft_releases(releases: &[ReleaseInfo], marker: &str) -> DraftSelection {
    let mut drafts: Vec<&ReleaseInfo> = releases
        .iter()
        .filter(|release| release.draft && release.body.as_deref().unwrap_or("").contains(marker))
        .collect();

    drafts.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    let primary = drafts.first().map(|release| release.id);
    let extras = drafts.iter().skip(1).map(|release| release.id).collect();

    DraftSelection { primary, extras }
}

fn select_latest_published_release<'a>(
    releases: &'a [ReleaseInfo],
    branch: &str,
) -> Option<&'a ReleaseInfo> {
    let mut published: Vec<&ReleaseInfo> = releases
        .iter()
        .filter(|release| !release.draft && release.target_commitish == branch)
        .collect();

    if published.is_empty() {
        return None;
    }

    published.sort_by(|left, right| {
        let left_key = left.published_at.as_deref().unwrap_or(&left.created_at);
        let right_key = right.published_at.as_deref().unwrap_or(&right.created_at);
        right_key.cmp(left_key)
    });

    published.first().copied()
}
