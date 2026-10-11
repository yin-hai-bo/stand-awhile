use std::{cell::RefCell, rc::Rc, sync::OnceLock};

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

struct HyperLinkTextCreateParams {
    text: String,
    on_click: Option<HyperLinkCallback>,
}

#[derive(Clone)]
struct HyperLinkTextState {
    font: Rc<LinkFont>,
    dpi: u32,
    text: String,
    hovered: bool,
    tracking_mouse: bool,
    on_click: Rc<RefCell<HyperLinkCallback>>,
}

struct LinkFont(HFONT);

impl Drop for LinkFont {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

#[derive(Clone)]
pub struct HyperLinkText {
    hwnd: HWND,
    text: String,
}

impl HyperLinkText {
    pub fn create<F>(parent: HWND, text: &str, base_font: HFONT, dpi: u32, on_click: F) -> Result<Self>
    where
        F: FnMut(HWND) + 'static,
    {
        let instance = current_module_instance()?;
        ensure_hyper_link_text_class_registered(instance)?;

        let mut params = HyperLinkTextCreateParams {
            text: text.to_owned(),
            on_click: Some(Box::new(on_click)),
        };

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
                Some((&mut params as *mut HyperLinkTextCreateParams).cast()),
            )
        };

        let hwnd = match hwnd_result {
            Ok(hwnd) if !hwnd.0.is_null() => hwnd,
            Err(error) => return Err(error),
            Ok(_) => return Err(Error::from_win32()),
        };

        let link = Self {
            hwnd,
            text: text.to_owned(),
        };
        if let Err(error) = link.set_font(base_font, dpi) {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return Err(error);
        }
        Ok(link)
    }

    #[cfg(test)]
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.text
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
        measure_text_rect(hdc, &self.text, state.font.0, state.dpi)
    }

    pub fn set_font(&self, base_font: HFONT, dpi: u32) -> Result<()> {
        let font = Rc::new(LinkFont(create_link_font(base_font)?));
        let previous = with_hyper_link_state(self.hwnd, |state| {
            state.dpi = dpi;
            std::mem::replace(&mut state.font, font)
        })
        .ok_or_else(Error::from_win32)?;
        drop(previous);
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
            let Some(params) = (unsafe { raw_params.as_mut() }) else {
                return LRESULT(0);
            };
            let Some(on_click) = params.on_click.take() else {
                return LRESULT(0);
            };
            let state = Box::new(HyperLinkTextState {
                font: Rc::new(LinkFont(HFONT::default())),
                dpi: unsafe { GetDpiForWindow(create.hwndParent) }.max(96),
                text: std::mem::take(&mut params.text),
                hovered: false,
                tracking_mouse: false,
                on_click: Rc::new(RefCell::new(on_click)),
            });

            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
            }
            LRESULT(1)
        }
        WM_PAINT => {
            let state = hyper_link_state(hwnd);
            let mut paint = PAINTSTRUCT::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let _ = paint_background(&paint.rcPaint, hdc);
            if let Some(state) = state {
                let _ = draw_hyper_link_text(hwnd, hdc, &state);
            }
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_GETFONT => LRESULT(hyper_link_state(hwnd).map(|state| state.font.0.0 as isize).unwrap_or(0)),
        WM_MOUSEMOVE => {
            let changes = with_hyper_link_state(hwnd, |state| {
                let track = !state.tracking_mouse;
                let invalidate = !state.hovered;
                state.tracking_mouse = true;
                state.hovered = true;
                (track, invalidate)
            });
            if let Some((track, invalidate)) = changes {
                if track {
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut tracking) };
                }

                if invalidate {
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            let invalidate = with_hyper_link_state(hwnd, |state| {
                let should_invalidate = state.hovered;
                state.hovered = false;
                state.tracking_mouse = false;
                should_invalidate
            })
            .unwrap_or(false);
            if invalidate {
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let callback = hyper_link_state(hwnd).map(|state| state.on_click);
            if let Some(callback) = callback {
                // A nested click cannot reborrow the active FnMut callback.
                if let Ok(mut callback) = callback.try_borrow_mut() {
                    callback(hwnd);
                }
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
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                let _ = unsafe { Box::from_raw(raw as *mut HyperLinkTextState) };
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn draw_hyper_link_text(hwnd: HWND, hdc: HDC, state: &HyperLinkTextState) -> Result<()> {
    let mut text = wide_text(&state.text);
    let mut text_rect = client_rect(hwnd)?;
    let old_font = unsafe { SelectObject(hdc, state.font.0.into()) };

    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, current_link_color(state.hovered));
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
fn hyper_link_state(hwnd: HWND) -> Option<HyperLinkTextState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const HyperLinkTextState;
    unsafe { raw.as_ref().cloned() }
}

// Window calls and user callbacks run outside this local mutation.
fn with_hyper_link_state<R>(hwnd: HWND, update: impl FnOnce(&mut HyperLinkTextState) -> R) -> Option<R> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut HyperLinkTextState;
    unsafe { raw.as_mut().map(update) }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

    struct TestParent(HWND);
    impl Drop for TestParent {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    struct DropProbe(Rc<Cell<u32>>);
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    fn test_parent() -> Result<TestParent> {
        let instance = current_module_instance()?;
        unsafe {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                0,
                0,
                400,
                200,
                None,
                None,
                Some(instance),
                None,
            )
            .map(TestParent)
        }
    }

    fn base_font() -> HFONT {
        crate::ui::font::common_gui_font(96, false).unwrap()
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

    #[test]
    fn nested_click_skips_busy_callback_and_owner_destruction_keeps_callback_alive() -> Result<()> {
        let parent = test_parent()?;
        let owner = parent.0;
        let calls = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        let callback_calls = calls.clone();
        let callback_drops = drops.clone();
        let probe = DropProbe(drops.clone());
        let link = HyperLinkText::create(owner, "Example", base_font(), 96, move |hwnd| {
            let _keep_alive = &probe;
            callback_calls.set(callback_calls.get() + 1);
            unsafe {
                SendMessageW(hwnd, WM_LBUTTONUP, None, None);
                SendMessageW(hwnd, WM_MOUSEMOVE, None, None);
                SendMessageW(hwnd, WM_MOUSELEAVE, None, None);
                if callback_calls.get() == 2 {
                    DestroyWindow(owner).unwrap();
                    assert!(hyper_link_state(hwnd).is_none());
                    assert_eq!(callback_drops.get(), 0, "the active callback must outlive its window");
                }
            }
        })?;
        let font = Rc::downgrade(&hyper_link_state(link.hwnd).unwrap().font);
        unsafe {
            SendMessageW(link.hwnd, WM_LBUTTONUP, None, None);
        }
        assert_eq!(calls.get(), 1);
        assert!(!hyper_link_state(link.hwnd).unwrap().hovered);
        assert_eq!(drops.get(), 0);
        unsafe {
            SendMessageW(link.hwnd, WM_LBUTTONUP, None, None);
        }
        assert_eq!(calls.get(), 2);
        assert_eq!(drops.get(), 1);
        assert!(font.upgrade().is_none());
        assert!(hyper_link_state(link.hwnd).is_none());
        Ok(())
    }

    #[test]
    fn font_snapshots_survive_replacement_and_parent_destruction() -> Result<()> {
        let parent = test_parent()?;
        let link = HyperLinkText::create(parent.0, "Example", base_font(), 96, |_| {})?;
        let snapshot = hyper_link_state(link.hwnd).unwrap();
        let old_font = snapshot.font.0;
        let old_owner = Rc::downgrade(&snapshot.font);
        let large_base = crate::ui::font::common_gui_font(192, false).unwrap();
        link.set_font(large_base, 192)?;
        assert!(font_is_valid(old_font));
        drop(snapshot);
        assert!(old_owner.upgrade().is_none());
        assert!(!font_is_valid(old_font));
        let snapshot = hyper_link_state(link.hwnd).unwrap();
        let current_font = snapshot.font.0;
        assert!(link.set_font(HFONT::default(), 96).is_err());
        assert!(Rc::ptr_eq(&hyper_link_state(link.hwnd).unwrap().font, &snapshot.font));
        drop(parent);
        assert!(hyper_link_state(link.hwnd).is_none());
        assert!(font_is_valid(current_font));
        let dc = unsafe { windows::Win32::Graphics::Gdi::CreateCompatibleDC(None) };
        let measured = measure_text_rect(dc, &snapshot.text, snapshot.font.0, snapshot.dpi)?;
        assert!(measured.right > 0 && measured.bottom > 0);
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteDC(dc);
        }
        drop(snapshot);
        assert!(!font_is_valid(current_font));
        assert!(font_is_valid(large_base), "shared GUI font remains owned by its cache");
        Ok(())
    }

    #[test]
    fn failed_creation_drops_callback_once_before_and_after_state_attachment() -> Result<()> {
        let parent = test_parent()?;
        for (owner, font) in [(HWND(1usize as _), base_font()), (parent.0, HFONT::default())] {
            let drops = Rc::new(Cell::new(0));
            let probe = DropProbe(drops.clone());
            assert!(
                HyperLinkText::create(owner, "Example", font, 96, move |_| {
                    let _ = &probe;
                })
                .is_err()
            );
            assert_eq!(drops.get(), 1);
        }
        Ok(())
    }

    #[test]
    fn hover_font_and_paint_keep_link_layout_at_supported_dpis() -> Result<()> {
        let parent = test_parent()?;
        let link = HyperLinkText::create(parent.0, "Example link", base_font(), 96, |_| {})?;
        for dpi in [96, 120, 144, 192, 96] {
            let font = crate::ui::font::common_gui_font(dpi, false).unwrap();
            link.set_font(font, dpi)?;
            let state = hyper_link_state(link.hwnd).unwrap();
            let mut info = LOGFONTW::default();
            assert_ne!(
                unsafe {
                    GetObjectW(
                        state.font.0.into(),
                        std::mem::size_of::<LOGFONTW>() as i32,
                        Some((&mut info as *mut LOGFONTW).cast()),
                    )
                },
                0
            );
            assert_eq!(info.lfUnderline, 1);
            let dc = unsafe { windows::Win32::Graphics::Gdi::GetDC(Some(parent.0)) };
            let (width, height) = link.window_size(dc)?;
            link.move_to(RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            })?;
            unsafe {
                windows::Win32::Graphics::Gdi::ReleaseDC(Some(parent.0), dc);
                SendMessageW(link.hwnd, WM_MOUSEMOVE, None, None);
                assert!(hyper_link_state(link.hwnd).unwrap().hovered);
                SendMessageW(link.hwnd, WM_PAINT, None, None);
                SendMessageW(link.hwnd, WM_MOUSELEAVE, None, None);
            }
            let state = hyper_link_state(link.hwnd).unwrap();
            assert!(!state.hovered && !state.tracking_mouse);
            assert_eq!(state.dpi, dpi);
        }
        Ok(())
    }
}
