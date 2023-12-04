use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use log::{debug, info};
use thiserror::Error;

use crate::{config::Library, libraries::DownloadableLibrary, run::CommandRunner};

mod dl {
    use std::{ffi::CStr, io};

    struct Dl(*mut libc::c_void);

    impl Dl {
        fn open(library: &str) -> Result<Self, io::Error> {
            let lib =
                unsafe { libc::dlopen(format!("{library}\0").as_ptr().cast(), libc::RTLD_NOW) };
            if lib.is_null() {
                return Err(io::Error::last_os_error());
            }

            Ok(Self(lib))
        }

        fn path(&self) -> Result<String, io::Error> {
            let name = [0u8; libc::PATH_MAX as usize + 1];
            if unsafe { libc::dlinfo(self.0, libc::RTLD_DI_ORIGIN, name.as_ptr() as _) } == 0 {
                let path = CStr::from_bytes_until_nul(&name)
                    .unwrap_or_default()
                    .to_string_lossy();

                Ok(path.to_string())
            } else {
                println!("!");
                Err(io::Error::last_os_error())
            }
        }
    }

    impl Drop for Dl {
        fn drop(&mut self) {
            unsafe { libc::dlclose(self.0) };
        }
    }

    pub fn find_dl_path(library: &str) -> Result<String, io::Error> {
        Dl::open(library)?.path()
    }

    #[cfg(test)]
    mod test {
        use crate::dll::dl::find_dl_path;

        #[test]
        fn test_dl() {
            assert_eq!(find_dl_path("libudev.so").unwrap(), "/usr/lib");
        }
    }
}

#[derive(Clone, Copy)]
pub enum Arch {
    X86,
    X64,
}

impl Arch {
    fn dir(self) -> &'static str {
        match self {
            Arch::X86 => "syswow64",
            Arch::X64 => "system32",
        }
    }
}

#[derive(Debug, Error)]
pub enum CopyError {
    #[error("Unable to copy dll: {0}")]
    Copy(io::Error),
    #[error("Invalid file name: {0}")]
    FileName(PathBuf),
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct OverrideError(#[from] io::Error);

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("Copy error: {0}")]
    Copy(#[from] CopyError),
    #[error("Override error: {0}")]
    Override(#[from] OverrideError),
    #[error("Unable to update state file: {0}")]
    StateWrite(io::Error),
}

impl CommandRunner {
    fn copy_dll(&self, dll: impl AsRef<Path>, arch: Arch) -> Result<(), CopyError> {
        let dest = self
            .wine_prefix()
            .join("drive_c")
            .join("windows")
            .join(arch.dir());

        let dll = dll.as_ref();
        let file_name = dll
            .file_name()
            .ok_or_else(|| CopyError::FileName(dll.to_path_buf()))?;

        debug!("Copying {} to {}", dll.display(), dest.display());
        fs::copy(dll, dest.join(file_name)).map_err(CopyError::Copy)?;

        Ok(())
    }

    fn override_dll(&self, dll: &str) -> Result<(), OverrideError> {
        debug!("Overriding {dll} to native in wine...");

        self.command(
            "wine",
            &[
                "reg",
                "add",
                r"'HKEY_CURRENT_USER\Software\Wine\DllOverrides'",
                "/v",
                &dll,
                "/d",
                "native",
                "/f",
            ],
        )
        .env("WINEDLLOVERRIDES", "mscoree,mshtml=")
        .status()?;

        Ok(())
    }

    fn install_dlls<'a>(
        &self,
        overrides: &mut Overrides<'a>,

        path: &Path,
        arch: Arch,
        dlls: &[&'a str],
    ) -> Result<(), InstallError> {
        for dll in dlls {
            self.copy_dll(path.join(dll).with_extension("dll"), arch)?;
            if !overrides.contains(dll) {
                self.override_dll(dll)?;
                overrides.insert(dll);
            }
        }

        Ok(())
    }

    pub fn install_libraries(
        &self,
        libraries: &HashMap<Library, PathBuf>,
    ) -> Result<(), InstallError> {
        let overrides_file = self.wine_prefix().join(".overrides");
        let overrides = fs::read_to_string(&overrides_file).unwrap_or_default();
        let mut overrides = Overrides::new(overrides.lines().collect());

        for (library, path) in libraries {
            info!(
                "Copying library {:?} dlls from {:?}...",
                library.name(),
                path
            );

            match library {
                Library::Dxvk | Library::DxvkGplAsync => {
                    let dlls = &["d3d9", "d3d10core", "d3d11", "dxgi"];
                    self.install_dlls(&mut overrides, &path.join("x64"), Arch::X64, dlls)?;
                    self.install_dlls(&mut overrides, &path.join("x32"), Arch::X86, dlls)?;
                }
                Library::DxvkNvapi => {
                    self.install_dlls(&mut overrides, &path.join("x64"), Arch::X64, &["nvapi64"])?;
                    self.install_dlls(&mut overrides, &path.join("x32"), Arch::X86, &["nvapi"])?;
                }
                Library::Vkd3dProton => {
                    let dlls = &["d3d12", "d3d12core"];
                    self.install_dlls(&mut overrides, &path.join("x64"), Arch::X64, dlls)?;
                    self.install_dlls(&mut overrides, &path.join("x86"), Arch::X86, dlls)?;
                }
            }
        }

        if let Ok(path) = dl::find_dl_path("libGLX_nvidia.so.0") {
            info!("Copying system nvngx...");

            let path = Path::new(&path).join("nvidia").join("wine");

            let dll = path.join("nvngx.dll");
            if dll.exists() {
                self.copy_dll(&dll, Arch::X64)?;
            }

            let dll = path.join("_nvngx.dll");
            if dll.exists() {
                self.copy_dll(&dll, Arch::X64)?;
            }
        }

        if overrides.new.is_empty() {
            return Ok(());
        }

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&overrides_file)
            .map_err(InstallError::StateWrite)?;

        for new in overrides.new {
            writeln!(file, "{new}").map_err(InstallError::StateWrite)?;
        }

        Ok(())
    }
}

struct Overrides<'a> {
    all: HashSet<&'a str>,
    new: HashSet<&'a str>,
}

impl<'a> Overrides<'a> {
    fn new(all: HashSet<&'a str>) -> Self {
        Self {
            all,
            new: HashSet::new(),
        }
    }

    fn contains(&self, dll: &str) -> bool {
        self.all.contains(dll)
    }

    fn insert(&mut self, dll: &'a str) {
        self.all.insert(dll);
        self.new.insert(dll);
    }
}
