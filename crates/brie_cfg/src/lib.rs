use std::{collections::BTreeMap, io, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Brie {
    pub steamgriddb_token: Option<String>,
    #[serde(default)]
    pub paths: Paths,
    pub units: BTreeMap<String, Unit>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Paths {
    pub sunshine: Option<String>,
    pub desktop: Option<String>,
    pub shell: Option<String>,
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Library {
    Dxvk,
    DxvkGplAsync,
    DxvkNvapi,
    NvidiaLibs,
    Vkd3dProton,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseVersion {
    #[serde(alias = "*")]
    Latest,
    #[serde(untagged)]
    Tag(String),
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Unit {
    pub name: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub steamgriddb_id: Option<u32>,
    #[serde(default)]
    pub cd: Option<String>,
    pub command: Vec<String>,
    #[serde(default)]
    pub winetricks: Vec<String>,
    #[serde(default)]
    pub mounts: BTreeMap<char, String>,
    #[serde(default)]
    pub before: Vec<Vec<String>>,
    #[serde(default)]
    pub generate: Generate,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub wrapper: Vec<String>,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub libraries: BTreeMap<Library, ReleaseVersion>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Runtime {
    System { path: Option<PathBuf> },
    GeProton { version: ReleaseVersion },
}

impl Default for Runtime {
    fn default() -> Self {
        Self::System { path: None }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Generate {
    #[serde(default)]
    pub sunshine: bool,
    #[serde(default)]
    pub desktop: bool,
    #[serde(default)]
    pub shell: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Yaml error. {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("Config file does not exist at `{0}`")]
    NoConfig(PathBuf),
}

pub fn read(path: PathBuf) -> Result<Brie, Error> {
    if !path.exists() {
        return Err(Error::NoConfig(path));
    }

    let cfg = std::fs::read(&path)?;
    let mut cfg: serde_yaml::Value = serde_yaml::from_slice(&cfg)?;
    cfg.apply_merge()?;
    let cfg: Brie = serde_yaml::from_value(cfg)?;

    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::Brie;

    #[test]
    fn serialize() {
        let cfg = include_str!("../tests/test.yaml");
        let mut cfg: serde_yaml::Value = serde_yaml::from_str(cfg).unwrap();
        cfg.apply_merge().unwrap();
        let cfg: Brie = serde_yaml::from_value(cfg).unwrap();

        assert_eq!(
            &format!("{cfg:#?}"),
            include_str!("../tests/test.ron").trim_end()
        );
    }
}
