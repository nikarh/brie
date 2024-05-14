use std::{
    borrow::Cow,
    io,
    sync::{Arc, OnceLock},
};

pub use native_tls::Error as TlsError;

use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressState, ProgressStyle};

pub const USER_AGENT_HEADER: &str = "github.com/nikarh/brie";

pub fn mp() -> &'static MultiProgress {
    static MP: OnceLock<MultiProgress> = OnceLock::new();
    MP.get_or_init(MultiProgress::new)
}

pub fn ureq() -> Result<&'static ureq::Agent, &'static native_tls::Error> {
    static AGENT: OnceLock<Result<ureq::Agent, native_tls::Error>> = OnceLock::new();
    AGENT
        .get_or_init(|| {
            Ok(ureq::AgentBuilder::new()
                .user_agent(USER_AGENT_HEADER)
                .tls_connector(Arc::new(native_tls::TlsConnector::new()?))
                .build())
        })
        .as_ref()
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("TLS error. {0}")]
    Tls(#[from] &'static TlsError),
    #[error("Http error. {0}")]
    Ureq(#[from] Box<ureq::Error>),
}

pub fn download_file(
    url: &str,
    authorization: Option<&str>,
) -> Result<DownloadStream<impl io::Read>, Error> {
    let req = match authorization {
        Some(header) => ureq()?.get(url).set("Authorization", header),
        None => ureq()?.get(url),
    };

    let response = req.call().map_err(Box::new)?;

    let len = response
        .header("Content-Length")
        .and_then(|h| h.parse::<usize>().ok());

    let body = response.into_reader();

    Ok(DownloadStream { body, len })
}

pub struct DownloadStream<R: io::Read> {
    pub body: R,
    pub len: Option<usize>,
}

impl<R: io::Read> DownloadStream<R> {
    #[allow(clippy::missing_panics_doc)]
    pub fn progress(self, name: impl Into<Cow<'static, str>>) -> (impl io::Read, ProgressBar) {
        let pb = match self.len {
            Some(len) => ProgressBar::new(len as u64),
            None => ProgressBar::new_spinner(),
        };

        let pb = pb
        .with_message(name)
        .with_finish(ProgressFinish::AndLeave)
        .with_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta}) - {msg:>15}")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));

        let pb = mp().add(pb);

        (pb.wrap_read(self.body), pb)
    }
}
