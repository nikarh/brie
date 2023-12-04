use std::{
    collections::{HashMap, HashSet},
    fs::{self},
    io::{self, Write},
    os::unix,
};

use log::{debug, info};

use crate::run::CommandRunner;

impl CommandRunner {
    pub fn prepare_wine_prefix(&self) -> Result<(), io::Error> {
        let prefix = self.wine_prefix();
        if prefix.exists() {
            return Ok(());
        }

        info!("Creating wine prefix");

        if let Some(parent) = prefix.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let _ = self.run("wine", &["__INIT_PREFIX"]);
        let _ = self.run("wineserver", &["--wait"]);

        info!("Replacing symlinks to $HOME with directories");

        let symlinks = fs::read_dir(prefix.join("drive_c").join("users"))?
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .filter_map(|p| fs::read_dir(p).ok())
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_symlink()).unwrap_or(false))
            .map(|e| e.path());

        for symlink in symlinks {
            fs::remove_file(&symlink)?;
            fs::create_dir(&symlink)?;
        }

        Ok(())
    }

    pub fn winetricks(&self, packages: &[impl AsRef<str>]) -> Result<(), io::Error> {
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
            let _ = self.run("winetricks", &["-q", package]);
            new.push(package);
        }

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&file)?;

        for new in new {
            writeln!(file, "{new}")?;
        }

        Ok(())
    }

    pub fn mounts(&self, mounts: &HashMap<char, String>) -> Result<(), io::Error> {
        info!("Checking drive mounts");
        // Iterate over mounts, check if there exists a symlink, if target is different, remove it,
        // if target is not a symlink, return error, then create a new link if necessary

        let dest = self.wine_prefix().join("dosdevices");

        for (drive, target) in mounts {
            let symlink = dest.join(format!("{drive}:"));

            if symlink.exists() {
                let symlink = symlink.read_link()?;
                let symlink = symlink.to_string_lossy();

                if &symlink == target {
                    continue;
                }

                debug!("Removing old symlink from `{drive}:` to `{symlink}`");
                fs::remove_file(symlink.as_ref())?;
            }

            info!("Mounting `{drive}:` to `{target}`");
            unix::fs::symlink(target, symlink)?;
        }

        Ok(())
    }

    pub fn before(&self, commands: &[Vec<String>]) -> Result<(), io::Error> {
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
