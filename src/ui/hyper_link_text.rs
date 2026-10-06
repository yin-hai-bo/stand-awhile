use std::sync::OnceLock;

use crate::ui::component::Component;
use crate::ui::theme::{is_dark_theme_active, paint_background};
use windows::Win32::{
    Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateFontIndirectW, DT_CALCRECT, DT_LEFT, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW,
        EndPaint, GetObjectW, HDC, HFONT, InvalidateRect, LOGFONTW, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor,
        TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::WM_MOUSELEAVE,
    UI::HiDpi::GetDpiForWindow,
    UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent},
    UI::WindowsAndMessaging::{
        CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW,
        HCURSOR, IDC_ARROW, IDC_HAND, LoadCursorW, MoveWindow, RegisterClassW, SetCursor, SetWindowLongPtrW,
        WINDOW_EX_STYLE, WM_ERASEBKGND, WM_GETFONT, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
        WM_SETCURSOR, WNDCLASSW, WS_CHILD, WS_VISIBLE,
    },
};
use windows::core::{Error, Result, w};

const HYPER_LINK_TEXT_CLASS_NAME: windows::core::PCWSTR = w!("YHB-StandAwhileHyperLinkText");
const LINK_PADDING_X: i32 = 10;
const LINK_PADDING_TOP: i32 = 7;
const LINK_PADDING_BOTTOM: i32 = 10;
const LINK_MEASURE_EXTRA_WIDTH: i32 = 4;
const LINK_MEASURE_EXTRA_HEIGHT: i32 = 4;

static HYPER_LINK_TEXT_CLASS_REGISTRATION: OnceLock<std::result::Result<(), i32>> = OnceLock::new();

type HyperLinkCallback = Box<dyn FnMut(HWND)>;
pub type HyperLinkTextLayout = fn(&HyperLinkText, HWND, HDC) -> Result<()>;

struct HyperLinkTextCreateParams {
    text: String,
    on_click: HyperLinkCallback,
}

struct HyperLinkTextState {
    font: HFONT,
    dpi: u32,
    text: String,
    hovered: bool,
    tracking_mouse: bool,
    on_click: HyperLinkCallback,
}

impl Drop for HyperLinkTextState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.font.into());
        }
    }
}

#[derive(Clone)]
pub struct HyperLinkText {
    hwnd: HWND,
    text: String,
    layout: HyperLinkTextLayout,
}

#[allow(dead_code)]
impl HyperLinkText {
    pub fn create<F>(
        parent: HWND,
        text: &str,
        base_font: HFONT,
        dpi: u32,
        on_click: F,
        layout: HyperLinkTextLayout,
    ) -> Result<Self>
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

        let link = Self {
            hwnd,
            text: text.to_owned(),
            layout,
        };
        if let Err(error) = link.set_font(base_font, dpi) {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return Err(error);
        }
        Ok(link)
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
        let state = hyper_link_state(self.hwnd).ok_or_else(Error::from_win32)?;
        measure_text_rect(hdc, &self.text, state.font, state.dpi)
    }

    pub fn set_font(&self, base_font: HFONT, dpi: u32) -> Result<()> {
        let state = hyper_link_state_mut(self.hwnd).ok_or_else(Error::from_win32)?;
        let font = create_link_font(base_font)?;
        let old_font = state.font;
        state.font = font;
        state.dpi = dpi;
        unsafe {
            let _ = DeleteObject(old_font.into());
        }
        self.invalidate();
        Ok(())
    }

    pub fn window_size(&self, hdc: HDC) -> Result<(i32, i32)> {
        let dpi = hyper_link_state(self.hwnd).ok_or_else(Error::from_win32)?.dpi;
        let rect = self.measure_text_rect(hdc)?;
        Ok((
            rect.right - rect.left + crate::scale_dimension(LINK_PADDING_X * 2, dpi),
            rect.bottom - rect.top + crate::scale_dimension(LINK_PADDING_TOP + LINK_PADDING_BOTTOM, dpi),
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
                font: HFONT::default(),
                dpi: unsafe { GetDpiForWindow(create.hwndParent) }.max(96),
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
        WM_GETFONT => LRESULT(hyper_link_state(hwnd).map(|state| state.font.0 as isize).unwrap_or(0)),
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
    let old_font = unsafe { SelectObject(hdc, state.font.into()) };

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

fn measure_text_rect(hdc: HDC, text: &str, font: HFONT, dpi: u32) -> Result<RECT> {
    let mut text = wide_text(text);
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

    measured.right += crate::scale_dimension(LINK_MEASURE_EXTRA_WIDTH, dpi);
    measured.bottom += crate::scale_dimension(LINK_MEASURE_EXTRA_HEIGHT, dpi);

    Ok(measured)
}

fn create_link_font(base_font: HFONT) -> Result<HFONT> {
    let mut font = LOGFONTW::default();
    if unsafe {
        GetObjectW(
            base_font.into(),
            std::mem::size_of::<LOGFONTW>() as i32,
            Some((&mut font as *mut LOGFONTW).cast()),
        )
    } == 0
    {
        return Err(Error::from_win32());
    }
    font.lfUnderline = 1;
    let handle = unsafe { CreateFontIndirectW(&font) };
    if handle.is_invalid() {
        Err(Error::from_win32())
    } else {
        Ok(handle)
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
