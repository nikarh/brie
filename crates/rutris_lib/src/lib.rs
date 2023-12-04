use indicatif::MultiProgress;
use lazy_static::lazy_static;

pub use config::{Library, Paths, ReleaseVersion, Runtime, Unit};
pub use run::{run, Error};

pub use dll::{CopyError, Error as DllError, InstallLibraryError, OverrideError};
pub use downloader::Error as DownloadError;
pub use prepare::{BeforeError, MountsError, WinePrefixError, WinetricksError};
pub use runtime::Error as RuntimeError;

mod command;
mod config;
mod dll;
mod downloader;
mod library;
mod prepare;
mod rayon_join;
mod run;
mod runtime;

lazy_static! {
    pub static ref MP: MultiProgress = MultiProgress::new();
}

trait WithContext<Target, Context> {
    fn context(self, context: Context) -> Target;
}
