mod theme;
mod window_proc;

use windows::Win32::{
    Foundation::HINSTANCE,
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DispatchMessageW, GetMessageW, HMENU, IDC_ARROW,
        IDI_APPLICATION, LoadCursorW, LoadIconW, MB_ICONERROR, MB_OK, MSG, MessageBoxW, RegisterClassW, SW_SHOW,
        ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WNDCLASSW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED,
        WS_SYSMENU, WS_VISIBLE,
    },
};
use windows::core::{Error, PCWSTR, Result, w};

use theme::apply_theme;
use window_proc::window_proc;

const WINDOW_WIDTH: i32 = 800;
const WINDOW_HEIGHT: i32 = 533;

fn main() {
    if let Err(message) = run() {
        unsafe {
            let text: Vec<u16> = message.to_string().encode_utf16().chain([0]).collect();
            let _ = MessageBoxW(None, PCWSTR(text.as_ptr()), w!("stand-awhile"), MB_OK | MB_ICONERROR);
        }
    }
}

fn run() -> Result<()> {
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
    let class_name = w!("YHB-StandAwhileWindowClass");

    let wnd_class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        hIcon: unsafe { LoadIconW(None, IDI_APPLICATION)? },
        ..Default::default()
    };

    if unsafe { RegisterClassW(&wnd_class) } == 0 {
        return Err(Error::from_win32());
    }

    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE;
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("stand-awhile"),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            None,
            Some(HMENU::default()),
            Some(instance),
            None,
        )
    }?;

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
