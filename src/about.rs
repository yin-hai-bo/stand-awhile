use std::sync::OnceLock;

use crate::{
    i18n::Language,
    ui::{
        font::common_gui_font,
        hyper_link_text::HyperLinkText,
        theme::{Theme, apply_theme, current_background_color, current_text_color, paint_background, refresh_theme},
    },
};
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateFontIndirectW, DC_BRUSH, DT_CALCRECT, DT_LEFT, DT_SINGLELINE, DeleteObject, DrawTextW,
        EndPaint, FW_SEMIBOLD, GetObjectW, GetStockObject, HDC, HFONT, InvalidateRect, LOGFONTW, PAINTSTRUCT,
        SelectObject, SetBkColor, SetDCBrushColor, SetTextColor,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        SystemServices::{SS_LEFT, SS_NOPREFIX},
    },
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
        Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled},
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
            GetMessageW, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, HICON, IDC_ARROW, IDCANCEL,
            IDI_APPLICATION, IDOK, IMAGE_ICON, IsDialogMessageW, IsWindow, LR_DEFAULTCOLOR, LoadCursorW, LoadIconW,
            LoadImageW, MSG, MoveWindow, PostQuitMessage, RegisterClassW, SM_CXICON, SM_CYICON, SW_SHOWNORMAL,
            SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
            ShowWindow, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
            WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT, WM_SETFONT,
            WM_SETTINGCHANGE, WM_SIZE, WM_THEMECHANGED, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
            WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
        },
    },
};
use windows::core::{Error, PCWSTR, Result, w};

const ABOUT_CLASS_NAME: windows::core::PCWSTR = w!("YHB-StandAwhileAboutWindow");
const ABOUT_WINDOW_WIDTH: i32 = 600;
const ABOUT_WINDOW_HEIGHT: i32 = 320;
const CONTENT_LEFT: i32 = 32;
const CONTENT_TOP: i32 = 32;
const CONTENT_RIGHT: i32 = 32;
const GITHUB_LABEL_TOP: i32 = 240;
const GITHUB_LABEL_GAP: i32 = 6;
const GITHUB_URL: &str = "https://github.com/yin-hai-bo/stand-awhile";
const APP_ICON_RESOURCE_ID: usize = 1;
const BUILD_COMMIT: &str = env!("BUILD_COMMIT");

static ABOUT_CLASS_REGISTRATION: OnceLock<std::result::Result<(), i32>> = OnceLock::new();

struct AboutState {
    language: Language,
    dpi: u32,
    theme: Theme,
    github_link: HyperLinkText,
    labels: [HWND; 4],
    title_font: HFONT,
    body_font: HFONT,
}

impl Drop for AboutState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.title_font.into());
        }
    }
}

pub fn show_about_window(owner: HWND, language: Language, theme: Theme) -> Result<()> {
    if !unsafe { IsWindowEnabled(owner) }.as_bool() {
        return Ok(());
    }
    let instance = current_module_instance()?;
    ensure_about_class_registered(instance)?;

    let title = wide_null(about_window_title(language));
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
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

    initialize_about_window(hwnd, language, theme)?;
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
    }
    run_about_modal(hwnd, owner)
}

fn run_about_modal(hwnd: HWND, owner: HWND) -> Result<()> {
    let owner_enabled = unsafe { IsWindowEnabled(owner) }.as_bool();
    unsafe {
        if owner_enabled {
            let _ = EnableWindow(owner, false);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    let result = (|| {
        let mut message = MSG::default();
        while unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
            if status == -1 {
                return Err(Error::from_win32());
            }
            if status == 0 {
                unsafe {
                    PostQuitMessage(message.wParam.0 as i32);
                }
                break;
            }
            if !unsafe { IsDialogMessageW(hwnd, &message) }.as_bool() {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        Ok(())
    })();
    unsafe {
        if owner_enabled && IsWindow(Some(owner)).as_bool() {
            let _ = EnableWindow(owner, true);
        }
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        if owner_enabled && IsWindow(Some(owner)).as_bool() {
            let _ = SetForegroundWindow(owner);
        }
    }
    result
}

fn initialize_about_window(hwnd: HWND, language: Language, theme: Theme) -> Result<()> {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let Some(body_font) = common_gui_font(dpi, language == Language::Chinese) else {
        let error = Error::from_win32();
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    };
    let github_link = match HyperLinkText::create(
        hwnd,
        GITHUB_URL,
        body_font,
        dpi,
        |hwnd| {
            let _ = open_url(hwnd, GITHUB_URL);
        },
        about_link_layout,
    ) {
        Ok(link) => link,
        Err(error) => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return Err(error);
        }
    };

    let mut state = Box::new(AboutState {
        language,
        dpi,
        theme,
        github_link,
        labels: [HWND::default(); 4],
        title_font: HFONT::default(),
        body_font,
    });
    if let Err(error) = create_about_labels(hwnd, language, &mut state) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    }

    let result = (|| {
        layout_about_window(hwnd)?;
        apply_theme(hwnd, theme)?;
        invalidate_about_labels(hwnd);
        Ok(())
    })();
    if let Err(error) = result {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }
    Ok(())
}

unsafe extern "system" fn about_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => LRESULT(0),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DPICHANGED => {
            let dpi = (wparam.0 & 0xffff) as u32;
            let _ = update_about_dpi(hwnd, dpi);
            let suggested = unsafe { &*(lparam.0 as *const RECT) };
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            let _ = layout_about_window(hwnd);
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as _);
            unsafe {
                let _ = SetTextColor(hdc, current_text_color());
                let _ = SetBkColor(hdc, current_background_color());
                let _ = SetDCBrushColor(hdc, current_background_color());
                LRESULT(GetStockObject(DC_BRUSH).0 as isize)
            }
        }
        WM_SIZE => {
            let _ = layout_about_window(hwnd);
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            if let Some(state) = about_state(hwnd) {
                refresh_theme(hwnd, state.theme);
                state.github_link.invalidate();
                invalidate_about_labels(hwnd);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND if (wparam.0 & 0xffff) as i32 == IDOK.0 || (wparam.0 & 0xffff) as i32 == IDCANCEL.0 => {
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
        hIcon: load_app_icon(instance),
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

fn load_app_icon(instance: HINSTANCE) -> HICON {
    let resource = PCWSTR(APP_ICON_RESOURCE_ID as *const u16);
    let icon = unsafe {
        LoadImageW(
            Some(instance),
            resource,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXICON),
            GetSystemMetrics(SM_CYICON),
            LR_DEFAULTCOLOR,
        )
        .ok()
    };

    icon.map(|handle| HICON(handle.0))
        .unwrap_or_else(|| unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() })
}

fn centered_window_rect(owner: HWND, style: WINDOW_STYLE, ex_style: WINDOW_EX_STYLE) -> Result<(i32, i32, i32, i32)> {
    let dpi = unsafe { GetDpiForWindow(owner) }.max(96);
    let mut window_rect = RECT {
        left: 0,
        top: 0,
        right: crate::scale_dimension(ABOUT_WINDOW_WIDTH, dpi),
        bottom: crate::scale_dimension(ABOUT_WINDOW_HEIGHT, dpi),
    };
    unsafe {
        AdjustWindowRectExForDpi(&mut window_rect, style, false, ex_style, dpi)?;
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

fn create_about_labels(hwnd: HWND, language: Language, state: &mut AboutState) -> Result<()> {
    state.title_font = create_title_font(state.body_font)?;

    let commit = format!("Commit: {BUILD_COMMIT}");
    let labels = [
        ("Stand Awhile", CONTENT_TOP, 40, state.title_font),
        (about_description(language), CONTENT_TOP + 64, 72, state.body_font),
        (commit.as_str(), 184, 32, state.body_font),
        ("GitHub:", GITHUB_LABEL_TOP, 32, state.body_font),
    ];
    for (index, (text, top, height, font)) in labels.into_iter().enumerate() {
        let text = wide_null(text);
        let label = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR(text.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_LEFT.0 | SS_NOPREFIX.0),
                crate::scale_dimension(CONTENT_LEFT, state.dpi),
                crate::scale_dimension(top, state.dpi),
                crate::scale_dimension(ABOUT_WINDOW_WIDTH - CONTENT_LEFT - CONTENT_RIGHT, state.dpi),
                crate::scale_dimension(height, state.dpi),
                Some(hwnd),
                None,
                Some(current_module_instance()?),
                None,
            )?
        };
        state.labels[index] = label;
        unsafe {
            SendMessageW(label, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
    }
    Ok(())
}

fn invalidate_about_labels(hwnd: HWND) {
    if let Some(state) = about_state(hwnd) {
        for label in state.labels.into_iter().filter(|label| !label.0.is_null()) {
            unsafe {
                let _ = InvalidateRect(Some(label), None, true);
            }
        }
    }
}

fn update_about_dpi(hwnd: HWND, dpi: u32) -> Result<()> {
    let Some(state) = about_state(hwnd) else {
        return Ok(());
    };
    if state.dpi == dpi {
        return Ok(());
    }
    let body_font = common_gui_font(dpi, state.language == Language::Chinese).ok_or_else(Error::from_win32)?;
    let title_font = create_title_font(body_font)?;
    if let Err(error) = state.github_link.set_font(body_font, dpi) {
        unsafe {
            let _ = DeleteObject(title_font.into());
        }
        return Err(error);
    }
    let labels = state.labels;
    let old_title = state.title_font;
    unsafe {
        let state = &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AboutState);
        state.dpi = dpi;
        state.title_font = title_font;
        state.body_font = body_font;
    }
    for (index, label) in labels.into_iter().enumerate() {
        let font = if index == 0 { title_font } else { body_font };
        unsafe {
            SendMessageW(label, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
    }
    unsafe {
        let _ = DeleteObject(old_title.into());
    }
    Ok(())
}

fn layout_about_window(hwnd: HWND) -> Result<()> {
    let Some(state) = about_state(hwnd) else {
        return Ok(());
    };
    if state.labels[3].0.is_null() {
        return Ok(());
    }

    let hdc = unsafe { windows::Win32::Graphics::Gdi::GetDC(Some(hwnd)) };
    if hdc.is_invalid() {
        return Err(Error::from_win32());
    }

    let result = layout_github_link(state, hwnd, hdc);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(Some(hwnd), hdc);
    }
    result
}

fn layout_github_link(state: &AboutState, hwnd: HWND, hdc: HDC) -> Result<()> {
    let scale = |value| crate::scale_dimension(value, state.dpi);
    for (label, (top, height)) in state.labels[..3]
        .iter()
        .zip([(CONTENT_TOP, 40), (CONTENT_TOP + 64, 72), (184, 32)])
    {
        unsafe {
            MoveWindow(
                *label,
                scale(CONTENT_LEFT),
                scale(top),
                scale(ABOUT_WINDOW_WIDTH - CONTENT_LEFT - CONTENT_RIGHT),
                scale(height),
                true,
            )?;
        }
    }
    let body_font = state.body_font;

    let (label_width, label_height) = unsafe {
        let old_font = SelectObject(hdc, body_font.into());
        let measured = measure_text_rect(hdc, "GitHub:")?;
        let _ = SelectObject(hdc, old_font);
        (measured.right - measured.left, measured.bottom - measured.top)
    };

    unsafe {
        MoveWindow(
            state.labels[3],
            scale(CONTENT_LEFT),
            scale(GITHUB_LABEL_TOP),
            label_width,
            scale(32),
            true,
        )?;
    }

    let (link_width, link_height) = state.github_link.window_size(hdc)?;
    let left = scale(CONTENT_LEFT) + label_width + scale(GITHUB_LABEL_GAP);
    let top = scale(GITHUB_LABEL_TOP) - (link_height - label_height) / 2;
    state.github_link.move_to(RECT {
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

fn create_title_font(body_font: HFONT) -> Result<HFONT> {
    let mut font = LOGFONTW::default();
    if unsafe {
        GetObjectW(
            body_font.into(),
            std::mem::size_of::<LOGFONTW>() as i32,
            Some((&mut font as *mut LOGFONTW).cast()),
        )
    } == 0
    {
        return Err(Error::from_win32());
    }
    font.lfHeight = font.lfHeight * 18 / 11;
    font.lfWeight = FW_SEMIBOLD.0 as i32;
    let handle = unsafe { CreateFontIndirectW(&font) };
    if handle.is_invalid() {
        Err(Error::from_win32())
    } else {
        Ok(handle)
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

fn about_description(language: Language) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{
        Graphics::Gdi::DT_WORDBREAK,
        UI::{
            Input::KeyboardAndMouse::VK_ESCAPE,
            WindowsAndMessaging::{
                GetClientRect, PM_REMOVE, PeekMessageW, PostMessageW, WM_APP, WM_GETFONT, WM_KEYDOWN, WM_QUIT,
            },
        },
    };

    struct TestWindow(HWND);

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    unsafe extern "system" fn modal_owner_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == WM_APP {
            let disabled = !unsafe { IsWindowEnabled(hwnd) }.as_bool();
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, disabled as isize);
            }
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    fn modal_test_windows() -> Result<(TestWindow, TestWindow)> {
        let instance = current_module_instance()?;
        ensure_about_class_registered(instance)?;
        static OWNER_CLASS: OnceLock<()> = OnceLock::new();
        let class_name = w!("YHB-AboutModalTestOwner");
        OWNER_CLASS.get_or_init(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(modal_owner_proc),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            assert_ne!(unsafe { RegisterClassW(&class) }, 0);
        });
        let create = |class, owner| unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                w!("Modal test"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                0,
                0,
                600,
                320,
                owner,
                None,
                Some(instance),
                None,
            )
            .map(TestWindow)
        };
        let owner = create(class_name, None)?;
        let about = create(ABOUT_CLASS_NAME, Some(owner.0))?;
        Ok((owner, about))
    }

    #[test]
    fn modal_disables_owner_until_close_or_escape_and_restores_enabled_state() -> Result<()> {
        for close_message in [WM_CLOSE, WM_KEYDOWN] {
            for initially_enabled in [true, false] {
                let (owner, about) = modal_test_windows()?;
                unsafe {
                    let _ = EnableWindow(owner.0, initially_enabled);
                    PostMessageW(Some(owner.0), WM_APP, WPARAM(0), LPARAM(0))?;
                    PostMessageW(Some(about.0), close_message, WPARAM(VK_ESCAPE.0 as usize), LPARAM(0))?;
                }
                run_about_modal(about.0, owner.0)?;
                assert_eq!(
                    unsafe { GetWindowLongPtrW(owner.0, GWLP_USERDATA) },
                    1,
                    "owner must be disabled while modal messages are dispatched"
                );
                assert_eq!(unsafe { IsWindowEnabled(owner.0) }.as_bool(), initially_enabled);
                assert!(!unsafe { IsWindow(Some(about.0)) }.as_bool());
            }
        }
        Ok(())
    }

    #[test]
    fn modal_preserves_quit_message_and_cleans_up_dialog() -> Result<()> {
        let (owner, about) = modal_test_windows()?;
        unsafe {
            PostQuitMessage(7);
        }
        run_about_modal(about.0, owner.0)?;
        let mut message = MSG::default();
        assert!(unsafe { PeekMessageW(&mut message, None, WM_QUIT, WM_QUIT, PM_REMOVE) }.as_bool());
        assert_eq!(message.wParam.0, 7);
        assert!(unsafe { IsWindowEnabled(owner.0) }.as_bool());
        assert!(!unsafe { IsWindow(Some(about.0)) }.as_bool());
        Ok(())
    }

    fn font_info(label: HWND) -> LOGFONTW {
        let font = unsafe { SendMessageW(label, WM_GETFONT, None, None) };
        let mut info = LOGFONTW::default();
        assert_ne!(
            unsafe {
                GetObjectW(
                    windows::Win32::Graphics::Gdi::HGDIOBJ(font.0 as _),
                    std::mem::size_of::<LOGFONTW>() as i32,
                    Some((&mut info as *mut LOGFONTW).cast()),
                )
            },
            0
        );
        info
    }

    fn check_dpi_changes(language: Language) -> Result<()> {
        let instance = current_module_instance()?;
        ensure_about_class_registered(instance)?;
        let window = TestWindow(unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                ABOUT_CLASS_NAME,
                w!("DPI regression test"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
                0,
                0,
                600,
                320,
                None,
                None,
                Some(instance),
                None,
            )?
        });
        initialize_about_window(window.0, language, Theme::Light)?;
        let initial_dpi = unsafe { GetDpiForWindow(window.0) }.max(96);
        let state = about_state(window.0).unwrap();
        assert_eq!(
            font_info(state.labels[0]).lfHeight,
            font_info(state.labels[1]).lfHeight * 18 / 11
        );
        let mut initial_rect = RECT::default();
        unsafe {
            GetWindowRect(state.labels[1], &mut initial_rect)?;
        }
        assert_eq!(
            initial_rect.right - initial_rect.left,
            crate::scale_dimension(536, initial_dpi)
        );
        let other_font = common_gui_font(96, language == Language::Chinese).unwrap();
        let other_link = HyperLinkText::create(window.0, GITHUB_URL, other_font, 96, |_| {}, about_link_layout)?;
        for dpi in [144usize, 192, 96, 144] {
            let suggested = RECT {
                left: 20,
                top: 20,
                right: 20 + 600 * dpi as i32 / 96,
                bottom: 20 + 320 * dpi as i32 / 96,
            };
            unsafe {
                SendMessageW(
                    window.0,
                    WM_DPICHANGED,
                    Some(WPARAM(dpi | (dpi << 16))),
                    Some(LPARAM((&suggested as *const RECT) as isize)),
                );
            }
            let mut actual = RECT::default();
            unsafe {
                GetWindowRect(window.0, &mut actual)?;
            }
            assert_eq!(actual, suggested, "window must apply the suggested DPI rectangle");
            let state = about_state(window.0).unwrap();
            let shared_font = common_gui_font(dpi as u32, language == Language::Chinese).unwrap();
            for label in &state.labels[1..] {
                assert_eq!(
                    unsafe { SendMessageW(*label, WM_GETFONT, None, None) }.0,
                    shared_font.0 as isize
                );
            }
            let body = font_info(state.labels[1]);
            let title = font_info(state.labels[0]);
            let link = font_info(state.github_link.hwnd());
            assert_eq!(title.lfFaceName, body.lfFaceName);
            assert_eq!(title.lfHeight, body.lfHeight * 18 / 11);
            assert_eq!(title.lfWeight, FW_SEMIBOLD.0 as i32);
            assert_eq!(link.lfFaceName, body.lfFaceName);
            assert_eq!(link.lfHeight, body.lfHeight);
            assert_eq!(link.lfUnderline, 1);
            unsafe {
                GetWindowRect(state.labels[1], &mut actual)?;
            }
            assert_eq!(actual.right - actual.left, 536 * dpi as i32 / 96);
            assert_eq!(actual.bottom - actual.top, 72 * dpi as i32 / 96);
            // A link at another DPI must not change this dialog's font or measurements.
            other_link.set_font(other_font, 96)?;
            let hdc = unsafe { windows::Win32::Graphics::Gdi::GetDC(Some(window.0)) };
            let large = state.github_link.window_size(hdc)?;
            let small = other_link.window_size(hdc)?;
            let large_again = state.github_link.window_size(hdc)?;
            let mut description_rect = RECT {
                right: actual.right - actual.left,
                ..Default::default()
            };
            let mut description = wide_text(about_description(language));
            let old_font = unsafe { SelectObject(hdc, state.body_font.into()) };
            unsafe {
                DrawTextW(
                    hdc,
                    &mut description,
                    &mut description_rect,
                    DT_CALCRECT | DT_LEFT | DT_WORDBREAK,
                );
            }
            let label_rect = measure_text_rect(hdc, "GitHub:")?;
            unsafe {
                let _ = SelectObject(hdc, old_font);
            }
            unsafe {
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(Some(window.0), hdc);
            }
            assert!(
                description_rect.bottom <= actual.bottom - actual.top,
                "description must fit its STATIC control"
            );
            assert_eq!(large, large_again);
            if dpi > 96 {
                assert!(large.0 > small.0 * 13 / 10, "link font must grow with DPI");
                assert!(large.1 > small.1, "link height must grow with DPI");
            }
            unsafe {
                GetWindowRect(state.github_link.hwnd(), &mut actual)?;
            }
            assert_eq!(actual.right - actual.left, large.0);
            assert_eq!(actual.bottom - actual.top, large.1);
            let link_top = actual.top;
            unsafe {
                GetWindowRect(state.labels[3], &mut actual)?;
            }
            assert!(
                (link_top + (large.1 - label_rect.bottom) / 2 - actual.top).abs() <= 1,
                "GitHub label and link text must align vertically"
            );
            let mut client = RECT::default();
            unsafe {
                GetClientRect(window.0, &mut client)?;
            }
            assert!(
                crate::scale_dimension(CONTENT_LEFT + GITHUB_LABEL_GAP + CONTENT_RIGHT, dpi as u32)
                    + label_rect.right
                    + large.0
                    <= client.right,
                "link must fit horizontally"
            );
        }
        let state = about_state(window.0).unwrap();
        let body_font = state.body_font;
        let title_font = state.title_font;
        let link_font = unsafe { SendMessageW(state.github_link.hwnd(), WM_GETFONT, None, None) };
        drop(window);
        let mut info = LOGFONTW::default();
        for owned in [title_font.0, link_font.0 as _] {
            assert_eq!(
                unsafe {
                    GetObjectW(
                        windows::Win32::Graphics::Gdi::HGDIOBJ(owned),
                        std::mem::size_of::<LOGFONTW>() as i32,
                        Some((&mut info as *mut LOGFONTW).cast()),
                    )
                },
                0
            );
        }
        assert_ne!(
            unsafe {
                GetObjectW(
                    body_font.into(),
                    std::mem::size_of::<LOGFONTW>() as i32,
                    Some((&mut info as *mut LOGFONTW).cast()),
                )
            },
            0,
            "closing About must preserve the shared font"
        );
        Ok(())
    }

    #[test]
    fn dpi_changed_resizes_window_controls_and_fonts_without_accumulating_scale() -> Result<()> {
        check_dpi_changes(Language::English)?;
        check_dpi_changes(Language::Chinese)
    }
}
