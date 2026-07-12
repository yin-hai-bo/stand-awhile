use std::sync::Mutex;

use windows::Win32::Graphics::GdiPlus::{GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, Status};
use windows::core::Error;

static GDIPLUS_TOKEN: Mutex<Option<usize>> = Mutex::new(None);

pub struct GdiPlus;

impl GdiPlus {
    pub fn new() -> windows::core::Result<Self> {
        let mut token = GDIPLUS_TOKEN.lock().expect("gdiplus token mutex poisoned");
        if token.is_some() {
            return Err(Error::from_win32());
        }

        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: false.into(),
            SuppressExternalCodecs: false.into(),
        };
        let mut new_token = 0usize;
        let status = unsafe { GdiplusStartup(&mut new_token, &input, std::ptr::null_mut()) };
        ensure_gdiplus_ok(status)?;
        *token = Some(new_token);

        Ok(Self)
    }
}

impl Drop for GdiPlus {
    fn drop(&mut self) {
        let mut token = GDIPLUS_TOKEN.lock().expect("gdiplus token mutex poisoned");
        if let Some(token_value) = token.take() {
            unsafe {
                GdiplusShutdown(token_value);
            }
        }
    }
}

fn ensure_gdiplus_ok(status: Status) -> windows::core::Result<()> {
    if status.0 == 0 {
        Ok(())
    } else {
        Err(Error::from_win32())
    }
}
