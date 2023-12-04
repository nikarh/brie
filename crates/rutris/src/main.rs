use std::{env::args, io, path::PathBuf};

use rutris_lib::{Paths, Unit, MP};

use crate::config::Rutris;

mod config;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Xdg error. {0}")]
    Xdg(#[from] xdg::BaseDirectoriesError),
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Yaml error. {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Config file does not exist at `{0}`")]
    NoConfig(PathBuf),
    #[error("Unit not provided")]
    NoUnitProvided,
    #[error("Unit not found")]
    NoUnitFound,
    #[error("Run error. {0}")]
    Rutris(#[from] rutris_lib::Error),
}

fn main() -> Result<(), Error> {
    let mut builder = pretty_env_logger::formatted_builder();
    builder.filter_level(log::LevelFilter::Debug);
    if let Ok(s) = ::std::env::var("RUST_LOG") {
        builder.parse_filters(&s);
    }

    indicatif_log_bridge::LogWrapper::new(MP.clone(), builder.build())
        .try_init()
        .unwrap();

    // Multiple modes:

    // 1. Run game
    // 2. Generate stuff? For sunshine, desktop files, shell scripts? May be that's a separate binary?
    // 3. A watcher that would auto-generate stuff on file change? May be that's a also separate binary with it's own config?
    // 4. rutrisctl? to check latest deps and update them? and to manage games yaml?

    let xdg = xdg::BaseDirectories::with_prefix("rutris")?;

    let config_home = xdg.get_config_home();
    let data_home = xdg.get_data_home();

    let cfg = config_home.join("rutris.yaml");
    if !cfg.exists() {
        return Err(Error::NoConfig(cfg));
    }

    let cfg = std::fs::read(&cfg)?;
    let mut cfg: serde_yaml::Value = serde_yaml::from_slice(&cfg)?;
    cfg.apply_merge()?;
    let mut cfg: Rutris = serde_yaml::from_value(cfg)?;

    let unit = args().nth(1).ok_or(Error::NoUnitProvided)?;
    let unit = cfg.units.remove(&unit).ok_or(Error::NoUnitFound)?;

    let paths = Paths::new(&data_home);
    let cfg = Unit {
        runtime: unit.runtime.into(),
        libraries: unit
            .libraries
            .into_iter()
            .map(|(k, v)| (k, v.into()))
            .collect(),
        env: unit.env.into_iter().collect(),
        prefix: unit.prefix.unwrap_or_else(|| unit.name.clone()),
        mounts: unit.mounts.into_iter().collect(),
        before: unit.before,
        winetricks: unit.winetricks,
        cd: unit.cd,
        command: unit.command,
    };

    rutris_lib::run(&paths, cfg)?;

    Ok(())
}
