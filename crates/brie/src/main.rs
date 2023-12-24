use std::{collections::BTreeMap, env::args};

use brie_lib::{Paths, Unit, MP};

fn main() {
    let log = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("brie", log::LevelFilter::Trace)
        .env();

    indicatif_log_bridge::LogWrapper::new(MP.clone(), log)
        .try_init()
        .unwrap();

    if let Err(e) = launch() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct Units(Vec<String>);

impl Units {
    fn new(units: &BTreeMap<String, brie_cfg::Unit>) -> Self {
        Self(units.keys().cloned().collect())
    }
}

impl std::fmt::Display for Units {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for unit in &self.0 {
            writeln!(f, "  - {unit}")?;
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Xdg error. {0}")]
    Xdg(#[from] xdg::BaseDirectoriesError),
    #[error("Config error. {0}")]
    Config(#[from] brie_cfg::Error),
    #[error("Run error. {0}")]
    Brie(#[from] brie_lib::Error),
    #[error("Unit not provided as an argument. Available units:\n{0}")]
    NoUnitProvided(Units),
    #[error("Unit `{0}` not found. Available units:\n{1}")]
    NotFound(String, Units),
}

fn launch() -> Result<(), Error> {
    let xdg = xdg::BaseDirectories::with_prefix("brie")?;

    let config_home = xdg.get_config_home();
    let data_home = xdg.get_data_home();

    let mut cfg = brie_cfg::read(config_home.join("brie.yaml"))?;

    let mut args = args();
    let name = args
        .nth(1)
        .ok_or_else(|| Error::NoUnitProvided(Units::new(&cfg.units)))?;
    let mut unit = cfg
        .units
        .remove(&name)
        .ok_or_else(|| Error::NotFound(name.clone(), Units::new(&cfg.units)))?;

    unit.command.extend(args);

    let paths = Paths::new(&data_home);
    let cfg = Unit {
        runtime: unit.runtime,
        libraries: unit.libraries,
        env: unit.env,
        prefix: unit
            .prefix
            .unwrap_or_else(|| sanitize_directory_name(&unit.name.unwrap_or(name))),
        mounts: unit.mounts,
        before: unit.before,
        winetricks: unit.winetricks,
        cd: unit.cd,
        command: unit.command,
    };

    brie_lib::launch(&paths, cfg)?;

    Ok(())
}

fn sanitize_directory_name(dir_name: &str) -> String {
    static ILLEGAL: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    dir_name
        .chars()
        .filter(|&c| !ILLEGAL.contains(&c))
        .collect()
}
