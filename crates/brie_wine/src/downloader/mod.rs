use brie_cfg::ReleaseVersion;
use brie_download::TlsError;

pub mod github;
pub mod gitlab;

#[derive(Clone, Copy)]
pub struct GitRepo<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
}

impl<'a> GitRepo<'a> {
    pub fn new(owner: &'a str, repo: &'a str) -> GitRepo<'a> {
        GitRepo { owner, repo }
    }
}

impl std::fmt::Display for GitRepo<'_> {
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
    #[error("TLS error. {0}")]
    Tls(#[from] &'static TlsError),
    #[error("Unable to get release data. {0}")]
    ReleaseGet(#[from] Box<ureq::Error>),
    #[error("Unable to parse release data. {0}")]
    ReleaseParse(#[from] std::io::Error),
    #[error("No asset matching predicate found.")]
    NoMatchingAsset,
}
