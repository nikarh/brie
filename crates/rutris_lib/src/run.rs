use std::{collections::HashMap, fs, io};

use fslock::LockFile;
use log::info;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{
    command::Runner,
    library::{
        ensure_cabextract_exists, ensure_library_exists, ensure_winetricks_exists, Downloadable,
    },
    runtime, WithContext,
};
use crate::{
    config::{Paths, Unit},
    prepare::{BeforeError, MountsError, WinePrefixError, WinetricksError},
};
use crate::{dll, library};
use crate::{join, runtime::get_runtime};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Runtime error. {0}")]
    Runtime(#[from] runtime::Error),
    #[error("Library `{0}` download error. {1}")]
    LibraryDownload(&'static str, library::Error),
    #[error("Library installation error. {0}")]
    LibraryInstall(#[from] dll::Error),
    #[error("Unable to set up wine prefix. {0}")]
    Prefix(#[from] WinePrefixError),
    #[error("Winetricks error. {0}")]
    Tricks(#[from] WinetricksError),
    #[error("Unable to symlink mounts. {0}")]
    Mounts(#[from] MountsError),
    #[error("Before command error. {0}")]
    Before(#[from] BeforeError),
    #[error("Lock error. {0}")]
    Lock(#[source] io::Error),
    #[error("Unable to create libraries folder. {0}")]
    Libraries(#[source] io::Error),
    #[error("Command runner error. {0}")]
    Runner(#[source] io::Error),
    #[error("Wineserver wait error. {0}")]
    Wait(#[source] io::Error),
    #[error("Run error. {0}")]
    Run(#[source] io::Error),
}

impl<T> WithContext<Result<T, Error>, &'static str> for Result<T, library::Error> {
    fn context(self, context: &'static str) -> Result<T, Error> {
        self.map_err(|e| Error::LibraryDownload(context, e))
    }
}

pub fn run(paths: &Paths, config: Unit) -> Result<(), Error> {
    info!("Preparing environment with config: {config:#?}");
    info!("Paths: {paths:?}");

    info!("Obtaining a lock on dependency download");
    fs::create_dir_all(&paths.libraries).map_err(Error::Libraries)?;
    let mut lock = LockFile::open(&paths.libraries.join(".rutris-lock")).map_err(Error::Lock)?;
    lock.lock_with_pid().map_err(Error::Lock)?;

    let (runtime, winetricks, cabextract, libraries) = join!(
        || get_runtime(&paths.libraries, &config.runtime),
        || ensure_winetricks_exists(&paths.libraries).context("winetricks"),
        || ensure_cabextract_exists(&paths.libraries).context("cabextract"),
        || {
            config
                .libraries
                .par_iter()
                .map(|(l, version)| {
                    ensure_library_exists(l, &paths.libraries, version)
                        .map(|path| (*l, path))
                        .context(l.name())
                })
                .collect::<Result<HashMap<_, _>, _>>()
        }
    );

    drop(lock);

    let runtime = runtime?;
    let libraries = libraries?;
    winetricks?;
    cabextract?;

    let runner = Runner::new(runtime, config.env, paths, &config.prefix).map_err(Error::Runner)?;
    runner.prepare_wine_prefix()?;

    info!("Obtaining a lock on wineprefix");
    let mut lock =
        LockFile::open(&runner.wine_prefix().join(".rutris-lock")).map_err(Error::Lock)?;
    lock.lock_with_pid().map_err(Error::Lock)?;
    runner.winetricks(&config.winetricks)?;
    runner.mounts(&config.mounts)?;
    runner.install_libraries(&libraries)?;
    runner.before(&config.before)?;
    runner.run("wineserver", &["--wait"]).map_err(Error::Wait)?;
    drop(lock);

    if !config.command.is_empty() {
        info!("Running: {:?} in {:?}", config.command, config.cd);
        let mut command = runner.command("wine", &config.command);
        if let Some(cd) = &config.cd {
            command.current_dir(cd);
        }

        command.status().map_err(Error::Run)?;
    }

    info!("Waiting for wineserver to exit");
    runner.run("wineserver", &["--wait"]).map_err(Error::Wait)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::Path};

    use indicatif_log_bridge::LogWrapper;

    use crate::{
        config::{Library, Paths, ReleaseVersion, Runtime, Unit},
        MP,
    };

    use super::run;

    #[test]
    pub fn test_run() {
        let mut builder = pretty_env_logger::formatted_builder();
        builder.filter_level(log::LevelFilter::Info);
        if let Ok(s) = ::std::env::var("RUST_LOG") {
            builder.parse_filters(&s);
        }

        LogWrapper::new(MP.clone(), builder.build())
            .try_init()
            .unwrap();

        run(
            &Paths {
                libraries: Path::new(".tmp").join("libraries"),
                prefixes: Path::new(".tmp").join("prefixes"),
            },
            Unit {
                runtime: Runtime::GeProton(ReleaseVersion::Latest),
                libraries: [
                    (Library::DxvkGplAsync, ReleaseVersion::Latest),
                    (Library::DxvkNvapi, ReleaseVersion::Latest),
                    (Library::Vkd3dProton, ReleaseVersion::Latest),
                ]
                .into(),
                env: HashMap::default(),
                prefix: "TEST_PREFIX".into(),

                cd: None,
                command: vec!["winecfg".into()],
                mounts: [('r', "/etc".into())].into(),
                before: vec![],
                winetricks: vec![],
            },
        )
        .unwrap();
    }
}
