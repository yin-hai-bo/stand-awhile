use std::sync::atomic::{AtomicU32, Ordering};

use crate::{
    draw::{countdown_rect, draw_countdown, invalidate_countdown_font, release_countdown_font},
    theme::{paint_background, refresh_theme},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, GetDC, InvalidateRect, PAINTSTRUCT, ReleaseDC},
    UI::WindowsAndMessaging::{
        DefWindowProcW, KillTimer, PostQuitMessage, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WM_DESTROY,
        WM_DPICHANGED, WM_PAINT, WM_SETTINGCHANGE, WM_THEMECHANGED, WM_TIMER,
    },
};

pub const TIMER_ID: usize = 1;
const INITIAL_REMAINING_SECONDS: u32 = 20 * 60;

static REMAINING_SECONDS: AtomicU32 = AtomicU32::new(INITIAL_REMAINING_SECONDS);

pub unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
            let _ = draw_countdown(hwnd, hdc, REMAINING_SECONDS.load(Ordering::Relaxed));
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            };
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_ID {
                let previous_remaining = REMAINING_SECONDS.load(Ordering::Relaxed);
                REMAINING_SECONDS
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                        Some(value.saturating_sub(1))
                    })
                    .ok();
                let current_remaining = REMAINING_SECONDS.load(Ordering::Relaxed);
                unsafe {
                    invalidate_countdown(hwnd, previous_remaining);
                    if current_remaining != previous_remaining {
                        invalidate_countdown(hwnd, current_remaining);
                    }
                }
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DPICHANGED => {
            invalidate_countdown_font();

            let suggested_rect = unsafe { &*(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested_rect.left,
                    suggested_rect.top,
                    suggested_rect.right - suggested_rect.left,
                    suggested_rect.bottom - suggested_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let _ = InvalidateRect(Some(hwnd), None, false);
            }

            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            refresh_theme(hwnd);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                release_countdown_font();
                PostQuitMessage(0);
            };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn invalidate_countdown(hwnd: HWND, remaining_seconds: u32) {
    let hdc = unsafe { GetDC(Some(hwnd)) };
    if !hdc.is_invalid() {
        if let Ok(rect) = countdown_rect(hwnd, hdc, remaining_seconds) {
            let _ = unsafe { InvalidateRect(Some(hwnd), Some(&rect), false) };
        }
        let _ = unsafe { ReleaseDC(Some(hwnd), hdc) };
    }
}
