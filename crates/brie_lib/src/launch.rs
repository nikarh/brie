use std::{borrow::Cow, collections::BTreeMap, env::VarError, fs, io, path::Path};

use fslock::LockFile;
use log::info;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{
    command::Runner,
    library::{
        ensure_cabextract_exists, ensure_library_exists, ensure_winetricks_exists, Downloadable,
    },
    runtime, state, WithContext,
};
use crate::{dll, library};
use crate::{join, runtime::ensure_runtime_exists};
use crate::{
    prepare::{BeforeError, MountsError, WinePrefixError, WinetricksError},
    Paths, Unit,
};

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
    #[error("Unable to write state file. {0}")]
    StateWrite(#[source] io::Error),
    #[error("Unable to create libraries folder. {0}")]
    Libraries(#[source] io::Error),
    #[error("Command runner error. {0}")]
    Runner(#[source] io::Error),
    #[error("Wineserver wait error. {0}")]
    Wait(#[source] io::Error),
    #[error("Run error. {0}")]
    Run(#[source] io::Error),
    #[error("Unable to expand path. {0}")]
    Expand(#[from] shellexpand::LookupError<VarError>),
}

impl<T> WithContext<Result<T, Error>, &'static str> for Result<T, library::Error> {
    fn context(self, context: &'static str) -> Result<T, Error> {
        self.map_err(|e| Error::LibraryDownload(context, e))
    }
}

pub fn launch(paths: &Paths, unit: Unit) -> Result<(), Error> {
    info!("Preparing to launch unit: {unit:#?}");
    info!("Paths: {paths:?}");

    info!("Obtaining a lock on dependency download");
    fs::create_dir_all(&paths.libraries).map_err(Error::Libraries)?;
    let mut lock = LockFile::open(&paths.libraries.join(".brie.lock")).map_err(Error::Lock)?;
    lock.lock_with_pid().map_err(Error::Lock)?;

    let mut state = state::read(&paths.libraries);

    // Download all dependencies in parallel
    let (wine, winetricks, cabextract, libraries) = join!(
        || ensure_runtime_exists(
            &paths.libraries,
            &unit.runtime,
            state.wine.and_then(|t| t.elapsed().ok())
        ),
        || ensure_winetricks_exists(&paths.libraries).context("winetricks"),
        || ensure_cabextract_exists(&paths.libraries).context("cabextract"),
        || {
            unit.libraries
                .par_iter()
                .map(|(l, version)| {
                    ensure_library_exists(
                        l,
                        &paths.libraries,
                        version,
                        state.libraries.get(l).and_then(|t| t.elapsed().ok()),
                    )
                    .map(|path| (*l, path))
                    .context(l.name())
                })
                .collect::<Result<BTreeMap<_, _>, _>>()
        }
    );

    drop(lock);

    let wine = wine?;
    let libraries = libraries?;
    winetricks?;
    cabextract?;

    if wine.updated {
        state.wine = Some(std::time::SystemTime::now());
    }

    for (l, s) in &libraries {
        if s.updated {
            state.libraries.insert(*l, std::time::SystemTime::now());
        }
    }

    state::write(&paths.libraries, &state).map_err(Error::StateWrite)?;

    let libraries = libraries
        .into_iter()
        .map(|(l, path)| (l, path.path))
        .collect::<BTreeMap<_, _>>();

    let runner =
        Runner::new(paths, wine.path, unit.env, &unit.prefix, &libraries).map_err(Error::Runner)?;
    runner.prepare_wine_prefix()?;

    info!("Obtaining a lock on wineprefix");
    let mut lock = LockFile::open(&runner.wine_prefix().join(".brie.lock")).map_err(Error::Lock)?;
    lock.lock_with_pid().map_err(Error::Lock)?;
    runner.winetricks(&unit.winetricks)?;
    runner.mounts(&unit.mounts)?;
    runner.install_libraries(&libraries)?;
    runner.before(&unit.before)?;
    runner.run("wineserver", &["--wait"]).map_err(Error::Wait)?;
    drop(lock);

    if !unit.command.is_empty() {
        let cd = unit.cd.as_ref().map(shellexpand::full).transpose()?;
        let cd = cd.as_deref().map_or_else(
            || Cow::Owned(runner.wine_prefix().join("drive_c")),
            |p| Cow::Borrowed(Path::new(p)),
        );

        info!("Running: {:?} in {}", unit.command, cd.display());
        let mut command = runner.command("wine", &unit.command);
        command.current_dir(cd);
        command.status().map_err(Error::Run)?;
    }

    info!("Waiting for wineserver to exit");
    runner.run("wineserver", &["--wait"]).map_err(Error::Wait)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::Path};

    use brie_cfg::{Library, ReleaseVersion, Runtime};
    use indicatif_log_bridge::LogWrapper;

    use crate::{Paths, Unit, MP};

    use super::launch;

    #[test]
    pub fn test_run() {
        let log = simple_logger::SimpleLogger::new();
        LogWrapper::new(MP.clone(), log).try_init().unwrap();

        launch(
            &Paths {
                libraries: Path::new(".tmp").join("libraries"),
                prefixes: Path::new(".tmp").join("prefixes"),
            },
            Unit {
                runtime: Runtime::GeProton {
                    version: ReleaseVersion::Latest,
                },
                libraries: [
                    (Library::DxvkGplAsync, ReleaseVersion::Latest),
                    (Library::DxvkNvapi, ReleaseVersion::Latest),
                    (Library::Vkd3dProton, ReleaseVersion::Latest),
                ]
                .into(),
                env: BTreeMap::default(),
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
