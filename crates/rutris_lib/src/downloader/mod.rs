use std::io;

use derive_more::{Constructor, Display};
use thiserror::Error;

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

#[derive(Error, Debug)]
pub enum ReleaseError {
    #[error("Unable to get release data. {0}")]
    ReleaseGet(#[from] Box<ureq::Error>),
    #[error("Unable to parse release data. {0}")]
    ReleaseParse(#[from] std::io::Error),
    #[error("No asset matching predicate found.")]
    NoMatchingAsset,
}

pub trait ReleaseProvider {
    fn get_release(
        &self,
        repo: &GitRepo<'_>,
        version: &ReleaseVersion,
    ) -> Result<Release, ReleaseError>;
}

pub struct ReleaseStream<R: io::Read> {
    pub body: R,
    pub len: Option<usize>,
}

pub fn download_release(
    release: &Release,
) -> Result<ReleaseStream<impl io::Read>, Box<ureq::Error>> {
    let response = ureq::get(&release.url)
        .set("User-Agent", USER_AGENT_HEADER)
        .call()
        .map_err(Box::new)?;

    let len = response
        .header("Content-Length")
        .and_then(|h| h.parse::<usize>().ok());

    let body = response.into_reader();

    Ok(ReleaseStream { body, len })
}
