#![cfg_attr(windows, windows_subsystem = "windows")]

mod about;
mod animation;
mod app_state;
mod asset;
mod autostart;
mod config;
mod gdi;
mod i18n;
mod persistence;
mod pet_window;
mod render;
mod settings;
mod single_instance;
mod speech_bubble;
mod speech_bubble_window;
mod startup;
mod timer_panel;
mod tray_icon;
mod ui;
mod window_proc;

use crate::app_state::AppState;
use crate::asset::load_character_catalog;
use crate::config::Config;
use crate::pet_window::PetWindow;
use crate::settings::{create_about_button, create_settings_button};
use crate::timer_panel::{create_timer_panel, register_timer_panel_class, resize_timer_panel};
use windows::Win32::{
    Foundation::{HINSTANCE, RECT},
    System::LibraryLoader::GetModuleHandleW,
    UI::HiDpi::{
        AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
        SetProcessDpiAwarenessContext,
    },
    UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DispatchMessageW, GetMessageW, GetSystemMetrics, HICON, IDC_ARROW,
        IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadCursorW, LoadIconW, LoadImageW, MB_ICONERROR, MB_OK, MSG,
        MessageBoxW, RegisterClassExW, SM_CXICON, SM_CXSCREEN, SM_CXSMICON, SM_CYICON, SM_CYSCREEN, SM_CYSMICON,
        TranslateMessage, WINDOW_EX_STYLE, WNDCLASSEXW,
    },
};
use windows::core::{Error, PCWSTR, Result};

use i18n::{detect_language, main_window_title};
use tray_icon::TrayIcon;
use ui::{
    button::{create_control_buttons, layout_control_buttons_for, register_button_class, update_control_buttons_for},
    font::{common_gui_font, release_common_gui_fonts},
    gdi_plus::GdiPlus,
    theme::apply_theme,
};
use window_proc::{
    WindowState, attach_window_state, create_auto_hide_checkbox, layout_window_state, process_dialog_message,
    set_initial_remaining_seconds, window_proc,
};

const WINDOW_WIDTH: i32 = 800;
const WINDOW_HEIGHT: i32 = 533;
const APP_ICON_RESOURCE_ID: usize = 1;

fn main() {
    let language = detect_language();
    let app_title = main_window_title(language);

    if let Err(message) = run() {
        unsafe {
            let text: Vec<u16> = message.to_string().encode_utf16().chain([0]).collect();
            let caption = wide_null(app_title);
            let _ = MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                PCWSTR(caption.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}

fn run() -> Result<()> {
    let autostart = startup::has_autostart_argument(std::env::args_os().skip(1));
    let Some(_instance) = single_instance::SingleInstance::acquire()? else {
        return startup::handle_existing_instance(autostart);
    };
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)? };
    let config = Config::load()?;
    let startup_sync = autostart::sync(config.launch_at_startup);
    let startup_mode = startup::StartupMode::new(autostart, config.launch_at_startup);
    let app_state = AppState::load()?;
    let language = config.language();
    let theme = config.theme();
    set_initial_remaining_seconds(config.period);

    let app_title = wide_null(main_window_title(language));
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
    let class_name = single_instance::MAIN_WINDOW_CLASS;
    let (large_icon, small_icon) = load_app_icons(instance);

    let wnd_class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        hIcon: large_icon,
        hIconSm: small_icon,
        ..Default::default()
    };

    if unsafe { RegisterClassExW(&wnd_class) } == 0 {
        return Err(Error::from_win32());
    }

    register_button_class(instance)?;
    register_timer_panel_class(instance)?;

    let style = startup_mode.window_style();
    let ex_style = WINDOW_EX_STYLE::default();
    let dpi = unsafe { GetDpiForSystem() }.max(96);
    let window_width = scale_dimension(WINDOW_WIDTH, dpi);
    let window_height = scale_dimension(WINDOW_HEIGHT, dpi);
    let (window_x, window_y) = centered_window_position(style, ex_style, window_width, window_height, dpi)?;
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name,
            PCWSTR(app_title.as_ptr()),
            style,
            window_x,
            window_y,
            window_width,
            window_height,
            None,
            None,
            Some(instance),
            None,
        )
    }?;
    let timer_panel = create_timer_panel(hwnd, instance)?;
    resize_timer_panel(timer_panel, hwnd)?;
    let gdi_plus = GdiPlus::new()?;
    let catalog = load_character_catalog(&gdi_plus).map_err(|error| {
        Error::new(
            windows::core::HRESULT(0x8000_4005u32 as i32),
            format!("Could not load pet assets: {error}"),
        )
    })?;
    let animations = catalog
        .get(&config.character)
        .or_else(|| catalog.get("cat"))
        .ok_or_else(Error::from_win32)?
        .clone();
    let pet_window = PetWindow::create(
        instance,
        hwnd,
        animations,
        language,
        app_state.pet_position,
        tray_menu_settings_text(language),
    )?;
    pet_window.set_language(language);
    let control_buttons = create_control_buttons(timer_panel, instance)?;
    let tray_icon = TrayIcon::create(
        hwnd,
        small_icon,
        main_window_title(language),
        tray_menu_start_text(language),
        tray_menu_show_text(language),
        tray_menu_settings_text(language),
        tray_menu_about_text(language),
        tray_menu_exit_text(language),
    )?;
    let common_gui_font = common_gui_font(dpi, language == i18n::Language::Chinese);
    let settings_button = create_settings_button(hwnd, instance, settings_menu_text(language), common_gui_font);
    let about_button = create_about_button(hwnd, instance, about_button_text(language), common_gui_font);
    let auto_hide_checkbox =
        create_auto_hide_checkbox(hwnd, instance, language, common_gui_font, config.auto_hide_on_start)?;
    attach_window_state(
        hwnd,
        WindowState {
            auto_hide_on_start: config.auto_hide_on_start,
            auto_hide_checkbox,
            language,
            theme,
            tray_icon,
            tray_when_close: config.tray_when_close,
            pet_window,
            character_catalog: catalog,
            components: Vec::new(),
            common_gui_font,
            settings_button,
            settings_button_hovered: false,
            about_button,
            about_button_hovered: false,
            settings_panel: windows::Win32::Foundation::HWND::default(),
            timer_panel,
            control_buttons,
        },
    );
    layout_control_buttons_for(timer_panel, &control_buttons)?;
    layout_window_state(hwnd)?;
    update_control_buttons_for(&control_buttons, true, false, false);
    apply_theme(hwnd, theme)?;

    startup_mode.finish(hwnd);
    if let Err(error) = startup_sync {
        autostart::show_error(hwnd, language, &error);
    }

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 == -1 {
            return Err(Error::from_win32());
        }
        if result.0 == 0 {
            break;
        }

        if process_dialog_message(hwnd, &message) {
            continue;
        }

        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    release_common_gui_fonts();

    Ok(())
}

fn centered_window_position(
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    client_width: i32,
    client_height: i32,
    dpi: u32,
) -> Result<(i32, i32)> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: client_width,
        bottom: client_height,
    };

    unsafe {
        AdjustWindowRectExForDpi(&mut rect, style, false, ex_style, dpi)?;
    }

    let window_width = rect.right - rect.left;
    let window_height = rect.bottom - rect.top;

    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    let x = (screen_width - window_width) / 2;
    let y = (screen_height - window_height) / 2;

    Ok((x, y))
}

fn scale_dimension(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi as i64 + 95) / 96) as i32
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

pub(crate) fn tray_menu_show_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "显示主窗口 (&M)",
        i18n::Language::English => "Show &main window",
    }
}

pub(crate) fn tray_menu_start_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "开始计时 (&S)",
        i18n::Language::English => "&Start Timer",
    }
}

pub(crate) fn tray_menu_exit_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "退出 (&X)",
        i18n::Language::English => "E&xit",
    }
}

pub(crate) fn tray_menu_about_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "关于 (&A)",
        i18n::Language::English => "&About",
    }
}

pub(crate) fn tray_menu_settings_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "设置 (&E)",
        i18n::Language::English => "S&ettings",
    }
}

pub(crate) fn about_button_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "关于",
        i18n::Language::English => "About",
    }
}

fn settings_menu_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "设置",
        i18n::Language::English => "Settings",
    }
}

fn load_app_icons(instance: HINSTANCE) -> (HICON, HICON) {
    let large_icon = load_icon_with_size(instance, unsafe { GetSystemMetrics(SM_CXICON) }, unsafe {
        GetSystemMetrics(SM_CYICON)
    });
    let small_icon = load_icon_with_size(instance, unsafe { GetSystemMetrics(SM_CXSMICON) }, unsafe {
        GetSystemMetrics(SM_CYSMICON)
    });

    let fallback = unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() };
    (large_icon.unwrap_or(fallback), small_icon.unwrap_or(fallback))
}

fn load_icon_with_size(instance: HINSTANCE, width: i32, height: i32) -> Option<HICON> {
    let resource = PCWSTR(APP_ICON_RESOURCE_ID as *const u16);
    let handle = unsafe { LoadImageW(Some(instance), resource, IMAGE_ICON, width, height, LR_DEFAULTCOLOR).ok()? };
    Some(HICON(handle.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_menu_access_keys_match_without_changing_footer_labels() {
        for (language, expected) in [
            (
                i18n::Language::English,
                ["&Start Timer", "Show &main window", "S&ettings", "&About", "E&xit"],
            ),
            (
                i18n::Language::Chinese,
                [
                    "开始计时 (&S)",
                    "显示主窗口 (&M)",
                    "设置 (&E)",
                    "关于 (&A)",
                    "退出 (&X)",
                ],
            ),
        ] {
            let actual = [
                tray_menu_start_text(language),
                tray_menu_show_text(language),
                tray_menu_settings_text(language),
                tray_menu_about_text(language),
                tray_menu_exit_text(language),
            ];
            assert_eq!(actual, expected);
            for (text, key) in actual.into_iter().zip(['s', 'm', 'e', 'a', 'x']) {
                let (_, marked) = text.split_once('&').unwrap();
                assert_eq!(marked.chars().next().unwrap().to_ascii_lowercase(), key);
                assert!(!marked.contains('&'));
            }
            assert!(!settings_menu_text(language).contains('&'));
            assert!(!about_button_text(language).contains('&'));
        }
    }
}
