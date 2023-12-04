use std::{io, path::Path};

use thiserror::Error;

use crate::{
    config::Runtime,
    libraries::{ensure_library_exists, LibraryDownloadError, WineGe},
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("System wine runtime not found. {0}")]
    Which(#[from] which::Error),
    #[error("IO Error. {0}")]
    Io(#[from] io::Error),
    #[error("Download error. {0}")]
    Library(#[from] LibraryDownloadError),
}

/// This function checks if a requested runtime exists, and downloads it if it doesn't.
/// In case native runtime was requested, simply checks that wine binary
/// is either accessible by it's optional path or is in env.PATH.
///
/// In case of success returns a full path to wine binary.
pub(crate) fn get_runtime(
    cache_path: impl AsRef<Path>,
    runtime: &Runtime,
) -> Result<std::path::PathBuf, Error> {
    Ok(match runtime {
        Runtime::System(None) => which::which("wine")?,
        Runtime::System(Some(path)) => which::which(path.join("wine"))?,
        Runtime::GeProton(version) => {
            let wine_path = cache_path
                .as_ref()
                .join(version.as_path())
                .join("bin")
                .join("wine");

            ensure_library_exists(&WineGe, cache_path, version)?;

            wine_path
        }
    })
}
