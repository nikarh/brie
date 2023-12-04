use std::io;

use derive_more::{Constructor, Display};

use crate::config::ReleaseVersion;

pub mod github;
pub mod gitlab;

const USER_AGENT_HEADER: &str = "github.com/nikarh/rutris";

#[derive(Constructor, Display)]
#[display(fmt = "{owner}/{repo}")]
pub struct GitRepo<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
}

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

pub struct ReleaseStream<R: io::Read> {
    pub body: R,
    pub len: Option<usize>,
}

pub fn download_file(url: &str) -> Result<ReleaseStream<impl io::Read>, Box<ureq::Error>> {
    let response = ureq::get(url)
        .set("User-Agent", USER_AGENT_HEADER)
        .call()
        .map_err(Box::new)?;

    let len = response
        .header("Content-Length")
        .and_then(|h| h.parse::<usize>().ok());

    let body = response.into_reader();

    Ok(ReleaseStream { body, len })
}
