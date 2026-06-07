use std::{
    borrow::Cow,
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use brie_cfg::Library;
use indexmap::IndexMap;
use log::{debug, info};
use thiserror::Error;

use crate::{command::Runner, library::Downloadable, WithContext};

#[cfg(not(target_os = "linux"))]
mod dl {
    use std::io;

    pub fn find_dl_path(_library: &str) -> Result<String, io::Error> {
        return Err(io::Error::other("Unsupported platform"));
    }
}

#[cfg(target_os = "linux")]
mod dl {
    use std::{ffi::CStr, io};

    struct Dl(*mut libc::c_void);

    impl Dl {
        fn open(library: &str) -> Result<Self, io::Error> {
            let lib =
                unsafe { libc::dlopen(format!("{library}\0").as_ptr().cast(), libc::RTLD_LAZY) };
            if lib.is_null() {
                let error = unsafe { CStr::from_ptr(libc::dlerror()) };
                let error = error.to_string_lossy().to_string();

                return Err(io::Error::other(error));
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
        #[ignore = "not really a test"]
        fn test_dl() {
            // FIXME: use a static asset instead of guessing system so
            assert_eq!(find_dl_path("libelf.so").unwrap(), "/usr/lib");
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
    #[error("No supported nvidia-libs DLL layout found at {0}")]
    NvidiaLayout(PathBuf),
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error installing {0} library. {1}")]
    Library(&'static str, CopyError),
    #[error("Unable to override dlls. {0}")]
    Reg(io::Error),
    #[error("Unable to create reg file. Wine prefix is an invalid path.")]
    InvalidPath,
    #[error("Unable to update state file. {0}")]
    StateWrite(io::Error),
}

impl<T> WithContext<Result<T, Error>, &'static str> for Result<T, CopyError> {
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

        // Broken symlinks return false on `.exists()` check, so it is skipped here.
        if dest.is_symlink() {
            debug!("Destination is a symlink, removing it");
            let _ = fs::remove_file(&dest);
        }

        fs::copy(source, dest).map_err(CopyError::Copy)?;

        Ok(())
    }

    fn install_dlls<'a>(
        &self,
        overrides: &mut Overrides<'a>,

        path: &Path,
        arch: Arch,
        dlls: &[&'a str],
    ) -> Result<(), CopyError> {
        for dll in dlls {
            self.copy_dll(path.join(dll), arch)?;

            let dll = dll.strip_suffix(".so").unwrap_or(dll);
            let dll = dll.strip_suffix(".dll").unwrap_or(dll);
            overrides.insert(dll);
        }

        Ok(())
    }

    fn install_existing_dlls<'a>(
        &self,
        overrides: &mut Overrides<'a>,
        path: &Path,
        arch: Arch,
        dlls: &[&'a str],
    ) -> Result<usize, CopyError> {
        let mut installed = 0;

        for dll in dlls {
            let source = path.join(dll);

            if !source.exists() {
                debug!("Skipping missing {}", source.display());
                continue;
            }

            self.copy_dll(&source, arch)?;

            let dll = dll.strip_suffix(".so").unwrap_or(dll);
            let dll = dll.strip_suffix(".dll").unwrap_or(dll);
            overrides.insert(dll);
            installed += 1;
        }

        Ok(installed)
    }

    fn install_nvidia_libs(&self, overrides: &mut Overrides, path: &Path) -> Result<(), CopyError> {
        const X64_DLLS: &[&str] = &[
            "nvcuda.dll",
            "nvoptix.dll",
            "nvcuvid.dll",
            "nvencodeapi64.dll",
            "nvapi64.dll",
            "nvofapi64.dll",
        ];
        const X86_DLLS: &[&str] = &["nvcuda.dll", "nvcuvid.dll", "nvencodeapi.dll", "nvapi.dll"];
        const X64_FAKEDLLS: &[&str] = &[
            "nvcuda.dll.so",
            "nvoptix.dll.so",
            "nvcuvid.dll.so",
            "nvencodeapi64.dll.so",
        ];
        const X86_FAKEDLLS: &[&str] = &["nvcuda.dll.so", "nvcuvid.dll.so", "nvencodeapi.dll.so"];
        const X64_FAKEDLL_WINDOWS: &[&str] = &["nvapi64.dll", "nvofapi64.dll"];
        const X86_FAKEDLL_WINDOWS: &[&str] = &["nvapi.dll"];

        if path.join("x64").join("nvcuda.dll").exists() {
            let installed =
                self.install_existing_dlls(overrides, &path.join("x64"), Arch::X64, X64_DLLS)?;

            if installed == 0 {
                return Err(CopyError::NvidiaLayout(path.to_path_buf()));
            }

            self.install_existing_dlls(overrides, &path.join("x32"), Arch::X86, X86_DLLS)?;

            return Ok(());
        }

        let mut installed = 0;
        for dir in [
            path.join("lib64").join("wine").join("x86_64-unix"),
            path.join("lib").join("wine").join("x86_64-unix"),
        ] {
            installed += self.install_existing_dlls(overrides, &dir, Arch::X64, X64_FAKEDLLS)?;
        }

        for dir in [
            path.join("lib64").join("wine").join("x86_64-windows"),
            path.join("lib").join("wine").join("x86_64-windows"),
        ] {
            installed +=
                self.install_existing_dlls(overrides, &dir, Arch::X64, X64_FAKEDLL_WINDOWS)?;
        }

        if installed == 0 {
            return Err(CopyError::NvidiaLayout(path.to_path_buf()));
        }

        self.install_existing_dlls(
            overrides,
            &path.join("lib").join("wine").join("i386-unix"),
            Arch::X86,
            X86_FAKEDLLS,
        )?;
        self.install_existing_dlls(
            overrides,
            &path.join("lib").join("wine").join("i386-windows"),
            Arch::X86,
            X86_FAKEDLL_WINDOWS,
        )?;

        Ok(())
    }

    fn install_library_dlls(
        &self,
        overrides: &mut Overrides,
        library: Library,
        path: &Path,
    ) -> Result<(), CopyError> {
        let o = overrides;
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
                self.install_nvidia_libs(o, path)?;
            }
        }

        Ok(())
    }

    pub fn install_libraries(&self, libraries: &IndexMap<Library, PathBuf>) -> Result<(), Error> {
        let overrides_file = self.wine_prefix().join(".overrides");
        let overrides = fs::read_to_string(&overrides_file).unwrap_or_default();
        let mut overrides = Overrides::new(&overrides);

        for (library, path) in libraries {
            let name = library.name();
            info!("Copying library {name} dlls from {:?}", path.display());
            self.install_library_dlls(&mut overrides, *library, path)
                .context(name)?;
        }

        if let Ok(path) = dl::find_dl_path("libGLX_nvidia.so.0") {
            let path = Path::new(&path).join("nvidia").join("wine");
            if path.exists() {
                info!("Copying system nvngx dlls");
                let dlls = &["nvngx.dll", "_nvngx.dll"];
                self.install_dlls(&mut overrides, &path, Arch::X64, dlls)
                    .context("nvngx")?;
            }
        }

        if overrides.new.is_empty() {
            return Ok(());
        }

        debug!("Overriding dlls: {:?}", overrides.new);
        let reg = self.wine_prefix().join("dlls.reg");
        let reg = reg.to_str().ok_or(Error::InvalidPath)?;
        fs::write(reg, overrides.reg()).map_err(Error::Reg)?;
        self.command("wine", &["regedit", reg])
            .status()
            .map_err(Error::Reg)?;
        let _ = fs::remove_file(reg).map_err(Error::Reg);

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&overrides_file)
            .map_err(Error::StateWrite)?;

        for dll in overrides.new {
            writeln!(file, "{dll}").map_err(Error::StateWrite)?;
        }

        Ok(())
    }
}

pub fn mut_env(library: Library, path: &Path, env: &mut IndexMap<String, String>) {
    #[allow(clippy::single_match)]
    match library {
        Library::NvidiaLibs => {
            let Some(path64) = nvidia_winedll_path(path) else {
                return;
            };
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

fn nvidia_winedll_path(path: &Path) -> Option<PathBuf> {
    [
        path.join("x64").join("wine"),
        path.join("lib64").join("wine"),
        path.join("lib").join("wine"),
    ]
    .into_iter()
    .find(|path| path.exists())
}

struct Overrides<'a> {
    all: BTreeSet<&'a str>,
    new: BTreeSet<&'a str>,
}

impl<'a> Overrides<'a> {
    fn new(existing: &'a str) -> Self {
        Self {
            all: existing.lines().collect(),
            new: BTreeSet::new(),
        }
    }

    fn insert(&mut self, dll: &'a str) {
        if !self.all.contains(dll) {
            self.all.insert(dll);
            self.new.insert(dll);
        }
    }

    fn reg(&self) -> String {
        let mut reg = String::from(
            "Windows Registry Editor Version 5.00\n\n\
            [HKEY_CURRENT_USER\\Software\\Wine\\DllOverrides]\n",
        );

        for dll in &self.new {
            reg.push('"');
            reg.push_str(dll);
            reg.push_str("\"=\"native\"\n");
        }
        reg
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    use super::{CopyError, Overrides};
    use crate::{command::Runner, Library};

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("brie-dll-test-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, []).unwrap();
    }

    fn runner(root: &Path) -> Runner {
        let prefix = root.join("prefix");
        fs::create_dir_all(prefix.join("drive_c").join("windows").join("system32")).unwrap();
        fs::create_dir_all(prefix.join("drive_c").join("windows").join("syswow64")).unwrap();

        Runner::test(prefix)
    }

    #[test]
    fn installs_current_nvidia_regular_layout_without_32_bit_nvcuda() {
        let root = TestDir::new();
        let source = root.path().join("nvidia-libs");

        for dll in [
            "nvcuda.dll",
            "nvoptix.dll",
            "nvcuvid.dll",
            "nvencodeapi64.dll",
            "nvapi64.dll",
            "nvofapi64.dll",
        ] {
            touch(&source.join("x64").join(dll));
        }
        touch(&source.join("x32").join("nvapi.dll"));

        let runner = runner(root.path());
        let mut overrides = Overrides::new("");
        runner
            .install_library_dlls(&mut overrides, Library::NvidiaLibs, &source)
            .unwrap();

        let windows = runner.wine_prefix().join("drive_c").join("windows");
        assert!(windows.join("system32").join("nvcuda.dll").exists());
        assert!(windows.join("system32").join("nvencodeapi64.dll").exists());
        assert!(windows.join("syswow64").join("nvapi.dll").exists());
        assert!(!windows.join("syswow64").join("nvcuda.dll").exists());
        assert!(overrides.new.contains("nvcuda"));
        assert!(overrides.new.contains("nvapi"));
    }

    #[test]
    fn installs_nvidia_fakedll_layout() {
        let root = TestDir::new();
        let source = root.path().join("nvidia-libs");

        for dll in [
            "nvcuda.dll.so",
            "nvoptix.dll.so",
            "nvcuvid.dll.so",
            "nvencodeapi64.dll.so",
        ] {
            touch(
                &source
                    .join("lib")
                    .join("wine")
                    .join("x86_64-unix")
                    .join(dll),
            );
        }
        touch(
            &source
                .join("lib")
                .join("wine")
                .join("x86_64-windows")
                .join("nvapi64.dll"),
        );
        touch(
            &source
                .join("lib")
                .join("wine")
                .join("i386-windows")
                .join("nvapi.dll"),
        );

        let runner = runner(root.path());
        let mut overrides = Overrides::new("");
        runner
            .install_library_dlls(&mut overrides, Library::NvidiaLibs, &source)
            .unwrap();

        let windows = runner.wine_prefix().join("drive_c").join("windows");
        assert!(windows.join("system32").join("nvcuda.dll").exists());
        assert!(windows.join("system32").join("nvapi64.dll").exists());
        assert!(windows.join("syswow64").join("nvapi.dll").exists());
        assert!(overrides.new.contains("nvcuda"));
        assert!(overrides.new.contains("nvapi64"));
    }

    #[test]
    fn rejects_nvidia_archive_without_64_bit_layout() {
        let root = TestDir::new();
        let source = root.path().join("nvidia-libs");
        touch(&source.join("x32").join("nvcuda.dll"));

        let runner = runner(root.path());
        let mut overrides = Overrides::new("");
        let err = runner
            .install_library_dlls(&mut overrides, Library::NvidiaLibs, &source)
            .unwrap_err();

        assert!(matches!(err, CopyError::NvidiaLayout(_)));
    }
}
