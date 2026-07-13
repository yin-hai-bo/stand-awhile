use std::sync::OnceLock;

use crate::{
    i18n::Language,
    ui::{
        hyper_link_text::HyperLinkText,
        theme::{Theme, apply_theme, current_text_color, paint_background, refresh_theme},
    },
};
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DT_CALCRECT, DT_LEFT,
        DT_SINGLELINE, DT_WORDBREAK, DeleteObject, DrawTextW, EndPaint, FF_SWISS, FW_NORMAL, FW_SEMIBOLD,
        GetDeviceCaps, HDC, HFONT, InvalidateRect, LOGPIXELSY, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject,
        SetBkMode, SetTextColor, TRANSPARENT, VARIABLE_PITCH,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            AdjustWindowRectEx, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
            GetWindowLongPtrW, GetWindowRect, IDC_ARROW, LoadCursorW, RegisterClassW, SW_SHOWNORMAL, SetWindowLongPtrW,
            WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT,
            WM_SETTINGCHANGE, WM_SIZE, WM_THEMECHANGED, WNDCLASSW, WS_CAPTION, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
        },
    },
};
use windows::core::{Error, PCWSTR, Result, w};

const ABOUT_CLASS_NAME: windows::core::PCWSTR = w!("YHB-StandAwhileAboutWindow");
const ABOUT_WINDOW_WIDTH: i32 = 460;
const ABOUT_WINDOW_HEIGHT: i32 = 250;
const CONTENT_LEFT: i32 = 28;
const CONTENT_TOP: i32 = 24;
const CONTENT_RIGHT: i32 = 28;
const GITHUB_LABEL_TOP: i32 = 164;
const GITHUB_LABEL_GAP: i32 = 6;
const GITHUB_URL: &str = "https://github.com/yin-hai-bo/stand-awhile";

static ABOUT_CLASS_REGISTRATION: OnceLock<std::result::Result<(), i32>> = OnceLock::new();

struct AboutState {
    language: Language,
    theme: Theme,
    github_link: HyperLinkText,
}

pub fn show_about_window(owner: HWND, language: Language, theme: Theme) -> Result<()> {
    let instance = current_module_instance()?;
    ensure_about_class_registered(instance)?;

    let title = wide_null(about_window_title(language));
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE;
    let ex_style = WINDOW_EX_STYLE::default();
    let (x, y, width, height) = centered_window_rect(owner, style, ex_style)?;

    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            ABOUT_CLASS_NAME,
            PCWSTR(title.as_ptr()),
            style,
            x,
            y,
            width,
            height,
            Some(owner),
            None,
            Some(instance),
            None,
        )
    }?;

    let github_link = HyperLinkText::create(
        hwnd,
        GITHUB_URL,
        |hwnd| {
            let _ = open_url(hwnd, GITHUB_URL);
        },
        about_link_layout,
    )?;

    let state = Box::new(AboutState {
        language,
        theme,
        github_link,
    });
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    }

    layout_about_window(hwnd)?;
    apply_theme(hwnd, theme)?;
    Ok(())
}

unsafe extern "system" fn about_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
            let _ = draw_about_window(hwnd, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            let _ = layout_about_window(hwnd);
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            if let Some(state) = about_state(hwnd) {
                refresh_theme(hwnd, state.theme);
                state.github_link.invalidate();
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        WM_NCDESTROY => {
            release_about_state(hwnd);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn register_about_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(about_window_proc),
        hInstance: instance,
        lpszClassName: ABOUT_CLASS_NAME,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        ..Default::default()
    };

    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(Error::from_win32());
    }

    Ok(())
}

fn ensure_about_class_registered(instance: HINSTANCE) -> Result<()> {
    match ABOUT_CLASS_REGISTRATION.get_or_init(|| register_about_class(instance).map_err(|error| error.code().0)) {
        Ok(()) => Ok(()),
        Err(code) => Err(Error::from(windows::core::HRESULT(*code))),
    }
}

fn current_module_instance() -> Result<HINSTANCE> {
    Ok(unsafe { GetModuleHandleW(None)? }.into())
}

fn centered_window_rect(owner: HWND, style: WINDOW_STYLE, ex_style: WINDOW_EX_STYLE) -> Result<(i32, i32, i32, i32)> {
    let mut window_rect = RECT {
        left: 0,
        top: 0,
        right: ABOUT_WINDOW_WIDTH,
        bottom: ABOUT_WINDOW_HEIGHT,
    };
    unsafe {
        AdjustWindowRectEx(&mut window_rect, style, false, ex_style)?;
    }

    let width = window_rect.right - window_rect.left;
    let height = window_rect.bottom - window_rect.top;
    let mut owner_rect = RECT::default();
    unsafe {
        GetWindowRect(owner, &mut owner_rect)?;
    }

    let x = owner_rect.left + (owner_rect.right - owner_rect.left - width) / 2;
    let y = owner_rect.top + (owner_rect.bottom - owner_rect.top - height) / 2;
    Ok((x, y, width, height))
}

fn draw_about_window(hwnd: HWND, hdc: HDC) -> Result<()> {
    let state = about_state(hwnd).ok_or_else(Error::from_win32)?;
    let mut title_rect = RECT {
        left: CONTENT_LEFT,
        top: CONTENT_TOP,
        right: ABOUT_WINDOW_WIDTH - CONTENT_RIGHT,
        bottom: CONTENT_TOP + 34,
    };
    let mut body_rect = RECT {
        left: CONTENT_LEFT,
        top: CONTENT_TOP + 48,
        right: ABOUT_WINDOW_WIDTH - CONTENT_RIGHT,
        bottom: GITHUB_LABEL_TOP - 10,
    };
    let mut github_label_rect = RECT {
        left: CONTENT_LEFT,
        top: GITHUB_LABEL_TOP,
        right: ABOUT_WINDOW_WIDTH - CONTENT_RIGHT,
        bottom: GITHUB_LABEL_TOP + 24,
    };

    let title_font = create_about_font(hdc, 18, FW_SEMIBOLD.0 as i32);
    if title_font.is_invalid() {
        return Err(Error::from_win32());
    }
    let body_font = create_about_font(hdc, 11, FW_NORMAL.0 as i32);
    if body_font.is_invalid() {
        unsafe {
            let _ = DeleteObject(title_font.into());
        }
        return Err(Error::from_win32());
    }

    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, current_text_color());

        let old_font = SelectObject(hdc, title_font.into());
        let mut title = wide_text("Stand Awhile");
        let _ = DrawTextW(hdc, title.as_mut_slice(), &mut title_rect, DT_LEFT | DT_SINGLELINE);

        let _ = SelectObject(hdc, body_font.into());
        let mut body = wide_text(about_body_text(state.language));
        let _ = DrawTextW(hdc, body.as_mut_slice(), &mut body_rect, DT_LEFT | DT_WORDBREAK);

        let mut github_label = wide_text("GitHub:");
        let _ = DrawTextW(
            hdc,
            github_label.as_mut_slice(),
            &mut github_label_rect,
            DT_LEFT | DT_SINGLELINE,
        );

        let _ = SelectObject(hdc, old_font);
        let _ = DeleteObject(title_font.into());
        let _ = DeleteObject(body_font.into());
    }

    Ok(())
}

fn layout_about_window(hwnd: HWND) -> Result<()> {
    let Some(state) = about_state(hwnd) else {
        return Ok(());
    };

    let hdc = unsafe { windows::Win32::Graphics::Gdi::GetDC(Some(hwnd)) };
    if hdc.is_invalid() {
        return Err(Error::from_win32());
    }

    let result = layout_github_link(&state.github_link, hwnd, hdc);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(Some(hwnd), hdc);
    }
    result
}

fn layout_github_link(link: &HyperLinkText, hwnd: HWND, hdc: HDC) -> Result<()> {
    let body_font = create_about_font(hdc, 11, FW_NORMAL.0 as i32);
    if body_font.is_invalid() {
        return Err(Error::from_win32());
    }

    let label_width = unsafe {
        let old_font = SelectObject(hdc, body_font.into());
        let measured = measure_text_rect(hdc, "GitHub:")?;
        let _ = SelectObject(hdc, old_font);
        let _ = DeleteObject(body_font.into());
        measured.right - measured.left
    };

    let (link_width, link_height) = link.window_size(hdc)?;
    let left = CONTENT_LEFT + label_width + GITHUB_LABEL_GAP;
    let top = GITHUB_LABEL_TOP - 5;
    link.move_to(RECT {
        left,
        top,
        right: left + link_width,
        bottom: top + link_height,
    })?;

    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    Ok(())
}

fn measure_text_rect(hdc: HDC, text: &str) -> Result<RECT> {
    let mut rect = RECT::default();
    let mut wide = wide_text(text);
    unsafe {
        let _ = DrawTextW(
            hdc,
            wide.as_mut_slice(),
            &mut rect,
            DT_LEFT | DT_SINGLELINE | DT_CALCRECT,
        );
    }
    Ok(rect)
}

fn create_about_font(hdc: HDC, point_size: i32, weight: i32) -> HFONT {
    let dpi_y = unsafe { GetDeviceCaps(Some(hdc), LOGPIXELSY) };
    let font_height = -(point_size * dpi_y / 72);

    unsafe {
        CreateFontW(
            font_height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (VARIABLE_PITCH.0 | FF_SWISS.0) as u32,
            w!("Segoe UI"),
        )
    }
}

fn about_link_layout(_: &HyperLinkText, _: HWND, _: HDC) -> Result<()> {
    Ok(())
}

fn open_url(hwnd: HWND, url: &str) -> Result<()> {
    let url = wide_null(url);
    let result = unsafe { ShellExecuteW(Some(hwnd), w!("open"), PCWSTR(url.as_ptr()), None, None, SW_SHOWNORMAL) };
    if (result.0 as usize) <= 32 {
        return Err(Error::from_win32());
    }
    Ok(())
}

fn about_window_title(language: Language) -> &'static str {
    match language {
        Language::Chinese => "关于 站一站",
        Language::English => "About Stand Awhile",
    }
}

fn about_body_text(language: Language) -> &'static str {
    match language {
        Language::Chinese => "一个轻量的 Windows 桌面提醒工具，帮助你定时站起来、伸展身体，减少久坐带来的负担。",
        Language::English => {
            "A lightweight Windows desktop reminder that helps you stand up, stretch, and move regularly during long work sessions."
        }
    }
}

fn about_state(hwnd: HWND) -> Option<&'static AboutState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const AboutState;
    unsafe { raw.as_ref() }
}

fn release_about_state(hwnd: HWND) {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if raw != 0 {
        let _ = unsafe { Box::from_raw(raw as *mut AboutState) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn wide_text(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
