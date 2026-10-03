use windows::{Win32::Foundation::HWND, core::Result as WindowsResult};

use crate::{asset::PreparedFrame, gdi::DibSurface};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfacePoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    InvalidSize,
    BufferSizeMismatch,
    FrameOutsideSurface,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelSurface {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl PixelSurface {
    pub fn new(width: u32, height: u32) -> std::result::Result<Self, SurfaceError> {
        if width == 0 || height == 0 {
            return Err(SurfaceError::InvalidSize);
        }

        let pixel_count = width
            .checked_mul(height)
            .and_then(|count| count.checked_mul(4))
            .ok_or(SurfaceError::InvalidSize)?;

        Ok(Self {
            width,
            height,
            pixels: vec![0; pixel_count as usize],
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    pub fn draw_frame(&mut self, frame: &PreparedFrame, origin: SurfacePoint) -> std::result::Result<(), SurfaceError> {
        self.draw_frame_with_pivot(frame, origin, SurfacePoint { x: 0, y: 0 })
    }

    /// Draws a frame so that its pivot lands at `position` on this surface.
    pub fn draw_frame_with_pivot(
        &mut self,
        frame: &PreparedFrame,
        position: SurfacePoint,
        pivot: SurfacePoint,
    ) -> std::result::Result<(), SurfaceError> {
        let expected_size = frame
            .width
            .checked_mul(frame.height)
            .and_then(|count| count.checked_mul(4))
            .ok_or(SurfaceError::BufferSizeMismatch)? as usize;
        if frame.pixels.len() != expected_size {
            return Err(SurfaceError::BufferSizeMismatch);
        }

        let origin_x = position.x.saturating_sub(pivot.x);
        let origin_y = position.y.saturating_sub(pivot.y);
        let left = origin_x.max(0) as u32;
        let top = origin_y.max(0) as u32;
        let source_left = origin_x.saturating_neg().max(0) as u32;
        let source_top = origin_y.saturating_neg().max(0) as u32;
        let copy_width = frame
            .width
            .saturating_sub(source_left)
            .min(self.width.saturating_sub(left));
        let copy_height = frame
            .height
            .saturating_sub(source_top)
            .min(self.height.saturating_sub(top));
        if copy_width == 0 || copy_height == 0 {
            return Err(SurfaceError::FrameOutsideSurface);
        }

        for row in 0..copy_height as usize {
            let source_start = ((source_top as usize + row) * frame.width as usize + source_left as usize) * 4;
            let destination_start = ((top as usize + row) * self.width as usize + left as usize) * 4;
            let byte_count = copy_width as usize * 4;
            self.pixels[destination_start..destination_start + byte_count]
                .copy_from_slice(&frame.pixels[source_start..source_start + byte_count]);
        }

        Ok(())
    }
}

pub struct LayeredRenderer {
    hwnd: HWND,
    surface: Option<DibSurface>,
}

impl LayeredRenderer {
    pub fn new(hwnd: HWND) -> WindowsResult<Self> {
        Ok(Self { hwnd, surface: None })
    }

    pub fn submit(&mut self, surface: &PixelSurface, position: SurfacePoint) -> WindowsResult<()> {
        let needs_surface = self
            .surface
            .as_ref()
            .is_none_or(|dib| dib.dimensions() != (surface.width, surface.height));
        if needs_surface {
            self.surface = Some(DibSurface::new(surface.width, surface.height)?);
        }

        let dib = self.surface.as_mut().expect("surface was created above");
        dib.copy_from(&surface.pixels);
        dib.submit(self.hwnd, position)
    }
}

#[cfg(test)]
mod tests {
    use super::{PixelSurface, SurfaceError, SurfacePoint};
    use crate::asset::{FrameHitbox, PreparedFrame};

    fn frame(width: u32, height: u32, pixels: Vec<u8>) -> PreparedFrame {
        PreparedFrame {
            width,
            height,
            pixels,
            hitbox: Some(FrameHitbox {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            }),
        }
    }

    #[test]
    fn rejects_empty_surfaces() {
        assert_eq!(PixelSurface::new(0, 10), Err(SurfaceError::InvalidSize));
        assert_eq!(PixelSurface::new(10, 0), Err(SurfaceError::InvalidSize));
    }

    #[test]
    fn draws_a_frame_at_the_requested_origin() {
        let mut surface = PixelSurface::new(3, 2).expect("surface should be valid");
        let source = frame(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        surface
            .draw_frame(&source, SurfacePoint { x: 1, y: 1 })
            .expect("frame should fit");

        assert_eq!(
            surface.pixels(),
            &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[test]
    fn draws_a_frame_with_its_pivot_at_the_requested_position() {
        let mut surface = PixelSurface::new(2, 2).expect("surface should be valid");
        let source = frame(2, 2, (1..=16).collect());

        surface
            .draw_frame_with_pivot(&source, SurfacePoint { x: 1, y: 1 }, SurfacePoint { x: 1, y: 1 })
            .expect("frame should fit");

        assert_eq!(
            surface.pixels(),
            &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn clips_frames_that_extend_past_the_surface() {
        let mut surface = PixelSurface::new(2, 2).expect("surface should be valid");
        let source = frame(2, 2, (1..=16).collect());

        surface
            .draw_frame(&source, SurfacePoint { x: -1, y: -1 })
            .expect("frame should overlap");

        assert_eq!(surface.pixels(), &[13, 14, 15, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rejects_frames_with_invalid_buffers() {
        let mut surface = PixelSurface::new(2, 2).expect("surface should be valid");
        let source = frame(1, 1, vec![0; 3]);

        assert_eq!(
            surface.draw_frame(&source, SurfacePoint { x: 0, y: 0 }),
            Err(SurfaceError::BufferSizeMismatch)
        );
    }
}
