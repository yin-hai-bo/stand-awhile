use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetClientRect, IDC_ARROW, LoadCursorW, RegisterClassW,
            SetWindowLongPtrW, WM_COMMAND, WM_ERASEBKGND, WM_NCCREATE, WM_PAINT, WNDCLASSW, WS_CHILD, WS_VISIBLE,
        },
    },
    core::{Error, Result, w},
};

use crate::settings::SETTINGS_BUTTON_RESERVED_HEIGHT;
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
