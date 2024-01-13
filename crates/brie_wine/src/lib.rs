use std::path::{Path, PathBuf};

use brie_cfg::{Library, ReleaseVersion, Runtime};
use indexmap::IndexMap;

pub use launch::{launch, Error};

pub use brie_download::mp;
pub use dll::{CopyError, Error as DllError};
pub use downloader::Error as DownloadError;
pub use prepare::{BeforeError, MountsError, WinePrefixError, WinetricksError};
pub use runtime::Error as RuntimeError;

mod command;
mod dll;
mod downloader;
mod launch;
mod library;
mod prepare;
mod rayon_join;
mod runtime;
mod state;

trait WithContext<Target, Context> {
    fn context(self, context: Context) -> Target;
}

#[derive(Debug)]
pub struct Unit {
    pub runtime: Runtime,
    pub libraries: IndexMap<Library, ReleaseVersion>,

    pub env: IndexMap<String, String>,
    pub prefix: String,

    pub mounts: IndexMap<char, String>,
    pub before: Vec<Vec<String>>,
    pub winetricks: Vec<String>,

    pub cd: Option<String>,
    pub command: Vec<String>,
    pub wrapper: Vec<String>,
}

#[derive(Debug)]
pub struct Paths {
    pub libraries: PathBuf,
    pub prefixes: PathBuf,
}

impl Paths {
    #[must_use]
    pub fn new(data_home: &Path) -> Self {
        Self {
            libraries: data_home.join("libraries"),
            prefixes: data_home.join("prefixes"),
        }
    }
}
