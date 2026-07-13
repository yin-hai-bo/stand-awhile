use std::sync::{Mutex, OnceLock};

use crate::ui::component::Component;
use crate::ui::theme::{is_dark_theme_active, paint_background};
use windows::Win32::{
    Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DT_CALCRECT, DT_LEFT,
        DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, EndPaint, FF_SWISS, GetDeviceCaps, HDC, HFONT,
        InvalidateRect, LOGPIXELSY, OUT_DEFAULT_PRECIS, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor,
        TRANSPARENT, VARIABLE_PITCH,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::WM_MOUSELEAVE,
    UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
    UI::WindowsAndMessaging::{
        CREATESTRUCTW, CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, HCURSOR,
        IDC_ARROW, IDC_HAND, LoadCursorW, MoveWindow, RegisterClassW, SetCursor, SetWindowLongPtrW, WINDOW_EX_STYLE,
        WM_ERASEBKGND, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR, WNDCLASSW,
        WS_CHILD, WS_VISIBLE,
    },
};
use windows::core::{Error, Result, w};

const HYPER_LINK_TEXT_CLASS_NAME: windows::core::PCWSTR = w!("YHB-StandAwhileHyperLinkText");
const LINK_PADDING_X: i32 = 10;
const LINK_PADDING_TOP: i32 = 7;
const LINK_PADDING_BOTTOM: i32 = 10;
const LINK_MEASURE_EXTRA_WIDTH: i32 = 4;
const LINK_MEASURE_EXTRA_HEIGHT: i32 = 4;

static LINK_FONT: Mutex<Option<usize>> = Mutex::new(None);
static HYPER_LINK_TEXT_CLASS_REGISTRATION: OnceLock<std::result::Result<(), i32>> = OnceLock::new();

type HyperLinkCallback = Box<dyn FnMut(HWND)>;
pub type HyperLinkTextLayout = fn(&HyperLinkText, HWND, HDC) -> Result<()>;

struct HyperLinkTextCreateParams {
    text: String,
    on_click: HyperLinkCallback,
}

struct HyperLinkTextState {
    text: String,
    hovered: bool,
    tracking_mouse: bool,
    on_click: HyperLinkCallback,
}

#[derive(Clone)]
pub struct HyperLinkText {
    hwnd: HWND,
    text: String,
    layout: HyperLinkTextLayout,
}

#[allow(dead_code)]
impl HyperLinkText {
    pub fn create<F>(parent: HWND, text: &str, on_click: F, layout: HyperLinkTextLayout) -> Result<Self>
    where
        F: FnMut(HWND) + 'static,
    {
        let instance = current_module_instance()?;
        ensure_hyper_link_text_class_registered(instance)?;

        let params = Box::new(HyperLinkTextCreateParams {
            text: text.to_owned(),
            on_click: Box::new(on_click),
        });
        let raw_params = Box::into_raw(params);

        let hwnd_result = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                HYPER_LINK_TEXT_CLASS_NAME,
                w!(""),
                WS_CHILD | WS_VISIBLE,
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

    pub fn from_hwnd(hwnd: HWND) -> Result<Self> {
        let state = hyper_link_state(hwnd).ok_or_else(Error::from_win32)?;
        Ok(Self {
            hwnd,
            text: state.text.clone(),
            layout: default_hyper_link_text_layout,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: &str) -> Result<()> {
        let state = hyper_link_state_mut(self.hwnd).ok_or_else(Error::from_win32)?;
        state.text.clear();
        state.text.push_str(text);
        self.text.clear();
        self.text.push_str(text);
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        Ok(())
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

    pub fn measure_text_rect(&self, hdc: HDC) -> Result<RECT> {
        measure_text_rect(hdc, &self.text)
    }

    pub fn window_size(&self, hdc: HDC) -> Result<(i32, i32)> {
        let rect = self.measure_text_rect(hdc)?;
        Ok((
            rect.right - rect.left + LINK_PADDING_X * 2,
            rect.bottom - rect.top + LINK_PADDING_TOP + LINK_PADDING_BOTTOM,
        ))
    }
}

impl Component for HyperLinkText {
    fn layout(&self, parent: HWND, dc: HDC) -> Result<()> {
        (self.layout)(self, parent, dc)
    }

    fn invalidate(&self) {
        HyperLinkText::invalidate(self);
    }
}

fn register_hyper_link_text_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(hyper_link_text_window_proc),
        hInstance: instance,
        lpszClassName: HYPER_LINK_TEXT_CLASS_NAME,
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

fn default_hyper_link_text_layout(_: &HyperLinkText, _: HWND, _: HDC) -> Result<()> {
    Ok(())
}

fn ensure_hyper_link_text_class_registered(instance: HINSTANCE) -> Result<()> {
    match HYPER_LINK_TEXT_CLASS_REGISTRATION
        .get_or_init(|| register_hyper_link_text_class(instance).map_err(|error| error.code().0))
    {
        Ok(()) => Ok(()),
        Err(code) => Err(Error::from(windows::core::HRESULT(*code))),
    }
}

pub fn release_hyper_link_text_font() {
    let mut cached_font = LINK_FONT.lock().expect("hyper link text font mutex poisoned");
    if let Some(raw_font) = cached_font.take() {
        unsafe {
            let _ = DeleteObject(HFONT(raw_font as _).into());
        }
    }
}

pub fn invalidate_hyper_link_text_font() {
    release_hyper_link_text_font();
}

unsafe extern "system" fn hyper_link_text_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let raw_params = create.lpCreateParams as *mut HyperLinkTextCreateParams;
            if raw_params.is_null() {
                return LRESULT(0);
            }

            let params = unsafe { Box::from_raw(raw_params) };
            let state = Box::new(HyperLinkTextState {
                text: params.text,
                hovered: false,
                tracking_mouse: false,
                on_click: params.on_click,
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
            let _ = draw_hyper_link_text(hwnd, hdc, hyper_link_hovered(hwnd));
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            if let Some(state) = hyper_link_state_mut(hwnd) {
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
            if let Some(state) = hyper_link_state_mut(hwnd) {
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
            if let Some(state) = hyper_link_state_mut(hwnd) {
                (state.on_click)(hwnd);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if let Some(cursor) = load_hand_cursor() {
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
                let _ = unsafe { Box::from_raw(raw as *mut HyperLinkTextState) };
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn draw_hyper_link_text(hwnd: HWND, hdc: HDC, hovered: bool) -> Result<()> {
    let state = hyper_link_state(hwnd).ok_or_else(Error::from_win32)?;
    let mut text = wide_text(&state.text);
    let mut text_rect = client_rect(hwnd)?;
    let font = get_link_font(hdc)?;
    let old_font = unsafe { SelectObject(hdc, font.into()) };

    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, current_link_color(hovered));
        let _ = DrawTextW(
            hdc,
            text.as_mut_slice(),
            &mut text_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let _ = SelectObject(hdc, old_font);
    }

    Ok(())
}

fn client_rect(hwnd: HWND) -> Result<RECT> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect)? };
    Ok(rect)
}

fn measure_text_rect(hdc: HDC, text: &str) -> Result<RECT> {
    let mut text = wide_text(text);
    let font = get_link_font(hdc)?;
    let old_font = unsafe { SelectObject(hdc, font.into()) };
    let mut measured = RECT::default();

    unsafe {
        let _ = DrawTextW(
            hdc,
            text.as_mut_slice(),
            &mut measured,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_CALCRECT,
        );
        let _ = SelectObject(hdc, old_font);
    }

    measured.right += LINK_MEASURE_EXTRA_WIDTH;
    measured.bottom += LINK_MEASURE_EXTRA_HEIGHT;

    Ok(measured)
}

fn get_link_font(hdc: HDC) -> Result<HFONT> {
    let mut cached_font = LINK_FONT.lock().expect("hyper link text font mutex poisoned");

    if let Some(raw_font) = *cached_font {
        return Ok(HFONT(raw_font as _));
    }

    let font = create_link_font(hdc);
    if font.is_invalid() {
        return Err(Error::from_win32());
    }

    *cached_font = Some(font.0 as usize);
    Ok(font)
}

fn create_link_font(hdc: HDC) -> HFONT {
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
            1,
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

fn hyper_link_state(hwnd: HWND) -> Option<&'static HyperLinkTextState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const HyperLinkTextState;
    unsafe { raw.as_ref() }
}

fn hyper_link_state_mut(hwnd: HWND) -> Option<&'static mut HyperLinkTextState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut HyperLinkTextState;
    unsafe { raw.as_mut() }
}

fn hyper_link_hovered(hwnd: HWND) -> bool {
    hyper_link_state(hwnd).map(|state| state.hovered).unwrap_or(false)
}

fn current_link_color(hovered: bool) -> COLORREF {
    match (is_dark_theme_active(), hovered) {
        (true, true) => rgb(150, 200, 255),
        (true, false) => rgb(120, 180, 255),
        (false, true) => rgb(0, 84, 168),
        (false, false) => rgb(0, 102, 204),
    }
}

fn load_hand_cursor() -> Option<HCURSOR> {
    unsafe { LoadCursorW(None, IDC_HAND) }.ok()
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
