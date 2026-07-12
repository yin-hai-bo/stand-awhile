use crate::ui::theme::{is_dark_theme_active, paint_background};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, EndPaint, HDC, HGDIOBJ,
    InvalidateRect, PAINTSTRUCT, SRCCOPY, SelectObject,
};
use windows::Win32::Graphics::GdiPlus::{
    DashCapRound, FillModeAlternate, GdipCreateFromHDC, GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush,
    GdipDeleteGraphics, GdipDeletePen, GdipDrawArcI, GdipDrawEllipseI, GdipFillEllipseI, GdipFillPolygonI,
    GdipFillRectangleI, GdipSetPenLineCap197819, GdipSetPenLineJoin, GdipSetPixelOffsetMode, GdipSetSmoothingMode,
    GpBrush, GpGraphics, GpPen, GpSolidFill, LineCapRound, LineJoinRound, PixelOffsetModeHalf, Point as GpPoint,
    SmoothingModeAntiAlias, Status, UnitPixel,
};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BN_CLICKED, CREATESTRUCTW, CreateWindowExW, DefWindowProcW, GWLP_USERDATA, GetClientRect, GetDlgItem, GetParent,
    GetWindowLongPtrW, HCURSOR, HMENU, IDC_ARROW, IDC_HAND, LoadCursorW, MoveWindow, PostMessageW, RegisterClassW,
    SetCursor, SetWindowLongPtrW, WINDOW_EX_STYLE, WM_CAPTURECHANGED, WM_COMMAND, WM_ENABLE, WM_ERASEBKGND,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_CHILD,
    WS_VISIBLE,
};
use windows::core::{Error, Result, w};

const BUTTON_COUNT: usize = 3;
const BUTTON_CLASS_NAME: windows::core::PCWSTR = w!("YHB-StandAwhileControlButton");
const BUTTON_PADDING: i32 = 4;
const BUTTON_BORDER_WIDTH: f32 = 2.6;
const PLAY_BUTTON_ID: i32 = 1001;
const PAUSE_BUTTON_ID: i32 = 1002;
const RESET_BUTTON_ID: i32 = 1003;

#[derive(Clone, Copy)]
struct ButtonColors {
    fill: COLORREF,
    border: COLORREF,
    icon: COLORREF,
}

struct ButtonWindowState {
    kind: ControlButton,
    hovered: bool,
    pressed: bool,
    tracking_mouse: bool,
}

#[derive(Clone, Copy)]
pub struct ControlButtonLayout {
    pub kind: ControlButton,
    pub rect: RECT,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlButton {
    Play,
    Pause,
    Reset,
}

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

const DARK_BACKGROUND: COLORREF = rgb(32, 32, 32);
const LIGHT_BACKGROUND: COLORREF = rgb(240, 240, 240);
const DARK_TEXT: COLORREF = rgb(240, 240, 240);
const LIGHT_TEXT: COLORREF = rgb(32, 32, 32);
const DARK_BUTTON_FILL: COLORREF = rgb(54, 54, 54);
const LIGHT_BUTTON_FILL: COLORREF = rgb(226, 226, 226);
const DARK_BUTTON_HOVER: COLORREF = rgb(72, 72, 72);
const LIGHT_BUTTON_HOVER: COLORREF = rgb(212, 212, 212);
const DARK_BUTTON_PRESSED: COLORREF = rgb(88, 88, 88);
const LIGHT_BUTTON_PRESSED: COLORREF = rgb(196, 196, 196);
const DARK_BUTTON_BORDER: COLORREF = rgb(124, 124, 124);
const LIGHT_BUTTON_BORDER: COLORREF = rgb(144, 144, 144);
const DARK_ACCENT_FILL: COLORREF = rgb(226, 175, 57);
const LIGHT_ACCENT_FILL: COLORREF = rgb(208, 141, 30);
const DARK_ACCENT_HOVER: COLORREF = rgb(238, 189, 82);
const LIGHT_ACCENT_HOVER: COLORREF = rgb(222, 155, 43);
const DARK_ACCENT_PRESSED: COLORREF = rgb(204, 153, 36);
const LIGHT_ACCENT_PRESSED: COLORREF = rgb(186, 123, 16);
const DARK_DISABLED_FILL: COLORREF = rgb(44, 44, 44);
const LIGHT_DISABLED_FILL: COLORREF = rgb(232, 232, 232);
const DARK_DISABLED_BORDER: COLORREF = rgb(72, 72, 72);
const LIGHT_DISABLED_BORDER: COLORREF = rgb(204, 204, 204);
const DARK_DISABLED_ICON: COLORREF = rgb(104, 104, 104);
const LIGHT_DISABLED_ICON: COLORREF = rgb(180, 180, 180);

pub fn register_button_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSW {
        lpfnWndProc: Some(button_window_proc),
        hInstance: instance,
        lpszClassName: BUTTON_CLASS_NAME,
        hCursor: load_arrow_cursor().unwrap_or(HCURSOR::default()),
        ..Default::default()
    };

    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(Error::from_win32());
    }

    Ok(())
}

pub fn create_control_buttons(parent: HWND, instance: HINSTANCE) -> Result<()> {
    for button in [ControlButton::Play, ControlButton::Pause, ControlButton::Reset] {
        let id = button_command_id(button);
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                BUTTON_CLASS_NAME,
                w!(""),
                WS_CHILD | WS_VISIBLE,
                0,
                0,
                1,
                1,
                Some(parent),
                Some(HMENU(id as usize as *mut _)),
                Some(instance),
                Some(id as usize as *const _),
            )
        }?;

        if hwnd.0.is_null() {
            return Err(Error::from_win32());
        }
    }

    Ok(())
}

pub fn layout_control_buttons(parent: HWND) -> Result<()> {
    for layout in control_button_layouts(parent)? {
        let hwnd = child_button_window(parent, layout.kind)?;
        let width = layout.rect.right - layout.rect.left;
        let height = layout.rect.bottom - layout.rect.top;
        unsafe {
            MoveWindow(hwnd, layout.rect.left, layout.rect.top, width, height, true)?;
        }
    }

    Ok(())
}

pub fn update_control_buttons(
    parent: HWND,
    play_enabled: bool,
    pause_enabled: bool,
    reset_enabled: bool,
) -> Result<()> {
    set_button_enabled(parent, ControlButton::Play, play_enabled)?;
    set_button_enabled(parent, ControlButton::Pause, pause_enabled)?;
    set_button_enabled(parent, ControlButton::Reset, reset_enabled)?;
    Ok(())
}

pub fn refresh_control_buttons(parent: HWND) -> Result<()> {
    for button in [ControlButton::Play, ControlButton::Pause, ControlButton::Reset] {
        let hwnd = child_button_window(parent, button)?;
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn controls_rect(parent: HWND) -> Result<RECT> {
    let layouts = control_button_layouts(parent)?;
    let mut union_rect = layouts[0].rect;

    for layout in &layouts[1..] {
        union_rect.left = union_rect.left.min(layout.rect.left);
        union_rect.top = union_rect.top.min(layout.rect.top);
        union_rect.right = union_rect.right.max(layout.rect.right);
        union_rect.bottom = union_rect.bottom.max(layout.rect.bottom);
    }

    Ok(union_rect)
}

pub fn button_from_command(wparam: WPARAM) -> Option<ControlButton> {
    let value = wparam.0 as u32;
    let notification = ((value >> 16) & 0xFFFF) as u16;
    let id = (value & 0xFFFF) as i32;

    if notification != BN_CLICKED as u16 {
        return None;
    }

    control_button_from_id(id)
}

unsafe extern "system" fn button_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            let Some(kind) = control_button_from_id(create.lpCreateParams as usize as i32) else {
                return LRESULT(0);
            };

            let state = Box::new(ButtonWindowState {
                kind,
                hovered: false,
                pressed: false,
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
            if let Some(state) = button_state(hwnd) {
                let _ = paint_button_buffered(hwnd, hdc, state);
            }
            unsafe {
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_ENABLE => {
            if let Some(state) = button_state_mut(hwnd) {
                state.hovered = false;
                state.pressed = false;
                state.tracking_mouse = false;
            }
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let enabled = is_button_enabled(hwnd);
            let inside = enabled && point_in_button(hwnd, lparam);

            if let Some(state) = button_state_mut(hwnd) {
                if enabled && !state.tracking_mouse {
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = unsafe { TrackMouseEvent(&mut tracking) };
                    state.tracking_mouse = true;
                }

                if state.hovered != inside {
                    state.hovered = inside;
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }

            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            if let Some(state) = button_state_mut(hwnd) {
                let should_invalidate = state.hovered || state.pressed;
                state.hovered = false;
                state.pressed = false;
                state.tracking_mouse = false;
                if should_invalidate {
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if !is_button_enabled(hwnd) {
                return LRESULT(0);
            }

            if let Some(state) = button_state_mut(hwnd) {
                let inside = point_in_button(hwnd, lparam);
                if inside {
                    state.pressed = true;
                    state.hovered = true;
                    unsafe {
                        let _ = SetCapture(hwnd);
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }

            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let enabled = is_button_enabled(hwnd);
            let inside = enabled && point_in_button(hwnd, lparam);
            let mut should_click = false;

            if let Some(state) = button_state_mut(hwnd) {
                should_click = state.pressed && inside;
                let should_invalidate = state.pressed || state.hovered != inside;
                state.pressed = false;
                state.hovered = inside;
                if should_invalidate {
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }

            unsafe {
                let _ = ReleaseCapture();
            }

            if should_click {
                notify_parent_clicked(hwnd);
            }

            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            if let Some(state) = button_state_mut(hwnd) {
                if state.pressed {
                    state.pressed = false;
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, false);
                    }
                }
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let cursor = if is_button_enabled(hwnd) {
                load_hand_cursor()
            } else {
                load_arrow_cursor()
            };
            if let Some(cursor) = cursor {
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
                let _ = unsafe { Box::from_raw(raw as *mut ButtonWindowState) };
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn draw_button_window(hwnd: HWND, hdc: HDC, state: &ButtonWindowState) -> Result<()> {
    let mut client_rect = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut client_rect)?;
    }

    let button_rect = inset_rect(client_rect, BUTTON_PADDING);
    let graphics = GdiPlusGraphics::from_hdc(hdc)?;

    draw_control_button(
        &graphics,
        ControlButtonLayout {
            kind: state.kind,
            rect: button_rect,
        },
        is_button_enabled(hwnd),
        state.hovered,
        state.pressed,
    )
}

fn paint_button_buffered(hwnd: HWND, target_hdc: HDC, state: &ButtonWindowState) -> Result<()> {
    let mut client_rect = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut client_rect)?;
    }

    let width = client_rect.right - client_rect.left;
    let height = client_rect.bottom - client_rect.top;
    if width <= 0 || height <= 0 {
        return Ok(());
    }

    let memory_hdc = unsafe { CreateCompatibleDC(Some(target_hdc)) };
    if memory_hdc.is_invalid() {
        return Err(Error::from_win32());
    }

    let bitmap = unsafe { CreateCompatibleBitmap(target_hdc, width, height) };
    if bitmap.is_invalid() {
        unsafe {
            let _ = DeleteDC(memory_hdc);
        }
        return Err(Error::from_win32());
    }

    let old_bitmap = unsafe { SelectObject(memory_hdc, HGDIOBJ(bitmap.0)) };
    let result = paint_button_contents(hwnd, memory_hdc, state).and_then(|_| {
        unsafe {
            BitBlt(target_hdc, 0, 0, width, height, Some(memory_hdc), 0, 0, SRCCOPY)?;
        }
        Ok(())
    });

    unsafe {
        let _ = SelectObject(memory_hdc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory_hdc);
    }

    result
}

fn paint_button_contents(hwnd: HWND, hdc: HDC, state: &ButtonWindowState) -> Result<()> {
    let mut client_rect = RECT::default();
    unsafe {
        GetClientRect(hwnd, &mut client_rect)?;
    }

    let _ = paint_background(&client_rect, hdc);
    draw_button_window(hwnd, hdc, state)
}

fn control_button_layouts(parent: HWND) -> Result<[ControlButtonLayout; BUTTON_COUNT]> {
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(parent, &mut client_rect)? };

    let client_width = client_rect.right - client_rect.left;
    let client_height = client_rect.bottom - client_rect.top;
    let diameter = (client_width.min(client_height) / 8).clamp(48, 68);
    let spacing = (diameter * 32) / 100;
    let total_width = diameter * BUTTON_COUNT as i32 + spacing * (BUTTON_COUNT as i32 - 1);
    let left = client_rect.left + (client_width - total_width) / 2;
    let top = client_rect.top + client_height * 58 / 100;
    let window_size = diameter + BUTTON_PADDING * 2;

    Ok([
        ControlButtonLayout {
            kind: ControlButton::Play,
            rect: rect_from_origin(left - BUTTON_PADDING, top - BUTTON_PADDING, window_size, window_size),
        },
        ControlButtonLayout {
            kind: ControlButton::Pause,
            rect: rect_from_origin(
                left + diameter + spacing - BUTTON_PADDING,
                top - BUTTON_PADDING,
                window_size,
                window_size,
            ),
        },
        ControlButtonLayout {
            kind: ControlButton::Reset,
            rect: rect_from_origin(
                left + (diameter + spacing) * 2 - BUTTON_PADDING,
                top - BUTTON_PADDING,
                window_size,
                window_size,
            ),
        },
    ])
}

fn child_button_window(parent: HWND, button: ControlButton) -> Result<HWND> {
    unsafe { GetDlgItem(Some(parent), button_command_id(button)) }
}

fn set_button_enabled(parent: HWND, button: ControlButton, enabled: bool) -> Result<()> {
    let hwnd = child_button_window(parent, button)?;
    if is_button_enabled(hwnd) == enabled {
        return Ok(());
    }

    unsafe {
        let _ = EnableWindow(hwnd, enabled);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }

    Ok(())
}

fn notify_parent_clicked(hwnd: HWND) {
    let Ok(parent) = (unsafe { GetParent(hwnd) }) else {
        return;
    };

    let id = match button_state(hwnd) {
        Some(state) => button_command_id(state.kind),
        None => return,
    };
    let command = ((BN_CLICKED as usize) << 16) | (id as usize & 0xFFFF);

    unsafe {
        let _ = PostMessageW(Some(parent), WM_COMMAND, WPARAM(command), LPARAM(hwnd.0 as isize));
    }
}

fn point_in_button(hwnd: HWND, lparam: LPARAM) -> bool {
    let (x, y) = point_from_lparam(lparam);
    let mut client_rect = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client_rect) }.is_err() {
        return false;
    }

    let rect = inset_rect(client_rect, BUTTON_PADDING);
    let center_x = (rect.left + rect.right) / 2;
    let center_y = (rect.top + rect.bottom) / 2;
    let radius = (rect.right - rect.left) / 2;
    let dx = x - center_x;
    let dy = y - center_y;
    dx * dx + dy * dy <= radius * radius
}

fn point_from_lparam(lparam: LPARAM) -> (i32, i32) {
    let value = lparam.0 as u32;
    let x = (value & 0xFFFF) as u16 as i16 as i32;
    let y = ((value >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}

fn button_state(hwnd: HWND) -> Option<&'static ButtonWindowState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const ButtonWindowState;
    unsafe { raw.as_ref() }
}

fn button_state_mut(hwnd: HWND) -> Option<&'static mut ButtonWindowState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ButtonWindowState;
    unsafe { raw.as_mut() }
}

fn is_button_enabled(hwnd: HWND) -> bool {
    unsafe { IsWindowEnabled(hwnd).as_bool() }
}

fn control_button_from_id(id: i32) -> Option<ControlButton> {
    match id {
        PLAY_BUTTON_ID => Some(ControlButton::Play),
        PAUSE_BUTTON_ID => Some(ControlButton::Pause),
        RESET_BUTTON_ID => Some(ControlButton::Reset),
        _ => None,
    }
}

fn button_command_id(button: ControlButton) -> i32 {
    match button {
        ControlButton::Play => PLAY_BUTTON_ID,
        ControlButton::Pause => PAUSE_BUTTON_ID,
        ControlButton::Reset => RESET_BUTTON_ID,
    }
}

fn load_hand_cursor() -> Option<HCURSOR> {
    unsafe { LoadCursorW(None, IDC_HAND) }.ok()
}

fn load_arrow_cursor() -> Option<HCURSOR> {
    unsafe { LoadCursorW(None, IDC_ARROW) }.ok()
}

fn inset_rect(rect: RECT, amount: i32) -> RECT {
    RECT {
        left: rect.left + amount,
        top: rect.top + amount,
        right: rect.right - amount,
        bottom: rect.bottom - amount,
    }
}

fn draw_control_button(
    graphics: &GdiPlusGraphics,
    layout: ControlButtonLayout,
    enabled: bool,
    hovered: bool,
    pressed: bool,
) -> Result<()> {
    let prominent = !matches!(layout.kind, ControlButton::Reset);
    let colors = current_button_colors(enabled, prominent, hovered, pressed);
    let fill_brush = GdiPlusBrush::solid(colors.fill)?;
    let border_pen = GdiPlusPen::new(colors.border, BUTTON_BORDER_WIDTH, true)?;
    let width = layout.rect.right - layout.rect.left;
    let height = layout.rect.bottom - layout.rect.top;

    unsafe {
        ensure_gdiplus_ok(GdipFillEllipseI(
            graphics.raw,
            fill_brush.raw as *mut GpBrush,
            layout.rect.left,
            layout.rect.top,
            width,
            height,
        ))?;
        ensure_gdiplus_ok(GdipDrawEllipseI(
            graphics.raw,
            border_pen.raw,
            layout.rect.left,
            layout.rect.top,
            width,
            height,
        ))?;
    }

    match layout.kind {
        ControlButton::Play => draw_play_icon(graphics, layout.rect, colors.icon)?,
        ControlButton::Pause => draw_pause_icon(graphics, layout.rect, colors.icon)?,
        ControlButton::Reset => draw_reset_icon(graphics, layout.rect, colors.icon)?,
    }

    Ok(())
}

fn current_button_colors(enabled: bool, prominent: bool, hovered: bool, pressed: bool) -> ButtonColors {
    let dark_mode = is_dark_theme_active();

    let (fill, border, icon) = match (dark_mode, enabled, prominent, hovered, pressed) {
        (true, false, _, _, _) => (DARK_DISABLED_FILL, DARK_DISABLED_BORDER, DARK_DISABLED_ICON),
        (true, true, true, _, true) => (DARK_ACCENT_PRESSED, DARK_ACCENT_PRESSED, DARK_BACKGROUND),
        (true, true, true, true, false) => (DARK_ACCENT_HOVER, DARK_ACCENT_HOVER, DARK_BACKGROUND),
        (true, true, true, false, false) => (DARK_ACCENT_FILL, DARK_ACCENT_FILL, DARK_BACKGROUND),
        (true, true, false, _, true) => (DARK_BUTTON_PRESSED, DARK_BUTTON_BORDER, DARK_TEXT),
        (true, true, false, true, false) => (DARK_BUTTON_HOVER, DARK_BUTTON_BORDER, DARK_TEXT),
        (true, true, false, false, false) => (DARK_BUTTON_FILL, DARK_BUTTON_BORDER, DARK_TEXT),
        (false, false, _, _, _) => (LIGHT_DISABLED_FILL, LIGHT_DISABLED_BORDER, LIGHT_DISABLED_ICON),
        (false, true, true, _, true) => (LIGHT_ACCENT_PRESSED, LIGHT_ACCENT_PRESSED, LIGHT_BACKGROUND),
        (false, true, true, true, false) => (LIGHT_ACCENT_HOVER, LIGHT_ACCENT_HOVER, LIGHT_BACKGROUND),
        (false, true, true, false, false) => (LIGHT_ACCENT_FILL, LIGHT_ACCENT_FILL, LIGHT_BACKGROUND),
        (false, true, false, _, true) => (LIGHT_BUTTON_PRESSED, LIGHT_BUTTON_BORDER, LIGHT_TEXT),
        (false, true, false, true, false) => (LIGHT_BUTTON_HOVER, LIGHT_BUTTON_BORDER, LIGHT_TEXT),
        (false, true, false, false, false) => (LIGHT_BUTTON_FILL, LIGHT_BUTTON_BORDER, LIGHT_TEXT),
    };

    ButtonColors { fill, border, icon }
}

fn draw_play_icon(graphics: &GdiPlusGraphics, rect: RECT, color: COLORREF) -> Result<()> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let left = rect.left + width * 41 / 100;
    let right = rect.left + width * 70 / 100;
    let top = rect.top + height * 29 / 100;
    let bottom = rect.top + height * 71 / 100;
    let center_y = (rect.top + rect.bottom) / 2;

    let points = [
        GpPoint { X: left, Y: top },
        GpPoint { X: left, Y: bottom },
        GpPoint { X: right, Y: center_y },
    ];

    fill_polygon(graphics, &points, color)
}

fn draw_pause_icon(graphics: &GdiPlusGraphics, rect: RECT, color: COLORREF) -> Result<()> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let bar_width = (width * 12 / 100).max(6);
    let gap = (width * 10 / 100).max(6);
    let top = rect.top + height * 28 / 100;
    let bottom = rect.top + height * 72 / 100;
    let left_bar_left = rect.left + width * 34 / 100;
    let right_bar_left = left_bar_left + bar_width + gap;

    fill_rect(
        graphics,
        rect_from_origin(left_bar_left, top, bar_width, bottom - top),
        color,
    )?;
    fill_rect(
        graphics,
        rect_from_origin(right_bar_left, top, bar_width, bottom - top),
        color,
    )
}

fn draw_reset_icon(graphics: &GdiPlusGraphics, rect: RECT, color: COLORREF) -> Result<()> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let center_x = (rect.left + rect.right) / 2;
    let center_y = (rect.top + rect.bottom) / 2;
    let radius = width.min(height) * 24 / 100;
    let pen = GdiPlusPen::new(color, 3.0, true)?;
    let arc_left = center_x - radius;
    let arc_top = center_y - radius;
    let arc_size = radius * 2;

    unsafe {
        ensure_gdiplus_ok(GdipDrawArcI(
            graphics.raw,
            pen.raw,
            arc_left,
            arc_top,
            arc_size,
            arc_size,
            140.0,
            300.0,
        ))?;
    }

    Ok(())
}

fn rect_from_origin(left: i32, top: i32, width: i32, height: i32) -> RECT {
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn fill_polygon(graphics: &GdiPlusGraphics, points: &[GpPoint], color: COLORREF) -> Result<()> {
    let brush = GdiPlusBrush::solid(color)?;
    unsafe {
        ensure_gdiplus_ok(GdipFillPolygonI(
            graphics.raw,
            brush.raw as *mut GpBrush,
            points.as_ptr(),
            points.len() as i32,
            FillModeAlternate,
        ))
    }
}

fn fill_rect(graphics: &GdiPlusGraphics, rect: RECT, color: COLORREF) -> Result<()> {
    let brush = GdiPlusBrush::solid(color)?;
    unsafe {
        ensure_gdiplus_ok(GdipFillRectangleI(
            graphics.raw,
            brush.raw as *mut GpBrush,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        ))
    }
}

fn ensure_gdiplus_ok(status: Status) -> Result<()> {
    if status.0 == 0 {
        Ok(())
    } else {
        Err(Error::from_win32())
    }
}

fn colorref_to_argb(color: COLORREF) -> u32 {
    let value = color.0;
    let red = value & 0xFF;
    let green = (value >> 8) & 0xFF;
    let blue = (value >> 16) & 0xFF;
    0xFF00_0000 | (red << 16) | (green << 8) | blue
}

struct GdiPlusGraphics {
    raw: *mut GpGraphics,
}

impl GdiPlusGraphics {
    fn from_hdc(hdc: HDC) -> Result<Self> {
        let mut raw = std::ptr::null_mut();
        unsafe {
            ensure_gdiplus_ok(GdipCreateFromHDC(hdc, &mut raw))?;
            ensure_gdiplus_ok(GdipSetSmoothingMode(raw, SmoothingModeAntiAlias))?;
            ensure_gdiplus_ok(GdipSetPixelOffsetMode(raw, PixelOffsetModeHalf))?;
        }
        Ok(Self { raw })
    }
}

impl Drop for GdiPlusGraphics {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeleteGraphics(self.raw);
        }
    }
}

struct GdiPlusBrush {
    raw: *mut GpSolidFill,
}

impl GdiPlusBrush {
    fn solid(color: COLORREF) -> Result<Self> {
        let mut raw = std::ptr::null_mut();
        unsafe {
            ensure_gdiplus_ok(GdipCreateSolidFill(colorref_to_argb(color), &mut raw))?;
        }
        Ok(Self { raw })
    }
}

impl Drop for GdiPlusBrush {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeleteBrush(self.raw as *mut GpBrush);
        }
    }
}

struct GdiPlusPen {
    raw: *mut GpPen,
}

impl GdiPlusPen {
    fn new(color: COLORREF, width: f32, rounded: bool) -> Result<Self> {
        let mut raw = std::ptr::null_mut();
        unsafe {
            ensure_gdiplus_ok(GdipCreatePen1(colorref_to_argb(color), width, UnitPixel, &mut raw))?;
            if rounded {
                ensure_gdiplus_ok(GdipSetPenLineJoin(raw, LineJoinRound))?;
                ensure_gdiplus_ok(GdipSetPenLineCap197819(raw, LineCapRound, LineCapRound, DashCapRound))?;
            }
        }
        Ok(Self { raw })
    }
}

impl Drop for GdiPlusPen {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeletePen(self.raw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ControlButton, rect_from_origin};

    #[test]
    fn builds_rect_from_origin() {
        let rect = rect_from_origin(10, 20, 30, 40);
        assert_eq!(rect.left, 10);
        assert_eq!(rect.top, 20);
        assert_eq!(rect.right, 40);
        assert_eq!(rect.bottom, 60);
    }

    #[test]
    fn control_button_equality_is_stable() {
        assert_eq!(ControlButton::Play, ControlButton::Play);
        assert_ne!(ControlButton::Pause, ControlButton::Reset);
    }
}
