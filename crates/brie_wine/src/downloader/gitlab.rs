use brie_download::ureq;
use log::info;
use serde::Deserialize;

use super::{Error, GitRepo, Release, ReleaseVersion};

#[derive(Deserialize, Debug)]
pub struct GlFile {
    pub name: String,
    pub path: String,
}

pub struct Client;

impl Client {
    #[allow(clippy::unused_self)]
    pub fn tree_file(
        &self,
        repo: GitRepo<'_>,
        version: &ReleaseVersion,
        tree_path: &str,
        version_extractor: impl for<'b> Fn(&'b str) -> Option<&'b str>,
    ) -> Result<Release, Error> {
        let url = format!(
            "https://gitlab.com/api/v4/projects/{repo}/repository/tree?path={tree_path}",
            repo = format!("{repo}").replace('/', "%2F"),
            tree_path = tree_path.replace('/', "%2F")
        );

        info!("Downloading {version:?} release metadata from {url}");

        let mut releases: Vec<GlFile> = ureq()?.get(&url).call().map_err(Box::new)?.into_json()?;

        let release = match version {
            ReleaseVersion::Latest => {
                releases.sort_by(|a, b| a.name.cmp(&b.name));
                releases.into_iter().next_back()
            }
            ReleaseVersion::Tag(tag) => {
                let sub = format!("{repo}-{tag}.", repo = repo.repo);
                releases.into_iter().find(|r| r.name.contains(&sub))
            }
        };

        let release = release.ok_or(Error::NoMatchingAsset)?;
        let version = version_extractor(&release.name)
            .ok_or(Error::NoMatchingAsset)?
            .to_owned();
        let filename = release.name;

        let url = format!(
            "https://gitlab.com/{repo}/-/raw/main/{path}?ref_type=heads&inline=false",
            repo = repo,
            path = release.path
        );

        Ok(Release {
            version,
            filename,
            url,
        })
    }
}

/// A simple prefix+suffix file name based version extractor
pub fn filename_version<'a>(
    prefix: &'a str,
    suffix: &'a str,
) -> impl for<'b> Fn(&'b str) -> Option<&'b str> + 'a {
    move |filename| {
        filename
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
    }
}

#[cfg(test)]
mod test {
    use crate::downloader::{
        gitlab::{filename_version, Client},
        GitRepo, ReleaseVersion,
    };

    #[test]
    fn download_dxvk_async() {
        let repo = GitRepo::new("Ph42oN", "dxvk-gplasync");
        let tree_path = "releases";
        let extractor = || filename_version("dxvk-gplasync-", ".tar.gz");

        let latest = Client
            .tree_file(repo, &ReleaseVersion::Latest, tree_path, extractor())
            .unwrap();
        let older = Client
            .tree_file(
                repo,
                &ReleaseVersion::Tag("v2.1-3".into()),
                tree_path,
                extractor(),
            )
            .unwrap();

        assert_ne!(latest.version, "v2.1-3");
        assert!(latest.url.starts_with("https"));

        assert_eq!(older.version, "v2.1-3");
        assert!(older.url.starts_with("https"));
    }
}
