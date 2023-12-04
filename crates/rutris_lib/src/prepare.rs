use std::{
    collections::{HashMap, HashSet},
    fs::{self},
    io::{self, Write},
    os::unix,
    path::PathBuf,
};

use log::{debug, info};
use thiserror::Error;

use crate::run::CommandRunner;

#[derive(Debug, Error)]
pub enum WinePrefixError {
    #[error("Wine error. {0}")]
    Wine(io::Error),
    #[error("Unable to read drive_c/users. {0}")]
    Read(io::Error),
    #[error("Unable to remove symlink. {0}")]
    Rm(io::Error),
    #[error("Unable to create directory. {0}")]
    Mkdir(io::Error),
}

#[derive(Debug, Error)]
pub enum WinetricksError {
    #[error("Unable to update lock file. {0}")]
    Lock(io::Error),
    #[error("Winetricks failed for `{0}`. {1}")]
    Winetricks(String, io::Error),
}

#[derive(Debug, Error)]
pub enum MountsError {
    #[error("Unable to read link at `{0}`. {1}")]
    Read(PathBuf, io::Error),
    #[error("Unable to remove file at `{0}`. {1}")]
    Rm(PathBuf, io::Error),
    #[error("Unable to create link at `{0}`. {1}")]
    Link(PathBuf, io::Error),
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct BeforeError(#[from] io::Error);

impl CommandRunner {
    pub fn prepare_wine_prefix(&self) -> Result<(), WinePrefixError> {
        let prefix = self.wine_prefix();
        if prefix.exists() {
            return Ok(());
        }

        info!("Creating wine prefix");

        if let Some(parent) = prefix.parent() {
            let _ = fs::create_dir_all(parent);
        }

        self.run("wine", &["__INIT_PREFIX"])
            .map_err(WinePrefixError::Wine)?;
        self.run("wineserver", &["--wait"])
            .map_err(WinePrefixError::Wine)?;

        info!("Replacing symlinks to $HOME with directories");

        let symlinks = fs::read_dir(prefix.join("drive_c").join("users"))
            .map_err(WinePrefixError::Read)?
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .filter_map(|p| fs::read_dir(p).ok())
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_symlink()).unwrap_or(false))
            .map(|e| e.path());

        for symlink in symlinks {
            fs::remove_file(&symlink).map_err(WinePrefixError::Rm)?;
            fs::create_dir(&symlink).map_err(WinePrefixError::Mkdir)?;
        }

        Ok(())
    }

    pub fn winetricks(&self, packages: &[impl AsRef<str>]) -> Result<(), WinetricksError> {
        info!("Checking winetricks");

        let file = self.wine_prefix().join(".winetricks");

        let installed = fs::read_to_string(&file).ok().unwrap_or_default();
        let installed = installed.lines().collect::<HashSet<_>>();

        let mut new = Vec::with_capacity(packages.len());

        for package in packages
            .iter()
            .map(AsRef::as_ref)
            .filter(|p| !installed.contains(p))
        {
            info!("Installing `{package}` with winetricks");
            self.run("winetricks", &["-q", package])
                .map_err(|e| WinetricksError::Winetricks(package.to_string(), e))?;
            new.push(package);
        }

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&file)
            .map_err(WinetricksError::Lock)?;

        for new in new {
            writeln!(file, "{new}").map_err(WinetricksError::Lock)?;
        }

        Ok(())
    }

    pub fn mounts(&self, mounts: &HashMap<char, String>) -> Result<(), MountsError> {
        info!("Checking drive mounts");
        // Iterate over mounts, check if there exists a symlink, if target is different, remove it,
        // if target is not a symlink, return error, then create a new link if necessary

        let dest = self.wine_prefix().join("dosdevices");

        for (drive, new_target) in mounts {
            let symlink = dest.join(format!("{drive}:"));

            if symlink.exists() {
                let current_target = symlink
                    .read_link()
                    .map_err(|e| MountsError::Read(symlink.clone(), e))?;
                if &current_target.to_string_lossy() == new_target {
                    continue;
                }

                debug!(
                    "Removing old symlink from `{drive}:` to `{}`",
                    symlink.display()
                );

                fs::remove_file(&symlink).map_err(|e| MountsError::Rm(symlink.clone(), e))?;
            }

            info!("Mounting `{drive}:` to `{new_target}`");
            unix::fs::symlink(new_target, &symlink).map_err(|e| MountsError::Link(symlink, e))?;
        }

        Ok(())
    }

    pub fn before(&self, commands: &[Vec<String>]) -> Result<(), BeforeError> {
        for line in commands {
            if line.is_empty() {
                continue;
            }

            info!("Running before-script: {line:?}");
            self.run(&line[0], &line[1..])?;
        }

        Ok(())
    }
}
