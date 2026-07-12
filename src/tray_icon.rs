use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT, WPARAM},
    UI::{
        Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW},
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

    pub fn handle_callback(&self, hwnd: HWND, lparam: LPARAM) -> Result<bool> {
        match lparam.0 as u32 {
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
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: icon,
        ..Default::default()
    };

    let wide_tooltip = wide_null(tooltip);
    let len = wide_tooltip
        .len()
        .saturating_sub(1)
        .min(data.szTip.len().saturating_sub(1));
    data.szTip[..len].copy_from_slice(&wide_tooltip[..len]);
    data
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
