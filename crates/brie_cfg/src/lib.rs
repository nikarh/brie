use std::{io, path::PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_with::{formats::PreferOne, serde_as, OneOrMany};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Brie {
    pub tokens: Option<Tokens>,

    #[serde(default)]
    pub paths: Paths,
    pub units: IndexMap<String, Unit>,
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tokens {
    pub steamgriddb: Option<String>,
    pub github: Option<String>,
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Paths {
    pub steam_config: Option<String>,
    pub sunshine: Option<String>,
    pub desktop: Option<String>,
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

impl ReleaseVersion {
    #[must_use]
    pub fn to_str(&self) -> &str {
        match self {
            Self::Latest => "latest",
            Self::Tag(tag) => tag,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind")]
pub enum Unit {
    #[serde(rename = "native")]
    Native(NativeUnit),
    #[serde(untagged)]
    Wine(WineUnit),
}

impl Unit {
    #[must_use]
    pub fn common(&self) -> &UnitCommon {
        match self {
            Self::Native(unit) => &unit.common,
            Self::Wine(unit) => &unit.common,
        }
    }

    #[must_use]
    pub fn common_mut(&mut self) -> &mut UnitCommon {
        match self {
            Self::Native(unit) => &mut unit.common,
            Self::Wine(unit) => &mut unit.common,
        }
    }
}

#[serde_as]
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnitCommon {
    pub name: Option<String>,
    pub steamgriddb_id: Option<u32>,
    pub cd: Option<String>,
    #[serde_as(deserialize_as = "OneOrMany<_, PreferOne>")]
    pub command: Vec<String>,
    #[serde(default)]
    pub env: IndexMap<String, String>,
    #[serde(default)]
    pub generate: Generate,
    #[serde(default)]
    #[serde_as(deserialize_as = "OneOrMany<_, PreferOne>")]
    pub wrapper: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WineUnit {
    #[serde(flatten)]
    pub common: UnitCommon,

    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub winetricks: Vec<String>,
    #[serde(default)]
    pub mounts: IndexMap<char, String>,
    #[serde(default)]
    pub before: Vec<Vec<String>>,
    #[serde(default)]
    pub runtime: Runtime,
    #[serde(default)]
    pub libraries: IndexMap<Library, ReleaseVersion>,
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeUnit {
    #[serde(flatten)]
    pub common: UnitCommon,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Runtime {
    System { path: Option<PathBuf> },
    GeProton { version: ReleaseVersion },
    Tkg { version: ReleaseVersion },
}

impl Default for Runtime {
    fn default() -> Self {
        Self::System { path: None }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Generate {
    #[serde(default)]
    pub sunshine: bool,
    #[serde(default)]
    pub desktop: bool,
    #[serde(default)]
    pub steam_shortcut: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error. {0}")]
    Io(#[from] io::Error),
    #[error("Yaml error. {0}")]
    Yaml(#[from] serde_saphyr::Error),
    #[error("Config file does not exist at `{0}`")]
    NoConfig(PathBuf),
}

pub fn read(path: PathBuf) -> Result<Brie, Error> {
    if !path.exists() {
        return Err(Error::NoConfig(path));
    }

    let cfg = std::fs::read_to_string(&path)?;

    Ok(serde_saphyr::from_str(&cfg)?)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::Brie;

    #[test]
    fn serialize() {
        let cfg = include_str!("../tests/test.yaml");
        let cfg: Brie = serde_saphyr::from_str(cfg).unwrap();

        assert_eq!(
            &format!("{cfg:#?}"),
            include_str!("../tests/test.ron").trim_end()
        );
    }

    #[test]
    fn nested_merge() {
        let cfg = r"
x-base: &base
  env:
    A: base
  wrapper: [gamemoderun]
x-derived: &derived
  <<: *base
  name: Derived
units:
  game:
    <<: *derived
    command: [game.exe]
";
        let cfg: Brie = serde_saphyr::from_str(cfg).unwrap();
        let unit = cfg.units["game"].common();

        assert_eq!(unit.name.as_deref(), Some("Derived"));
        assert_eq!(unit.env["A"], "base");
        assert_eq!(unit.wrapper, ["gamemoderun"]);
        assert_eq!(unit.command, ["game.exe"]);
    }
}
