use windows::{
    Win32::Graphics::Gdi::{DeleteDC, HDC},
    core::Free,
};

pub(crate) struct HdcOwner(HDC);

impl HdcOwner {
    pub(crate) fn new(handle: HDC) -> Self {
        Self(handle)
    }

    pub(crate) fn handle(&self) -> HDC {
        self.0
    }
}

impl Free for HdcOwner {
    unsafe fn free(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}
