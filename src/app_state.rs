use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use windows::core::{Error, HRESULT, Result};

use crate::persistence::{io_error_to_win_error, state_file_path};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AppState {
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

impl AppState {
    pub fn load() -> Result<Self> {
        Self::load_from(&state_file_path()?)
    }

    fn load_from(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).map_err(json_error),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(io_error_to_win_error(error)),
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&state_file_path()?)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(json_error)?;
        fs::write(path, format!("{json}\n")).map_err(io_error_to_win_error)
    }
}

pub fn save_pet_position(position: PetPosition) -> Result<()> {
    let mut state = AppState::load()?;
    state.pet_position = Some(position);
    state.save()
}

fn json_error(error: serde_json::Error) -> Error {
    Error::new(HRESULT(0x8000_4005u32 as i32), error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_or_null_position_uses_default() {
        for json in ["{}", r#"{"pet_position":null}"#] {
            assert_eq!(serde_json::from_str::<AppState>(json).unwrap(), AppState::default());
        }
    }

    #[test]
    fn state_persistence_is_independent_of_config() {
        let directory = std::env::temp_dir().join(format!("stand-awhile-state-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("state.json");
        let config_path = directory.join("config.json");
        let legacy_config =
            r#"{"period":90,"pet_position":{"monitor":"old","relative_x":1,"relative_y":2,"screen_x":3,"screen_y":4}}"#;
        fs::write(&config_path, legacy_config).unwrap();
        assert_eq!(AppState::load_from(&path).unwrap(), AppState::default());
        assert!(!path.exists());

        let state = AppState {
            pet_position: Some(PetPosition {
                monitor: "显示器".into(),
                relative_x: -20,
                relative_y: 30,
                screen_x: -40,
                screen_y: 50,
            }),
        };
        state.save_to(&path).unwrap();
        assert_eq!(AppState::load_from(&path).unwrap(), state);
        assert_eq!(fs::read_to_string(&config_path).unwrap(), legacy_config);
        fs::write(&config_path, "invalid config").unwrap();
        assert_eq!(AppState::load_from(&path).unwrap(), state);
        fs::write(&path, "invalid state").unwrap();
        assert!(AppState::load_from(&path).is_err());
        fs::remove_file(&path).unwrap();
        fs::remove_file(&config_path).unwrap();
        fs::remove_dir(&directory).unwrap();
    }
}
