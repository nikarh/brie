use std::{collections::HashMap, path::PathBuf};

use strum::Display;

#[derive(Debug)]
pub enum Runtime {
    System(Option<PathBuf>),
    GeProton(ReleaseVersion),
}

#[derive(Debug, Display, PartialEq, Eq)]
pub enum ReleaseVersion {
    Latest,
    Tag(String),
}

impl ReleaseVersion {
    #[must_use]
    pub fn as_path(&self) -> &str {
        match self {
            ReleaseVersion::Latest => "latest",
            ReleaseVersion::Tag(tag) => tag,
        }
    }
}

#[derive(Debug)]
pub struct Paths {
    pub libraries: PathBuf,
    pub prefixes: PathBuf,
}

impl Paths {
    pub fn xdg() -> Result<Self, xdg::BaseDirectoriesError> {
        let xdg = xdg::BaseDirectories::with_profile("rutris", "wine")?;
        let data_home = xdg.get_data_home();

        Ok(Self {
            libraries: data_home.join("libraries"),
            prefixes: data_home.join("prefixes"),
        })
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Library {
    Dxvk,
    DxvkGplAsync,
    DxvkNvapi,
    // NvidiaLibs,
    Vkd3dProton,
}

#[derive(Debug)]
pub struct Config {
    pub runtime: Runtime,
    pub libraries: HashMap<Library, ReleaseVersion>,

    pub env: HashMap<String, String>,
    pub prefix: String,
    pub cd: String,
    pub command: Vec<String>,

    pub mounts: HashMap<char, String>,
    pub before: Vec<Vec<String>>,
    pub winetricks: Vec<String>,
}
