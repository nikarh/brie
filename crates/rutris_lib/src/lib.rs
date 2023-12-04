use std::{collections::HashMap, io};

use config::{Config, Paths};
use indicatif::MultiProgress;
use lazy_static::lazy_static;
use libraries::{ensure_library_exists, LibraryDownloadError};
use log::info;
use prepare::{BeforeError, MountsError, WinePrefixError, WinetricksError};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use run::CommandRunner;
use runtime::get_runtime;
use thiserror::Error;

pub mod config;
mod dll;
mod downloader;
mod libraries;
mod prepare;
mod run;
pub mod runtime;

lazy_static! {
    static ref MP: MultiProgress = MultiProgress::new();
}

#[derive(Error, Debug)]
pub enum LaunchError {
    #[error("Runtime error. {0}")]
    Runtime(#[from] runtime::Error),
    #[error("Library download error. {0}")]
    LibraryDownload(#[from] LibraryDownloadError),
    #[error("Library installation error. {0}")]
    LibraryInstall(#[from] dll::InstallError),
    #[error("Unable to set up wine prefix. {0}")]
    Prefix(#[from] WinePrefixError),
    #[error("Winetricks error. {0}")]
    Tricks(#[from] WinetricksError),
    #[error("Unable to symlink mounts. {0}")]
    Mounts(#[from] MountsError),
    #[error("Before command error. {0}")]
    Before(#[from] BeforeError),
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
}

pub fn launch(paths: Paths, config: Config) -> Result<(), LaunchError> {
    info!("Launching with config: {config:#?}");
    info!("Paths: {paths:#?}");

    let (runtime, libraries) = rayon::join(
        || get_runtime(&paths.libraries, &config.runtime),
        || {
            config
                .libraries
                .par_iter()
                .map(|(l, version)| {
                    ensure_library_exists(l, &paths.libraries, version).map(|path| (*l, path))
                })
                .collect::<Result<HashMap<_, _>, _>>()
        },
    );

    let runtime = runtime?;
    let libraries = libraries?;

    // FIXME: download cabextract and winetricks

    let runner = CommandRunner::new(runtime, config.env, paths.prefixes, &config.prefix)?;

    runner.prepare_wine_prefix()?;
    runner.winetricks(&config.winetricks)?;
    runner.mounts(&config.mounts)?;
    runner.install_libraries(&libraries)?;
    runner.before(&config.before)?;

    runner.run("wineserver", &["--wait"])?;
    // Run game
    runner.run("wineserver", &["--wait"])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::Path};

    use indicatif_log_bridge::LogWrapper;

    use crate::{
        config::{Config, Library, Paths, ReleaseVersion, Runtime},
        launch, MP,
    };

    #[test]
    pub fn test_run() {
        let mut builder = pretty_env_logger::formatted_builder();
        if let Ok(s) = ::std::env::var("RUST_LOG") {
            builder.parse_filters(&s);
        }

        LogWrapper::new(MP.clone(), builder.build())
            .try_init()
            .unwrap();

        launch(
            Paths {
                libraries: Path::new(".tmp").join("libraries"),
                prefixes: Path::new(".tmp").join("prefixes"),
            },
            Config {
                runtime: Runtime::GeProton(ReleaseVersion::Latest),
                libraries: [
                    (Library::DxvkGplAsync, ReleaseVersion::Latest),
                    (Library::DxvkNvapi, ReleaseVersion::Latest),
                    (Library::Vkd3dProton, ReleaseVersion::Latest),
                    // (Library::NvidiaLibs, ReleaseVersion::Latest),
                ]
                .into(),
                env: HashMap::default(),
                prefix: "TEST_PREFIX".into(),

                cd: String::default(),
                command: vec![],
                mounts: [('r', "/etc".into())].into(),
                before: vec![],
                winetricks: vec![],
            },
        )
        .unwrap();
    }
}
