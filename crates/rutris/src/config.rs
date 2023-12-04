use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use rutris_lib::Library;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Rutris {
    pub units: HashMap<String, Unit>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Unit {
    pub name: String,
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

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseVersion(String);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Generate {
    #[serde(default)]
    pub sunshine: bool,
    #[serde(default)]
    pub desktop: bool,
    #[serde(default)]
    pub shell: bool,
}

impl From<Runtime> for rutris_lib::Runtime {
    fn from(value: Runtime) -> Self {
        match value {
            Runtime::System { path } => rutris_lib::Runtime::System(path),
            Runtime::GeProton { version } => rutris_lib::Runtime::GeProton(version.into()),
        }
    }
}

impl From<ReleaseVersion> for rutris_lib::ReleaseVersion {
    fn from(value: ReleaseVersion) -> Self {
        match value.0.as_str() {
            "*" | "latest" => Self::Latest,
            _ => Self::Tag(value.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::config::Rutris;

    #[test]
    fn serialize() {
        let cfg = include_str!("../tests/test.yaml");
        let mut cfg: serde_yaml::Value = serde_yaml::from_str(cfg).unwrap();
        cfg.apply_merge().unwrap();
        let cfg: Rutris = serde_yaml::from_value(cfg).unwrap();

        assert_eq!(&format!("{cfg:#?}"), include_str!("../tests/test.ron"));
    }
}
