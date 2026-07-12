use crate::ui::theme::is_dark_theme_active;
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT, RECT},
    Graphics::Gdi::{
        CreatePen, CreateSolidBrush, DeleteObject, Ellipse, GetStockObject, HDC, HOLLOW_BRUSH, PS_SOLID, Polygon,
        Polyline, Rectangle, SelectObject,
    },
    UI::WindowsAndMessaging::GetClientRect,
};
use windows::core::Error;

const BUTTON_COUNT: usize = 3;

#[derive(Clone, Copy)]
struct ButtonColors {
    fill: COLORREF,
    border: COLORREF,
    icon: COLORREF,
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
const DARK_BUTTON_BORDER: COLORREF = rgb(108, 108, 108);
const LIGHT_BUTTON_BORDER: COLORREF = rgb(156, 156, 156);
const DARK_ACCENT_FILL: COLORREF = rgb(226, 175, 57);
const LIGHT_ACCENT_FILL: COLORREF = rgb(208, 141, 30);
const DARK_ACCENT_HOVER: COLORREF = rgb(238, 189, 82);
const LIGHT_ACCENT_HOVER: COLORREF = rgb(222, 155, 43);
const DARK_ACCENT_PRESSED: COLORREF = rgb(204, 153, 36);
const LIGHT_ACCENT_PRESSED: COLORREF = rgb(186, 123, 16);
const DARK_DISABLED_FILL: COLORREF = rgb(44, 44, 44);
const LIGHT_DISABLED_FILL: COLORREF = rgb(232, 232, 232);
const DARK_DISABLED_BORDER: COLORREF = rgb(78, 78, 78);
const LIGHT_DISABLED_BORDER: COLORREF = rgb(188, 188, 188);
const DARK_DISABLED_ICON: COLORREF = rgb(118, 118, 118);
const LIGHT_DISABLED_ICON: COLORREF = rgb(168, 168, 168);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlButton {
    Play,
    Pause,
    Reset,
}

#[derive(Clone, Copy)]
pub struct ControlButtonLayout {
    pub kind: ControlButton,
    pub rect: RECT,
}

pub fn draw_controls(
    hwnd: HWND,
    hdc: HDC,
    hovered: Option<ControlButton>,
    pressed: Option<ControlButton>,
    play_enabled: bool,
    pause_enabled: bool,
    reset_enabled: bool,
) -> windows::core::Result<()> {
    for button in control_button_layouts(hwnd)? {
        let is_enabled = match button.kind {
            ControlButton::Play => play_enabled,
            ControlButton::Pause => pause_enabled,
            ControlButton::Reset => reset_enabled,
        };
        let is_hovered = is_enabled && hovered == Some(button.kind);
        let is_pressed = is_enabled && pressed == Some(button.kind);

        draw_control_button(hdc, button, is_enabled, is_hovered, is_pressed)?;
    }

    Ok(())
}

pub fn controls_rect(hwnd: HWND) -> windows::core::Result<RECT> {
    let layouts = control_button_layouts(hwnd)?;
    let mut union_rect = layouts[0].rect;

    for button in &layouts[1..] {
        union_rect.left = union_rect.left.min(button.rect.left);
        union_rect.top = union_rect.top.min(button.rect.top);
        union_rect.right = union_rect.right.max(button.rect.right);
        union_rect.bottom = union_rect.bottom.max(button.rect.bottom);
    }

    union_rect.left -= 4;
    union_rect.top -= 4;
    union_rect.right += 4;
    union_rect.bottom += 4;

    Ok(union_rect)
}

pub fn hit_test_control_button(
    hwnd: HWND,
    x: i32,
    y: i32,
    play_enabled: bool,
    pause_enabled: bool,
    reset_enabled: bool,
) -> windows::core::Result<Option<ControlButton>> {
    for button in control_button_layouts(hwnd)? {
        let is_enabled = match button.kind {
            ControlButton::Play => play_enabled,
            ControlButton::Pause => pause_enabled,
            ControlButton::Reset => reset_enabled,
        };
        if !is_enabled {
            continue;
        }

        let center_x = (button.rect.left + button.rect.right) / 2;
        let center_y = (button.rect.top + button.rect.bottom) / 2;
        let radius = (button.rect.right - button.rect.left) / 2;
        let dx = x - center_x;
        let dy = y - center_y;
        if dx * dx + dy * dy <= radius * radius {
            return Ok(Some(button.kind));
        }
    }

    Ok(None)
}

fn control_button_layouts(hwnd: HWND) -> windows::core::Result<[ControlButtonLayout; BUTTON_COUNT]> {
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client_rect)? };

    let client_width = client_rect.right - client_rect.left;
    let client_height = client_rect.bottom - client_rect.top;
    let diameter = (client_width.min(client_height) / 8).clamp(48, 68);
    let spacing = (diameter * 32) / 100;
    let total_width = diameter * BUTTON_COUNT as i32 + spacing * (BUTTON_COUNT as i32 - 1);
    let left = client_rect.left + (client_width - total_width) / 2;
    let top = client_rect.top + client_height * 58 / 100;

    Ok([
        ControlButtonLayout {
            kind: ControlButton::Play,
            rect: rect_from_origin(left, top, diameter, diameter),
        },
        ControlButtonLayout {
            kind: ControlButton::Pause,
            rect: rect_from_origin(left + diameter + spacing, top, diameter, diameter),
        },
        ControlButtonLayout {
            kind: ControlButton::Reset,
            rect: rect_from_origin(left + (diameter + spacing) * 2, top, diameter, diameter),
        },
    ])
}

fn rect_from_origin(left: i32, top: i32, width: i32, height: i32) -> RECT {
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn draw_control_button(
    hdc: HDC,
    layout: ControlButtonLayout,
    enabled: bool,
    hovered: bool,
    pressed: bool,
) -> windows::core::Result<()> {
    let prominent = !matches!(layout.kind, ControlButton::Reset);
    let colors = current_button_colors(enabled, prominent, hovered, pressed);
    let pen = unsafe { CreatePen(PS_SOLID, 2, colors.border) };
    if pen.is_invalid() {
        return Err(Error::from_win32());
    }

    let brush = unsafe { CreateSolidBrush(colors.fill) };
    if brush.is_invalid() {
        unsafe {
            let _ = DeleteObject(pen.into());
        }
        return Err(Error::from_win32());
    }

    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    let old_brush = unsafe { SelectObject(hdc, brush.into()) };

    unsafe {
        let _ = Ellipse(
            hdc,
            layout.rect.left,
            layout.rect.top,
            layout.rect.right,
            layout.rect.bottom,
        );
    }

    match layout.kind {
        ControlButton::Play => draw_play_icon(hdc, layout.rect, colors.icon)?,
        ControlButton::Pause => draw_pause_icon(hdc, layout.rect, colors.icon)?,
        ControlButton::Reset => draw_reset_icon(hdc, layout.rect, colors.icon)?,
    }

    unsafe {
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
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

fn draw_play_icon(hdc: HDC, rect: RECT, color: COLORREF) -> windows::core::Result<()> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let left = rect.left + width * 38 / 100;
    let right = rect.left + width * 67 / 100;
    let top = rect.top + height * 29 / 100;
    let bottom = rect.top + height * 71 / 100;
    let center_y = (rect.top + rect.bottom) / 2;

    let points = [
        POINT { x: left, y: top },
        POINT { x: left, y: bottom },
        POINT { x: right, y: center_y },
    ];

    fill_polygon(hdc, &points, color)
}

fn draw_pause_icon(hdc: HDC, rect: RECT, color: COLORREF) -> windows::core::Result<()> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let bar_width = (width * 12 / 100).max(6);
    let gap = (width * 10 / 100).max(6);
    let top = rect.top + height * 28 / 100;
    let bottom = rect.top + height * 72 / 100;
    let left_bar_left = rect.left + width * 34 / 100;
    let right_bar_left = left_bar_left + bar_width + gap;

    fill_rect(
        hdc,
        rect_from_origin(left_bar_left, top, bar_width, bottom - top),
        color,
    )?;
    fill_rect(
        hdc,
        rect_from_origin(right_bar_left, top, bar_width, bottom - top),
        color,
    )
}

fn draw_reset_icon(hdc: HDC, rect: RECT, color: COLORREF) -> windows::core::Result<()> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let center_x = (rect.left + rect.right) / 2;
    let center_y = (rect.top + rect.bottom) / 2;
    let radius = width.min(height) * 24 / 100;

    let pen = unsafe { CreatePen(PS_SOLID, 3, color) };
    if pen.is_invalid() {
        return Err(Error::from_win32());
    }

    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    let hollow_brush = unsafe { GetStockObject(HOLLOW_BRUSH) };
    let old_brush = unsafe { SelectObject(hdc, hollow_brush) };

    let points = arc_points(center_x, center_y, radius, 210.0, -95.0, 14);
    unsafe {
        let _ = Polyline(hdc, &points);
    }

    let arrow_tip = *points.last().expect("arc points should not be empty");
    let arrow_points = [
        arrow_tip,
        POINT {
            x: arrow_tip.x - width * 9 / 100,
            y: arrow_tip.y + height * 4 / 100,
        },
        POINT {
            x: arrow_tip.x - width * 2 / 100,
            y: arrow_tip.y + height * 10 / 100,
        },
    ];

    unsafe {
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(pen.into());
    }

    fill_polygon(hdc, &arrow_points, color)
}

fn arc_points(
    center_x: i32,
    center_y: i32,
    radius: i32,
    start_degrees: f64,
    end_degrees: f64,
    steps: usize,
) -> Vec<POINT> {
    let mut points = Vec::with_capacity(steps + 1);

    for step in 0..=steps {
        let t = step as f64 / steps as f64;
        let angle = (start_degrees + (end_degrees - start_degrees) * t).to_radians();
        points.push(POINT {
            x: center_x + (radius as f64 * angle.cos()).round() as i32,
            y: center_y - (radius as f64 * angle.sin()).round() as i32,
        });
    }

    points
}

fn fill_polygon(hdc: HDC, points: &[POINT], color: COLORREF) -> windows::core::Result<()> {
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    if pen.is_invalid() {
        return Err(Error::from_win32());
    }

    let brush = unsafe { CreateSolidBrush(color) };
    if brush.is_invalid() {
        unsafe {
            let _ = DeleteObject(pen.into());
        }
        return Err(Error::from_win32());
    }

    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    let old_brush = unsafe { SelectObject(hdc, brush.into()) };

    unsafe {
        let _ = Polygon(hdc, points);
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }

    Ok(())
}

fn fill_rect(hdc: HDC, rect: RECT, color: COLORREF) -> windows::core::Result<()> {
    let pen = unsafe { CreatePen(PS_SOLID, 1, color) };
    if pen.is_invalid() {
        return Err(Error::from_win32());
    }

    let brush = unsafe { CreateSolidBrush(color) };
    if brush.is_invalid() {
        unsafe {
            let _ = DeleteObject(pen.into());
        }
        return Err(Error::from_win32());
    }

    let old_pen = unsafe { SelectObject(hdc, pen.into()) };
    let old_brush = unsafe { SelectObject(hdc, brush.into()) };

    unsafe {
        let _ = Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
        let _ = SelectObject(hdc, old_brush);
        let _ = SelectObject(hdc, old_pen);
        let _ = DeleteObject(brush.into());
        let _ = DeleteObject(pen.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ControlButton, arc_points, rect_from_origin};

    #[test]
    fn builds_rect_from_origin() {
        let rect = rect_from_origin(10, 20, 30, 40);
        assert_eq!(rect.left, 10);
        assert_eq!(rect.top, 20);
        assert_eq!(rect.right, 40);
        assert_eq!(rect.bottom, 60);
    }

    #[test]
    fn reset_arc_contains_expected_number_of_points() {
        let points = arc_points(50, 50, 20, 210.0, -95.0, 14);
        assert_eq!(points.len(), 15);
    }

    #[test]
    fn control_button_equality_is_stable() {
        assert_eq!(ControlButton::Play, ControlButton::Play);
        assert_ne!(ControlButton::Pause, ControlButton::Reset);
    }
}
