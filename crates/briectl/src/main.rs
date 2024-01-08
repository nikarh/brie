use std::{
    collections::HashMap,
    io,
    process::Command,
    sync::{mpsc, Arc},
};

use brie_cfg::Brie;
use brie_download::mp;
use clap::{Parser, Subcommand};
use log::{error, info};
use notify::{event::ModifyKind, Event, EventKind, RecursiveMode, Watcher};

mod assets;
mod desktop;
mod exe;
mod steam;
mod sunshine;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download assets from steamgriddb for units
    Assets,
    /// Config related commands
    Config {
        #[command(subcommand)]
        command: Config,
    },
    /// Generate .desktop files or .sh files or configuration in sunshine
    Generate {
        #[command(subcommand)]
        command: Generate,
    },
    /// Watch the configuration file for changes and download necessary assets and generate necessary files on change
    Watch,
}

#[derive(Subcommand)]
enum Generate {
    /// Update sunshine configuration with brie units
    Sunshine,
    /// Generate .desktop files
    Desktop,
    /// Add unit to steam as non-steam titles
    Steam,
    /// Update sunshine configuration and generate .desktop files
    All,
}

#[derive(Subcommand)]
enum Config {
    /// Open config file in the editor
    Edit,
}

fn main() {
    let log = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("briectl", log::LevelFilter::Trace);
    let max_level = log.max_level();
    let _ = indicatif_log_bridge::LogWrapper::new(mp().clone(), log).try_init();
    log::set_max_level(max_level);

    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Xdg error. {0}")]
    Xdg(#[from] xdg::BaseDirectoriesError),
    #[error("Config error. {0}")]
    Config(#[from] brie_cfg::Error),
    #[error("Asset error. {0}")]
    Assets(#[from] assets::Error),
    #[error("Unable to update sunshine config. {0}")]
    Sunshine(#[from] sunshine::Error),
    #[error("Unable to create .desktop files. {0}")]
    Desktop(#[from] desktop::Error),
    #[error("Unable to add units to steam. {0}")]
    Steam(#[from] steam::Error),
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Notify error. {0}")]
    Notify(#[from] notify::Error),
}

fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    let xdg = xdg::BaseDirectories::with_prefix("brie")?;
    let cache_dir = xdg.get_data_home();
    let config_file = xdg.get_config_file("brie.yaml");
    let exe = exe::path();

    match cli.command {
        Commands::Config {
            command: Config::Edit,
        } => {
            if let Some(path) = config_file.parent() {
                let _ = std::fs::create_dir_all(path);
            }
            let editor = std::env::var("EDITOR")
                .or(std::env::var("VISUAL"))
                .unwrap_or_else(|_| "vi".to_string());
            Command::new(editor).arg(&config_file).status()?;
        }
        Commands::Assets => {
            let config = brie_cfg::read(config_file)?;
            assets::download_all(&cache_dir, &config)?;
        }
        Commands::Generate { command } => {
            let config = brie_cfg::read(config_file)?;
            let images = assets::download_all(&cache_dir, &config)?;
            match command {
                Generate::Sunshine => {
                    info!("Generating sunshine configuration");
                    sunshine::update(&exe, &images, &config)?;
                }
                Generate::Desktop => {
                    info!("Generating .desktop files");
                    desktop::update(&exe, &images, &config)?;
                }
                Generate::Steam => {
                    info!("Adding units to steam");
                    steam::update(&exe, &images, &config)?;
                }
                Generate::All => {
                    update_all(&exe, &images, &config)?;
                }
            }
        }
        Commands::Watch => {
            info!(
                "Watching config file `{}` for changes",
                config_file.display()
            );

            let (sender, receiver) = mpsc::channel::<()>();

            let sender = Arc::new(sender);
            let on_event = || {
                let sender = sender.clone();
                move |res: notify::Result<Event>| {
                    match &res {
                        Ok(event) => match event.kind {
                            EventKind::Create(_)
                            | EventKind::Modify(ModifyKind::Data(_))
                            | EventKind::Remove(_) => {
                                log::debug!("Received event: {event:?}");
                                let _ = sender.send(());
                            }
                            _ => {}
                        },
                        Err(err) => {
                            error!("Event error: {err}");
                        }
                    };
                }
            };

            let process = |config: &Brie| {
                let images = assets::download_all(&cache_dir, config)?;
                update_all(&exe, &images, config)?;
                Ok::<_, Error>(())
            };

            let mut config = brie_cfg::read(config_file.clone())?;

            info!("Processing config before watch");
            if let Err(err) = process(&config) {
                error!("Error processing config: {err}");
            }

            info!("Starting watcher");
            let mut watcher = notify::recommended_watcher(on_event())?;
            watcher.watch(&config_file, RecursiveMode::NonRecursive)?;

            while let Ok(()) = receiver.recv() {
                // If a file is edited by deleting the original and creating a new one, without restarting the watcher
                // after deletion watcher will never receive new events.
                watcher = notify::recommended_watcher(on_event())?;
                watcher.watch(&config_file, RecursiveMode::NonRecursive)?;

                info!("Received event, processing config");

                let new_config = brie_cfg::read(config_file.clone())?;
                if new_config == config {
                    info!("Config did not change");
                    continue;
                }
                config = new_config;

                if let Err(err) = process(&config) {
                    error!("Error processing config: {err}");
                }
            }

            info!("Loop ended?");
        }
    };

    Ok(())
}

fn update_all(
    exe: &str,
    images: &HashMap<String, assets::Images>,
    config: &Brie,
) -> Result<(), Error> {
    info!("Generating sunshine configuration");
    sunshine::update(exe, images, config)?;
    info!("Generating .desktop files");
    desktop::update(exe, images, config)?;
    info!("Adding units to steam");
    steam::update(exe, images, config)?;

    Ok(())
}
