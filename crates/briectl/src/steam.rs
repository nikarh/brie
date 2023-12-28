use std::{
    collections::{HashMap, HashSet},
    env::VarError,
    io,
    path::Path,
};

use brie_cfg::Brie;
use log::{debug, info};
use shellexpand::LookupError;
use steam_shortcuts_util::{parse_shortcuts, shortcuts_to_bytes, Shortcut};

use crate::assets::{ImageKind, Images};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Steam shortcuts error. {0}")]
    Steam(String),
    #[error("Path error. {0}")]
    Expand(#[from] LookupError<VarError>),
}

pub fn update(images: &HashMap<String, Images>, config: &Brie) -> Result<(), Error> {
    let Some(steam_config) = config.paths.steam_config.as_ref() else {
        info!("Steam config path not provided, skipping shortcut generation");
        return Ok(());
    };

    let steam_config = shellexpand::full(steam_config)?;
    let steam_config = Path::new(steam_config.as_ref());

    let shortcuts_path = steam_config.join("shortcuts.vdf");
    let grid_path = steam_config.join("grid");

    let units = config
        .units
        .iter()
        .filter(|(_, unit)| unit.generate.steam_shortcut)
        .map(|(k, u)| (k.as_str(), u))
        .collect::<HashMap<_, _>>();

    if units.is_empty() {
        info!("No units to generate shortcuts for, skipping");
        return Ok(());
    }

    let shortcuts = std::fs::read(&shortcuts_path).ok();
    let shortcuts = match shortcuts.as_ref() {
        Some(s) => {
            info!("Reading shortcuts from {shortcuts_path:?}");
            parse_shortcuts(s).map_err(Error::Steam)?
        }
        None => {
            info!("No shortcuts found, generating new ones");
            Vec::new()
        }
    };

    let existing_images = grid_path
        .read_dir()
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|r| r.path())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Remove shortcuts that are not in the config any more
    let (mut shortcuts, delete) = shortcuts.into_iter().partition::<Vec<_>, _>(|s| {
        units.contains_key(s.launch_options) || !s.tags.iter().any(|&t| t == "brie")
    });

    for shortcut in delete {
        info!("Removing shortcut for `{}`", shortcut.launch_options);

        for path in &existing_images {
            let Some(name) = path.file_name() else {
                continue;
            };

            let name = name.to_string_lossy();
            let id = shortcut.app_id;
            if name.starts_with(&format!("{id}_")) || name.starts_with(&format!("{id}p")) {
                debug!("Removing image {path:?}");
                let _ = std::fs::remove_file(path);
            }
        }
    }

    let mut updated_keys = HashSet::new();
    let mut app_ids = HashMap::new();

    // Update shortcuts that are in the config
    let update_iter = shortcuts
        .iter_mut()
        .filter(|s| s.tags.iter().any(|&t| t == "brie"))
        .filter_map(|s| units.get(s.launch_options).map(|u| (s, u)));

    for (shortcut, unit) in update_iter {
        info!("Updating shortcut for `{}`", shortcut.launch_options);
        updated_keys.insert(shortcut.launch_options);
        shortcut.app_name = unit.name.as_deref().unwrap_or(shortcut.launch_options);
        app_ids.insert(shortcut.launch_options, shortcut.app_id);
    }

    // Insert missing units
    let insert_iter = units.iter().filter(|(&key, _)| !updated_keys.contains(key));

    for (key, unit) in insert_iter {
        info!("Adding shortcut for `{key}`");
        let name = unit.name.as_deref().unwrap_or(key);
        let mut shortcut = Shortcut::new("0", name, "brie", "", "", "", key);
        shortcut.tags = vec!["brie"];
        app_ids.insert(key, shortcut.app_id);
        shortcuts.push(shortcut);
    }

    // Copy all images into grid folder
    info!("Copying images");
    let _ = std::fs::create_dir_all(&grid_path);
    for (key, _) in units {
        let (Some(&app_id), Some(images)) = (app_ids.get(key), images.get(key)) else {
            continue;
        };

        if let Some(image) = images.get(ImageKind::Grid) {
            let name = format!("{app_id}p.png");
            let path = grid_path.join(&name);
            debug!("Copying image {image:?} to {path:?}");
            let _ = std::fs::copy(image, path)?;
        }

        if let Some(image) = images.get(ImageKind::Hero) {
            let name = format!("{app_id}_hero.png");
            let path = grid_path.join(&name);
            debug!("Copying image {image:?} to {path:?}");
            let _ = std::fs::copy(image, path)?;
        }
    }

    let shortcuts = shortcuts_to_bytes(&shortcuts);
    std::fs::write(shortcuts_path, shortcuts).unwrap_or_default();

    Ok(())
}
