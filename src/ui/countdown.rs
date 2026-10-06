use std::sync::Mutex;

use crate::ui::theme::current_text_color;
use windows::core::Error;

use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DT_CALCRECT, DT_CENTER, DT_SINGLELINE,
        DT_VCENTER, DeleteObject, DrawTextW, FF_SWISS, FW_NORMAL, HDC, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode,
        SetTextColor, TRANSPARENT, VARIABLE_PITCH,
    },
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::GetClientRect},
};

static COUNTDOWN_FONT: Mutex<Option<(u32, usize)>> = Mutex::new(None);

pub fn draw_countdown(hwnd: HWND, hdc: HDC, remaining_seconds: u32) -> windows::core::Result<()> {
    let text = format_remaining_time(remaining_seconds);
    let mut wide_text: Vec<u16> = text.encode_utf16().collect();
    let mut draw_rect = get_countdown_rect(hwnd, hdc, &mut wide_text)?;
    let font = get_countdown_font(unsafe { GetDpiForWindow(hwnd) }.max(96))?;
    let old_font = unsafe { SelectObject(hdc, font.into()) };

    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, current_text_color());
        let _ = DrawTextW(
            hdc,
            wide_text.as_mut_slice(),
            &mut draw_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        let _ = SelectObject(hdc, old_font);
    }

    Ok(())
}

pub fn countdown_rect(hwnd: HWND, hdc: HDC, remaining_seconds: u32) -> windows::core::Result<RECT> {
    let text = format_remaining_time(remaining_seconds);
    let mut wide_text: Vec<u16> = text.encode_utf16().collect();
    get_countdown_rect(hwnd, hdc, &mut wide_text)
}

pub fn release_countdown_font() {
    let mut cached_font = COUNTDOWN_FONT.lock().expect("countdown font mutex poisoned");
    if let Some((_, raw_font)) = cached_font.take() {
        unsafe {
            let _ = DeleteObject(windows::Win32::Graphics::Gdi::HFONT(raw_font as _).into());
        }
    }
}

pub fn invalidate_countdown_font() {
    release_countdown_font();
}

fn get_countdown_font(dpi: u32) -> windows::core::Result<windows::Win32::Graphics::Gdi::HFONT> {
    let mut cached_font = COUNTDOWN_FONT.lock().expect("countdown font mutex poisoned");

    if let Some((cached_dpi, raw_font)) = *cached_font {
        if cached_dpi == dpi {
            return Ok(windows::Win32::Graphics::Gdi::HFONT(raw_font as _));
        }
    }

    let font = create_countdown_font(dpi);
    if font.is_invalid() {
        return Err(Error::from_win32());
    }

    if let Some((_, raw_font)) = cached_font.replace((dpi, font.0 as usize)) {
        unsafe {
            let _ = DeleteObject(windows::Win32::Graphics::Gdi::HFONT(raw_font as _).into());
        }
    }
    Ok(font)
}

fn get_countdown_rect(hwnd: HWND, hdc: HDC, text: &mut [u16]) -> windows::core::Result<RECT> {
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client_rect)? };

    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let font = get_countdown_font(dpi)?;
    let old_font = unsafe { SelectObject(hdc, font.into()) };

    let mut measured_rect = client_rect;
    unsafe {
        let _ = DrawTextW(
            hdc,
            text,
            &mut measured_rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_CALCRECT,
        );
        let _ = SelectObject(hdc, old_font);
    }

    let text_height = measured_rect.bottom - measured_rect.top;
    let client_height = client_rect.bottom - client_rect.top;
    let vertical_center = client_height * 42 / 100;
    let vertical_padding = (text_height / 3).max(12 * dpi as i32 / 96);
    let mut draw_rect = client_rect;

    draw_rect.top = vertical_center - text_height / 2 - vertical_padding;
    draw_rect.bottom = vertical_center + text_height / 2 + vertical_padding;

    Ok(draw_rect)
}

fn create_countdown_font(dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    let font_height = -(80 * dpi as i32 / 72);

    unsafe {
        CreateFontW(
            font_height,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (VARIABLE_PITCH.0 | FF_SWISS.0) as u32,
            windows::core::w!("Segoe UI"),
        )
    }
}

fn format_remaining_time(total_seconds: u32) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours == 0 {
        format!("{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC, GetObjectW, LOGFONTW};

    #[test]
    fn countdown_font_and_text_follow_dpi_changes() {
        let hdc = unsafe { CreateCompatibleDC(None) };
        assert!(!hdc.is_invalid());
        for dpi in [96, 120, 144, 192, 96] {
            let font = get_countdown_font(dpi).unwrap();
            let mut description = LOGFONTW::default();
            assert_ne!(
                unsafe {
                    GetObjectW(
                        font.into(),
                        std::mem::size_of::<LOGFONTW>() as i32,
                        Some((&mut description as *mut LOGFONTW).cast()),
                    )
                },
                0
            );
            assert_eq!(description.lfHeight, -(80 * dpi as i32 / 72));
            assert_eq!(get_countdown_font(dpi).unwrap(), font);
            let old_font = unsafe { SelectObject(hdc, font.into()) };
            for seconds in [65, 3661] {
                let mut text: Vec<u16> = format_remaining_time(seconds).encode_utf16().collect();
                let mut rect = RECT::default();
                unsafe {
                    DrawTextW(hdc, &mut text, &mut rect, DT_SINGLELINE | DT_CALCRECT);
                }
                assert!(rect.right > 0);
                assert!(rect.bottom >= -description.lfHeight);
            }
            unsafe {
                SelectObject(hdc, old_font);
            }
        }
        release_countdown_font();
        assert!(COUNTDOWN_FONT.lock().unwrap().is_none());
        unsafe {
            let _ = DeleteDC(hdc);
        }
    }

    #[test]
    fn formats_minutes_and_seconds() {
        assert_eq!(format_remaining_time(65), "01:05");
    }

    #[test]
    fn formats_hours_when_needed() {
        assert_eq!(format_remaining_time(3661), "01:01:01");
    }
}
