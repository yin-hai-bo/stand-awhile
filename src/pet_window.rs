use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromPoint, MonitorFromWindow,
        },
        UI::HiDpi::GetDpiForWindow,
        UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        UI::WindowsAndMessaging::{
            AppendMenuW, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
            DestroyMenu, DestroyWindow, GWLP_USERDATA, GetCursorPos, GetWindowLongPtrW, GetWindowRect, HTCLIENT,
            HTTRANSPARENT, HWND_TOPMOST, IDC_ARROW, KillTimer, LoadCursorW, MF_SEPARATOR, MF_STRING, RegisterClassExW,
            SPI_GETWORKAREA, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetTimer,
            SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, TPM_LEFTALIGN, TPM_RETURNCMD,
            TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_APP, WM_DPICHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
            WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::{
    animation::{AnimationPlayer, PlaybackState},
    asset::{CharacterAnimations, PreparedAnimation, PreparedFrame},
    config::{PetPosition, save_pet_position},
    i18n::Language,
    render::{LayeredRenderer, PixelSurface, SurfacePoint},
    speech_bubble_window::SpeechBubbleController,
};

const PET_WINDOW_CLASS: PCWSTR = w!("YHB-StandAwhilePetWindow");
const PET_MARGIN: i32 = 24;
const PET_TIMER_ID: usize = 1;
const PET_TIMER_INTERVAL_MS: u32 = 16;
const CLICK_DRAG_THRESHOLD: i32 = 4;
const MIN_VISIBLE_PET_SIZE: i32 = 24;
const WALK_START_DELAY: Duration = Duration::from_secs(2);
const SPEECH_BUBBLE_INITIAL_DELAY: Duration = Duration::from_millis(500);

pub const WM_PET_COMMAND: u32 = WM_APP + 2;
pub const PET_COMMAND_ACKNOWLEDGE: usize = 1;
pub const PET_COMMAND_SHOW_MAIN: usize = 2;
pub const PET_COMMAND_EXIT: usize = 3;
pub const PET_COMMAND_START: usize = 4;
pub const PET_COMMAND_SETTINGS: usize = 5;
pub const PET_COMMAND_ABOUT: usize = 6;
const PET_MENU_START: usize = 1;
const PET_MENU_SHOW_MAIN: usize = 2;
const PET_MENU_EXIT: usize = 3;
const PET_MENU_SETTINGS: usize = 4;
const PET_MENU_ABOUT: usize = 5;
const HIDE_ANIMATION_DURATION_MS: u128 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveClip {
    Idle,
    Walk,
    Jump,
}

struct PetWindowState {
    renderer: LayeredRenderer,
    surface: PixelSurface,
    dpi: u32,
    position: (i32, i32),
    idle: PreparedAnimation,
    walk: PreparedAnimation,
    jump: PreparedAnimation,
    player: AnimationPlayer,
    active_clip: ActiveClip,
    last_frame: Option<(ActiveClip, u32)>,
    owner: HWND,
    drag: Option<DragState>,
    hide_animation: Option<HideAnimation>,
    next_walk_at: Instant,
    speech_bubble: SpeechBubbleController,
    start_menu_text: String,
    show_main_menu_text: String,
    settings_menu_text: String,
    about_menu_text: String,
    exit_menu_text: String,
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
    pub fn create(
        instance: HINSTANCE,
        owner: HWND,
        animations: CharacterAnimations,
        language: Language,
        saved_position: Option<PetPosition>,
        settings_menu_text: &str,
    ) -> Result<Self> {
        register_class(instance)?;

        let frame = &animations.idle.frames[0];
        let width = i32::try_from(frame.width).map_err(|_| Error::from_win32())?;
        let height = i32::try_from(frame.height).map_err(|_| Error::from_win32())?;
        let position = saved_position
            .map(|saved| restore_position(&saved, (width, height)))
            .transpose()?
            .unwrap_or(pet_position(width, height)?);
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

        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let mut surface = pet_surface(frame, dpi)?;
        surface.draw_scaled_frame(frame).map_err(|_| Error::from_win32())?;
        let position = clamp_position(
            position,
            (surface.width() as i32, surface.height() as i32),
            monitor_work_area(hwnd)?,
        );

        let renderer = match LayeredRenderer::new(hwnd) {
            Ok(renderer) => renderer,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(error);
            }
        };
        let speech_bubble = match SpeechBubbleController::create(instance, hwnd, language) {
            Ok(speech_bubble) => speech_bubble,
            Err(error) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(error);
            }
        };
        let now = Instant::now();
        let player = AnimationPlayer::new(animations.idle.clip.clone());
        attach_state(
            hwnd,
            PetWindowState {
                renderer,
                surface,
                dpi,
                position,
                idle: animations.idle,
                walk: animations.walk,
                jump: animations.jump,
                player,
                active_clip: ActiveClip::Idle,
                last_frame: None,
                owner,
                drag: None,
                hide_animation: None,
                next_walk_at: now + WALK_START_DELAY,
                speech_bubble,
                start_menu_text: crate::tray_menu_start_text(Language::English).to_owned(),
                show_main_menu_text: crate::tray_menu_show_text(Language::English).to_owned(),
                settings_menu_text: settings_menu_text.to_owned(),
                about_menu_text: crate::tray_menu_about_text(Language::English).to_owned(),
                exit_menu_text: crate::tray_menu_exit_text(Language::English).to_owned(),
            },
        );

        let pet = Self { hwnd };
        state_mut(pet.hwnd).ok_or_else(Error::from_win32)?.player.play(now);
        let state = state_mut(pet.hwnd).ok_or_else(Error::from_win32)?;
        update_frame(state)?;
        unsafe {
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
            state.speech_bubble.hide();
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
        begin_hide_animation(self.hwnd, state)
    }

    #[allow(dead_code)]
    pub fn show(&self) {
        if let Some(state) = state_mut(self.hwnd) {
            cancel_hide_animation(state);
            let size = (state.surface.width() as i32, state.surface.height() as i32);
            let position = monitor_work_area(self.hwnd)
                .map(|work_area| clamp_position(state.position, size, work_area))
                .unwrap_or(state.position);
            let _ = set_position(self.hwnd, state, position);
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    pub fn set_language(&self, language: Language) {
        if let Some(state) = state_mut(self.hwnd) {
            let _ = state.speech_bubble.set_language(language);
            state.start_menu_text = crate::tray_menu_start_text(language).to_owned();
            state.show_main_menu_text = crate::tray_menu_show_text(language).to_owned();
            state.settings_menu_text = crate::settings_menu_text(language).to_owned();
            state.about_menu_text = crate::tray_menu_about_text(language).to_owned();
            state.exit_menu_text = crate::tray_menu_exit_text(language).to_owned();
        }
    }

    pub fn set_animations(&self, animations: CharacterAnimations) -> Result<()> {
        let state = state_mut(self.hwnd).ok_or_else(Error::from_win32)?;
        let surface = pet_surface(&animations.idle.frames[0], state.dpi)?;
        state.idle = animations.idle;
        state.walk = animations.walk;
        state.jump = animations.jump;
        state.active_clip = ActiveClip::Idle;
        state.player = AnimationPlayer::new(state.idle.clip.clone());
        state.player.play(Instant::now());
        state.last_frame = None;
        state.surface = surface;
        let position = state.position;
        set_position(self.hwnd, state, position)?;
        update_frame(state)
    }
}

fn begin_hide_animation(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    state.speech_bubble.hide();
    let work_area = monitor_work_area(hwnd)?;
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

fn restore_position(saved: &PetPosition, size: (i32, i32)) -> Result<(i32, i32)> {
    let monitor = unsafe {
        MonitorFromPoint(
            POINT {
                x: saved.screen_x,
                y: saved.screen_y,
            },
            MONITOR_DEFAULTTONEAREST,
        )
    };
    let info = monitor_info_ex(monitor)?;
    let monitor_name = monitor_device_name(&info);
    let candidate = if monitor_name == saved.monitor {
        (
            info.monitorInfo.rcWork.left + saved.relative_x,
            info.monitorInfo.rcWork.top + saved.relative_y,
        )
    } else {
        (saved.screen_x, saved.screen_y)
    };
    Ok(clamp_position(candidate, size, info.monitorInfo.rcWork))
}

fn persistent_position(hwnd: HWND, position: (i32, i32)) -> Result<PetPosition> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let info = monitor_info_ex(monitor)?;
    Ok(PetPosition {
        monitor: monitor_device_name(&info),
        relative_x: position.0 - info.monitorInfo.rcWork.left,
        relative_y: position.1 - info.monitorInfo.rcWork.top,
        screen_x: position.0,
        screen_y: position.1,
    })
}

fn monitor_info_ex(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Result<MONITORINFOEXW> {
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) }.as_bool() {
        return Err(Error::from_win32());
    }
    Ok(info)
}

fn monitor_device_name(info: &MONITORINFOEXW) -> String {
    let length = info
        .szDevice
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(info.szDevice.len());
    String::from_utf16_lossy(&info.szDevice[..length])
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

unsafe extern "system" fn pet_window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            LRESULT(1)
        }
        WM_TIMER if wparam.0 == PET_TIMER_ID => {
            if let Some(state) = state_mut(hwnd) {
                let _ = update_hide_animation(hwnd, state);
                let _ = update_idle_walk(hwnd, state);
                let _ = update_frame(state);
                let _ = update_speech_bubble(hwnd, state);
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(state) = state_mut(hwnd) {
                let rect = unsafe { &*(lparam.0 as *const RECT) };
                let _ = apply_pet_dpi(state, (wparam.0 & 0xffff) as u32, (rect.left, rect.top));
            }
            LRESULT(0)
        }
        WM_NCHITTEST => {
            if let Some(state) = state_mut(hwnd) {
                if !hit_test_current_frame(hwnd, state, lparam) {
                    return LRESULT(HTTRANSPARENT as isize);
                }
            }
            LRESULT(HTCLIENT as isize)
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
                let _ = end_drag(hwnd, state);
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
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn start_jump(state: &mut PetWindowState) {
    let now = Instant::now();
    state.active_clip = ActiveClip::Jump;
    state.player = AnimationPlayer::new(state.jump.clip.clone());
    state.player.play(now);
    state.next_walk_at = now + WALK_START_DELAY;
    state.speech_bubble.start(now, SPEECH_BUBBLE_INITIAL_DELAY);
}

fn start_walk(state: &mut PetWindowState, now: Instant) {
    state.active_clip = ActiveClip::Walk;
    state.player = AnimationPlayer::new(state.walk.clip.clone());
    state.player.play(now);
}

fn start_idle(state: &mut PetWindowState, now: Instant) {
    state.active_clip = ActiveClip::Idle;
    state.player = AnimationPlayer::new(state.idle.clip.clone());
    state.player.play(now);
    state.next_walk_at = now + WALK_START_DELAY;
}

fn update_idle_walk(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    if state.hide_animation.is_some()
        || state.drag.is_some()
        || !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
        || state.active_clip == ActiveClip::Jump
    {
        return Ok(());
    }

    let now = Instant::now();
    if state.active_clip == ActiveClip::Idle {
        if now >= state.next_walk_at {
            start_walk(state, now);
        }
        return Ok(());
    }

    Ok(())
}

fn begin_drag(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    if state.active_clip == ActiveClip::Walk {
        start_idle(state, Instant::now());
    }
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

fn hit_test_current_frame(hwnd: HWND, state: &PetWindowState, lparam: LPARAM) -> bool {
    let Some(_) = state.last_frame else {
        return true;
    };
    let mut window = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
        return false;
    }
    let screen_x = (lparam.0 as u16) as i16 as i32;
    let screen_y = ((lparam.0 >> 16) as u16) as i16 as i32;
    let x = screen_x - window.left;
    let y = screen_y - window.top;
    if x < 0 || y < 0 || x >= state.surface.width() as i32 || y >= state.surface.height() as i32 {
        return false;
    }
    state.surface.pixels()[((y as u32 * state.surface.width() + x as u32) * 4 + 3) as usize] >= 16
}

fn move_drag(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    let Some(drag) = state.drag.as_mut() else {
        return Ok(());
    };
    let mut pointer = POINT::default();
    unsafe { GetCursorPos(&mut pointer)? };
    drag.moved = drag.moved || movement_exceeded(drag.pointer_start, pointer);
    let position = clamp_drag_position(
        (
            drag.window_start.x + pointer.x - drag.pointer_start.x,
            drag.window_start.y + pointer.y - drag.pointer_start.y,
        ),
        (state.surface.width() as i32, state.surface.height() as i32),
        monitor_work_area_at_point(pointer)?,
    );
    set_position(hwnd, state, position)
}

fn end_drag(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    let Some(drag) = state.drag.take() else {
        return Ok(());
    };
    unsafe {
        let _ = ReleaseCapture();
    }
    if !drag.moved {
        let _ = begin_hide_animation(hwnd, state);
        post_pet_command(state.owner, PET_COMMAND_ACKNOWLEDGE);
    }
    if drag.moved {
        if let Ok(position) = persistent_position(hwnd, state.position) {
            let _ = save_pet_position(position);
        }
    }
    Ok(())
}

fn show_context_menu(hwnd: HWND, state: &PetWindowState) -> Result<()> {
    let menu = unsafe { CreatePopupMenu()? };
    unsafe {
        let start_text = wide_null(&state.start_menu_text);
        let show_main_text = wide_null(&state.show_main_menu_text);
        let settings_text = wide_null(&state.settings_menu_text);
        let about_text = wide_null(&state.about_menu_text);
        let exit_text = wide_null(&state.exit_menu_text);
        AppendMenuW(menu, MF_STRING, PET_MENU_START, PCWSTR(start_text.as_ptr()))?;
        AppendMenuW(menu, MF_STRING, PET_MENU_SHOW_MAIN, PCWSTR(show_main_text.as_ptr()))?;
        AppendMenuW(menu, MF_STRING, PET_MENU_SETTINGS, PCWSTR(settings_text.as_ptr()))?;
        AppendMenuW(menu, MF_STRING, PET_MENU_ABOUT, PCWSTR(about_text.as_ptr()))?;
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        AppendMenuW(menu, MF_STRING, PET_MENU_EXIT, PCWSTR(exit_text.as_ptr()))?;
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
        PET_MENU_SETTINGS => post_pet_command(state.owner, PET_COMMAND_SETTINGS),
        PET_MENU_ABOUT => post_pet_command(state.owner, PET_COMMAND_ABOUT),
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

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
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
            x: state.position.0,
            y: state.position.1,
        },
    )?;
    state.speech_bubble.set_position(
        state.position,
        (state.surface.width() as i32, state.surface.height() as i32),
    )?;
    Ok(())
}

fn update_speech_bubble(hwnd: HWND, state: &mut PetWindowState) -> Result<()> {
    if state.hide_animation.is_some()
        || !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
    {
        state.speech_bubble.hide();
        return Ok(());
    }

    state.speech_bubble.update(
        state.position,
        (state.surface.width() as i32, state.surface.height() as i32),
        Instant::now(),
    )
}

fn monitor_work_area(hwnd: HWND) -> Result<RECT> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    monitor_work_area_for_monitor(monitor)
}

fn monitor_work_area_at_point(point: POINT) -> Result<RECT> {
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    monitor_work_area_for_monitor(monitor)
}

fn monitor_work_area_for_monitor(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Result<RECT> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(Error::from_win32());
    }
    Ok(info.rcWork)
}

fn clamp_position(position: (i32, i32), size: (i32, i32), work_area: RECT) -> (i32, i32) {
    let min_x = work_area.left;
    let min_y = work_area.top;
    let max_x = (work_area.right - size.0).max(min_x);
    let max_y = (work_area.bottom - size.1).max(min_y);
    (position.0.clamp(min_x, max_x), position.1.clamp(min_y, max_y))
}

fn clamp_drag_position(position: (i32, i32), size: (i32, i32), work_area: RECT) -> (i32, i32) {
    let visible_width = MIN_VISIBLE_PET_SIZE.min(size.0);
    let visible_height = MIN_VISIBLE_PET_SIZE.min(size.1);
    let min_x = work_area.left - size.0 + visible_width;
    let min_y = work_area.top - size.1 + visible_height;
    let max_x = work_area.right - visible_width;
    let max_y = work_area.bottom - visible_height;
    (position.0.clamp(min_x, max_x), position.1.clamp(min_y, max_y))
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
        ActiveClip::Walk => &state.walk.frames,
        ActiveClip::Jump => &state.jump.frames,
    };
    let frame = frames.get(selection.frame.id as usize).ok_or_else(Error::from_win32)?;
    state.surface.clear();
    state
        .surface
        .draw_scaled_frame(frame)
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

fn pet_surface(frame: &PreparedFrame, dpi: u32) -> Result<PixelSurface> {
    let scale = |value: u32| -> Result<u32> {
        let scaled = (value as u64 * dpi.max(96) as u64 + 48) / 96;
        if scaled == 0 || scaled > i32::MAX as u64 {
            return Err(Error::from_win32());
        }
        Ok(scaled as u32)
    };
    PixelSurface::new(scale(frame.width)?, scale(frame.height)?).map_err(|_| Error::from_win32())
}

fn apply_pet_dpi(state: &mut PetWindowState, dpi: u32, position: (i32, i32)) -> Result<()> {
    let surface = pet_surface(&state.idle.frames[0], dpi)?;
    state.dpi = dpi.max(96);
    state.surface = surface;
    state.position = position;
    state.last_frame = None;
    // Rebase an active drag so the next mouse message does not undo the suggested position.
    if let Some(drag) = state.drag.as_mut() {
        unsafe { GetCursorPos(&mut drag.pointer_start)? };
        drag.window_start = POINT {
            x: position.0,
            y: position.1,
        };
    }
    update_frame(state)?;
    state
        .speech_bubble
        .set_position(position, (state.surface.width() as i32, state.surface.height() as i32))
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Foundation::RECT;

    use super::{
        ActiveClip, PlaybackState, clamp_drag_position, clamp_position, hide_target_y, movement_exceeded,
        should_return_to_idle,
    };

    #[test]
    fn pet_replaces_wait_cursor_with_arrow() {
        use windows::Win32::{
            Foundation::{HINSTANCE, LPARAM, WPARAM},
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GetCursor, HTCLIENT, IDC_ARROW, IDC_WAIT, LoadCursorW, SendMessageW,
                SetCursor, WM_MOUSEMOVE, WM_SETCURSOR, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
            },
        };
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            super::register_class(instance).unwrap();
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                super::PET_WINDOW_CLASS,
                windows::core::w!(""),
                WS_POPUP,
                0,
                0,
                100,
                100,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let arrow = LoadCursorW(None, IDC_ARROW).unwrap();
            let previous = SetCursor(Some(LoadCursorW(None, IDC_WAIT).unwrap()));
            SendMessageW(
                hwnd,
                WM_SETCURSOR,
                Some(WPARAM(hwnd.0 as usize)),
                Some(LPARAM(HTCLIENT as isize | ((WM_MOUSEMOVE as isize) << 16))),
            );
            let actual = GetCursor();
            SetCursor(Some(previous));
            DestroyWindow(hwnd).unwrap();
            assert_eq!(actual, arrow);
        }
    }

    #[test]
    fn finished_jump_returns_to_idle() {
        assert!(should_return_to_idle(ActiveClip::Jump, PlaybackState::Finished));
        assert!(!should_return_to_idle(ActiveClip::Jump, PlaybackState::Playing));
        assert!(!should_return_to_idle(ActiveClip::Idle, PlaybackState::Finished));
    }

    #[test]
    fn dpi_changes_resize_rendering_and_hit_testing_without_accumulating_scale() {
        use super::*;
        use crate::animation::{AnimationClip, Frame, LoopMode};
        use windows::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::SendMessageW};
        unsafe {
            let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
            register_class(instance).unwrap();
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                PET_WINDOW_CLASS,
                w!(""),
                WS_POPUP,
                100,
                100,
                3,
                2,
                None,
                None,
                Some(instance),
                None,
            )
            .unwrap();
            let pet = PetWindow { hwnd };
            let pixels = [0, 0, 0, 0, 10, 20, 30, 255, 10, 20, 30, 255].repeat(2);
            let frame = PreparedFrame {
                width: 3,
                height: 2,
                pixels,
                hitbox: None,
            };
            let clip = AnimationClip::new(vec![Frame { id: 0 }], Duration::from_millis(120), LoopMode::Loop).unwrap();
            let animation = PreparedAnimation {
                clip: clip.clone(),
                frames: vec![frame.clone()],
            };
            attach_state(
                hwnd,
                PetWindowState {
                    renderer: LayeredRenderer::new(hwnd).unwrap(),
                    surface: pet_surface(&frame, 96).unwrap(),
                    dpi: 96,
                    position: (100, 100),
                    idle: animation.clone(),
                    walk: animation.clone(),
                    jump: animation,
                    player: AnimationPlayer::new(clip),
                    active_clip: ActiveClip::Idle,
                    last_frame: None,
                    owner: HWND::default(),
                    drag: None,
                    hide_animation: None,
                    next_walk_at: Instant::now() + WALK_START_DELAY,
                    speech_bubble: SpeechBubbleController::create(instance, hwnd, Language::English).unwrap(),
                    start_menu_text: String::new(),
                    show_main_menu_text: String::new(),
                    settings_menu_text: String::new(),
                    about_menu_text: String::new(),
                    exit_menu_text: String::new(),
                },
            );
            for (dpi, width, height) in [
                (96, 3, 2),
                (120, 4, 3),
                (144, 5, 3),
                (192, 6, 4),
                (144, 5, 3),
                (96, 3, 2),
            ] {
                let suggested = RECT {
                    left: 100,
                    top: 100,
                    right: 100 + width,
                    bottom: 100 + height,
                };
                SendMessageW(
                    hwnd,
                    WM_DPICHANGED,
                    Some(WPARAM(dpi | (dpi << 16))),
                    Some(LPARAM((&suggested as *const RECT) as isize)),
                );
                let state = state_mut(hwnd).unwrap();
                assert_eq!(state.dpi, dpi as u32);
                assert_eq!(
                    (state.surface.width(), state.surface.height()),
                    (width as u32, height as u32)
                );
                let mut actual = RECT::default();
                GetWindowRect(hwnd, &mut actual).unwrap();
                assert_eq!(actual, suggested);
                let point = |x: i32, y: i32| LPARAM(((y as u16 as u32) << 16 | x as u16 as u32) as isize);
                assert!(!hit_test_current_frame(hwnd, state, point(100, 100)));
                assert!(hit_test_current_frame(
                    hwnd,
                    state,
                    point(100 + width - 1, 100 + height - 1)
                ));
                assert!(!hit_test_current_frame(hwnd, state, point(100 + width, 100)));
            }
            drop(pet);
        }
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

    #[test]
    fn clamps_pet_position_to_the_monitor_work_area() {
        let work_area = RECT {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        assert_eq!(clamp_position((-20, -30), (100, 100), work_area), (0, 0));
        assert_eq!(clamp_position((950, 750), (100, 100), work_area), (900, 700));
    }

    #[test]
    fn allows_pet_to_be_partially_clipped_while_dragging() {
        let work_area = RECT {
            left: 0,
            top: 0,
            right: 1_000,
            bottom: 800,
        };

        assert_eq!(clamp_drag_position((-200, -200), (100, 100), work_area), (-76, -76));
        assert_eq!(clamp_drag_position((1_000, 800), (100, 100), work_area), (976, 776));
    }
}
