use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows::Win32::{
    System::Com::CoTaskMemFree,
    UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath},
};
use windows::core::{Error, HRESULT, PWSTR, Result};

use crate::i18n::resolve_language;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub period: u32,
    pub tray_when_close: bool,
    pub language: String,
    pub theme: String,
    pub character: String,
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
    pet_position: Option<PetPosition>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = ensure_config_file_path()?;
        let contents = fs::read_to_string(path).map_err(io_error_to_win_error)?;
        Self::from_json(&contents)
    }

    fn from_json(contents: &str) -> Result<Self> {
        let file: ConfigFile = serde_json::from_str(contents)
            .map_err(|error| Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()))?;

        Ok(Self {
            period: file.period.unwrap_or(DEFAULT_PERIOD_SECONDS),
            tray_when_close: file.tray_when_close.unwrap_or(DEFAULT_TRAY_WHEN_CLOSE),
            language: file.language.unwrap_or_else(|| DEFAULT_LANGUAGE.to_owned()),
            theme: file.theme.unwrap_or_else(|| DEFAULT_THEME.to_owned()),
            character: file.character.unwrap_or_else(|| DEFAULT_CHARACTER.to_owned()),
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
        fs::write(path, self.to_json()?).map_err(io_error_to_win_error)
    }

    fn to_json(&self) -> Result<String> {
        let file = ConfigFile {
            period: Some(self.period),
            tray_when_close: Some(self.tray_when_close),
            language: Some(self.language.clone()),
            theme: Some(self.theme.clone()),
            character: Some(self.character.clone()),
            pet_position: self.pet_position.clone(),
        };
        serde_json::to_string_pretty(&file)
            .map(|json| format!("{json}\n"))
            .map_err(|error| Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()))
    }
}

pub fn save_pet_position(position: PetPosition) -> Result<()> {
    let mut config = Config::load()?;
    config.pet_position = Some(position);
    config.save()
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
        assert!(config.pet_position.is_none());
    }

    #[test]
    fn legacy_speech_bubble_is_ignored_and_not_written_back() {
        let legacy = r#"{
            "period": 90, "language": "zh", "character": "dog",
            "speech_bubble": {
                "messages": [{"text": "custom message", "display_duration_ms": 8000}],
                "hidden_gap_ms": 2000
            },
            "pet_position": {"monitor": "test", "relative_x": 12, "relative_y": 34, "screen_x": 56, "screen_y": 78}
        }"#;
        let config = Config::from_json(legacy).unwrap();
        assert_eq!(config.period, 90);
        assert_eq!(config.language(), crate::i18n::Language::Chinese);
        assert_eq!(config.character, "dog");
        assert_eq!(config.pet_position.as_ref().unwrap().screen_x, 56);
        let saved = config.to_json().unwrap();
        let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert!(json.get("speech_bubble").is_none());
        assert_eq!(Config::from_json(&saved).unwrap(), config);
    }

    #[test]
    fn omitted_or_null_settings_use_defaults() {
        assert_eq!(Config::from_json("{}").unwrap(), Config::default());
        assert_eq!(
            Config::from_json(r#"{"period":null,"language":null,"speech_bubble":null}"#).unwrap(),
            Config::default()
        );
    }
}
