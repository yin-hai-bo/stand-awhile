use std::{rc::Rc, sync::OnceLock};

use crate::{
    i18n::Language,
    ui::{
        font::common_gui_font,
        hyper_link_text::HyperLinkText,
        theme::{
            Theme, apply_theme, current_background_color, current_text_color, is_dark_theme_active, paint_background,
            refresh_theme,
        },
    },
};
use windows::Win32::{
    Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateFontIndirectW, DC_BRUSH, DT_CALCRECT, DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER,
        DeleteObject, DrawFocusRect, DrawTextW, EndPaint, FW_SEMIBOLD, FillRect, FrameRect, GetObjectW, GetStockObject,
        HBRUSH, HDC, HFONT, InvalidateRect, LOGFONTW, PAINTSTRUCT, RestoreDC, SaveDC, SelectObject, SetBkColor,
        SetBkMode, SetDCBrushColor, SetTextColor, TRANSPARENT,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        SystemServices::{SS_LEFT, SS_NOPREFIX},
    },
    UI::{
        Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED},
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow},
        Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled, SetFocus},
        Shell::ShellExecuteW,
        WindowsAndMessaging::{
            BS_OWNERDRAW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DC_HASDEFID, DM_GETDEFID, DefWindowProcW,
            DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW, GetSystemMetrics,
            GetWindowLongPtrW, GetWindowRect, HICON, HMENU, IDC_ARROW, IDCANCEL, IDI_APPLICATION, IDOK, IMAGE_ICON,
            IsDialogMessageW, IsWindow, LR_DEFAULTCOLOR, LoadCursorW, LoadIconW, LoadImageW, MSG, MoveWindow,
            PostQuitMessage, RegisterClassW, SM_CXICON, SM_CYICON, SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOZORDER,
            SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
            WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORSTATIC, WM_DESTROY,
            WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_NCDESTROY, WM_PAINT, WM_SETFONT, WM_SETTINGCHANGE, WM_SIZE,
            WM_THEMECHANGED, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP,
            WS_VISIBLE,
        },
    },
};
use windows::core::{Error, PCWSTR, Result, w};

const ABOUT_CLASS_NAME: windows::core::PCWSTR = w!("YHB-StandAwhileAboutWindow");
const ABOUT_WINDOW_WIDTH: i32 = 600;
const ABOUT_WINDOW_HEIGHT: i32 = 480;
const CONTENT_LEFT: i32 = 32;
const CONTENT_TOP: i32 = 24;
const CONTENT_RIGHT: i32 = 32;
const GITHUB_LABEL_TOP: i32 = 156;
const GITHUB_LABEL_GAP: i32 = 6;
const GITHUB_URL: &str = "https://github.com/yin-hai-bo/stand-awhile";
const ASSET_SOURCE_URL: &str = "https://opengameart.org/content/cat-dog-free-sprites";
const ASSET_LICENSE_URL: &str = "https://creativecommons.org/publicdomain/zero/1.0/legalcode.en";
const CREDIT_DIVIDER_TOP: i32 = 228;
const CREDIT_SOURCE_TOP: i32 = 308;
const CREDIT_LICENSE_TOP: i32 = 336;
const VERSION_LABEL: usize = 9;
const OK_BUTTON_WIDTH: i32 = 88;
const OK_BUTTON_HEIGHT: i32 = 32;
const OK_BUTTON_BOTTOM: i32 = 24;
const LABEL_LAYOUT: [(i32, i32); 10] = [
    (CONTENT_TOP, 32),
    (80, 56),
    (188, 24),
    (GITHUB_LABEL_TOP, 32),
    (244, 24),
    (268, 36),
    (CREDIT_SOURCE_TOP, 24),
    (CREDIT_LICENSE_TOP, 24),
    (CREDIT_LICENSE_TOP, 40),
    (56, 20),
];
const APP_ICON_RESOURCE_ID: usize = 1;
const BUILD_COMMIT: &str = env!("BUILD_COMMIT");

static ABOUT_CLASS_REGISTRATION: OnceLock<std::result::Result<(), i32>> = OnceLock::new();

#[derive(Clone)]
struct AboutState {
    language: Language,
    dpi: u32,
    theme: Theme,
    github_link: HyperLinkText,
    credit_links: Vec<HyperLinkText>,
    labels: [HWND; 10],
    ok_button: HWND,
    title_font: Rc<AboutFont>,
    body_font: HFONT,
}

struct AboutFont(HFONT);

impl Drop for AboutFont {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
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
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Ok(());
    }
    let owner_enabled = unsafe { IsWindowEnabled(owner) }.as_bool();
    unsafe {
        if owner_enabled {
            let _ = EnableWindow(owner, false);
        }
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = SetForegroundWindow(hwnd);
            if let Some(state) = about_state(hwnd) {
                let _ = SetFocus(Some(state.ok_button));
            }
        }
    }
    let mut quit_code = None;
    let result = (|| {
        let mut message = MSG::default();
        while unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            let status = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
            if status == -1 {
                return Err(Error::from_win32());
            }
            if status == 0 {
                quit_code = Some(message.wParam.0 as i32);
                break;
            }
            if !unsafe { IsWindow(Some(hwnd)).as_bool() && IsDialogMessageW(hwnd, &message).as_bool() } {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        Ok(())
    })();
    unsafe {
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        if owner_enabled && IsWindow(Some(owner)).as_bool() {
            let _ = EnableWindow(owner, true);
        }
        if owner_enabled && IsWindow(Some(owner)).as_bool() {
            let _ = SetForegroundWindow(owner);
        }
    }
    if let Some(code) = quit_code {
        unsafe {
            PostQuitMessage(code);
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
    let github_link = match HyperLinkText::create(hwnd, GITHUB_URL, body_font, dpi, |hwnd| {
        let _ = open_url(hwnd, GITHUB_URL);
    }) {
        Ok(link) => link,
        Err(error) => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return Err(error);
        }
    };

    let credit_links = credit_link_specs()
        .into_iter()
        .map(|(text, url)| {
            HyperLinkText::create(hwnd, text, body_font, dpi, move |hwnd| {
                let _ = open_url(hwnd, url);
            })
        })
        .collect::<Result<Vec<_>>>();
    let credit_links = match credit_links {
        Ok(links) => links,
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
        credit_links,
        labels: [HWND::default(); 10],
        ok_button: HWND::default(),
        title_font: Rc::new(AboutFont(HFONT::default())),
        body_font,
    });
    if let Err(error) = create_about_labels(hwnd, language, &mut state) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return Err(error);
    }
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err(Error::from_win32());
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
            if let Some(state) = about_state(hwnd) {
                paint_credit_divider(hwnd, hdc, state.dpi);
            }
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        DM_GETDEFID => LRESULT(((DC_HASDEFID << 16) | IDOK.0 as u32) as isize),
        WM_DRAWITEM => {
            let item = unsafe { (lparam.0 as *const DRAWITEMSTRUCT).as_ref() };
            if let (Some(item), Some(state)) = (item, about_state(hwnd)) {
                if item.hwndItem == state.ok_button {
                    draw_about_ok_button(item, &state);
                    return LRESULT(1);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_DPICHANGED => {
            let dpi = ((wparam.0 & 0xffff) as u32).max(96);
            let suggested = unsafe { (lparam.0 as *const RECT).as_ref().copied() };
            let _ = update_about_dpi(hwnd, dpi);
            if !about_state(hwnd).is_some_and(|state| state.dpi == dpi) {
                return LRESULT(0);
            }
            if let Some(suggested) = suggested {
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
                for link in &state.credit_links {
                    link.invalidate();
                }
                invalidate_about_labels(hwnd);
                unsafe {
                    let _ = InvalidateRect(Some(state.ok_button), None, true);
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
    state.title_font = Rc::new(AboutFont(create_title_font(state.body_font)?));

    let commit = format!("Commit: {BUILD_COMMIT}");
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let labels = [
        (about_app_name(language), state.title_font.0),
        (about_description(language), state.body_font),
        (commit.as_str(), state.body_font),
        ("GitHub:", state.body_font),
        (credit_heading(language), state.body_font),
        (credit_thanks(language), state.body_font),
        (credit_source_prefix(language), state.body_font),
        (credit_license_prefix(language), state.body_font),
        (credit_license_suffix(language), state.body_font),
        (version.as_str(), state.body_font),
    ];
    for (index, ((text, font), (top, height))) in labels.into_iter().zip(LABEL_LAYOUT).enumerate() {
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
    let text = wide_null(match language {
        Language::Chinese => "确定",
        Language::English => "OK",
    });
    state.ok_button = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
            0,
            0,
            1,
            1,
            Some(hwnd),
            Some(HMENU(IDOK.0 as usize as _)),
            Some(current_module_instance()?),
            None,
        )?
    };
    unsafe {
        SendMessageW(
            state.ok_button,
            WM_SETFONT,
            Some(WPARAM(state.body_font.0 as usize)),
            Some(LPARAM(1)),
        );
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
    let dpi = dpi.max(96);
    let Some(state) = about_state(hwnd) else {
        return Ok(());
    };
    if state.dpi == dpi {
        return Ok(());
    }
    let body_font = common_gui_font(dpi, state.language == Language::Chinese).ok_or_else(Error::from_win32)?;
    let title_font = Rc::new(AboutFont(create_title_font(body_font)?));
    state.github_link.set_font(body_font, dpi)?;
    for link in &state.credit_links {
        if !about_layout_is_current(hwnd, &state) {
            return Ok(());
        }
        link.set_font(body_font, dpi)?;
    }
    if !about_layout_is_current(hwnd, &state) {
        return Ok(());
    }
    let labels = state.labels;
    let previous = with_about_state(hwnd, |current| {
        current.dpi = dpi;
        current.body_font = body_font;
        std::mem::replace(&mut current.title_font, title_font.clone())
    });
    let Some(previous) = previous else {
        return Ok(());
    };
    let current = about_state(hwnd).unwrap();
    for (index, label) in labels.into_iter().enumerate() {
        if !about_layout_is_current(hwnd, &current) {
            return Ok(());
        }
        let font = if index == 0 { title_font.0 } else { body_font };
        unsafe {
            SendMessageW(label, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }
    }
    if about_layout_is_current(hwnd, &current) {
        unsafe {
            SendMessageW(
                state.ok_button,
                WM_SETFONT,
                Some(WPARAM(body_font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
    }
    drop(previous);
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

    let result = layout_github_link(&state, hwnd, hdc);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::ReleaseDC(Some(hwnd), hdc);
    }
    result
}

fn layout_github_link(state: &AboutState, hwnd: HWND, hdc: HDC) -> Result<()> {
    let scale = |value| crate::scale_dimension(value, state.dpi);
    for (label, (top, height)) in state.labels.iter().zip(LABEL_LAYOUT) {
        if !about_layout_is_current(hwnd, state) {
            return Ok(());
        }
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
    if !about_layout_is_current(hwnd, state) {
        return Ok(());
    }
    let title_rect = unsafe {
        let old_font = SelectObject(hdc, state.title_font.0.into());
        let measured = measure_text_rect(hdc, about_app_name(state.language))?;
        SelectObject(hdc, old_font);
        measured
    };
    let version_rect = unsafe {
        let old_font = SelectObject(hdc, state.body_font.into());
        let measured = measure_text_rect(hdc, &format!("v{}", env!("CARGO_PKG_VERSION")))?;
        SelectObject(hdc, old_font);
        measured
    };
    let title_ascent = font_ascent(hdc, state.title_font.0)?;
    let version_ascent = font_ascent(hdc, state.body_font)?;
    unsafe {
        MoveWindow(
            state.labels[0],
            scale(CONTENT_LEFT),
            scale(CONTENT_TOP),
            title_rect.right,
            title_rect.bottom,
            true,
        )?;
    }
    if !about_layout_is_current(hwnd, state) {
        return Ok(());
    }
    unsafe {
        MoveWindow(
            state.labels[VERSION_LABEL],
            scale(CONTENT_LEFT) + title_rect.right + scale(12),
            scale(CONTENT_TOP) + title_ascent - version_ascent,
            version_rect.right,
            version_rect.bottom,
            true,
        )?;
    }
    if !about_layout_is_current(hwnd, state) {
        return Ok(());
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

    if !about_layout_is_current(hwnd, state) {
        return Ok(());
    }
    let mut client = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut client)?;
        MoveWindow(
            state.ok_button,
            client.right - scale(CONTENT_RIGHT + OK_BUTTON_WIDTH),
            client.bottom - scale(OK_BUTTON_BOTTOM + OK_BUTTON_HEIGHT),
            scale(OK_BUTTON_WIDTH),
            scale(OK_BUTTON_HEIGHT),
            true,
        )?;
    }
    if !about_layout_is_current(hwnd, state) {
        return Ok(());
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

    for (index, label_index, row_top, prefix) in [
        (0, 6, CREDIT_SOURCE_TOP, credit_source_prefix(state.language)),
        (1, 7, CREDIT_LICENSE_TOP, credit_license_prefix(state.language)),
    ] {
        if !about_layout_is_current(hwnd, state) {
            return Ok(());
        }
        let prefix_rect = unsafe {
            let old_font = SelectObject(hdc, body_font.into());
            let measured = measure_text_rect(hdc, prefix)?;
            let _ = SelectObject(hdc, old_font);
            measured
        };
        unsafe {
            MoveWindow(
                state.labels[label_index],
                scale(CONTENT_LEFT),
                scale(row_top),
                prefix_rect.right,
                scale(24),
                true,
            )?;
        }
        if !about_layout_is_current(hwnd, state) {
            return Ok(());
        }
        let link = &state.credit_links[index];
        let (_, height) = link.window_size(hdc)?;
        let width = link.measure_text_rect(hdc)?.right;
        let left = scale(CONTENT_LEFT) + prefix_rect.right;
        let top = scale(row_top) - (height - prefix_rect.bottom) / 2;
        link.move_to(RECT {
            left,
            top,
            right: left + width,
            bottom: top + height,
        })?;

        if index == 1 {
            if !about_layout_is_current(hwnd, state) {
                return Ok(());
            }
            unsafe {
                MoveWindow(
                    state.labels[8],
                    left + width,
                    scale(row_top),
                    scale(ABOUT_WINDOW_WIDTH - CONTENT_RIGHT) - left - width,
                    scale(40),
                    true,
                )?;
            }
        }
    }

    if !about_layout_is_current(hwnd, state) {
        return Ok(());
    }
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

fn font_ascent(hdc: HDC, font: HFONT) -> Result<i32> {
    let mut metrics = windows::Win32::Graphics::Gdi::TEXTMETRICW::default();
    let success = unsafe {
        let previous = SelectObject(hdc, font.into());
        let success = windows::Win32::Graphics::Gdi::GetTextMetricsW(hdc, &mut metrics).as_bool();
        SelectObject(hdc, previous);
        success
    };
    if success {
        Ok(metrics.tmAscent)
    } else {
        Err(Error::from_win32())
    }
}

fn paint_credit_divider(hwnd: HWND, hdc: HDC, dpi: u32) {
    let mut client = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client) }.is_err() {
        return;
    }
    let background = current_background_color().0;
    let text = current_text_color().0;
    let component = |shift: u32| (((background >> shift) & 255u32) * 3 + ((text >> shift) & 255u32)) / 4;
    let color = COLORREF(component(0) | (component(8) << 8) | (component(16) << 16));
    let line = RECT {
        left: crate::scale_dimension(CONTENT_LEFT, dpi),
        top: crate::scale_dimension(CREDIT_DIVIDER_TOP, dpi),
        right: client.right - crate::scale_dimension(CONTENT_RIGHT, dpi),
        bottom: crate::scale_dimension(CREDIT_DIVIDER_TOP, dpi) + 1,
    };
    unsafe {
        let previous = SetDCBrushColor(hdc, color);
        FillRect(hdc, &line, HBRUSH(GetStockObject(DC_BRUSH).0));
        SetDCBrushColor(hdc, previous);
    }
}

fn draw_about_ok_button(item: &DRAWITEMSTRUCT, state: &AboutState) {
    let dark = is_dark_theme_active();
    let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
    let fill = match (dark, pressed) {
        (true, false) => COLORREF(0x00303030),
        (true, true) => COLORREF(0x00404040),
        (false, false) => COLORREF(0x00e4e4e4),
        (false, true) => COLORREF(0x00d0d0d0),
    };
    let border = if dark {
        COLORREF(0x00808080)
    } else {
        COLORREF(0x00909090)
    };
    let mut text = wide_text(match state.language {
        Language::Chinese => "确定",
        Language::English => "OK",
    });
    let mut rect = item.rcItem;
    if pressed {
        rect.left += 1;
        rect.top += 1;
    }
    unsafe {
        let saved = SaveDC(item.hDC);
        let brush = HBRUSH(GetStockObject(DC_BRUSH).0);
        SetDCBrushColor(item.hDC, fill);
        FillRect(item.hDC, &item.rcItem, brush);
        SetDCBrushColor(item.hDC, border);
        FrameRect(item.hDC, &item.rcItem, brush);
        SelectObject(item.hDC, state.body_font.into());
        SetBkMode(item.hDC, TRANSPARENT);
        SetTextColor(item.hDC, current_text_color());
        DrawTextW(item.hDC, &mut text, &mut rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        if item.itemState.0 & ODS_FOCUS.0 != 0 && item.itemState.0 & ODS_NOFOCUSRECT.0 == 0 {
            let inset = crate::scale_dimension(4, state.dpi);
            let focus = RECT {
                left: item.rcItem.left + inset,
                top: item.rcItem.top + inset,
                right: item.rcItem.right - inset,
                bottom: item.rcItem.bottom - inset,
            };
            let _ = DrawFocusRect(item.hDC, &focus);
        }
        let _ = RestoreDC(item.hDC, saved);
    }
}

fn about_app_name(language: Language) -> &'static str {
    match language {
        Language::Chinese => "站一站",
        Language::English => "Stand Awhile",
    }
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

fn credit_heading(language: Language) -> &'static str {
    match language {
        Language::Chinese => "动画资源鸣谢",
        Language::English => "Animation credits",
    }
}

fn credit_thanks(language: Language) -> &'static str {
    match language {
        Language::Chinese => "感谢 pzUH 免费分享 Cat & Dog - Free Sprites 猫狗动画。",
        Language::English => "Thanks to pzUH for sharing the Cat & Dog - Free Sprites animations.",
    }
}

fn credit_source_prefix(language: Language) -> &'static str {
    match language {
        Language::Chinese => "下载来源：",
        Language::English => "Downloaded from ",
    }
}

fn credit_license_prefix(language: Language) -> &'static str {
    match language {
        Language::Chinese => "许可：",
        Language::English => "License: ",
    }
}

fn credit_license_suffix(language: Language) -> &'static str {
    match language {
        Language::Chinese => "（公共领域贡献）。",
        Language::English => " (public domain dedication).",
    }
}

fn credit_link_specs() -> [(&'static str, &'static str); 2] {
    [
        ("OpenGameArt", ASSET_SOURCE_URL),
        ("CC0 1.0 Universal", ASSET_LICENSE_URL),
    ]
}

fn about_state(hwnd: HWND) -> Option<AboutState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const AboutState;
    unsafe { raw.as_ref().cloned() }
}

// Keep window calls outside the attached state's mutable borrow.
fn with_about_state<R>(hwnd: HWND, update: impl FnOnce(&mut AboutState) -> R) -> Option<R> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut AboutState;
    unsafe { raw.as_mut().map(update) }
}

fn about_layout_is_current(hwnd: HWND, snapshot: &AboutState) -> bool {
    about_state(hwnd).is_some_and(|current| Rc::ptr_eq(&current.title_font, &snapshot.title_font))
}

fn release_about_state(hwnd: HWND) {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if raw != 0 {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        let _ = unsafe { Box::from_raw(raw as *mut AboutState) };
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

    #[test]
    fn default_ok_button_closes_on_click_and_enter_in_each_language() -> Result<()> {
        use windows::Win32::UI::{
            Input::KeyboardAndMouse::VK_RETURN,
            WindowsAndMessaging::{BM_CLICK, GWL_STYLE, GetWindowTextW},
        };
        for language in [Language::Chinese, Language::English] {
            for action in 0..3 {
                let (owner, about) = modal_test_windows()?;
                initialize_about_window(about.0, language, Theme::Light)?;
                let state = about_state(about.0).unwrap();
                let mut text = [0u16; 64];
                let count = unsafe { GetWindowTextW(state.ok_button, &mut text) } as usize;
                assert_eq!(
                    String::from_utf16_lossy(&text[..count]),
                    if language == Language::Chinese { "确定" } else { "OK" }
                );
                assert_eq!(
                    unsafe { GetWindowLongPtrW(state.ok_button, GWL_STYLE) } & 0xf,
                    BS_OWNERDRAW as isize
                );
                let default = unsafe { SendMessageW(about.0, DM_GETDEFID, None, None) }.0 as u32;
                assert_eq!(default & 0xffff, IDOK.0 as u32);
                assert_eq!(default >> 16, DC_HASDEFID);
                let (target, message, key) = match action {
                    0 => (state.ok_button, BM_CLICK, 0),
                    1 => (about.0, WM_KEYDOWN, VK_RETURN.0 as usize),
                    _ => (state.ok_button, WM_KEYDOWN, VK_RETURN.0 as usize),
                };
                drop(state);
                unsafe {
                    PostMessageW(Some(target), message, WPARAM(key), LPARAM(0))?;
                }
                run_about_modal(about.0, owner.0)?;
                assert!(about_state(about.0).is_none());
                assert!(!unsafe { IsWindow(Some(about.0)).as_bool() });
                assert!(unsafe { IsWindowEnabled(owner.0).as_bool() });
            }
        }
        Ok(())
    }

    #[test]
    fn owner_drawn_ok_button_uses_theme_colors_and_pressed_feedback() -> Result<()> {
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, GetDC, GetPixel, ReleaseDC,
        };
        let (_owner, about) = modal_test_windows()?;
        initialize_about_window(about.0, Language::English, Theme::Light)?;
        let state = about_state(about.0).unwrap();
        unsafe {
            let screen = GetDC(Some(about.0));
            let dc = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, 88, 32);
            assert!(!dc.is_invalid() && !bitmap.is_invalid());
            let previous = SelectObject(dc, bitmap.into());
            for theme in [Theme::Light, Theme::Dark] {
                apply_theme(about.0, theme)?;
                let mut colors = Vec::new();
                for pressed in [false, true] {
                    let item = DRAWITEMSTRUCT {
                        CtlID: IDOK.0 as u32,
                        hwndItem: state.ok_button,
                        hDC: dc,
                        itemState: if pressed { ODS_SELECTED } else { Default::default() },
                        rcItem: RECT {
                            left: 0,
                            top: 0,
                            right: 88,
                            bottom: 32,
                        },
                        ..Default::default()
                    };
                    assert_eq!(
                        SendMessageW(
                            about.0,
                            WM_DRAWITEM,
                            Some(WPARAM(IDOK.0 as usize)),
                            Some(LPARAM(&item as *const DRAWITEMSTRUCT as isize))
                        )
                        .0,
                        1
                    );
                    let fill = GetPixel(dc, 4, 4);
                    let brightness = fill.0 & 255;
                    assert_eq!(brightness < 128, theme == Theme::Dark);
                    assert_ne!(GetPixel(dc, 0, 0), fill, "button border must remain visible");
                    colors.push(fill);
                }
                assert_ne!(colors[0], colors[1], "pressing must produce visual feedback");
            }
            apply_theme(about.0, Theme::Light)?;
            SelectObject(dc, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            ReleaseDC(Some(about.0), screen);
        }
        Ok(())
    }

    #[test]
    fn modal_handles_owner_destruction_during_disable_dispatch_and_restore() -> Result<()> {
        use std::cell::Cell;
        use windows::Win32::UI::{
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::WM_ENABLE,
        };
        struct Callback {
            about: HWND,
            mode: u8,
            called: Cell<bool>,
            about_closed_before_restore: Cell<bool>,
        }
        unsafe extern "system" fn destroy_owner(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
            _: usize,
            data: usize,
        ) -> LRESULT {
            let data = unsafe { &*(data as *const Callback) };
            let destroy = match data.mode {
                0 => msg == WM_ENABLE && wp.0 == 0,
                1 => msg == WM_APP,
                _ => msg == WM_ENABLE && wp.0 != 0,
            };
            if destroy && !data.called.replace(true) {
                data.about_closed_before_restore
                    .set(!unsafe { IsWindow(Some(data.about)).as_bool() });
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return LRESULT(0);
            }
            unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
        }
        for mode in 0..3 {
            let (owner, about) = modal_test_windows()?;
            initialize_about_window(about.0, Language::English, Theme::Light)?;
            let title = Rc::downgrade(&about_state(about.0).unwrap().title_font);
            let data = Callback {
                about: about.0,
                mode,
                called: Cell::new(false),
                about_closed_before_restore: Cell::new(false),
            };
            assert!(unsafe {
                SetWindowSubclass(owner.0, Some(destroy_owner), 1, &data as *const _ as usize).as_bool()
            });
            unsafe {
                PostMessageW(Some(owner.0), WM_APP, WPARAM(0), LPARAM(0))?;
                PostMessageW(Some(about.0), WM_CLOSE, WPARAM(0), LPARAM(0))?;
            }
            run_about_modal(about.0, owner.0)?;
            assert!(data.called.get());
            assert!(!unsafe { IsWindow(Some(owner.0)).as_bool() });
            assert!(!unsafe { IsWindow(Some(about.0)).as_bool() });
            assert!(about_state(about.0).is_none());
            assert!(title.upgrade().is_none());
            if mode == 2 {
                assert!(data.about_closed_before_restore.get());
            }
        }
        Ok(())
    }

    #[test]
    fn font_callback_can_close_about_without_releasing_font_in_use() -> Result<()> {
        use std::cell::Cell;
        use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
        struct Callback {
            about: HWND,
            font: Cell<HFONT>,
            valid: Cell<bool>,
            detached: Cell<bool>,
        }
        unsafe extern "system" fn close_about(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
            _: usize,
            data: usize,
        ) -> LRESULT {
            let data = unsafe { &*(data as *const Callback) };
            if msg == WM_SETFONT {
                let font = HFONT(wp.0 as _);
                data.font.set(font);
                unsafe {
                    let _ = DestroyWindow(data.about);
                }
                data.detached.set(about_state(data.about).is_none());
                data.valid.set(font_is_valid(font));
                return LRESULT(0);
            }
            unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
        }
        let (_owner, about) = modal_test_windows()?;
        initialize_about_window(about.0, Language::English, Theme::Light)?;
        let label = about_state(about.0).unwrap().labels[0];
        let data = Callback {
            about: about.0,
            font: Cell::new(HFONT::default()),
            valid: Cell::new(false),
            detached: Cell::new(false),
        };
        assert!(unsafe { SetWindowSubclass(label, Some(close_about), 1, &data as *const _ as usize).as_bool() });
        update_about_dpi(about.0, 144)?;
        assert!(data.detached.get());
        assert!(data.valid.get());
        assert!(!font_is_valid(data.font.get()));
        Ok(())
    }

    #[test]
    fn layout_callback_can_close_about_and_release_snapshot_after_return() -> Result<()> {
        use std::cell::Cell;
        use windows::Win32::UI::{
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::WM_WINDOWPOSCHANGING,
        };
        struct Callback {
            about: HWND,
            font: HFONT,
            valid: Cell<bool>,
            called: Cell<bool>,
        }
        unsafe extern "system" fn close_about(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
            _: usize,
            data: usize,
        ) -> LRESULT {
            let data = unsafe { &*(data as *const Callback) };
            if msg == WM_WINDOWPOSCHANGING && !data.called.replace(true) {
                unsafe {
                    let _ = DestroyWindow(data.about);
                }
                data.valid.set(font_is_valid(data.font));
                return LRESULT(0);
            }
            unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
        }
        let (_owner, about) = modal_test_windows()?;
        initialize_about_window(about.0, Language::English, Theme::Light)?;
        let state = about_state(about.0).unwrap();
        let label = state.labels[0];
        let title = Rc::downgrade(&state.title_font);
        let data = Callback {
            about: about.0,
            font: state.title_font.0,
            valid: Cell::new(false),
            called: Cell::new(false),
        };
        drop(state);
        assert!(unsafe { SetWindowSubclass(label, Some(close_about), 1, &data as *const _ as usize).as_bool() });
        let _ = layout_about_window(about.0);
        assert!(data.called.get());
        assert!(data.valid.get());
        assert!(about_state(about.0).is_none());
        assert!(title.upgrade().is_none());
        assert!(!font_is_valid(data.font));
        Ok(())
    }

    #[test]
    fn nested_dpi_update_does_not_restore_outer_fonts_or_rectangle() -> Result<()> {
        use std::cell::Cell;
        use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
        struct Callback {
            about: HWND,
            called: Cell<bool>,
            rectangle: RECT,
        }
        unsafe extern "system" fn change_dpi(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
            _: usize,
            data: usize,
        ) -> LRESULT {
            let data = unsafe { &*(data as *const Callback) };
            if msg == WM_SETFONT && !data.called.replace(true) {
                let result = unsafe { DefSubclassProc(hwnd, msg, wp, lp) };
                unsafe {
                    SendMessageW(
                        data.about,
                        WM_DPICHANGED,
                        Some(WPARAM(192 | (192 << 16))),
                        Some(LPARAM(&data.rectangle as *const RECT as isize)),
                    );
                }
                return result;
            }
            unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
        }
        let (_owner, about) = modal_test_windows()?;
        initialize_about_window(about.0, Language::English, Theme::Light)?;
        let label = about_state(about.0).unwrap().labels[0];
        let data = Callback {
            about: about.0,
            called: Cell::new(false),
            rectangle: RECT {
                left: 30,
                top: 30,
                right: 1230,
                bottom: 990,
            },
        };
        assert!(unsafe { SetWindowSubclass(label, Some(change_dpi), 1, &data as *const _ as usize).as_bool() });
        let outer_rectangle = RECT {
            left: 10,
            top: 10,
            right: 910,
            bottom: 730,
        };
        unsafe {
            SendMessageW(
                about.0,
                WM_DPICHANGED,
                Some(WPARAM(144 | (144 << 16))),
                Some(LPARAM(&outer_rectangle as *const RECT as isize)),
            );
        }
        assert!(data.called.get());
        let state = about_state(about.0).unwrap();
        assert_eq!(state.dpi, 192);
        for label in &state.labels[1..] {
            assert_eq!(
                unsafe { SendMessageW(*label, WM_GETFONT, None, None) }.0,
                state.body_font.0 as isize
            );
        }
        assert_eq!(
            font_info(state.labels[0]).lfHeight,
            font_info(state.labels[1]).lfHeight * 18 / 11
        );
        let mut rectangle = RECT::default();
        unsafe {
            GetWindowRect(about.0, &mut rectangle)?;
        }
        assert_eq!(rectangle, data.rectangle);
        Ok(())
    }

    fn font_is_valid(font: HFONT) -> bool {
        let mut info = LOGFONTW::default();
        unsafe {
            GetObjectW(
                font.into(),
                std::mem::size_of::<LOGFONTW>() as i32,
                Some((&mut info as *mut LOGFONTW).cast()),
            ) != 0
        }
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
                ABOUT_WINDOW_WIDTH,
                ABOUT_WINDOW_HEIGHT,
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
        let other_link = HyperLinkText::create(window.0, GITHUB_URL, other_font, 96, |_| {})?;
        for dpi in [144usize, 192, 96, 144] {
            let suggested = RECT {
                left: 20,
                top: 20,
                right: 20 + ABOUT_WINDOW_WIDTH * dpi as i32 / 96,
                bottom: 20 + ABOUT_WINDOW_HEIGHT * dpi as i32 / 96,
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
            assert_eq!(
                unsafe { SendMessageW(state.ok_button, WM_GETFONT, None, None) }.0,
                shared_font.0 as isize
            );
            let mut title_rect = RECT::default();
            let mut version_rect = RECT::default();
            let mut button_rect = RECT::default();
            let mut client = RECT::default();
            unsafe {
                GetWindowRect(state.labels[0], &mut title_rect)?;
                GetWindowRect(state.labels[VERSION_LABEL], &mut version_rect)?;
                GetWindowRect(state.ok_button, &mut button_rect)?;
                GetClientRect(window.0, &mut client)?;
            }
            let dc = unsafe { windows::Win32::Graphics::Gdi::GetDC(Some(window.0)) };
            let mut title_metrics = windows::Win32::Graphics::Gdi::TEXTMETRICW::default();
            let mut version_metrics = windows::Win32::Graphics::Gdi::TEXTMETRICW::default();
            unsafe {
                let previous = SelectObject(dc, state.title_font.0.into());
                assert!(windows::Win32::Graphics::Gdi::GetTextMetricsW(dc, &mut title_metrics).as_bool());
                SelectObject(dc, state.body_font.into());
                assert!(windows::Win32::Graphics::Gdi::GetTextMetricsW(dc, &mut version_metrics).as_bool());
                SelectObject(dc, previous);
                windows::Win32::Graphics::Gdi::ReleaseDC(Some(window.0), dc);
            }
            assert_eq!(
                title_rect.top + title_metrics.tmAscent,
                version_rect.top + version_metrics.tmAscent,
                "title and version must share a text baseline for {language:?} at {dpi} DPI"
            );
            assert_eq!(
                version_rect.left,
                title_rect.right + crate::scale_dimension(12, dpi as u32)
            );
            assert_eq!(
                button_rect.right - button_rect.left,
                crate::scale_dimension(OK_BUTTON_WIDTH, dpi as u32)
            );
            assert_eq!(
                button_rect.bottom - button_rect.top,
                crate::scale_dimension(OK_BUTTON_HEIGHT, dpi as u32)
            );
            let mut text = [0u16; 64];
            let count =
                unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(state.labels[0], &mut text) } as usize;
            assert_eq!(String::from_utf16_lossy(&text[..count]), about_app_name(language));
            let count = unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(state.labels[VERSION_LABEL], &mut text)
            } as usize;
            assert_eq!(
                String::from_utf16_lossy(&text[..count]),
                format!("v{}", env!("CARGO_PKG_VERSION"))
            );
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
            assert_eq!(actual.bottom - actual.top, 56 * dpi as i32 / 96);
            let client_left = actual.left - crate::scale_dimension(CONTENT_LEFT, dpi as u32);
            let client_top = actual.top - crate::scale_dimension(LABEL_LAYOUT[1].0, dpi as u32);
            assert_eq!(
                button_rect.right,
                client_left + client.right - crate::scale_dimension(CONTENT_RIGHT, dpi as u32)
            );
            assert_eq!(
                button_rect.bottom,
                client_top + client.bottom - crate::scale_dimension(OK_BUTTON_BOTTOM, dpi as u32)
            );
            let mut github_rect = RECT::default();
            let mut commit_rect = RECT::default();
            let mut credit_rect = RECT::default();
            unsafe {
                GetWindowRect(state.labels[3], &mut github_rect)?;
                GetWindowRect(state.labels[2], &mut commit_rect)?;
                GetWindowRect(state.labels[4], &mut credit_rect)?;
            }
            assert!(github_rect.top < commit_rect.top);
            assert!(commit_rect.bottom < client_top + crate::scale_dimension(CREDIT_DIVIDER_TOP, dpi as u32));
            assert!(credit_rect.top > client_top + crate::scale_dimension(CREDIT_DIVIDER_TOP, dpi as u32));
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
            for (index, text) in [(5, credit_thanks(language)), (8, credit_license_suffix(language))] {
                let mut text_rect = RECT {
                    right: {
                        let mut label_rect = RECT::default();
                        unsafe {
                            GetWindowRect(state.labels[index], &mut label_rect)?;
                        }
                        label_rect.right - label_rect.left
                    },
                    ..Default::default()
                };
                let mut text = wide_text(text);
                unsafe {
                    DrawTextW(hdc, &mut text, &mut text_rect, DT_CALCRECT | DT_LEFT | DT_WORDBREAK);
                }
                assert!(
                    text_rect.bottom <= crate::scale_dimension(LABEL_LAYOUT[index].1, dpi as u32),
                    "credits must fit their STATIC controls"
                );
            }
            let source_rect = measure_text_rect(hdc, credit_source_prefix(language))?;
            let license_rect = measure_text_rect(hdc, credit_license_prefix(language))?;
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
            let mut prefix_rect = RECT::default();
            assert_eq!(state.credit_links.len(), 2);
            for (index, link) in state.credit_links.iter().enumerate() {
                let font = font_info(link.hwnd());
                assert_eq!(font.lfFaceName, body.lfFaceName);
                assert_eq!(font.lfHeight, body.lfHeight);
                assert_eq!(font.lfUnderline, 1);
                unsafe {
                    GetWindowRect(link.hwnd(), &mut actual)?;
                    GetWindowRect(state.labels[6 + index], &mut prefix_rect)?;
                }
                assert_eq!(link.text(), credit_link_specs()[index].0);
                assert_eq!(actual.left, prefix_rect.right, "source link must follow its prefix");
                let text_height = if index == 0 {
                    source_rect.bottom
                } else {
                    license_rect.bottom
                };
                assert!(
                    (actual.top + (actual.bottom - actual.top - text_height) / 2 - prefix_rect.top).abs() <= 1,
                    "source prefix and link text must align vertically"
                );
                if index == 1 {
                    let mut suffix_rect = RECT::default();
                    unsafe {
                        GetWindowRect(state.labels[8], &mut suffix_rect)?;
                    }
                    assert_eq!(suffix_rect.left, actual.right);
                    assert_eq!(suffix_rect.top, prefix_rect.top);
                }
                assert!(
                    actual.right <= client_left + client.right - crate::scale_dimension(CONTENT_RIGHT, dpi as u32),
                    "credit links must fit horizontally"
                );
                assert!(
                    actual.bottom <= client_top + client.bottom,
                    "credit links must fit vertically"
                );
            }
        }
        let state = about_state(window.0).unwrap();
        let body_font = state.body_font;
        let title_font = state.title_font.0;
        let link_font = unsafe { SendMessageW(state.github_link.hwnd(), WM_GETFONT, None, None) };
        drop(state);
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

    #[test]
    fn animation_credits_match_checked_in_asset_provenance() {
        let metadata: serde_json::Value =
            serde_json::from_str(include_str!("../assets/pets/cat-dog/manifest.json")).unwrap();
        assert_eq!(metadata["source"].as_str().unwrap(), ASSET_SOURCE_URL);
        for language in [Language::Chinese, Language::English] {
            let text = credit_thanks(language);
            assert!(text.contains(metadata["author"].as_str().unwrap()));
            assert!(credit_link_specs()[1].0.contains(metadata["license"].as_str().unwrap()));
            assert!(text.contains("Cat & Dog - Free Sprites"));
            assert_eq!(credit_link_specs()[0], ("OpenGameArt", ASSET_SOURCE_URL));
        }
    }
}
