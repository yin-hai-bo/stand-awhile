use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
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
            HTTRANSPARENT, HWND_TOPMOST, IDC_ARROW, IsWindowVisible, KillTimer, LoadCursorW, MF_SEPARATOR, MF_STRING,
            RegisterClassExW, SPI_GETWORKAREA, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
            SWP_NOSIZE, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, TPM_LEFTALIGN,
            TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_APP, WM_CAPTURECHANGED, WM_DPICHANGED, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_RBUTTONUP, WM_SHOWWINDOW, WM_TIMER,
            WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::{
    animation::{AnimationPlayer, PlaybackState},
    app_state::{PetPosition, save_pet_position},
    asset::{CharacterAnimations, PreparedAnimation, PreparedFrame},
    i18n::Language,
    render::{CachedSurface, LayeredRenderer, PixelSurface, SurfacePoint},
    speech_bubble_window::SpeechBubbleController,
};

const PET_WINDOW_CLASS: PCWSTR = w!("YHB-StandAwhilePetWindow");
const PET_MARGIN: i32 = 24;
const PET_TIMER_ID: usize = 1;
const PET_TIMER_INTERVAL_MS: u32 = 16;
const CLICK_DRAG_THRESHOLD: i32 = 4;
const MIN_VISIBLE_PET_SIZE: i32 = 24;
const SPEECH_BUBBLE_INITIAL_DELAY: Duration = Duration::from_millis(500);

pub const WM_PET_COMMAND: u32 = WM_APP + 2;
pub const PET_COMMAND_ACKNOWLEDGE: usize = 1;
pub const PET_COMMAND_SHOW_MAIN: usize = 2;
pub const PET_COMMAND_EXIT: usize = 3;
pub const PET_COMMAND_START: usize = 4;
pub const PET_COMMAND_SETTINGS: usize = 5;
pub const PET_COMMAND_ABOUT: usize = 6;
pub const PET_COMMAND_HIDDEN: usize = 7;
const PET_MENU_START: usize = 1;
const PET_MENU_SHOW_MAIN: usize = 2;
const PET_MENU_EXIT: usize = 3;
const PET_MENU_SETTINGS: usize = 4;
const PET_MENU_ABOUT: usize = 5;
const HIDE_ANIMATION_DURATION_MS: u128 = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ActiveClip {
    Idle,
    Walk,
    Jump,
}

struct PetWindowState {
    renderer: Rc<RefCell<LayeredRenderer>>,
    surface: Rc<PixelSurface>,
    scaled_frames: HashMap<(ActiveClip, u32), CachedSurface>,
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
    show_animation: Option<ShowAnimation>,
    speech_bubble: Rc<RefCell<SpeechBubbleController>>,
    language: Language,
    bubble_start: Option<Instant>,
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

struct ShowAnimation {
    started_at: Instant,
    from: (i32, i32),
    target: (i32, i32),
}

pub struct PetWindow {
    hwnd: HWND,
}

impl PetWindow {
    pub(crate) fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub(crate) fn position(&self) -> Result<PetPosition> {
        let position = with_state(self.hwnd, |state| {
            state
                .show_animation
                .as_ref()
                .map(|animation| animation.target)
                .or_else(|| {
                    state
                        .hide_animation
                        .as_ref()
                        .map(|animation| animation.restore_position)
                })
                .unwrap_or(state.position)
        })
        .ok_or_else(Error::from_win32)?;
        persistent_position(self.hwnd, position)
    }

    #[cfg(test)]
    pub(crate) fn finish_hide_for_test(&self) {
        with_state(self.hwnd, |state| {
            state.hide_animation.as_mut().unwrap().started_at =
                Instant::now() - Duration::from_millis(HIDE_ANIMATION_DURATION_MS as u64);
        })
        .unwrap();
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::SendMessageW(
                self.hwnd,
                WM_TIMER,
                Some(WPARAM(PET_TIMER_ID)),
                None,
            );
        }
    }

    #[cfg(test)]
    pub(crate) fn test_hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn create(
        instance: HINSTANCE,
        owner: HWND,
        animations: CharacterAnimations,
        language: Language,
        saved_position: Option<PetPosition>,
        settings_menu_text: &str,
    ) -> Result<Self> {
        register_class(instance)?;

        let frame = animations.walk.frames.first().ok_or_else(Error::from_win32)?;
        let width = i32::try_from(frame.width).map_err(|_| Error::from_win32())?;
        let height = i32::try_from(frame.height).map_err(|_| Error::from_win32())?;
        let position = saved_position
            .map(|saved| restore_position(&saved, (width, height)))
            .transpose()?
            .unwrap_or(pet_position(width, height)?);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
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
        // Own the HWND immediately so every later initialization error destroys it.
        let pet = Self { hwnd };

        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let mut surface = pet_surface(frame, dpi)?;
        surface.draw_scaled_frame(frame).map_err(|_| Error::from_win32())?;
        let position = clamp_position(
            position,
            (surface.width() as i32, surface.height() as i32),
            monitor_work_area(hwnd)?,
        );

        let renderer = LayeredRenderer::new(hwnd)?;
        let speech_bubble = SpeechBubbleController::create(instance, hwnd, language)?;
        let now = Instant::now();
        let player = AnimationPlayer::new(animations.walk.clip.clone());
        attach_state(
            hwnd,
            PetWindowState {
                renderer: Rc::new(RefCell::new(renderer)),
                surface: Rc::new(surface),
                scaled_frames: HashMap::new(),
                dpi,
                position,
                idle: animations.idle,
                walk: animations.walk,
                jump: animations.jump,
                player,
                active_clip: ActiveClip::Walk,
                last_frame: None,
                owner,
                drag: None,
                hide_animation: None,
                show_animation: None,
                speech_bubble: Rc::new(RefCell::new(speech_bubble)),
                language,
                bubble_start: None,
                start_menu_text: crate::tray_menu_start_text(Language::English).to_owned(),
                show_main_menu_text: crate::tray_menu_show_text(Language::English).to_owned(),
                settings_menu_text: settings_menu_text.to_owned(),
                about_menu_text: crate::tray_menu_about_text(Language::English).to_owned(),
                exit_menu_text: crate::tray_menu_exit_text(Language::English).to_owned(),
            },
        );

        with_state(pet.hwnd, |state| state.player.play(now)).ok_or_else(Error::from_win32)?;
        update_frame(pet.hwnd)?;
        Ok(pet)
    }

    pub fn hide(&self) {
        with_state(self.hwnd, |state| {
            cancel_show_animation(state);
            cancel_hide_animation(state);
            sync_bubble_animation(state, false, Instant::now());
        });
        hide_bubble(self.hwnd);
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn hide_animated(&self) -> Result<()> {
        if !unsafe { IsWindowVisible(self.hwnd).as_bool() } {
            return Ok(());
        }
        begin_hide_animation(self.hwnd)
    }

    pub fn show_reminder(&self) -> Result<()> {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        begin_show_animation(self.hwnd, Instant::now())?;
        with_state(self.hwnd, |_| ()).ok_or_else(Error::from_win32)?;
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(())
    }

    pub fn set_language(&self, language: Language) {
        let bubble = with_state(self.hwnd, |state| {
            state.language = language;
            state.start_menu_text = crate::tray_menu_start_text(language).to_owned();
            state.show_main_menu_text = crate::tray_menu_show_text(language).to_owned();
            state.settings_menu_text = crate::tray_menu_settings_text(language).to_owned();
            state.about_menu_text = crate::tray_menu_about_text(language).to_owned();
            state.exit_menu_text = crate::tray_menu_exit_text(language).to_owned();
            state.speech_bubble.clone()
        });
        if let Some(bubble) = bubble {
            if let Ok(mut bubble) = bubble.try_borrow_mut() {
                let _ = bubble.set_language(language);
            }
        }
    }

    pub fn set_animations(&self, animations: CharacterAnimations) -> Result<()> {
        let (dpi, clip, bubble) = with_state(self.hwnd, |state| {
            (state.dpi, state.active_clip, state.speech_bubble.clone())
        })
        .ok_or_else(Error::from_win32)?;
        let frame = animations.idle.frames.first().ok_or_else(Error::from_win32)?;
        let surface = Rc::new(pet_surface(frame, dpi)?);
        let renderer = Rc::new(RefCell::new(LayeredRenderer::new(self.hwnd)?));
        let active_clip = if clip == ActiveClip::Jump {
            ActiveClip::Jump
        } else if bubble.try_borrow().map_err(|_| Error::from_win32())?.is_visible() {
            ActiveClip::Idle
        } else {
            ActiveClip::Walk
        };
        let mut player = AnimationPlayer::new(match active_clip {
            ActiveClip::Idle => animations.idle.clip.clone(),
            ActiveClip::Walk => animations.walk.clip.clone(),
            ActiveClip::Jump => animations.jump.clip.clone(),
        });
        player.play(Instant::now());
        let (previous, position) = with_state(self.hwnd, |state| {
            let previous = (
                std::mem::replace(&mut state.idle, animations.idle),
                std::mem::replace(&mut state.walk, animations.walk),
                std::mem::replace(&mut state.jump, animations.jump),
                std::mem::replace(&mut state.surface, surface),
                std::mem::replace(&mut state.renderer, renderer.clone()),
                std::mem::take(&mut state.scaled_frames),
                std::mem::replace(&mut state.player, player),
                state.active_clip,
                state.last_frame,
            );
            state.active_clip = active_clip;
            state.last_frame = None;
            (previous, state.position)
        })
        .ok_or_else(Error::from_win32)?;
        let result = set_position(self.hwnd, position).and_then(|_| update_frame(self.hwnd));
        if result.is_err() {
            // A callback may have destroyed the Pet or installed another replacement.
            let restored = with_state(self.hwnd, |state| {
                if !Rc::ptr_eq(&state.renderer, &renderer) {
                    return false;
                }
                (
                    state.idle,
                    state.walk,
                    state.jump,
                    state.surface,
                    state.renderer,
                    state.scaled_frames,
                    state.player,
                    state.active_clip,
                    state.last_frame,
                ) = previous;
                true
            })
            .unwrap_or(false);
            if restored {
                let _ = set_position(self.hwnd, position);
            }
        }
        result
    }
}

fn hide_bubble(hwnd: HWND) {
    if let Some(bubble) = with_state(hwnd, |state| {
        state.bubble_start = None;
        state.speech_bubble.clone()
    }) {
        if let Ok(bubble) = bubble.try_borrow() {
            bubble.hide();
        }
    }
}

fn start_bubble(hwnd: HWND, now: Instant) {
    let bubble = with_state(hwnd, |state| {
        state.bubble_start = Some(now);
        state.speech_bubble.clone()
    });
    if let Some(bubble) = bubble {
        if let Ok(mut bubble) = bubble.try_borrow_mut() {
            if let Some(Some(now)) = with_state(hwnd, |state| state.bubble_start.take()) {
                bubble.start(now, SPEECH_BUBBLE_INITIAL_DELAY);
            }
        }
    }
}

fn begin_hide_animation(hwnd: HWND) -> Result<()> {
    let work_area = monitor_work_area(hwnd)?;
    with_state(hwnd, |state| {
        cancel_show_animation(state);
        sync_bubble_animation(state, false, Instant::now());
        state.hide_animation = Some(HideAnimation {
            started_at: Instant::now(),
            from: state.position,
            restore_position: state.position,
            target_y: hide_target_y(state.position.1, state.surface.height() as i32, work_area),
        });
    })
    .ok_or_else(Error::from_win32)?;
    hide_bubble(hwnd);
    Ok(())
}

fn begin_show_animation(hwnd: HWND, now: Instant) -> Result<()> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let info = monitor_info_ex(monitor)?;
    with_state(hwnd, |state| {
        cancel_show_animation(state);
        cancel_hide_animation(state);
        let size = (state.surface.width() as i32, state.surface.height() as i32);
        let target = clamp_position(state.position, size, info.monitorInfo.rcWork);
        let from = (target.0, hide_target_y(target.1, size.1, info.monitorInfo.rcMonitor));
        state.show_animation = Some(ShowAnimation {
            started_at: now,
            from,
            target,
        });
        state.position = from;
        start_clip(state, ActiveClip::Jump, now);
    })
    .ok_or_else(Error::from_win32)?;
    hide_bubble(hwnd);
    update_frame_at(hwnd, now)
}

fn update_show_animation(hwnd: HWND, now: Instant) -> Result<()> {
    let position = with_state(hwnd, |state| {
        let animation = state.show_animation.as_ref()?;
        let elapsed = now.saturating_duration_since(animation.started_at).as_millis();
        let progress = elapsed.min(HIDE_ANIMATION_DURATION_MS);
        let target = animation.target;
        let y = animation.from.1 + (target.1 - animation.from.1) * progress as i32 / HIDE_ANIMATION_DURATION_MS as i32;
        if elapsed >= HIDE_ANIMATION_DURATION_MS {
            state.show_animation = None;
        }
        Some((target.0, y))
    })
    .flatten();
    if let Some(position) = position {
        set_position(hwnd, position)?;
    }
    Ok(())
}

fn cancel_show_animation(state: &mut PetWindowState) {
    if let Some(animation) = state.show_animation.take() {
        state.position = animation.target;
    }
    if state.active_clip == ActiveClip::Jump {
        start_clip(state, ActiveClip::Walk, Instant::now());
    }
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

// The closure may only access local data; callbacks run after it returns.
fn with_state<R>(hwnd: HWND, update: impl FnOnce(&mut PetWindowState) -> R) -> Option<R> {
    let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut PetWindowState;
    unsafe { raw.as_mut().map(update) }
}

#[cfg(test)]
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
        WM_SHOWWINDOW => {
            // The animation timer belongs to the visible pet, not the countdown.
            unsafe {
                if wparam.0 != 0 {
                    let _ = SetTimer(Some(hwnd), PET_TIMER_ID, PET_TIMER_INTERVAL_MS, None);
                } else {
                    let _ = KillTimer(Some(hwnd), PET_TIMER_ID);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_TIMER if wparam.0 == PET_TIMER_ID => {
            // KillTimer leaves already queued messages in the queue.
            if !unsafe { IsWindowVisible(hwnd).as_bool() } {
                return LRESULT(0);
            }
            let _ = update_hide_animation(hwnd);
            if !unsafe { IsWindowVisible(hwnd).as_bool() } {
                return LRESULT(0);
            }
            let _ = update_show_animation(hwnd, Instant::now());
            let _ = update_speech_bubble(hwnd);
            let _ = update_frame(hwnd);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let rect = unsafe { *(lparam.0 as *const RECT) };
            let _ = apply_pet_dpi(hwnd, (wparam.0 & 0xffff) as u32, (rect.left, rect.top));
            LRESULT(0)
        }
        WM_NCHITTEST => {
            if with_state(hwnd, |state| !hit_test_current_frame(hwnd, state, lparam)).unwrap_or(false) {
                return LRESULT(HTTRANSPARENT as isize);
            }
            LRESULT(HTCLIENT as isize)
        }
        WM_LBUTTONDOWN => {
            let _ = begin_drag(hwnd);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let _ = move_drag(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = end_drag(hwnd);
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            with_state(hwnd, |state| state.drag = None);
            LRESULT(0)
        }

        WM_RBUTTONUP => {
            let _ = show_context_menu(hwnd);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            unsafe {
                let _ = KillTimer(Some(hwnd), PET_TIMER_ID);
            }
            let raw = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
            if raw != 0 {
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                drop(unsafe { Box::from_raw(raw as *mut PetWindowState) });
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn sync_bubble_animation(state: &mut PetWindowState, bubble_visible: bool, now: Instant) {
    if state.active_clip == ActiveClip::Jump {
        return;
    }
    let clip = if bubble_visible {
        ActiveClip::Idle
    } else {
        ActiveClip::Walk
    };
    if state.active_clip == clip {
        return;
    }
    start_clip(state, clip, now);
}

fn start_clip(state: &mut PetWindowState, clip: ActiveClip, now: Instant) {
    state.active_clip = clip;
    state.player = AnimationPlayer::new(match clip {
        ActiveClip::Idle => state.idle.clip.clone(),
        ActiveClip::Walk => state.walk.clip.clone(),
        ActiveClip::Jump => state.jump.clip.clone(),
    });
    state.player.play(now);
    state.last_frame = None;
}
fn begin_drag(hwnd: HWND) -> Result<()> {
    let mut pointer = POINT::default();
    unsafe {
        GetCursorPos(&mut pointer)?;
    }
    let mut window = RECT::default();
    unsafe {
        GetWindowRect(hwnd, &mut window)?;
    }
    let start = with_state(hwnd, |state| {
        state.show_animation = None;
        let start = if state.active_clip == ActiveClip::Jump {
            start_clip(state, ActiveClip::Walk, Instant::now());
            true
        } else {
            false
        };
        state.drag = Some(DragState {
            pointer_start: pointer,
            window_start: POINT {
                x: window.left,
                y: window.top,
            },
            moved: false,
        });
        start
    })
    .ok_or_else(Error::from_win32)?;
    if start {
        start_bubble(hwnd, Instant::now());
    }
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

fn move_drag(hwnd: HWND) -> Result<()> {
    let mut pointer = POINT::default();
    unsafe {
        GetCursorPos(&mut pointer)?;
    }
    let work_area = monitor_work_area_at_point(pointer)?;
    let position = with_state(hwnd, |state| {
        let drag = state.drag.as_mut()?;
        drag.moved = drag.moved || movement_exceeded(drag.pointer_start, pointer);
        Some(clamp_drag_position(
            (
                drag.window_start.x + pointer.x - drag.pointer_start.x,
                drag.window_start.y + pointer.y - drag.pointer_start.y,
            ),
            (state.surface.width() as i32, state.surface.height() as i32),
            work_area,
        ))
    })
    .flatten();
    if let Some(position) = position {
        set_position(hwnd, position)?;
    }
    Ok(())
}

fn end_drag(hwnd: HWND) -> Result<()> {
    let Some(Some((moved, owner, position))) = with_state(hwnd, |state| {
        state.drag.take().map(|drag| (drag.moved, state.owner, state.position))
    }) else {
        return Ok(());
    };
    unsafe {
        let _ = ReleaseCapture();
    }
    if !moved {
        // Capture loss can synchronously destroy the Pet. Do not acknowledge a stale click.
        if with_state(hwnd, |_| ()).is_some() {
            let _ = begin_hide_animation(hwnd);
            if with_state(hwnd, |_| ()).is_some() {
                post_pet_command(owner, PET_COMMAND_ACKNOWLEDGE);
            }
        }
    } else if let Ok(position) = persistent_position(hwnd, position) {
        let _ = save_pet_position(position);
    }
    Ok(())
}

fn show_context_menu(hwnd: HWND) -> Result<()> {
    show_context_menu_with(hwnd, |menu, point| unsafe {
        TrackPopupMenuEx(
            menu,
            (TPM_LEFTALIGN | TPM_RIGHTBUTTON | TPM_RETURNCMD).0,
            point.x,
            point.y,
            hwnd,
            None,
        )
        .0 as usize
    })
}

fn show_context_menu_with(
    hwnd: HWND,
    track: impl FnOnce(windows::Win32::UI::WindowsAndMessaging::HMENU, POINT) -> usize,
) -> Result<()> {
    // Do not hold a window-state reference across the menu's nested message loop.
    let (owner, start_text, show_main_text, settings_text, about_text, exit_text) = {
        with_state(hwnd, |state| {
            (
                state.owner,
                wide_null(&state.start_menu_text),
                wide_null(&state.show_main_menu_text),
                wide_null(&state.settings_menu_text),
                wide_null(&state.about_menu_text),
                wide_null(&state.exit_menu_text),
            )
        })
        .ok_or_else(Error::from_win32)?
    };
    let menu = unsafe { CreatePopupMenu()? };
    let result = (|| -> Result<usize> {
        unsafe {
            AppendMenuW(menu, MF_STRING, PET_MENU_START, PCWSTR(start_text.as_ptr()))?;
            AppendMenuW(menu, MF_STRING, PET_MENU_SHOW_MAIN, PCWSTR(show_main_text.as_ptr()))?;
            AppendMenuW(menu, MF_STRING, PET_MENU_SETTINGS, PCWSTR(settings_text.as_ptr()))?;
            AppendMenuW(menu, MF_STRING, PET_MENU_ABOUT, PCWSTR(about_text.as_ptr()))?;
            AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
            AppendMenuW(menu, MF_STRING, PET_MENU_EXIT, PCWSTR(exit_text.as_ptr()))?;
        }

        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point)?;
        }
        Ok(track(menu, point))
    })();
    unsafe {
        let _ = DestroyMenu(menu);
    }
    let command = result?;

    if with_state(hwnd, |_| ()).is_none() {
        return Ok(());
    }
    match command {
        PET_MENU_START => post_pet_command(owner, PET_COMMAND_START),
        PET_MENU_SHOW_MAIN => post_pet_command(owner, PET_COMMAND_SHOW_MAIN),
        PET_MENU_SETTINGS => post_pet_command(owner, PET_COMMAND_SETTINGS),
        PET_MENU_ABOUT => post_pet_command(owner, PET_COMMAND_ABOUT),
        PET_MENU_EXIT => post_pet_command(owner, PET_COMMAND_EXIT),
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

fn update_hide_animation(hwnd: HWND) -> Result<()> {
    let transition = with_state(hwnd, |state| {
        let animation = state.hide_animation.as_ref()?;
        let elapsed = animation.started_at.elapsed().as_millis();
        let progress = elapsed.min(HIDE_ANIMATION_DURATION_MS);
        let y = animation.from.1
            + (animation.target_y - animation.from.1) * progress as i32 / HIDE_ANIMATION_DURATION_MS as i32;
        Some((
            (animation.from.0, y),
            animation.restore_position,
            elapsed >= HIDE_ANIMATION_DURATION_MS,
            state.owner,
            animation.started_at,
        ))
    })
    .flatten();
    let Some((position, restore_position, finished, owner, started_at)) = transition else {
        return Ok(());
    };
    set_position(hwnd, position)?;
    if finished {
        let active = with_state(hwnd, |state| {
            if state.hide_animation.as_ref().map(|animation| animation.started_at) != Some(started_at) {
                return false;
            }
            state.hide_animation = None;
            state.position = restore_position;
            true
        })
        .unwrap_or(false);
        if active {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
                // Release through the owner after this callback returns.
                let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(owner),
                    WM_PET_COMMAND,
                    WPARAM(PET_COMMAND_HIDDEN),
                    LPARAM(hwnd.0 as isize),
                );
            }
        }
    }
    Ok(())
}

fn cancel_hide_animation(state: &mut PetWindowState) {
    if let Some(animation) = state.hide_animation.take() {
        state.position = animation.restore_position;
    }
}

fn set_position(hwnd: HWND, position: (i32, i32)) -> Result<()> {
    with_state(hwnd, |state| state.position = position).ok_or_else(Error::from_win32)?;
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
    submit_frame(hwnd)?;
    position_bubble(hwnd)
}

fn position_bubble(hwnd: HWND) -> Result<()> {
    let (bubble, position, size) = with_state(hwnd, |state| {
        (
            state.speech_bubble.clone(),
            state.position,
            (state.surface.width() as i32, state.surface.height() as i32),
        )
    })
    .ok_or_else(Error::from_win32)?;
    let result = if let Ok(bubble) = bubble.try_borrow() {
        bubble.set_position(position, size)
    } else {
        Ok(())
    };
    result
}

fn update_speech_bubble(hwnd: HWND) -> Result<()> {
    let Some((bubble, hidden, position, size)) = with_state(hwnd, |state| {
        (
            state.speech_bubble.clone(),
            state.hide_animation.is_some() || state.active_clip == ActiveClip::Jump,
            state.position,
            (state.surface.width() as i32, state.surface.height() as i32),
        )
    }) else {
        return Ok(());
    };
    let now = Instant::now();
    let visible = {
        let Ok(mut bubble) = bubble.try_borrow_mut() else {
            return Ok(());
        };
        let Some((language, start)) = with_state(hwnd, |state| (state.language, state.bubble_start.take())) else {
            return Ok(());
        };
        if let Some(start) = start {
            bubble.start(start, SPEECH_BUBBLE_INITIAL_DELAY);
        }
        bubble.set_language(language)?;
        if hidden || !unsafe { IsWindowVisible(hwnd).as_bool() } {
            bubble.hide();
            false
        } else {
            bubble.update(position, size, now)?
        }
    };
    with_state(hwnd, |state| sync_bubble_animation(state, visible, now));
    Ok(())
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

fn update_frame(hwnd: HWND) -> Result<()> {
    update_frame_at(hwnd, Instant::now())
}

fn update_frame_at(hwnd: HWND, now: Instant) -> Result<()> {
    let Some(renderer) = with_state(hwnd, |state| state.renderer.clone()) else {
        return Err(Error::from_win32());
    };
    // A nested timer must not borrow the renderer already submitting this Pet.
    if renderer.try_borrow_mut().is_err() {
        return Ok(());
    }
    let (changed, should_start_bubble) =
        with_state(hwnd, |state| prepare_frame_at(state, now)).ok_or_else(Error::from_win32)??;
    if should_start_bubble {
        start_bubble(hwnd, now);
    }
    if changed {
        submit_frame(hwnd)?;
    }
    Ok(())
}

fn prepare_frame_at(state: &mut PetWindowState, now: Instant) -> Result<(bool, bool)> {
    let mut selection = state.player.update(now);
    let mut start_bubble = false;
    if state.active_clip == ActiveClip::Jump
        && state.show_animation.is_none()
        && selection.state == PlaybackState::Finished
    {
        start_clip(state, ActiveClip::Walk, now);
        start_bubble = true;
        selection = state.player.update(now);
    }
    let frame_key = (state.active_clip, selection.frame.id);
    if state.last_frame == Some(frame_key) {
        return Ok((false, start_bubble));
    }
    let frames = match state.active_clip {
        ActiveClip::Idle => &state.idle.frames,
        ActiveClip::Walk => &state.walk.frames,
        ActiveClip::Jump => &state.jump.frames,
    };
    let frame = frames.get(selection.frame.id as usize).ok_or_else(Error::from_win32)?;
    let surface = Rc::make_mut(&mut state.surface);
    if let Some(cached) = state.scaled_frames.get(&frame_key) {
        cached.restore(surface).map_err(|_| Error::from_win32())?;
    } else {
        surface.clear();
        surface.draw_scaled_frame(frame).map_err(|_| Error::from_win32())?;
        // Retain resampled looping frames, but not the one-shot Jump.
        if state.active_clip != ActiveClip::Jump && (surface.width(), surface.height()) != (frame.width, frame.height) {
            state.scaled_frames.insert(frame_key, CachedSurface::new(surface));
        }
    }
    state.last_frame = Some(frame_key);
    Ok((true, start_bubble))
}

fn submit_frame(hwnd: HWND) -> Result<()> {
    let (renderer, surface, position) = with_state(hwnd, |state| {
        (state.renderer.clone(), state.surface.clone(), state.position)
    })
    .ok_or_else(Error::from_win32)?;
    let Ok(mut renderer) = renderer.try_borrow_mut() else {
        with_state(hwnd, |state| state.last_frame = None);
        return Ok(());
    };
    let result = renderer.submit(
        &surface,
        SurfacePoint {
            x: position.0,
            y: position.1,
        },
    );
    if result.is_err() {
        with_state(hwnd, |state| state.last_frame = None);
    }
    result
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

fn apply_pet_dpi(hwnd: HWND, dpi: u32, position: (i32, i32)) -> Result<()> {
    let mut pointer = POINT::default();
    unsafe {
        GetCursorPos(&mut pointer)?;
    }
    with_state(hwnd, |state| -> Result<()> {
        state.surface = Rc::new(pet_surface(&state.idle.frames[0], dpi)?);
        state.dpi = dpi.max(96);
        state.scaled_frames.clear();
        state.position = position;
        state.last_frame = None;
        if let Some(drag) = state.drag.as_mut() {
            drag.pointer_start = pointer;
            drag.window_start = POINT {
                x: position.0,
                y: position.1,
            };
        }
        Ok(())
    })
    .ok_or_else(Error::from_win32)??;
    update_frame(hwnd)?;
    position_bubble(hwnd)
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Foundation::RECT;

    use super::{clamp_drag_position, clamp_position, hide_target_y, movement_exceeded};

    #[test]
    fn position_callback_can_destroy_pet_and_release_attached_resources() {
        use super::*;
        use std::cell::Cell;
        use windows::Win32::UI::{
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::{IsWindow, WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING},
        };
        struct Callback {
            renderer: std::rc::Weak<RefCell<LayeredRenderer>>,
            renderer_alive: Cell<bool>,
            pointer_cleared: Cell<bool>,
        }
        unsafe extern "system" fn callback(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
            _: usize,
            data: usize,
        ) -> LRESULT {
            if matches!(msg, WM_WINDOWPOSCHANGING | WM_WINDOWPOSCHANGED) {
                let data = unsafe { &*(data as *const Callback) };
                if let Some(renderer) = data.renderer.upgrade() {
                    data.renderer_alive.set(renderer.try_borrow_mut().is_ok());
                }
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                data.pointer_cleared
                    .set(unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } == 0);
                return LRESULT(0);
            }
            unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
        }
        let pet = hidden_test_pet();
        let renderer = with_state(pet.hwnd, |state| Rc::downgrade(&state.renderer)).unwrap();
        let data = Callback {
            renderer,
            renderer_alive: Cell::new(false),
            pointer_cleared: Cell::new(false),
        };
        assert!(unsafe { SetWindowSubclass(pet.hwnd, Some(callback), 1, &data as *const _ as usize).as_bool() });
        assert!(set_position(pet.hwnd, (120, 120)).is_err());
        assert!(data.renderer_alive.get());
        assert!(data.pointer_cleared.get());
        assert!(data.renderer.upgrade().is_none());
        assert!(!unsafe { IsWindow(Some(pet.hwnd)).as_bool() });
    }

    #[test]
    fn nested_timer_defers_rendering_until_renderer_is_available() {
        use super::*;
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
        let pet = hidden_test_pet();
        pet.show_reminder().unwrap();
        let renderer = with_state(pet.hwnd, |state| {
            state.last_frame = None;
            state.renderer.clone()
        })
        .unwrap();
        let submitting = renderer.borrow_mut();
        unsafe {
            SendMessageW(pet.hwnd, WM_TIMER, Some(WPARAM(PET_TIMER_ID)), None);
        }
        assert!(with_state(pet.hwnd, |state| state.last_frame.is_none()).unwrap());
        drop(submitting);
        unsafe {
            SendMessageW(pet.hwnd, WM_TIMER, Some(WPARAM(PET_TIMER_ID)), None);
        }
        assert!(with_state(pet.hwnd, |state| state.last_frame.is_some()).unwrap());
        pet.hide();
    }

    #[test]
    fn busy_bubble_retains_start_and_language_changes_for_next_update() {
        use super::*;
        let pet = hidden_test_pet();
        pet.show_reminder().unwrap();
        let bubble = with_state(pet.hwnd, |state| state.speech_bubble.clone()).unwrap();
        let updating = bubble.borrow_mut();
        let now = Instant::now();
        start_bubble(pet.hwnd, now);
        pet.set_language(Language::Chinese);
        update_speech_bubble(pet.hwnd).unwrap();
        assert_eq!(
            with_state(pet.hwnd, |state| (state.bubble_start, state.language)).unwrap(),
            (Some(now), Language::Chinese)
        );
        drop(updating);
        update_speech_bubble(pet.hwnd).unwrap();
        assert!(with_state(pet.hwnd, |state| state.bubble_start.is_none()).unwrap());
        pet.hide();
    }

    #[test]
    fn release_capture_can_destroy_pet_without_posting_stale_acknowledgement() {
        use super::*;
        use windows::Win32::UI::{
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::{MSG, PM_REMOVE, PeekMessageW},
        };
        unsafe extern "system" fn callback(
            hwnd: HWND,
            msg: u32,
            wp: WPARAM,
            lp: LPARAM,
            _: usize,
            _: usize,
        ) -> LRESULT {
            if msg == WM_CAPTURECHANGED {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return LRESULT(0);
            }
            unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
        }
        let pet = hidden_test_pet();
        begin_drag(pet.hwnd).unwrap();
        assert!(unsafe { SetWindowSubclass(pet.hwnd, Some(callback), 1, 0).as_bool() });
        end_drag(pet.hwnd).unwrap();
        assert!(with_state(pet.hwnd, |_| ()).is_none());
        let mut message = MSG::default();
        assert!(!unsafe { PeekMessageW(&mut message, None, WM_PET_COMMAND, WM_PET_COMMAND, PM_REMOVE).as_bool() });
    }

    #[test]
    fn menu_discards_command_when_pet_is_destroyed_during_tracking() {
        use super::*;
        use windows::Win32::UI::WindowsAndMessaging::{IsMenu, MSG, PM_REMOVE, PeekMessageW};
        let pet = hidden_test_pet();
        let mut menu_handle = Default::default();
        show_context_menu_with(pet.hwnd, |menu, _| {
            menu_handle = menu;
            // Model messages dispatched by TrackPopupMenuEx's nested message loop.
            unsafe {
                DestroyWindow(pet.hwnd).unwrap();
            }
            PET_MENU_START
        })
        .unwrap();
        assert!(!unsafe { IsMenu(menu_handle).as_bool() });
        let mut message = MSG::default();
        assert!(!unsafe { PeekMessageW(&mut message, None, WM_PET_COMMAND, WM_PET_COMMAND, PM_REMOVE).as_bool() });
    }

    #[test]
    fn failed_animation_replacement_restores_frames_buffers_and_playback() {
        use super::*;
        let pet = hidden_test_pet();
        let state = state_mut(pet.hwnd).unwrap();
        apply_pet_dpi(pet.hwnd, 144, state.position).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        state.player.play(Instant::now());
        update_frame(pet.hwnd).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        let original = CharacterAnimations {
            idle: state.idle.clone(),
            walk: state.walk.clone(),
            jump: state.jump.clone(),
        };
        let pixels = state.surface.pixels().to_vec();
        let last_frame = state.last_frame;
        let dimensions = (state.surface.width(), state.surface.height());
        let cached_bytes: usize = state.scaled_frames.values().map(CachedSurface::byte_len).sum();
        assert!(cached_bytes > 0);
        let started_at = Instant::now();
        for failure in 0..3 {
            let mut replacement = original.clone();
            match failure {
                0 => replacement.idle.frames.clear(),
                1 => replacement.idle.frames[0].width = 0,
                _ => {
                    replacement.idle.frames[0].width *= 2;
                    replacement.walk.frames[0].pixels.clear();
                }
            }
            assert!(pet.set_animations(replacement).is_err());
            let state = state_mut(pet.hwnd).unwrap();
            assert_eq!(state.idle, original.idle);
            assert_eq!(state.walk, original.walk);
            assert_eq!(state.jump, original.jump);
            assert_eq!(state.surface.pixels(), pixels);
            assert_eq!((state.surface.width(), state.surface.height()), dimensions);
            assert_eq!(
                state.scaled_frames.values().map(CachedSurface::byte_len).sum::<usize>(),
                cached_bytes
            );
            assert_eq!(state.last_frame, last_frame);
            assert_eq!(state.player.state(), PlaybackState::Playing);
            state.last_frame = None;
            update_frame_at(pet.hwnd, started_at + Duration::from_millis(240)).unwrap();
        }
        pet.show_reminder().unwrap();
        assert!(unsafe { IsWindowVisible(pet.hwnd).as_bool() });
    }

    #[test]
    fn failed_pet_initialization_destroys_window_before_and_after_state_attachment() {
        use super::*;
        use crate::animation::Frame;
        use windows::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::FindWindowW};
        let template_pet = hidden_test_pet();
        let state = state_mut(template_pet.hwnd).unwrap();
        let original = CharacterAnimations {
            idle: state.idle.clone(),
            walk: state.walk.clone(),
            jump: state.jump.clone(),
        };
        let instance = unsafe { GetModuleHandleW(None).unwrap().into() };
        for failure in 0..3 {
            let mut animations = original.clone();
            match failure {
                0 => animations.walk.frames[0].width = 0,
                1 => animations.walk.frames[0].pixels.clear(),
                _ => {
                    animations.walk.clip = crate::animation::AnimationClip::new(
                        vec![Frame { id: 99 }],
                        Duration::from_millis(120),
                        crate::animation::LoopMode::Loop,
                    )
                    .unwrap()
                }
            }
            assert!(unsafe { FindWindowW(PET_WINDOW_CLASS, w!("Stand Awhile Pet")) }.is_err());
            assert!(
                PetWindow::create(
                    instance,
                    HWND::default(),
                    animations,
                    Language::English,
                    None,
                    "Settings"
                )
                .is_err()
            );
            assert!(unsafe { FindWindowW(PET_WINDOW_CLASS, w!("Stand Awhile Pet")) }.is_err());
        }
    }

    #[test]
    fn scaled_animation_cache_preserves_pixels_across_dpi_and_character_changes() {
        use super::*;
        use crate::animation::{AnimationClip, Frame, LoopMode};

        let pet = hidden_test_pet();
        let state = state_mut(pet.hwnd).unwrap();
        let template = CharacterAnimations {
            idle: state.idle.clone(),
            walk: state.walk.clone(),
            jump: state.jump.clone(),
        };
        let characters: Vec<_> = [40, 160]
            .into_iter()
            .map(|color| {
                let mut animations = template.clone();
                for animation in [&mut animations.idle, &mut animations.walk] {
                    let mut frame = animation.frames[0].clone();
                    for pixel in frame.pixels.chunks_exact_mut(4).filter(|pixel| pixel[3] != 0) {
                        pixel[0] = color;
                    }
                    let mut second = frame.clone();
                    second.pixels[7] = 0;
                    second.pixels[4..7].fill(0);
                    animation.frames = vec![frame, second];
                    animation.clip = AnimationClip::new(
                        vec![Frame { id: 0 }, Frame { id: 1 }],
                        Duration::from_millis(120),
                        LoopMode::Loop,
                    )
                    .unwrap();
                }
                animations
            })
            .collect();
        for dpi in [144, 192, 144, 96] {
            for (character, animations) in characters.iter().enumerate() {
                pet.set_animations(animations.clone()).unwrap();
                let state = state_mut(pet.hwnd).unwrap();
                let current_frame = &state.walk.frames[state.last_frame.unwrap().1 as usize];
                let mut expected = pet_surface(current_frame, state.dpi).unwrap();
                expected.draw_scaled_frame(current_frame).unwrap();
                assert_eq!(
                    *state.surface, expected,
                    "character switch must discard previous cached pixels"
                );
                apply_pet_dpi(pet.hwnd, dpi, (100, 400)).unwrap();
                for clip in [ActiveClip::Walk, ActiveClip::Idle] {
                    let now = Instant::now();
                    with_state(pet.hwnd, |state| start_clip(state, clip, now)).unwrap();
                    for index in 0..20 {
                        let time = now + Duration::from_millis(index * 120);
                        update_frame_at(pet.hwnd, time).unwrap();
                        let state = state_mut(pet.hwnd).unwrap();
                        let frame_id = state.last_frame.unwrap().1 as usize;
                        let animation = if clip == ActiveClip::Walk {
                            &state.walk
                        } else {
                            &state.idle
                        };
                        let mut expected = pet_surface(&animation.frames[frame_id], dpi).unwrap();
                        expected.draw_scaled_frame(&animation.frames[frame_id]).unwrap();
                        assert_eq!(
                            *state.surface, expected,
                            "{character} {clip:?} dpi={dpi} frame={frame_id}"
                        );
                    }
                }
                if dpi == 96 {
                    assert!(
                        with_state(pet.hwnd, |state| state.scaled_frames.is_empty()).unwrap(),
                        "native-size frames need no extra cache"
                    );
                }
            }
        }
        unsafe { DestroyWindow(pet.hwnd).unwrap() };
    }

    #[test]
    #[ignore = "CPU benchmark; run explicitly on an otherwise idle machine"]
    fn visible_pet_render_cpu_budget() {
        use super::*;
        use crate::{asset::load_character_catalog, gdi::GdiPlus};
        use windows::Win32::{
            Foundation::FILETIME,
            System::{
                LibraryLoader::GetModuleHandleW,
                Threading::{GetCurrentThread, GetThreadTimes},
            },
        };

        fn cpu_ms() -> f64 {
            let mut creation = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            unsafe { GetThreadTimes(GetCurrentThread(), &mut creation, &mut exit, &mut kernel, &mut user).unwrap() };
            let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
            (ticks(kernel) + ticks(user)) as f64 / 10_000.0
        }

        let gdi_plus = GdiPlus::new().unwrap();
        let catalog = load_character_catalog(&gdi_plus).unwrap();
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }.unwrap().into();
        let pet = PetWindow::create(
            instance,
            HWND::default(),
            catalog.get("cat").unwrap().clone(),
            Language::English,
            None,
            "Settings",
        )
        .unwrap();
        pet.show_reminder().unwrap();
        let mut costs = Vec::new();
        for dpi in [96, 144, 192] {
            apply_pet_dpi(pet.hwnd, dpi, (100, 400)).unwrap();
            let now = Instant::now();
            with_state(pet.hwnd, |state| {
                state.show_animation = None;
                start_clip(state, ActiveClip::Walk, now);
            })
            .unwrap();
            // Measure steady looping after every frame has appeared once.
            for index in 0..10 {
                update_frame_at(pet.hwnd, now + Duration::from_millis(index * 100)).unwrap();
            }
            let before = cpu_ms();
            for index in 10..70 {
                update_frame_at(pet.hwnd, now + Duration::from_millis(index * 100)).unwrap();
            }
            let render_ms = cpu_ms() - before;
            let state = state_mut(pet.hwnd).unwrap();
            let cache_bytes: usize = state.scaled_frames.values().map(CachedSurface::byte_len).sum();
            state.speech_bubble.borrow_mut().start(now, Duration::ZERO);
            let before = cpu_ms();
            for _ in 0..360 {
                update_speech_bubble(pet.hwnd).unwrap();
            }
            let bubble_ms = cpu_ms() - before;
            eprintln!(
                "dpi={dpi}: 60 animation frames CPU={render_ms:.3} ms; 360 unchanged bubble updates CPU={bubble_ms:.3} ms; frame cache={cache_bytes} bytes"
            );
            costs.push(render_ms);
        }
        unsafe { DestroyWindow(pet.hwnd).unwrap() };
        assert!(
            costs.iter().all(|cost| *cost <= 100.0),
            "steady animation budget: <=100 ms CPU per 60 frames; measured {costs:?}"
        );
    }

    #[test]
    fn hidden_pet_has_no_animation_timer_work() {
        use super::*;
        use crate::{asset::load_character_catalog, gdi::GdiPlus};
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, SendMessageW},
        };

        fn pump_pet_timers(hwnd: HWND) -> usize {
            fn thread_cpu_ticks() -> u64 {
                use windows::Win32::{
                    Foundation::FILETIME,
                    System::Threading::{GetCurrentThread, GetThreadTimes},
                };
                let mut creation = FILETIME::default();
                let mut exit = FILETIME::default();
                let mut kernel = FILETIME::default();
                let mut user = FILETIME::default();
                unsafe {
                    GetThreadTimes(GetCurrentThread(), &mut creation, &mut exit, &mut kernel, &mut user).unwrap()
                };
                let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
                ticks(kernel) + ticks(user)
            }

            let cpu_before = thread_cpu_ticks();
            let start = Instant::now();
            let mut count = 0;
            while start.elapsed() < Duration::from_millis(250) {
                let mut message = MSG::default();
                while unsafe { PeekMessageW(&mut message, Some(hwnd), WM_TIMER, WM_TIMER, PM_REMOVE).as_bool() } {
                    count += 1;
                    unsafe { DispatchMessageW(&message) };
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            eprintln!(
                "{} timer callbacks / {} ms wall time / {:.3} ms thread CPU",
                count,
                start.elapsed().as_millis(),
                (thread_cpu_ticks() - cpu_before) as f64 / 10_000.0
            );
            count
        }

        let gdi_plus = GdiPlus::new().unwrap();
        let catalog = load_character_catalog(&gdi_plus).unwrap();
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }.unwrap().into();
        let pet = PetWindow::create(
            instance,
            HWND::default(),
            catalog.get("cat").unwrap().clone(),
            Language::English,
            None,
            "Settings",
        )
        .unwrap();
        let startup_ticks = pump_pet_timers(pet.hwnd);
        pet.show_reminder().unwrap();
        let visible_ticks = pump_pet_timers(pet.hwnd);
        pet.hide();
        // KillTimer cannot remove WM_TIMER messages already queued.
        state_mut(pet.hwnd).unwrap().last_frame = None;
        unsafe { SendMessageW(pet.hwnd, WM_TIMER, Some(WPARAM(PET_TIMER_ID)), None) };
        let stale_tick_rendered = state_mut(pet.hwnd).unwrap().last_frame.is_some();
        let hidden_ticks = pump_pet_timers(pet.hwnd);
        pet.show_reminder().unwrap();
        let resumed_ticks = pump_pet_timers(pet.hwnd);
        pet.hide_animated().unwrap();
        let exit_ticks = pump_pet_timers(pet.hwnd);
        let exit_hidden = !unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(pet.hwnd).as_bool() };
        let after_exit_ticks = pump_pet_timers(pet.hwnd);
        unsafe { DestroyWindow(pet.hwnd).unwrap() };

        eprintln!(
            "pet timer callbacks: startup={startup_ticks}, visible={visible_ticks}, hidden={hidden_ticks}, resumed={resumed_ticks}, exit={exit_ticks}, after_exit={after_exit_ticks}"
        );
        assert_eq!(startup_ticks, 0, "hidden startup must not animate or render");
        assert!(visible_ticks > 0, "visible reminder must animate");
        assert!(
            !stale_tick_rendered,
            "queued timer messages must not render a hidden pet"
        );
        assert_eq!(hidden_ticks, 0, "immediate hide must stop animation timer");
        assert!(resumed_ticks > 0, "showing again must restart animation timer");
        assert!(exit_ticks > 0, "exit animation must run until hidden");
        assert!(exit_hidden);
        assert_eq!(after_exit_ticks, 0, "completed exit must stop animation timer");
    }

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

    fn hidden_test_pet() -> super::PetWindow {
        use super::*;
        use crate::animation::{AnimationClip, Frame, LoopMode};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
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
                    renderer: Rc::new(RefCell::new(LayeredRenderer::new(hwnd).unwrap())),
                    surface: Rc::new(pet_surface(&frame, 96).unwrap()),
                    scaled_frames: HashMap::new(),
                    dpi: 96,
                    position: (100, 100),
                    idle: animation.clone(),
                    walk: animation.clone(),
                    jump: animation.clone(),
                    player: AnimationPlayer::new(clip),
                    active_clip: ActiveClip::Walk,
                    last_frame: None,
                    owner: HWND::default(),
                    drag: None,
                    hide_animation: None,
                    show_animation: None,
                    speech_bubble: Rc::new(RefCell::new(
                        SpeechBubbleController::create(instance, hwnd, Language::English).unwrap(),
                    )),
                    language: Language::English,
                    bubble_start: None,
                    start_menu_text: String::new(),
                    show_main_menu_text: String::new(),
                    settings_menu_text: String::new(),
                    about_menu_text: String::new(),
                    exit_menu_text: String::new(),
                },
            );
            pet
        }
    }

    #[test]
    fn losing_mouse_capture_cancels_drag_without_acknowledging() {
        use super::*;
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CAPTURECHANGED};
        let pet = hidden_test_pet();
        state_mut(pet.hwnd).unwrap().drag = Some(DragState {
            pointer_start: POINT { x: 100, y: 100 },
            window_start: POINT { x: 100, y: 100 },
            moved: false,
        });
        unsafe {
            SendMessageW(pet.hwnd, WM_CAPTURECHANGED, Some(WPARAM(0)), Some(LPARAM(0)));
        }
        assert!(state_mut(pet.hwnd).unwrap().drag.is_none());
        assert!(state_mut(pet.hwnd).unwrap().hide_animation.is_none());
    }

    #[test]
    fn pet_stays_above_normal_windows_from_creation() {
        use super::*;
        use crate::animation::{AnimationClip, Frame, LoopMode};
        use windows::Win32::{
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                GW_HWNDPREV, GWL_EXSTYLE, GetForegroundWindow, GetWindow, HWND_TOP, SWP_NOMOVE, WS_EX_TOPMOST,
                WS_VISIBLE,
            },
        };
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }.unwrap().into();
        let animation = PreparedAnimation {
            clip: AnimationClip::new(vec![Frame { id: 0 }], Duration::from_millis(120), LoopMode::Loop).unwrap(),
            frames: vec![PreparedFrame {
                width: 2,
                height: 2,
                pixels: [20, 40, 60, 255].repeat(4),
                hitbox: None,
            }],
        };
        let foreground = unsafe { GetForegroundWindow() };
        let pet = PetWindow::create(
            instance,
            HWND::default(),
            CharacterAnimations {
                idle: animation.clone(),
                walk: animation.clone(),
                jump: animation,
            },
            Language::English,
            None,
            "Settings",
        )
        .unwrap();
        assert!(!unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(pet.hwnd).as_bool() });
        unsafe {
            let _ = ShowWindow(pet.hwnd, SW_SHOWNOACTIVATE);
        }
        let mut rect = RECT::default();
        unsafe { GetWindowRect(pet.hwnd, &mut rect) }.unwrap();
        // Place an ordinary window over the Pet without changing foreground focus.
        let other = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("STATIC"),
                w!("Pet topmost regression test"),
                WS_POPUP | WS_VISIBLE,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                None,
                None,
                Some(instance),
                None,
            )
        }
        .unwrap();
        let check_topmost = || {
            unsafe {
                SetWindowPos(
                    other,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
                .unwrap();
            }
            let mut above = unsafe { GetWindow(other, GW_HWNDPREV) }.unwrap_or_default();
            while !above.is_invalid() && above != pet.hwnd {
                above = unsafe { GetWindow(above, GW_HWNDPREV) }.unwrap_or_default();
            }
            assert_eq!(above, pet.hwnd, "ordinary window covers the Pet");
            let style = unsafe { GetWindowLongPtrW(pet.hwnd, GWL_EXSTYLE) } as u32;
            assert_ne!(style & WS_EX_TOPMOST.0, 0);
            assert_ne!(style & WS_EX_NOACTIVATE.0, 0);
            assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        };
        // Keep assertions until after cleanup, including in the failing reproduction.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            check_topmost();
            pet.hide();
            unsafe {
                let _ = ShowWindow(pet.hwnd, SW_SHOWNOACTIVATE);
            }
            check_topmost();
            let state = state_mut(pet.hwnd).unwrap();
            set_position(pet.hwnd, state.position).unwrap();
            check_topmost();
            pet.hide();
            pet.show_reminder().unwrap();
            check_topmost();
        }));
        unsafe { DestroyWindow(other) }.unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }

    #[test]
    fn bubble_cycle_controls_animation_without_restarting_each_tick() {
        use super::*;
        use crate::{
            animation::{AnimationClip, Frame, LoopMode},
            speech_bubble::SpeechBubblePlayer,
        };
        let pet = hidden_test_pet();
        let state = state_mut(pet.hwnd).unwrap();
        let clip = AnimationClip::new(
            vec![Frame { id: 0 }, Frame { id: 1 }],
            Duration::from_millis(120),
            LoopMode::Loop,
        )
        .unwrap();
        state.idle.clip = clip.clone();
        state.walk.clip = clip.clone();
        state.player = AnimationPlayer::new(clip);
        let now = Instant::now();
        state.player.play(now);
        let mut bubble = SpeechBubblePlayer::new();
        sync_bubble_animation(state, bubble.update(now), now);
        assert_eq!(state.active_clip, ActiveClip::Walk);
        for (ms, expected) in [
            (10_000, ActiveClip::Idle),
            (15_000, ActiveClip::Walk),
            (25_000, ActiveClip::Idle),
        ] {
            let time = now + Duration::from_millis(ms);
            sync_bubble_animation(state, bubble.update(time), time);
            assert_eq!(state.active_clip, expected);
            assert_eq!(state.player.update(time).frame.id, 0);
            let next = time + Duration::from_millis(120);
            sync_bubble_animation(state, bubble.update(next), next);
            assert_eq!(state.player.update(next).frame.id, 1);
        }
    }

    #[test]
    fn showing_reminder_restores_hidden_pet_and_cancels_in_progress_exit() {
        use super::*;
        use windows::Win32::UI::WindowsAndMessaging::IsWindowVisible;
        let pet = hidden_test_pet();
        pet.show_reminder().unwrap();
        assert!(unsafe { IsWindowVisible(pet.hwnd).as_bool() });
        assert_eq!(state_mut(pet.hwnd).unwrap().active_clip, ActiveClip::Jump);

        let target = state_mut(pet.hwnd).unwrap().show_animation.as_ref().unwrap().target;
        pet.hide_animated().unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        state.hide_animation.as_mut().unwrap().started_at = Instant::now() - Duration::from_millis(90);
        update_hide_animation(pet.hwnd).unwrap();
        pet.show_reminder().unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert!(unsafe { IsWindowVisible(pet.hwnd).as_bool() });
        assert!(state.hide_animation.is_none());
        assert_eq!(state.active_clip, ActiveClip::Jump);
        assert_eq!(state.show_animation.as_ref().unwrap().target, target);
        assert!(!state.speech_bubble.borrow_mut().is_visible());
    }

    #[test]
    fn reminder_enters_with_jump_then_walk_before_the_bubble() {
        use super::*;
        use crate::animation::{AnimationClip, Frame, LoopMode};
        let pet = hidden_test_pet();
        let state = state_mut(pet.hwnd).unwrap();
        state.jump.clip = AnimationClip::new(
            vec![Frame { id: 0 }, Frame { id: 1 }],
            Duration::from_millis(200),
            LoopMode::Once,
        )
        .unwrap();
        state.jump.frames.push(state.jump.frames[0].clone());
        let now = Instant::now();
        begin_show_animation(pet.hwnd, now).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        let entrance = state.show_animation.as_ref().unwrap();
        let (from, target) = (entrance.from, entrance.target);
        let monitor = unsafe { MonitorFromWindow(pet.hwnd, MONITOR_DEFAULTTONEAREST) };
        let bounds = monitor_info_ex(monitor).unwrap().monitorInfo.rcMonitor;
        assert!(from.1 + state.surface.height() as i32 <= bounds.top || from.1 >= bounds.bottom);
        assert_eq!(state.active_clip, ActiveClip::Jump);
        assert!(!state.speech_bubble.borrow_mut().is_visible());
        sync_bubble_animation(state, true, now);
        assert_eq!(state.active_clip, ActiveClip::Jump);
        update_show_animation(pet.hwnd, now + Duration::from_millis(90)).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.position.1, from.1 + (target.1 - from.1) / 2);
        update_show_animation(pet.hwnd, now + Duration::from_millis(180)).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.position, target);
        assert!(state.show_animation.is_none());
        update_frame_at(pet.hwnd, now + Duration::from_millis(399)).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Jump);
        update_frame_at(pet.hwnd, now + Duration::from_millis(400)).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Walk);
        let visible = state
            .speech_bubble
            .borrow_mut()
            .update(state.position, (3, 2), now + Duration::from_millis(899))
            .unwrap();
        assert!(!visible);
        let visible = state
            .speech_bubble
            .borrow_mut()
            .update(state.position, (3, 2), now + Duration::from_millis(900))
            .unwrap();
        assert!(visible);
        sync_bubble_animation(state, visible, now + Duration::from_millis(900));
        assert_eq!(state.active_clip, ActiveClip::Idle);
        state.speech_bubble.borrow_mut().hide();
    }

    #[test]
    fn short_jump_waits_for_entrance_to_finish() {
        use super::*;
        use crate::animation::{AnimationClip, Frame, LoopMode};
        let pet = hidden_test_pet();
        let state = state_mut(pet.hwnd).unwrap();
        state.jump.clip =
            AnimationClip::new(vec![Frame { id: 0 }], Duration::from_millis(120), LoopMode::Once).unwrap();
        let now = Instant::now();
        begin_show_animation(pet.hwnd, now).unwrap();
        update_frame_at(pet.hwnd, now + Duration::from_millis(120)).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Jump);
        update_show_animation(pet.hwnd, now + Duration::from_millis(180)).unwrap();
        update_frame_at(pet.hwnd, now + Duration::from_millis(180)).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Walk);
        assert!(!state.speech_bubble.borrow_mut().is_visible());
    }

    #[test]
    fn reminder_transitions_to_walk_and_drag_preserves_bubble_animation() {
        use super::*;
        use crate::animation::{AnimationClip, Frame, LoopMode};
        let pet = hidden_test_pet();
        state_mut(pet.hwnd).unwrap().jump.clip =
            AnimationClip::new(vec![Frame { id: 0 }], Duration::from_millis(120), LoopMode::Once).unwrap();
        sync_bubble_animation(state_mut(pet.hwnd).unwrap(), true, Instant::now());
        pet.show_reminder().unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Jump);
        let now = state.show_animation.as_ref().unwrap().started_at + Duration::from_millis(180);
        update_show_animation(pet.hwnd, now).unwrap();
        update_frame_at(pet.hwnd, now).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Walk);
        assert!(!state.speech_bubble.borrow_mut().is_visible());
        begin_drag(pet.hwnd).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Walk);
        sync_bubble_animation(state, true, Instant::now());
        assert_eq!(state.active_clip, ActiveClip::Idle);
        begin_drag(pet.hwnd).unwrap();
        let state = state_mut(pet.hwnd).unwrap();
        assert_eq!(state.active_clip, ActiveClip::Idle);
        unsafe {
            let _ = ReleaseCapture();
        }
        with_state(pet.hwnd, |state| state.drag = None).unwrap();
        pet.hide();
    }

    #[test]
    fn dpi_changes_resize_rendering_and_hit_testing_without_accumulating_scale() {
        use super::*;
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
        let pet = hidden_test_pet();
        let hwnd = pet.hwnd;
        unsafe {
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
