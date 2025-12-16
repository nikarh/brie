use std::{
    collections::{HashMap, HashSet},
    env::VarError,
    io,
    path::{Path, PathBuf},
};

use brie_cfg::Brie;
use log::{debug, info};
use shellexpand::LookupError;
use steam_shortcuts_util::{
    calculate_app_id_for_shortcut, parse_shortcuts, shortcuts_to_bytes, Shortcut,
};

use crate::assets::{Assets, ImageKind, Images};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Steam shortcuts error. {0}")]
    Steam(String),
    #[error("Path error. {0}")]
    Expand(#[from] LookupError<VarError>),
}

pub fn update(exe: &str, assets: &Assets, config: &Brie) -> Result<(), Error> {
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
        .map(|(k, v)| (k, v.common()))
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
            info!("Reading shortcuts from {}", shortcuts_path.display());
            parse_shortcuts(s).map_err(Error::Steam)?
        }
        None => {
            info!("No shortcuts found, generating new ones");
            Vec::new()
        }
    };

    let existing_images = ls(&grid_path);

    // Remove shortcuts that are not in the config any more
    let (mut shortcuts, delete) = shortcuts.into_iter().partition::<Vec<_>, _>(|s| {
        units.contains_key(s.launch_options) || !s.tags.contains(&"brie")
    });

    // Remove images for deleted shortcuts
    for shortcut in delete {
        info!("Removing shortcut for `{}`", shortcut.launch_options);
        delete_images(&existing_images, shortcut.app_id);
    }

    let mut updated_keys = HashSet::new();
    let mut app_ids = HashMap::new();
    let mut icons = HashMap::new();

    // Icons will be copied to steam config folder so that steam has access to them if running from flatpak
    let icon_path = |app_id: u32| {
        grid_path
            .join(ImageKind::Icon.steam_file_name(app_id))
            .with_extension("png")
    };

    // Update shortcuts that are in the config
    let update_iter = shortcuts
        .iter_mut()
        .filter(|s| s.tags.contains(&"brie"))
        .filter_map(|s| units.get(s.launch_options).map(|u| (s, u)));

    for (shortcut, unit) in update_iter {
        let name = shortcut.launch_options;
        info!("Updating shortcut for `{name}`");
        updated_keys.insert(name);
        shortcut.exe = exe;
        shortcut.app_name = unit.name.as_deref().unwrap_or(name);
        shortcut.app_id = calculate_app_id_for_shortcut(shortcut);
        app_ids.insert(name, shortcut.app_id);
        icons.insert(shortcut.app_id, icon_path(shortcut.app_id));
    }

    // Insert missing units
    let insert_iter = units.iter().filter(|(&key, _)| !updated_keys.contains(key));

    for (key, unit) in insert_iter {
        info!("Adding shortcut for `{key}`");
        let name = unit.name.as_deref().unwrap_or(key);
        let mut shortcut = Shortcut::new("0", name, exe, "", "", "", key);

        shortcut.tags = vec!["brie"];
        app_ids.insert(key, shortcut.app_id);
        icons.insert(shortcut.app_id, icon_path(shortcut.app_id));
        shortcuts.push(shortcut);
    }

    // Copy all images into grid folder
    info!("Copying images");
    let _ = std::fs::create_dir_all(&grid_path);
    for (key, _) in units {
        let (Some(&app_id), images) = (app_ids.get(key), assets.get_all(key)) else {
            continue;
        };

        copy_images(&grid_path, app_id, images.as_ref())?;
    }

    // Update icons
    for shortcut in shortcuts.iter_mut().filter(|s| s.tags.contains(&"brie")) {
        let icon = icons.get(&shortcut.app_id);
        let Some(icon) = icon else { continue };
        let Some(icon) = icon.to_str() else { continue };
        shortcut.icon = icon;
    }

    let shortcuts = shortcuts_to_bytes(&shortcuts);
    std::fs::write(shortcuts_path, shortcuts).unwrap_or_default();

    Ok(())
}

impl ImageKind {
    fn steam_file_name(self, app_id: u32) -> String {
        match self {
            ImageKind::Grid => format!("{app_id}p"),
            ImageKind::Hero => format!("{app_id}_hero"),
            ImageKind::Logo => format!("{app_id}_logo"),
            ImageKind::Icon => format!("{app_id}_icon"),
        }
    }
}

fn ls(path: &Path) -> Vec<PathBuf> {
    path.read_dir()
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|r| r.path())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn copy_images(grid_path: &Path, app_id: u32, images: &Images) -> Result<(), Error> {
    for kind in ImageKind::all() {
        let name = kind.steam_file_name(app_id);
        let Some(image) = images.get(kind) else {
            continue;
        };

        let ext = image.extension().unwrap_or_default();
        let path = grid_path.join(name).with_extension(ext);
        debug!("Copying image {} to {}", image.display(), path.display());
        let _ = std::fs::copy(image, path)?;
    }

    Ok(())
}

fn delete_images(images: &[PathBuf], id: u32) {
    for image in images {
        let Some(name) = image.file_name() else {
            continue;
        };

        let name = name.to_string_lossy();
        if name.starts_with(&format!("{id}_")) || name.starts_with(&format!("{id}p")) {
            debug!("Removing image {}", image.display());
            let _ = std::fs::remove_file(image);
        }
    }
}
