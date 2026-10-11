use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use windows::Win32::{
    Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MoveFileExW},
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath},
};
use windows::core::{Error, GUID, HRESULT, PCWSTR, PWSTR, Result};

pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    write_atomic_with(path, |file| {
        file.write_all(contents.as_bytes())?;
        file.sync_all()
    })
}

fn write_atomic_with(path: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> Result<()> {
    let (temporary, mut file) = create_temporary_file(path).map_err(io_error_to_win_error)?;
    let result = write(&mut file).map_err(io_error_to_win_error);
    drop(file); // Close the handle before renaming or deleting the temporary file.
    let result = result.and_then(|()| {
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain([0]).collect();
        let destination: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // Both paths share a directory; never fall back to copying over the target.
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_REPLACE_EXISTING,
            )
        }
    });
    if let Err(error) = result {
        if let Err(cleanup_error) = fs::remove_file(&temporary) {
            return Err(Error::new(
                error.code(),
                format!(
                    "{error}; could not remove temporary file {}: {cleanup_error}",
                    temporary.display(),
                ),
            ));
        }
        return Err(error);
    }
    Ok(())
}

fn create_temporary_file(path: &Path) -> io::Result<(PathBuf, File)> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing file name"))?;
    loop {
        let mut temporary_name = name.to_os_string();
        temporary_name.push(format!(
            ".{}-{}.tmp",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = path.with_file_name(temporary_name);
        match OpenOptions::new().write(true).create_new(true).open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

pub(crate) fn config_file_path() -> Result<PathBuf> {
    data_file_path(&FOLDERID_RoamingAppData, "config.json")
}

pub(crate) fn state_file_path() -> Result<PathBuf> {
    data_file_path(&FOLDERID_LocalAppData, "state.json")
}

fn data_file_path(folder: &GUID, name: &str) -> Result<PathBuf> {
    let directory = appdata_dir(folder)?.join("yinhaibo").join("stand-awhile");
    fs::create_dir_all(&directory).map_err(io_error_to_win_error)?;
    Ok(directory.join(name))
}

fn appdata_dir(folder: &GUID) -> Result<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(folder, Default::default(), None)? };
    let result = pwstr_to_pathbuf(path);
    unsafe {
        CoTaskMemFree(Some(path.0.cast()));
    }
    result
}

fn pwstr_to_pathbuf(path: PWSTR) -> Result<PathBuf> {
    if path.is_null() {
        return Err(Error::from_win32());
    }

    let mut length = 0usize;
    unsafe {
        while *path.0.add(length) != 0 {
            length += 1;
        }
        let slice = std::slice::from_raw_parts(path.0, length);
        let text = String::from_utf16(slice).map_err(|_| Error::from_win32())?;
        Ok(PathBuf::from(text))
    }
}

pub(crate) fn io_error_to_win_error(error: std::io::Error) -> Error {
    match error.raw_os_error() {
        Some(code) => Error::new(HRESULT::from_win32(code as u32), error.to_string()),
        None => Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("stand-awhile-atomic-{name}-{}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn file_count(&self) -> usize {
            fs::read_dir(&self.0).unwrap().count()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn atomic_save_creates_and_replaces_utf8_file_without_temporary_files() {
        let directory = TestDirectory::new("成功 中文");
        let path = directory.0.join("配置.json");
        for contents in ["{\"角色\":\"小猫\"}\n", "{\"角色\":\"小狗🐕\"}\n"] {
            write_atomic(&path, contents).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
            assert_eq!(directory.file_count(), 1);
        }
    }

    #[test]
    fn partial_write_failure_preserves_old_file_and_cleans_temporary_file() {
        let directory = TestDirectory::new("partial-write");
        let path = directory.0.join("config.json");
        let old = "{\"character\":\"cat\"}\n";
        fs::write(&path, old).unwrap();
        let result = write_atomic_with(&path, |file| {
            file.write_all(b"{\"character\":")?;
            assert_eq!(fs::read_to_string(&path)?, old);
            Err(io::Error::other("injected write failure"))
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), old);
        assert_eq!(directory.file_count(), 1);
    }

    #[test]
    fn failed_first_write_does_not_publish_partial_file() {
        let directory = TestDirectory::new("first-write-failure");
        let path = directory.0.join("state.json");
        assert!(
            write_atomic_with(&path, |file| {
                file.write_all(b"{")?;
                Err(io::Error::other("injected write failure"))
            })
            .is_err()
        );
        assert!(!path.exists());
        assert_eq!(directory.file_count(), 0);
    }

    #[test]
    fn locked_target_preserves_old_file_and_cleans_temporary_file() {
        let directory = TestDirectory::new("locked-target");
        let path = directory.0.join("state.json");
        let old = "{\"pet_position\":null}\n";
        fs::write(&path, old).unwrap();
        let lock = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(&path)
            .unwrap();
        assert!(write_atomic(&path, "{}\n").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), old);
        assert_eq!(directory.file_count(), 1);
        drop(lock);
        write_atomic(&path, "{}\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}\n");
        assert_eq!(directory.file_count(), 1);
    }

    #[test]
    fn cleanup_failure_reports_temporary_path_and_preserves_target() {
        let directory = TestDirectory::new("cleanup-failure");
        let path = directory.0.join("config.json");
        fs::write(&path, "old").unwrap();
        let mut lock = None;
        let mut temporary = PathBuf::new();
        let result = write_atomic_with(&path, |file| {
            file.write_all(b"partial")?;
            temporary = fs::read_dir(&directory.0)?
                .map(|entry| entry.unwrap().path())
                .find(|entry| entry != &path)
                .unwrap();
            lock = Some(
                OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
                    .open(&temporary)?,
            );
            Err(io::Error::other("injected write failure"))
        });
        let error = result.unwrap_err().to_string();
        assert!(error.contains("injected write failure"));
        assert!(error.contains("could not remove temporary file"));
        assert!(error.contains(temporary.to_str().unwrap()));
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        drop(lock);
        fs::remove_file(temporary).unwrap();
        assert_eq!(directory.file_count(), 1);
    }

    #[test]
    fn missing_parent_returns_error_without_creating_files() {
        let directory = TestDirectory::new("missing-parent");
        assert!(write_atomic(&directory.0.join("missing").join("config.json"), "{}\n").is_err());
        assert_eq!(directory.file_count(), 0);
    }

    #[test]
    fn temporary_files_do_not_overwrite_each_other_or_existing_files() {
        let directory = TestDirectory::new("temporary-files");
        let path = directory.0.join("config.json");
        fs::write(&path, "old").unwrap();
        let (first, mut first_file) = create_temporary_file(&path).unwrap();
        first_file.write_all(b"first").unwrap();
        let (second, second_file) = create_temporary_file(&path).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.parent(), path.parent());
        assert_eq!(fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        drop(first_file);
        drop(second_file);
    }
}
