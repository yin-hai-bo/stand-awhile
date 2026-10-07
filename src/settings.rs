use std::sync::OnceLock;

use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            CreateFontIndirectW, CreateSolidBrush, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawFocusRect,
            DrawTextW, FillRect, GetObjectW, HBRUSH, HDC, HFONT, HGDIOBJ, InvalidateRect, LOGFONTW, SelectObject,
            SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::{
            Controls::{BST_CHECKED, DRAWITEMSTRUCT, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED, WM_MOUSELEAVE},
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTORADIOBUTTON, BS_OWNERDRAW, CREATESTRUCTW, CreateWindowExW,
                DefWindowProcW, DestroyWindow, EN_CHANGE, EN_KILLFOCUS, ES_NUMBER, GWLP_USERDATA, GetWindowLongPtrW,
                GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, IDC_HAND, LoadCursorW, RegisterClassExW,
                SendMessageW, SetCursor, SetWindowLongPtrW, SetWindowTextW, WM_CLOSE, WM_COMMAND, WM_CREATE,
                WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_MOUSEMOVE,
                WM_NCACTIVATE, WM_NCDESTROY, WM_SETCURSOR, WM_SETFOCUS, WM_SETFONT, WNDCLASSEXW, WS_CHILD,
                WS_EX_CONTROLPARENT, WS_GROUP, WS_TABSTOP, WS_VISIBLE,
            },
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::config::Config;
use crate::ui::theme::{Theme, apply_window_caption_color, apply_window_dark_mode, is_dark_mode};

const SETTINGS_CLASS: PCWSTR = w!("YHB-StandAwhileSettings");
pub const SETTINGS_BUTTON_RESERVED_HEIGHT: i32 = 64;
pub const SETTINGS_APPLIED_ID: usize = 5002;
pub const SETTINGS_CHANGED_ID: usize = 5004;
pub const SETTINGS_CLOSED_ID: usize = 5003;
pub const SETTINGS_BUTTON_ID: usize = 1;
pub const ABOUT_BUTTON_ID: usize = 2;
const PERIOD_ID: usize = 10;
const CHARACTER_CAT_ID: usize = 11;
const CHARACTER_DOG_ID: usize = 12;
const LANGUAGE_AUTO_ID: usize = 13;
const LANGUAGE_ZH_ID: usize = 14;
const LANGUAGE_EN_ID: usize = 15;
const THEME_SYSTEM_ID: usize = 16;
const THEME_LIGHT_ID: usize = 17;
const THEME_DARK_ID: usize = 18;
const CLOSE_EXIT_ID: usize = 19;
const CLOSE_TRAY_ID: usize = 20;
const AUTO_HIDE_YES_ID: usize = 21;
const AUTO_HIDE_NO_ID: usize = 22;
const STARTUP_YES_ID: usize = 23;
const STARTUP_NO_ID: usize = 24;
const RESET_DEFAULTS_ID: usize = 25;
const SETTINGS_TITLE_COUNT: usize = 7;

const BASE_GRID_LEFT: i32 = 72;
const BASE_TITLE_WIDTH: i32 = 196;
const BASE_COLUMN_GAP: i32 = 12;
const BASE_CONTROL_WIDTH: i32 = 280;
const BASE_GRID_TOP: i32 = 56;
const BASE_ROW_HEIGHT: i32 = 32;
const BASE_ROW_GAP: i32 = 16;

struct SettingsState {
    parent: HWND,
    embedded: bool,
    instance: HINSTANCE,
    config: Config,
    period: HWND,
    period_changed: bool,
    character: [HWND; 2],
    language: [HWND; 3],
    theme: [HWND; 3],
    close_behavior: [HWND; 2],
    auto_hide: [HWND; 2],
    launch_at_startup: [HWND; 2],
    reset_defaults: HWND,
    reset_defaults_hovered: bool,
    font: Option<HFONT>,
    title_font: Option<HFONT>,
    font_controls: Vec<HWND>,
    dark_mode: bool,
    background_brush: HBRUSH,
    control_brush: HBRUSH,
}

impl Drop for SettingsState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.background_brush.0));
            let _ = DeleteObject(HGDIOBJ(self.control_brush.0));
            if let Some(font) = self.title_font.take() {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
        #[cfg(test)]
        STATE_DROPS.with(|count| count.set(count.get() + 1));
    }
}

#[cfg(test)]
thread_local! {
    static STATE_DROPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn create_settings_panel(parent: HWND, instance: HINSTANCE, config: Config, font: Option<HFONT>) -> Result<HWND> {
    register_class(instance)?;
    let dark_mode = is_settings_dark_mode(&config);
    let state = Box::new(SettingsState {
        parent,
        embedded: true,
        instance,
        config,
        period: HWND::default(),
        period_changed: false,
        character: [HWND::default(); 2],
        language: [HWND::default(); 3],
        theme: [HWND::default(); 3],
        close_behavior: [HWND::default(); 2],
        auto_hide: [HWND::default(); 2],
        launch_at_startup: [HWND::default(); 2],
        reset_defaults: HWND::default(),
        reset_defaults_hovered: false,
        font,
        title_font: None,
        font_controls: Vec::new(),
        dark_mode,
        background_brush: HBRUSH::default(),
        control_brush: HBRUSH::default(),
    });
    let mut state = Some(state);
    unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            SETTINGS_CLASS,
            w!("Settings"),
            WS_CHILD | WS_VISIBLE | windows::Win32::UI::WindowsAndMessaging::WS_CLIPSIBLINGS,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance),
            Some((&mut state as *mut Option<Box<SettingsState>>).cast()),
        )
    }
}

pub fn resize_settings_panel(hwnd: HWND, parent: HWND) -> Result<()> {
    let mut rect = RECT::default();
    let dpi = unsafe { GetDpiForWindow(parent) }.max(96);
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetClientRect(parent, &mut rect)?;
        windows::Win32::UI::WindowsAndMessaging::MoveWindow(hwnd, 0, 0, rect.right, rect.bottom, true)?;
    }
    layout_settings_panel(hwnd, dpi);
    Ok(())
}

pub fn focus_settings_panel(hwnd: HWND) {
    unsafe {
        let _ = SetFocus(Some(hwnd));
    }
}

pub fn layout_settings_panel(hwnd: HWND, dpi: u32) {
    let Some(state) = state_mut(hwnd) else {
        return;
    };
    let scale = |value: i32| value * dpi as i32 / 96;
    let title_x = scale(BASE_GRID_LEFT);
    let title_width = scale(BASE_TITLE_WIDTH);
    let control_x = scale(BASE_GRID_LEFT + BASE_TITLE_WIDTH + BASE_COLUMN_GAP);
    let control_width = scale(BASE_CONTROL_WIDTH);
    let row_height = scale(BASE_ROW_HEIGHT);
    let row_step = scale(BASE_ROW_HEIGHT + BASE_ROW_GAP);
    let grid_top = scale(BASE_GRID_TOP);

    let labels = [
        state.font_controls[0],
        state.font_controls[1],
        state.font_controls[2],
        state.font_controls[3],
        state.font_controls[4],
        state.font_controls[5],
        state.font_controls[6],
    ];
    for (index, label) in labels.into_iter().enumerate() {
        let y = grid_top + index as i32 * row_step;
        move_control(label, title_x, y, title_width, row_height);
    }
    let edit_height = scale(24);
    let edit_y = grid_top + (row_height - edit_height) / 2;
    move_control(state.period, control_x, edit_y, control_width, edit_height);
    layout_radio_group(
        &state.character,
        control_x,
        grid_top + row_step,
        control_width,
        row_height,
        dpi,
    );
    layout_radio_group(
        &state.language,
        control_x,
        grid_top + 2 * row_step,
        control_width,
        row_height,
        dpi,
    );
    layout_radio_group(
        &state.theme,
        control_x,
        grid_top + 3 * row_step,
        control_width,
        row_height,
        dpi,
    );
    layout_radio_group(
        &state.close_behavior,
        control_x,
        grid_top + 4 * row_step,
        control_width,
        row_height,
        dpi,
    );
    layout_radio_group(
        &state.auto_hide,
        control_x,
        grid_top + 5 * row_step,
        control_width,
        row_height,
        dpi,
    );
    layout_radio_group(
        &state.launch_at_startup,
        control_x,
        grid_top + 6 * row_step,
        control_width,
        row_height,
        dpi,
    );
    let mut rect = RECT::default();
    if unsafe { windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect) }.is_ok() {
        // Match the footer Back button, with a 12-DIP gap on its left.
        move_control(
            state.reset_defaults,
            rect.right - scale(12 + 96 + 12 + 128),
            rect.bottom - scale(32 + 32),
            scale(128),
            scale(32),
        );
    }
}

fn layout_radio_group(radios: &[HWND], x: i32, y: i32, width: i32, height: i32, dpi: u32) {
    let gap = 8 * dpi as i32 / 96;
    let item_width = (width - gap * (radios.len() as i32 - 1)) / radios.len() as i32;
    for (index, radio) in radios.iter().enumerate() {
        move_control(*radio, x + index as i32 * (item_width + gap), y, item_width, height);
    }
}

fn move_control(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::MoveWindow(hwnd, x, y, width, height, true);
    }
}

fn draw_period_underline(hwnd: HWND, hdc: HDC, dark_mode: bool) {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let scale = |value: i32| value * dpi as i32 / 96;
    let y = scale(BASE_GRID_TOP + BASE_ROW_HEIGHT) - 1;
    let rect = RECT {
        left: scale(BASE_GRID_LEFT + BASE_TITLE_WIDTH + BASE_COLUMN_GAP),
        top: y,
        right: scale(BASE_GRID_LEFT + BASE_TITLE_WIDTH + BASE_COLUMN_GAP + BASE_CONTROL_WIDTH),
        bottom: y + 1,
    };
    let color = if dark_mode {
        COLORREF(0x00606060)
    } else {
        COLORREF(0x00A0A0A0)
    };
    unsafe {
        let brush = CreateSolidBrush(color);
        let _ = FillRect(hdc, &rect, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

fn register_class(instance: HINSTANCE) -> Result<()> {
    static REGISTERED: OnceLock<std::result::Result<(), i32>> = OnceLock::new();
    match REGISTERED.get_or_init(|| {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(settings_window_proc),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
            lpszClassName: SETTINGS_CLASS,
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&class) };
        if atom == 0 {
            Err(Error::from_win32().code().0)
        } else {
            Ok(())
        }
    }) {
        Ok(()) => Ok(()),
        Err(code) => Err(Error::new(
            windows::core::HRESULT(*code),
            "could not register Settings window class",
        )),
    }
}

unsafe extern "system" fn settings_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE => {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let Some(state) = (unsafe { (create.lpCreateParams as *mut Option<Box<SettingsState>>).as_mut() }) else {
                return LRESULT(0);
            };
            let Some(state) = state.take() else {
                return LRESULT(0);
            };
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize) };
            LRESULT(1)
        }
        WM_CREATE => {
            if let Some(state) = state_mut(hwnd) {
                create_controls(hwnd, state);
                let _ = apply_window_dark_mode(hwnd, state.dark_mode);
            }
            LRESULT(0)
        }
        WM_NCACTIVATE => {
            if let Some(state) = state_mut(hwnd) {
                apply_window_caption_color(hwnd, state.dark_mode, wparam.0 != 0);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_ERASEBKGND => {
            if let Some(state) = state_mut(hwnd) {
                let hdc = HDC(wparam.0 as _);
                let mut rect = RECT::default();
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
                    let _ = FillRect(hdc, &rect, state.background_brush);
                }
                draw_period_underline(hwnd, hdc, state.dark_mode);
                return LRESULT(1);
            }
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX => {
            if let Some(state) = state_mut(hwnd) {
                let hdc = HDC(wparam.0 as _);
                let title = HWND(lparam.0 as _);
                let text = if state.font_controls[..SETTINGS_TITLE_COUNT].contains(&title) {
                    if state.dark_mode {
                        COLORREF(0x0066D1FF)
                    } else {
                        COLORREF(0x008E3A5B)
                    }
                } else if state.dark_mode {
                    COLORREF(0x00FAFAFA)
                } else {
                    COLORREF(0x00202020)
                };
                let background = if state.dark_mode {
                    COLORREF(0x00202020)
                } else {
                    COLORREF(0x00F0F0F0)
                };
                unsafe {
                    let _ = SetTextColor(hdc, text);
                    let _ = SetBkColor(hdc, background);
                }
                return LRESULT(state.control_brush.0 as isize);
            }
            LRESULT(0)
        }
        windows::Win32::UI::WindowsAndMessaging::WM_DRAWITEM => {
            if let Some(state) = state_mut(hwnd) {
                let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
                if item.CtlID as usize == RESET_DEFAULTS_ID {
                    draw_settings_button(item, state.dark_mode, state.reset_defaults_hovered, state.font);
                    return LRESULT(1);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as usize;
            let notification = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if id == PERIOD_ID && notification == EN_CHANGE {
                if let Some(state) = state_mut(hwnd)
                    && !state.period.is_invalid()
                {
                    state.period_changed = true;
                }
            }
            if id == PERIOD_ID && notification == EN_KILLFOCUS {
                if let Some(state) = state_mut(hwnd)
                    && state.period_changed
                {
                    normalize_period(state.period);
                }
            }
            if notification == BN_CLICKED {
                match id {
                    RESET_DEFAULTS_ID => {
                        let result =
                            Config::load().and_then(|previous| reset_to_defaults(hwnd, previous, Config::save));
                        if let Err(error) = result {
                            if let Some(state) = state_mut(hwnd) {
                                let message = match state.config.language() {
                                    crate::i18n::Language::Chinese => format!("重置为默认值失败。\n{error}"),
                                    crate::i18n::Language::English => format!("Could not reset to defaults.\n{error}"),
                                };
                                unsafe {
                                    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
                                    let _ = MessageBoxW(
                                        Some(hwnd),
                                        PCWSTR(wide(&message).as_ptr()),
                                        PCWSTR(wide(crate::i18n::main_window_title(state.config.language())).as_ptr()),
                                        MB_OK | MB_ICONERROR,
                                    );
                                }
                            }
                        } else if let Some(state) = state_mut(hwnd) {
                            unsafe {
                                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                    Some(state.parent),
                                    WM_COMMAND,
                                    WPARAM(SETTINGS_CHANGED_ID),
                                    LPARAM(0),
                                );
                            }
                        }
                    }
                    STARTUP_YES_ID | STARTUP_NO_ID => {
                        if let Some(state) = state_mut(hwnd) {
                            let result = Config::load().and_then(|previous| {
                                apply_startup_preference(state, previous, Config::save, crate::autostart::sync)
                            });
                            if let Err(error) = result {
                                restore_startup_radio(state);
                                crate::autostart::show_error(hwnd, state.config.language(), &error);
                            }
                        }
                    }
                    CHARACTER_CAT_ID | CHARACTER_DOG_ID | LANGUAGE_AUTO_ID | LANGUAGE_ZH_ID | LANGUAGE_EN_ID
                    | THEME_SYSTEM_ID | THEME_LIGHT_ID | THEME_DARK_ID | CLOSE_EXIT_ID | CLOSE_TRAY_ID
                    | AUTO_HIDE_YES_ID | AUTO_HIDE_NO_ID => {
                        if let Some(state) = state_mut(hwnd) {
                            if let Ok(config) = Config::load().map(|config| read_config_without_period(state, config)) {
                                if config.save().is_ok() {
                                    state.config = config;
                                    unsafe {
                                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                                            Some(state.parent),
                                            WM_COMMAND,
                                            WPARAM(SETTINGS_CHANGED_ID),
                                            LPARAM(0),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_SETFOCUS => {
            if let Some(state) = state_mut(hwnd) {
                unsafe {
                    let _ = SetFocus(Some(state.period));
                }
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            if let Some(state) = state_mut(hwnd)
                && state.embedded
            {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(state.parent),
                        WM_COMMAND,
                        WPARAM(SETTINGS_CLOSED_ID),
                        LPARAM(0),
                    );
                }
            }
            let raw = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            if raw != 0 {
                drop(unsafe { Box::from_raw(raw as *mut SettingsState) });
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn state_mut(hwnd: HWND) -> Option<&'static mut SettingsState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsState;
    unsafe { raw.as_mut() }
}

fn create_controls(hwnd: HWND, state: &mut SettingsState) {
    let background = if state.dark_mode {
        COLORREF(0x00202020)
    } else {
        COLORREF(0x00F0F0F0)
    };
    state.background_brush = unsafe { CreateSolidBrush(background) };
    state.control_brush = unsafe { CreateSolidBrush(background) };
    let chinese = state.config.language() == crate::i18n::Language::Chinese;
    let mut font_controls = Vec::new();
    state.period = create_edit(
        hwnd,
        state.instance,
        PERIOD_ID,
        &period_minutes(state.config.period).to_string(),
        280,
        80,
    );
    state.character = [
        create_radio(
            hwnd,
            state.instance,
            CHARACTER_CAT_ID,
            if chinese { "小猫" } else { "cat" },
            true,
        ),
        create_radio(
            hwnd,
            state.instance,
            CHARACTER_DOG_ID,
            if chinese { "小狗" } else { "dog" },
            false,
        ),
    ];
    state.language = [
        create_radio(
            hwnd,
            state.instance,
            LANGUAGE_AUTO_ID,
            if chinese { "自动" } else { "Auto" },
            true,
        ),
        create_radio(hwnd, state.instance, LANGUAGE_ZH_ID, "中文", false),
        create_radio(hwnd, state.instance, LANGUAGE_EN_ID, "English", false),
    ];
    state.theme = [
        create_radio(
            hwnd,
            state.instance,
            THEME_SYSTEM_ID,
            if chinese { "系统" } else { "System" },
            true,
        ),
        create_radio(
            hwnd,
            state.instance,
            THEME_LIGHT_ID,
            if chinese { "浅色" } else { "Light" },
            false,
        ),
        create_radio(
            hwnd,
            state.instance,
            THEME_DARK_ID,
            if chinese { "深色" } else { "Dark" },
            false,
        ),
    ];
    state.close_behavior = [
        create_radio(
            hwnd,
            state.instance,
            CLOSE_EXIT_ID,
            if chinese { "退出程序" } else { "Exit program" },
            true,
        ),
        create_radio(
            hwnd,
            state.instance,
            CLOSE_TRAY_ID,
            if chinese {
                "缩小为托盘图标"
            } else {
                "Minimize to tray"
            },
            false,
        ),
    ];
    state.auto_hide = [
        create_radio(
            hwnd,
            state.instance,
            AUTO_HIDE_YES_ID,
            if chinese { "是" } else { "Yes" },
            true,
        ),
        create_radio(
            hwnd,
            state.instance,
            AUTO_HIDE_NO_ID,
            if chinese { "否" } else { "No" },
            false,
        ),
    ];
    state.launch_at_startup = [
        create_radio(
            hwnd,
            state.instance,
            STARTUP_YES_ID,
            if chinese { "是" } else { "Yes" },
            true,
        ),
        create_radio(
            hwnd,
            state.instance,
            STARTUP_NO_ID,
            if chinese { "否" } else { "No" },
            false,
        ),
    ];
    state.reset_defaults = create_button_with_style(
        hwnd,
        state.instance,
        RESET_DEFAULTS_ID,
        reset_defaults_text(state.config.language()),
        BS_OWNERDRAW as u32 | WS_GROUP.0,
        0,
        0,
        1,
        1,
    );
    unsafe {
        let _ = SetWindowSubclass(
            state.reset_defaults,
            Some(reset_defaults_subclass),
            RESET_DEFAULTS_ID,
            hwnd.0 as usize,
        );
    }
    let label = create_static(
        hwnd,
        state.instance,
        if chinese {
            "倒计数分钟数："
        } else {
            "Countdown minutes:"
        },
        72,
        84,
    );
    font_controls.push(label);
    let label = create_static(hwnd, state.instance, if chinese { "桌宠：" } else { "Pet:" }, 72, 132);
    font_controls.push(label);
    let label = create_static(
        hwnd,
        state.instance,
        if chinese { "语言：" } else { "Language:" },
        72,
        180,
    );
    font_controls.push(label);
    let label = create_static(hwnd, state.instance, if chinese { "主题：" } else { "Theme:" }, 72, 228);
    font_controls.push(label);
    let label = create_static(
        hwnd,
        state.instance,
        if chinese {
            "关闭主窗口行为："
        } else {
            "Close behavior:"
        },
        72,
        276,
    );
    font_controls.push(label);
    let label = create_static(
        hwnd,
        state.instance,
        if chinese {
            "开始后自动隐藏主窗口："
        } else {
            "Auto-hide on start:"
        },
        72,
        324,
    );
    font_controls.push(label);
    let label = create_static(
        hwnd,
        state.instance,
        if chinese {
            "开机自启动："
        } else {
            "Launch at startup:"
        },
        72,
        372,
    );
    font_controls.push(label);
    font_controls.push(state.period);
    font_controls.extend(state.character);
    font_controls.extend(state.language);
    font_controls.extend(state.theme);
    font_controls.extend(state.close_behavior);
    font_controls.extend(state.auto_hide);
    font_controls.extend(state.launch_at_startup);
    font_controls.push(state.reset_defaults);
    state.font_controls = font_controls;
    populate_config_controls(state);
    replace_title_font(state);
    for control in &state.font_controls {
        set_font(*control, state.font);
    }
    for title in &state.font_controls[..SETTINGS_TITLE_COUNT] {
        set_font(*title, state.title_font);
    }
}

fn populate_config_controls(state: &mut SettingsState) {
    set_window_text(state.period, &period_minutes(state.config.period).to_string());
    state.period_changed = false;
    set_radio_group(&state.character, &state.config.character, &["cat", "dog"]);
    set_radio_group(&state.language, &state.config.language, &["auto", "zh", "en"]);
    set_radio_group(&state.theme, &state.config.theme, &["system", "light", "dark"]);
    set_radio_group(
        &state.close_behavior,
        if state.config.tray_when_close { "tray" } else { "exit" },
        &["exit", "tray"],
    );
    set_radio_group(
        &state.auto_hide,
        if state.config.auto_hide_on_start { "yes" } else { "no" },
        &["yes", "no"],
    );
    set_radio_group(
        &state.launch_at_startup,
        if state.config.launch_at_startup { "yes" } else { "no" },
        &["yes", "no"],
    );
}

pub fn update_settings_panel_font(hwnd: HWND, dpi: u32) {
    let Some(state) = state_mut(hwnd) else {
        return;
    };
    state.font = crate::ui::font::common_gui_font(dpi, state.config.language() == crate::i18n::Language::Chinese);
    replace_title_font(state);
    for control in &state.font_controls {
        set_font(*control, state.font);
    }
    for title in &state.font_controls[..SETTINGS_TITLE_COUNT] {
        set_font(*title, state.title_font);
    }
    layout_settings_panel(hwnd, dpi);
}

pub fn update_settings_panel_language(hwnd: HWND, language: crate::i18n::Language) {
    let Some(state) = state_mut(hwnd) else {
        return;
    };
    let chinese = language == crate::i18n::Language::Chinese;
    let titles = [
        if chinese {
            "倒计数分钟数："
        } else {
            "Countdown minutes:"
        },
        if chinese { "桌宠：" } else { "Pet:" },
        if chinese { "语言：" } else { "Language:" },
        if chinese { "主题：" } else { "Theme:" },
        if chinese {
            "关闭主窗口行为："
        } else {
            "Close behavior:"
        },
        if chinese {
            "开始后自动隐藏主窗口："
        } else {
            "Auto-hide on start:"
        },
        if chinese {
            "开机自启动："
        } else {
            "Launch at startup:"
        },
    ];
    for (control, text) in state.font_controls[..SETTINGS_TITLE_COUNT].iter().zip(titles) {
        set_control_text(*control, text);
    }
    let language_options = if chinese {
        ["自动", "中文", "English"]
    } else {
        ["Auto", "中文", "English"]
    };
    for (control, text) in state.language.iter().zip(language_options) {
        set_control_text(*control, text);
    }
    let character_options = if chinese { ["小猫", "小狗"] } else { ["cat", "dog"] };
    for (control, text) in state.character.iter().zip(character_options) {
        set_control_text(*control, text);
    }
    let theme_options = if chinese {
        ["系统", "浅色", "深色"]
    } else {
        ["System", "Light", "Dark"]
    };
    for (control, text) in state.theme.iter().zip(theme_options) {
        set_control_text(*control, text);
    }
    let close_options = if chinese {
        ["退出程序", "缩小为托盘图标"]
    } else {
        ["Exit program", "Minimize to tray"]
    };
    for (control, text) in state.close_behavior.iter().zip(close_options) {
        set_control_text(*control, text);
    }
    let auto_hide_options = if chinese { ["是", "否"] } else { ["Yes", "No"] };
    for (control, text) in state.auto_hide.iter().zip(auto_hide_options) {
        set_control_text(*control, text);
    }
    for (control, text) in state.launch_at_startup.iter().zip(auto_hide_options) {
        set_control_text(*control, text);
    }
    set_control_text(state.reset_defaults, reset_defaults_text(language));
    set_window_text(hwnd, if chinese { "设置" } else { "Settings" });
}

pub fn update_settings_panel_auto_hide(hwnd: HWND, enabled: bool) {
    if let Some(state) = state_mut(hwnd) {
        set_radio_group(&state.auto_hide, if enabled { "yes" } else { "no" }, &["yes", "no"]);
    }
}

pub fn refresh_settings_panel_theme(hwnd: HWND, config: &Config) {
    let Some(state) = state_mut(hwnd) else {
        return;
    };
    let dark_mode = is_settings_dark_mode(config);
    let background = if dark_mode {
        COLORREF(0x00202020)
    } else {
        COLORREF(0x00F0F0F0)
    };

    state.dark_mode = dark_mode;
    unsafe {
        if !state.background_brush.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(state.background_brush.0));
        }
        if !state.control_brush.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(state.control_brush.0));
        }
        state.background_brush = CreateSolidBrush(background);
        state.control_brush = CreateSolidBrush(background);
        let _ = InvalidateRect(Some(hwnd), None, true);
        for control in &state.font_controls {
            let _ = InvalidateRect(Some(*control), None, true);
        }
    }
}

fn replace_title_font(state: &mut SettingsState) {
    if let Some(font) = state.title_font.take() {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
    }
    state.title_font = state.font.and_then(create_bold_font);
}

fn create_bold_font(font: HFONT) -> Option<HFONT> {
    let mut logfont = LOGFONTW::default();
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ(font.0),
            std::mem::size_of::<LOGFONTW>() as i32,
            Some((&mut logfont as *mut LOGFONTW).cast()),
        )
    };
    if copied == 0 {
        return None;
    }
    logfont.lfWeight = 700;
    let bold_font = unsafe { CreateFontIndirectW(&logfont) };
    (!bold_font.is_invalid()).then_some(bold_font)
}

fn set_font(hwnd: HWND, font: Option<HFONT>) {
    if let Some(font) = font {
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
    }
}

fn set_control_text(hwnd: HWND, text: &str) {
    set_window_text(hwnd, text);
}

fn set_window_text(hwnd: HWND, text: &str) {
    let value = wide(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(value.as_ptr()));
    }
}

pub fn create_settings_button(parent: HWND, instance: HINSTANCE, text: &str, font: Option<HFONT>) -> HWND {
    create_footer_button(parent, instance, SETTINGS_BUTTON_ID, text, font)
}

pub fn create_about_button(parent: HWND, instance: HINSTANCE, text: &str, font: Option<HFONT>) -> HWND {
    create_footer_button(parent, instance, ABOUT_BUTTON_ID, text, font)
}

fn create_footer_button(parent: HWND, instance: HINSTANCE, id: usize, text: &str, font: Option<HFONT>) -> HWND {
    let button = create_button(parent, instance, id, text, 0, 0, 96, 32);
    set_font(button, font);
    button
}

pub fn set_settings_button_text(hwnd: HWND, text: &str) {
    let value = wide(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(value.as_ptr()));
    }
}

pub fn update_settings_button_font(hwnd: HWND, font: Option<HFONT>) {
    set_font(hwnd, font);
}

unsafe extern "system" fn reset_defaults_subclass(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    panel: usize,
) -> LRESULT {
    match message {
        WM_SETCURSOR => {
            unsafe {
                let _ = SetCursor(Some(LoadCursorW(None, IDC_HAND).unwrap_or_default()));
            }
            return LRESULT(1);
        }
        WM_MOUSEMOVE | WM_MOUSELEAVE => {
            if let Some(state) = state_mut(HWND(panel as _)) {
                let hovered = message == WM_MOUSEMOVE;
                if state.reset_defaults_hovered != hovered {
                    state.reset_defaults_hovered = hovered;
                    unsafe {
                        if hovered {
                            let mut tracking = TRACKMOUSEEVENT {
                                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                                dwFlags: TME_LEAVE,
                                hwndTrack: hwnd,
                                ..Default::default()
                            };
                            let _ = TrackMouseEvent(&mut tracking);
                        }
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
        }
        WM_NCDESTROY => unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(reset_defaults_subclass), id);
        },
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

pub fn draw_settings_button(item: &DRAWITEMSTRUCT, dark_mode: bool, hovered: bool, font: Option<HFONT>) {
    let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
    let background = if dark_mode {
        if pressed {
            COLORREF(0x00303030)
        } else if hovered {
            COLORREF(0x00303030)
        } else {
            COLORREF(0x00202020)
        }
    } else if pressed {
        COLORREF(0x00D0D0D0)
    } else if hovered {
        COLORREF(0x00FFFFFF)
    } else {
        COLORREF(0x00F0F0F0)
    };
    unsafe {
        let background_brush = CreateSolidBrush(background);
        let _ = FillRect(item.hDC, &item.rcItem, background_brush);
        let _ = DeleteObject(HGDIOBJ(background_brush.0));
    }
    draw_button_text(item, dark_mode, font);
}

fn draw_button_text(item: &DRAWITEMSTRUCT, dark_mode: bool, font: Option<HFONT>) {
    let foreground = if dark_mode {
        COLORREF(0x00FAFAFA)
    } else {
        COLORREF(0x00202020)
    };
    unsafe {
        let _ = SetTextColor(item.hDC, foreground);
        let _ = SetBkMode(item.hDC, TRANSPARENT);
        let old_font = font.map(|font| SelectObject(item.hDC, HGDIOBJ(font.0)));
        let mut text = get_window_text(item.hwndItem)
            .encode_utf16()
            .chain([0])
            .collect::<Vec<_>>();
        let mut rect = item.rcItem;
        let _ = DrawTextW(item.hDC, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        if let Some(old_font) = old_font {
            let _ = SelectObject(item.hDC, old_font);
        }
        if item.itemState.0 & ODS_FOCUS.0 != 0 && item.itemState.0 & ODS_NOFOCUSRECT.0 == 0 {
            let dpi = GetDpiForWindow(item.hwndItem).max(96);
            let inset = 3 * dpi as i32 / 96;
            let focus_rect = RECT {
                left: item.rcItem.left + inset,
                top: item.rcItem.top + inset,
                right: item.rcItem.right - inset,
                bottom: item.rcItem.bottom - inset,
            };
            let _ = DrawFocusRect(item.hDC, &focus_rect);
        }
    }
}

fn is_settings_dark_mode(config: &Config) -> bool {
    match config.theme() {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => is_dark_mode().unwrap_or(false),
    }
}

fn read_config(state: &SettingsState, config: Config) -> Config {
    let mut config = read_config_without_period(state, config);
    if state.period_changed {
        config.period = parse_period_minutes(&get_window_text(state.period)) * 60;
    }
    config
}

fn period_minutes(seconds: u32) -> u32 {
    seconds.div_ceil(60).max(1)
}

fn parse_period_minutes(text: &str) -> u32 {
    text.parse::<u32>().unwrap_or(1).clamp(1, u32::MAX / 60)
}

fn normalize_period(hwnd: HWND) {
    let text = get_window_text(hwnd);
    let period = parse_period_minutes(&text);
    let value = period.to_string();
    if text != value {
        unsafe {
            let _ = SetWindowTextW(hwnd, PCWSTR(wide(&value).as_ptr()));
        }
    }
}

pub fn save_settings_panel(hwnd: HWND) -> Result<()> {
    let Some(state) = state_mut(hwnd) else {
        return Err(Error::from_win32());
    };
    read_config(state, Config::load()?).save()
}

fn restore_startup_radio(state: &SettingsState) {
    set_radio_group(
        &state.launch_at_startup,
        if state.config.launch_at_startup { "yes" } else { "no" },
        &["yes", "no"],
    );
}

fn reset_defaults_text(language: crate::i18n::Language) -> &'static str {
    match language {
        crate::i18n::Language::Chinese => "重置为默认值",
        crate::i18n::Language::English => "Reset to defaults",
    }
}

fn reset_to_defaults(hwnd: HWND, previous: Config, save: impl FnOnce(&Config) -> Result<()>) -> Result<()> {
    let defaults = Config {
        launch_at_startup: previous.launch_at_startup,
        ..Config::default()
    };
    save(&defaults)?;
    let state = state_mut(hwnd).ok_or_else(Error::from_win32)?;
    state.config = defaults.clone();
    populate_config_controls(state);
    update_settings_panel_language(hwnd, defaults.language());
    refresh_settings_panel_theme(hwnd, &defaults);
    update_settings_panel_font(hwnd, unsafe { GetDpiForWindow(hwnd) }.max(96));
    Ok(())
}

fn apply_startup_preference(
    state: &mut SettingsState,
    previous: Config,
    save: impl FnMut(&Config) -> Result<()>,
    sync: impl FnOnce(bool) -> Result<()>,
) -> Result<()> {
    let mut next = previous.clone();
    next.launch_at_startup = is_checked(state.launch_at_startup[0]);
    state.config = previous;
    let result = crate::autostart::save_preference(&state.config, &next, save, sync);
    if result.is_ok() {
        state.config = next;
    }
    restore_startup_radio(state);
    result
}

fn read_config_without_period(state: &SettingsState, mut config: Config) -> Config {
    config.character = radio_text(&state.character, &["cat", "dog"]);
    config.language = radio_text(&state.language, &["auto", "zh", "en"]);
    config.theme = radio_text(&state.theme, &["system", "light", "dark"]);
    config.tray_when_close = is_checked(state.close_behavior[1]);
    config.auto_hide_on_start = is_checked(state.auto_hide[0]);
    config.launch_at_startup = is_checked(state.launch_at_startup[0]);
    config
}

fn create_static(parent: HWND, instance: HINSTANCE, text: &str, x: i32, y: i32) -> HWND {
    let value = wide(text);
    unsafe {
        CreateWindowExW(
            Default::default(),
            w!("STATIC"),
            PCWSTR(value.as_ptr()),
            // SS_RIGHT | SS_CENTERIMAGE: keep the title right-aligned and vertically centered in the grid row.
            WS_CHILD | WS_VISIBLE | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0x202),
            x,
            y,
            145,
            24,
            Some(parent),
            None,
            Some(instance),
            None,
        )
        .unwrap_or_default()
    }
}

fn create_edit(parent: HWND, instance: HINSTANCE, id: usize, text: &str, x: i32, y: i32) -> HWND {
    let value = wide(text);
    unsafe {
        CreateWindowExW(
            Default::default(),
            w!("EDIT"),
            PCWSTR(value.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(ES_NUMBER as u32),
            x,
            y,
            280,
            24,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(instance),
            None,
        )
        .unwrap_or_default()
    }
}

fn create_radio(parent: HWND, instance: HINSTANCE, id: usize, text: &str, first: bool) -> HWND {
    let style = (BS_AUTORADIOBUTTON | if first { WS_GROUP.0 as i32 } else { 0 }) as u32;
    create_button_with_style(parent, instance, id, text, style, 0, 0, 96, 32)
}

fn create_button_with_style(
    parent: HWND,
    instance: HINSTANCE,
    id: usize,
    text: &str,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> HWND {
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            w!("BUTTON"),
            PCWSTR(wide(text).as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(style),
            x,
            y,
            width,
            height,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(instance),
            None,
        )
        .unwrap_or_default()
    };
    hwnd
}

fn create_button(
    parent: HWND,
    instance: HINSTANCE,
    id: usize,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> HWND {
    let value = wide(text);
    unsafe {
        CreateWindowExW(
            Default::default(),
            w!("BUTTON"),
            PCWSTR(value.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(BS_OWNERDRAW as u32),
            x,
            y,
            width,
            height,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(instance),
            None,
        )
        .unwrap_or_default()
    }
}

fn set_radio_group(radios: &[HWND], value: &str, values: &[&str]) {
    for (radio, candidate) in radios.iter().zip(values) {
        set_check(*radio, *candidate == value);
    }
}

fn radio_text(radios: &[HWND], values: &[&str]) -> String {
    radios
        .iter()
        .zip(values)
        .find_map(|(radio, value)| is_checked(*radio).then_some((*value).to_string()))
        .unwrap_or_default()
}

fn set_check(hwnd: HWND, checked: bool) {
    unsafe {
        let _ = SendMessageW(
            hwnd,
            BM_SETCHECK,
            Some(WPARAM(if checked { BST_CHECKED.0 as usize } else { 0 })),
            Some(LPARAM(0)),
        );
    }
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0 == BST_CHECKED.0 as isize }
}

fn get_window_text(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) } as usize;
    let mut text = vec![0; length + 1];
    unsafe {
        let _ = GetWindowTextW(hwnd, &mut text);
    }
    String::from_utf16_lossy(&text[..length])
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;

    #[test]
    fn reset_defaults_preserves_startup_and_preserves_panel_on_save_failure() -> Result<()> {
        use windows::Win32::Foundation::ERROR_ACCESS_DENIED;
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
        let parent = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                0,
                0,
                800,
                533,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        for launch_at_startup in [false, true] {
            let previous = Config {
                period: 90,
                character: "dog".into(),
                language: "en".into(),
                theme: "dark".into(),
                tray_when_close: false,
                auto_hide_on_start: false,
                launch_at_startup,
            };
            let expected = Config {
                launch_at_startup,
                ..Config::default()
            };
            let panel = create_settings_panel(parent, instance, previous.clone(), None)?;
            set_window_text(state_mut(panel).unwrap().period, "37");
            let result = reset_to_defaults(panel, previous.clone(), |config| {
                assert_eq!(config, &expected);
                Err(Error::from_hresult(ERROR_ACCESS_DENIED.to_hresult()))
            });
            assert!(result.is_err());
            let state = state_mut(panel).unwrap();
            assert_eq!(state.config, previous);
            assert_eq!(get_window_text(state.period), "37");
            assert!(state.period_changed);
            assert_eq!(read_config_without_period(state, previous.clone()), previous);
            for _ in 0..2 {
                let mut saved = None;
                let previous = state_mut(panel).unwrap().config.clone();
                reset_to_defaults(panel, previous, |config| {
                    saved = Some(config.clone());
                    Ok(())
                })?;
                assert_eq!(saved, Some(expected.clone()));
                let state = state_mut(panel).unwrap();
                assert_eq!(state.config, expected);
                assert_eq!(get_window_text(state.period), "20");
                assert!(!state.period_changed);
                assert_eq!(read_config(state, expected.clone()), expected);
                assert_eq!(is_checked(state.launch_at_startup[0]), launch_at_startup);
                assert_eq!(is_checked(state.launch_at_startup[1]), !launch_at_startup);
            }
            update_settings_panel_language(panel, crate::i18n::Language::Chinese);
            assert_eq!(
                get_window_text(state_mut(panel).unwrap().reset_defaults),
                "重置为默认值"
            );
            update_settings_panel_language(panel, crate::i18n::Language::English);
            assert_eq!(
                get_window_text(state_mut(panel).unwrap().reset_defaults),
                "Reset to defaults"
            );
            let button = state_mut(panel).unwrap().reset_defaults;
            assert!(!state_mut(panel).unwrap().reset_defaults_hovered);
            unsafe {
                SendMessageW(button, WM_MOUSEMOVE, None, None);
                assert!(state_mut(panel).unwrap().reset_defaults_hovered);
                SendMessageW(button, WM_SETCURSOR, None, None);
                assert_eq!(
                    windows::Win32::UI::WindowsAndMessaging::GetCursor(),
                    LoadCursorW(None, IDC_HAND)?
                );
                SendMessageW(button, WM_MOUSELEAVE, None, None);
                assert!(!state_mut(panel).unwrap().reset_defaults_hovered);
            }
            unsafe {
                DestroyWindow(panel)?;
            }
        }
        unsafe {
            DestroyWindow(parent)?;
        }
        Ok(())
    }

    #[test]
    fn startup_changes_restore_radios_on_failure_and_preserve_unsaved_minutes() -> Result<()> {
        use windows::Win32::Foundation::ERROR_ACCESS_DENIED;
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
        let parent = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                0,
                0,
                800,
                533,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        for previous_enabled in [false, true] {
            let previous = Config {
                period: 90,
                launch_at_startup: previous_enabled,
                ..Config::default()
            };
            let panel = create_settings_panel(parent, instance, previous.clone(), None)?;
            let state = state_mut(panel).unwrap();
            set_window_text(state.period, "37");
            for failure in 0..4 {
                set_radio_group(
                    &state.launch_at_startup,
                    if previous_enabled { "no" } else { "yes" },
                    &["yes", "no"],
                );
                let mut saves = 0;
                let mut syncs = 0;
                let result = apply_startup_preference(
                    state,
                    previous.clone(),
                    |_| {
                        saves += 1;
                        if failure == 0 || (failure == 2 && saves == 2) {
                            Err(Error::from_hresult(ERROR_ACCESS_DENIED.to_hresult()))
                        } else {
                            Ok(())
                        }
                    },
                    |enabled| {
                        syncs += 1;
                        assert_eq!(enabled, !previous_enabled);
                        if failure == 3 {
                            Ok(())
                        } else {
                            Err(Error::from_hresult(ERROR_ACCESS_DENIED.to_hresult()))
                        }
                    },
                );
                assert_eq!(result.is_ok(), failure == 3);
                assert_eq!(syncs, usize::from(failure != 0));
                assert_eq!(saves, if failure == 1 || failure == 2 { 2 } else { 1 });
                let expected = if failure == 3 {
                    !previous_enabled
                } else {
                    previous_enabled
                };
                assert_eq!(is_checked(state.launch_at_startup[0]), expected);
                assert_eq!(is_checked(state.launch_at_startup[1]), !expected);
                assert_eq!(
                    state.config,
                    Config {
                        launch_at_startup: expected,
                        ..previous.clone()
                    }
                );
                assert_eq!(get_window_text(state.period), "37");
                assert!(state.period_changed);
            }
            unsafe {
                DestroyWindow(panel)?;
            }
        }
        unsafe {
            DestroyWindow(parent)?;
        }
        Ok(())
    }

    #[test]
    fn settings_focus_starts_in_minutes_and_tabs_through_back_button() -> Result<()> {
        use windows::Win32::UI::{
            Input::KeyboardAndMouse::{GetFocus, VK_TAB},
            WindowsAndMessaging::{GetNextDlgTabItem, IsDialogMessageW, MSG, WM_KEYDOWN},
        };
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
        let parent = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                0,
                0,
                800,
                533,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        let back = create_settings_button(parent, instance, "返回", None);
        let panel = create_settings_panel(parent, instance, Config::default(), None)?;
        let minutes = state_mut(panel).unwrap().period;
        focus_settings_panel(panel);
        assert_eq!(unsafe { GetFocus() }, minutes);
        let mut visited = Vec::new();
        for _ in 0..32 {
            let message = MSG {
                hwnd: unsafe { GetFocus() },
                message: WM_KEYDOWN,
                wParam: WPARAM(VK_TAB.0 as usize),
                ..Default::default()
            };
            assert!(unsafe { IsDialogMessageW(parent, &message).as_bool() });
            let focused = unsafe { GetFocus() };
            visited.push(focused);
            if focused == minutes {
                break;
            }
        }
        assert!(visited.contains(&back), "Tab must include the footer Back button");
        assert!(
            visited.contains(&state_mut(panel).unwrap().reset_defaults),
            "Tab must include reset to defaults"
        );
        assert_eq!(
            unsafe { GetNextDlgTabItem(parent, Some(state_mut(panel).unwrap().reset_defaults), false)? },
            back
        );
        assert_eq!(visited.last(), Some(&minutes), "Tab must loop back to minutes");
        assert_eq!(unsafe { GetNextDlgTabItem(parent, Some(minutes), true)? }, back);
        unsafe { SetFocus(Some(back))? };
        focus_settings_panel(panel);
        assert_eq!(
            unsafe { GetFocus() },
            minutes,
            "reopening must restore the initial focus"
        );
        unsafe { DestroyWindow(parent)? };
        Ok(())
    }

    #[test]
    fn settings_preserve_period_until_saved_and_release_state_on_close() -> Result<()> {
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
        let parent = unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                0,
                0,
                800,
                533,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        let drops = STATE_DROPS.with(|count| count.get());
        let back = create_settings_button(parent, instance, "返回", None);
        for period in [30, 90, 1200] {
            let initial = Config {
                period,
                auto_hide_on_start: period != 90,
                launch_at_startup: period == 90,
                ..Config::default()
            };
            let panel = create_settings_panel(parent, instance, initial.clone(), None)?;
            for dpi in [96, 144, 192] {
                move_control(panel, 0, 0, 800 * dpi as i32 / 96, 533 * dpi as i32 / 96);
                move_control(
                    back,
                    (800 - 12 - 96) * dpi as i32 / 96,
                    (533 - 64) * dpi as i32 / 96,
                    96 * dpi as i32 / 96,
                    32 * dpi as i32 / 96,
                );
                layout_settings_panel(panel, dpi);
                let state = state_mut(panel).unwrap();
                let mut panel_rect = RECT::default();
                let mut previous_row = RECT::default();
                let mut auto_hide_rect = RECT::default();
                let mut startup_rect = RECT::default();
                let mut reset_rect = RECT::default();
                let mut back_rect = RECT::default();
                unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
                    GetWindowRect(panel, &mut panel_rect)?;
                    GetWindowRect(state.close_behavior[0], &mut previous_row)?;
                    GetWindowRect(state.auto_hide[0], &mut auto_hide_rect)?;
                    GetWindowRect(state.launch_at_startup[0], &mut startup_rect)?;
                    GetWindowRect(state.reset_defaults, &mut reset_rect)?;
                    GetWindowRect(back, &mut back_rect)?;
                }
                assert!(auto_hide_rect.top >= previous_row.bottom);
                assert!(auto_hide_rect.bottom <= panel_rect.bottom);
                assert!(startup_rect.top >= auto_hide_rect.bottom);
                assert!(startup_rect.bottom <= panel_rect.bottom);
                assert!(reset_rect.top >= startup_rect.bottom);
                assert!(reset_rect.bottom <= panel_rect.bottom);
                assert_eq!(reset_rect.top, back_rect.top);
                assert_eq!(reset_rect.bottom, back_rect.bottom);
                assert_eq!(back_rect.left - reset_rect.right, 12 * dpi as i32 / 96);
            }
            update_settings_panel_language(panel, crate::i18n::Language::English);
            assert_eq!(get_window_text(state_mut(panel).unwrap().auto_hide[0]), "Yes");
            assert_eq!(get_window_text(state_mut(panel).unwrap().launch_at_startup[0]), "Yes");
            update_settings_panel_language(panel, crate::i18n::Language::Chinese);
            assert_eq!(get_window_text(state_mut(panel).unwrap().auto_hide[1]), "否");
            assert_eq!(get_window_text(state_mut(panel).unwrap().launch_at_startup[1]), "否");
            let state = state_mut(panel).unwrap();
            assert_eq!(is_checked(state.launch_at_startup[0]), initial.launch_at_startup);
            assert_eq!(is_checked(state.launch_at_startup[1]), !initial.launch_at_startup);
            assert_eq!(
                read_config_without_period(state, initial.clone()).launch_at_startup,
                initial.launch_at_startup
            );
            set_radio_group(
                &state.launch_at_startup,
                if initial.launch_at_startup { "no" } else { "yes" },
                &["yes", "no"],
            );
            assert_eq!(is_checked(state.auto_hide[0]), initial.auto_hide_on_start);
            assert_eq!(is_checked(state.auto_hide[1]), !initial.auto_hide_on_start);
            assert_eq!(
                read_config_without_period(state, initial.clone()).auto_hide_on_start,
                initial.auto_hide_on_start
            );
            update_settings_panel_auto_hide(panel, false);
            assert!(!is_checked(state.auto_hide[0]));
            assert!(is_checked(state.auto_hide[1]));
            let latest = Config {
                auto_hide_on_start: false,
                ..initial
            };
            assert_eq!(get_window_text(state.period), period_minutes(period).to_string());
            normalize_period(state.period);
            assert!(!state.period_changed);
            assert_eq!(read_config(state, latest.clone()).period, period);
            set_window_text(state.period, "90");
            assert!(state.period_changed);
            let changed = read_config_without_period(state, latest.clone());
            assert_eq!(changed.period, latest.period);
            assert!(!changed.auto_hide_on_start);
            assert_eq!(changed.launch_at_startup, !latest.launch_at_startup);
            let saved = read_config(state, latest.clone());
            assert_eq!(saved.period, 90 * 60);
            assert!(!saved.auto_hide_on_start);
            assert_eq!(saved.launch_at_startup, !latest.launch_at_startup);
            unsafe { DestroyWindow(panel)? };
        }
        unsafe { DestroyWindow(parent)? };
        assert_eq!(STATE_DROPS.with(|count| count.get()), drops + 3);
        Ok(())
    }

    #[test]
    fn minute_conversion_rounds_up_and_bounds_saved_seconds() {
        for (seconds, minutes) in [
            (0, 1),
            (1, 1),
            (59, 1),
            (60, 1),
            (61, 2),
            (90, 2),
            (1200, 20),
            (u32::MAX, 71582789),
        ] {
            assert_eq!(period_minutes(seconds), minutes);
        }
        for (input, minutes) in [("", 1), ("0", 1), ("1", 1), ("20", 20), ("4294967295", u32::MAX / 60)] {
            let parsed = parse_period_minutes(input);
            assert_eq!(parsed, minutes);
            assert_eq!((parsed * 60) % 60, 0);
        }
    }

    #[test]
    fn failed_settings_creation_releases_state() {
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None).unwrap() }.into();
        let drops = STATE_DROPS.with(|count| count.get());
        assert!(create_settings_panel(HWND(-1isize as _), instance, Config::default(), None).is_err());
        assert_eq!(STATE_DROPS.with(|count| count.get()), drops + 1);
    }
}
