use brie_cfg::ReleaseVersion;

pub mod github;
pub mod gitlab;

pub struct GitRepo<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
}

impl<'a> GitRepo<'a> {
    pub fn new(owner: &'a str, repo: &'a str) -> GitRepo<'a> {
        GitRepo { owner, repo }
    }
}

impl<'a> std::fmt::Display for GitRepo<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.repo)
    }
}

#[derive(Debug)]
pub struct Release {
    pub version: String,
    pub filename: String,
    pub url: String,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Unable to get release data. {0}")]
    ReleaseGet(#[from] Box<ureq::Error>),
    #[error("Unable to parse release data. {0}")]
    ReleaseParse(#[from] std::io::Error),
    #[error("No asset matching predicate found.")]
    NoMatchingAsset,
}

pub trait ReleaseProvider {
    fn get_release(&self, repo: &GitRepo<'_>, version: &ReleaseVersion) -> Result<Release, Error>;
}
