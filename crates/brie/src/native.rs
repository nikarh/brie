use std::{
    borrow::Cow,
    env::VarError,
    io,
    path::Path,
    process::{Command, Stdio},
};

use brie_cfg::NativeUnit;
use log::debug;
use path_absolutize::Absolutize;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Invalid unit - empty command field.")]
    EmptyCommand,
    #[error("Unable to expand path. {0}")]
    Shellexpand(#[from] shellexpand::LookupError<VarError>),
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
}

pub fn launch(unit: NativeUnit) -> Result<(), Error> {
    let mut unit = unit.common;

    let cd = match unit.cd.as_ref() {
        Some(cd) => Some(shellexpand::full(cd)?),
        None => None,
    };

    match unit.command.first_mut() {
        Some(command) => *command = resolve(cd.as_deref(), command)?.to_string(),
        None => {
            return Err(Error::EmptyCommand);
        }
    }

    let mut args = unit.wrapper;
    args.extend(unit.command);

    let mut command = Command::new(&args[0]);
    if let Some(cd) = cd {
        command.current_dir(cd.as_ref());
    }

    command
        .args(&args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .envs(&unit.env);

    if let Some(cd) = unit.cd {
        command.current_dir(cd);
    }

    debug!("Running command: {:?}", args);
    command.status()?;

    Ok(())
}

fn resolve<'a>(cd: Option<&str>, command: &'a str) -> Result<Cow<'a, str>, Error> {
    let command = shellexpand::full(command)?;

    Ok(if let Some(cd) = cd {
        Cow::Owned(
            Path::new(&*command)
                .absolutize_from(Path::new(cd))?
                .to_string_lossy()
                .to_string(),
        )
    } else {
        command
    })
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    pub fn resolve_global() {
        let home = std::env::var("HOME").unwrap();

        assert_eq!(resolve(None, "ls").unwrap(), "ls");
        assert_eq!(resolve(None, "~/ls").unwrap(), format!("{home}/ls"));
        assert_eq!(resolve(None, "./ls").unwrap(), "./ls");
        assert_eq!(resolve(Some("/a"), "./ls").unwrap(), "/a/ls");
        assert_eq!(resolve(Some("/a/b"), "../ls").unwrap(), "/a/ls");
    }
}
