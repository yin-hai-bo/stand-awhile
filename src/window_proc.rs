use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use crate::ui::{
    button::{ControlButton, controls_rect, draw_controls, hit_test_control_button},
    countdown_rect, draw_countdown, invalidate_countdown_font, release_countdown_font,
    theme::{paint_background, refresh_theme},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, GetDC, InvalidateRect, PAINTSTRUCT, ReleaseDC},
    UI::WindowsAndMessaging::{
        DefWindowProcW, KillTimer, PostQuitMessage, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, WM_DESTROY,
        WM_DPICHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SETTINGCHANGE, WM_THEMECHANGED,
        WM_TIMER,
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
static BUTTON_STATE: Mutex<ButtonState> = Mutex::new(ButtonState {
    hovered: None,
    pressed: None,
});

struct ButtonState {
    hovered: Option<ControlButton>,
    pressed: Option<ControlButton>,
}

pub unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
            let _ = draw_countdown(hwnd, hdc, REMAINING_SECONDS.load(Ordering::Relaxed));
            if should_repaint_controls(hwnd, &paint.rcPaint) {
                let button_state = BUTTON_STATE.lock().expect("button state mutex poisoned");
                let timer_state = *TIMER_STATE.lock().expect("timer state mutex poisoned");
                let _ = draw_controls(
                    hwnd,
                    hdc,
                    button_state.hovered,
                    button_state.pressed,
                    play_enabled(timer_state),
                    pause_enabled(timer_state),
                    reset_enabled(REMAINING_SECONDS.load(Ordering::Relaxed)),
                );
            }
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
                    invalidate_controls(hwnd);
                }
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
        WM_MOUSEMOVE => {
            update_hover_state(hwnd, lparam);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = point_from_lparam(lparam);
            let timer_state = *TIMER_STATE.lock().expect("timer state mutex poisoned");
            let button = hit_test_control_button(
                hwnd,
                x,
                y,
                play_enabled(timer_state),
                pause_enabled(timer_state),
                reset_enabled(REMAINING_SECONDS.load(Ordering::Relaxed)),
            )
            .ok()
            .flatten();
            if button.is_some() {
                let mut button_state = BUTTON_STATE.lock().expect("button state mutex poisoned");
                button_state.pressed = button;
                button_state.hovered = button;
                drop(button_state);
                invalidate_controls(hwnd);
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_LBUTTONUP => {
            let (pressed, released_over) = {
                let (x, y) = point_from_lparam(lparam);
                let timer_state = *TIMER_STATE.lock().expect("timer state mutex poisoned");
                let released_over = hit_test_control_button(
                    hwnd,
                    x,
                    y,
                    play_enabled(timer_state),
                    pause_enabled(timer_state),
                    reset_enabled(REMAINING_SECONDS.load(Ordering::Relaxed)),
                )
                .ok()
                .flatten();
                let mut button_state = BUTTON_STATE.lock().expect("button state mutex poisoned");
                let pressed = button_state.pressed.take();
                button_state.hovered = released_over;
                (pressed, released_over)
            };

            if let Some(button) = pressed {
                if Some(button) == released_over {
                    activate_button(hwnd, button);
                }
                invalidate_controls(hwnd);
                return LRESULT(0);
            }

            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
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

fn invalidate_controls(hwnd: HWND) {
    if let Ok(rect) = controls_rect(hwnd) {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), Some(&rect), false);
        }
    }
}

fn should_repaint_controls(hwnd: HWND, paint_rect: &RECT) -> bool {
    if let Ok(button_rect) = controls_rect(hwnd) {
        return rects_intersect(paint_rect, &button_rect);
    }

    true
}

fn rects_intersect(a: &RECT, b: &RECT) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

fn update_hover_state(hwnd: HWND, lparam: LPARAM) {
    let (x, y) = point_from_lparam(lparam);
    let timer_state = *TIMER_STATE.lock().expect("timer state mutex poisoned");
    let hovered = hit_test_control_button(
        hwnd,
        x,
        y,
        play_enabled(timer_state),
        pause_enabled(timer_state),
        reset_enabled(REMAINING_SECONDS.load(Ordering::Relaxed)),
    )
    .ok()
    .flatten();
    let mut button_state = BUTTON_STATE.lock().expect("button state mutex poisoned");
    if button_state.hovered != hovered {
        button_state.hovered = hovered;
        drop(button_state);
        invalidate_controls(hwnd);
    }
}

fn activate_button(hwnd: HWND, button: ControlButton) {
    match button {
        ControlButton::Play => {
            let previous_remaining = REMAINING_SECONDS.load(Ordering::Relaxed);
            if previous_remaining == 0 {
                REMAINING_SECONDS.store(INITIAL_REMAINING_SECONDS, Ordering::Relaxed);
                unsafe {
                    invalidate_countdown(hwnd, previous_remaining);
                    invalidate_countdown(hwnd, INITIAL_REMAINING_SECONDS);
                }
            }
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::Running;
            start_timer(hwnd);
            invalidate_controls(hwnd);
        }
        ControlButton::Pause => {
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::Paused;
            stop_timer(hwnd);
            invalidate_controls(hwnd);
        }
        ControlButton::Reset => {
            let previous_remaining = REMAINING_SECONDS.swap(INITIAL_REMAINING_SECONDS, Ordering::Relaxed);
            let new_state = if previous_remaining == INITIAL_REMAINING_SECONDS {
                *TIMER_STATE.lock().expect("timer state mutex poisoned")
            } else {
                TimerState::NotStarted
            };
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = new_state;
            if matches!(
                new_state,
                TimerState::NotStarted | TimerState::Finished | TimerState::Paused
            ) {
                stop_timer(hwnd);
            }
            unsafe {
                invalidate_countdown(hwnd, previous_remaining);
                invalidate_countdown(hwnd, INITIAL_REMAINING_SECONDS);
            }
            invalidate_controls(hwnd);
        }
    }
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
        let _ = windows::Win32::UI::WindowsAndMessaging::SetTimer(Some(hwnd), TIMER_ID, 1_000, None);
    }
}

fn stop_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
    }
}

fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
    let value = lparam.0 as u32;
    let x = (value & 0xFFFF) as u16 as i16 as i32;
    let y = ((value >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}
