use brie_download::ureq;
use log::info;
use serde::Deserialize;

use super::{Error, GitRepo, Release, ReleaseVersion};

const ACCEPT_HEADER: &str = "application/vnd.github.v3+json";

#[derive(Deserialize, Debug)]
pub struct GhAsset {
    pub name: String,
    #[serde(alias = "archive_download_url")]
    pub browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhWorkflowRun {
    id: u64,
}

#[derive(Deserialize)]
struct GhWorkflowRuns {
    workflow_runs: Vec<GhWorkflowRun>,
}

#[derive(Deserialize, Debug)]
struct GhArtifacts {
    artifacts: Vec<GhAsset>,
}

pub struct Client<'a> {
    /// GitHub PAT
    token: Option<&'a str>,
}

impl<'a> Client<'a> {
    pub fn new(token: Option<&'a str>) -> Self {
        Self { token }
    }

    pub fn release(
        &self,
        repo: GitRepo<'_>,
        version: &ReleaseVersion,
        matcher: impl Fn(&GhAsset) -> bool,
    ) -> Result<Release, Error> {
        let url = match version {
            ReleaseVersion::Latest => {
                format!("https://api.github.com/repos/{repo}/releases/latest")
            }
            ReleaseVersion::Tag(tag) => {
                format!("https://api.github.com/repos/{repo}/releases/tags/{tag}")
            }
        };

        info!("Downloading {version:?} release metadata from {url}");

        let mut req = ureq()?.get(&url).set("Accept", ACCEPT_HEADER);
        if let Some(token) = self.token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }

        let release: GhRelease = req.call().map_err(Box::new)?.into_json()?;

        let asset = release
            .assets
            .into_iter()
            .find(matcher)
            .ok_or(Error::NoMatchingAsset)?;

        Ok(Release {
            version: release.tag_name,
            filename: asset.name,
            url: asset.browser_download_url,
        })
    }

    pub fn workflow_artifact(
        &self,
        repo: GitRepo<'_>,
        version: &ReleaseVersion,
        workflow_id: u64,
        matcher: impl Fn(&GhAsset) -> bool,
    ) -> Result<Release, Error> {
        let run_id = match version {
            ReleaseVersion::Latest => {
                let url = format!("https://api.github.com/repos/{repo}/actions/workflows/{workflow_id}/runs?status=success&per_page=1");
                info!("Getting workflow run data from {url}");
                let mut req = ureq()?.get(&url).set("Accept", ACCEPT_HEADER);
                if let Some(token) = self.token {
                    req = req.set("Authorization", &format!("Bearer {token}"));
                }

                let response: GhWorkflowRuns = req.call().map_err(Box::new)?.into_json()?;
                let id = response
                    .workflow_runs
                    .first()
                    .ok_or(Error::NoMatchingAsset)?
                    .id;

                format!("{id}")
            }
            ReleaseVersion::Tag(tag) => tag.clone(),
        };

        // Get the workflow run
        let url = format!("https://api.github.com/repos/{repo}/actions/runs/{run_id}/artifacts");

        info!("Downloading {run_id:?} workflow run metadata from {url}");
        let mut req = ureq()?.get(&url).set("Accept", ACCEPT_HEADER);
        if let Some(token) = self.token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }

        let response: GhArtifacts = req.call().map_err(Box::new)?.into_json()?;

        let asset = response
            .artifacts
            .into_iter()
            .find(matcher)
            .ok_or(Error::NoMatchingAsset)?;

        Ok(Release {
            version: run_id,
            filename: asset.name,
            url: asset.browser_download_url,
        })
    }
}

/// A simple matcher that checks if the asset name ends with the given suffix.
pub fn with_suffix(suffix: &str) -> impl Fn(&GhAsset) -> bool + '_ {
    move |asset| asset.name.ends_with(suffix)
}

#[cfg(test)]
mod test {
    use brie_cfg::{ReleaseVersion, Tokens};

    use crate::{
        downloader::{
            github::{with_suffix, Client},
            GitRepo,
        },
        library::{Downloadable, WineTkg},
    };

    #[test]
    fn download_vkd3d() {
        let client = Client::new(None);
        let repo = GitRepo::new("HansKristian-Work", "vkd3d-proton");
        let matcher = || with_suffix(".tar.zst");

        let latest = client
            .release(repo, &ReleaseVersion::Latest, matcher())
            .unwrap();
        let older = client
            .release(repo, &ReleaseVersion::Tag("v2.9".into()), matcher())
            .unwrap();

        assert_ne!(latest.version, "v2.9");
        assert!(latest.url.starts_with("https"));

        assert_eq!(older.version, "v2.9");
        assert!(older.url.starts_with("https"));
    }

    #[test]
    fn download_tkg() {
        let latest = WineTkg
            .get_meta(&Tokens::default(), &ReleaseVersion::Latest)
            .unwrap();
        let older = WineTkg
            .get_meta(
                &Tokens::default(),
                &ReleaseVersion::Tag("8992124483".into()),
            )
            .unwrap();

        assert_ne!(latest.version, "8992124483");
        assert!(latest.url.starts_with("https"));

        assert_eq!(older.version, "8992124483");
        assert!(older.url.starts_with("https"));
    }
}
