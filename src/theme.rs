use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::{
    Foundation::{COLORREF, HWND, NO_ERROR, RECT},
    Graphics::{
        Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
        Gdi::{CreateSolidBrush, DeleteObject, FillRect, InvalidateRect},
    },
    System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
};
use windows::core::{Error, HRESULT, Result, w};

static IS_DARK_MODE: AtomicBool = AtomicBool::new(false);

const DARK_BACKGROUND: COLORREF = COLORREF(0x202020);
const LIGHT_BACKGROUND: COLORREF = COLORREF(0xF0F0F0);
const DARK_TEXT: COLORREF = COLORREF(0xF0F0F0);
const LIGHT_TEXT: COLORREF = COLORREF(0x202020);

pub fn is_dark_mode() -> Result<bool> {
    let mut value = 1u32;
    let mut size = std::mem::size_of::<u32>() as u32;

    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if status != NO_ERROR {
        return Err(Error::from_hresult(HRESULT::from_win32(status.0)));
    }

    Ok(value == 0)
}

pub fn apply_theme(hwnd: HWND) -> Result<()> {
    let is_dark = is_dark_mode()?;
    IS_DARK_MODE.store(is_dark, Ordering::Relaxed);

    let dark_flag = if is_dark { 1i32 } else { 0i32 };
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_flag as *const i32 as _,
            std::mem::size_of::<i32>() as u32,
        )?;
        let _ = InvalidateRect(Some(hwnd), None, true);
    }

    Ok(())
}

pub fn paint_background(rect: &RECT, hdc: windows::Win32::Graphics::Gdi::HDC) -> Result<()> {
    let color = if IS_DARK_MODE.load(Ordering::Relaxed) {
        DARK_BACKGROUND
    } else {
        LIGHT_BACKGROUND
    };

    unsafe {
        let brush = CreateSolidBrush(color);
        if brush.is_invalid() {
            return Err(Error::from_win32());
        }

        let _ = FillRect(hdc, rect, brush);
        let _ = DeleteObject(brush.into());
    }

    Ok(())
}

pub fn refresh_theme(hwnd: HWND) {
    let _ = apply_theme(hwnd);
}

pub fn current_text_color() -> COLORREF {
    if IS_DARK_MODE.load(Ordering::Relaxed) {
        DARK_TEXT
    } else {
        LIGHT_TEXT
    }
}
