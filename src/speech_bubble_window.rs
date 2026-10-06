use std::sync::OnceLock;

use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreatePen, CreateSolidBrush,
            DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DeleteObject, DrawTextW, Ellipse,
            EndPaint, FF_DONTCARE, FW_NORMAL, GetDC, GetMonitorInfoW, GetTextExtentPoint32W, HGDIOBJ, InvalidateRect,
            MONITOR_DEFAULTTONEAREST, MonitorFromWindow, OUT_DEFAULT_PRECIS, PS_SOLID, ReleaseDC, SelectObject,
            SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT,
        },
        UI::HiDpi::GetDpiForWindow,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect, GetWindowRect, HWND_TOPMOST,
            RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SetWindowLongPtrW,
            SetWindowPos, ShowWindow, WM_DPICHANGED, WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WNDCLASSEXW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::i18n::{Language, pet_reminder_text};
use crate::speech_bubble::SpeechBubblePlayer;

const BUBBLE_WINDOW_CLASS: PCWSTR = w!("YHB-StandAwhileSpeechBubble");
const BUBBLE_MIN_WIDTH: i32 = 180;
const BUBBLE_MIN_HEIGHT: i32 = 76;
const BUBBLE_HORIZONTAL_PADDING: i32 = 80;
const BUBBLE_VERTICAL_PADDING: i32 = 56;
const BUBBLE_TEXT_GAP: i32 = 6;
const BUBBLE_GAP: i32 = 8;
const BUBBLE_FONT_HEIGHT: i32 = -30;
const BUBBLE_HINT_FONT_HEIGHT: i32 = 20;
const BUBBLE_BORDER_WIDTH: i32 = 4;
const BUBBLE_BACKGROUND: COLORREF = COLORREF(0x00c4efff);
const BUBBLE_BORDER: COLORREF = COLORREF(0x006e6948);
const BUBBLE_TEXT: COLORREF = COLORREF(0x00504a35);

pub struct SpeechBubbleWindow {
    hwnd: HWND,
}

impl SpeechBubbleWindow {
    pub fn create(instance: HINSTANCE, owner: HWND) -> Result<Self> {
        register_class(instance)?;
        let mut owner_rect = RECT::default();
        unsafe { GetWindowRect(owner, &mut owner_rect)? };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                BUBBLE_WINDOW_CLASS,
                w!("Stand Awhile Speech Bubble"),
                WS_POPUP,
                owner_rect.left,
                owner_rect.top,
                BUBBLE_MIN_WIDTH,
                BUBBLE_MIN_HEIGHT,
                Some(owner),
                None,
                Some(instance),
                None,
            )
        }?;
        let state = match BubbleState::new(unsafe { GetDpiForWindow(hwnd) }.max(96), owner) {
            Ok(state) => state,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(error);
            }
        };
        attach_state(hwnd, state);
        Ok(Self { hwnd })
    }

    pub fn update(&self, pet_position: (i32, i32), pet_size: (i32, i32), text: Option<&str>) -> Result<()> {
        let state = state_mut(self.hwnd).ok_or_else(Error::from_win32)?;
        let next_text = text.unwrap_or_default().to_owned();
        let text_changed = state.text != next_text;
        state.text = next_text;
        if state.text.is_empty() {
            if state.visible {
                state.visible = false;
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
            return Ok(());
        }

        self.set_position(pet_position, pet_size)?;

        if text_changed {
            unsafe {
                let _ = InvalidateRect(Some(self.hwnd), None, false);
            }
        }
        if !state.visible {
            state.visible = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
        }
        Ok(())
    }

    pub fn set_position(&self, pet_position: (i32, i32), pet_size: (i32, i32)) -> Result<()> {
        let state = state_mut(self.hwnd).ok_or_else(Error::from_win32)?;
        state.pet_position = pet_position;
        state.pet_size = pet_size;
        layout_bubble(self.hwnd, state)
    }

    pub fn hide(&self) {
        let Some(state) = state_mut(self.hwnd) else {
            return;
        };
        if !state.visible {
            return;
        }
        state.visible = false;
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }
}

fn bubble_position(
    pet_position: (i32, i32),
    pet_size: (i32, i32),
    bubble_size: (i32, i32),
    work_area: RECT,
    dpi: u32,
) -> (i32, i32) {
    let (width, height) = bubble_size;
    let centered_x = pet_position.0 + (pet_size.0 - width) / 2;
    let x = centered_x.clamp(work_area.left, (work_area.right - width).max(work_area.left));
    let above_y = pet_position.1 - height - scale(BUBBLE_GAP, dpi);
    let below_y = pet_position.1 + pet_size.1 + scale(BUBBLE_GAP, dpi);
    let y = if above_y >= work_area.top {
        above_y
    } else if below_y + height <= work_area.bottom {
        below_y
    } else {
        above_y.clamp(work_area.top, (work_area.bottom - height).max(work_area.top))
    };
    (x, y)
}

fn work_area_for_window(hwnd: HWND) -> Result<RECT> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = windows::Win32::Graphics::Gdi::MONITORINFO {
        cbSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(Error::from_win32());
    }
    Ok(info.rcWork)
}

fn set_ellipse_region(hwnd: HWND, width: i32, height: i32) -> Result<()> {
    let region = unsafe { windows::Win32::Graphics::Gdi::CreateEllipticRgn(0, 0, width, height) };
    if region.is_invalid() {
        return Err(Error::from_win32());
    }
    unsafe {
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(HGDIOBJ(region.0));
            return Err(Error::from_win32());
        }
    }
    Ok(())
}

fn measure_line(
    hdc: windows::Win32::Graphics::Gdi::HDC,
    state: &BubbleState,
    index: usize,
    line: &str,
) -> Result<SIZE> {
    let font = if index == 0 { state.font } else { state.hint_font };
    let utf16 = line.encode_utf16().collect::<Vec<_>>();
    let mut size = SIZE::default();
    let previous = unsafe { SelectObject(hdc, HGDIOBJ(font.0)) };
    let success = unsafe { GetTextExtentPoint32W(hdc, &utf16, &mut size) }.as_bool();
    unsafe {
        SelectObject(hdc, previous);
    }
    if !success {
        return Err(Error::from_win32());
    }
    Ok(size)
}

fn measure_bubble(hwnd: HWND, state: &BubbleState) -> Result<(i32, i32)> {
    let hdc = unsafe { GetDC(Some(hwnd)) };
    if hdc.is_invalid() {
        return Err(Error::from_win32());
    }
    let result = (|| {
        let mut width = 0;
        let mut height = 0;
        for (index, line) in state.text.lines().enumerate() {
            let size = measure_line(hdc, state, index, line)?;
            width = width.max(size.cx);
            if index > 0 {
                height += scale(BUBBLE_TEXT_GAP, state.dpi);
            }
            height += size.cy;
        }
        Ok((
            (width + 2 * scale(BUBBLE_HORIZONTAL_PADDING / 2, state.dpi)).max(scale(BUBBLE_MIN_WIDTH, state.dpi)),
            (height + 2 * scale(BUBBLE_VERTICAL_PADDING / 2, state.dpi)).max(scale(BUBBLE_MIN_HEIGHT, state.dpi)),
        ))
    })();
    unsafe {
        let _ = ReleaseDC(Some(hwnd), hdc);
    }
    result
}

fn create_bubble_font(height: i32, dpi: u32) -> windows::Win32::Graphics::Gdi::HFONT {
    unsafe {
        CreateFontW(
            -scale(height, dpi),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
            w!("Segoe UI"),
        )
    }
}
impl Drop for SpeechBubbleWindow {
    fn drop(&mut self) {
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

struct BubbleState {
    dpi: u32,
    owner: HWND,
    pet_position: (i32, i32),
    pet_size: (i32, i32),
    text: String,
    font: windows::Win32::Graphics::Gdi::HFONT,
    hint_font: windows::Win32::Graphics::Gdi::HFONT,
    brush: windows::Win32::Graphics::Gdi::HBRUSH,
    pen: windows::Win32::Graphics::Gdi::HPEN,
    width: i32,
    height: i32,
    position: Option<(i32, i32)>,
    visible: bool,
}

impl BubbleState {
    fn new(dpi: u32, owner: HWND) -> Result<Self> {
        let (font, hint_font, brush, pen) = unsafe {
            let font = create_bubble_font(-BUBBLE_FONT_HEIGHT, dpi);
            let hint_font = create_bubble_font(BUBBLE_HINT_FONT_HEIGHT, dpi);
            let brush = CreateSolidBrush(BUBBLE_BACKGROUND);
            let pen = CreatePen(PS_SOLID, scale(BUBBLE_BORDER_WIDTH, dpi), BUBBLE_BORDER);
            (font, hint_font, brush, pen)
        };
        if font.is_invalid() || hint_font.is_invalid() || brush.is_invalid() || pen.is_invalid() {
            unsafe {
                if !font.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(font.0));
                }
                if !hint_font.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(hint_font.0));
                }
                if !brush.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                }
                if !pen.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(pen.0));
                }
            }
            return Err(Error::from_win32());
        }

        Ok(Self {
            dpi,
            owner,
            pet_position: (0, 0),
            pet_size: (0, 0),
            text: String::new(),
            font,
            hint_font,
            brush,
            pen,
            width: scale(BUBBLE_MIN_WIDTH, dpi),
            height: scale(BUBBLE_MIN_HEIGHT, dpi),
            position: None,
            visible: false,
        })
    }

    fn set_dpi(&mut self, dpi: u32) -> Result<()> {
        let dpi = dpi.max(96);
        if self.dpi == dpi {
            return Ok(());
        }
        let mut resources = Self::new(dpi, self.owner)?;
        std::mem::swap(&mut self.font, &mut resources.font);
        std::mem::swap(&mut self.hint_font, &mut resources.hint_font);
        std::mem::swap(&mut self.pen, &mut resources.pen);
        self.dpi = dpi;
        self.position = None;
        Ok(())
    }
}

impl Drop for BubbleState {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(self.font.0));
            let _ = DeleteObject(HGDIOBJ(self.hint_font.0));
            let _ = DeleteObject(HGDIOBJ(self.brush.0));
            let _ = DeleteObject(HGDIOBJ(self.pen.0));
        }
    }
}

fn scale(value: i32, dpi: u32) -> i32 {
    ((value as i64 * dpi.max(96) as i64 + 48) / 96) as i32
}

fn layout_bubble(hwnd: HWND, state: &mut BubbleState) -> Result<()> {
    let (width, height) = measure_bubble(hwnd, state)?;
    let size_changed = (state.width, state.height) != (width, height);
    let position = bubble_position(
        state.pet_position,
        state.pet_size,
        (width, height),
        work_area_for_window(state.owner)?,
        state.dpi,
    );
    if !size_changed && state.position == Some(position) {
        return Ok(());
    }
    set_ellipse_region(hwnd, width, height)?;
    state.width = width;
    state.height = height;
    state.position = Some(position);
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            position.0,
            position.1,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )?;
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    Ok(())
}

fn register_class(instance: HINSTANCE) -> Result<()> {
    static REGISTERED: OnceLock<std::result::Result<(), i32>> = OnceLock::new();
    match REGISTERED.get_or_init(|| {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(speech_bubble_window_proc),
            hInstance: instance,
            lpszClassName: BUBBLE_WINDOW_CLASS,
            ..Default::default()
        };
        let atom = unsafe { RegisterClassExW(&class) };
        if atom == 0 {
            Err(Error::from_win32().code().0)
        } else {
            Ok(())
        }
    }) {
        Ok(()) => Ok(()),
        Err(code) => Err(Error::new(
            windows::core::HRESULT(*code),
            "could not register speech bubble class",
        )),
    }
}

fn attach_state(hwnd: HWND, state: BubbleState) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(state)) as isize);
    }
}

fn state_mut(hwnd: HWND) -> Option<&'static mut BubbleState> {
    let raw =
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut BubbleState;
    unsafe { raw.as_mut() }
}

unsafe extern "system" fn speech_bubble_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => LRESULT(1),
        WM_ERASEBKGND => LRESULT(1),
        WM_DPICHANGED => {
            if let Some(state) = state_mut(hwnd) {
                if state.set_dpi((wparam.0 & 0xffff) as u32).is_ok() {
                    let _ = layout_bubble(hwnd, state);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut paint = Default::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut rect = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rect);
                if let Some(state) = state_mut(hwnd) {
                    let previous_brush = SelectObject(hdc, HGDIOBJ(state.brush.0));
                    let previous_pen = SelectObject(hdc, HGDIOBJ(state.pen.0));
                    let border_inset = scale(BUBBLE_BORDER_WIDTH, state.dpi) / 2;
                    let _ = Ellipse(
                        hdc,
                        rect.left + border_inset,
                        rect.top + border_inset,
                        rect.right - border_inset,
                        rect.bottom - border_inset,
                    );
                    SelectObject(hdc, previous_brush);
                    SelectObject(hdc, previous_pen);
                    let _ = SetBkMode(hdc, TRANSPARENT);
                    let _ = SetTextColor(hdc, BUBBLE_TEXT);
                    rect.left += scale(BUBBLE_HORIZONTAL_PADDING / 2, state.dpi);
                    rect.right -= scale(BUBBLE_HORIZONTAL_PADDING / 2, state.dpi);
                    rect.top += scale(BUBBLE_VERTICAL_PADDING / 2, state.dpi);
                    for (index, line) in state.text.lines().enumerate() {
                        let Ok(size) = measure_line(hdc, state, index, line) else {
                            continue;
                        };
                        if index > 0 {
                            rect.top += scale(BUBBLE_TEXT_GAP, state.dpi);
                        }
                        rect.bottom = rect.top + size.cy;
                        let font = if index == 0 { state.font } else { state.hint_font };
                        let previous = SelectObject(hdc, HGDIOBJ(font.0));
                        let mut text = line.encode_utf16().collect::<Vec<_>>();
                        if !text.is_empty() {
                            let _ = DrawTextW(hdc, &mut text, &mut rect, DT_CENTER | DT_NOPREFIX | DT_SINGLELINE);
                        }
                        SelectObject(hdc, previous);
                        rect.top = rect.bottom;
                    }
                }
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let raw = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if raw != 0 {
                let _state = unsafe { Box::from_raw(raw as *mut BubbleState) };
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub struct SpeechBubbleController {
    window: SpeechBubbleWindow,
    player: SpeechBubblePlayer,
    language: Language,
}

impl SpeechBubbleController {
    pub fn create(instance: HINSTANCE, owner: HWND, language: Language) -> Result<Self> {
        Ok(Self {
            window: SpeechBubbleWindow::create(instance, owner)?,
            player: SpeechBubblePlayer::new(),
            language,
        })
    }

    pub fn update(&mut self, position: (i32, i32), size: (i32, i32), now: std::time::Instant) -> Result<()> {
        let text = self.player.update(now).then(|| pet_reminder_text(self.language));
        self.window.update(position, size, text)
    }

    pub fn set_language(&mut self, language: Language) -> Result<()> {
        if self.language == language {
            return Ok(());
        }
        self.language = language;
        let state = state_mut(self.window.hwnd).ok_or_else(Error::from_win32)?;
        let visible = state.visible && self.player.is_visible();
        let (position, size) = (state.pet_position, state.pet_size);
        self.window
            .update(position, size, visible.then(|| pet_reminder_text(language)))
    }

    pub fn start(&mut self, now: std::time::Instant, initial_delay: std::time::Duration) {
        self.player.start(now, initial_delay);
    }

    pub fn set_position(&self, position: (i32, i32), size: (i32, i32)) -> Result<()> {
        self.window.set_position(position, size)
    }

    pub fn hide(&self) {
        self.window.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::bubble_position;
    use windows::Win32::Foundation::RECT;

    const WORK_AREA: RECT = RECT {
        left: 0,
        top: 0,
        right: 1_000,
        bottom: 800,
    };

    #[test]
    fn language_switch_updates_visible_text_without_restarting_or_showing_hidden_bubble() {
        use super::*;
        use std::time::{Duration, Instant};
        use windows::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::IsWindowVisible};
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let owner = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                100,
                100,
                100,
                100,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let mut controller = SpeechBubbleController::create(instance, owner, Language::English).unwrap();
            controller.set_language(Language::Chinese).unwrap();
            assert!(!IsWindowVisible(controller.window.hwnd).as_bool());
            assert!(state_mut(controller.window.hwnd).unwrap().text.is_empty());
            let now = Instant::now();
            controller.start(now, Duration::ZERO);
            // Model a visible bubble while keeping the native test window hidden.
            state_mut(controller.window.hwnd).unwrap().visible = true;
            controller.update((300, 500), (100, 100), now).unwrap();
            assert_eq!(
                state_mut(controller.window.hwnd).unwrap().text,
                "该站起来活动一下啦！\n单击桌宠，开始新一轮倒计时"
            );
            controller.set_language(Language::English).unwrap();
            assert_eq!(
                state_mut(controller.window.hwnd).unwrap().text,
                "Time to stand up and stretch!\nClick the pet to start a new countdown."
            );
            assert_eq!(state_mut(controller.window.hwnd).unwrap().text.lines().count(), 2);
            assert!(!IsWindowVisible(controller.window.hwnd).as_bool());
            controller
                .update((300, 500), (100, 100), now + Duration::from_secs(5))
                .unwrap();
            assert!(state_mut(controller.window.hwnd).unwrap().text.is_empty());
            controller.set_language(Language::Chinese).unwrap();
            assert!(!IsWindowVisible(controller.window.hwnd).as_bool());
            assert!(state_mut(controller.window.hwnd).unwrap().text.is_empty());
            drop(controller);
            DestroyWindow(owner).unwrap();
        }
    }

    #[test]
    fn dpi_changes_update_font_border_region_and_anchored_layout() {
        use super::*;
        use windows::Win32::{
            Graphics::Gdi::{CreateRectRgn, GetObjectW, GetRgnBox, GetWindowRgn, LOGFONTW, LOGPEN},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::SendMessageW,
        };
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let owner = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                100,
                100,
                100,
                100,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let bubble = SpeechBubbleWindow::create(instance, owner).unwrap();
            assert_eq!(
                state_mut(bubble.hwnd).unwrap().dpi,
                GetDpiForWindow(bubble.hwnd).max(96)
            );
            let work_area = work_area_for_window(owner).unwrap();
            let pet_position = (work_area.left + 400, work_area.top + 500);
            let pet_size = (100, 100);
            state_mut(bubble.hwnd).unwrap().text = "Stand up\nand stretch!".to_owned();
            bubble.set_position(pet_position, pet_size).unwrap();
            let mut baseline = None;
            for dpi in [96, 120, 144, 192, 144, 96] {
                SendMessageW(
                    bubble.hwnd,
                    WM_DPICHANGED,
                    Some(WPARAM(dpi | (dpi << 16))),
                    Some(LPARAM(0)),
                );
                let state = state_mut(bubble.hwnd).unwrap();
                assert_eq!(state.dpi, dpi as u32);
                let mut font = LOGFONTW::default();
                assert_ne!(
                    GetObjectW(
                        HGDIOBJ(state.font.0),
                        std::mem::size_of::<LOGFONTW>() as i32,
                        Some((&mut font as *mut LOGFONTW).cast())
                    ),
                    0
                );
                assert_eq!(font.lfHeight, -scale(30, dpi as u32));
                let mut pen = LOGPEN::default();
                assert_ne!(
                    GetObjectW(
                        HGDIOBJ(state.pen.0),
                        std::mem::size_of::<LOGPEN>() as i32,
                        Some((&mut pen as *mut LOGPEN).cast())
                    ),
                    0
                );
                assert_eq!(pen.lopnWidth.x, scale(4, dpi as u32));
                assert!(state.height > scale(BUBBLE_MIN_HEIGHT, dpi as u32));
                assert!(state.width >= scale(180, dpi as u32));
                let mut actual = RECT::default();
                GetWindowRect(bubble.hwnd, &mut actual).unwrap();
                assert_eq!(
                    (actual.right - actual.left, actual.bottom - actual.top),
                    (state.width, state.height)
                );
                assert_eq!(
                    (actual.left, actual.top),
                    bubble_position(
                        pet_position,
                        pet_size,
                        (state.width, state.height),
                        work_area,
                        dpi as u32
                    )
                );
                assert_eq!(actual.bottom, pet_position.1 - scale(BUBBLE_GAP, dpi as u32));
                let region = CreateRectRgn(0, 0, 0, 0);
                assert_ne!(GetWindowRgn(bubble.hwnd, region).0, 0);
                let mut bounds = RECT::default();
                GetRgnBox(region, &mut bounds);
                let _ = DeleteObject(HGDIOBJ(region.0));
                assert_eq!(
                    bounds,
                    RECT {
                        left: 0,
                        top: 0,
                        right: state.width - 1,
                        bottom: state.height - 1
                    }
                );
                if dpi == 96 {
                    let dimensions = (state.width, state.height);
                    if let Some(original) = baseline {
                        assert_eq!(dimensions, original);
                    }
                    baseline = Some(dimensions);
                } else {
                    assert!(state.width > baseline.unwrap().0);
                }
                // Exercise the actual paint branch on a hidden window.
                SendMessageW(bubble.hwnd, WM_PAINT, Some(WPARAM(0)), Some(LPARAM(0)));
            }
            // A changed text height must resize even if work-area clamping keeps the position fixed.
            state_mut(bubble.hwnd).unwrap().text = "One line".to_owned();
            bubble.set_position((work_area.left, work_area.top), pet_size).unwrap();
            state_mut(bubble.hwnd).unwrap().text = "One line\nTwo lines".to_owned();
            bubble.set_position((work_area.left, work_area.top), pet_size).unwrap();
            let mut actual = RECT::default();
            GetWindowRect(bubble.hwnd, &mut actual).unwrap();
            assert!(actual.bottom - actual.top > scale(BUBBLE_MIN_HEIGHT, 96));
            drop(bubble);
            DestroyWindow(owner).unwrap();
        }
    }

    #[test]
    fn localized_reminder_and_click_instruction_fit_at_supported_dpis() {
        use super::*;
        use windows::Win32::{
            Graphics::Gdi::DT_CALCRECT, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::SendMessageW,
        };
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            let owner = CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                w!(""),
                Default::default(),
                100,
                100,
                100,
                100,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let bubble = SpeechBubbleWindow::create(instance, owner).unwrap();
            for language in [Language::Chinese, Language::English] {
                state_mut(bubble.hwnd).unwrap().text = pet_reminder_text(language).to_owned();
                bubble.set_position((300, 500), (100, 100)).unwrap();
                for dpi in [96, 120, 144, 192, 96] {
                    SendMessageW(
                        bubble.hwnd,
                        WM_DPICHANGED,
                        Some(WPARAM(dpi | (dpi << 16))),
                        Some(LPARAM(0)),
                    );
                    let state = state_mut(bubble.hwnd).unwrap();
                    let hdc = GetDC(Some(bubble.hwnd));
                    assert!(!hdc.is_invalid());
                    let available_width = state.width - 2 * scale(BUBBLE_HORIZONTAL_PADDING / 2, dpi as u32);
                    let available_height = state.height - 2 * scale(BUBBLE_VERTICAL_PADDING / 2, dpi as u32);
                    let mut total_height = 0;
                    for (index, line) in state.text.lines().enumerate() {
                        let font = if index == 0 { state.font } else { state.hint_font };
                        let mut descriptor = windows::Win32::Graphics::Gdi::LOGFONTW::default();
                        assert_ne!(
                            windows::Win32::Graphics::Gdi::GetObjectW(
                                HGDIOBJ(font.0),
                                std::mem::size_of_val(&descriptor) as i32,
                                Some((&mut descriptor as *mut windows::Win32::Graphics::Gdi::LOGFONTW).cast())
                            ),
                            0
                        );
                        assert_eq!(
                            descriptor.lfHeight,
                            -scale(if index == 0 { 30 } else { 20 }, dpi as u32)
                        );
                        let previous = SelectObject(hdc, HGDIOBJ(font.0));
                        let mut text = line.encode_utf16().collect::<Vec<_>>();
                        let mut text_rect = RECT {
                            left: 0,
                            top: 0,
                            right: available_width,
                            bottom: 0,
                        };
                        let height = DrawTextW(
                            hdc,
                            &mut text,
                            &mut text_rect,
                            DT_CALCRECT | DT_NOPREFIX | DT_SINGLELINE,
                        );
                        SelectObject(hdc, previous);
                        assert!(height > 0);
                        assert!(text_rect.right <= available_width);
                        if index > 0 {
                            total_height += scale(BUBBLE_TEXT_GAP, dpi as u32);
                        }
                        total_height += text_rect.bottom;
                    }
                    let _ = ReleaseDC(Some(bubble.hwnd), hdc);
                    assert!(
                        total_height <= available_height,
                        "{language:?} at {dpi} DPI: text height {total_height} exceeds {available_height}"
                    );
                }
            }
            drop(bubble);
            DestroyWindow(owner).unwrap();
        }
    }

    #[test]
    fn moves_bubble_below_pet_when_above_edge_is_clipped() {
        assert_eq!(
            bubble_position((400, 20), (100, 100), (200, 80), WORK_AREA, 96),
            (350, 128)
        );
    }

    #[test]
    fn keeps_bubble_inside_horizontal_work_area_edges() {
        assert_eq!(bubble_position((0, 300), (100, 100), (200, 80), WORK_AREA, 96).0, 0);
        assert_eq!(bubble_position((950, 300), (100, 100), (200, 80), WORK_AREA, 96).0, 800);
    }
}
