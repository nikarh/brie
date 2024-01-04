use brie_download::ureq;
use log::info;
use serde::Deserialize;

use super::{Error, GitRepo, Release, ReleaseProvider, ReleaseVersion};

const ACCEPT_HEADER: &str = "application/vnd.github.v3+json";

#[derive(Deserialize, Debug)]
pub struct GhAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Deserialize)]
pub struct GhRelease {
    pub tag_name: String,
    pub name: String,
    pub assets: Vec<GhAsset>,
}

pub struct Github<M> {
    asset_matcher: M,
}

impl<M> Github<M>
where
    M: Fn(&GhAsset) -> bool,
{
    pub fn new(asset_matcher: M) -> Self {
        Self { asset_matcher }
    }
}

impl<M> ReleaseProvider for Github<M>
where
    M: Fn(&GhAsset) -> bool,
{
    fn get_release(&self, repo: &GitRepo<'_>, version: &ReleaseVersion) -> Result<Release, Error> {
        let url = match version {
            ReleaseVersion::Latest => {
                format!("https://api.github.com/repos/{repo}/releases/latest")
            }
            ReleaseVersion::Tag(tag) => {
                format!("https://api.github.com/repos/{repo}/releases/tags/{tag}")
            }
        };

        info!("Downloading {version:?} release metadata from {}", url);

        let release: GhRelease = ureq()?
            .get(&url)
            .set("Accept", ACCEPT_HEADER)
            .call()
            .map_err(Box::new)?
            .into_json()?;

        let asset = release
            .assets
            .into_iter()
            .find(|asset| (self.asset_matcher)(asset))
            .ok_or(Error::NoMatchingAsset)?;

        Ok(Release {
            version: release.tag_name,
            filename: asset.name,
            url: asset.browser_download_url,
        })
    }
}

pub fn with_suffix(suffix: &str) -> impl Fn(&GhAsset) -> bool + '_ {
    move |asset| asset.name.ends_with(suffix)
}

#[cfg(test)]
mod test {
    use crate::downloader::{
        github::{with_suffix, Github},
        GitRepo, ReleaseProvider, ReleaseVersion,
    };

    #[test]
    fn download_vkd3d() {
        let repo = GitRepo::new("HansKristian-Work", "vkd3d-proton");
        let downloader = Github::new(with_suffix(".tar.zst"));

        let latest = downloader
            .get_release(&repo, &ReleaseVersion::Latest)
            .unwrap();
        let older = downloader
            .get_release(&repo, &ReleaseVersion::Tag("v2.9".into()))
            .unwrap();

        assert_ne!(latest.version, "v2.9");
        assert!(latest.url.starts_with("https"));

        assert_eq!(older.version, "v2.9");
        assert!(older.url.starts_with("https"));
    }
}
