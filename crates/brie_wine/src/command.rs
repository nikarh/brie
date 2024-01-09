use std::{
    env,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

use brie_cfg::Library;
use indexmap::IndexMap;
use log::debug;
use path_absolutize::Absolutize;

use crate::{dll::mut_env, Paths};

pub struct Runner {
    envs: IndexMap<String, String>,
    prefix: PathBuf,
}

impl Runner {
    pub fn new(
        paths: &Paths,
        wine: impl AsRef<Path>,
        mut envs: IndexMap<String, String>,
        prefix: &str,
        libraries: &IndexMap<Library, PathBuf>,
    ) -> Result<Self, io::Error> {
        let wine = wine.as_ref();

        let wine_path = wine
            .absolutize()?
            .parent()
            .and_then(|p| p.to_str())
            .map(ToString::to_string);

        let path = env::var_os("PATH")
            .and_then(|p| p.into_string().ok())
            .and_then(|rest| wine_path.as_ref().map(|p| format!("{p}:{rest}")))
            .or(wine_path)
            .map(|p| format!("{p}:{bin}", bin = paths.libraries.join(".bin").display()));

        if let Some(path) = path {
            envs.insert("PATH".to_owned(), path);
        }

        let dll_overrides = envs.entry("WINEDLLOVERRIDES".to_owned()).or_default();
        dll_overrides.push_str(if dll_overrides.is_empty() { "" } else { ";" });
        dll_overrides.push_str("winemenubuilder.exe=");

        for (&library, path) in libraries {
            mut_env(library, path, &mut envs);
        }

        let prefix = paths.prefixes.absolutize()?.join(prefix);

        let prefix_str = prefix.to_string_lossy();
        envs.insert("WINEPREFIX".to_owned(), prefix_str.to_string());

        Ok(Self { envs, prefix })
    }

    pub fn command(&self, command: impl AsRef<OsStr>, args: &[impl AsRef<OsStr>]) -> Command {
        let mut command = Command::new(command);

        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .envs(&self.envs);

        debug!("Running command: {:?}", command);

        command
    }

    pub fn run(
        &self,
        command: impl AsRef<OsStr>,
        args: &[impl AsRef<OsStr>],
    ) -> Result<ExitStatus, io::Error> {
        self.command(command, args).status()
    }

    pub fn wine_prefix(&self) -> &Path {
        &self.prefix
    }
}
