use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use windows::{
    Win32::Graphics::GdiPlus::{
        BitmapData, GdipBitmapLockBits, GdipBitmapUnlockBits, GdipCloneBitmapAreaI, GdipCreateBitmapFromStream,
        GdipDisposeImage, GdipGetImageHeight, GdipGetImageWidth, GpBitmap, GpImage, ImageLockModeRead,
        PixelFormatAlpha, PixelFormatCanonical, PixelFormatGDI, Rect, Status,
    },
    Win32::UI::Shell::SHCreateMemStream,
};

use crate::{
    animation::{AnimationClip, Frame, LoopMode},
    ui::GdiPlus,
};

const PIXEL_FORMAT_32BPP_ARGB: i32 = (PixelFormatAlpha | PixelFormatCanonical | PixelFormatGDI | (32 << 8) | 10) as i32;
const EMBEDDED_MANIFEST: &str = include_str!("../assets/pets/cat-dog/manifest.json");
include!(concat!(env!("OUT_DIR"), "/pet_frames.rs"));

#[derive(Debug)]
pub enum AssetError {
    Manifest(serde_json::Error),
    InvalidManifest(String),
    Decode(String),
}

impl From<serde_json::Error> for AssetError {
    fn from(error: serde_json::Error) -> Self {
        Self::Manifest(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameHitbox {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl FrameHitbox {
    fn from_pixels(width: u32, height: u32, pixels: &[u8]) -> Option<Self> {
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for y in 0..height {
            for x in 0..width {
                let alpha = pixels[((y * width + x) * 4 + 3) as usize];
                if alpha == 0 {
                    continue;
                }
                bounds = Some(match bounds {
                    Some((left, top, right, bottom)) => (left.min(x), top.min(y), right.max(x + 1), bottom.max(y + 1)),
                    None => (x, y, x + 1, y + 1),
                });
            }
        }
        bounds.map(|(left, top, right, bottom)| Self {
            left,
            top,
            right,
            bottom,
        })
    }

    pub(crate) fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub hitbox: Option<FrameHitbox>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedAnimation {
    pub clip: AnimationClip,
    pub frames: Vec<PreparedFrame>,
}

#[derive(Clone)]
pub struct CharacterAnimations {
    pub idle: PreparedAnimation,
    pub walk: PreparedAnimation,
    pub jump: PreparedAnimation,
}

pub struct CharacterCatalog {
    characters: BTreeMap<String, CharacterAnimations>,
}

impl CharacterCatalog {
    pub fn get(&self, character_id: &str) -> Option<&CharacterAnimations> {
        self.characters.get(character_id)
    }

    pub fn character_ids(&self) -> impl Iterator<Item = &str> {
        self.characters.keys().map(String::as_str)
    }
}

#[derive(Deserialize)]
struct Manifest {
    default_frame_duration_ms: u64,
    characters: BTreeMap<String, CharacterManifest>,
}

#[derive(Deserialize)]
struct CharacterManifest {
    path: PathBuf,
    animations: BTreeMap<String, AnimationManifest>,
}

#[derive(Deserialize)]
struct AnimationManifest {
    pattern: String,
    frame_count: usize,
    frame_duration_ms: Option<u64>,
    #[serde(rename = "loop")]
    loop_mode: bool,
}

pub fn load_character_catalog(_gdi_plus: &GdiPlus) -> Result<CharacterCatalog, AssetError> {
    let manifest = load_manifest()?;
    let characters = manifest
        .characters
        .iter()
        .map(|(id, character)| {
            load_character_animations(character, manifest.default_frame_duration_ms)
                .map(|animations| (id.clone(), animations))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    if characters.is_empty() {
        return Err(AssetError::InvalidManifest("manifest has no characters".to_owned()));
    }

    Ok(CharacterCatalog { characters })
}

pub fn load_cat_animations(_gdi_plus: &GdiPlus) -> Result<CharacterAnimations, AssetError> {
    load_character_catalog(_gdi_plus)?
        .characters
        .remove("cat")
        .ok_or_else(|| AssetError::InvalidManifest("missing cat character".to_owned()))
}

fn load_character_animations(
    character: &CharacterManifest,
    default_frame_duration_ms: u64,
) -> Result<CharacterAnimations, AssetError> {
    let idle = load_animation(character, "idle", default_frame_duration_ms)?;
    let walk = load_animation(character, "walk", default_frame_duration_ms)?;
    let jump = load_animation(character, "jump", default_frame_duration_ms)?;

    Ok(CharacterAnimations { idle, walk, jump })
}

fn load_manifest() -> Result<Manifest, AssetError> {
    Ok(serde_json::from_str(EMBEDDED_MANIFEST)?)
}

fn load_animation(
    character: &CharacterManifest,
    name: &str,
    default_frame_duration_ms: u64,
) -> Result<PreparedAnimation, AssetError> {
    let animation = character
        .animations
        .get(name)
        .ok_or_else(|| AssetError::InvalidManifest(format!("missing cat animation: {name}")))?;
    if animation.frame_count == 0 {
        return Err(AssetError::InvalidManifest(format!("animation has no frames: {name}")));
    }

    let frame_paths = (1..=animation.frame_count)
        .map(|index| {
            let filename = animation.pattern.replace("{index}", &index.to_string());
            character.path.join(filename)
        })
        .collect::<Vec<_>>();
    let frames = frame_paths
        .iter()
        .map(|path| load_png_frame(path))
        .collect::<Result<Vec<_>, _>>()?;
    let clip = AnimationClip::new(
        (0..frames.len()).map(|id| Frame { id: id as u32 }).collect(),
        Duration::from_millis(animation.frame_duration_ms.unwrap_or(default_frame_duration_ms)),
        if animation.loop_mode {
            LoopMode::Loop
        } else {
            LoopMode::Once
        },
    )
    .map_err(|error| AssetError::InvalidManifest(format!("invalid animation {name}: {error:?}")))?;

    Ok(PreparedAnimation { clip, frames })
}

fn load_png_frame(path: &Path) -> Result<PreparedFrame, AssetError> {
    let name = path.to_string_lossy().replace('\\', "/");
    let bytes = EMBEDDED_FRAMES
        .iter()
        .find_map(|(key, bytes)| (*key == name).then_some(*bytes))
        .ok_or_else(|| AssetError::InvalidManifest(format!("missing embedded PNG: {name}")))?;
    decode_png_frame(bytes)
}

fn decode_png_frame(bytes: &[u8]) -> Result<PreparedFrame, AssetError> {
    let stream = unsafe { SHCreateMemStream(Some(bytes)) }
        .ok_or_else(|| AssetError::Decode("could not allocate PNG memory stream".to_owned()))?;
    let mut bitmap = std::ptr::null_mut::<GpBitmap>();

    let status = unsafe { GdipCreateBitmapFromStream(&stream, &mut bitmap) };
    ensure_gdiplus_ok(status, "could not load PNG")?;

    let result = read_bitmap(bitmap);
    unsafe {
        let _ = GdipDisposeImage(bitmap.cast::<GpImage>());
    }
    result
}

fn read_bitmap(bitmap: *mut GpBitmap) -> Result<PreparedFrame, AssetError> {
    let mut width = 0;
    let mut height = 0;
    ensure_gdiplus_ok(
        unsafe { GdipGetImageWidth(bitmap.cast::<GpImage>(), &mut width) },
        "could not read PNG width",
    )?;
    ensure_gdiplus_ok(
        unsafe { GdipGetImageHeight(bitmap.cast::<GpImage>(), &mut height) },
        "could not read PNG height",
    )?;

    let mut normalized_bitmap = std::ptr::null_mut::<GpBitmap>();
    ensure_gdiplus_ok(
        unsafe {
            GdipCloneBitmapAreaI(
                0,
                0,
                width as i32,
                height as i32,
                PIXEL_FORMAT_32BPP_ARGB,
                bitmap,
                &mut normalized_bitmap,
            )
        },
        "could not normalize PNG pixel format",
    )?;

    let result = read_locked_bitmap(normalized_bitmap, width, height);
    unsafe {
        let _ = GdipDisposeImage(normalized_bitmap.cast::<GpImage>());
    }
    result
}

fn read_locked_bitmap(bitmap: *mut GpBitmap, width: u32, height: u32) -> Result<PreparedFrame, AssetError> {
    let rect = Rect {
        X: 0,
        Y: 0,
        Width: width as i32,
        Height: height as i32,
    };
    let mut data = BitmapData::default();
    ensure_gdiplus_ok(
        unsafe {
            GdipBitmapLockBits(
                bitmap,
                &rect,
                ImageLockModeRead.0 as u32,
                PIXEL_FORMAT_32BPP_ARGB,
                &mut data,
            )
        },
        "could not lock PNG pixels",
    )?;

    let result = copy_premultiplied_pixels(&data, width, height);
    let unlock_status = unsafe { GdipBitmapUnlockBits(bitmap, &mut data) };
    ensure_gdiplus_ok(unlock_status, "could not unlock PNG pixels")?;
    result.map(|pixels| PreparedFrame {
        width,
        height,
        hitbox: FrameHitbox::from_pixels(width, height, &pixels),
        pixels,
    })
}

fn copy_premultiplied_pixels(data: &BitmapData, width: u32, height: u32) -> Result<Vec<u8>, AssetError> {
    if data.Scan0.is_null() {
        return Err(AssetError::Decode("PNG pixel buffer is null".to_owned()));
    }

    let row_bytes = width as usize * 4;
    let mut pixels = vec![0; row_bytes * height as usize];
    let stride = data.Stride;
    let stride_abs = stride.unsigned_abs() as usize;
    if stride_abs < row_bytes {
        return Err(AssetError::Decode("PNG pixel stride is too small".to_owned()));
    }

    for row in 0..height as usize {
        let source_row = if stride >= 0 { row } else { height as usize - 1 - row };
        let source =
            unsafe { std::slice::from_raw_parts(data.Scan0.cast::<u8>().add(source_row * stride_abs), row_bytes) };
        let destination = &mut pixels[row * row_bytes..(row + 1) * row_bytes];
        for (source_pixel, destination_pixel) in source.chunks_exact(4).zip(destination.chunks_exact_mut(4)) {
            let alpha = source_pixel[3] as u16;
            destination_pixel[0] = premultiply(source_pixel[0], alpha);
            destination_pixel[1] = premultiply(source_pixel[1], alpha);
            destination_pixel[2] = premultiply(source_pixel[2], alpha);
            destination_pixel[3] = source_pixel[3];
        }
    }

    Ok(pixels)
}

fn premultiply(channel: u8, alpha: u16) -> u8 {
    ((channel as u16 * alpha + 127) / 255) as u8
}

fn ensure_gdiplus_ok(status: Status, operation: &str) -> Result<(), AssetError> {
    if status.0 == 0 {
        Ok(())
    } else {
        Err(AssetError::Decode(format!("{operation}: GDI+ status {}", status.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameHitbox, copy_premultiplied_pixels, load_cat_animations, load_manifest, premultiply};
    use std::ptr::null_mut;
    use windows::Win32::Graphics::GdiPlus::BitmapData;

    #[test]
    fn loads_the_checked_in_cat_manifest() {
        let manifest = load_manifest().expect("checked-in manifest should be valid");
        let cat = manifest.characters.get("cat").expect("cat should be present");
        assert_eq!(cat.animations["idle"].frame_count, 10);
        assert_eq!(cat.animations["walk"].frame_count, 10);
        assert_eq!(cat.animations["jump"].frame_count, 8);
    }

    #[test]
    fn resolves_frame_numbers_in_stable_order() {
        let manifest = load_manifest().expect("checked-in manifest should be valid");
        let cat = manifest.characters.get("cat").expect("cat should be present");
        let animation = cat.animations.get("idle").expect("idle should be present");

        let paths = (1..=animation.frame_count)
            .map(|index| animation.pattern.replace("{index}", &index.to_string()))
            .collect::<Vec<_>>();
        assert_eq!(paths.first().expect("first frame"), "Idle (1).png");
        assert_eq!(paths.last().expect("last frame"), "Idle (10).png");
    }

    #[test]
    fn loads_embedded_frames_for_every_manifest_animation() {
        let gdi_plus = crate::ui::GdiPlus::new().expect("GDI+ should initialize");
        let assets = load_cat_animations(&gdi_plus).expect("checked-in cat assets should load");

        assert_eq!(assets.idle.frames.len(), 10);
        assert_eq!(assets.walk.frames.len(), 10);
        assert_eq!(assets.jump.frames.len(), 8);
        assert_eq!(
            assets.idle.frames[0].pixels.len(),
            (assets.idle.frames[0].width * assets.idle.frames[0].height * 4) as usize
        );
        assert!(assets.idle.frames[0].pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));

        let catalog = super::load_character_catalog(&gdi_plus).unwrap();
        for character in ["cat", "dog"] {
            let animations = catalog.get(character).unwrap();
            assert_eq!(animations.idle.frames.len(), 10);
            assert_eq!(animations.walk.frames.len(), 10);
            assert_eq!(animations.jump.frames.len(), 8);
            assert!(animations.jump.frames.iter().all(|frame| frame.hitbox.is_some()));
        }
        // Verify every manifest frame, including animations reserved for later use.
        let manifest = load_manifest().unwrap();
        let mut frame_count = 0;
        for character in manifest.characters.values() {
            for animation in character.animations.values() {
                for index in 1..=animation.frame_count {
                    let path = character
                        .path
                        .join(animation.pattern.replace("{index}", &index.to_string()));
                    let frame = super::load_png_frame(&path).unwrap();
                    assert!(frame.width > 0 && frame.height > 0);
                    assert_eq!(frame.pixels.len(), (frame.width * frame.height * 4) as usize);
                    frame_count += 1;
                }
            }
        }
        assert_eq!(frame_count, super::EMBEDDED_FRAMES.len());
        assert!(super::decode_png_frame(b"not a PNG").is_err());
    }

    #[test]
    fn missing_embedded_frames_report_the_resource_name() {
        let error = super::load_png_frame(std::path::Path::new("png/missing.png")).unwrap_err();
        assert!(matches!(error, super::AssetError::InvalidManifest(message) if message.contains("png/missing.png")));
    }

    #[test]
    fn loads_every_character_into_the_catalog() {
        let manifest = load_manifest().expect("checked-in character catalog should load");

        assert_eq!(
            manifest.characters.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["cat", "dog"]
        );
        assert_eq!(manifest.characters["dog"].animations["idle"].frame_count, 10);
        assert_eq!(manifest.characters["dog"].animations["jump"].frame_count, 8);
    }

    #[test]
    fn premultiplies_channels() {
        assert_eq!(premultiply(255, 128), 128);
        assert_eq!(premultiply(100, 0), 0);
        assert_eq!(premultiply(100, 255), 100);
    }

    #[test]
    fn copies_bgra_pixels_and_preserves_rows() {
        let source = [10_u8, 20, 30, 128, 40, 50, 60, 255];
        let data = BitmapData {
            Width: 2,
            Height: 1,
            Stride: 8,
            PixelFormat: 0,
            Scan0: source.as_ptr().cast_mut().cast(),
            Reserved: 0,
        };

        let pixels = copy_premultiplied_pixels(&data, 2, 1).expect("pixels should copy");
        assert_eq!(pixels, vec![5, 10, 15, 128, 40, 50, 60, 255]);
    }

    #[test]
    fn rejects_null_pixel_buffers() {
        let data = BitmapData {
            Width: 1,
            Height: 1,
            Stride: 4,
            PixelFormat: 0,
            Scan0: null_mut(),
            Reserved: 0,
        };

        assert!(copy_premultiplied_pixels(&data, 1, 1).is_err());
    }

    #[test]
    fn builds_a_per_frame_hitbox_from_alpha_pixels() {
        let pixels = [0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 255];
        let hitbox = FrameHitbox::from_pixels(2, 2, &pixels).expect("opaque pixels should have a hitbox");

        assert_eq!(
            hitbox,
            FrameHitbox {
                left: 1,
                top: 0,
                right: 2,
                bottom: 2
            }
        );
        assert!(!hitbox.contains(0, 0));
        assert!(hitbox.contains(1, 1));
    }
}
