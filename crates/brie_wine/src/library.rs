use std::{
    fs::{self, File, Permissions},
    io::{self, Cursor, Read},
    os::unix::{self, fs::PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use brie_cfg::{Library, ReleaseVersion, Tokens};
use brie_download::download_file;
use flate2::read::GzDecoder;
use log::{debug, error, info};
use tar::Archive;
use thiserror::Error;
use xz2::read::XzDecoder;
use zstd::stream::Decoder as ZstDecoder;

use crate::downloader::{
    self,
    github::{self, with_suffix},
    gitlab::{self, filename_version},
    GitRepo,
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
    #[error("ZIP error. {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Unknown library archive format for file {0}.")]
    UnknownFormat(String),
}

pub trait Downloadable {
    /// Folder name where the artifact will be saved to
    fn name(&self) -> &'static str;

    /// Used to strip the directory from the archive
    fn substring(&self) -> &'static str {
        self.name()
    }

    /// Get download link for the given release version.
    /// Also returns the resolved release version (e.g. for "latest")
    fn get_meta(
        &self,
        tokens: &Tokens,
        version: &ReleaseVersion,
    ) -> Result<downloader::Release, downloader::Error>;

    /// Downloads the artifacts and unpacks it to dir.
    fn download(
        &self,
        tokens: &Tokens,
        release: &downloader::Release,
        dest: &Path,
    ) -> Result<(), Error>;
}

pub struct WineGe;

impl Downloadable for WineGe {
    fn name(&self) -> &'static str {
        "wine-ge-custom"
    }

    fn substring(&self) -> &'static str {
        "GE-Proton"
    }

    fn get_meta(
        &self,
        tokens: &Tokens,
        version: &ReleaseVersion,
    ) -> Result<downloader::Release, downloader::Error> {
        github::Client::new(tokens.github.as_deref()).release(
            GitRepo::new("GloriousEggroll", "wine-ge-custom"),
            version,
            with_suffix(".tar.xz"),
        )
    }

    fn download(
        &self,
        tokens: &Tokens,
        release: &downloader::Release,
        dest: &Path,
    ) -> Result<(), Error> {
        let authorization = tokens.github.as_ref().map(|t| format!("Bearer {t}"));

        let (lib, pb) =
            download_file(&release.url, authorization.as_deref())?.progress(self.name());

        match &release.filename {
            n if n.ends_with(".tar.gz") => untar(GzDecoder::new(lib), dest)?,
            n if n.ends_with(".tar.xz") => untar(XzDecoder::new(lib), dest)?,
            n if n.ends_with(".tar.zst") => untar(ZstDecoder::new(lib)?, dest)?,
            _ => {
                return Err(Error::UnknownFormat(release.filename.to_string()));
            }
        }

        pb.finish();

        Ok(())
    }
}

pub struct WineTkg;

impl Downloadable for WineTkg {
    fn name(&self) -> &'static str {
        "wine-tkg"
    }

    fn get_meta(
        &self,
        tokens: &Tokens,
        version: &ReleaseVersion,
    ) -> Result<downloader::Release, downloader::Error> {
        #[allow(clippy::unreadable_literal)]
        github::Client::new(tokens.github.as_deref()).workflow_artifact(
            GitRepo::new("Frogging-Family", "wine-tkg-git"),
            version,
            11219483, // Wine Arch Linux CI
            with_suffix("wine-tkg-build"),
        )
    }

    fn download(
        &self,
        tokens: &Tokens,
        release: &downloader::Release,
        dest: &Path,
    ) -> Result<(), Error> {
        let authorization = tokens.github.as_ref().map(|t| format!("Bearer {t}"));

        let (mut lib, pb) =
            download_file(&release.url, authorization.as_deref())?.progress(self.name());

        let buf = {
            let mut buf = Vec::new();
            lib.read_to_end(&mut buf)?;
            let mut zip = Cursor::new(buf);
            let mut zip = zip::ZipArchive::new(&mut zip)?;
            let mut tar_zst = zip.by_index(0)?;
            #[allow(clippy::cast_possible_truncation)]
            let mut buf = Vec::with_capacity(tar_zst.size() as usize);
            tar_zst.read_to_end(&mut buf)?;
            buf
        };

        untar(ZstDecoder::new(Cursor::new(buf))?, dest)?;

        pb.finish();

        Ok(())
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

    fn get_meta(
        &self,
        tokens: &Tokens,
        version: &ReleaseVersion,
    ) -> Result<downloader::Release, downloader::Error> {
        match self {
            Library::Dxvk => github::Client::new(tokens.github.as_deref()).release(
                GitRepo::new("doitsujin", "dxvk"),
                version,
                |a| a.name.ends_with(".tar.gz") && !a.name.contains("sniper"),
            ),
            Library::DxvkGplAsync => gitlab::Client.tree_file(
                GitRepo::new("Ph42oN", "dxvk-gplasync"),
                version,
                "releases",
                filename_version("dxvk-gplasync-", ".tar.gz"),
            ),
            Library::DxvkNvapi => github::Client::new(tokens.github.as_deref()).release(
                GitRepo::new("jp7677", "dxvk-nvapi"),
                version,
                with_suffix(".tar.gz"),
            ),
            Library::Vkd3dProton => github::Client::new(tokens.github.as_deref()).release(
                GitRepo::new("HansKristian-Work", "vkd3d-proton"),
                version,
                with_suffix(".tar.zst"),
            ),
            Library::NvidiaLibs => github::Client::new(tokens.github.as_deref()).release(
                GitRepo::new("SveSop", "nvidia-libs"),
                version,
                with_suffix(".tar.xz"),
            ),
        }
    }

    fn download(
        &self,
        tokens: &Tokens,
        release: &downloader::Release,
        dest: &Path,
    ) -> Result<(), Error> {
        let authorization = match self {
            Library::DxvkGplAsync => None,
            Library::Dxvk | Library::DxvkNvapi | Library::NvidiaLibs | Library::Vkd3dProton => {
                tokens.github.as_ref().map(|t| format!("Bearer {t}"))
            }
        };

        let (lib, pb) =
            download_file(&release.url, authorization.as_deref())?.progress(self.name());

        match &release.filename {
            n if n.ends_with(".tar.gz") => untar(GzDecoder::new(lib), dest)?,
            n if n.ends_with(".tar.xz") => untar(XzDecoder::new(lib), dest)?,
            n if n.ends_with(".tar.zst") => untar(ZstDecoder::new(lib)?, dest)?,
            _ => {
                return Err(Error::UnknownFormat(release.filename.to_string()));
            }
        }

        pb.finish();

        Ok(())
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

impl Drop for DirGuard<'_> {
    fn drop(&mut self) {
        if !self.success {
            info!("Removing {path}", path = self.path.display());
            let _ = fs::remove_dir_all(self.path);
        }
    }
}

fn download_library(
    library: &impl Downloadable,
    version: &ReleaseVersion,
    release: &downloader::Release,
    library_dir: &Path,
    tokens: &Tokens,
) -> Result<(), Error> {
    let name = library.name();

    info!("Downloading library {name} {version:?}: {release:?}");
    let dest = library_dir.join(&release.version);

    fs::create_dir_all(&dest)?;

    // Auto-delete directory if extraction fails mid-way
    let mut guard = DirGuard::new(&dest);

    library.download(tokens, release, &dest)?;

    if let Some(dest) = contains_single_directory_with_substring(&dest, library.substring())? {
        move_paths_to_parent_directory(&dest)?;
    }

    if version == &ReleaseVersion::Latest {
        let dir = library_dir.join("latest");

        _ = fs::remove_file(&dir);
        unix::fs::symlink(&release.version, &dir)?;
    }

    guard.success = true;

    Ok(())
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
    tokens: &Tokens,
    version: &ReleaseVersion,
    time_since_update: Option<Duration>,
) -> Result<State, Error> {
    let name = library.name();
    let library_dir = library_dir.as_ref();

    info!("Checking library {name} {version:?}");
    let library_dir = library_dir.join(name);
    let version_dir = library_dir.join(version.to_str());

    if version_dir.exists() {
        if matches!(version, ReleaseVersion::Latest)
            && time_since_update.is_none_or(|d| d > Duration::from_secs(86400))
        {
            info!("Checking latest release for {name} {version:?}");
            let release = match library.get_meta(tokens, version) {
                Ok(release) => release,
                Err(err) => {
                    error!("Unable to check latest release for {name}: {err}");
                    return Ok(State::untouched(version_dir));
                }
            };

            // Check symlink of the "latest" folder
            let latest_version = version_dir.read_link()?;
            let latest_version = latest_version.file_name().unwrap_or_default();

            if latest_version == &*release.version {
                debug!("Latest version for {name} is still {}", &release.version);
                return Ok(State::new(version_dir, true));
            }

            info!("Updating {name} to {}", release.version);
            if let Err(err) = download_library(library, version, &release, &library_dir, tokens) {
                error!("Unable to update {name}: {err}");
            }
        }

        return Ok(State::new(version_dir, true));
    }

    debug!("Checking release for {name} {version:?}");
    let release = library.get_meta(tokens, version)?;
    download_library(library, version, &release, &library_dir, tokens)?;

    Ok(State::new(
        version_dir,
        matches!(version, ReleaseVersion::Latest),
    ))
}

pub fn ensure_winetricks_exists(cache_dir: impl AsRef<Path>) -> Result<(), Error> {
    let target = cache_dir.as_ref().join(".bin").join("winetricks");
    if target.exists() {
        return Ok(());
    }

    info!("Downloading winetricks");
    let url = "https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks";
    let (mut read, pb) = download_file(url, None)?.progress("winetricks");

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
    let (read, pb) = download_file(url, None)?.progress("cabextract");

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

    use brie_cfg::{Library, ReleaseVersion, Runtime, Tokens};
    use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

    use crate::{library::ensure_library_exists, runtime::ensure_runtime_exists};

    #[test]
    #[ignore]
    fn test_download() {
        let version = ReleaseVersion::Latest;
        let cache_dir = Path::new("./.tmp");

        let tokens = Tokens {
            github: Some(std::env::var("GITHUB_TOKEN").unwrap()),
            ..Tokens::default()
        };

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
                    &tokens,
                    cache_dir.join("wine"),
                    &Runtime::GeProton {
                        version: ReleaseVersion::Latest,
                    },
                    None,
                )
                .unwrap();
            });

            s.spawn(|_| {
                ensure_runtime_exists(
                    &tokens,
                    cache_dir.join("wine"),
                    &Runtime::Tkg {
                        version: ReleaseVersion::Latest,
                    },
                    None,
                )
                .unwrap();
            });

            libraries.par_iter().for_each(|l| {
                ensure_library_exists(l, cache_dir, &tokens, &version, None).unwrap();
            });
        });

        // FIXME add assertions
    }
}
