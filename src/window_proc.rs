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
    create_settings_panel, draw_settings_button, focus_settings_panel, refresh_settings_panel_theme,
    resize_settings_panel, save_settings_panel, set_settings_button_text, update_settings_button_font,
    update_settings_panel_auto_hide, update_settings_panel_font, update_settings_panel_language,
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
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, DC_BRUSH, EndPaint, GetDC, GetStockObject, HDC, HFONT, InvalidateRect, PAINTSTRUCT, ReleaseDC,
        SetBkColor, SetDCBrushColor, SetTextColor,
    },
    UI::Controls::BST_CHECKED,
    UI::HiDpi::GetDpiForWindow,
    UI::WindowsAndMessaging::{
        BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX, CreateWindowExW, DefWindowProcW, DestroyWindow,
        GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, HMENU, HWND_BOTTOM, HWND_TOP, IDC_HAND, IsDialogMessageW,
        IsWindow, IsWindowVisible, KillTimer, LoadCursorW, MSG, MoveWindow, PostQuitMessage, SW_HIDE, SW_SHOW,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetCursor, SetForegroundWindow, SetTimer,
        SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
        WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_NCDESTROY, WM_PAINT,
        WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE, WM_THEMECHANGED, WM_TIMER, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
    },
};

pub const TIMER_ID: usize = 1;
const DEFAULT_INITIAL_REMAINING_SECONDS: u32 = 20 * 60;
const AUTO_HIDE_CHECKBOX_ID: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TimerState {
    NotStarted,
    Running,
    Paused,
    Finished,
}

pub struct WindowState {
    pub auto_hide_on_start: bool,
    pub auto_hide_checkbox: HWND,
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

pub fn create_auto_hide_checkbox(
    parent: HWND,
    instance: HINSTANCE,
    language: Language,
    font: Option<HFONT>,
    checked: bool,
) -> windows::core::Result<HWND> {
    let text: Vec<u16> = crate::i18n::auto_hide_text(language)
        .encode_utf16()
        .chain([0])
        .collect();
    let checkbox = unsafe {
        CreateWindowExW(
            Default::default(),
            windows::core::w!("BUTTON"),
            windows::core::PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            0,
            0,
            1,
            1,
            Some(parent),
            Some(HMENU(AUTO_HIDE_CHECKBOX_ID as *mut _)),
            Some(instance),
            None,
        )?
    };
    update_settings_button_font(checkbox, font);
    set_auto_hide_checked(checkbox, checked);
    Ok(checkbox)
}

fn set_auto_hide_checked(checkbox: HWND, checked: bool) {
    unsafe {
        SendMessageW(
            checkbox,
            BM_SETCHECK,
            Some(WPARAM(if checked { BST_CHECKED.0 as usize } else { 0 })),
            None,
        );
    }
}

fn save_auto_hide_preference(hwnd: HWND) {
    let Some(state) = window_state_mut(hwnd) else { return };
    let checked =
        unsafe { SendMessageW(state.auto_hide_checkbox, BM_GETCHECK, None, None).0 == BST_CHECKED.0 as isize };
    let result = Config::load().and_then(|mut config| {
        config.auto_hide_on_start = checked;
        config.save()
    });
    if let Err(error) = result {
        set_auto_hide_checked(state.auto_hide_checkbox, state.auto_hide_on_start);
        let text: Vec<u16> = error.to_string().encode_utf16().chain([0]).collect();
        let title: Vec<u16> = crate::i18n::main_window_title(state.language)
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
            MessageBoxW(
                Some(hwnd),
                windows::core::PCWSTR(text.as_ptr()),
                windows::core::PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    } else {
        state.auto_hide_on_start = checked;
        update_settings_panel_auto_hide(state.settings_panel, checked);
    }
}

pub fn set_initial_remaining_seconds(seconds: u32) {
    INITIAL_REMAINING_SECONDS.store(seconds, Ordering::Relaxed);
    REMAINING_SECONDS.store(seconds, Ordering::Relaxed);
}

fn apply_timer_period(hwnd: HWND, seconds: u32) {
    if seconds == initial_remaining_seconds() {
        return;
    }
    let previous_remaining = remaining_seconds();
    set_initial_remaining_seconds(seconds);
    unsafe {
        invalidate_countdown(hwnd, previous_remaining);
        invalidate_countdown(hwnd, seconds);
    }
    if window_state(hwnd).is_some() {
        let _ = sync_control_button_enabled(hwnd);
    }
}

pub fn remaining_seconds() -> u32 {
    if *TIMER_STATE.lock().expect("timer state mutex poisoned") == TimerState::Finished {
        initial_remaining_seconds()
    } else {
        REMAINING_SECONDS.load(Ordering::Relaxed)
    }
}

pub fn attach_window_state(hwnd: HWND, state: WindowState) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(state)) as isize);
    }
}

pub fn process_dialog_message(hwnd: HWND, message: &MSG) -> bool {
    unsafe { IsDialogMessageW(hwnd, message).as_bool() }
}

pub unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        crate::single_instance::WM_SHOW_EXISTING_INSTANCE => {
            crate::tray_icon::show_main_window(hwnd);
            LRESULT(0)
        }
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
                    show_pet_reminder(hwnd);
                }

                let _ = sync_control_button_enabled(hwnd);

                let displayed_remaining = remaining_seconds();
                unsafe {
                    invalidate_countdown(hwnd, previous_remaining);
                    if displayed_remaining != previous_remaining {
                        invalidate_countdown(hwnd, displayed_remaining);
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
                AUTO_HIDE_CHECKBOX_ID if (wparam.0 >> 16) as u32 == BN_CLICKED => {
                    save_auto_hide_preference(hwnd);
                    return LRESULT(0);
                }
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
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            if window_state(hwnd).is_some_and(|state| state.auto_hide_checkbox == HWND(lparam.0 as _)) {
                let hdc = HDC(wparam.0 as _);
                let background = crate::ui::theme::current_background_color();
                unsafe {
                    let _ = SetBkColor(hdc, background);
                    let _ = SetTextColor(hdc, crate::ui::theme::current_text_color());
                    let _ = SetDCBrushColor(hdc, background);
                    return LRESULT(GetStockObject(DC_BRUSH).0 as isize);
                }
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
                unsafe {
                    let _ = InvalidateRect(Some(state.timer_panel), None, false);
                }
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
                update_settings_button_font(state.auto_hide_checkbox, state.common_gui_font);
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
                    let _ = InvalidateRect(Some(state.auto_hide_checkbox), None, false);
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
        _ => {
            if let Some(state) = window_state(hwnd)
                && state.tray_icon.handle_taskbar_created(hwnd, msg).unwrap_or(false)
            {
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
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
            let initial_remaining = initial_remaining_seconds();
            let previous_remaining = REMAINING_SECONDS.swap(initial_remaining, Ordering::Relaxed);
            *TIMER_STATE.lock().expect("timer state mutex poisoned") = TimerState::NotStarted;
            stop_timer(hwnd);
            hide_pet(hwnd);
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
        reset_enabled(timer_state, remaining),
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

fn reset_enabled(timer_state: TimerState, remaining_seconds: u32) -> bool {
    timer_state != TimerState::Finished
        && (timer_state != TimerState::NotStarted || remaining_seconds != initial_remaining_seconds())
}

fn start_timer(hwnd: HWND) {
    unsafe {
        let _ = SetTimer(Some(hwnd), TIMER_ID, 1_000, None);
    }
    hide_main_window_on_start(hwnd, window_state(hwnd).is_some_and(|state| state.auto_hide_on_start));
}

fn stop_timer(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(Some(hwnd), TIMER_ID);
    }
}

fn show_pet_reminder(hwnd: HWND) {
    if let Some(state) = window_state(hwnd) {
        let _ = state.pet_window.show_reminder();
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

fn hide_main_window_on_start(hwnd: HWND, auto_hide: bool) {
    if auto_hide {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
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
    let right_margin = scale(12);
    let bottom_margin = scale(32);
    let settings_left = rect.right - right_margin - width;
    unsafe {
        let _ = MoveWindow(
            state.auto_hide_checkbox,
            scale(24),
            rect.bottom - bottom_margin - height,
            (settings_left - scale(24) - width - scale(48)).max(0),
            height,
            true,
        );
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
            settings_left - scale(12) - width,
            rect.bottom - bottom_margin - height,
            width,
            height,
            true,
        );
    }
    set_main_tab_order(
        state.timer_panel,
        state.auto_hide_checkbox,
        state.about_button,
        state.settings_button,
    );
    if unsafe { IsWindowVisible(state.settings_panel).as_bool() } {
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

pub(crate) fn set_main_tab_order(timer_panel: HWND, checkbox: HWND, about: HWND, settings: HWND) {
    for control in [timer_panel, checkbox, about, settings] {
        unsafe {
            let _ = SetWindowPos(
                control,
                Some(HWND_BOTTOM),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

fn refresh_window_state(hwnd: HWND) {
    if let Some(state) = window_state(hwnd) {
        for component in &state.components {
            component.invalidate();
        }
    }
}

pub(crate) fn window_state(hwnd: HWND) -> Option<&'static WindowState> {
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
        let _ = ShowWindow(state.auto_hide_checkbox, SW_HIDE);
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
        focus_settings_panel(state.settings_panel);
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
            focus_settings_panel(panel);
        }
    }
}

fn show_timer_panel(hwnd: HWND) {
    if let Some(state) = window_state(hwnd) {
        set_settings_button_text_for(hwnd, false);
        unsafe {
            let _ = ShowWindow(state.timer_panel, SW_SHOW);
            let _ = ShowWindow(state.about_button, SW_SHOW);
            let _ = ShowWindow(state.auto_hide_checkbox, SW_SHOW);
        }
    }
    layout_settings_button(hwnd);
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
    apply_timer_period(hwnd, config.period);
    let language = config.language();
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    if let Some(state) = window_state_mut(hwnd) {
        state.language = language;
        state.auto_hide_on_start = config.auto_hide_on_start;
        update_settings_panel_auto_hide(state.settings_panel, config.auto_hide_on_start);
        set_auto_hide_checked(state.auto_hide_checkbox, config.auto_hide_on_start);
        set_settings_button_text(state.auto_hide_checkbox, crate::i18n::auto_hide_text(language));
        unsafe {
            let _ = InvalidateRect(Some(state.timer_panel), None, false);
        }
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
        update_settings_button_font(state.auto_hide_checkbox, font);
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

#[cfg(test)]
mod tests {
    use super::*;
    static TIMER_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn background_start_counts_down_and_reminds_with_either_auto_hide_preference() {
        use crate::{startup::StartupMode, ui::gdi_plus::GdiPlus};
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                IDI_APPLICATION, LoadIconW, PM_REMOVE, PeekMessageW, RegisterClassW, WM_QUIT, WNDCLASSW,
            },
        };
        let _guard = TIMER_TEST_LOCK.lock().unwrap();
        let gdi_plus = GdiPlus::new().unwrap();
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: windows::core::w!("StandAwhileBackgroundCountdownTest"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            for auto_hide_on_start in [false, true] {
                let mode = StartupMode::new(true, true);
                let hwnd = CreateWindowExW(
                    Default::default(),
                    class.lpszClassName,
                    windows::core::w!(""),
                    mode.window_style(),
                    -10000,
                    -10000,
                    100,
                    100,
                    None,
                    None,
                    Some(instance),
                    None,
                )
                .unwrap();
                let catalog = crate::asset::load_character_catalog(&gdi_plus).unwrap();
                let pet_window = PetWindow::create(
                    instance,
                    hwnd,
                    catalog.get("cat").unwrap().clone(),
                    Language::English,
                    None,
                    "Settings",
                )
                .unwrap();
                let pet_hwnd = pet_window.test_hwnd();
                let tray_icon = TrayIcon::create(
                    hwnd,
                    LoadIconW(None, IDI_APPLICATION).unwrap(),
                    "Startup test",
                    "Start",
                    "Show",
                    "Settings",
                    "About",
                    "Exit",
                )
                .unwrap();
                // Native child controls let the real timer handler update button state.
                let controls = std::array::from_fn(|_| {
                    CreateWindowExW(
                        Default::default(),
                        windows::core::w!("BUTTON"),
                        windows::core::w!(""),
                        WS_CHILD,
                        0,
                        0,
                        1,
                        1,
                        Some(hwnd),
                        None,
                        Some(instance),
                        None,
                    )
                    .unwrap()
                });
                attach_window_state(
                    hwnd,
                    WindowState {
                        auto_hide_on_start,
                        auto_hide_checkbox: controls[0],
                        language: Language::English,
                        theme: Theme::System,
                        tray_icon,
                        tray_when_close: true,
                        pet_window,
                        character_catalog: catalog,
                        components: Vec::new(),
                        common_gui_font: None,
                        settings_button: controls[0],
                        settings_button_hovered: false,
                        about_button: controls[1],
                        about_button_hovered: false,
                        settings_panel: HWND::default(),
                        timer_panel: controls[2],
                        control_buttons: controls,
                    },
                );
                set_initial_remaining_seconds(2);
                *TIMER_STATE.lock().unwrap() = TimerState::NotStarted;
                mode.finish(hwnd);
                assert_eq!(*TIMER_STATE.lock().unwrap(), TimerState::Running);
                assert_eq!(remaining_seconds(), 2);
                assert!(!IsWindowVisible(hwnd).as_bool());
                assert!(!IsWindowVisible(pet_hwnd).as_bool());

                SendMessageW(hwnd, WM_TIMER, Some(WPARAM(TIMER_ID)), None);
                assert_eq!(remaining_seconds(), 1);
                crate::startup::handle_existing_instance(true).unwrap();
                assert!(!IsWindowVisible(hwnd).as_bool());
                assert!(!IsWindowVisible(pet_hwnd).as_bool());
                assert_eq!(*TIMER_STATE.lock().unwrap(), TimerState::Running);
                assert_eq!(remaining_seconds(), 1);
                SendMessageW(
                    hwnd,
                    WM_COMMAND,
                    Some(WPARAM(crate::tray_icon::TRAY_MENU_SHOW_ID)),
                    None,
                );
                assert!(IsWindowVisible(hwnd).as_bool());
                assert_eq!(remaining_seconds(), 1);
                assert_eq!(*TIMER_STATE.lock().unwrap(), TimerState::Running);
                assert!(!IsWindowVisible(pet_hwnd).as_bool());

                SendMessageW(hwnd, WM_TIMER, Some(WPARAM(TIMER_ID)), None);
                assert_eq!(*TIMER_STATE.lock().unwrap(), TimerState::Finished);
                assert_eq!(REMAINING_SECONDS.load(Ordering::Relaxed), 0);
                assert!(IsWindowVisible(pet_hwnd).as_bool());
                let _ = ShowWindow(hwnd, SW_HIDE);
                crate::startup::handle_existing_instance(true).unwrap();
                assert!(!IsWindowVisible(hwnd).as_bool());
                assert!(IsWindowVisible(pet_hwnd).as_bool());
                assert_eq!(*TIMER_STATE.lock().unwrap(), TimerState::Finished);
                assert_eq!(REMAINING_SECONDS.load(Ordering::Relaxed), 0);
                DestroyWindow(hwnd).unwrap();
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE);
            }
        }
        set_initial_remaining_seconds(DEFAULT_INITIAL_REMAINING_SECONDS);
        *TIMER_STATE.lock().unwrap() = TimerState::NotStarted;
    }

    #[test]
    fn duplicate_instance_restores_main_window_without_changing_timer() {
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                CreateWindowExW, IsIconic, PM_REMOVE, PeekMessageW, RegisterClassW, SW_SHOWMINNOACTIVE, WM_QUIT,
                WNDCLASSW, WS_OVERLAPPEDWINDOW,
            },
        };
        let _guard = TIMER_TEST_LOCK.lock().unwrap();
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: windows::core::w!("StandAwhileDuplicateInstanceRestoreTest"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let hwnd = CreateWindowExW(
                Default::default(),
                class.lpszClassName,
                windows::core::w!(""),
                WS_OVERLAPPEDWINDOW,
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
            set_initial_remaining_seconds(1200);
            for (state, remaining) in [
                (TimerState::Running, 731),
                (TimerState::Paused, 412),
                (TimerState::Finished, 0),
            ] {
                *TIMER_STATE.lock().unwrap() = state;
                REMAINING_SECONDS.store(remaining, Ordering::Relaxed);
                for presentation in [SW_HIDE, SW_SHOWMINNOACTIVE] {
                    let _ = ShowWindow(hwnd, presentation);
                    assert!(!IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool());
                    SendMessageW(hwnd, crate::single_instance::WM_SHOW_EXISTING_INSTANCE, None, None);
                    assert!(IsWindowVisible(hwnd).as_bool());
                    assert!(!IsIconic(hwnd).as_bool());
                    assert_eq!(*TIMER_STATE.lock().unwrap(), state);
                    assert_eq!(REMAINING_SECONDS.load(Ordering::Relaxed), remaining);
                }
            }
            DestroyWindow(hwnd).unwrap();
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE);
            set_initial_remaining_seconds(DEFAULT_INITIAL_REMAINING_SECONDS);
            *TIMER_STATE.lock().unwrap() = TimerState::NotStarted;
        }
    }

    #[test]
    fn finished_countdown_displays_configured_duration() {
        let _guard = TIMER_TEST_LOCK.lock().unwrap();
        for period in [1200, 600, 3661] {
            set_initial_remaining_seconds(period);
            REMAINING_SECONDS.store(0, Ordering::Relaxed);
            *TIMER_STATE.lock().unwrap() = TimerState::Finished;
            let displayed = remaining_seconds();
            set_initial_remaining_seconds(DEFAULT_INITIAL_REMAINING_SECONDS);
            *TIMER_STATE.lock().unwrap() = TimerState::NotStarted;
            assert_eq!(displayed, period);
        }
    }

    #[test]
    fn auto_hide_option_controls_every_start_and_does_not_hide_when_toggled() {
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP},
        };
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                windows::core::w!("STATIC"),
                windows::core::w!(""),
                WS_POPUP,
                -10000,
                -10000,
                1,
                1,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let checkbox = create_auto_hide_checkbox(hwnd, instance, Language::Chinese, None, true).unwrap();
            assert_eq!(
                SendMessageW(checkbox, BM_GETCHECK, None, None).0,
                BST_CHECKED.0 as isize
            );
            for enabled in [true, true, false, true, false] {
                let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE);
                set_auto_hide_checked(checkbox, enabled);
                assert!(IsWindowVisible(hwnd).as_bool(), "toggling must not hide the window");
                let checked = SendMessageW(checkbox, BM_GETCHECK, None, None).0 == BST_CHECKED.0 as isize;
                assert_eq!(checked, enabled);
                hide_main_window_on_start(hwnd, checked);
                assert_eq!(IsWindowVisible(hwnd).as_bool(), !enabled);
            }
            DestroyWindow(hwnd).unwrap();
        }
    }

    #[test]
    fn unrelated_settings_preserve_running_paused_and_finished_countdowns() {
        let _guard = TIMER_TEST_LOCK.lock().unwrap();
        set_initial_remaining_seconds(1200);
        for (state, remaining) in [
            (TimerState::Running, 731),
            (TimerState::Paused, 412),
            (TimerState::Finished, 0),
        ] {
            *TIMER_STATE.lock().unwrap() = state;
            REMAINING_SECONDS.store(remaining, Ordering::Relaxed);
            apply_timer_period(HWND::default(), 1200);
            assert_eq!(REMAINING_SECONDS.load(Ordering::Relaxed), remaining);
            assert_eq!(
                remaining_seconds(),
                if state == TimerState::Finished { 1200 } else { remaining }
            );
            assert_eq!(*TIMER_STATE.lock().unwrap(), state);
        }
        apply_timer_period(HWND::default(), 600);
        assert_eq!(initial_remaining_seconds(), 600);
        assert_eq!(remaining_seconds(), 600);
        set_initial_remaining_seconds(DEFAULT_INITIAL_REMAINING_SECONDS);
        *TIMER_STATE.lock().unwrap() = TimerState::NotStarted;
    }

    #[test]
    fn finished_countdown_disables_reset() {
        let _guard = TIMER_TEST_LOCK.lock().unwrap();
        for remaining in [0, initial_remaining_seconds()] {
            assert!(!reset_enabled(TimerState::Finished, remaining));
        }
        assert!(play_enabled(TimerState::Finished));
        assert!(!pause_enabled(TimerState::Finished));
    }

    #[test]
    fn timer_button_states_allow_reset_before_the_first_tick() {
        let _guard = TIMER_TEST_LOCK.lock().unwrap();
        let initial = initial_remaining_seconds();
        assert!(play_enabled(TimerState::NotStarted));
        assert!(!pause_enabled(TimerState::NotStarted));
        assert!(!reset_enabled(TimerState::NotStarted, initial));
        for state in [TimerState::Running, TimerState::Paused] {
            assert!(reset_enabled(state, initial));
            assert_eq!(play_enabled(state), state != TimerState::Running);
            assert_eq!(pause_enabled(state), state == TimerState::Running);
        }
    }
}
