use std::{env::VarError, io, path::Path};

use brie_cfg::Brie;
use log::{debug, info};
use shellexpand::LookupError;

use crate::assets;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Assets error. {0}")]
    Assets(#[from] assets::Error),
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Path error. {0}")]
    Expand(#[from] LookupError<VarError>),
}

pub fn update(cache_dir: &Path, config: &Brie) -> Result<(), Error> {
    let Some(desktop_path) = config.paths.desktop.as_ref() else {
        info!("Desktop file path not provided, skipping generation");
        return Ok(());
    };

    let desktop_path = shellexpand::full(desktop_path)?;
    let desktop_path = Path::new(desktop_path.as_ref());
    let _ = std::fs::create_dir_all(desktop_path);

    // Remove existing files
    desktop_path.read_dir()?.for_each(|entry| {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file()
            && path.starts_with("brie-")
            && path.extension().is_some_and(|e| e == "desktop")
        {
            debug!("Removing {}", path.display());
            let _ = std::fs::remove_file(&path);
        }
    });

    // Download assets
    let images = assets::download_all(cache_dir, config)?;

    // Recreate files for all units
    for (key, unit) in config.units.iter().filter(|(_, u)| u.generate.desktop) {
        let path = desktop_path.join(format!("brie-{key}.desktop"));

        let icon = images
            .get(key)
            .and_then(|p| p.icon.as_deref())
            .unwrap_or_else(|| Path::new(""));

        let name = unit.name.as_ref().unwrap_or(key);
        let desktop = format!(
            "[Desktop Entry]\n\
            Type=Application\n\
            Version=1.0\n\
            Name={name}\n\
            Exec=brie {key}\n\
            Icon={icon}\n\
            Terminal=false\n\
            Categories=Games;\n",
            icon = icon.display()
        );

        info!("Writing desktop file for {key} to {}", path.display());
        std::fs::write(&path, desktop)?;
    }

    Ok(())
}
