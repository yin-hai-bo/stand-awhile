use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use crate::ui::{
    button::{
        ControlButton, button_from_command, layout_control_buttons, refresh_control_buttons, update_control_buttons,
    },
    countdown_rect, draw_countdown, invalidate_countdown_font, release_countdown_font,
    theme::{paint_background, refresh_theme},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, GetDC, InvalidateRect, PAINTSTRUCT, ReleaseDC},
    UI::WindowsAndMessaging::{
        DefWindowProcW, KillTimer, PostQuitMessage, SWP_NOACTIVATE, SWP_NOZORDER, SetTimer, SetWindowPos, WM_COMMAND,
        WM_DESTROY, WM_DPICHANGED, WM_PAINT, WM_SETTINGCHANGE, WM_THEMECHANGED, WM_TIMER,
    },
};

pub const TIMER_ID: usize = 1;
const INITIAL_REMAINING_SECONDS: u32 = 20 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerState {
    NotStarted,
    Running,
    Paused,
    Finished,
}

static REMAINING_SECONDS: AtomicU32 = AtomicU32::new(INITIAL_REMAINING_SECONDS);
static TIMER_STATE: Mutex<TimerState> = Mutex::new(TimerState::NotStarted);

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
                if *TIMER_STATE.lock().expect("timer state mutex poisoned") != TimerState::Running {
                    return LRESULT(0);
                }

                let previous_remaining = REMAINING_SECONDS.load(Ordering::Relaxed);
                REMAINING_SECONDS
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                        Some(value.saturating_sub(1))
                    })
                    .ok();
                let current_remaining = REMAINING_SECONDS.load(Ordering::Relaxed);
                if current_remaining == 0 {
                    *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::Finished;
                    stop_timer(hwnd);
                }

                let _ = sync_control_button_enabled(hwnd);

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
        WM_COMMAND => {
            if let Some(button) = button_from_command(wparam) {
                activate_button(hwnd, button);
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

            let _ = layout_control_buttons(hwnd);
            let _ = refresh_control_buttons(hwnd);

            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            refresh_theme(hwnd);
            let _ = refresh_control_buttons(hwnd);
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

fn activate_button(hwnd: HWND, button: ControlButton) {
    let previous_remaining = REMAINING_SECONDS.load(Ordering::Relaxed);

    match button {
        ControlButton::Play => {
            if previous_remaining == 0 {
                REMAINING_SECONDS.store(INITIAL_REMAINING_SECONDS, Ordering::Relaxed);
                unsafe {
                    invalidate_countdown(hwnd, previous_remaining);
                    invalidate_countdown(hwnd, INITIAL_REMAINING_SECONDS);
                }
            }
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::Running;
            start_timer(hwnd);
        }
        ControlButton::Pause => {
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::Paused;
            stop_timer(hwnd);
        }
        ControlButton::Reset => {
            let previous_remaining = REMAINING_SECONDS.swap(INITIAL_REMAINING_SECONDS, Ordering::Relaxed);
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::NotStarted;
            stop_timer(hwnd);
            unsafe {
                invalidate_countdown(hwnd, previous_remaining);
                invalidate_countdown(hwnd, INITIAL_REMAINING_SECONDS);
            }
        }
    }

    let _ = sync_control_button_enabled(hwnd);
}

fn sync_control_button_enabled(hwnd: HWND) -> windows::core::Result<()> {
    let timer_state = *TIMER_STATE.lock().expect("timer state mutex poisoned");
    let remaining = REMAINING_SECONDS.load(Ordering::Relaxed);

    update_control_buttons(
        hwnd,
        play_enabled(timer_state),
        pause_enabled(timer_state),
        reset_enabled(remaining),
    )
}

fn play_enabled(timer_state: TimerState) -> bool {
    matches!(
        timer_state,
        TimerState::NotStarted | TimerState::Paused | TimerState::Finished
    )
}

fn pause_enabled(timer_state: TimerState) -> bool {
    matches!(timer_state, TimerState::Running)
}

fn reset_enabled(remaining_seconds: u32) -> bool {
    remaining_seconds != INITIAL_REMAINING_SECONDS
}

fn start_timer(hwnd: HWND) {
    unsafe {
        let _ = SetTimer(Some(hwnd), TIMER_ID, 1_000, None);
    }
}

fn stop_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
    }
}
