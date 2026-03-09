use pi_daemon_types::config::GitHubConfig;
use pi_daemon_types::error::{DaemonError, DaemonResult};
use serde::Deserialize;
use tracing::debug;

/// GitHub authenticated user info.
#[derive(Debug, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    pub name: Option<String>,
    pub id: u64,
}

/// GitHub repository info.
#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    pub full_name: String,
    pub private: bool,
    pub html_url: String,
    pub description: Option<String>,
}

/// Verify the GitHub PAT is valid and return the authenticated user.
pub async fn verify_github_auth(config: &GitHubConfig) -> DaemonResult<GitHubUser> {
    if config.personal_access_token.is_empty() {
        return Err(DaemonError::Config("GitHub PAT not configured".to_string()));
    }

    let base_url = &config.api_base_url;
    debug!("Verifying GitHub auth with API at {}", base_url);

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base_url}/user"))
        .header(
            "Authorization",
            format!("Bearer {}", config.personal_access_token),
        )
        .header("User-Agent", "pi-daemon")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| DaemonError::Config(format!("GitHub API request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(DaemonError::Config(format!(
            "GitHub auth failed (HTTP {})",
            resp.status()
        )));
    }

    let user = resp
        .json::<GitHubUser>()
        .await
        .map_err(|e| DaemonError::Config(format!("Failed to parse GitHub user: {e}")))?;

    debug!("GitHub auth successful for user: {}", user.login);
    Ok(user)
}

/// List private repos accessible with the PAT.
pub async fn list_repos(config: &GitHubConfig, page: u32) -> DaemonResult<Vec<GitHubRepo>> {
    if config.personal_access_token.is_empty() {
        return Err(DaemonError::Config("GitHub PAT not configured".to_string()));
    }

    let base_url = &config.api_base_url;
    debug!("Listing repos from {} (page {})", base_url, page);

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base_url}/user/repos"))
        .query(&[
            ("page", page.to_string()),
            ("per_page", "30".to_string()),
            ("visibility", "private".to_string()),
        ])
        .header(
            "Authorization",
            format!("Bearer {}", config.personal_access_token),
        )
        .header("User-Agent", "pi-daemon")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| DaemonError::Config(format!("GitHub API request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(DaemonError::Config(format!(
            "GitHub repos request failed (HTTP {})",
            resp.status()
        )));
    }

    let repos = resp
        .json::<Vec<GitHubRepo>>()
        .await
        .map_err(|e| DaemonError::Config(format!("Failed to parse repos: {e}")))?;

    debug!("Found {} repos on page {}", repos.len(), page);
    Ok(repos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_github_auth_empty_token() {
        let config = GitHubConfig {
            personal_access_token: String::new(),
            api_base_url: "https://api.github.com".to_string(),
            default_owner: String::new(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(verify_github_auth(&config));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("GitHub PAT not configured"));
    }

    #[test]
    fn test_list_repos_empty_token() {
        let config = GitHubConfig {
            personal_access_token: String::new(),
            api_base_url: "https://api.github.com".to_string(),
            default_owner: String::new(),
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(list_repos(&config, 1));

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("GitHub PAT not configured"));
    }

    // Note: Real GitHub API tests would use a mock server to avoid hitting the actual API
    // For now, these tests just verify the error handling for missing tokens
}
