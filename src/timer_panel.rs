use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DrawTextW, EndPaint, HDC, PAINTSTRUCT, SelectObject,
            SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetClientRect, IDC_ARROW, LoadCursorW, RegisterClassW,
            SetWindowLongPtrW, WM_COMMAND, WM_ERASEBKGND, WM_NCCREATE, WM_PAINT, WNDCLASSW, WS_CHILD, WS_VISIBLE,
        },
    },
    core::{Error, Result, w},
};

use crate::i18n::{Language, timer_hint_text};
use crate::settings::SETTINGS_BUTTON_RESERVED_HEIGHT;
use crate::ui::{button::controls_rect, font::common_gui_font, theme::is_dark_theme_active};
use crate::ui::{draw_countdown, theme::paint_background};

const TIMER_PANEL_CLASS: windows::core::PCWSTR = w!("YHB-StandAwhileTimerPanel");

pub fn register_timer_panel_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(timer_panel_proc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        lpszClassName: TIMER_PANEL_CLASS,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(Error::from_win32());
    }
    Ok(())
}

pub fn create_timer_panel(parent: HWND, instance: HINSTANCE) -> Result<HWND> {
    unsafe {
        CreateWindowExW(
            Default::default(),
            TIMER_PANEL_CLASS,
            w!(""),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance),
            None,
        )
    }
}

pub fn resize_timer_panel(hwnd: HWND, parent: HWND) -> Result<()> {
    let mut rect = RECT::default();
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(parent) }.max(96);
    let reserved_height = SETTINGS_BUTTON_RESERVED_HEIGHT * dpi as i32 / 96;
    unsafe {
        GetClientRect(parent, &mut rect)?;
        windows::Win32::UI::WindowsAndMessaging::MoveWindow(
            hwnd,
            0,
            0,
            rect.right,
            (rect.bottom - reserved_height).max(0),
            true,
        )?;
    }
    Ok(())
}

unsafe extern "system" fn timer_panel_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam.0) };
            LRESULT(1)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
            let _ = draw_countdown(hwnd, hdc, crate::window_proc::remaining_seconds());
            let _ = draw_timer_hint(hwnd, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_COMMAND => {
            if let Ok(parent) = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) } {
                unsafe {
                    let _ =
                        windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(parent), WM_COMMAND, wparam, lparam);
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn draw_timer_hint(hwnd: HWND, hdc: HDC) -> Result<()> {
    let parent = unsafe { windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd)? };
    let Some(state) = crate::window_proc::window_state(parent) else {
        return Ok(());
    };
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) }.max(96);
    let Some(font) = common_gui_font(dpi, state.language == Language::Chinese) else {
        return Ok(());
    };
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect)? };
    rect.left += 16 * dpi as i32 / 96;
    rect.right -= 16 * dpi as i32 / 96;
    rect.top = controls_rect(hwnd)?.bottom + 16 * dpi as i32 / 96;
    rect.bottom = rect.top + 32 * dpi as i32 / 96;
    let mut text: Vec<u16> = timer_hint_text(state.language).encode_utf16().collect();
    let shade = if is_dark_theme_active() { 170 } else { 100 };
    unsafe {
        let old_font = SelectObject(hdc, font.into());
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, windows::Win32::Foundation::COLORREF(shade * 0x010101));
        let _ = DrawTextW(hdc, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        let _ = SelectObject(hdc, old_font);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            DestroyWindow, GetCursor, HTCLIENT, IDC_ARROW, IDC_HAND, IDC_WAIT, LoadCursorW, SendMessageW, SetCursor,
            WM_MOUSEMOVE, WM_SETCURSOR,
        },
    };

    unsafe extern "system" fn parent_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    #[test]
    fn localized_hint_fits_on_one_line_at_supported_dpis() {
        use windows::Win32::Graphics::Gdi::{DT_CALCRECT, GetDC, ReleaseDC};
        let hdc = unsafe { GetDC(None) };
        assert!(!hdc.is_invalid());
        for dpi in [96, 120, 144, 192] {
            for language in [Language::Chinese, Language::English] {
                let font = common_gui_font(dpi, language == Language::Chinese).unwrap();
                let mut text: Vec<u16> = timer_hint_text(language).encode_utf16().collect();
                let mut measured = RECT::default();
                unsafe {
                    let old = SelectObject(hdc, font.into());
                    DrawTextW(hdc, &mut text, &mut measured, DT_CALCRECT | DT_SINGLELINE);
                    SelectObject(hdc, old);
                }
                assert!(measured.right > 0);
                assert!(measured.right <= (crate::WINDOW_WIDTH - 64) * dpi as i32 / 96);
                assert!(measured.bottom <= 32 * dpi as i32 / 96);
                let mut checkbox_text: Vec<u16> = crate::i18n::auto_hide_text(language).encode_utf16().collect();
                let mut measured = RECT::default();
                unsafe {
                    let old = SelectObject(hdc, font.into());
                    DrawTextW(hdc, &mut checkbox_text, &mut measured, DT_CALCRECT | DT_SINGLELINE);
                    SelectObject(hdc, old);
                }
                // Reserve the window borders, footer buttons, margins, and checkbox glyph.
                assert!(measured.right <= (crate::WINDOW_WIDTH - 16 - 24 - 96 - 24 - 96 - 48 - 24) * dpi as i32 / 96);
                assert!(measured.bottom <= 32 * dpi as i32 / 96);
            }
        }
        unsafe { ReleaseDC(None, hdc) };
    }

    #[test]
    fn timer_panel_replaces_stale_cursor_with_arrow() {
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let parent_class = w!("StandAwhileCursorTestParent");
            let arrow = LoadCursorW(None, IDC_ARROW).unwrap();
            let class = WNDCLASSW {
                lpfnWndProc: Some(parent_proc),
                hInstance: instance,
                hCursor: arrow,
                lpszClassName: parent_class,
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            register_timer_panel_class(instance).unwrap();
            let parent = CreateWindowExW(
                Default::default(),
                parent_class,
                w!(""),
                Default::default(),
                0,
                0,
                100,
                100,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let panel = create_timer_panel(parent, instance).unwrap();
            let previous_cursor = GetCursor();
            let mut actual_cursors = Vec::new();
            for initial_cursor in [IDC_WAIT, IDC_HAND] {
                SetCursor(Some(LoadCursorW(None, initial_cursor).unwrap()));
                SendMessageW(
                    panel,
                    WM_SETCURSOR,
                    Some(WPARAM(panel.0 as usize)),
                    Some(LPARAM(HTCLIENT as isize | ((WM_MOUSEMOVE as isize) << 16))),
                );
                actual_cursors.push(GetCursor());
            }
            SetCursor(Some(previous_cursor));
            DestroyWindow(parent).unwrap();
            assert_eq!(actual_cursors, vec![arrow, arrow]);
        }
    }
}
