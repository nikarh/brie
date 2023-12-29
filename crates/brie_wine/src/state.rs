use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use brie_cfg::Library;
use log::info;
use serde::{Deserialize, Serialize};
use ureq::serde_json;

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    pub wine: Option<SystemTime>,
    pub libraries: HashMap<Library, SystemTime>,
}

fn path(library_path: &Path) -> PathBuf {
    library_path.join(".state")
}

pub fn read(library_path: &Path) -> State {
    info!("Reading state file");
    std::fs::read(path(library_path))
        .ok()
        .and_then(|s| serde_json::from_slice(&s).ok())
        .unwrap_or_default()
}

pub fn write(library_path: &Path, state: &State) -> std::io::Result<()> {
    info!("Saving state file");
    let state = serde_json::to_string_pretty(&state)?;
    std::fs::write(path(library_path), state)?;
    Ok(())
}
