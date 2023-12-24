use std::{io, process::Command};

use brie_download::MP;
use clap::{Parser, Subcommand};
use log::info;

mod assets;
mod desktop;
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
}

#[derive(Subcommand)]
enum Config {
    /// Open config file in the editor
    Edit,
}

fn main() {
    let log = simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("brie", log::LevelFilter::Trace)
        .env();

    indicatif_log_bridge::LogWrapper::new(MP.clone(), log)
        .try_init()
        .unwrap();

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
    #[error("Sunshine config error. {0}")]
    Sunshine(#[from] sunshine::Error),
    #[error("Desktop file error. {0}")]
    Desktop(#[from] desktop::Error),
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
}

fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    let xdg = xdg::BaseDirectories::with_prefix("brie")?;
    let cache_dir = xdg.get_cache_home();
    let config_file = xdg.get_config_file("brie.yaml");

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
        Commands::Generate { command } => match command {
            Generate::Sunshine => {
                info!("Generating sunshine configuration");
                let config = brie_cfg::read(config_file)?;
                sunshine::update(&cache_dir, &config)?;
            }
            Generate::Desktop => {
                info!("Generating .desktop files");
                let config = brie_cfg::read(config_file)?;
                desktop::update(&cache_dir, &config)?;
            }
        },
        Commands::Watch => todo!(),
    };

    Ok(())
}
