mod config;
mod github;
mod release_notes;
mod version;

use anyhow::{Context, Result, anyhow, bail};
use config::ReleaseConfig;
use github::ReleaseInfo;
use release_notes::{build_release_notes, release_marker};
use std::env;
use std::path::Path;
use version::{is_prerelease_version, parse_languages, resolve_version};

const MAX_PER_PAGE: u32 = 100;

struct DraftSelection {
    primary: Option<u64>,
    extras: Vec<u64>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let branch = resolve_branch()?;
    let directory = resolve_directory(read_input("directory"))?;
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

    let version_root = match &directory {
        Some(directory) => cwd.join(directory),
        None => cwd.clone(),
    };
    let version_info = resolve_version(&version_root, &languages)?;

    let tag_name = resolve_tag_name(
        &version_info.version,
        &tag_prefix,
        directory.as_deref(),
        config.as_ref(),
    );
    let release_name = resolve_release_name(
        &version_info.version,
        &tag_name,
        &branch,
        directory.as_deref(),
        config.as_ref(),
    );
    let marker = release_marker(&branch, directory.as_deref());
    let prerelease = is_prerelease_version(&version_info.version);
    let scope_label = format_scope_label(&branch, directory.as_deref());

    let (owner, repo) = parse_repository()?;
    let client = github::GitHubClient::new(&token, &owner, &repo)?;

    let releases = client.list_all_releases(MAX_PER_PAGE)?;
    let selection = select_draft_releases(&releases, &marker);

    for release_id in selection.extras {
        client.delete_release(release_id)?;
        println!("Deleted extra draft release {release_id} for {scope_label}");
    }

    let marker_filter = directory.as_deref().map(|_| marker.as_str());
    let latest_published = select_latest_published_release(&releases, &branch, marker_filter);
    let current_sha = resolve_current_sha();
    let skip_create = if selection.primary.is_none() {
        if let (Some(current_sha), Some(latest_published)) =
            (current_sha.as_deref(), latest_published)
        {
            published_release_matches_commit(&client, latest_published, current_sha)?
        } else {
            false
        }
    } else {
        false
    };

    if skip_create {
        let current_sha = current_sha.as_deref().unwrap_or("unknown");
        println!(
            "Skipping draft release for {scope_label} because a published release already exists for commit {current_sha}"
        );
        return Ok(());
    }

    let since = latest_published
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
            prerelease,
            &branch,
        )?;
        println!("Updated draft release {release_id} for {scope_label}");
    } else {
        client.create_release(
            &tag_name,
            &release_name,
            &release_notes,
            prerelease,
            &branch,
        )?;
        println!("Created draft release for {scope_label}");
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
    if alternate != key
        && let Ok(value) = env::var(&alternate)
    {
        return Some(value);
    }

    None
}

fn resolve_language(input: &str, config: Option<&ReleaseConfig>) -> Result<String> {
    if !input.trim().is_empty() {
        return Ok(input.trim().to_string());
    }
    if let Some(config) = config
        && let Some(language) = &config.language
        && !language.trim().is_empty()
    {
        return Ok(language.trim().to_string());
    }
    bail!("Missing required input: language");
}

fn apply_template(template: &str, version: &str, directory: Option<&str>) -> String {
    let mut rendered = template.replace("$VERSION", version);
    rendered = rendered.replace("$DIRECTORY", directory.unwrap_or(""));
    rendered
}

fn resolve_tag_name(
    version: &str,
    tag_prefix: &str,
    directory: Option<&str>,
    config: Option<&ReleaseConfig>,
) -> String {
    if let Some(config) = config
        && let Some(template) = &config.tag_template
    {
        return apply_template(template, version, directory);
    }
    format!("{}{}", tag_prefix.trim(), version)
}

fn resolve_release_name(
    version: &str,
    tag_name: &str,
    branch: &str,
    directory: Option<&str>,
    config: Option<&ReleaseConfig>,
) -> String {
    if let Some(config) = config
        && let Some(template) = &config.name_template
    {
        return apply_template(template, version, directory);
    }
    let scope = format_scope_label(branch, directory);
    format!("{tag_name} ({scope})")
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
        if trimmed.starts_with("refs/pull/")
            && let Ok(head) = env::var("GITHUB_HEAD_REF")
        {
            let head = head.trim();
            if !head.is_empty() {
                return Ok(head.to_string());
            }
        }
    }

    bail!("Unable to determine branch name from GitHub environment.");
}

fn resolve_current_sha() -> Option<String> {
    env::var("GITHUB_SHA").ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn resolve_directory(input: Option<String>) -> Result<Option<String>> {
    let Some(raw) = input else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let mut value = trimmed.trim_end_matches('/');
    value = value.trim_end_matches('\\');
    if value == "." {
        return Ok(None);
    }
    if Path::new(value).is_absolute() {
        bail!("Directory input must be a relative path within the repository.");
    }

    value = value.trim_start_matches("./");
    if value.is_empty() || value == "." {
        return Ok(None);
    }

    Ok(Some(value.to_string()))
}

fn format_scope_label(branch: &str, directory: Option<&str>) -> String {
    if let Some(directory) = directory.filter(|value| !value.trim().is_empty()) {
        return format!("{branch}/{directory}");
    }
    branch.to_string()
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
    marker: Option<&str>,
) -> Option<&'a ReleaseInfo> {
    let mut published: Vec<&ReleaseInfo> = releases
        .iter()
        .filter(|release| {
            if release.draft || release.target_commitish != branch {
                return false;
            }
            if let Some(marker) = marker {
                return release.body.as_deref().unwrap_or("").contains(marker);
            }
            true
        })
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

fn published_release_matches_commit(
    client: &github::GitHubClient,
    release: &ReleaseInfo,
    current_sha: &str,
) -> Result<bool> {
    if release.target_commitish == current_sha {
        return Ok(true);
    }
    let tag_name = release.tag_name.trim();
    if tag_name.is_empty() {
        return Ok(false);
    }
    let release_sha = client.resolve_commit_sha(tag_name)?;
    Ok(release_sha == current_sha)
}
