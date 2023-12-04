use std::{
    fs::{self, File, Permissions},
    io::{self},
    os::unix::{self, fs::PermissionsExt},
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressFinish, ProgressState, ProgressStyle};
use log::info;
use tar::Archive;
use thiserror::Error;
use xz2::read::XzDecoder;
use zstd::stream::Decoder as ZstDecoder;

use crate::{
    config::{Library, ReleaseVersion},
    downloader::{
        self, download_file,
        github::{with_suffix, Github},
        gitlab::{filename_version, Gitlab},
        GitRepo, ReleaseProvider, ReleaseStream,
    },
    MP,
};

#[derive(Error, Debug)]
pub enum Error {
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
            // Library::NvidiaLibs => Github::new(with_suffix(".tar.xz"))
            //     .get_release(&GitRepo::new("SveSop", "nvidia-libs"), version),
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

fn progress(
    name: &'static str,
    stream: ReleaseStream<impl io::Read>,
) -> (impl io::Read, ProgressBar) {
    let pb = match stream.len {
        Some(len) => ProgressBar::new(len as u64),
        None => ProgressBar::new_spinner(),
    };

    let pb = pb
        .with_message(name)
        .with_finish(ProgressFinish::AndLeave)
        .with_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta}) - {msg:>15}")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-"));

    let pb = MP.add(pb);

    (pb.wrap_read(stream.body), pb)
}

pub fn ensure_library_exists(
    library: &impl Downloadable,
    cache_dir: impl AsRef<Path>,
    version: &ReleaseVersion,
) -> Result<PathBuf, Error> {
    let name = library.name();
    let cache_dir = cache_dir.as_ref();

    info!("Checking library {name} {version}");
    let cache_dir = cache_dir.join(name);
    let version_dir = cache_dir.join(version.as_path());

    if version_dir.exists() {
        return Ok(version_dir);
    }

    info!("Downloading library {name} {version}");
    let release = library.get_release(version)?;
    let dest = cache_dir.join(&release.version);

    fs::create_dir_all(&dest)?;

    let mut guard = DirGuard::new(&dest);

    let lib = download_file(&release.url)?;
    let (lib, pb) = progress(name, lib);

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

pub fn ensure_winetricks_exists(cache_dir: impl AsRef<Path>) -> Result<(), Error> {
    let target = cache_dir.as_ref().join(".bin").join("winetricks");
    if target.exists() {
        return Ok(());
    }

    info!("Downloading winetricks");
    let url = "https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks";
    let file = download_file(url)?;
    let (mut read, pb) = progress("winetricks", file);

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
    let file = download_file(url)?;
    let (read, pb) = progress("winetricks", file);

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

    use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

    use crate::{
        config::{Library, ReleaseVersion, Runtime},
        library::ensure_library_exists,
        runtime::get_runtime,
    };

    #[test]
    fn test_download() {
        let version = ReleaseVersion::Latest;
        let cache_dir = Path::new("./tmp");

        let libraries = [
            Library::Dxvk,
            Library::DxvkGplAsync,
            Library::DxvkNvapi,
            Library::Vkd3dProton,
        ];

        rayon::scope(|s| {
            s.spawn(|_| {
                get_runtime(
                    cache_dir.join("wine"),
                    &Runtime::GeProton(ReleaseVersion::Latest),
                )
                .unwrap();
            });

            libraries.par_iter().for_each(|l| {
                ensure_library_exists(l, cache_dir, &version).unwrap();
            });
        });
    }
}
