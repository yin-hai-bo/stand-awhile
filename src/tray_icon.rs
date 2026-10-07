use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT, WPARAM},
    UI::{
        Shell::{
            NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
            NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, MF_SEPARATOR, MF_STRING,
            RegisterWindowMessageW, SW_RESTORE, SW_SHOW, SetForegroundWindow, ShowWindow, TPM_BOTTOMALIGN,
            TPM_LEFTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONDBLCLK,
            WM_RBUTTONUP,
        },
    },
};
use windows::core::{Error, PCWSTR, Result, w};

pub const WM_TRAYICON: u32 = WM_APP + 1;
pub const TRAY_ICON_ID: u32 = 1;
pub const TRAY_MENU_SHOW_ID: usize = 41001;
pub const TRAY_MENU_ABOUT_ID: usize = 41002;
pub const TRAY_MENU_EXIT_ID: usize = 41003;
pub const TRAY_MENU_START_ID: usize = 41004;
pub const TRAY_MENU_SETTINGS_ID: usize = 41005;

pub struct TrayIcon {
    taskbar_created_message: u32,
    icon: HICON,
    tooltip: String,
    start_menu_text: String,
    show_menu_text: String,
    settings_text: String,
    about_text: String,
    exit_menu_text: String,
}

impl TrayIcon {
    #[cfg(test)]
    pub(crate) fn test_tooltip(&self) -> &str {
        &self.tooltip
    }

    pub fn create(
        hwnd: HWND,
        icon: HICON,
        tooltip: &str,
        start_menu_text: &str,
        show_menu_text: &str,
        settings_text: &str,
        about_text: &str,
        exit_menu_text: &str,
    ) -> Result<Self> {
        let taskbar_created_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        if taskbar_created_message == 0 {
            return Err(Error::from_win32());
        }

        let tray = Self {
            taskbar_created_message,
            icon,
            tooltip: tooltip.to_owned(),
            start_menu_text: start_menu_text.to_owned(),
            show_menu_text: show_menu_text.to_owned(),
            settings_text: settings_text.to_owned(),
            about_text: about_text.to_owned(),
            exit_menu_text: exit_menu_text.to_owned(),
        };
        tray.add(hwnd, notify_icon)?;
        Ok(tray)
    }

    pub fn handle_taskbar_created(&self, hwnd: HWND, message: u32) -> Result<bool> {
        self.handle_taskbar_created_with(hwnd, message, notify_icon)
    }

    fn handle_taskbar_created_with(
        &self,
        hwnd: HWND,
        message: u32,
        notify: impl FnMut(NOTIFY_ICON_MESSAGE, &NOTIFYICONDATAW) -> Result<()>,
    ) -> Result<bool> {
        if message != self.taskbar_created_message {
            return Ok(false);
        }
        self.add(hwnd, notify)?;
        Ok(true)
    }

    fn add(
        &self,
        hwnd: HWND,
        mut notify: impl FnMut(NOTIFY_ICON_MESSAGE, &NOTIFYICONDATAW) -> Result<()>,
    ) -> Result<()> {
        let mut data = notify_icon_data(hwnd, self.icon, &self.tooltip);
        notify(NIM_ADD, &data)?;
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if let Err(error) = notify(NIM_SETVERSION, &data) {
            let _ = notify(NIM_DELETE, &data);
            return Err(error);
        }
        Ok(())
    }

    pub fn delete(&self, hwnd: HWND) {
        let icon_data = notify_icon_data(hwnd, self.icon, &self.tooltip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data);
        }
    }

    pub fn update_language(
        &mut self,
        hwnd: HWND,
        tooltip: &str,
        start_menu_text: &str,
        show_menu_text: &str,
        settings_text: &str,
        about_text: &str,
        exit_menu_text: &str,
    ) -> Result<()> {
        self.tooltip = tooltip.to_owned();
        self.start_menu_text = start_menu_text.to_owned();
        self.show_menu_text = show_menu_text.to_owned();
        self.settings_text = settings_text.to_owned();
        self.about_text = about_text.to_owned();
        self.exit_menu_text = exit_menu_text.to_owned();
        let icon_data = notify_icon_data(hwnd, self.icon, &self.tooltip);
        unsafe {
            if !Shell_NotifyIconW(NIM_MODIFY, &icon_data).as_bool() {
                return Err(Error::from_win32());
            }
        }
        Ok(())
    }

    pub fn update_tooltip(&mut self, hwnd: HWND, tooltip: &str) -> Result<()> {
        self.update_tooltip_with(hwnd, tooltip, notify_icon)
    }

    fn update_tooltip_with(
        &mut self,
        hwnd: HWND,
        tooltip: &str,
        mut notify: impl FnMut(NOTIFY_ICON_MESSAGE, &NOTIFYICONDATAW) -> Result<()>,
    ) -> Result<()> {
        if self.tooltip == tooltip {
            return Ok(());
        }
        self.tooltip = tooltip.to_owned();
        notify(NIM_MODIFY, &notify_icon_data(hwnd, self.icon, &self.tooltip))
    }

    pub fn handle_callback(&self, hwnd: HWND, lparam: LPARAM) -> Result<bool> {
        match loword(lparam.0 as u32) as u32 {
            WM_LBUTTONDBLCLK => {
                show_main_window(hwnd);
                Ok(true)
            }
            WM_RBUTTONUP | WM_CONTEXTMENU => {
                self.show_context_menu(hwnd)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn handle_command(&self, hwnd: HWND, wparam: WPARAM) -> bool {
        match (wparam.0 & 0xFFFF) as usize {
            TRAY_MENU_SHOW_ID => {
                show_main_window(hwnd);
                true
            }
            TRAY_MENU_EXIT_ID => {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
                }
                true
            }
            _ => false,
        }
    }

    fn show_context_menu(&self, hwnd: HWND) -> Result<()> {
        let menu = unsafe { CreatePopupMenu()? };
        let show_text = wide_null(&self.show_menu_text);
        let settings_text = wide_null(&self.settings_text);
        let about_text = wide_null(&self.about_text);
        let exit_text = wide_null(&self.exit_menu_text);

        unsafe {
            AppendMenuW(
                menu,
                MF_STRING,
                TRAY_MENU_START_ID,
                PCWSTR(wide_null(&self.start_menu_text).as_ptr()),
            )?;
            AppendMenuW(menu, MF_STRING, TRAY_MENU_SHOW_ID, PCWSTR(show_text.as_ptr()))?;
            AppendMenuW(menu, MF_STRING, TRAY_MENU_SETTINGS_ID, PCWSTR(settings_text.as_ptr()))?;
            AppendMenuW(menu, MF_STRING, TRAY_MENU_ABOUT_ID, PCWSTR(about_text.as_ptr()))?;
            AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
            AppendMenuW(menu, MF_STRING, TRAY_MENU_EXIT_ID, PCWSTR(exit_text.as_ptr()))?;
        }

        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point)?;
            let _ = SetForegroundWindow(hwnd);
            let _ = TrackPopupMenu(
                menu,
                TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                Some(0),
                hwnd,
                None,
            );
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
        }

        Ok(())
    }
}

fn notify_icon_data(hwnd: HWND, icon: HICON, tooltip: &str) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: icon,
        ..Default::default()
    };

    copy_wide_text(tooltip, &mut data.szTip);
    data
}

fn notify_icon(message: NOTIFY_ICON_MESSAGE, data: &NOTIFYICONDATAW) -> Result<()> {
    unsafe {
        if !Shell_NotifyIconW(message, data).as_bool() {
            return Err(Error::from_win32());
        }
    }

    Ok(())
}

pub(crate) fn show_main_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn copy_wide_text<const N: usize>(value: &str, target: &mut [u16; N]) {
    let wide_text = wide_null(value);
    let len = wide_text.len().saturating_sub(1).min(target.len().saturating_sub(1));
    target[..len].copy_from_slice(&wide_text[..len]);
    target[len] = 0;
}

fn loword(value: u32) -> u16 {
    (value & 0xFFFF) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tray() -> TrayIcon {
        TrayIcon {
            taskbar_created_message: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
            icon: HICON(123usize as _),
            tooltip: "站一站".into(),
            start_menu_text: "开始计时".into(),
            show_menu_text: "显示主窗口".into(),
            settings_text: "设置".into(),
            about_text: "关于".into(),
            exit_menu_text: "退出".into(),
        }
    }

    #[test]
    fn tooltip_changes_notify_once_and_survive_explorer_restart() {
        let mut tray = tray();
        let hwnd = HWND(456usize as _);
        for tooltip in ["站一站（计时中）", "站一站", "Stand Awhile (Timing)"] {
            let mut calls = 0;
            for _ in 0..2 {
                tray.update_tooltip_with(hwnd, tooltip, |command, data| {
                    calls += 1;
                    assert_eq!(command, NIM_MODIFY);
                    assert_eq!(data.hWnd, hwnd);
                    let length = data.szTip.iter().position(|c| *c == 0).unwrap();
                    assert_eq!(String::from_utf16(&data.szTip[..length]).unwrap(), tooltip);
                    Ok(())
                })
                .unwrap();
            }
            assert_eq!(calls, 1, "unchanged countdown ticks must not notify the Shell again");
            tray.handle_taskbar_created_with(hwnd, tray.taskbar_created_message, |_, data| {
                let length = data.szTip.iter().position(|c| *c == 0).unwrap();
                assert_eq!(String::from_utf16(&data.szTip[..length]).unwrap(), tooltip);
                Ok(())
            })
            .unwrap();
        }
    }

    #[test]
    fn taskbar_created_readds_the_current_icon_and_restores_callback_version() {
        let mut tray = tray();
        assert_ne!(tray.taskbar_created_message, 0);
        // Recovery uses the latest language, rather than the startup tooltip.
        tray.tooltip = "Stand Awhile".into();
        let hwnd = HWND(456usize as _);
        for _ in 0..2 {
            let mut calls = Vec::new();
            assert!(
                tray.handle_taskbar_created_with(hwnd, tray.taskbar_created_message, |command, data| {
                    calls.push(command);
                    assert_eq!(data.hWnd, hwnd);
                    assert_eq!(data.uID, TRAY_ICON_ID);
                    assert_eq!(data.hIcon, tray.icon);
                    assert_eq!(data.uCallbackMessage, WM_TRAYICON);
                    assert_eq!(data.uFlags, NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP);
                    let length = data.szTip.iter().position(|c| *c == 0).unwrap();
                    assert_eq!(String::from_utf16(&data.szTip[..length]).unwrap(), "Stand Awhile");
                    if command == NIM_SETVERSION {
                        assert_eq!(unsafe { data.Anonymous.uVersion }, NOTIFYICON_VERSION_4);
                    }
                    Ok(())
                })
                .unwrap()
            );
            assert_eq!(calls, [NIM_ADD, NIM_SETVERSION]);
        }
    }

    #[test]
    fn unrelated_messages_do_not_recreate_the_icon() {
        let tray = tray();
        assert!(
            !tray
                .handle_taskbar_created_with(HWND::default(), WM_TRAYICON, |_, _| {
                    panic!("unrelated message must not touch the Shell icon");
                })
                .unwrap()
        );
    }

    #[test]
    fn failed_recovery_stops_after_add_and_cleans_up_if_version_fails() {
        let tray = tray();
        for failed_command in [NIM_ADD, NIM_SETVERSION] {
            let mut calls = Vec::new();
            let error =
                tray.handle_taskbar_created_with(HWND::default(), tray.taskbar_created_message, |command, _| {
                    calls.push(command);
                    if command == failed_command {
                        Err(Error::from_hresult(windows::core::HRESULT(0x8000_4005u32 as i32)))
                    } else {
                        Ok(())
                    }
                });
            assert!(error.is_err());
            assert_eq!(
                calls,
                if failed_command == NIM_ADD {
                    vec![NIM_ADD]
                } else {
                    vec![NIM_ADD, NIM_SETVERSION, NIM_DELETE]
                }
            );
        }
    }
}
