mod i18n;
mod ui;
mod window_proc;

use windows::Win32::{
    Foundation::{HINSTANCE, RECT},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        AdjustWindowRectEx, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DispatchMessageW, GetMessageW, GetSystemMetrics,
        HICON, HMENU, IDC_ARROW, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadCursorW, LoadIconW, LoadImageW,
        MB_ICONERROR, MB_OK, MSG, MessageBoxW, RegisterClassExW, SM_CXICON, SM_CXSCREEN, SM_CXSMICON, SM_CYICON,
        SM_CYSCREEN, SM_CYSMICON, SW_SHOW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSEXW, WS_CAPTION,
        WS_CLIPCHILDREN, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    },
};
use windows::core::{Error, PCWSTR, Result, w};

use i18n::{detect_language, main_window_title};
use ui::{
    button::{create_control_buttons, layout_control_buttons, register_button_class, update_control_buttons},
    gdi_plus::GdiPlus,
    theme::apply_theme,
};
use window_proc::window_proc;

const WINDOW_WIDTH: i32 = 800;
const WINDOW_HEIGHT: i32 = 533;
const APP_ICON_RESOURCE_ID: usize = 1;

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
            Some(HMENU::default()),
            Some(instance),
            None,
        )
    }?;

    let _gdi_plus = GdiPlus::new()?;
    create_control_buttons(hwnd, instance)?;
    layout_control_buttons(hwnd)?;
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
