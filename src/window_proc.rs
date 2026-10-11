use std::{
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::about::show_about_window;
use crate::app_state::PetPosition;
use crate::asset::{CharacterAnimations, load_character};
use crate::countdown::{Countdown, TimerState};
use crate::gdi::GdiPlus;
use crate::pet_window::{
    PET_COMMAND_ABOUT, PET_COMMAND_ACKNOWLEDGE, PET_COMMAND_EXIT, PET_COMMAND_HIDDEN, PET_COMMAND_SETTINGS,
    PET_COMMAND_SHOW_MAIN, PET_COMMAND_START, PetWindow, WM_PET_COMMAND,
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
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetCursor, SetTimer, SetWindowLongPtrW,
        SetWindowPos, SetWindowTextW, ShowWindow, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN,
        WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR,
        WM_SETTINGCHANGE, WM_SIZE, WM_THEMECHANGED, WM_TIMER, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
    },
};

pub const TIMER_ID: usize = 1;
const DEFAULT_INITIAL_REMAINING_SECONDS: u32 = 20 * 60;
const AUTO_HIDE_CHECKBOX_ID: usize = 3;

#[derive(Clone)]
pub struct WindowState {
    pub countdown: Countdown,
    pub auto_hide_on_start: bool,
    pub auto_hide_checkbox: HWND,
    pub language: Language,
    pub theme: Theme,
    pub tray_icon: TrayIcon,
    pub tray_when_close: bool,
    pub pet_window: Option<Rc<PetWindow>>,
    pub gdi_plus: Rc<GdiPlus>,
    pub character: String,
    pub pet_position: Option<PetPosition>,
    pub components: Vec<Rc<dyn Component>>,
    pub common_gui_font: Option<HFONT>,
    pub settings_button: HWND,
    pub settings_button_hovered: bool,
    pub about_button: HWND,
    pub about_button_hovered: bool,
    pub settings_panel: HWND,
    pub timer_panel: HWND,
    pub control_buttons: [HWND; 3],
}

// Painting can happen before WindowState is attached; this is only a startup fallback.
static INITIAL_REMAINING_SECONDS: AtomicU32 = AtomicU32::new(DEFAULT_INITIAL_REMAINING_SECONDS);

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
    let Some(state) = window_state(hwnd) else { return };
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
        with_window_state_mut(hwnd, |current| {
            current.auto_hide_on_start = checked;
        });
        update_settings_panel_auto_hide(state.settings_panel, checked);
    }
}

pub fn set_initial_remaining_seconds(seconds: u32) {
    INITIAL_REMAINING_SECONDS.store(seconds, Ordering::Relaxed);
}

fn apply_timer_period(hwnd: HWND, seconds: u32) {
    let Some(Some(previous_remaining)) = with_window_state_mut(hwnd, |state| {
        let previous = state.countdown.displayed_remaining();
        state.countdown.set_period(seconds).then_some(previous)
    }) else {
        return;
    };
    unsafe {
        invalidate_countdown(hwnd, previous_remaining);
        invalidate_countdown(hwnd, seconds);
    }
    let _ = sync_control_button_enabled(hwnd);
}

pub fn remaining_seconds(hwnd: HWND) -> u32 {
    window_state(hwnd).map_or_else(initial_remaining_seconds, |state| state.countdown.displayed_remaining())
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
                let Some(Some((previous_remaining, finished))) = with_window_state_mut(hwnd, |state| {
                    if state.countdown.state() != TimerState::Running {
                        return None;
                    }
                    let previous = state.countdown.remaining();
                    Some((previous, state.countdown.tick()))
                }) else {
                    return LRESULT(0);
                };
                if finished {
                    stop_timer(hwnd);
                    show_pet_reminder(hwnd);
                }

                let _ = sync_control_button_enabled(hwnd);

                let displayed_remaining = remaining_seconds(hwnd);
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
                PET_COMMAND_HIDDEN => release_hidden_pet(hwnd, HWND(lparam.0 as *mut _)),
                PET_COMMAND_START => activate_button(hwnd, ControlButton::Play),
                PET_COMMAND_SETTINGS => open_settings(hwnd),
                PET_COMMAND_ABOUT => open_about(hwnd),
                PET_COMMAND_SHOW_MAIN => crate::tray_icon::show_main_window(hwnd),
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
            if let Some(mut state) = window_state(hwnd) {
                let cursor_window = HWND(wparam.0 as _);
                with_window_state_mut(hwnd, |current| {
                    current.settings_button_hovered = cursor_window == state.settings_button;
                    current.about_button_hovered = cursor_window == state.about_button;
                });
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
            let font = common_gui_font(dpi, chinese);
            with_window_state_mut(hwnd, |state| state.common_gui_font = font);
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
    let Some((previous_remaining, displayed_remaining)) = with_window_state_mut(hwnd, |state| {
        let previous = state.countdown.remaining();
        match button {
            ControlButton::Play => state.countdown.start(),
            ControlButton::Pause => state.countdown.pause(),
            ControlButton::Reset => state.countdown.reset(),
        }
        (previous, state.countdown.displayed_remaining())
    }) else {
        return;
    };

    match button {
        ControlButton::Play => {
            let _ = hide_pet_animated(hwnd);
            if previous_remaining == 0 {
                unsafe {
                    invalidate_countdown(hwnd, previous_remaining);
                    invalidate_countdown(hwnd, displayed_remaining);
                }
            }
            start_timer(hwnd);
        }
        ControlButton::Pause => {
            hide_pet(hwnd);
            stop_timer(hwnd);
        }
        ControlButton::Reset => {
            stop_timer(hwnd);
            hide_pet(hwnd);
            unsafe {
                invalidate_countdown(hwnd, previous_remaining);
                invalidate_countdown(hwnd, displayed_remaining);
            }
        }
    }

    let _ = sync_control_button_enabled(hwnd);
}

fn sync_control_button_enabled(hwnd: HWND) -> windows::core::Result<()> {
    let Some(state) = window_state(hwnd) else {
        return Ok(());
    };
    let (play, pause, reset) = state.countdown.buttons();
    update_control_buttons_for(&state.control_buttons, play, pause, reset);
    let Some(mut state) = window_state(hwnd) else {
        return Ok(());
    };
    let result = state.tray_icon.update_tooltip(
        hwnd,
        crate::i18n::tray_tooltip(state.language, state.countdown.state() == TimerState::Running),
    );
    with_window_state_mut(hwnd, |current| {
        current.tray_icon = state.tray_icon;
    });
    result
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
    if let Err(error) = try_show_pet_reminder(hwnd) {
        show_pet_error(hwnd, &error);
    }
}

fn show_pet_error(hwnd: HWND, error: &windows::core::Error) {
    let text: Vec<u16> = error.to_string().encode_utf16().chain([0]).collect();
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let _ = MessageBoxW(
            Some(hwnd),
            windows::core::PCWSTR(text.as_ptr()),
            windows::core::w!("Stand Awhile"),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn try_show_pet_reminder(hwnd: HWND) -> windows::core::Result<()> {
    let Some(state) = window_state(hwnd) else {
        return Ok(());
    };
    if state.pet_window.is_none() {
        let animations = load_character(&state.character, &state.gdi_plus).map_err(|error| {
            windows::core::Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string())
        })?;
        let instance: HINSTANCE = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None)? }.into();
        let pet = PetWindow::create(
            instance,
            hwnd,
            animations,
            state.language,
            state.pet_position.clone(),
            crate::tray_menu_settings_text(state.language),
        )?;
        pet.set_language(state.language);
        pet.show_reminder()?;
        let pet = Rc::new(pet);
        let previous = with_window_state_mut(hwnd, |current| current.pet_window.replace(pet.clone()));
        drop(previous);
        return Ok(());
    }
    state.pet_window.as_ref().unwrap().show_reminder()
}

fn release_hidden_pet(hwnd: HWND, pet_hwnd: HWND) {
    let Some(state) = window_state(hwnd) else { return };
    if let Some(pet) = state.pet_window.as_ref() {
        if pet.hwnd() != pet_hwnd || unsafe { IsWindowVisible(pet_hwnd).as_bool() } {
            return;
        }
        let position = pet.position().ok();
        let released = with_window_state_mut(hwnd, |current| {
            if let Some(position) = position {
                current.pet_position = Some(position);
            }
            current.pet_window.take()
        });
        drop(released);
    }
}

fn acknowledge_pet(hwnd: HWND) {
    let Some((previous_remaining, displayed_remaining)) = with_window_state_mut(hwnd, |state| {
        let previous = state.countdown.remaining();
        state.countdown.restart();
        (previous, state.countdown.displayed_remaining())
    }) else {
        return;
    };
    start_timer(hwnd);
    unsafe {
        invalidate_countdown(hwnd, previous_remaining);
        invalidate_countdown(hwnd, displayed_remaining);
    }
    let _ = sync_control_button_enabled(hwnd);
}

fn hide_pet(hwnd: HWND) {
    let pet_hwnd = window_state(hwnd).and_then(|state| state.pet_window).map(|pet| {
        pet.hide();
        pet.hwnd()
    });
    if let Some(pet_hwnd) = pet_hwnd {
        release_hidden_pet(hwnd, pet_hwnd);
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
    state.pet_window.as_ref().map_or(Ok(()), |pet| pet.hide_animated())
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

// Return owned data: callbacks may mutate or destroy the attached state.
pub(crate) fn window_state(hwnd: HWND) -> Option<WindowState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
    unsafe { raw.as_ref().cloned() }
}

// Only change local data here; never send messages, pump messages, or drop windows.
fn with_window_state_mut<R>(hwnd: HWND, update: impl FnOnce(&mut WindowState) -> R) -> Option<R> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
    unsafe { raw.as_mut().map(update) }
}

fn release_window_state(hwnd: HWND) {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if raw != 0 {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        drop(unsafe { Box::from_raw(raw as *mut WindowState) });
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
    crate::tray_icon::show_main_window(hwnd);
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
        with_window_state_mut(hwnd, |state| state.settings_panel = panel);
        if let Some(state) = window_state(hwnd) {
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
    let font = common_gui_font(dpi, language == Language::Chinese);
    with_window_state_mut(hwnd, |state| {
        state.language = language;
        state.auto_hide_on_start = config.auto_hide_on_start;
        state.tray_when_close = config.tray_when_close;
        state.theme = config.theme();
        state.common_gui_font = font;
    });
    if let Some(mut state) = window_state(hwnd) {
        update_settings_panel_auto_hide(state.settings_panel, config.auto_hide_on_start);
        set_auto_hide_checked(state.auto_hide_checkbox, config.auto_hide_on_start);
        set_settings_button_text(state.auto_hide_checkbox, crate::i18n::auto_hide_text(language));
        unsafe {
            let _ = InvalidateRect(Some(state.timer_panel), None, false);
        }
        refresh_theme(hwnd, state.theme);
        refresh_settings_panel_theme(state.settings_panel, &config);
        let _ = state.tray_icon.update_language(
            hwnd,
            crate::i18n::tray_tooltip(language, state.countdown.state() == TimerState::Running),
            crate::tray_menu_start_text(language),
            crate::tray_menu_show_text(language),
            crate::tray_menu_settings_text(language),
            crate::tray_menu_about_text(language),
            crate::tray_menu_exit_text(language),
        );
        with_window_state_mut(hwnd, |current| {
            current.tray_icon = state.tray_icon.clone();
        });
        if let Some(pet) = state.pet_window.as_ref() {
            pet.set_language(language);
        }
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
        set_settings_button_text(state.about_button, crate::about_button_text(language));
    }
    let result = window_state(hwnd).map(|mut state| {
        change_character(
            &mut state.character,
            state.pet_window.as_deref(),
            &config.character,
            || {
                load_character(&config.character, &state.gdi_plus).map_err(|error| {
                    windows::core::Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string())
                })
            },
        )?;
        with_window_state_mut(hwnd, |current| {
            current.character = state.character;
        });
        Ok::<_, windows::core::Error>(())
    });
    if let Some(Err(error)) = result {
        show_pet_error(hwnd, &error);
    }
    let settings_visible = window_state(hwnd)
        .map(|state| unsafe { IsWindowVisible(state.settings_panel).as_bool() })
        .unwrap_or(false);
    set_settings_button_text_for(hwnd, settings_visible);
}

fn change_character(
    current: &mut String,
    pet: Option<&PetWindow>,
    selected: &str,
    load: impl FnOnce() -> windows::core::Result<CharacterAnimations>,
) -> windows::core::Result<()> {
    if current == selected {
        return Ok(());
    }
    if let Some(pet) = pet {
        pet.set_animations(load()?)?;
    }
    // The saved selection may differ from the loaded character after a failure.
    *current = selected.to_owned();
    Ok(())
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

    #[test]
    fn layout_can_reenter_and_destroy_main_window_before_snapshot_is_released() {
        use std::cell::Cell;
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{PM_REMOVE, PeekMessageW, RegisterClassW, WM_QUIT, WNDCLASSW},
        };

        struct ReentrantComponent {
            hwnd: HWND,
            dropped: Rc<Cell<bool>>,
        }
        impl Component for ReentrantComponent {
            fn layout(&self, parent: HWND, _: HDC) -> windows::core::Result<()> {
                unsafe {
                    SendMessageW(parent, WM_TIMER, Some(WPARAM(TIMER_ID)), None);
                }
                assert_eq!(remaining_seconds(parent), 2);
                unsafe {
                    DestroyWindow(parent)?;
                }
                assert!(window_state(parent).is_none());
                assert!(!self.dropped.get());
                Ok(())
            }
            fn invalidate(&self) {}
        }
        impl Drop for ReentrantComponent {
            fn drop(&mut self) {
                assert_eq!(unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) }, 0);
                self.dropped.set(true);
            }
        }

        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: windows::core::w!("StandAwhileMainStateReentrancyTest"),
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&class), 0);
            let hwnd = CreateWindowExW(
                Default::default(),
                class.lpszClassName,
                windows::core::w!(""),
                Default::default(),
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
            let mut countdown = Countdown::new(3);
            countdown.start();
            attach_test_state(hwnd, instance, countdown, false);
            let dropped = Rc::new(Cell::new(false));
            with_window_state_mut(hwnd, |state| {
                state.components.push(Rc::new(ReentrantComponent {
                    hwnd,
                    dropped: dropped.clone(),
                }));
            })
            .unwrap();
            layout_window_state(hwnd).unwrap();
            assert!(dropped.get());
            assert!(!IsWindow(Some(hwnd)).as_bool());
            assert!(window_state(hwnd).is_none());
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE);
        }
    }

    fn attach_test_state(hwnd: HWND, instance: HINSTANCE, countdown: Countdown, auto_hide_on_start: bool) {
        use windows::Win32::UI::WindowsAndMessaging::{IDI_APPLICATION, LoadIconW};
        let tray_icon = TrayIcon::create(
            hwnd,
            unsafe { LoadIconW(None, IDI_APPLICATION).unwrap() },
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
            unsafe {
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
            }
            .unwrap()
        });
        attach_window_state(
            hwnd,
            WindowState {
                countdown,
                auto_hide_on_start,
                auto_hide_checkbox: controls[0],
                language: Language::English,
                theme: Theme::System,
                tray_icon,
                tray_when_close: true,
                pet_window: None,
                gdi_plus: Rc::new(GdiPlus::new().unwrap()),
                character: "cat".to_owned(),
                pet_position: None,
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
    }

    #[test]
    fn character_selection_changes_only_after_successful_load_and_replacement() {
        use crate::{
            animation::{AnimationClip, Frame, LoopMode},
            asset::{PreparedAnimation, PreparedFrame},
        };
        use std::time::Duration;
        let animation = PreparedAnimation {
            clip: AnimationClip::new(vec![Frame { id: 0 }], Duration::from_millis(120), LoopMode::Loop).unwrap(),
            frames: vec![PreparedFrame {
                width: 1,
                height: 1,
                pixels: vec![0, 0, 0, 255],
            }],
        };
        let original = CharacterAnimations {
            idle: animation.clone(),
            walk: animation.clone(),
            jump: animation,
        };
        let instance = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
                .unwrap()
                .into()
        };
        let pet = PetWindow::create(
            instance,
            HWND::default(),
            original.clone(),
            Language::English,
            None,
            "Settings",
        )
        .unwrap();
        pet.show_reminder().unwrap();
        let mut character = "cat".to_owned();
        assert!(
            change_character(&mut character, Some(&pet), "dog", || Err(
                windows::core::Error::from_win32()
            ))
            .is_err()
        );
        assert_eq!(character, "cat");
        let mut invalid = original.clone();
        invalid.jump.frames[0].pixels.clear();
        assert!(change_character(&mut character, Some(&pet), "dog", || Ok(invalid)).is_err());
        assert_eq!(character, "cat");
        pet.show_reminder().unwrap();
        change_character(&mut character, Some(&pet), "dog", || Ok(original)).unwrap();
        assert_eq!(character, "dog");
        change_character(&mut character, Some(&pet), "dog", || {
            panic!("same character must not reload")
        })
        .unwrap();
        change_character(&mut character, None, "cat", || {
            panic!("hidden pet must not decode frames")
        })
        .unwrap();
        assert_eq!(character, "cat");
    }

    #[test]
    fn background_start_counts_down_and_reminds_with_either_auto_hide_preference() {
        use crate::startup::StartupMode;
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{PM_REMOVE, PeekMessageW, RegisterClassW, WM_QUIT, WNDCLASSW},
        };

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
                attach_test_state(hwnd, instance, Countdown::new(2), auto_hide_on_start);

                mode.finish(hwnd);
                assert_eq!(window_state(hwnd).unwrap().countdown.state(), TimerState::Running);
                assert_eq!(
                    window_state(hwnd).unwrap().tray_icon.test_tooltip(),
                    "Stand Awhile (Timing)"
                );
                activate_button(hwnd, ControlButton::Pause);
                assert_eq!(window_state(hwnd).unwrap().tray_icon.test_tooltip(), "Stand Awhile");
                activate_button(hwnd, ControlButton::Play);
                assert_eq!(
                    window_state(hwnd).unwrap().tray_icon.test_tooltip(),
                    "Stand Awhile (Timing)"
                );
                activate_button(hwnd, ControlButton::Reset);
                assert_eq!(window_state(hwnd).unwrap().tray_icon.test_tooltip(), "Stand Awhile");
                activate_button(hwnd, ControlButton::Play);
                assert_eq!(remaining_seconds(hwnd), 2);
                assert!(!IsWindowVisible(hwnd).as_bool());
                assert!(window_state(hwnd).unwrap().pet_window.is_none());

                SendMessageW(hwnd, WM_TIMER, Some(WPARAM(TIMER_ID)), None);
                assert_eq!(remaining_seconds(hwnd), 1);
                crate::startup::handle_existing_instance(true).unwrap();
                assert!(!IsWindowVisible(hwnd).as_bool());
                assert!(window_state(hwnd).unwrap().pet_window.is_none());
                assert_eq!(window_state(hwnd).unwrap().countdown.state(), TimerState::Running);
                assert_eq!(remaining_seconds(hwnd), 1);
                SendMessageW(
                    hwnd,
                    WM_COMMAND,
                    Some(WPARAM(crate::tray_icon::TRAY_MENU_SHOW_ID)),
                    None,
                );
                assert!(IsWindowVisible(hwnd).as_bool());
                assert_eq!(remaining_seconds(hwnd), 1);
                assert_eq!(window_state(hwnd).unwrap().countdown.state(), TimerState::Running);
                assert!(window_state(hwnd).unwrap().pet_window.is_none());

                SendMessageW(hwnd, WM_TIMER, Some(WPARAM(TIMER_ID)), None);
                assert_eq!(window_state(hwnd).unwrap().countdown.state(), TimerState::Finished);
                assert_eq!(window_state(hwnd).unwrap().tray_icon.test_tooltip(), "Stand Awhile");
                assert_eq!(window_state(hwnd).unwrap().countdown.remaining(), 0);
                let pet_hwnd = window_state(hwnd).unwrap().pet_window.as_ref().unwrap().test_hwnd();
                assert!(IsWindowVisible(pet_hwnd).as_bool());
                let _ = ShowWindow(hwnd, SW_HIDE);
                crate::startup::handle_existing_instance(true).unwrap();
                assert!(!IsWindowVisible(hwnd).as_bool());
                assert!(IsWindowVisible(pet_hwnd).as_bool());
                assert_eq!(window_state(hwnd).unwrap().countdown.state(), TimerState::Finished);
                assert_eq!(window_state(hwnd).unwrap().countdown.remaining(), 0);
                // A queued hide notification must not release a visible reminder.
                release_hidden_pet(hwnd, pet_hwnd);
                assert!(window_state(hwnd).unwrap().pet_window.is_some());
                let position = window_state(hwnd)
                    .unwrap()
                    .pet_window
                    .as_ref()
                    .unwrap()
                    .position()
                    .unwrap();
                hide_pet(hwnd);
                assert!(window_state(hwnd).unwrap().pet_window.is_none());
                assert!(!IsWindow(Some(pet_hwnd)).as_bool());
                assert_eq!(window_state(hwnd).unwrap().pet_position.as_ref(), Some(&position));
                // The next reminder decodes only the newly selected character.
                with_window_state_mut(hwnd, |state| state.character = "dog".to_owned()).unwrap();
                try_show_pet_reminder(hwnd).unwrap();
                let pet_hwnd = window_state(hwnd).unwrap().pet_window.as_ref().unwrap().test_hwnd();
                assert!(IsWindowVisible(pet_hwnd).as_bool());
                hide_pet_animated(hwnd).unwrap();
                assert!(window_state(hwnd).unwrap().pet_window.is_some());
                window_state(hwnd)
                    .unwrap()
                    .pet_window
                    .as_ref()
                    .unwrap()
                    .finish_hide_for_test();
                assert!(!IsWindowVisible(pet_hwnd).as_bool());
                let mut hidden_message = MSG::default();
                assert!(
                    PeekMessageW(
                        &mut hidden_message,
                        Some(hwnd),
                        WM_PET_COMMAND,
                        WM_PET_COMMAND,
                        PM_REMOVE
                    )
                    .as_bool()
                );
                assert_eq!(hidden_message.wParam.0, PET_COMMAND_HIDDEN);
                windows::Win32::UI::WindowsAndMessaging::DispatchMessageW(&hidden_message);
                assert!(window_state(hwnd).unwrap().pet_window.is_none());
                assert!(!IsWindow(Some(pet_hwnd)).as_bool());
                DestroyWindow(hwnd).unwrap();
                let mut message = MSG::default();
                let _ = PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE);
            }
        }
    }

    #[test]
    fn restore_commands_show_hidden_or_minimized_main_window_without_changing_timer() {
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                CreateWindowExW, IsIconic, PM_REMOVE, PeekMessageW, RegisterClassW, SW_SHOWMINNOACTIVE, WM_QUIT,
                WNDCLASSW, WS_OVERLAPPEDWINDOW,
            },
        };

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
            attach_test_state(hwnd, instance, Countdown::new(1200), false);
            // Supply an isolated panel so restore commands never load the user's config.
            let panel = create_settings_panel(hwnd, instance, Config::default(), None).unwrap();
            with_window_state_mut(hwnd, |state| state.settings_panel = panel).unwrap();
            for (state, remaining) in [
                (TimerState::Running, 731),
                (TimerState::Paused, 412),
                (TimerState::Finished, 0),
            ] {
                let mut countdown = Countdown::new(1200);
                countdown.start();
                for _ in 0..(1200 - remaining) {
                    countdown.tick();
                }
                if state == TimerState::Paused {
                    countdown.pause();
                }
                with_window_state_mut(hwnd, |state| state.countdown = countdown).unwrap();
                apply_timer_period(hwnd, 1200);
                assert_eq!(window_state(hwnd).unwrap().countdown, countdown);

                for presentation in [SW_HIDE, SW_SHOWMINNOACTIVE] {
                    for (message, command) in [
                        (crate::single_instance::WM_SHOW_EXISTING_INSTANCE, 0),
                        (WM_PET_COMMAND, PET_COMMAND_SHOW_MAIN),
                        (WM_PET_COMMAND, PET_COMMAND_SETTINGS),
                        (WM_COMMAND, TRAY_MENU_SETTINGS_ID),
                    ] {
                        let _ = ShowWindow(hwnd, presentation);
                        assert!(!IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool());
                        SendMessageW(hwnd, message, Some(WPARAM(command)), None);
                        assert!(IsWindowVisible(hwnd).as_bool());
                        assert!(!IsIconic(hwnd).as_bool(), "message={message}, command={command}");
                        assert_eq!(window_state(hwnd).unwrap().countdown.state(), state);
                        assert_eq!(window_state(hwnd).unwrap().countdown.remaining(), remaining);
                    }
                }
                apply_timer_period(hwnd, 600);
                assert_eq!(window_state(hwnd).unwrap().countdown.state(), state);
                assert_eq!(window_state(hwnd).unwrap().countdown.remaining(), 600);
                assert_eq!(remaining_seconds(hwnd), 600);
            }
            DestroyWindow(hwnd).unwrap();
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE);
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
}
