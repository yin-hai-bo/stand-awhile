use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use windows::Win32::{
    Foundation::LPARAM,
    Graphics::Gdi::{
        CreateFontIndirectW, DEFAULT_CHARSET, DeleteObject, EnumFontFamiliesExW, GetDC, HFONT, HGDIOBJ, LOGFONTW,
        ReleaseDC, TEXTMETRICW,
    },
    UI::{
        HiDpi::SystemParametersInfoForDpi,
        WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS},
    },
};

static COMMON_GUI_FONT_CACHE: OnceLock<Mutex<HashMap<(u32, bool), usize>>> = OnceLock::new();
static FONT_EXISTENCE_CACHE: OnceLock<Mutex<HashMap<&'static str, bool>>> = OnceLock::new();

pub fn common_gui_font(dpi: u32, chinese: bool) -> Option<HFONT> {
    let cache = COMMON_GUI_FONT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("common GUI font cache poisoned");
    if let Some(raw_font) = cache.get(&(dpi, chinese)) {
        return Some(HFONT(*raw_font as _));
    }

    let font = create_common_gui_font(dpi, chinese)?;
    cache.insert((dpi, chinese), font.0 as usize);
    Some(font)
}

pub fn release_common_gui_fonts() {
    let Some(cache) = COMMON_GUI_FONT_CACHE.get() else {
        return;
    };
    let mut cache = cache.lock().expect("common GUI font cache poisoned");
    for raw_font in cache.drain().map(|(_, raw_font)| raw_font) {
        unsafe {
            let _ = DeleteObject(HGDIOBJ(raw_font as _));
        }
    }
}

fn create_common_gui_font(dpi: u32, chinese: bool) -> Option<HFONT> {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some((&mut metrics as *mut NONCLIENTMETRICSW).cast()),
            0,
            dpi,
        )
        .ok()?;
    }
    let candidates = if chinese {
        ["Microsoft YaHei", "Segoe UI"]
    } else {
        ["Segoe UI", "Arial"]
    };
    let default_font = metrics.lfMessageFont;
    for candidate in candidates {
        if !font_exists(candidate) {
            continue;
        }
        let mut font = default_font;
        set_face_name(&mut font, candidate);
        font.lfHeight = font.lfHeight * 11 / 10;
        let handle = unsafe { CreateFontIndirectW(&font) };
        if !handle.is_invalid() {
            return Some(handle);
        }
    }

    let mut font = default_font;
    font.lfHeight = font.lfHeight * 11 / 10;
    let handle = unsafe { CreateFontIndirectW(&font) };
    (!handle.is_invalid()).then_some(handle)
}

fn font_exists(face_name: &'static str) -> bool {
    let cache = FONT_EXISTENCE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(exists) = cache
        .lock()
        .expect("font existence cache poisoned")
        .get(face_name)
        .copied()
    {
        return exists;
    }

    let hdc = unsafe { GetDC(None) };
    if hdc.is_invalid() {
        cache
            .lock()
            .expect("font existence cache poisoned")
            .insert(face_name, false);
        return false;
    }

    let mut query = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..Default::default()
    };
    set_face_name(&mut query, face_name);
    let mut search = FontSearch {
        face_name,
        found: false,
    };
    unsafe {
        let _ = EnumFontFamiliesExW(
            hdc,
            &query,
            Some(font_search_callback),
            LPARAM((&mut search as *mut FontSearch).cast::<()>() as isize),
            0,
        );
        let _ = ReleaseDC(None, hdc);
    }
    cache
        .lock()
        .expect("font existence cache poisoned")
        .insert(face_name, search.found);
    search.found
}

struct FontSearch {
    face_name: &'static str,
    found: bool,
}

unsafe extern "system" fn font_search_callback(
    log_font: *const LOGFONTW,
    _text_metric: *const TEXTMETRICW,
    _font_type: u32,
    lparam: LPARAM,
) -> i32 {
    if log_font.is_null() {
        return 1;
    }
    let search = unsafe { &mut *(lparam.0 as *mut FontSearch) };
    let name = unsafe { String::from_utf16_lossy(&(*log_font).lfFaceName) };
    search.found = name.trim_end_matches('\0').eq_ignore_ascii_case(search.face_name);
    0
}

fn set_face_name(font: &mut LOGFONTW, face_name: &str) {
    let mut chars = face_name.encode_utf16().chain([0]);
    for slot in &mut font.lfFaceName {
        *slot = chars.next().unwrap_or(0);
    }
}
