use std::ffi::OsString;

use windows::Win32::{
    Foundation::{HWND, WPARAM},
    UI::WindowsAndMessaging::{
        SW_SHOW, SendMessageW, ShowWindow, WINDOW_STYLE, WS_CAPTION, WS_CLIPCHILDREN, WS_MINIMIZEBOX, WS_OVERLAPPED,
        WS_SYSMENU, WS_VISIBLE,
    },
};

use crate::pet_window::{PET_COMMAND_START, WM_PET_COMMAND};

pub fn has_autostart_argument(arguments: impl IntoIterator<Item = OsString>) -> bool {
    arguments.into_iter().any(|argument| argument == "--autostart")
}

pub fn handle_existing_instance(autostart: bool) -> windows::core::Result<()> {
    if autostart || crate::single_instance::activate_existing()? {
        return Ok(());
    }
    Err(windows::core::Error::new(
        windows::core::HRESULT(0x8000_4005u32 as i32),
        crate::i18n::existing_instance_unavailable_text(crate::i18n::detect_language()),
    ))
}

#[derive(Clone, Copy)]
pub enum StartupMode {
    Manual,
    Background,
}

impl StartupMode {
    pub fn new(autostart: bool, launch_at_startup: bool) -> Self {
        if autostart && launch_at_startup {
            Self::Background
        } else {
            Self::Manual
        }
    }

    pub fn window_style(self) -> WINDOW_STYLE {
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        match self {
            Self::Manual => style | WS_VISIBLE,
            Self::Background => style,
        }
    }

    // Called only after the tray icon and WindowState are ready.
    pub fn finish(self, hwnd: HWND) {
        unsafe {
            match self {
                Self::Manual => {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                }
                Self::Background => {
                    SendMessageW(hwnd, WM_PET_COMMAND, Some(WPARAM(PET_COMMAND_START)), None);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        Win32::{
            Foundation::{HINSTANCE, LPARAM, LRESULT},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetWindowLongPtrW, IsWindowVisible,
                RegisterClassW, SetWindowLongPtrW, WM_SHOWWINDOW, WNDCLASSW,
            },
        },
        core::w,
    };

    #[test]
    fn recognizes_only_the_complete_autostart_argument() {
        for (arguments, expected) in [
            (vec![], false),
            (vec!["--autostart"], true),
            (vec!["ignored", "--autostart"], true),
            (vec!["--autostart=false"], false),
            (vec!["--AUTOSTART"], false),
        ] {
            assert_eq!(
                has_autostart_argument(arguments.into_iter().map(OsString::from)),
                expected
            );
        }
    }

    #[derive(Default)]
    struct ObservedStartup {
        shown: usize,
        starts: usize,
    }

    unsafe extern "system" fn observe_startup(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ObservedStartup;
        if let Some(state) = unsafe { state.as_mut() } {
            if message == WM_SHOWWINDOW && wparam.0 != 0 {
                state.shown += 1;
            }
            if message == WM_PET_COMMAND && wparam.0 == PET_COMMAND_START {
                state.starts += 1;
            }
        }
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    #[test]
    fn flag_and_preference_control_visibility_and_timer_start() {
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(observe_startup),
                hInstance: instance,
                lpszClassName: w!("StandAwhileStartupModeTest"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            for autostart in [false, true] {
                for preference in [false, true] {
                    let background = autostart && preference;
                    let mode = StartupMode::new(autostart, preference);
                    let hwnd = CreateWindowExW(
                        Default::default(),
                        class.lpszClassName,
                        w!(""),
                        mode.window_style(),
                        -10000,
                        -10000,
                        100,
                        100,
                        None,
                        None,
                        Some(instance),
                        None,
                    )
                    .unwrap();
                    assert_eq!(IsWindowVisible(hwnd).as_bool(), !background, "visibility at creation");
                    let mut observed = ObservedStartup::default();
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut observed as *mut _ as isize);
                    mode.finish(hwnd);
                    assert_eq!(IsWindowVisible(hwnd).as_bool(), !background);
                    assert_eq!(observed.shown, 0, "background startup must never show the window");
                    assert_eq!(observed.starts, usize::from(background));
                    DestroyWindow(hwnd).unwrap();
                }
            }
        }
    }
}
