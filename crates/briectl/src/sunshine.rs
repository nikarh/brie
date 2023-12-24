use std::{
    env::VarError,
    io,
    path::{Path, PathBuf},
};

use brie_cfg::Brie;
use log::info;
use serde::{Deserialize, Serialize};
use shellexpand::LookupError;

use crate::assets;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Assets error. {0}")]
    Assets(#[from] assets::Error),
    #[error("JSON error. {0}")]
    Serde(#[from] serde_json::Error),
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Path error. {0}")]
    Expand(#[from] LookupError<VarError>),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct App {
    pub name: String,
    pub output: String,
    pub cmd: String,
    pub image_path: Option<PathBuf>,

    #[serde(flatten)]
    pub rest: serde_json::Value,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    pub apps: Vec<App>,
    #[serde(flatten)]
    pub rest: serde_json::Value,
}

pub fn update(cache_dir: &Path, config: &Brie) -> Result<(), Error> {
    let Some(sunshine_path) = config.paths.sunshine.as_ref() else {
        info!("Sunshine path not provided, skipping sunshine generation");
        return Ok(());
    };

    let sunshine_path = shellexpand::full(sunshine_path)?;
    let sunshine_path = Path::new(sunshine_path.as_ref());

    if let Some(path) = sunshine_path.parent() {
        let _ = std::fs::create_dir_all(path);
    }

    info!("Downloading assets");
    let images = assets::download_all(cache_dir, config)?;

    info!("Loading sunshine config from {}", sunshine_path.display());
    let mut sunshine_config: Config = std::fs::read(sunshine_path)
        .ok()
        .and_then(|s| serde_json::from_slice(&s).ok())
        .unwrap_or_default();

    sunshine_config.apps.retain(|a| !a.cmd.starts_with("brie "));

    config
        .units
        .iter()
        .filter(|(_, unit)| unit.generate.sunshine)
        .map(|(k, unit)| App {
            name: unit.name.as_ref().unwrap_or(k).clone(),
            output: String::default(),
            cmd: format!("brie {k}"),
            image_path: images.get(k).and_then(|i| i.grid.clone()),
            rest: serde_json::Value::Object(serde_json::Map::default()),
        })
        .for_each(|app| sunshine_config.apps.push(app));

    let sunshine_apps = serde_json::to_string_pretty(&sunshine_config)?;

    info!("Saving sunshine config to {}", sunshine_path.display());
    std::fs::write(sunshine_path, sunshine_apps)?;

    Ok(())
}
