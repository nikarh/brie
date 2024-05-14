use std::{path::Path, time::Duration};

use brie_cfg::{Runtime, Tokens};

use crate::library::{self, ensure_library_exists, WineGe, WineTkg};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("System wine runtime not found. {0}")]
    Which(#[from] which::Error),
    #[error("Download error. {0}")]
    Library(#[from] library::Error),
}

/// This function checks if a requested runtime exists, and downloads it if it doesn't.
/// In case native runtime was requested, simply checks that wine binary
/// is either accessible by it's optional path or is in $PATH env.
///
/// In case of success returns a full path to wine binary.
pub fn ensure_runtime_exists(
    tokens: &Tokens,
    library_dir: impl AsRef<Path>,
    runtime: &Runtime,
    time_since_update: Option<Duration>,
) -> Result<library::State, Error> {
    Ok(match runtime {
        Runtime::System { path: None } => library::State::untouched(which::which("wine")?),
        Runtime::System { path: Some(path) } => {
            library::State::untouched(which::which(path.join("wine"))?)
        }
        Runtime::Tkg { version } => {
            let state =
                ensure_library_exists(&WineTkg, library_dir, tokens, version, time_since_update)?;
            library::State {
                path: state.path.join("usr").join("bin").join("wine"),
                updated: state.updated,
            }
        }
        Runtime::GeProton { version } => {
            let state =
                ensure_library_exists(&WineGe, library_dir, tokens, version, time_since_update)?;
            library::State {
                path: state.path.join("bin").join("wine"),
                updated: state.updated,
            }
        }
    })
}
