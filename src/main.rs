mod config;
mod i18n;
mod tray_icon;
mod ui;
mod window_proc;

use crate::config::{Config, open_config_directory, set_tray_when_close, show_config_open_error};
use windows::Win32::{
    Foundation::{HINSTANCE, RECT},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        AdjustWindowRectEx, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DispatchMessageW, GetMessageW, GetSystemMetrics,
        HICON, IDC_ARROW, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadCursorW, LoadIconW, LoadImageW,
        MB_ICONERROR, MB_OK, MSG, MessageBoxW, RegisterClassExW, SM_CXICON, SM_CXSCREEN, SM_CXSMICON, SM_CYICON,
        SM_CYSCREEN, SM_CYSMICON, SW_SHOW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSEXW, WS_CAPTION,
        WS_CLIPCHILDREN, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    },
};
use windows::core::{Error, PCWSTR, Result, w};

use i18n::{detect_language, main_window_title};
use tray_icon::TrayIcon;
use ui::{
    button::{create_control_buttons, layout_control_buttons, register_button_class, update_control_buttons},
    check_box::CheckBox,
    component::Component,
    gdi_plus::GdiPlus,
    hyper_link_text::HyperLinkText,
    theme::apply_theme,
};
use window_proc::{WindowState, attach_window_state, layout_window_state, set_initial_remaining_seconds, window_proc};

const WINDOW_WIDTH: i32 = 800;
const WINDOW_HEIGHT: i32 = 533;
const APP_ICON_RESOURCE_ID: usize = 1;
const TRAY_CHECK_BOX_MARGIN_X: i32 = 28;
const CONFIG_LINK_MARGIN_X: i32 = 28;
const CONFIG_LINK_MARGIN_Y: i32 = 34;

fn main() {
    let language = detect_language();
    let app_title = main_window_title(language);

    if let Err(message) = run(language) {
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

fn run(language: i18n::Language) -> Result<()> {
    let config = Config::load()?;
    set_initial_remaining_seconds(config.period);

    let app_title = wide_null(main_window_title(language));
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
    let class_name = w!("YHB-StandAwhileWindowClass");
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

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN | WS_VISIBLE;
    let ex_style = WINDOW_EX_STYLE::default();
    let (window_x, window_y) = centered_window_position(style, ex_style)?;
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            class_name,
            PCWSTR(app_title.as_ptr()),
            style,
            window_x,
            window_y,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            None,
            None,
            Some(instance),
            None,
        )
    }?;

    let _gdi_plus = GdiPlus::new()?;
    create_control_buttons(hwnd, instance)?;
    let config_link = HyperLinkText::create(
        hwnd,
        config_link_text(language),
        |hwnd| {
            if let Err(error) = open_config_directory(hwnd) {
                show_config_open_error(hwnd, &error);
            }
        },
        layout_config_link,
    )?;
    let tray_check_box = CheckBox::create(
        hwnd,
        tray_check_box_text(language),
        config.tray_when_close,
        layout_tray_check_box,
        |hwnd, checked| {
            if let Err(error) = set_tray_when_close(checked) {
                let text: Vec<u16> = error.to_string().encode_utf16().chain([0]).collect();
                let caption = wide_null(main_window_title(detect_language()));
                unsafe {
                    let _ = MessageBoxW(
                        Some(hwnd),
                        PCWSTR(text.as_ptr()),
                        PCWSTR(caption.as_ptr()),
                        MB_OK | MB_ICONERROR,
                    );
                }
            }
        },
    )?;
    let tray_icon = TrayIcon::create(
        hwnd,
        small_icon,
        main_window_title(language),
        tray_menu_show_text(language),
        tray_menu_open_config_text(language),
        tray_menu_about_text(language),
        tray_menu_exit_text(language),
    )?;
    attach_window_state(
        hwnd,
        WindowState {
            tray_icon,
            tray_check_box: tray_check_box.clone(),
            components: vec![
                Box::new(config_link) as Box<dyn Component>,
                Box::new(tray_check_box) as Box<dyn Component>,
            ],
        },
    );
    layout_control_buttons(hwnd)?;
    layout_window_state(hwnd)?;
    update_control_buttons(hwnd, true, false, false)?;
    apply_theme(hwnd)?;

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
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

        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

fn centered_window_position(
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
) -> Result<(i32, i32)> {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: WINDOW_WIDTH,
        bottom: WINDOW_HEIGHT,
    };

    unsafe {
        AdjustWindowRectEx(&mut rect, style, false, ex_style)?;
    }

    let window_width = rect.right - rect.left;
    let window_height = rect.bottom - rect.top;

    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    let x = (screen_width - window_width) / 2;
    let y = (screen_height - window_height) / 2;

    Ok((x, y))
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn config_link_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "打开配置目录",
        i18n::Language::English => "Open config folder",
    }
}

fn tray_check_box_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "关闭时缩小到系统托盘图标",
        i18n::Language::English => "Minimize to system tray when closing",
    }
}

fn tray_menu_show_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "显示主窗口",
        i18n::Language::English => "Show window",
    }
}

fn tray_menu_exit_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "退出",
        i18n::Language::English => "Exit",
    }
}

fn tray_menu_open_config_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "打开配置目录",
        i18n::Language::English => "Open config folder",
    }
}

fn tray_menu_about_text(language: i18n::Language) -> &'static str {
    match language {
        i18n::Language::Chinese => "关于",
        i18n::Language::English => "About",
    }
}

fn layout_config_link(
    link: &HyperLinkText,
    parent: windows::Win32::Foundation::HWND,
    dc: windows::Win32::Graphics::Gdi::HDC,
) -> Result<()> {
    let mut client_rect = RECT::default();
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetClientRect(parent, &mut client_rect)?;
    }

    let (width, height) = link.window_size(dc)?;
    link.move_to(RECT {
        left: client_rect.right - CONFIG_LINK_MARGIN_X - width,
        top: client_rect.bottom - CONFIG_LINK_MARGIN_Y - height,
        right: client_rect.right - CONFIG_LINK_MARGIN_X,
        bottom: client_rect.bottom - CONFIG_LINK_MARGIN_Y,
    })
}

fn layout_tray_check_box(
    check_box: &CheckBox,
    parent: windows::Win32::Foundation::HWND,
    dc: windows::Win32::Graphics::Gdi::HDC,
) -> Result<()> {
    let mut client_rect = RECT::default();
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetClientRect(parent, &mut client_rect)?;
    }

    let (width, height) = check_box.window_size(dc)?;
    let bottom = client_rect.bottom - CONFIG_LINK_MARGIN_Y;
    let top = bottom - height;

    check_box.move_to(RECT {
        left: client_rect.left + TRAY_CHECK_BOX_MARGIN_X,
        top,
        right: client_rect.left + TRAY_CHECK_BOX_MARGIN_X + width,
        bottom: top + height,
    })
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
