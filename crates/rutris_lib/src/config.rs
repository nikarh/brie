use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug)]
pub struct Unit {
    pub runtime: Runtime,
    pub libraries: HashMap<Library, ReleaseVersion>,

    pub env: HashMap<String, String>,
    pub prefix: String,

    pub mounts: HashMap<char, String>,
    pub before: Vec<Vec<String>>,
    pub winetricks: Vec<String>,

    pub cd: Option<String>,
    pub command: Vec<String>,
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Library {
    Dxvk,
    DxvkGplAsync,
    DxvkNvapi,
    Vkd3dProton,
}

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
    pub(crate) fn as_path(&self) -> &str {
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
    #[must_use]
    pub fn new(data_home: &Path) -> Self {
        Self {
            libraries: data_home.join("libraries"),
            prefixes: data_home.join("prefixes"),
        }
    }
}
