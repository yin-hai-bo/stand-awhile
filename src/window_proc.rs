use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use crate::about::show_about_window;
use crate::ui::{
    button::{
        ControlButton, button_from_command, layout_control_buttons, refresh_control_buttons, update_control_buttons,
    },
    check_box::{CheckBox, invalidate_check_box_font, release_check_box_font},
    component::Component,
    countdown_rect, draw_countdown,
    hyper_link_text::{invalidate_hyper_link_text_font, release_hyper_link_text_font},
    invalidate_countdown_font, release_countdown_font,
    theme::{paint_background, refresh_theme},
};
use crate::{
    config::{open_config_directory, show_config_open_error},
    i18n::{Language, reminder_notification_message, reminder_notification_title},
    toast,
    tray_icon::{TRAY_MENU_ABOUT_ID, TRAY_MENU_OPEN_CONFIG_ID, TrayIcon, WM_TRAYICON},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, GetDC, InvalidateRect, PAINTSTRUCT, ReleaseDC},
    UI::WindowsAndMessaging::{
        DefWindowProcW, FLASHW_ALL, FLASHW_TIMERNOFG, FLASHWINFO, FlashWindowEx, GWLP_USERDATA, GetWindowLongPtrW,
        IsWindowVisible, KillTimer, PostQuitMessage, SW_HIDE, SWP_NOACTIVATE, SWP_NOZORDER, SetTimer,
        SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DPICHANGED, WM_NCDESTROY,
        WM_PAINT, WM_SETTINGCHANGE, WM_THEMECHANGED, WM_TIMER,
    },
};

pub const TIMER_ID: usize = 1;
const DEFAULT_INITIAL_REMAINING_SECONDS: u32 = 20 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerState {
    NotStarted,
    Running,
    Paused,
    Finished,
}

pub struct WindowState {
    pub language: Language,
    pub tray_icon: TrayIcon,
    pub tray_check_box: CheckBox,
    pub components: Vec<Box<dyn Component>>,
}

static INITIAL_REMAINING_SECONDS: AtomicU32 = AtomicU32::new(DEFAULT_INITIAL_REMAINING_SECONDS);
static REMAINING_SECONDS: AtomicU32 = AtomicU32::new(DEFAULT_INITIAL_REMAINING_SECONDS);
static TIMER_STATE: Mutex<TimerState> = Mutex::new(TimerState::NotStarted);

pub fn set_initial_remaining_seconds(seconds: u32) {
    INITIAL_REMAINING_SECONDS.store(seconds, Ordering::Relaxed);
    REMAINING_SECONDS.store(seconds, Ordering::Relaxed);
}

pub fn attach_window_state(hwnd: HWND, state: WindowState) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(state)) as isize);
    }
}

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
                    notify_timer_finished(hwnd);
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
            if handle_tray_menu_command(hwnd, wparam) {
                return LRESULT(0);
            }
            if let Some(state) = window_state(hwnd) {
                if state.tray_icon.handle_command(hwnd, wparam) {
                    return LRESULT(0);
                }
            }
            if let Some(button) = button_from_command(wparam) {
                activate_button(hwnd, button);
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_TRAYICON => {
            if let Some(state) = window_state(hwnd) {
                if state.tray_icon.handle_callback(hwnd, lparam).unwrap_or(false) {
                    return LRESULT(0);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if window_state(hwnd)
                .map(|state| state.tray_check_box.is_checked())
                .unwrap_or(false)
            {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DPICHANGED => {
            invalidate_countdown_font();
            invalidate_check_box_font();
            invalidate_hyper_link_text_font();

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
            let _ = layout_window_state(hwnd);
            let _ = refresh_control_buttons(hwnd);

            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            refresh_theme(hwnd);
            let _ = refresh_control_buttons(hwnd);
            refresh_window_state(hwnd);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DESTROY => {
            if let Some(state) = window_state(hwnd) {
                state.tray_icon.delete(hwnd);
            }
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                release_check_box_font();
                release_countdown_font();
                release_hyper_link_text_font();
                PostQuitMessage(0);
            };
            LRESULT(0)
        }
        WM_NCDESTROY => {
            release_window_state(hwnd);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
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
            let initial_remaining = initial_remaining_seconds();
            if previous_remaining == 0 {
                REMAINING_SECONDS.store(initial_remaining, Ordering::Relaxed);
                unsafe {
                    invalidate_countdown(hwnd, previous_remaining);
                    invalidate_countdown(hwnd, initial_remaining);
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
            let initial_remaining = initial_remaining_seconds();
            let previous_remaining = REMAINING_SECONDS.swap(initial_remaining, Ordering::Relaxed);
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::NotStarted;
            stop_timer(hwnd);
            unsafe {
                invalidate_countdown(hwnd, previous_remaining);
                invalidate_countdown(hwnd, initial_remaining);
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
    remaining_seconds != initial_remaining_seconds()
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

fn notify_timer_finished(hwnd: HWND) {
    if unsafe { IsWindowVisible(hwnd).as_bool() } {
        flash_window(hwnd);
        return;
    }

    let Some(state) = window_state(hwnd) else {
        return;
    };

    let language = state.language;
    if toast::show(
        reminder_notification_title(language),
        reminder_notification_message(language),
    )
    .is_err()
    {
        let _ = state.tray_icon.show_notification(
            hwnd,
            reminder_notification_title(language),
            reminder_notification_message(language),
        );
    }
}

fn flash_window(hwnd: HWND) {
    let mut flash_info = FLASHWINFO {
        cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
        hwnd,
        dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
        uCount: 3,
        dwTimeout: 0,
    };

    unsafe {
        let _ = FlashWindowEx(&mut flash_info);
    }
}

fn initial_remaining_seconds() -> u32 {
    INITIAL_REMAINING_SECONDS.load(Ordering::Relaxed)
}

pub fn layout_window_state(hwnd: HWND) -> windows::core::Result<()> {
    let Some(state) = window_state(hwnd) else {
        return Ok(());
    };

    let hdc = unsafe { GetDC(Some(hwnd)) };
    if hdc.is_invalid() {
        return Err(windows::core::Error::from_win32());
    }

    let result = state
        .components
        .iter()
        .try_for_each(|component| component.layout(hwnd, hdc));

    unsafe {
        let _ = ReleaseDC(Some(hwnd), hdc);
    }

    result
}

fn refresh_window_state(hwnd: HWND) {
    if let Some(state) = window_state(hwnd) {
        for component in &state.components {
            component.invalidate();
        }
    }
}

fn window_state(hwnd: HWND) -> Option<&'static WindowState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
    unsafe { raw.as_ref() }
}

fn release_window_state(hwnd: HWND) {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if raw != 0 {
        let _ = unsafe { Box::from_raw(raw as *mut WindowState) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
    }
}

fn handle_tray_menu_command(hwnd: HWND, wparam: WPARAM) -> bool {
    match (wparam.0 & 0xFFFF) as usize {
        TRAY_MENU_OPEN_CONFIG_ID => {
            if let Err(error) = open_config_directory(hwnd) {
                let language = window_state(hwnd)
                    .map(|state| state.language)
                    .unwrap_or(Language::English);
                show_config_open_error(hwnd, &error, language);
            }
            true
        }
        TRAY_MENU_ABOUT_ID => {
            let language = window_state(hwnd)
                .map(|state| state.language)
                .unwrap_or(Language::English);
            let _ = show_about_window(hwnd, language);
            true
        }
        _ => false,
    }
}
