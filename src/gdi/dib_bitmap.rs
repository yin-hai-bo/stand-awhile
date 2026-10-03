use std::{mem::size_of, ptr::copy_nonoverlapping};

use windows::{
    Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, HBITMAP, HDC,
        HGDIOBJ, SelectObject,
    },
    core::{Free, Owned, Result},
};

use crate::gdi::hdc_owner::HdcOwner;

pub(crate) struct DibBitmap {
    _bitmap: Owned<HBITMAP>,
    hdc_owner: Owned<HdcOwner>,
    old_bitmap: HGDIOBJ,
    bits: *mut u8,
}

impl DibBitmap {
    pub(crate) fn new(width: u32, height: u32) -> Result<Self> {
        let hdc = unsafe { CreateCompatibleDC(None) };
        if hdc.is_invalid() {
            return Err(windows::core::Error::from_win32());
        }
        let hdc_owner = unsafe { Owned::new(HdcOwner::new(hdc)) };
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(
                Some(hdc_owner.handle()),
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )?
        };
        let bitmap = unsafe { Owned::new(bitmap) };
        if bits.is_null() {
            return Err(windows::core::Error::from_win32());
        }

        let old_bitmap = unsafe { SelectObject(hdc_owner.handle(), HGDIOBJ((*bitmap).0)) };
        if old_bitmap.is_invalid() {
            return Err(windows::core::Error::from_win32());
        }

        Ok(Self {
            _bitmap: bitmap,
            hdc_owner,
            old_bitmap,
            bits: bits.cast(),
        })
    }

    pub(crate) fn hdc(&self) -> HDC {
        self.hdc_owner.handle()
    }

    pub(crate) fn copy_from(&mut self, pixels: &[u8]) {
        unsafe {
            copy_nonoverlapping(pixels.as_ptr(), self.bits, pixels.len());
        }
    }
}

impl Free for DibBitmap {
    unsafe fn free(&mut self) {
        // Restore the selected bitmap before the owned resources are dropped.
        unsafe {
            let _ = SelectObject(self.hdc_owner.handle(), self.old_bitmap);
        }
    }
}
