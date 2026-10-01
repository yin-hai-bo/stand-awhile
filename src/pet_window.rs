use std::{sync::OnceLock, time::Instant};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        UI::WindowsAndMessaging::{
            AppendMenuW, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
            DestroyMenu, DestroyWindow, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST,
            IDC_ARROW, KillTimer, LoadCursorW, MF_STRING, RegisterClassExW, SPI_GETWORKAREA, SW_HIDE,
            SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_SHOWWINDOW, SetTimer,
            SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, TPM_LEFTALIGN, TPM_RETURNCMD,
            TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_APP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE,
            WM_NCDESTROY, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_POPUP,
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::{
    animation::{AnimationPlayer, PlaybackState},
    asset::{CatAnimations, PreparedAnimation},
    render::{LayeredRenderer, PixelSurface, SurfacePoint},
};

const PET_WINDOW_CLASS: PCWSTR = w!("YHB-StandAwhilePetWindow");
const PET_MARGIN: i32 = 24;
const PET_TIMER_ID: usize = 1;
const PET_TIMER_INTERVAL_MS: u32 = 16;
const CLICK_DRAG_THRESHOLD: i32 = 4;

pub const WM_PET_COMMAND: u32 = WM_APP + 2;
pub const PET_COMMAND_ACKNOWLEDGE: usize = 1;
pub const PET_COMMAND_SHOW_MAIN: usize = 2;
pub const PET_COMMAND_EXIT: usize = 3;
pub const PET_COMMAND_START: usize = 4;
const PET_MENU_START: usize = 1;
const PET_MENU_SHOW_MAIN: usize = 2;
const PET_MENU_EXIT: usize = 3;
const HIDE_ANIMATION_DURATION_MS: u128 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveClip {
    Idle,
    Jump,
}

struct PetWindowState {
    renderer: LayeredRenderer,
    surface: PixelSurface,
    position: (i32, i32),
    idle: PreparedAnimation,
    jump: PreparedAnimation,
    player: AnimationPlayer,
    active_clip: ActiveClip,
    last_frame: Option<(ActiveClip, u32)>,
    owner: HWND,
    drag: Option<DragState>,
    hide_animation: Option<HideAnimation>,
}

struct DragState {
    pointer_start: POINT,
    window_start: POINT,
    moved: bool,
}

struct HideAnimation {
    started_at: Instant,
    from: (i32, i32),
    restore_position: (i32, i32),
    target_y: i32,
}

pub struct PetWindow {
    hwnd: HWND,
}

impl PetWindow {
    pub fn create(instance: HINSTANCE, owner: HWND, animations: CatAnimations) -> Result<Self> {
        register_class(instance)?;

        let frame = &animations.idle.frames[0];
        let width = i32::try_from(frame.width).map_err(|_| Error::from_win32())?;
        let height = i32::try_from(frame.height).map_err(|_| Error::from_win32())?;
        let position = pet_position(width, height)?;
        let mut surface = PixelSurface::new(frame.width, frame.height).map_err(|_| Error::from_win32())?;
        surface
            .draw_frame(frame, SurfacePoint { x: 0, y: 0 })
            .map_err(|_| Error::from_win32())?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
                PET_WINDOW_CLASS,
                w!("Stand Awhile Pet"),
                WS_POPUP,
                position.0,
                position.1,
                width,
                height,
                None,
                None,
                Some(instance),
                None,
            )
        }?;

        let renderer = match LayeredRenderer::new(hwnd) {
            Ok(renderer) => renderer,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(error);
            }
        };
        let player = AnimationPlayer::new(animations.idle.clip.clone());
        attach_state(
            hwnd,
            PetWindowState {
                renderer,
                surface,
                position,
                idle: animations.idle,
                jump: animations.jump,
                player,
                active_clip: ActiveClip::Idle,
                last_frame: None,
                owner,
                drag: None,
                hide_animation: None,
            },
        );

        let pet = Self { hwnd };
        state_mut(pet.hwnd)
            .ok_or_else(Error::from_win32)?
            .player
            .play(Instant::now());
        let state = state_mut(pet.hwnd).ok_or_else(Error::from_win32)?;
        update_frame(state)?;
        unsafe {
            let _ = SetWindowPos(
                pet.hwnd,
                Some(HWND_TOPMOST),
                position.0,
                position.1,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = ShowWindow(pet.hwnd, SW_SHOWNOACTIVATE);
            let _ = SetTimer(Some(pet.hwnd), PET_TIMER_ID, PET_TIMER_INTERVAL_MS, None);
        }
        Ok(pet)
    }

    #[allow(dead_code)]
    pub fn play_jump(&self) -> Result<()> {
        let state = state_mut(self.hwnd).ok_or_else(Error::from_win32)?;
        start_jump(state);
        update_frame(state)
    }

    #[allow(dead_code)]
    pub fn hide(&self) {
        if let Some(state) = state_mut(self.hwnd) {
            cancel_hide_animation(state);
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn hide_animated(&self) -> Result<()> {
        let state = state_mut(self.hwnd).ok_or_else(Error::from_win32)?;
        if !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(self.hwnd).as_bool() } {
            return Ok(());
        }
        begin_hide_animation(state)
    }

    #[allow(dead_code)]
    pub fn show(&self) {
        if let Some(state) = state_mut(self.hwnd) {
            cancel_hide_animation(state);
            let position = state.position;
            let _ = set_position(self.hwnd, state, position);
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }
}

fn begin_hide_animation(state: &mut PetWindowState) -> Result<()> {
    let work_area = primary_work_area()?;
    let height = state.surface.height() as i32;
    state.hide_animation = Some(HideAnimation {
        started_at: Instant::now(),
        from: state.position,
        restore_position: state.position,
        target_y: hide_target_y(state.position.1, height, work_area),
    });
    Ok(())
}

fn hide_target_y(position_y: i32, height: i32, work_area: RECT) -> i32 {
    let center_y = position_y + height / 2;
    let distance_to_top = center_y - work_area.top;
    let distance_to_bottom = work_area.bottom - center_y;
    if distance_to_top < distance_to_bottom {
        work_area.top - height
    } else {
        work_area.bottom
    }
}

impl Drop for PetWindow {
    fn drop(&mut self) {
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

fn register_class(instance: HINSTANCE) -> Result<()> {
    static REGISTERED: OnceLock<std::result::Result<(), i32>> = OnceLock::new();
    match REGISTERED.get_or_init(|| {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(pet_window_proc),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
            lpszClassName: PET_WINDOW_CLASS,
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
            "could not register Pet window class",
        )),
    }
}

fn pet_position(width: i32, height: i32) -> Result<(i32, i32)> {
    let work_area = primary_work_area()?;

    Ok((
        work_area.right - width - PET_MARGIN,
        work_area.bottom - height - PET_MARGIN,
    ))
}

fn primary_work_area() -> Result<RECT> {
    let mut work_area = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut work_area as *mut RECT).cast()),
            Default::default(),
        )?
    };
    Ok(work_area)
}

fn attach_state(hwnd: HWND, state: PetWindowState) {
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(state)) as isize);
    }
}

fn state_mut(hwnd: HWND) -> Option<&'static mut PetWindowState> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut PetWindowState;
    unsafe { raw.as_mut() }
}

unsafe extern "system" fn pet_window_proc(hwnd: HWND, msg: u32, _wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            LRESULT(1)
        }
        WM_TIMER if _wparam.0 == PET_TIMER_ID => {
            if let Some(state) = state_mut(hwnd) {
                let _ = update_hide_animation(hwnd, state);
                let _ = update_frame(state);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(state) = state_mut(hwnd) {
                let _ = begin_drag(hwnd, state);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(state) = state_mut(hwnd) {
                let _ = move_drag(hwnd, state);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(state) = state_mut(hwnd) {
                let _ = end_drag(state);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if let Some(state) = state_mut(hwnd) {
                let _ = show_context_menu(hwnd, state);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), PET_TIMER_ID);
            }
            let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if raw != 0 {
                let _ = unsafe { Box::from_raw(raw as *mut PetWindowState) };
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
            }
            unsafe { DefWindowProcW(hwnd, msg, WPARAM(0), lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, WPARAM(0), lparam) },
    }
}

fn start_jump(state: &mut PetWindowState) {
    state.active_clip = ActiveClip::Jump;
    state.player = AnimationPlayer::new(state.jump.clip.clone());
    state.player.play(Instant::now());
}

fn begin_drag(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    let mut pointer = POINT::default();
    unsafe { GetCursorPos(&mut pointer)? };
    let mut window = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut window)? };
    state.drag = Some(DragState {
        pointer_start: pointer,
        window_start: POINT {
            x: window.left,
            y: window.top,
        },
        moved: false,
    });
    unsafe {
        let _ = SetCapture(hwnd);
    }
    Ok(())
}

fn move_drag(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    let Some(drag) = state.drag.as_mut() else {
        return Ok(());
    };
    let mut pointer = POINT::default();
    unsafe { GetCursorPos(&mut pointer)? };
    drag.moved = drag.moved || movement_exceeded(drag.pointer_start, pointer);
    let position = (
        drag.window_start.x + pointer.x - drag.pointer_start.x,
        drag.window_start.y + pointer.y - drag.pointer_start.y,
    );
    set_position(hwnd, state, position)
}

fn end_drag(state: &mut PetWindowState) -> Result<()> {
    let Some(drag) = state.drag.take() else {
        return Ok(());
    };
    unsafe {
        let _ = ReleaseCapture();
    }
    if !drag.moved {
        let _ = begin_hide_animation(state);
        post_pet_command(state.owner, PET_COMMAND_ACKNOWLEDGE);
    }
    Ok(())
}

fn show_context_menu(hwnd: HWND, state: &PetWindowState) -> Result<()> {
    let menu = unsafe { CreatePopupMenu()? };
    unsafe {
        AppendMenuW(menu, MF_STRING, PET_MENU_START, w!("Start Timer"))?;
        AppendMenuW(menu, MF_STRING, PET_MENU_SHOW_MAIN, w!("Show Main Window"))?;
        AppendMenuW(menu, MF_STRING, PET_MENU_EXIT, w!("Exit"))?;
    }

    let mut point = POINT::default();
    let command = unsafe {
        GetCursorPos(&mut point)?;
        let command = TrackPopupMenuEx(
            menu,
            (TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD).0,
            point.x,
            point.y,
            hwnd,
            None,
        )
        .0 as usize;
        let _ = DestroyMenu(menu);
        command
    };

    match command {
        PET_MENU_START => post_pet_command(state.owner, PET_COMMAND_START),
        PET_MENU_SHOW_MAIN => post_pet_command(state.owner, PET_COMMAND_SHOW_MAIN),
        PET_MENU_EXIT => post_pet_command(state.owner, PET_COMMAND_EXIT),
        _ => {}
    }
    Ok(())
}

fn post_pet_command(owner: HWND, command: usize) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(owner),
            WM_PET_COMMAND,
            WPARAM(command),
            LPARAM(0),
        );
    }
}

fn movement_exceeded(start: POINT, current: POINT) -> bool {
    (current.x - start.x).abs() > CLICK_DRAG_THRESHOLD || (current.y - start.y).abs() > CLICK_DRAG_THRESHOLD
}

fn update_hide_animation(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    let Some(animation) = state.hide_animation.as_ref() else {
        return Ok(());
    };
    let elapsed = animation.started_at.elapsed().as_millis();
    let progress = elapsed.min(HIDE_ANIMATION_DURATION_MS);
    let from = animation.from;
    let target_y = animation.target_y;
    let restore_position = animation.restore_position;
    let y = from.1 + (target_y - from.1) * progress as i32 / HIDE_ANIMATION_DURATION_MS as i32;
    set_position(hwnd, state, (from.0, y))?;
    if elapsed >= HIDE_ANIMATION_DURATION_MS {
        state.hide_animation = None;
        state.position = restore_position;
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
    Ok(())
}

fn cancel_hide_animation(state: &mut PetWindowState) {
    if let Some(animation) = state.hide_animation.take() {
        state.position = animation.restore_position;
    }
}

fn set_position(hwnd: HWND, state: &mut PetWindowState, position: (i32, i32)) -> Result<()> {
    state.position = position;
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            position.0,
            position.1,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        )?;
    }
    state.renderer.submit(
        &state.surface,
        SurfacePoint {
            x: position.0,
            y: position.1,
        },
    )?;
    Ok(())
}

fn update_frame(state: &mut PetWindowState) -> Result<()> {
    let now = Instant::now();
    let mut selection = state.player.update(now);
    if should_return_to_idle(state.active_clip, selection.state) {
        state.active_clip = ActiveClip::Idle;
        state.player = AnimationPlayer::new(state.idle.clip.clone());
        state.player.play(now);
        selection = state.player.update(now);
    }

    let frame_key = (state.active_clip, selection.frame.id);
    if state.last_frame == Some(frame_key) {
        return Ok(());
    }

    let frames = match state.active_clip {
        ActiveClip::Idle => &state.idle.frames,
        ActiveClip::Jump => &state.jump.frames,
    };
    let frame = frames.get(selection.frame.id as usize).ok_or_else(Error::from_win32)?;
    state.surface.clear();
    state
        .surface
        .draw_frame(frame, SurfacePoint { x: 0, y: 0 })
        .map_err(|_| Error::from_win32())?;
    state.renderer.submit(
        &state.surface,
        SurfacePoint {
            x: state.position.0,
            y: state.position.1,
        },
    )?;
    state.last_frame = Some(frame_key);
    Ok(())
}

fn should_return_to_idle(active_clip: ActiveClip, playback_state: PlaybackState) -> bool {
    active_clip == ActiveClip::Jump && playback_state == PlaybackState::Finished
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Foundation::RECT;

    use super::{ActiveClip, PlaybackState, hide_target_y, movement_exceeded, should_return_to_idle};

    #[test]
    fn finished_jump_returns_to_idle() {
        assert!(should_return_to_idle(ActiveClip::Jump, PlaybackState::Finished));
        assert!(!should_return_to_idle(ActiveClip::Jump, PlaybackState::Playing));
        assert!(!should_return_to_idle(ActiveClip::Idle, PlaybackState::Finished));
    }

    #[test]
    fn small_pointer_movement_is_not_dragging() {
        let start = POINT { x: 100, y: 100 };
        assert!(!movement_exceeded(start, POINT { x: 104, y: 104 }));
        assert!(movement_exceeded(start, POINT { x: 105, y: 100 }));
    }

    #[test]
    fn hide_animation_exits_through_the_nearer_vertical_edge() {
        let work_area = RECT {
            left: 0,
            top: 100,
            right: 1920,
            bottom: 1100,
        };
        assert_eq!(hide_target_y(120, 100, work_area), 0);
        assert_eq!(hide_target_y(1000, 100, work_area), 1100);
    }
}
