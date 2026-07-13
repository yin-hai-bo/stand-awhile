use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT, WPARAM},
    UI::{
        Shell::{
            NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY,
            NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HICON, MF_STRING, SW_RESTORE, SW_SHOW,
            SetForegroundWindow, ShowWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP,
            WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
        },
    },
};
use windows::core::{Error, PCWSTR, Result};

pub const WM_TRAYICON: u32 = WM_APP + 1;
pub const TRAY_ICON_ID: u32 = 1;
pub const TRAY_MENU_SHOW_ID: usize = 41001;
pub const TRAY_MENU_OPEN_CONFIG_ID: usize = 41002;
pub const TRAY_MENU_ABOUT_ID: usize = 41003;
pub const TRAY_MENU_EXIT_ID: usize = 41004;

pub struct TrayIcon {
    tooltip: String,
    show_menu_text: String,
    open_config_text: String,
    about_text: String,
    exit_menu_text: String,
}

impl TrayIcon {
    pub fn create(
        hwnd: HWND,
        icon: HICON,
        tooltip: &str,
        show_menu_text: &str,
        open_config_text: &str,
        about_text: &str,
        exit_menu_text: &str,
    ) -> Result<Self> {
        let icon_data = notify_icon_data(hwnd, icon, tooltip);
        unsafe {
            if !Shell_NotifyIconW(NIM_ADD, &icon_data).as_bool() {
                return Err(Error::from_win32());
            }
        }
        set_notify_icon_version(hwnd, tooltip)?;

        Ok(Self {
            tooltip: tooltip.to_owned(),
            show_menu_text: show_menu_text.to_owned(),
            open_config_text: open_config_text.to_owned(),
            about_text: about_text.to_owned(),
            exit_menu_text: exit_menu_text.to_owned(),
        })
    }

    pub fn delete(&self, hwnd: HWND) {
        let icon_data = notify_icon_data(hwnd, HICON::default(), &self.tooltip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data);
        }
    }

    pub fn show_notification(&self, hwnd: HWND, title: &str, message: &str) -> Result<()> {
        let mut icon_data = notify_icon_data(hwnd, HICON::default(), &self.tooltip);
        icon_data.uFlags = NIF_INFO;
        icon_data.dwInfoFlags = NIIF_INFO;
        copy_wide_text(title, &mut icon_data.szInfoTitle);
        copy_wide_text(message, &mut icon_data.szInfo);

        unsafe {
            if !Shell_NotifyIconW(NIM_MODIFY, &icon_data).as_bool() {
                return Err(Error::from_win32());
            }
        }

        Ok(())
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
        let open_config_text = wide_null(&self.open_config_text);
        let about_text = wide_null(&self.about_text);
        let exit_text = wide_null(&self.exit_menu_text);

        unsafe {
            AppendMenuW(menu, MF_STRING, TRAY_MENU_SHOW_ID, PCWSTR(show_text.as_ptr()))?;
            AppendMenuW(
                menu,
                MF_STRING,
                TRAY_MENU_OPEN_CONFIG_ID,
                PCWSTR(open_config_text.as_ptr()),
            )?;
            AppendMenuW(menu, MF_STRING, TRAY_MENU_ABOUT_ID, PCWSTR(about_text.as_ptr()))?;
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

fn set_notify_icon_version(hwnd: HWND, tooltip: &str) -> Result<()> {
    let mut icon_data = notify_icon_data(hwnd, HICON::default(), tooltip);
    icon_data.Anonymous.uVersion = NOTIFYICON_VERSION_4;

    unsafe {
        if !Shell_NotifyIconW(NIM_SETVERSION, &icon_data).as_bool() {
            return Err(Error::from_win32());
        }
    }

    Ok(())
}

fn show_main_window(hwnd: HWND) {
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
