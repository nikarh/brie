use std::{
    fs::{self, File, Permissions},
    io::{self},
    os::unix::{self, fs::PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use brie_cfg::{Library, ReleaseVersion};
use brie_download::download_file;
use flate2::read::GzDecoder;
use log::{debug, error, info};
use tar::Archive;
use thiserror::Error;
use xz2::read::XzDecoder;
use zstd::stream::Decoder as ZstDecoder;

use crate::downloader::{
    self,
    github::{with_suffix, Github},
    gitlab::{filename_version, Gitlab},
    GitRepo, ReleaseProvider,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Download error. {0}")]
    Download(#[from] brie_download::Error),
    #[error("Release check error. {0}")]
    Release(#[from] downloader::Error),
    #[error("Download error. {0}")]
    Http(#[from] Box<ureq::Error>),
    #[error("IO error. {0}")]
    Io(#[from] std::io::Error),
    #[error("Unknown library archive format for file {0}.")]
    UnknownFormat(String),
}

pub trait Downloadable {
    fn name(&self) -> &'static str;

    fn substring(&self) -> &'static str {
        self.name()
    }

    fn get_release(
        &self,
        version: &ReleaseVersion,
    ) -> Result<downloader::Release, downloader::Error>;
}

pub struct WineGe;

impl Downloadable for WineGe {
    fn name(&self) -> &'static str {
        "wine-ge-custom"
    }

    fn substring(&self) -> &'static str {
        "GE-Proton"
    }

    fn get_release(
        &self,
        version: &ReleaseVersion,
    ) -> Result<downloader::Release, downloader::Error> {
        Github::new(with_suffix(".tar.xz"))
            .get_release(&GitRepo::new("GloriousEggroll", "wine-ge-custom"), version)
    }
}

impl Downloadable for Library {
    fn name(&self) -> &'static str {
        match self {
            Library::Dxvk => "dxvk",
            Library::DxvkGplAsync => "dxvk-gplasync",
            Library::DxvkNvapi => "dxvk-nvapi",
            Library::NvidiaLibs => "nvidia-libs",
            Library::Vkd3dProton => "vkd3d-proton",
        }
    }

    fn get_release(
        &self,
        version: &ReleaseVersion,
    ) -> Result<downloader::Release, downloader::Error> {
        match self {
            Library::Dxvk => {
                Github::new(|a| a.name.ends_with(".tar.gz") && !a.name.contains("sniper"))
                    .get_release(&GitRepo::new("doitsujin", "dxvk"), version)
            }
            Library::DxvkGplAsync => {
                Gitlab::new("releases", filename_version("dxvk-gplasync-", ".tar.gz"))
                    .get_release(&GitRepo::new("Ph42oN", "dxvk-gplasync"), version)
            }
            Library::DxvkNvapi => Github::new(with_suffix(".tar.gz"))
                .get_release(&GitRepo::new("jp7677", "dxvk-nvapi"), version),
            Library::Vkd3dProton => Github::new(with_suffix(".tar.zst"))
                .get_release(&GitRepo::new("HansKristian-Work", "vkd3d-proton"), version),
            Library::NvidiaLibs => Github::new(with_suffix(".tar.xz"))
                .get_release(&GitRepo::new("SveSop", "nvidia-libs"), version),
        }
    }
}

fn untar(tar: impl io::Read, destination: impl AsRef<Path>) -> Result<(), io::Error> {
    let destination = destination.as_ref();

    let mut archive = Archive::new(tar);
    archive.unpack(destination)?;

    Ok(())
}

fn contains_single_directory_with_substring(
    path: &Path,
    substring: &str,
) -> Result<Option<PathBuf>, io::Error> {
    let mut entries = fs::read_dir(path)?;

    let entry = entries
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Directory is empty"))??;
    let entry_path = entry.path();

    if !entry_path.is_dir() {
        return Ok(None);
    }

    let file_name = entry.file_name();
    if file_name.to_string_lossy().contains(substring) && entries.next().is_none() {
        Ok(Some(entry_path))
    } else {
        Ok(None)
    }
}

fn move_paths_to_parent_directory(target_path: &Path) -> Result<(), std::io::Error> {
    let parent = target_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Directory has no parent"))?;

    let temp_path = parent.join(uuid::Uuid::new_v4().to_string());

    fs::rename(target_path, &temp_path)?;
    let _guard = DirGuard::new(&temp_path);

    for entry in fs::read_dir(&temp_path)? {
        let entry = entry?;
        let entry_path = entry.path();

        let new_path = parent.join(entry.file_name());
        fs::rename(&entry_path, &new_path)?;
    }

    Ok(())
}

struct DirGuard<'a> {
    path: &'a Path,
    success: bool,
}

impl<'a> DirGuard<'a> {
    fn new(path: &'a Path) -> Self {
        let success = false;
        Self { path, success }
    }
}

impl<'a> Drop for DirGuard<'a> {
    fn drop(&mut self) {
        if !self.success {
            info!("Removing {path}", path = self.path.display());
            let _ = fs::remove_dir_all(self.path);
        }
    }
}

fn download_library(
    release: downloader::Release,
    library: &impl Downloadable,
    library_dir: &Path,
    version_dir: PathBuf,
    version: &ReleaseVersion,
) -> Result<PathBuf, Error> {
    let name = library.name();

    info!("Downloading library {name} {version:?}: {release:?}");
    let dest = library_dir.join(&release.version);

    fs::create_dir_all(&dest)?;

    // Auto-delete directory if extraction fails mid-way
    let mut guard = DirGuard::new(&dest);

    let (lib, pb) = download_file(&release.url)?.progress(name);

    match &release.filename {
        n if n.ends_with(".tar.gz") => untar(GzDecoder::new(lib), &dest)?,
        n if n.ends_with(".tar.xz") => untar(XzDecoder::new(lib), &dest)?,
        n if n.ends_with(".tar.zst") => untar(ZstDecoder::new(lib)?, &dest)?,
        _ => {
            return Err(Error::UnknownFormat(release.filename));
        }
    }

    pb.finish();

    if let Some(dest) = contains_single_directory_with_substring(&dest, library.substring())? {
        move_paths_to_parent_directory(&dest)?;
    }

    if version == &ReleaseVersion::Latest {
        _ = fs::remove_file(&version_dir);
        unix::fs::symlink(&release.version, &version_dir)?;
    }

    guard.success = true;
    Ok(version_dir)
}

pub struct State {
    pub path: PathBuf,
    pub updated: bool,
}

impl State {
    pub fn new(path: PathBuf, updated: bool) -> Self {
        Self { path, updated }
    }

    pub fn untouched(path: PathBuf) -> Self {
        Self::new(path, false)
    }
}

pub fn ensure_library_exists(
    library: &impl Downloadable,
    library_dir: impl AsRef<Path>,
    version: &ReleaseVersion,
    time_since_update: Option<Duration>,
) -> Result<State, Error> {
    let name = library.name();
    let library_dir = library_dir.as_ref();

    info!("Checking library {name} {version:?}");
    let library_dir = library_dir.join(name);

    let version_dir = library_dir.join(match version {
        ReleaseVersion::Latest => "latest",
        ReleaseVersion::Tag(tag) => tag,
    });

    if version_dir.exists() {
        if matches!(version, ReleaseVersion::Latest)
            && time_since_update.map_or(true, |d| d > Duration::from_secs(86400))
        {
            info!("Checking latest release for {name} {version:?}");
            let release = match library.get_release(version) {
                Ok(release) => release,
                Err(err) => {
                    error!("Unable to check latest release for {name}: {err}");
                    return Ok(State::untouched(version_dir));
                }
            };

            let latest_version = version_dir.read_link()?;
            let latest_version = latest_version.file_name().unwrap_or_default();

            if latest_version == &*release.version {
                debug!("Latest version for {name} is still {}", &release.version);
                return Ok(State::new(version_dir, true));
            }

            info!("Updating {name} to {}", release.version);
            if let Err(err) =
                download_library(release, library, &library_dir, version_dir.clone(), version)
            {
                error!("Unable to update {name}: {err}");
            }
        }

        return Ok(State::new(version_dir, true));
    }

    debug!("Checking release for {name} {version:?}");
    let release = library.get_release(version)?;
    let path = download_library(release, library, &library_dir, version_dir, version)?;

    Ok(State::new(path, matches!(version, ReleaseVersion::Latest)))
}

pub fn ensure_winetricks_exists(cache_dir: impl AsRef<Path>) -> Result<(), Error> {
    let target = cache_dir.as_ref().join(".bin").join("winetricks");
    if target.exists() {
        return Ok(());
    }

    info!("Downloading winetricks");
    let url = "https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks";
    let (mut read, pb) = download_file(url)?.progress("winetricks");

    let _ = fs::create_dir_all(cache_dir.as_ref().join(".bin"));
    let mut file = File::create(target)?;
    file.set_permissions(Permissions::from_mode(0o755))?;
    io::copy(&mut read, &mut file)?;

    pb.finish();
    Ok(())
}

pub fn ensure_cabextract_exists(cache_dir: impl AsRef<Path>) -> Result<(), Error> {
    let target = cache_dir.as_ref().join(".bin").join("cabextract");
    if target.exists() {
        return Ok(());
    }

    info!("Downloading cabextract");
    let url = "https://archlinux.org/packages/extra/x86_64/cabextract/download/";
    let (read, pb) = download_file(url)?.progress("cabextract");

    let _ = fs::create_dir_all(cache_dir.as_ref().join(".bin"));
    let mut tar = Archive::new(ZstDecoder::new(read)?);
    for e in tar.entries()? {
        let mut e = e?;

        if e.path()?.file_name().unwrap_or_default() == "cabextract" {
            let mut file = File::create(target)?;
            file.set_permissions(Permissions::from_mode(0o755))?;
            io::copy(&mut e, &mut file)?;
            break;
        }
    }

    pb.finish();
    Ok(())
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use brie_cfg::{Library, ReleaseVersion, Runtime};
    use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

    use crate::{library::ensure_library_exists, runtime::ensure_runtime_exists};

    #[test]
    #[ignore]
    fn test_download() {
        let version = ReleaseVersion::Latest;
        let cache_dir = Path::new("./.tmp");

        let libraries = [
            Library::Dxvk,
            Library::DxvkGplAsync,
            Library::DxvkNvapi,
            Library::Vkd3dProton,
            Library::NvidiaLibs,
        ];

        rayon::scope(|s| {
            s.spawn(|_| {
                ensure_runtime_exists(
                    cache_dir.join("wine"),
                    &Runtime::GeProton {
                        version: ReleaseVersion::Latest,
                    },
                    None,
                )
                .unwrap();
            });

            libraries.par_iter().for_each(|l| {
                ensure_library_exists(l, cache_dir, &version, None).unwrap();
            });
        });

        // FIXME add assertions
    }
}
