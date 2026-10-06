use std::sync::OnceLock;

use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            CreateSolidBrush, DT_CENTER, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, FillRect, HBRUSH, HDC,
            HFONT, HGDIOBJ, InvalidateRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::{
            Controls::{BST_CHECKED, DRAWITEMSTRUCT, ODS_SELECTED},
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::SetFocus,
            WindowsAndMessaging::{
                BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTORADIOBUTTON, BS_OWNERDRAW, CREATESTRUCTW, CreateWindowExW,
                DefWindowProcW, DestroyWindow, EN_KILLFOCUS, ES_NUMBER, GWLP_USERDATA, GetWindowLongPtrW,
                GetWindowTextLengthW, GetWindowTextW, HMENU, IDC_ARROW, LoadCursorW, RegisterClassExW, SendMessageW,
                SetWindowLongPtrW, SetWindowTextW, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLOREDIT,
                WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_NCACTIVATE, WM_NCDESTROY, WM_SETFOCUS,
                WM_SETFONT, WNDCLASSEXW, WS_CHILD, WS_EX_CONTROLPARENT, WS_GROUP, WS_TABSTOP, WS_VISIBLE,
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

const BASE_GRID_LEFT: i32 = 72;
const BASE_TITLE_WIDTH: i32 = 196;
const BASE_COLUMN_GAP: i32 = 12;
const BASE_CONTROL_WIDTH: i32 = 280;
const BASE_GRID_TOP: i32 = 80;
const BASE_ROW_HEIGHT: i32 = 32;
const BASE_ROW_GAP: i32 = 16;

struct SettingsState {
    parent: HWND,
    embedded: bool,
    instance: HINSTANCE,
    config: Config,
    period: HWND,
    character: [HWND; 2],
    language: [HWND; 3],
    theme: [HWND; 3],
    close_behavior: [HWND; 2],
    font: Option<HFONT>,
    font_controls: Vec<HWND>,
    dark_mode: bool,
    background_brush: HBRUSH,
    control_brush: HBRUSH,
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
        character: [HWND::default(); 2],
        language: [HWND::default(); 3],
        theme: [HWND::default(); 3],
        close_behavior: [HWND::default(); 2],
        font,
        font_controls: Vec::new(),
        dark_mode,
        background_brush: HBRUSH::default(),
        control_brush: HBRUSH::default(),
    });
    let state_ptr = Box::into_raw(state);
    match unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            SETTINGS_CLASS,
            w!("Settings"),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            1,
            1,
            Some(parent),
            None,
            Some(instance),
            Some(state_ptr.cast()),
        )
    } {
        Ok(hwnd) => Ok(hwnd),
        Err(error) => {
            unsafe {
                drop(Box::from_raw(state_ptr));
            }
            Err(error)
        }
    }
}

pub fn resize_settings_panel(hwnd: HWND, parent: HWND) -> Result<()> {
    let mut rect = RECT::default();
    let dpi = unsafe { GetDpiForWindow(parent) }.max(96);
    let reserved_height = SETTINGS_BUTTON_RESERVED_HEIGHT * dpi as i32 / 96;
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetClientRect(parent, &mut rect)?;
        windows::Win32::UI::WindowsAndMessaging::MoveWindow(
            hwnd,
            0,
            0,
            rect.right,
            (rect.bottom - reserved_height).max(0),
            true,
        )?;
    }
    layout_settings_panel(hwnd, dpi);
    Ok(())
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
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
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
                let (text, background) = if state.dark_mode {
                    (COLORREF(0x00FAFAFA), COLORREF(0x00202020))
                } else {
                    (COLORREF(0x00202020), COLORREF(0x00F0F0F0))
                };
                unsafe {
                    let _ = SetTextColor(hdc, text);
                    let _ = SetBkColor(hdc, background);
                }
                return LRESULT(state.control_brush.0 as isize);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as usize;
            let notification = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if id == PERIOD_ID && notification == EN_KILLFOCUS {
                if let Some(state) = state_mut(hwnd) {
                    normalize_period(state.period);
                }
            }
            if notification == BN_CLICKED {
                match id {
                    CHARACTER_CAT_ID | CHARACTER_DOG_ID | LANGUAGE_AUTO_ID | LANGUAGE_ZH_ID | LANGUAGE_EN_ID
                    | THEME_SYSTEM_ID | THEME_LIGHT_ID | THEME_DARK_ID | CLOSE_EXIT_ID | CLOSE_TRAY_ID => {
                        if let Some(state) = state_mut(hwnd) {
                            if let Ok(config) = read_config_without_period(state) {
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
            if let Some(state) = state_mut(hwnd) {
                if state.embedded {
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            Some(state.parent),
                            WM_COMMAND,
                            WPARAM(SETTINGS_CLOSED_ID),
                            LPARAM(0),
                        );
                    }
                }
                unsafe {
                    if !state.background_brush.is_invalid() {
                        let _ = DeleteObject(HGDIOBJ(state.background_brush.0));
                    }
                    if !state.control_brush.is_invalid() {
                        let _ = DeleteObject(HGDIOBJ(state.control_brush.0));
                    }
                }
            }
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            LRESULT(0)
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
        &state.config.period.to_string(),
        280,
        80,
    );
    state.character = [
        create_radio(hwnd, state.instance, CHARACTER_CAT_ID, "cat", true),
        create_radio(hwnd, state.instance, CHARACTER_DOG_ID, "dog", false),
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
    let label = create_static(
        hwnd,
        state.instance,
        if chinese {
            "倒计时秒数："
        } else {
            "Countdown seconds:"
        },
        72,
        84,
    );
    font_controls.push(label);
    let label = create_static(hwnd, state.instance, if chinese { "Pet：" } else { "Pet:" }, 72, 132);
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
    font_controls.push(state.period);
    font_controls.extend(state.character);
    font_controls.extend(state.language);
    font_controls.extend(state.theme);
    font_controls.extend(state.close_behavior);
    state.font_controls = font_controls;
    set_radio_group(&state.character, &state.config.character, &["cat", "dog"]);
    set_radio_group(&state.language, &state.config.language, &["auto", "zh", "en"]);
    set_radio_group(&state.theme, &state.config.theme, &["system", "light", "dark"]);
    set_radio_group(
        &state.close_behavior,
        if state.config.tray_when_close { "tray" } else { "exit" },
        &["exit", "tray"],
    );
    for control in &state.font_controls {
        set_font(*control, state.font);
    }
}

pub fn update_settings_panel_font(hwnd: HWND, dpi: u32) {
    let Some(state) = state_mut(hwnd) else {
        return;
    };
    state.font = crate::ui::font::common_gui_font(dpi, state.config.language() == crate::i18n::Language::Chinese);
    for control in &state.font_controls {
        set_font(*control, state.font);
    }
    layout_settings_panel(hwnd, dpi);
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

fn set_font(hwnd: HWND, font: Option<HFONT>) {
    if let Some(font) = font {
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
    }
}

pub fn create_settings_button(parent: HWND, instance: HINSTANCE, text: &str, font: Option<HFONT>) -> HWND {
    let button = create_button(parent, instance, SETTINGS_BUTTON_ID, text, 0, 0, 96, 32);
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
    let foreground = if dark_mode {
        COLORREF(0x00FAFAFA)
    } else {
        COLORREF(0x00202020)
    };
    unsafe {
        let background_brush = CreateSolidBrush(background);
        let _ = FillRect(item.hDC, &item.rcItem, background_brush);
        let _ = DeleteObject(HGDIOBJ(background_brush.0));

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
    }
}

fn is_settings_dark_mode(config: &Config) -> bool {
    match config.theme() {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => is_dark_mode().unwrap_or(false),
    }
}

fn read_config(state: &SettingsState) -> Result<Config> {
    let period = get_window_text(state.period).parse::<u32>().unwrap_or(1).max(1);
    let mut config = state.config.clone();
    config.period = period.max(1);
    config.character = radio_text(&state.character, &["cat", "dog"]);
    config.language = radio_text(&state.language, &["auto", "zh", "en"]);
    config.theme = radio_text(&state.theme, &["system", "light", "dark"]);
    config.tray_when_close = is_checked(state.close_behavior[1]);
    Ok(config)
}

fn normalize_period(hwnd: HWND) {
    let period = get_window_text(hwnd).parse::<u32>().unwrap_or(1).max(1);
    let value = period.to_string();
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide(&value).as_ptr()));
    }
}

pub fn save_settings_panel(hwnd: HWND) -> Result<()> {
    let Some(state) = state_mut(hwnd) else {
        return Err(Error::from_win32());
    };
    read_config(state)?.save()
}

fn read_config_without_period(state: &SettingsState) -> Result<Config> {
    let mut config = state.config.clone();
    config.character = radio_text(&state.character, &["cat", "dog"]);
    config.language = radio_text(&state.language, &["auto", "zh", "en"]);
    config.theme = radio_text(&state.theme, &["system", "light", "dark"]);
    config.tray_when_close = is_checked(state.close_behavior[1]);
    Ok(config)
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
