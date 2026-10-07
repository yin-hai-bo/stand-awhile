use std::{os::windows::ffi::OsStrExt, path::Path};

use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, HWND},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey, RegCreateKeyExW,
            RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
        },
        UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW},
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::{config::Config, i18n::Language};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("Yinhaibo.StandAwhile");

struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

pub fn sync(enabled: bool) -> Result<()> {
    let executable = if enabled {
        Some(std::env::current_exe().map_err(crate::persistence::io_error_to_win_error)?)
    } else {
        None
    };
    sync_at(RUN_KEY, executable.as_deref())
}

// The key is a parameter so tests can never touch the real login startup entry.
fn sync_at(subkey: PCWSTR, executable: Option<&Path>) -> Result<()> {
    let mut key = HKEY::default();
    unsafe {
        if let Some(executable) = executable {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey,
                None,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
            .ok()?;
            let key = RegistryKey(key);
            update_value(key.0, Some(executable))
        } else {
            let status = RegOpenKeyExW(HKEY_CURRENT_USER, subkey, None, KEY_SET_VALUE, &mut key);
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            status.ok()?;
            let key = RegistryKey(key);
            update_value(key.0, None)
        }
    }
}

fn update_value(key: HKEY, executable: Option<&Path>) -> Result<()> {
    unsafe {
        if let Some(executable) = executable {
            let command: Vec<u8> = startup_command(executable)
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect();
            RegSetValueExW(key, VALUE_NAME, None, REG_SZ, Some(&command)).ok()
        } else {
            let status = RegDeleteValueW(key, VALUE_NAME);
            if status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                status.ok()
            }
        }
    }
}

fn startup_command(executable: &Path) -> Vec<u16> {
    // Keep the native UTF-16 path, including spaces and non-ASCII characters.
    "\"".encode_utf16()
        .chain(executable.as_os_str().encode_wide())
        .chain("\" --autostart\0".encode_utf16())
        .collect()
}

pub fn save_preference(
    previous: &Config,
    next: &Config,
    mut save: impl FnMut(&Config) -> Result<()>,
    sync: impl FnOnce(bool) -> Result<()>,
) -> Result<()> {
    save(next)?;
    if let Err(error) = sync(next.launch_at_startup) {
        let message = match (previous.language(), save(previous)) {
            (Language::Chinese, Ok(())) => format!("{error}\n已恢复原配置。"),
            (Language::English, Ok(())) => format!("{error}\nThe previous configuration was restored."),
            (Language::Chinese, Err(rollback)) => format!("{error}\n恢复原配置也失败：{rollback}\n请检查配置文件。"),
            (Language::English, Err(rollback)) => format!(
                "{error}\nRestoring the previous configuration also failed: {rollback}\nPlease check the configuration file."
            ),
        };
        return Err(Error::new(error.code(), message));
    }
    Ok(())
}

pub fn show_error(hwnd: HWND, language: Language, error: &Error) {
    let message = match language {
        Language::Chinese => format!("无法更新开机自启动设置。\n{error}"),
        Language::English => format!("Could not update launch-at-startup settings.\n{error}"),
    };
    let text: Vec<u16> = message.encode_utf16().chain([0]).collect();
    let title: Vec<u16> = crate::i18n::main_window_title(language)
        .encode_utf16()
        .chain([0])
        .collect();
    unsafe {
        let _ = MessageBoxW(
            Some(hwnd),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use windows::Win32::{
        Foundation::ERROR_ACCESS_DENIED,
        System::Registry::{KEY_ALL_ACCESS, KEY_QUERY_VALUE, REG_VALUE_TYPE, RegDeleteTreeW, RegQueryValueExW},
    };

    struct TestKey {
        path: Vec<u16>,
        key: RegistryKey,
    }

    impl TestKey {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path: Vec<u16> = format!(
                "Software\\Yinhaibo.StandAwhile.Tests-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )
            .encode_utf16()
            .chain([0])
            .collect();
            let mut key = HKEY::default();
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    None,
                    None,
                    REG_OPTION_NON_VOLATILE,
                    KEY_ALL_ACCESS,
                    None,
                    &mut key,
                    None,
                )
                .ok()
                .unwrap();
            }
            Self {
                path,
                key: RegistryKey(key),
            }
        }

        fn path(&self) -> PCWSTR {
            PCWSTR(self.path.as_ptr())
        }

        fn read(&self, name: PCWSTR) -> Option<Vec<u16>> {
            let mut size = 0;
            let mut value_type = REG_VALUE_TYPE::default();
            unsafe {
                let status = RegQueryValueExW(self.key.0, name, None, Some(&mut value_type), None, Some(&mut size));
                if status == ERROR_FILE_NOT_FOUND {
                    return None;
                }
                status.ok().unwrap();
                assert_eq!(value_type, REG_SZ);
                let mut value = vec![0u16; size as usize / 2];
                RegQueryValueExW(
                    self.key.0,
                    name,
                    None,
                    None,
                    Some(value.as_mut_ptr().cast()),
                    Some(&mut size),
                )
                .ok()
                .unwrap();
                Some(value)
            }
        }
    }

    impl Drop for TestKey {
        fn drop(&mut self) {
            unsafe {
                RegDeleteTreeW(HKEY_CURRENT_USER, self.path()).ok().unwrap();
            }
        }
    }

    #[test]
    fn registers_updates_and_removes_only_our_value_in_an_isolated_key() {
        let key = TestKey::new();
        unsafe {
            RegSetValueExW(key.key.0, w!("OtherApp"), None, REG_SZ, Some(&[0, 0]))
                .ok()
                .unwrap();
        }
        for path in [
            r"C:\Program Files\Stand Awhile\stand-awhile.exe",
            r"D:\中文 文件夹\站一站.exe",
        ] {
            for _ in 0..2 {
                sync_at(key.path(), Some(Path::new(path))).unwrap();
                let value = key.read(VALUE_NAME).unwrap();
                assert_eq!(value.last(), Some(&0));
                assert_eq!(
                    String::from_utf16(&value[..value.len() - 1]).unwrap(),
                    format!("\"{path}\" --autostart")
                );
            }
        }
        for _ in 0..2 {
            sync_at(key.path(), None).unwrap();
            assert_eq!(key.read(VALUE_NAME), None);
            assert_eq!(key.read(w!("OtherApp")), Some(vec![0]));
        }
        let mut child = HKEY::default();
        // A missing Run key must not be created when disabling startup.
        let missing_path: Vec<u16> = key.path[..key.path.len() - 1]
            .iter()
            .copied()
            .chain("\\Missing\0".encode_utf16())
            .collect();
        sync_at(PCWSTR(missing_path.as_ptr()), None).unwrap();
        assert_eq!(
            unsafe { RegOpenKeyExW(key.key.0, w!("Missing"), None, KEY_QUERY_VALUE, &mut child) },
            ERROR_FILE_NOT_FOUND
        );
    }

    #[test]
    fn registry_failure_restores_configuration_and_preserves_the_previous_entry() {
        let key = TestKey::new();
        let path = Path::new(r"C:\测试 路径\stand-awhile.exe");
        sync_at(key.path(), Some(path)).unwrap();
        let mut readonly = HKEY::default();
        unsafe {
            RegOpenKeyExW(HKEY_CURRENT_USER, key.path(), None, KEY_QUERY_VALUE, &mut readonly)
                .ok()
                .unwrap();
        }
        let readonly = RegistryKey(readonly);
        for previous_enabled in [false, true] {
            let previous = Config {
                period: 90,
                language: "en".into(),
                launch_at_startup: previous_enabled,
                ..Config::default()
            };
            let next = Config {
                launch_at_startup: !previous_enabled,
                ..previous.clone()
            };
            let mut saved = Vec::new();
            let error = save_preference(
                &previous,
                &next,
                |config| {
                    saved.push(config.clone());
                    Ok(())
                },
                |enabled| update_value(readonly.0, enabled.then_some(path)),
            )
            .unwrap_err();
            assert_eq!(error.code(), ERROR_ACCESS_DENIED.to_hresult());
            assert!(error.message().contains("previous configuration was restored"));
            assert_eq!(saved, vec![next, previous]);
            assert_eq!(key.read(VALUE_NAME), Some(startup_command(path)));
        }
    }

    #[test]
    fn configuration_failure_does_not_attempt_registry_changes() {
        let previous = Config::default();
        let next = Config {
            launch_at_startup: true,
            ..previous.clone()
        };
        let error = save_preference(
            &previous,
            &next,
            |_| Err(Error::from_hresult(ERROR_ACCESS_DENIED.to_hresult())),
            |_| panic!("registry must not change before configuration is saved"),
        )
        .unwrap_err();
        assert_eq!(error.code(), ERROR_ACCESS_DENIED.to_hresult());
    }

    #[test]
    fn rollback_failure_reports_both_errors_in_each_language() {
        for language in ["zh", "en"] {
            let previous = Config {
                language: language.into(),
                ..Config::default()
            };
            let next = Config {
                launch_at_startup: true,
                ..previous.clone()
            };
            let mut saves = 0;
            let error = save_preference(
                &previous,
                &next,
                |_| {
                    saves += 1;
                    if saves == 1 {
                        Ok(())
                    } else {
                        Err(Error::new(ERROR_ACCESS_DENIED.to_hresult(), "rollback failed"))
                    }
                },
                |_| Err(Error::new(ERROR_ACCESS_DENIED.to_hresult(), "registration failed")),
            )
            .unwrap_err();
            assert_eq!(saves, 2);
            assert!(error.message().contains("registration failed"));
            assert!(error.message().contains("rollback failed"));
            assert!(error.message().contains(if language == "zh" {
                "恢复原配置也失败"
            } else {
                "also failed"
            }));
        }
    }

    #[test]
    fn successful_update_saves_configuration_before_syncing() {
        use std::cell::Cell;
        let saved = Cell::new(false);
        for enabled in [false, true] {
            saved.set(false);
            let previous = Config::default();
            let next = Config {
                launch_at_startup: enabled,
                ..previous.clone()
            };
            save_preference(
                &previous,
                &next,
                |config| {
                    assert_eq!(config, &next);
                    saved.set(true);
                    Ok(())
                },
                |actual| {
                    assert!(saved.get());
                    assert_eq!(actual, enabled);
                    Ok(())
                },
            )
            .unwrap();
        }
    }
}
