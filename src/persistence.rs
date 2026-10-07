use std::{fs, path::PathBuf};

use windows::Win32::{
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath},
};
use windows::core::{Error, GUID, HRESULT, PWSTR, Result};

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
