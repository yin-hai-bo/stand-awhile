use windows::{
    Win32::{
        Foundation::{COLORREF, HWND, POINT, SIZE},
        Graphics::Gdi::{AC_SRC_ALPHA, BLENDFUNCTION},
        UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow},
    },
    core::{Owned, Result},
};

use crate::{gdi::dib_bitmap::DibBitmap, render::SurfacePoint};

pub(crate) struct DibSurface {
    bitmap: Owned<DibBitmap>,
    width: u32,
    height: u32,
}

impl DibSurface {
    pub(crate) fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err(windows::core::Error::from_win32());
        }

        let bitmap = DibBitmap::new(width, height)?;
        let bitmap = unsafe { Owned::new(bitmap) };

        Ok(Self { bitmap, width, height })
    }

    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn copy_from(&mut self, pixels: &[u8]) {
        self.bitmap.copy_from(pixels);
    }

    pub(crate) fn submit(&self, hwnd: HWND, position: SurfacePoint) -> Result<()> {
        let destination = POINT {
            x: position.x,
            y: position.y,
        };
        let size = SIZE {
            cx: self.width as i32,
            cy: self.height as i32,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: 0,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        unsafe {
            UpdateLayeredWindow(
                hwnd,
                None,
                Some(&destination),
                Some(&size),
                Some(self.bitmap.hdc()),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
    }
}
