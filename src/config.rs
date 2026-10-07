use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows::core::{Error, Result};

use crate::i18n::resolve_language;
use crate::persistence::{config_file_path, io_error_to_win_error};
use crate::ui::theme::{Theme, resolve_theme};

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
    pub auto_hide_on_start: bool,
    pub language: String,
    pub theme: String,
    pub character: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            period: DEFAULT_PERIOD_SECONDS,
            tray_when_close: DEFAULT_TRAY_WHEN_CLOSE,
            auto_hide_on_start: true,
            language: DEFAULT_LANGUAGE.to_owned(),
            theme: DEFAULT_THEME.to_owned(),
            character: DEFAULT_CHARACTER.to_owned(),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ConfigFile {
    period: Option<u32>,
    tray_when_close: Option<bool>,
    auto_hide_on_start: Option<bool>,
    language: Option<String>,
    theme: Option<String>,
    character: Option<String>,
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
            auto_hide_on_start: file.auto_hide_on_start.unwrap_or(true),
            language: file.language.unwrap_or_else(|| DEFAULT_LANGUAGE.to_owned()),
            theme: file.theme.unwrap_or_else(|| DEFAULT_THEME.to_owned()),
            character: file.character.unwrap_or_else(|| DEFAULT_CHARACTER.to_owned()),
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
            auto_hide_on_start: Some(self.auto_hide_on_start),
            language: Some(self.language.clone()),
            theme: Some(self.theme.clone()),
            character: Some(self.character.clone()),
        };
        serde_json::to_string_pretty(&file)
            .map(|json| format!("{json}\n"))
            .map_err(|error| Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()))
    }
}

fn ensure_config_file_path() -> Result<PathBuf> {
    let path = config_file_path()?;
    ensure_config_file(&path)?;
    Ok(path)
}

fn ensure_config_file(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }

    fs::write(path, DEFAULT_CONFIG_CONTENTS).map_err(io_error_to_win_error)
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn config_default_values_are_stable() {
        let config = Config::default();
        assert_eq!(config.period, 20 * 60);
        assert!(!config.tray_when_close);
        assert!(config.auto_hide_on_start);
        assert_eq!(config.language, "auto");
        assert_eq!(config.theme, "system");
        assert_eq!(config.character, "cat");
    }

    #[test]
    fn legacy_runtime_and_speech_bubble_fields_are_ignored_and_not_written_back() {
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
        let saved = config.to_json().unwrap();
        let json: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert!(json.get("speech_bubble").is_none());
        assert!(json.get("pet_position").is_none());
        assert_eq!(Config::from_json(&saved).unwrap(), config);
    }

    #[test]
    fn omitted_or_null_settings_use_defaults() {
        assert_eq!(Config::from_json("{}").unwrap(), Config::default());
        assert_eq!(
            Config::from_json(r#"{"period":null,"language":null,"speech_bubble":null,"auto_hide_on_start":null}"#)
                .unwrap(),
            Config::default()
        );
    }

    #[test]
    fn auto_hide_preference_round_trips_without_changing_other_settings() {
        for enabled in [false, true] {
            let config = Config::from_json(&format!(
                r#"{{"period":90,"language":"zh","auto_hide_on_start":{enabled}}}"#
            ))
            .unwrap();
            assert_eq!(config.auto_hide_on_start, enabled);
            assert_eq!(Config::from_json(&config.to_json().unwrap()).unwrap(), config);
        }
    }
}
