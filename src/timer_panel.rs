use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DrawTextW, EndPaint, HDC, PAINTSTRUCT, SelectObject,
            SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetClientRect, IDC_ARROW, LoadCursorW, RegisterClassW,
            SetWindowLongPtrW, WM_COMMAND, WM_ERASEBKGND, WM_NCCREATE, WM_PAINT, WNDCLASSW, WS_CHILD,
            WS_EX_CONTROLPARENT, WS_VISIBLE,
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
            WS_EX_CONTROLPARENT,
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use windows::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            DestroyWindow, GetCursor, HTCLIENT, IDC_ARROW, IDC_HAND, IDC_WAIT, LoadCursorW, SendMessageW, SetCursor,
            WM_MOUSEMOVE, WM_SETCURSOR,
        },
    };

    static LAST_COMMAND: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "system" fn parent_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == WM_COMMAND {
            LAST_COMMAND.store(wparam.0 & 0xFFFF, Ordering::Relaxed);
        }
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
    fn main_controls_support_tab_keyboard_and_arrow_cursor() {
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
            {
                use crate::ui::button::{
                    ControlButton, button_from_command, create_control_buttons, register_button_class,
                    update_control_buttons_for,
                };
                use crate::window_proc::{create_auto_hide_checkbox, process_dialog_message, set_main_tab_order};
                use windows::Win32::UI::{
                    Input::KeyboardAndMouse::{GetFocus, SetFocus, VK_ESCAPE, VK_RETURN, VK_SPACE, VK_TAB},
                    WindowsAndMessaging::{
                        DispatchMessageW, GetNextDlgTabItem, HMENU, IDCANCEL, IDOK, MSG, PM_REMOVE, PeekMessageW,
                        WM_KEYDOWN, WM_KEYUP, WS_TABSTOP,
                    },
                };
                register_button_class(instance).unwrap();
                let buttons = create_control_buttons(panel, instance).unwrap();
                let settings = crate::settings::create_settings_button(parent, instance, "Settings", None);
                let about = crate::settings::create_about_button(parent, instance, "About", None);
                let checkbox = create_auto_hide_checkbox(parent, instance, Language::English, None, true).unwrap();
                set_main_tab_order(panel, checkbox, about, settings);
                for enabled in [false, true] {
                    update_control_buttons_for(&buttons, true, enabled, enabled);
                    SetFocus(Some(parent)).unwrap();
                    let mut expected = vec![buttons[0]];
                    if enabled {
                        expected.extend([buttons[1], buttons[2]]);
                    }
                    expected.extend([checkbox, about, settings, buttons[0]]);
                    for target in expected {
                        let message = MSG {
                            hwnd: GetFocus(),
                            message: WM_KEYDOWN,
                            wParam: WPARAM(VK_TAB.0 as usize),
                            ..Default::default()
                        };
                        assert!(process_dialog_message(parent, &message));
                        assert_eq!(GetFocus(), target);
                    }
                    assert_eq!(GetNextDlgTabItem(parent, Some(buttons[0]), true).unwrap(), settings);
                }
                for (index, kind) in [ControlButton::Play, ControlButton::Pause, ControlButton::Reset]
                    .into_iter()
                    .enumerate()
                {
                    for key in [VK_SPACE, VK_RETURN] {
                        SetFocus(Some(buttons[index])).unwrap();
                        for message_id in [WM_KEYDOWN, WM_KEYDOWN, WM_KEYUP] {
                            let message = MSG {
                                hwnd: buttons[index],
                                message: message_id,
                                wParam: WPARAM(key.0 as usize),
                                ..Default::default()
                            };
                            if !process_dialog_message(parent, &message) {
                                DispatchMessageW(&message);
                            }
                        }
                        let mut command = MSG::default();
                        assert!(PeekMessageW(&mut command, Some(panel), WM_COMMAND, WM_COMMAND, PM_REMOVE).as_bool());
                        DispatchMessageW(&command);
                        assert!(PeekMessageW(&mut command, Some(parent), WM_COMMAND, WM_COMMAND, PM_REMOVE).as_bool());
                        assert_eq!(button_from_command(command.wParam), Some(kind));
                        assert!(!PeekMessageW(&mut command, Some(panel), WM_COMMAND, WM_COMMAND, PM_REMOVE).as_bool());
                    }
                }
                SetFocus(Some(buttons[0])).unwrap();
                SendMessageW(buttons[0], WM_KEYDOWN, Some(WPARAM(VK_SPACE.0 as usize)), None);
                SetFocus(Some(checkbox)).unwrap();
                SendMessageW(buttons[0], WM_KEYUP, Some(WPARAM(VK_SPACE.0 as usize)), None);
                update_control_buttons_for(&buttons, true, false, false);
                SendMessageW(buttons[1], WM_KEYDOWN, Some(WPARAM(VK_RETURN.0 as usize)), None);
                SendMessageW(buttons[1], WM_KEYUP, Some(WPARAM(VK_RETURN.0 as usize)), None);
                let mut command = MSG::default();
                assert!(!PeekMessageW(&mut command, Some(panel), WM_COMMAND, WM_COMMAND, PM_REMOVE).as_bool());

                let edit = CreateWindowExW(
                    Default::default(),
                    w!("EDIT"),
                    w!("20"),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    0,
                    0,
                    100,
                    24,
                    Some(parent),
                    Some(HMENU(10usize as _)),
                    Some(instance),
                    None,
                )
                .unwrap();
                for focus in [parent, edit] {
                    SetFocus(Some(focus)).unwrap();
                    for (key, id) in [(VK_ESCAPE, IDCANCEL), (VK_RETURN, IDOK)] {
                        LAST_COMMAND.store(0, Ordering::Relaxed);
                        let message = MSG {
                            hwnd: focus,
                            message: WM_KEYDOWN,
                            wParam: WPARAM(key.0 as usize),
                            ..Default::default()
                        };
                        assert!(process_dialog_message(parent, &message));
                        let command = LAST_COMMAND.load(Ordering::Relaxed);
                        assert_eq!(command, id.0 as usize);
                        assert_ne!(command, crate::settings::SETTINGS_BUTTON_ID);
                        assert_ne!(command, crate::settings::ABOUT_BUTTON_ID);
                    }
                }
                for (button, id) in [
                    (about, crate::settings::ABOUT_BUTTON_ID),
                    (settings, crate::settings::SETTINGS_BUTTON_ID),
                ] {
                    SetFocus(Some(button)).unwrap();
                    LAST_COMMAND.store(0, Ordering::Relaxed);
                    for message_id in [WM_KEYDOWN, WM_KEYUP] {
                        let message = MSG {
                            hwnd: button,
                            message: message_id,
                            wParam: WPARAM(VK_SPACE.0 as usize),
                            ..Default::default()
                        };
                        if !process_dialog_message(parent, &message) {
                            DispatchMessageW(&message);
                        }
                    }
                    assert_eq!(LAST_COMMAND.load(Ordering::Relaxed), id);
                }
            }
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
