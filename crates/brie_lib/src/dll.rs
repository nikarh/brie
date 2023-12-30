use std::{
    borrow::Cow,
    collections::{BTreeMap, HashSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use brie_cfg::Library;
use log::{debug, info};
use thiserror::Error;

use crate::{command::Runner, library::Downloadable, WithContext};

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

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
pub enum Arch {
    X86,
    X64,
}

impl std::fmt::Display for Arch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Arch::X86 => "X86",
            Arch::X64 => "X64",
        })
    }
}

impl FromStr for Arch {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "X86" => Ok(Arch::X86),
            "X64" => Ok(Arch::X64),
            _ => Err(()),
        }
    }
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
    #[error("Unable to copy dll. {0}")]
    Copy(io::Error),
    #[error("Invalid file name: {0}")]
    FileName(PathBuf),
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct OverrideError(#[from] io::Error);

#[derive(Debug, Error)]
pub enum InstallLibraryError {
    #[error("Copy error. {0}")]
    Copy(#[from] CopyError),
    #[error("Override error. {0}")]
    Override(#[from] OverrideError),
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Install {0} library error. {1}")]
    Library(&'static str, InstallLibraryError),
    #[error("Unable to update state file. {0}")]
    StateWrite(io::Error),
}

impl<T> WithContext<Result<T, Error>, &'static str> for Result<T, InstallLibraryError> {
    fn context(self, context: &'static str) -> Result<T, Error> {
        self.map_err(|e| Error::Library(context, e))
    }
}

impl Runner {
    fn copy_dll(&self, source: impl AsRef<Path>, arch: Arch) -> Result<(), CopyError> {
        let dest = self
            .wine_prefix()
            .join("drive_c")
            .join("windows")
            .join(arch.dir());

        let source = source.as_ref();

        let target = match source.extension().is_some_and(|ext| ext == "so") {
            true => Cow::Owned(source.with_extension("")),
            false => Cow::Borrowed(source),
        };

        let file_name = target
            .file_name()
            .ok_or_else(|| CopyError::FileName(source.to_path_buf()))?;

        let dest = dest.join(file_name);

        debug!("Copying {} to {}", source.display(), dest.display());
        fs::copy(source, dest).map_err(CopyError::Copy)?;

        Ok(())
    }

    fn override_dll(&self, arch: Arch, dll: &str) -> Result<(), OverrideError> {
        debug!("Overriding {dll} to native in wine for {arch}");

        let key = r"HKEY_CURRENT_USER\Software\Wine\DllOverrides";

        let command = match arch {
            Arch::X86 => "wine",
            Arch::X64 => "wine64",
        };

        self.command(
            command,
            &["reg", "add", key, "/v", &dll, "/d", "native", "/f"],
        )
        .env("WINEDLLOVERRIDES", "winemenubuilder.exe,mscoree,mshtml=")
        .status()?;

        Ok(())
    }

    fn install_dlls<'a>(
        &self,
        overrides: &mut Overrides<'a>,

        path: &Path,
        arch: Arch,
        dlls: &[&'a str],
    ) -> Result<(), InstallLibraryError> {
        for dll in dlls {
            self.copy_dll(path.join(dll), arch)?;

            let dll = dll
                .strip_suffix(".so")
                .unwrap_or(dll)
                .strip_suffix(".dll")
                .unwrap_or(dll);

            if !overrides.contains(arch, dll) {
                self.override_dll(arch, dll)?;
                overrides.insert(arch, dll);
            }
        }

        Ok(())
    }

    pub fn install_libraries(&self, libraries: &BTreeMap<Library, PathBuf>) -> Result<(), Error> {
        let overrides_file = self.wine_prefix().join(".overrides");
        let overrides = fs::read_to_string(&overrides_file).unwrap_or_default();
        let mut overrides = Overrides::new(&overrides);

        let mut install = |library: Library, path: &Path| {
            let o = &mut overrides;
            match library {
                Library::Dxvk | Library::DxvkGplAsync => {
                    let dlls = &["d3d9.dll", "d3d10core.dll", "d3d11.dll", "dxgi.dll"];
                    self.install_dlls(o, &path.join("x64"), Arch::X64, dlls)?;
                    self.install_dlls(o, &path.join("x32"), Arch::X86, dlls)?;
                }
                Library::DxvkNvapi => {
                    self.install_dlls(o, &path.join("x64"), Arch::X64, &["nvapi64.dll"])?;
                    self.install_dlls(o, &path.join("x32"), Arch::X86, &["nvapi.dll"])?;
                }
                Library::Vkd3dProton => {
                    let dlls = &["d3d12.dll", "d3d12core.dll"];
                    self.install_dlls(o, &path.join("x64"), Arch::X64, dlls)?;
                    self.install_dlls(o, &path.join("x86"), Arch::X86, dlls)?;
                }
                Library::NvidiaLibs => {
                    let libs = path.join("lib64").join("wine").join("x86_64-unix");
                    self.install_dlls(o, &libs, Arch::X64, &["nvcuda.dll.so", "nvoptix.dll.so"])?;
                    let libs = path.join("lib").join("wine").join("i386-unix");
                    self.install_dlls(o, &libs, Arch::X86, &["nvcuda.dll.so"])?;
                }
            }

            Ok::<_, InstallLibraryError>(())
        };

        for (library, path) in libraries {
            let name = library.name();
            info!("Copying library {name} dlls from {:?}", path.display());
            install(*library, path).context(name)?;
        }

        if let Ok(path) = dl::find_dl_path("libGLX_nvidia.so.0") {
            info!("Copying system nvngx dlls");

            let path = Path::new(&path).join("nvidia").join("wine");

            let dll = path.join("nvngx.dll");
            if dll.exists() {
                self.copy_dll(&dll, Arch::X64)
                    .map_err(InstallLibraryError::from)
                    .context("nvngx")?;
            }

            let dll = path.join("_nvngx.dll");
            if dll.exists() {
                self.copy_dll(&dll, Arch::X64)
                    .map_err(InstallLibraryError::from)
                    .context("nvngx")?;
            }
        }

        if overrides.new.is_empty() {
            return Ok(());
        }

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&overrides_file)
            .map_err(Error::StateWrite)?;

        for (arch, dll) in overrides.new {
            writeln!(file, "{arch} {dll}").map_err(Error::StateWrite)?;
        }

        Ok(())
    }
}

pub fn mut_env(library: Library, path: &Path, env: &mut BTreeMap<String, String>) {
    #[allow(clippy::single_match)]
    match library {
        Library::NvidiaLibs => {
            let path64 = path.join("lib64").join("wine");
            let path64 = path64.display();

            let path = match env.get("WINEDLLPATH") {
                Some(path) => format!("{path}:{path64}"),
                None => format!("{path64}"),
            };

            env.insert("WINEDLLPATH".to_owned(), path);
        }
        _ => {}
    }
}

struct Overrides<'a> {
    all: HashSet<(Arch, &'a str)>,
    new: HashSet<(Arch, &'a str)>,
}

impl<'a> Overrides<'a> {
    fn new(existing: &'a str) -> Self {
        Self {
            all: existing
                .lines()
                .map(|line| line.split_whitespace())
                .filter_map(|mut s| match (s.next(), s.next()) {
                    (Some(arch), Some(dll)) => Some((arch, dll)),
                    _ => None,
                })
                .filter_map(|(arch, dll)| arch.parse().ok().map(|a| (a, dll)))
                .collect(),
            new: HashSet::new(),
        }
    }

    fn contains(&self, arch: Arch, dll: &str) -> bool {
        self.all.contains(&(arch, dll))
    }

    fn insert(&mut self, arch: Arch, dll: &'a str) {
        self.all.insert((arch, dll));
        self.new.insert((arch, dll));
    }
}
