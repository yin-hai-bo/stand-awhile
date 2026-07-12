use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use windows::Win32::{
    Foundation::HWND,
    System::Com::CoTaskMemFree,
    UI::{
        Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, ShellExecuteW},
        WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW, SW_SHOWNORMAL},
    },
};
use windows::core::{Error, HRESULT, PCWSTR, PWSTR, Result, w};

use crate::i18n::{detect_language, main_window_title};

const APP_DIRECTORY_NAME: &str = "yhb";
const APP_SUBDIRECTORY_NAME: &str = "stand-awhile";
const CONFIG_FILE_NAME: &str = "config.json";
const DEFAULT_CONFIG_CONTENTS: &str = "{}\n";
const DEFAULT_PERIOD_SECONDS: u32 = 20 * 60;
const DEFAULT_TRAY_WHEN_CLOSE: bool = false;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub period: u32,
    pub tray_when_close: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            period: DEFAULT_PERIOD_SECONDS,
            tray_when_close: DEFAULT_TRAY_WHEN_CLOSE,
        }
    }
}

#[derive(Deserialize)]
struct ConfigFile {
    period: Option<u32>,
    tray_when_close: Option<bool>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = ensure_config_file_path()?;
        let contents = fs::read_to_string(path).map_err(io_error_to_win_error)?;
        let file: ConfigFile = serde_json::from_str(&contents)
            .map_err(|error| Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()))?;

        Ok(Self {
            period: file.period.unwrap_or(DEFAULT_PERIOD_SECONDS),
            tray_when_close: file.tray_when_close.unwrap_or(DEFAULT_TRAY_WHEN_CLOSE),
        })
    }
}

pub fn open_config_directory(hwnd: HWND) -> Result<()> {
    let config_dir = ensure_config_directory()?;

    let directory = wide_null(config_dir.as_os_str().to_string_lossy().as_ref());
    let result = unsafe {
        ShellExecuteW(
            Some(hwnd),
            w!("open"),
            PCWSTR(directory.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if (result.0 as usize) <= 32 {
        return Err(Error::from_win32());
    }

    Ok(())
}

pub fn show_config_open_error(hwnd: HWND, error: &windows::core::Error) {
    let title = wide_null(main_window_title(detect_language()));
    let message = wide_null(&error.to_string());

    unsafe {
        let _ = MessageBoxW(
            Some(hwnd),
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn ensure_config_directory() -> Result<PathBuf> {
    let appdata = roaming_appdata_dir()?;
    let config_dir = appdata.join(APP_DIRECTORY_NAME).join(APP_SUBDIRECTORY_NAME);
    fs::create_dir_all(&config_dir).map_err(io_error_to_win_error)?;

    let config_file = config_file_path(&config_dir);
    ensure_config_file(&config_file)?;

    Ok(config_dir)
}

fn ensure_config_file_path() -> Result<PathBuf> {
    let config_dir = ensure_config_directory()?;
    Ok(config_file_path(&config_dir))
}

fn config_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CONFIG_FILE_NAME)
}

fn ensure_config_file(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    fs::write(path, DEFAULT_CONFIG_CONTENTS).map_err(io_error_to_win_error)
}

fn roaming_appdata_dir() -> Result<PathBuf> {
    let path = unsafe { SHGetKnownFolderPath(&FOLDERID_RoamingAppData, Default::default(), None)? };
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

fn io_error_to_win_error(error: std::io::Error) -> Error {
    match error.raw_os_error() {
        Some(code) => Error::new(HRESULT::from_win32(code as u32), error.to_string()),
        None => Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()),
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn config_default_values_are_stable() {
        let config = Config::default();
        assert_eq!(config.period, 20 * 60);
        assert!(!config.tray_when_close);
    }
}
