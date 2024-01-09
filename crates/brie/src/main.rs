use std::env::args;

use brie_wine::{mp, Paths, Unit};
use indexmap::IndexMap;

mod native;

fn main() {
    let log = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("brie", log::LevelFilter::Trace);
    let max_level = log.max_level();
    let _ = indicatif_log_bridge::LogWrapper::new(mp().clone(), log).try_init();
    log::set_max_level(max_level);

    if let Err(e) = launch() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct Units(Vec<String>);

impl Units {
    fn new(units: &IndexMap<String, brie_cfg::Unit>) -> Self {
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
    #[error("Unit not provided as an argument. Available units:\n{0}")]
    NoUnitProvided(Units),
    #[error("Unit `{0}` not found. Available units:\n{1}")]
    NotFound(String, Units),
    #[error("Wine unit error. {0}")]
    Wine(#[from] brie_wine::Error),
    #[error("Native unit error. {0}")]
    Native(#[from] native::Error),
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

    unit.common_mut().command.extend(args);

    match unit {
        brie_cfg::Unit::Native(unit) => {
            native::launch(unit)?;
        }
        brie_cfg::Unit::Wine(unit) => {
            let paths = Paths::new(&data_home);
            let cfg = Unit {
                runtime: unit.runtime,
                libraries: unit.libraries,
                env: unit.common.env,
                prefix: unit
                    .prefix
                    .unwrap_or_else(|| sanitize_directory_name(&unit.common.name.unwrap_or(name))),
                mounts: unit.mounts,
                before: unit.before,
                winetricks: unit.winetricks,
                cd: unit.common.cd,
                command: unit.common.command,
                wrapper: unit.common.wrapper,
            };

            brie_wine::launch(&paths, cfg)?;
        }
    };

    Ok(())
}

fn sanitize_directory_name(dir_name: &str) -> String {
    static ILLEGAL: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    dir_name
        .chars()
        .filter(|&c| !ILLEGAL.contains(&c))
        .collect()
}
