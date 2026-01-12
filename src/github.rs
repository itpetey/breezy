use crate::release_notes::PullRequestInfo;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::{Deserialize, Serialize};

const API_BASE: &str = "https://api.github.com";

#[derive(Debug, Deserialize)]
pub struct ReleaseInfo {
    pub id: u64,
    pub body: Option<String>,
    pub draft: bool,
    pub target_commitish: String,
    pub created_at: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    number: u64,
    title: String,
    user: Option<SearchUser>,
    labels: Vec<SearchLabel>,
    merged_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchUser {
    login: String,
}

#[derive(Debug, Deserialize)]
struct SearchLabel {
    name: String,
}

#[derive(Debug, Serialize)]
struct ReleaseRequest<'a> {
    tag_name: &'a str,
    name: &'a str,
    body: &'a str,
    draft: bool,
    prerelease: bool,
    target_commitish: &'a str,
}

#[derive(Serialize)]
struct PageQuery<'a> {
    per_page: u32,
    page: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    q: Option<&'a str>,
}

pub struct GitHubClient {
    client: Client,
    owner: String,
    repo: String,
}

impl GitHubClient {
    pub fn new(token: &str, owner: &str, repo: &str) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(USER_AGENT, HeaderValue::from_static("breezy"));
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        let auth = format!("Bearer {token}");
        headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth)?);

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .context("Failed to build GitHub HTTP client.")?;

        Ok(Self {
            client,
            owner: owner.to_string(),
            repo: repo.to_string(),
        })
    }

    pub fn list_all_releases(&self, per_page: u32) -> Result<Vec<ReleaseInfo>> {
        let mut releases = Vec::new();
        let mut page = 1_u32;

        loop {
            let url = format!("{API_BASE}/repos/{}/{}/releases", self.owner, self.repo);
            let response = self
                .client
                .get(url)
                .query(&PageQuery {
                    per_page,
                    page,
                    q: None,
                })
                .send()
                .context("Failed to list releases.")?
                .error_for_status()
                .context("GitHub release list request returned an error.")?;

            let page_releases: Vec<ReleaseInfo> = response.json()?;
            let count = page_releases.len();
            releases.extend(page_releases);

            if count < per_page as usize {
                break;
            }

            page += 1;
        }

        Ok(releases)
    }

    pub fn delete_release(&self, release_id: u64) -> Result<()> {
        let url = format!(
            "{API_BASE}/repos/{}/{}/releases/{release_id}",
            self.owner, self.repo
        );
        self.client
            .delete(url)
            .send()
            .context("Failed to delete release.")?
            .error_for_status()
            .context("GitHub release delete request returned an error.")?;
        Ok(())
    }

    pub fn update_release(
        &self,
        release_id: u64,
        tag_name: &str,
        name: &str,
        body: &str,
        prerelease: bool,
        target_commitish: &str,
    ) -> Result<ReleaseInfo> {
        let url = format!(
            "{API_BASE}/repos/{}/{}/releases/{release_id}",
            self.owner, self.repo
        );
        let payload = ReleaseRequest {
            tag_name,
            name,
            body,
            draft: true,
            prerelease,
            target_commitish,
        };
        let response = self
            .client
            .patch(url)
            .json(&payload)
            .send()
            .context("Failed to update release.")?
            .error_for_status()
            .context("GitHub release update request returned an error.")?;
        let release = response.json()?;
        Ok(release)
    }

    pub fn create_release(
        &self,
        tag_name: &str,
        name: &str,
        body: &str,
        prerelease: bool,
        target_commitish: &str,
    ) -> Result<ReleaseInfo> {
        let url = format!("{API_BASE}/repos/{}/{}/releases", self.owner, self.repo);
        let payload = ReleaseRequest {
            tag_name,
            name,
            body,
            draft: true,
            prerelease,
            target_commitish,
        };
        let response = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .context("Failed to create release.")?
            .error_for_status()
            .context("GitHub release create request returned an error.")?;
        let release = response.json()?;
        Ok(release)
    }

    pub fn fetch_merged_pull_requests(
        &self,
        branch: &str,
        since: Option<&str>,
        per_page: u32,
    ) -> Result<Vec<PullRequestInfo>> {
        let mut query_parts = vec![
            format!("repo:{}/{}", self.owner, self.repo),
            "is:pr".to_string(),
            "is:merged".to_string(),
            format!("base:{branch}"),
        ];
        if let Some(since) = since {
            query_parts.push(format!("merged:>={since}"));
        }
        let query = query_parts.join(" ");

        let mut pull_requests = Vec::new();
        let mut page = 1_u32;

        loop {
            let url = format!("{API_BASE}/search/issues");
            let response = self
                .client
                .get(url)
                .query(&PageQuery {
                    per_page,
                    page,
                    q: Some(query.as_str()),
                })
                .send()
                .context("Failed to search pull requests.")?
                .error_for_status()
                .context("GitHub pull request search returned an error.")?;

            let data: SearchResponse = response.json()?;
            let count = data.items.len();
            pull_requests.extend(data.items.into_iter().map(|item| {
                PullRequestInfo {
                    number: item.number,
                    title: item.title,
                    author: item
                        .user
                        .map(|user| user.login)
                        .unwrap_or_else(|| "unknown".to_string()),
                    labels: item.labels.into_iter().map(|label| label.name).collect(),
                    url: format!(
                        "https://github.com/{}/{}/pull/{}",
                        self.owner, self.repo, item.number
                    ),
                    merged_at: item.merged_at,
                }
            }));

            if count < per_page as usize {
                break;
            }
            page += 1;
        }

        Ok(pull_requests)
    }
}
