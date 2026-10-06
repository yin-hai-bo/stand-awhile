use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows::Win32::{
    Foundation::HWND,
    System::Com::CoTaskMemFree,
    UI::{
        Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, ShellExecuteW},
        WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW, SW_SHOWNORMAL},
    },
};
use windows::core::{Error, HRESULT, PCWSTR, PWSTR, Result, w};

use crate::i18n::{Language, main_window_title, resolve_language};
use crate::speech_bubble::{SpeechBubbleConfig, SpeechMessage};
use crate::ui::theme::{Theme, resolve_theme};

const APP_DIRECTORY_NAME: &str = "yhb";
const APP_SUBDIRECTORY_NAME: &str = "stand-awhile";
const CONFIG_FILE_NAME: &str = "config.json";
const DEFAULT_CONFIG_CONTENTS: &str = "{}\n";
const DEFAULT_PERIOD_SECONDS: u32 = 20 * 60;
const DEFAULT_TRAY_WHEN_CLOSE: bool = false;
const DEFAULT_LANGUAGE: &str = "auto";
const DEFAULT_THEME: &str = "system";
const DEFAULT_CHARACTER: &str = "cat";
const DEFAULT_SPEECH_MESSAGE: &str = "Time to stand up and stretch!";
const DEFAULT_SPEECH_DURATION_MS: u64 = 5_000;
const DEFAULT_SPEECH_HIDDEN_GAP_MS: u64 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub period: u32,
    pub tray_when_close: bool,
    pub language: String,
    pub theme: String,
    pub character: String,
    pub speech_bubble: SpeechBubbleConfig,
    pub pet_position: Option<PetPosition>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PetPosition {
    pub monitor: String,
    pub relative_x: i32,
    pub relative_y: i32,
    pub screen_x: i32,
    pub screen_y: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            period: DEFAULT_PERIOD_SECONDS,
            tray_when_close: DEFAULT_TRAY_WHEN_CLOSE,
            language: DEFAULT_LANGUAGE.to_owned(),
            theme: DEFAULT_THEME.to_owned(),
            character: DEFAULT_CHARACTER.to_owned(),
            speech_bubble: default_speech_bubble(),
            pet_position: None,
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ConfigFile {
    period: Option<u32>,
    tray_when_close: Option<bool>,
    language: Option<String>,
    theme: Option<String>,
    character: Option<String>,
    speech_bubble: Option<SpeechBubbleFile>,
    pet_position: Option<PetPosition>,
}

#[derive(Deserialize, Serialize)]
struct SpeechBubbleFile {
    messages: Vec<SpeechMessageFile>,
    hidden_gap_ms: u64,
}

#[derive(Deserialize, Serialize)]
struct SpeechMessageFile {
    text: String,
    display_duration_ms: u64,
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
            language: file.language.unwrap_or_else(|| DEFAULT_LANGUAGE.to_owned()),
            theme: file.theme.unwrap_or_else(|| DEFAULT_THEME.to_owned()),
            character: file.character.unwrap_or_else(|| DEFAULT_CHARACTER.to_owned()),
            speech_bubble: file
                .speech_bubble
                .map(speech_bubble_from_file)
                .unwrap_or_else(default_speech_bubble),
            pet_position: file.pet_position,
        })
    }

    pub fn language(&self) -> crate::i18n::Language {
        resolve_language(&self.language)
    }

    pub fn theme(&self) -> Theme {
        resolve_theme(&self.theme)
    }

    pub fn save(&self) -> Result<()> {
        let path = ensure_config_file_path()?;
        let file = ConfigFile {
            period: Some(self.period),
            tray_when_close: Some(self.tray_when_close),
            language: Some(self.language.clone()),
            theme: Some(self.theme.clone()),
            character: Some(self.character.clone()),
            speech_bubble: Some(speech_bubble_to_file(&self.speech_bubble)),
            pet_position: self.pet_position.clone(),
        };
        let contents = serde_json::to_string_pretty(&file)
            .map(|json| format!("{json}\n"))
            .map_err(|error| Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()))?;
        fs::write(path, contents).map_err(io_error_to_win_error)
    }
}

pub fn save_pet_position(position: PetPosition) -> Result<()> {
    let mut config = Config::load()?;
    config.pet_position = Some(position);
    config.save()
}

fn default_speech_bubble() -> SpeechBubbleConfig {
    SpeechBubbleConfig {
        messages: vec![SpeechMessage {
            text: DEFAULT_SPEECH_MESSAGE.to_owned(),
            display_duration: std::time::Duration::from_millis(DEFAULT_SPEECH_DURATION_MS),
        }],
        hidden_gap: std::time::Duration::from_millis(DEFAULT_SPEECH_HIDDEN_GAP_MS),
    }
}

fn speech_bubble_from_file(file: SpeechBubbleFile) -> SpeechBubbleConfig {
    SpeechBubbleConfig {
        messages: file
            .messages
            .into_iter()
            .map(|message| SpeechMessage {
                text: message.text,
                display_duration: std::time::Duration::from_millis(message.display_duration_ms),
            })
            .collect(),
        hidden_gap: std::time::Duration::from_millis(file.hidden_gap_ms),
    }
}

fn speech_bubble_to_file(config: &SpeechBubbleConfig) -> SpeechBubbleFile {
    SpeechBubbleFile {
        messages: config
            .messages
            .iter()
            .map(|message| SpeechMessageFile {
                text: message.text.clone(),
                display_duration_ms: message.display_duration.as_millis() as u64,
            })
            .collect(),
        hidden_gap_ms: config.hidden_gap.as_millis() as u64,
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

pub fn show_config_open_error(hwnd: HWND, error: &windows::core::Error, language: Language) {
    let title = wide_null(main_window_title(language));
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
        assert_eq!(config.language, "auto");
        assert_eq!(config.theme, "system");
        assert_eq!(config.character, "cat");
        assert_eq!(config.speech_bubble.messages.len(), 1);
        assert!(config.pet_position.is_none());
    }
}
