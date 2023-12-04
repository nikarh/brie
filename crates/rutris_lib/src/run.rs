use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};

use log::debug;
use path_absolutize::Absolutize;

pub struct CommandRunner {
    envs: HashMap<String, String>,
    prefix: PathBuf,
}

impl CommandRunner {
    pub fn new(
        wine: impl AsRef<Path>,
        mut envs: HashMap<String, String>,
        prefixes_path: impl AsRef<Path>,
        prefix: &str,
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
            .or(wine_path);

        if let Some(path) = path {
            envs.insert("PATH".to_owned(), path);
        }

        envs.insert(
            "WINEDLLOVERRIDES".to_owned(),
            "winemenubuilder.exe=".to_owned(),
        );

        let prefix = prefixes_path.as_ref().absolutize()?.join(prefix);

        let prefix_str = prefix.to_string_lossy();
        envs.insert("WINEPREFIX".to_owned(), prefix_str.to_string());

        Ok(Self { envs, prefix })
    }

    pub fn command(&self, command: &str, args: &[impl AsRef<OsStr>]) -> Command {
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

    pub fn run(&self, command: &str, args: &[impl AsRef<OsStr>]) -> Result<ExitStatus, io::Error> {
        self.command(command, args).status()
    }

    pub fn wine_prefix(&self) -> &Path {
        &self.prefix
    }
}
