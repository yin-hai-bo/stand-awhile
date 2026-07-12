use windows::Win32::{Foundation::HWND, Graphics::Gdi::HDC};
use windows::core::Result;

pub trait Component {
    fn layout(&self, parent: HWND, dc: HDC) -> Result<()>;

    fn invalidate(&self);
}
