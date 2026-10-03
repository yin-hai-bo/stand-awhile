use std::sync::OnceLock;

use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreatePen, CreateSolidBrush,
            DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_NOPREFIX, DT_WORDBREAK, DeleteObject, DrawTextW, Ellipse,
            EndPaint, FF_DONTCARE, FW_NORMAL, GetDC, GetMonitorInfoW, GetTextExtentPoint32W, HGDIOBJ, InvalidateRect,
            MONITOR_DEFAULTTONEAREST, MonitorFromWindow, OUT_DEFAULT_PRECIS, PS_SOLID, ReleaseDC, SelectObject,
            SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT,
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect, HWND_TOPMOST,
            RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SetWindowLongPtrW,
            SetWindowPos, ShowWindow, WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WNDCLASSEXW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::speech_bubble::{SpeechBubbleConfig, SpeechBubblePlayer};

const BUBBLE_WINDOW_CLASS: PCWSTR = w!("YHB-StandAwhileSpeechBubble");
const BUBBLE_MIN_WIDTH: i32 = 180;
const BUBBLE_MIN_HEIGHT: i32 = 76;
const BUBBLE_HORIZONTAL_PADDING: i32 = 80;
const BUBBLE_VERTICAL_PADDING: i32 = 56;
const BUBBLE_LINE_HEIGHT: i32 = 38;
const BUBBLE_GAP: i32 = 8;
const BUBBLE_FONT_HEIGHT: i32 = -30;
const BUBBLE_BORDER_WIDTH: i32 = 4;
const BUBBLE_BACKGROUND: COLORREF = COLORREF(0x00c4efff);
const BUBBLE_BORDER: COLORREF = COLORREF(0x006e6948);
const BUBBLE_TEXT: COLORREF = COLORREF(0x00504a35);

pub struct SpeechBubbleWindow {
    hwnd: HWND,
    owner: HWND,
}

impl SpeechBubbleWindow {
    pub fn create(instance: HINSTANCE, owner: HWND) -> Result<Self> {
        register_class(instance)?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                BUBBLE_WINDOW_CLASS,
                w!("Stand Awhile Speech Bubble"),
                WS_POPUP,
                0,
                0,
                BUBBLE_MIN_WIDTH,
                BUBBLE_MIN_HEIGHT,
                Some(owner),
                None,
                Some(instance),
                None,
            )
        }?;
        let state = match BubbleState::new() {
            Ok(state) => state,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(error);
            }
        };
        attach_state(hwnd, state);
        Ok(Self { hwnd, owner })
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

        let (width, height) = measure_bubble(self.hwnd, &state.text, state.font)?;
        let size_changed = state.width != width || state.height != height;
        state.width = width;
        state.height = height;

        if size_changed {
            set_ellipse_region(self.hwnd, width, height)?;
        }
        self.set_position(pet_position, pet_size)?;

        if text_changed || size_changed {
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
        let work_area = work_area_for_window(self.owner)?;
        let position = bubble_position(pet_position, pet_size, (state.width, state.height), work_area);
        if state.position == Some(position) {
            return Ok(());
        }
        state.position = Some(position);
        unsafe {
            SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                position.0,
                position.1,
                state.width,
                state.height,
                SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )?;
        }
        Ok(())
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
) -> (i32, i32) {
    let (width, height) = bubble_size;
    let centered_x = pet_position.0 + (pet_size.0 - width) / 2;
    let x = centered_x.clamp(work_area.left, (work_area.right - width).max(work_area.left));
    let above_y = pet_position.1 - height - BUBBLE_GAP;
    let below_y = pet_position.1 + pet_size.1 + BUBBLE_GAP;
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
        SetWindowRgn(hwnd, Some(region), true);
    }
    Ok(())
}

fn measure_bubble(hwnd: HWND, text: &str, font: windows::Win32::Graphics::Gdi::HFONT) -> Result<(i32, i32)> {
    let hdc = unsafe { GetDC(Some(hwnd)) };
    if hdc.is_invalid() {
        return Err(Error::from_win32());
    }
    let previous = unsafe { SelectObject(hdc, HGDIOBJ(font.0)) };
    let mut max_width = 0;
    for line in text.lines() {
        let utf16 = line.encode_utf16().collect::<Vec<_>>();
        let mut size = SIZE::default();
        if !unsafe { GetTextExtentPoint32W(hdc, &utf16, &mut size) }.as_bool() {
            unsafe {
                SelectObject(hdc, previous);
                let _ = ReleaseDC(Some(hwnd), hdc);
            }
            return Err(Error::from_win32());
        }
        max_width = max_width.max(size.cx);
    }
    unsafe {
        SelectObject(hdc, previous);
        let _ = ReleaseDC(Some(hwnd), hdc);
    }
    let line_count = text.lines().count().max(1) as i32;
    Ok((
        (max_width + BUBBLE_HORIZONTAL_PADDING).max(BUBBLE_MIN_WIDTH),
        (line_count * BUBBLE_LINE_HEIGHT + BUBBLE_VERTICAL_PADDING).max(BUBBLE_MIN_HEIGHT),
    ))
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
    text: String,
    font: windows::Win32::Graphics::Gdi::HFONT,
    brush: windows::Win32::Graphics::Gdi::HBRUSH,
    pen: windows::Win32::Graphics::Gdi::HPEN,
    width: i32,
    height: i32,
    position: Option<(i32, i32)>,
    visible: bool,
}

impl BubbleState {
    fn new() -> Result<Self> {
        let (font, brush, pen) = unsafe {
            let font = CreateFontW(
                BUBBLE_FONT_HEIGHT,
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
            );
            let brush = CreateSolidBrush(BUBBLE_BACKGROUND);
            let pen = CreatePen(PS_SOLID, BUBBLE_BORDER_WIDTH, BUBBLE_BORDER);
            (font, brush, pen)
        };
        if font.is_invalid() || brush.is_invalid() || pen.is_invalid() {
            unsafe {
                if !font.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(font.0));
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
            text: String::new(),
            font,
            brush,
            pen,
            width: BUBBLE_MIN_WIDTH,
            height: BUBBLE_MIN_HEIGHT,
            position: None,
            visible: false,
        })
    }
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

unsafe extern "system" fn speech_bubble_window_proc(hwnd: HWND, msg: u32, _wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => LRESULT(1),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut paint = Default::default();
            let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
            let mut rect = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rect);
                if let Some(state) = state_mut(hwnd) {
                    let previous_brush = SelectObject(hdc, HGDIOBJ(state.brush.0));
                    let previous_pen = SelectObject(hdc, HGDIOBJ(state.pen.0));
                    let border_inset = 1; // BUBBLE_BORDER_WIDTH / 2;
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
                    let previous_font = SelectObject(hdc, HGDIOBJ(state.font.0));
                    let mut text = state.text.encode_utf16().chain([0]).collect::<Vec<_>>();
                    let text_len = text.len().saturating_sub(1);
                    rect.left += 40;
                    rect.right -= 40;
                    rect.top += 28;
                    rect.bottom -= 28;
                    let _ = DrawTextW(
                        hdc,
                        &mut text[..text_len],
                        &mut rect,
                        DT_CENTER | DT_NOPREFIX | DT_WORDBREAK,
                    );
                    SelectObject(hdc, previous_font);
                }
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let raw = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if raw != 0 {
                let state = unsafe { Box::from_raw(raw as *mut BubbleState) };
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(state.font.0));
                    let _ = DeleteObject(HGDIOBJ(state.brush.0));
                    let _ = DeleteObject(HGDIOBJ(state.pen.0));
                }
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            }
            unsafe { DefWindowProcW(hwnd, msg, WPARAM(0), lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, WPARAM(0), lparam) },
    }
}

pub struct SpeechBubbleController {
    window: SpeechBubbleWindow,
    player: SpeechBubblePlayer,
}

impl SpeechBubbleController {
    pub fn create(instance: HINSTANCE, owner: HWND, config: SpeechBubbleConfig) -> Result<Self> {
        Ok(Self {
            window: SpeechBubbleWindow::create(instance, owner)?,
            player: SpeechBubblePlayer::new(config),
        })
    }

    pub fn update(&mut self, position: (i32, i32), size: (i32, i32), now: std::time::Instant) -> Result<()> {
        let text = self.player.update(now).map(str::to_owned);
        self.window.update(position, size, text.as_deref())
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
    fn moves_bubble_below_pet_when_above_edge_is_clipped() {
        assert_eq!(bubble_position((400, 20), (100, 100), (200, 80), WORK_AREA), (350, 128));
    }

    #[test]
    fn keeps_bubble_inside_horizontal_work_area_edges() {
        assert_eq!(bubble_position((0, 300), (100, 100), (200, 80), WORK_AREA).0, 0);
        assert_eq!(bubble_position((950, 300), (100, 100), (200, 80), WORK_AREA).0, 800);
    }
}
