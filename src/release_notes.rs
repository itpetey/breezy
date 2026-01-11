use crate::config::ReleaseConfig;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct PullRequestInfo {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub labels: Vec<String>,
    pub url: String,
    pub merged_at: Option<String>,
}

pub fn release_marker(branch: &str) -> String {
    format!("<!-- breezy:branch={branch} -->")
}

fn sort_by_merge_date(pull_requests: &[PullRequestInfo]) -> Vec<PullRequestInfo> {
    let mut ordered = pull_requests.to_vec();
    ordered.sort_by(|left, right| left.merged_at.cmp(&right.merged_at));
    ordered
}

pub fn build_release_notes(
    marker: &str,
    pull_requests: &[PullRequestInfo],
    config: Option<&ReleaseConfig>,
) -> String {
    if let Some(config) = config {
        let changes = build_changes(pull_requests, config);
        let body = if let Some(template) = &config.template {
            template.replace("$CHANGES", &changes)
        } else {
            changes
        };
        if body.trim().is_empty() {
            return marker.to_string();
        }
        return format!("{marker}\n\n{body}");
    }

    let mut lines = vec![marker.to_string()];
    let mut seen = HashSet::new();

    for pull_request in sort_by_merge_date(pull_requests) {
        if seen.contains(&pull_request.number) {
            continue;
        }
        seen.insert(pull_request.number);
        lines.push(pull_request.title.clone());
    }

    if lines.len() == 1 {
        return lines.remove(0);
    }

    let mut body = Vec::with_capacity(lines.len() + 1);
    body.push(lines.remove(0));
    body.push(String::new());
    body.extend(lines);
    body.join("\n")
}

fn build_changes(pull_requests: &[PullRequestInfo], config: &ReleaseConfig) -> String {
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();

    for pull_request in sort_by_merge_date(pull_requests) {
        if seen.insert(pull_request.number) {
            ordered.push(pull_request);
        }
    }

    let mut lines = Vec::new();
    let mut categorized = HashSet::new();

    if !config.categories.is_empty() {
        for category in &config.categories {
            let mut category_lines = Vec::new();
            for pull_request in &ordered {
                if is_excluded(pull_request, config) {
                    continue;
                }
                if !has_matching_label(pull_request, &category.labels) {
                    continue;
                }
                categorized.insert(pull_request.number);
                category_lines.push(apply_change_template(&config.change_template, pull_request));
            }
            if !category_lines.is_empty() {
                lines.push(format!("## {}", category.title));
                lines.extend(category_lines);
                lines.push(String::new());
            }
        }
    }

    let mut uncategorized = Vec::new();
    for pull_request in &ordered {
        if categorized.contains(&pull_request.number) {
            continue;
        }
        if is_excluded(pull_request, config) {
            continue;
        }
        uncategorized.push(apply_change_template(&config.change_template, pull_request));
    }

    if !uncategorized.is_empty() {
        if !config.categories.is_empty() {
            lines.push("## Other Changes".to_string());
        }
        lines.extend(uncategorized);
        lines.push(String::new());
    }

    while matches!(lines.last(), Some(value) if value.is_empty()) {
        lines.pop();
    }

    lines.join("\n")
}

fn has_matching_label(pull_request: &PullRequestInfo, category_labels: &[String]) -> bool {
    if category_labels.is_empty() {
        return false;
    }
    let labels = normalized_labels(&pull_request.labels);
    category_labels
        .iter()
        .any(|label| labels.contains(&label.to_lowercase()))
}

fn is_excluded(pull_request: &PullRequestInfo, config: &ReleaseConfig) -> bool {
    if config.exclude_labels.is_empty() {
        return false;
    }
    let labels = normalized_labels(&pull_request.labels);
    config
        .exclude_labels
        .iter()
        .any(|label| labels.contains(&label.to_lowercase()))
}

fn apply_change_template(template: &str, pull_request: &PullRequestInfo) -> String {
    template
        .replace("$TITLE", &pull_request.title)
        .replace("$AUTHOR", &pull_request.author)
        .replace("$NUMBER", &pull_request.url)
}

fn normalized_labels(labels: &[String]) -> HashSet<String> {
    labels
        .iter()
        .map(|label| label.trim().to_lowercase())
        .filter(|label| !label.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ReleaseCategory, ReleaseConfig};

    fn base_config(with_template: bool) -> ReleaseConfig {
        ReleaseConfig {
            language: None,
            tag_template: None,
            name_template: None,
            categories: vec![ReleaseCategory {
                title: "Features".to_string(),
                labels: vec!["feature".to_string()],
            }],
            exclude_labels: vec!["skip-log".to_string()],
            change_template: "* $TITLE @$AUTHOR ($NUMBER)".to_string(),
            template: if with_template {
                Some("## Changes\n\n$CHANGES".to_string())
            } else {
                None
            },
        }
    }

    #[test]
    fn renders_categories_and_urls() {
        let config = base_config(true);
        let marker = release_marker("main");
        let pull_requests = vec![
            PullRequestInfo {
                number: 1,
                title: "Add login".to_string(),
                author: "alice".to_string(),
                labels: vec!["feature".to_string()],
                url: "https://github.com/o/r/pull/1".to_string(),
                merged_at: Some("2024-01-01T00:00:00Z".to_string()),
            },
            PullRequestInfo {
                number: 2,
                title: "Fix bug".to_string(),
                author: "bob".to_string(),
                labels: vec!["bug".to_string()],
                url: "https://github.com/o/r/pull/2".to_string(),
                merged_at: Some("2024-01-02T00:00:00Z".to_string()),
            },
            PullRequestInfo {
                number: 3,
                title: "Chore".to_string(),
                author: "cam".to_string(),
                labels: vec!["skip-log".to_string()],
                url: "https://github.com/o/r/pull/3".to_string(),
                merged_at: Some("2024-01-03T00:00:00Z".to_string()),
            },
        ];

        let notes = build_release_notes(&marker, &pull_requests, Some(&config));

        let expected = [
            marker.as_str(),
            "",
            "## Changes",
            "",
            "## Features",
            "* Add login @alice (https://github.com/o/r/pull/1)",
            "",
            "## Other Changes",
            "* Fix bug @bob (https://github.com/o/r/pull/2)",
        ]
        .join("\n");

        assert_eq!(notes, expected);
    }

    #[test]
    fn returns_marker_when_no_changes() {
        let config = base_config(false);
        let marker = release_marker("main");
        let notes = build_release_notes(&marker, &[], Some(&config));

        assert_eq!(notes, marker);
    }
}
