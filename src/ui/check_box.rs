use std::sync::{Mutex, OnceLock};

use crate::ui::component::Component;
use crate::ui::theme::{current_text_color, is_dark_theme_active, paint_background};
use windows::Win32::{
    Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        ANTIALIASED_QUALITY, BeginPaint, CLIP_DEFAULT_PRECIS, CreateFontW, CreatePen, CreateSolidBrush,
        DEFAULT_CHARSET, DT_CALCRECT, DT_LEFT, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FF_SWISS,
        GetDeviceCaps, HDC, HFONT, InvalidateRect, LOGPIXELSY, LineTo, MoveToEx, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
        PS_SOLID, Rectangle, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, VARIABLE_PITCH,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::WM_MOUSELEAVE,
    UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
    UI::WindowsAndMessaging::{
        CREATESTRUCTW, CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, HCURSOR,
        IDC_ARROW, LoadCursorW, MoveWindow, RegisterClassW, SetCursor, SetWindowLongPtrW, WINDOW_EX_STYLE,
        WM_ERASEBKGND, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR, WNDCLASSW,
        WS_CHILD, WS_TABSTOP, WS_VISIBLE,
    },
};
use windows::core::{Error, Result, w};

const CHECK_BOX_CLASS_NAME: windows::core::PCWSTR = w!("YHB-StandAwhileCheckBox");
const CHECK_BOX_INDICATOR_SIZE: i32 = 16;
const CHECK_BOX_TEXT_GAP: i32 = 10;
const CHECK_BOX_PADDING_X: i32 = 2;
const CHECK_BOX_PADDING_Y: i32 = 4;

static CHECK_BOX_FONT: Mutex<Option<usize>> = Mutex::new(None);
static CHECK_BOX_CLASS_REGISTRATION: OnceLock<std::result::Result<(), i32>> = OnceLock::new();

pub type CheckBoxLayout = fn(&CheckBox, HWND, HDC) -> Result<()>;

struct CheckBoxCreateParams {
    text: String,
    checked: bool,
}

struct CheckBoxState {
    text: String,
    checked: bool,
    hovered: bool,
    tracking_mouse: bool,
}

#[derive(Clone)]
pub struct CheckBox {
    hwnd: HWND,
    text: String,
    layout: CheckBoxLayout,
}

impl CheckBox {
    pub fn create(parent: HWND, text: &str, checked: bool, layout: CheckBoxLayout) -> Result<Self> {
        let instance = current_module_instance()?;
        ensure_check_box_class_registered(instance)?;

        let params = Box::new(CheckBoxCreateParams {
            text: text.to_owned(),
            checked,
        });
        let raw_params = Box::into_raw(params);

        let hwnd_result = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CHECK_BOX_CLASS_NAME,
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                0,
                0,
                1,
                1,
                Some(parent),
                None,
                Some(instance),
                Some(raw_params.cast()),
            )
        };

        let hwnd = match hwnd_result {
            Ok(hwnd) if !hwnd.0.is_null() => hwnd,
            Ok(_) | Err(_) => {
                let _ = unsafe { Box::from_raw(raw_params) };
                return Err(Error::from_win32());
            }
        };

        Ok(Self {
            hwnd,
            text: text.to_owned(),
            layout,
        })
    }

    pub fn move_to(&self, rect: RECT) -> Result<()> {
        unsafe {
            MoveWindow(
                self.hwnd,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                true,
            )?;
        }
        Ok(())
    }

    pub fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    pub fn window_size(&self, dc: HDC) -> Result<(i32, i32)> {
        let rect = measure_text_rect(dc, &self.text)?;
        let text_width = rect.right - rect.left;
        let text_height = rect.bottom - rect.top;
        let width = CHECK_BOX_PADDING_X * 2 + CHECK_BOX_INDICATOR_SIZE + CHECK_BOX_TEXT_GAP + text_width;
        let height = CHECK_BOX_PADDING_Y * 2 + CHECK_BOX_INDICATOR_SIZE.max(text_height);
        Ok((width, height))
    }
}

impl Component for CheckBox {
    fn layout(&self, parent: HWND, dc: HDC) -> Result<()> {
        (self.layout)(self, parent, dc)
    }

    fn invalidate(&self) {
        CheckBox::invalidate(self);
    }
}

fn register_check_box_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(check_box_window_proc),
        hInstance: instance,
        lpszClassName: CHECK_BOX_CLASS_NAME,
        hCursor: load_arrow_cursor().unwrap_or(HCURSOR::default()),
        ..Default::default()
    };

    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(Error::from_win32());
    }

    Ok(())
}

fn current_module_instance() -> Result<HINSTANCE> {
    Ok(unsafe { GetModuleHandleW(None)? }.into())
}

fn ensure_check_box_class_registered(instance: HINSTANCE) -> Result<()> {
    match CHECK_BOX_CLASS_REGISTRATION
        .get_or_init(|| register_check_box_class(instance).map_err(|error| error.code().0))
    {
        Ok(()) => Ok(()),
        Err(code) => Err(Error::from(windows::core::HRESULT(*code))),
    }
}

pub fn release_check_box_font() {
    let mut cached_font = CHECK_BOX_FONT.lock().expect("check box font mutex poisoned");
    if let Some(raw_font) = cached_font.take() {
        unsafe {
            let _ = DeleteObject(HFONT(raw_font as _).into());
        }
    }
}

pub fn invalidate_check_box_font() {
    release_check_box_font();
}

unsafe extern "system" fn check_box_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let raw_params = create.lpCreateParams as *mut CheckBoxCreateParams;
            if raw_params.is_null() {
                return LRESULT(0);
            }

            let params = unsafe { Box::from_raw(raw_params) };
            let state = Box::new(CheckBoxState {
                text: params.text,
                checked: params.checked,
                hovered: false,
                tracking_mouse: false,
            });

            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
            }
            LRESULT(1)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
            let _ = draw_check_box(hwnd, hdc);
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            if let Some(state) = check_box_state_mut(hwnd) {
                if !state.tracking_mouse {
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut tracking) };
                    state.tracking_mouse = true;
                }

                if !state.hovered {
                    state.hovered = true;
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if let Some(state) = check_box_state_mut(hwnd) {
                let should_invalidate = state.hovered;
                state.hovered = false;
                state.tracking_mouse = false;
                if should_invalidate {
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(state) = check_box_state_mut(hwnd) {
                state.checked = !state.checked;
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if let Some(cursor) = load_arrow_cursor() {
                unsafe {
                    let _ = SetCursor(Some(cursor));
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_NCDESTROY => {
            let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if raw != 0 {
                let _ = unsafe { Box::from_raw(raw as *mut CheckBoxState) };
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn draw_check_box(hwnd: HWND, hdc: HDC) -> Result<()> {
    let state = check_box_state(hwnd).ok_or_else(Error::from_win32)?;
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client_rect)? };

    let indicator_top = client_rect.top + (client_rect.bottom - client_rect.top - CHECK_BOX_INDICATOR_SIZE) / 2;
    let indicator_rect = RECT {
        left: client_rect.left + CHECK_BOX_PADDING_X,
        top: indicator_top,
        right: client_rect.left + CHECK_BOX_PADDING_X + CHECK_BOX_INDICATOR_SIZE,
        bottom: indicator_top + CHECK_BOX_INDICATOR_SIZE,
    };

    draw_check_box_indicator(hdc, indicator_rect, state.checked, state.hovered)?;
    draw_check_box_text(hdc, &client_rect, &state.text)?;
    Ok(())
}

fn draw_check_box_indicator(hdc: HDC, rect: RECT, checked: bool, hovered: bool) -> Result<()> {
    let border = current_indicator_border_color(hovered);
    let fill = current_indicator_fill_color(hovered);
    let fill_brush = unsafe { CreateSolidBrush(fill) };
    if fill_brush.is_invalid() {
        return Err(Error::from_win32());
    }

    let border_pen = unsafe { CreatePen(PS_SOLID, 1, border) };
    if border_pen.is_invalid() {
        unsafe {
            let _ = DeleteObject(fill_brush.into());
        }
        return Err(Error::from_win32());
    }

    let old_pen = unsafe { SelectObject(hdc, border_pen.into()) };
    let old_brush = unsafe { SelectObject(hdc, fill_brush.into()) };

    unsafe {
        let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
        let _ = SelectObject(hdc, old_pen);
        let _ = SelectObject(hdc, old_brush);
        let _ = DeleteObject(border_pen.into());
        let _ = DeleteObject(fill_brush.into());
    }

    if checked {
        draw_check_mark(hdc, rect)?;
    }

    Ok(())
}

fn draw_check_mark(hdc: HDC, rect: RECT) -> Result<()> {
    let color = current_check_mark_color();
    let pen = unsafe { CreatePen(PS_SOLID, 2, color) };
    if pen.is_invalid() {
        return Err(Error::from_win32());
    }

    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    let start_x = rect.left + 3;
    let start_y = rect.top + 8;
    let mid_x = rect.left + 7;
    let mid_y = rect.bottom - 4;
    let end_x = rect.right - 3;
    let end_y = rect.top + 4;

    unsafe {
        let _ = MoveToEx(hdc, start_x, start_y, None);
        let _ = LineTo(hdc, mid_x, mid_y);
        let _ = LineTo(hdc, end_x, end_y);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());
    }

    Ok(())
}

fn draw_check_box_text(hdc: HDC, client_rect: &RECT, text: &str) -> Result<()> {
    let mut text_rect = *client_rect;
    text_rect.left += CHECK_BOX_PADDING_X + CHECK_BOX_INDICATOR_SIZE + CHECK_BOX_TEXT_GAP;
    let font = get_check_box_font(hdc)?;
    let old_font = unsafe { SelectObject(hdc, font.into()) };
    let mut wide_text = wide_text(text);

    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, current_text_color());
        let _ = DrawTextW(
            hdc,
            wide_text.as_mut_slice(),
            &mut text_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let _ = SelectObject(hdc, old_font);
    }

    Ok(())
}

fn measure_text_rect(hdc: HDC, text: &str) -> Result<RECT> {
    let font = get_check_box_font(hdc)?;
    let old_font = unsafe { SelectObject(hdc, font.into()) };
    let mut rect = RECT::default();
    let mut wide_text = wide_text(text);

    unsafe {
        let _ = DrawTextW(
            hdc,
            wide_text.as_mut_slice(),
            &mut rect,
            DT_LEFT | DT_SINGLELINE | DT_CALCRECT,
        );
        let _ = SelectObject(hdc, old_font);
    }

    Ok(rect)
}

fn get_check_box_font(hdc: HDC) -> Result<HFONT> {
    let mut cached_font = CHECK_BOX_FONT.lock().expect("check box font mutex poisoned");

    if let Some(raw_font) = *cached_font {
        return Ok(HFONT(raw_font as _));
    }

    let font = create_check_box_font(hdc);
    if font.is_invalid() {
        return Err(Error::from_win32());
    }

    *cached_font = Some(font.0 as usize);
    Ok(font)
}

fn create_check_box_font(hdc: HDC) -> HFONT {
    let dpi_y = unsafe { GetDeviceCaps(Some(hdc), LOGPIXELSY) };
    let font_height = -(11 * dpi_y / 72);

    unsafe {
        CreateFontW(
            font_height,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            (VARIABLE_PITCH.0 | FF_SWISS.0) as u32,
            w!("Segoe UI"),
        )
    }
}

fn current_indicator_border_color(hovered: bool) -> COLORREF {
    match (is_dark_theme_active(), hovered) {
        (true, true) => rgb(196, 196, 196),
        (true, false) => rgb(144, 144, 144),
        (false, true) => rgb(88, 88, 88),
        (false, false) => rgb(120, 120, 120),
    }
}

fn current_indicator_fill_color(hovered: bool) -> COLORREF {
    match (is_dark_theme_active(), hovered) {
        (true, true) => rgb(54, 54, 54),
        (true, false) => rgb(42, 42, 42),
        (false, true) => rgb(230, 230, 230),
        (false, false) => rgb(244, 244, 244),
    }
}

fn current_check_mark_color() -> COLORREF {
    if is_dark_theme_active() {
        rgb(238, 189, 82)
    } else {
        rgb(208, 141, 30)
    }
}

fn check_box_state(hwnd: HWND) -> Option<&'static CheckBoxState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const CheckBoxState;
    unsafe { raw.as_ref() }
}

fn check_box_state_mut(hwnd: HWND) -> Option<&'static mut CheckBoxState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut CheckBoxState;
    unsafe { raw.as_mut() }
}

fn load_arrow_cursor() -> Option<HCURSOR> {
    unsafe { LoadCursorW(None, IDC_ARROW) }.ok()
}

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

fn wide_text(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
