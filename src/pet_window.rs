use std::{sync::OnceLock, time::Instant};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        UI::WindowsAndMessaging::{
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA,
            GetWindowLongPtrW, HWND_TOPMOST, IDC_ARROW, KillTimer, LoadCursorW, RegisterClassExW, SPI_GETWORKAREA,
            SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetTimer, SetWindowLongPtrW, SetWindowPos,
            ShowWindow, SystemParametersInfoW, WM_NCCREATE, WM_NCDESTROY, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
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
}

pub struct PetWindow {
    hwnd: HWND,
}

impl PetWindow {
    pub fn create(instance: HINSTANCE, animations: CatAnimations) -> Result<Self> {
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
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    #[allow(dead_code)]
    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
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
    let mut work_area = RECT::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&mut work_area as *mut RECT).cast()),
            Default::default(),
        )?
    };

    Ok((
        work_area.right - width - PET_MARGIN,
        work_area.bottom - height - PET_MARGIN,
    ))
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
                let _ = update_frame(state);
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
    use super::{ActiveClip, PlaybackState, should_return_to_idle};

    #[test]
    fn finished_jump_returns_to_idle() {
        assert!(should_return_to_idle(ActiveClip::Jump, PlaybackState::Finished));
        assert!(!should_return_to_idle(ActiveClip::Jump, PlaybackState::Playing));
        assert!(!should_return_to_idle(ActiveClip::Idle, PlaybackState::Finished));
    }
}
