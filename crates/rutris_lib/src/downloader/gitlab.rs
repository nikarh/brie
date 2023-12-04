use derive_more::Constructor;
use log::info;
use serde::Deserialize;

use super::{Error, GitRepo, Release, ReleaseProvider, ReleaseVersion, USER_AGENT_HEADER};

#[derive(Deserialize, Debug)]
pub struct GlFile {
    pub name: String,
    pub path: String,
}

#[derive(Constructor)]
pub struct Gitlab<'a, E>
where
    E: for<'b> Fn(&'b str) -> Option<&'b str>,
{
    tree_path: &'a str,
    version_extractor: E,
}

impl<'a, E> ReleaseProvider for Gitlab<'a, E>
where
    E: for<'b> Fn(&'b str) -> Option<&'b str>,
{
    fn get_release(&self, repo: &GitRepo<'_>, version: &ReleaseVersion) -> Result<Release, Error> {
        let url = format!(
            "https://gitlab.com/api/v4/projects/{repo}/repository/tree?path={tree_path}",
            repo = format!("{repo}").replace('/', "%2F"),
            tree_path = self.tree_path.replace('/', "%2F")
        );

        info!("Downloading {version} release metadata from {}", url);

        let mut releases: Vec<GlFile> = ureq::get(&url)
            .set("User-Agent", USER_AGENT_HEADER)
            .call()
            .map_err(Box::new)?
            .into_json()?;

        let release = match version {
            ReleaseVersion::Latest => {
                releases.sort_by(|a, b| a.name.cmp(&b.name));
                releases.into_iter().last()
            }
            ReleaseVersion::Tag(tag) => {
                let sub = format!("{repo}-{tag}.", repo = repo.repo);
                releases.into_iter().find(|r| r.name.contains(&sub))
            }
        };

        let release = release.ok_or(Error::NoMatchingAsset)?;
        let version = (self.version_extractor)(&release.name)
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
        gitlab::{filename_version, Gitlab},
        GitRepo, ReleaseProvider, ReleaseVersion,
    };

    #[test]
    fn download_dxvk_async() {
        let repo = GitRepo::new("Ph42oN", "dxvk-gplasync");

        let downloader = Gitlab::new("releases", filename_version("dxvk-gplasync-", ".tar.gz"));

        let latest = downloader
            .get_release(&repo, &ReleaseVersion::Latest)
            .unwrap();
        let older = downloader
            .get_release(&repo, &ReleaseVersion::Tag("v2.1-3".into()))
            .unwrap();

        assert_ne!(latest.version, "v2.1-3");
        assert!(latest.url.starts_with("https"));

        assert_eq!(older.version, "v2.1-3");
        assert!(older.url.starts_with("https"));
    }
}
