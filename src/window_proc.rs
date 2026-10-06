use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use crate::about::show_about_window;
use crate::asset::CharacterCatalog;
use crate::pet_window::{
    PET_COMMAND_ABOUT, PET_COMMAND_ACKNOWLEDGE, PET_COMMAND_EXIT, PET_COMMAND_SETTINGS, PET_COMMAND_SHOW_MAIN,
    PET_COMMAND_START, PetWindow, WM_PET_COMMAND,
};
use crate::settings::{
    ABOUT_BUTTON_ID, SETTINGS_APPLIED_ID, SETTINGS_BUTTON_ID, SETTINGS_CHANGED_ID, SETTINGS_CLOSED_ID,
    create_settings_panel, draw_settings_button, refresh_settings_panel_theme, resize_settings_panel,
    save_settings_panel, set_settings_button_text, update_settings_button_font, update_settings_panel_font,
    update_settings_panel_language,
};
use crate::timer_panel::resize_timer_panel;
use crate::ui::font::common_gui_font;
use crate::ui::{
    button::{
        ControlButton, button_from_command, layout_control_buttons_for, refresh_control_buttons_for,
        update_control_buttons_for,
    },
    component::Component,
    countdown_rect, invalidate_countdown_font, release_countdown_font,
    theme::{Theme, paint_background, refresh_theme},
};
use crate::{
    config::Config,
    i18n::Language,
    tray_icon::{TRAY_MENU_ABOUT_ID, TRAY_MENU_SETTINGS_ID, TRAY_MENU_START_ID, TrayIcon, WM_TRAYICON},
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, GetDC, HFONT, InvalidateRect, PAINTSTRUCT, ReleaseDC},
    UI::HiDpi::GetDpiForWindow,
    UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, HWND_TOP, IDC_HAND,
        IsDialogMessageW, IsWindow, IsWindowVisible, KillTimer, LoadCursorW, MSG, MoveWindow, PostQuitMessage, SW_HIDE,
        SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetCursor, SetForegroundWindow, SetTimer,
        SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DPICHANGED,
        WM_DRAWITEM, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE, WM_THEMECHANGED, WM_TIMER,
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
    pub theme: Theme,
    pub tray_icon: TrayIcon,
    pub tray_when_close: bool,
    pub pet_window: PetWindow,
    pub character_catalog: CharacterCatalog,
    pub components: Vec<Box<dyn Component>>,
    pub common_gui_font: Option<HFONT>,
    pub settings_button: HWND,
    pub settings_button_hovered: bool,
    pub about_button: HWND,
    pub about_button_hovered: bool,
    pub settings_panel: HWND,
    pub timer_panel: HWND,
    pub control_buttons: [HWND; 3],
}

static INITIAL_REMAINING_SECONDS: AtomicU32 = AtomicU32::new(DEFAULT_INITIAL_REMAINING_SECONDS);
static REMAINING_SECONDS: AtomicU32 = AtomicU32::new(DEFAULT_INITIAL_REMAINING_SECONDS);
static TIMER_STATE: Mutex<TimerState> = Mutex::new(TimerState::NotStarted);

pub fn set_initial_remaining_seconds(seconds: u32) {
    INITIAL_REMAINING_SECONDS.store(seconds, Ordering::Relaxed);
    REMAINING_SECONDS.store(seconds, Ordering::Relaxed);
}

pub fn remaining_seconds() -> u32 {
    REMAINING_SECONDS.load(Ordering::Relaxed)
}

pub fn attach_window_state(hwnd: HWND, state: WindowState) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(state)) as isize);
    }
}

pub fn process_settings_message(hwnd: HWND, message: &MSG) -> bool {
    let Some(state) = window_state(hwnd) else {
        return false;
    };
    if unsafe { IsWindowVisible(state.settings_panel).as_bool() } {
        unsafe { IsDialogMessageW(state.settings_panel, message).as_bool() }
    } else {
        false
    }
}

pub unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
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
        WM_PET_COMMAND => {
            match wparam.0 {
                PET_COMMAND_ACKNOWLEDGE => acknowledge_pet(hwnd),
                PET_COMMAND_START => activate_button(hwnd, ControlButton::Play),
                PET_COMMAND_SETTINGS => open_settings(hwnd),
                PET_COMMAND_ABOUT => open_about(hwnd),
                PET_COMMAND_SHOW_MAIN => unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = SetForegroundWindow(hwnd);
                },
                PET_COMMAND_EXIT => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match (wparam.0 & 0xFFFF) as usize {
                SETTINGS_BUTTON_ID => {
                    toggle_settings(hwnd);
                    return LRESULT(0);
                }
                ABOUT_BUTTON_ID => {
                    open_about(hwnd);
                    return LRESULT(0);
                }
                SETTINGS_APPLIED_ID => {
                    apply_saved_settings(hwnd);
                    show_timer_panel(hwnd);
                    return LRESULT(0);
                }
                SETTINGS_CLOSED_ID => {
                    show_timer_panel(hwnd);
                    return LRESULT(0);
                }
                SETTINGS_CHANGED_ID => {
                    apply_saved_settings(hwnd);
                    return LRESULT(0);
                }
                _ => {}
            }
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
        WM_DRAWITEM => {
            if let Some(state) = window_state(hwnd) {
                let item = unsafe { &*(lparam.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT) };
                if matches!(item.CtlID as usize, SETTINGS_BUTTON_ID | ABOUT_BUTTON_ID) {
                    let dark_mode = match state.theme {
                        Theme::Dark => true,
                        Theme::Light => false,
                        Theme::System => crate::ui::theme::is_dark_mode().unwrap_or(false),
                    };
                    let hovered = if item.CtlID as usize == ABOUT_BUTTON_ID {
                        state.about_button_hovered
                    } else {
                        state.settings_button_hovered
                    };
                    draw_settings_button(item, dark_mode, hovered, state.common_gui_font);
                    return LRESULT(1);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SETCURSOR => {
            if let Some(state) = window_state_mut(hwnd) {
                let cursor_window = HWND(wparam.0 as _);
                for (button, hovered) in [
                    (state.settings_button, &mut state.settings_button_hovered),
                    (state.about_button, &mut state.about_button_hovered),
                ] {
                    let over_button = cursor_window == button;
                    if *hovered != over_button {
                        *hovered = over_button;
                        unsafe {
                            let _ = InvalidateRect(Some(button), None, false);
                        }
                    }
                }
                if cursor_window == state.settings_button || cursor_window == state.about_button {
                    unsafe {
                        let _ = SetCursor(Some(LoadCursorW(None, IDC_HAND).unwrap_or_default()));
                    }
                    return LRESULT(1);
                }
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
            hide_pet(hwnd);
            if window_state(hwnd).map(|state| state.tray_when_close).unwrap_or(false) {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
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

            let dpi = (wparam.0 & 0xFFFF) as u32;
            if let Some(state) = window_state(hwnd) {
                let _ = resize_timer_panel(state.timer_panel, hwnd);
                let _ = resize_settings_panel(state.settings_panel, hwnd);
                let _ = layout_control_buttons_for(state.timer_panel, &state.control_buttons);
            }
            let chinese = window_state(hwnd)
                .map(|state| state.language == crate::i18n::Language::Chinese)
                .unwrap_or(false);
            if let Some(state) = window_state_mut(hwnd) {
                state.common_gui_font = common_gui_font(dpi, chinese);
            }
            if let Some(state) = window_state(hwnd) {
                update_settings_button_font(state.settings_button, state.common_gui_font);
                update_settings_button_font(state.about_button, state.common_gui_font);
                update_settings_panel_font(state.settings_panel, dpi);
            }
            let _ = layout_window_state(hwnd);
            if let Some(state) = window_state(hwnd) {
                refresh_control_buttons_for(&state.control_buttons);
            }

            LRESULT(0)
        }
        WM_SIZE => {
            if let Some(state) = window_state(hwnd) {
                let _ = resize_timer_panel(state.timer_panel, hwnd);
                let _ = resize_settings_panel(state.settings_panel, hwnd);
                let _ = layout_control_buttons_for(state.timer_panel, &state.control_buttons);
            }
            layout_settings_button(hwnd);
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            if let Some(state) = window_state(hwnd) {
                refresh_theme(hwnd, state.theme);
                unsafe {
                    let _ = InvalidateRect(Some(state.settings_button), None, false);
                    let _ = InvalidateRect(Some(state.about_button), None, false);
                }
            }
            if let Some(state) = window_state(hwnd) {
                refresh_control_buttons_for(&state.control_buttons);
            }
            refresh_window_state(hwnd);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DESTROY => {
            hide_pet(hwnd);
            if let Some(state) = window_state(hwnd) {
                state.tray_icon.delete(hwnd);
            }
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                release_countdown_font();
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
    let Some(timer_panel) = window_state(hwnd).map(|state| state.timer_panel) else {
        return;
    };
    let hdc = unsafe { GetDC(Some(timer_panel)) };
    if !hdc.is_invalid() {
        if let Ok(rect) = countdown_rect(timer_panel, hdc, remaining_seconds) {
            let _ = unsafe { InvalidateRect(Some(timer_panel), Some(&rect), false) };
        }
        let _ = unsafe { ReleaseDC(Some(timer_panel), hdc) };
    }
}

fn activate_button(hwnd: HWND, button: ControlButton) {
    let previous_remaining = REMAINING_SECONDS.load(Ordering::Relaxed);

    match button {
        ControlButton::Play => {
            let _ = hide_pet_animated(hwnd);
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
            hide_pet(hwnd);
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::Paused;
            stop_timer(hwnd);
        }
        ControlButton::Reset => {
            hide_pet(hwnd);
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

    update_control_buttons_for(
        &window_state(hwnd).expect("window state missing").control_buttons,
        play_enabled(timer_state),
        pause_enabled(timer_state),
        reset_enabled(remaining),
    );
    Ok(())
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
    if let Some(state) = window_state(hwnd) {
        state.pet_window.show();
        let _ = state.pet_window.play_jump();
    }
}

fn acknowledge_pet(hwnd: HWND) {
    let initial_remaining = initial_remaining_seconds();
    let previous_remaining = REMAINING_SECONDS.swap(initial_remaining, Ordering::Relaxed);
    *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::Running;
    start_timer(hwnd);
    unsafe {
        invalidate_countdown(hwnd, previous_remaining);
        invalidate_countdown(hwnd, initial_remaining);
    }
    let _ = sync_control_button_enabled(hwnd);
}

fn hide_pet(hwnd: HWND) {
    if let Some(state) = window_state(hwnd) {
        state.pet_window.hide();
    }
}

fn hide_pet_animated(hwnd: HWND) -> windows::core::Result<()> {
    let Some(state) = window_state(hwnd) else {
        return Ok(());
    };
    state.pet_window.hide_animated()
}

fn initial_remaining_seconds() -> u32 {
    INITIAL_REMAINING_SECONDS.load(Ordering::Relaxed)
}

pub fn layout_window_state(hwnd: HWND) -> windows::core::Result<()> {
    let Some(state) = window_state(hwnd) else {
        return Ok(());
    };
    layout_settings_button(hwnd);

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

fn layout_settings_button(hwnd: HWND) {
    let Some(state) = window_state(hwnd) else {
        return;
    };
    if state.settings_button == HWND::default() {
        return;
    }
    let mut rect = RECT::default();
    unsafe {
        if GetClientRect(hwnd, &mut rect).is_err() {
            return;
        }
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let scale = |value: i32| value * dpi as i32 / 96;
    let width = scale(96);
    let height = scale(32);
    let right_margin = scale(24);
    let bottom_margin = scale(32);
    let settings_left = rect.right - right_margin - width;
    unsafe {
        let _ = MoveWindow(
            state.settings_button,
            settings_left,
            rect.bottom - bottom_margin - height,
            width,
            height,
            true,
        );
        let _ = MoveWindow(
            state.about_button,
            settings_left - scale(24) - width,
            rect.bottom - bottom_margin - height,
            width,
            height,
            true,
        );
        let _ = SetWindowPos(
            state.settings_button,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
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

fn window_state_mut(hwnd: HWND) -> Option<&'static mut WindowState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
    unsafe { raw.as_mut() }
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
        TRAY_MENU_SETTINGS_ID => {
            open_settings(hwnd);
            true
        }
        TRAY_MENU_START_ID => {
            activate_button(hwnd, ControlButton::Play);
            true
        }
        TRAY_MENU_ABOUT_ID => {
            open_about(hwnd);
            true
        }
        _ => false,
    }
}

fn open_about(hwnd: HWND) {
    let (language, theme) = window_state(hwnd)
        .map(|state| (state.language, state.theme))
        .unwrap_or((Language::English, Theme::System));
    let _ = show_about_window(hwnd, language, theme);
}

fn open_settings(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    let Some(state) = window_state(hwnd) else {
        return;
    };
    unsafe {
        let _ = ShowWindow(state.timer_panel, SW_HIDE);
        let _ = ShowWindow(state.about_button, SW_HIDE);
        let _ = SetWindowPos(
            state.settings_button,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
    set_settings_button_text_for(hwnd, true);
    if unsafe { IsWindow(Some(state.settings_panel)).as_bool() } {
        let _ = resize_settings_panel(state.settings_panel, hwnd);
        unsafe {
            let _ = ShowWindow(state.settings_panel, SW_SHOW);
        }
        return;
    }
    let Ok(config) = Config::load() else {
        return;
    };
    let font = state.common_gui_font;
    let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }
        .unwrap_or_default()
        .into();
    if let Ok(panel) = create_settings_panel(hwnd, instance, config, font) {
        if let Some(state) = window_state_mut(hwnd) {
            state.settings_panel = panel;
            let _ = resize_settings_panel(panel, hwnd);
            unsafe {
                let _ = SetWindowPos(
                    state.settings_button,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }
}

fn show_timer_panel(hwnd: HWND) {
    if let Some(state) = window_state(hwnd) {
        set_settings_button_text_for(hwnd, false);
        unsafe {
            let _ = ShowWindow(state.timer_panel, SW_SHOW);
            let _ = ShowWindow(state.about_button, SW_SHOW);
        }
    }
}

fn toggle_settings(hwnd: HWND) {
    let Some(state) = window_state(hwnd) else {
        return;
    };
    let settings_visible = unsafe { IsWindowVisible(state.settings_panel).as_bool() };
    if !settings_visible {
        open_settings(hwnd);
        return;
    }

    if save_settings_panel(state.settings_panel).is_ok() {
        apply_saved_settings(hwnd);
        unsafe {
            let _ = DestroyWindow(state.settings_panel);
        }
    }
}

fn set_settings_button_text_for(hwnd: HWND, settings_visible: bool) {
    if let Some(state) = window_state(hwnd) {
        let text = if settings_visible {
            settings_back_text(state.language)
        } else {
            settings_menu_text(state.language)
        };
        set_settings_button_text(state.settings_button, text);
    }
}

fn apply_saved_settings(hwnd: HWND) {
    let Ok(config) = Config::load() else {
        return;
    };
    set_initial_remaining_seconds(config.period);
    let language = config.language();
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    if let Some(state) = window_state_mut(hwnd) {
        state.language = language;
        state.tray_when_close = config.tray_when_close;
        state.theme = config.theme();
        refresh_theme(hwnd, state.theme);
        refresh_settings_panel_theme(state.settings_panel, &config);
        let font = common_gui_font(dpi, language == Language::Chinese);
        state.common_gui_font = font;
        let _ = state.tray_icon.update_language(
            hwnd,
            crate::i18n::main_window_title(language),
            crate::tray_menu_start_text(language),
            crate::tray_menu_show_text(language),
            settings_menu_text(language),
            crate::tray_menu_about_text(language),
            crate::tray_menu_exit_text(language),
        );
        state.pet_window.set_language(language);
        unsafe {
            let title = crate::i18n::main_window_title(language)
                .encode_utf16()
                .chain([0])
                .collect::<Vec<_>>();
            let _ = SetWindowTextW(hwnd, windows::core::PCWSTR(title.as_ptr()));
        }
        update_settings_panel_language(state.settings_panel, language);
        update_settings_panel_font(state.settings_panel, dpi);
        update_settings_button_font(state.settings_button, font);
        update_settings_button_font(state.about_button, font);
        set_settings_button_text(state.about_button, crate::tray_menu_about_text(language));
        if let Some(animations) = state
            .character_catalog
            .get(&config.character)
            .or_else(|| state.character_catalog.get("cat"))
        {
            let _ = state.pet_window.set_animations(animations.clone());
        }
    }
    let settings_visible = window_state(hwnd)
        .map(|state| unsafe { IsWindowVisible(state.settings_panel).as_bool() })
        .unwrap_or(false);
    set_settings_button_text_for(hwnd, settings_visible);
}

fn settings_menu_text(language: Language) -> &'static str {
    match language {
        Language::Chinese => "设置",
        Language::English => "Settings",
    }
}

fn settings_back_text(language: Language) -> &'static str {
    match language {
        Language::Chinese => "< 返回",
        Language::English => "< Back",
    }
}
