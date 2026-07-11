use std::sync::Mutex;

use crate::theme::current_text_color;
use windows::core::Error;

use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{
        ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DT_CALCRECT, DT_CENTER, DT_SINGLELINE,
        DT_VCENTER, DeleteObject, DrawTextW, FF_ROMAN, FW_NORMAL, GetDeviceCaps, HDC, LOGPIXELSY, OUT_DEFAULT_PRECIS,
        SelectObject, SetBkMode, SetTextColor, TRANSPARENT, VARIABLE_PITCH,
    },
    UI::WindowsAndMessaging::GetClientRect,
};

static COUNTDOWN_FONT: Mutex<Option<usize>> = Mutex::new(None);

pub fn draw_countdown(hwnd: HWND, hdc: HDC, remaining_seconds: u32) -> windows::core::Result<()> {
    let text = format_remaining_time(remaining_seconds);
    let mut wide_text: Vec<u16> = text.encode_utf16().collect();
    let mut draw_rect = get_countdown_rect(hwnd, hdc, &mut wide_text)?;
    let font = get_countdown_font(hdc)?;
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
    if let Some(raw_font) = cached_font.take() {
        unsafe {
            let _ = DeleteObject(windows::Win32::Graphics::Gdi::HFONT(raw_font as _).into());
        }
    }
}

pub fn invalidate_countdown_font() {
    release_countdown_font();
}

fn get_countdown_font(hdc: HDC) -> windows::core::Result<windows::Win32::Graphics::Gdi::HFONT> {
    let mut cached_font = COUNTDOWN_FONT.lock().expect("countdown font mutex poisoned");

    if let Some(raw_font) = *cached_font {
        return Ok(windows::Win32::Graphics::Gdi::HFONT(raw_font as _));
    }

    let font = create_countdown_font(hdc);
    if font.is_invalid() {
        return Err(Error::from_win32());
    }

    *cached_font = Some(font.0 as usize);
    Ok(font)
}

fn get_countdown_rect(hwnd: HWND, hdc: HDC, text: &mut [u16]) -> windows::core::Result<RECT> {
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client_rect)? };

    let font = get_countdown_font(hdc)?;
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
    let vertical_center = client_height * 45 / 100;
    let vertical_padding = (text_height / 3).max(12);
    let mut draw_rect = client_rect;

    draw_rect.top = vertical_center - text_height / 2 - vertical_padding;
    draw_rect.bottom = vertical_center + text_height / 2 + vertical_padding;

    Ok(draw_rect)
}

fn create_countdown_font(hdc: HDC) -> windows::Win32::Graphics::Gdi::HFONT {
    let dpi_y = unsafe { GetDeviceCaps(Some(hdc), LOGPIXELSY) };
    let font_height = -(80 * dpi_y / 72);

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
            ANTIALIASED_QUALITY,
            (VARIABLE_PITCH.0 | FF_ROMAN.0) as u32,
            windows::core::w!("Times New Roman"),
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
