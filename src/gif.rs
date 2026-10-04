//! An animated tile picture: GIF frames decoded once, cover-cropped to the
//! tile, masked to its rounded corners, and cached per file + draw size, so
//! the renderer picks a frame from a `Vec<Handle>` and nothing is decoded in
//! `view`.
//!
//! Everything else — PNG, JPG, WebP, SVG, a still GIF — stays on the single-
//! handle `tileimage::masked` path. The format test is byte-level, so a
//! file's name is never the authority.
//!
//! Decoded straight into tile pixel sizes (never a 4K GIF at 150 px) through
//! the same mask the still path uses, so a GIF corner is cut exactly as its
//! PNG neighbour's would be. If the file is missing or malformed, the loader
//! returns `None` and the caller falls back; the UI never panics on a tile.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use cosmic::widget::image::Handle;
use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;

/// A decoded animated image, ready to draw.
pub struct Animation {
    /// One handle per frame, each already masked to the tile's rounded corners.
    pub frames: Vec<Handle>,
    /// Per-frame display time, in the same order as `frames`.
    pub delays: Vec<Duration>,
}

/// Whether the bytes at the start of `data` are a GIF — magic alone, so a
/// `.jpg` saved from an animated GIF still animates and a `.gif` of a single
/// still still-frames correctly.
pub fn is_gif(data: &[u8]) -> bool {
    data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")
}

/// Whether the file at `path` is an animated GIF with more than one frame.
///
/// A single-frame GIF reads as a still: a one-frame "animation" is a still
/// picture, and `tileimage::masked` draws stills more cheaply.
#[allow(dead_code)] // public helper with its own tests; the UI reaches the
                    // same decision via `load(..).frames.len() > 1`.
pub fn is_animated(path: &str) -> bool {
    load(path, 1, 1, 0.0).is_some_and(|a| a.frames.len() > 1)
}

/// What the cache stores a decoded animation under: the file, the tile's
/// drawn pixel size, and the rounded-corner radius. Rounded to whole pixels,
/// which is all a mask can resolve.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    path: String,
    w: u32,
    h: u32,
    radius: u32,
}

type Cache = HashMap<Key, Option<std::sync::Arc<Animation>>>;

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The animation for `path`, decoded to `w`x`h` and masked to `radius`, or
/// `None` for a malformed or missing file (so the caller falls back to the
/// finish). A still file at that path, including a one-frame GIF, is still
/// handled here; it comes back as a one-frame `Animation`, which the UI then
/// draws through the still path instead.
///
/// Cached by `path + w + h + radius`, so a `view` builds no bytes.
pub fn load(path: &str, w: u32, h: u32, radius: f32) -> Option<std::sync::Arc<Animation>> {
    let key = Key {
        path: path.to_owned(),
        w: w.max(1),
        h: h.max(1),
        radius: radius.round().max(0.0) as u32,
    };
    if let Ok(mut guard) = CACHE.lock() {
        if let Some(hit) = guard.get_or_insert_with(HashMap::new).get(&key) {
            return hit.clone();
        }
    }
    let built = decode(&key).map(std::sync::Arc::new);
    if let Ok(mut guard) = CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(key, built.clone());
    }
    built
}

fn decode(key: &Key) -> Option<Animation> {
    let data = std::fs::read(&key.path).ok()?;
    if !is_gif(&data) {
        return None;
    }
    let decoder = GifDecoder::new(std::io::Cursor::new(data)).ok()?;
    // `collect_frames` on a malformed GIF returns Err; the caller takes that
    // as "no animation", and the still path draws whatever it can decode of
    // the file instead.
    let decoded = decoder.into_frames().collect_frames().ok()?;
    if decoded.is_empty() {
        return None;
    }
    let mut frames = Vec::with_capacity(decoded.len());
    let mut delays = Vec::with_capacity(decoded.len());
    for frame in decoded {
        let delay = Duration::from(frame.delay());
        // Clamp to a sensible minimum: the GIF spec's 100 Hz headroom reads
        // as "animate as fast as possible" and some pack `delay = 0`.
        let delay = delay.max(Duration::from_millis(20));
        let buffer = frame.into_buffer();
        let resized =
            image::imageops::resize(&buffer, key.w, key.h, image::imageops::FilterType::Triangle);
        let mut rgba = resized.into_raw();
        crate::tileimage::apply_mask(&mut rgba, key.w, key.h, key.radius as f32);
        frames.push(Handle::from_rgba(key.w, key.h, rgba));
        delays.push(delay);
    }
    Some(Animation { frames, delays })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_magic_is_bytes_not_the_name() {
        assert!(is_gif(b"GIF89a\0\0rest"));
        assert!(is_gif(b"GIF87a\0\0rest"));
        assert!(!is_gif(b"\x89PNG\r\n\x1a\n"));
        assert!(!is_gif(b""));
        assert!(!is_gif(b"GIF"));
    }

    /// A minimal two-frame GIF, built via the `image` crate's own encoder
    /// — the only reliable way to hand the decoder a well-formed fixture.
    fn two_frame_gif() -> Vec<u8> {
        use image::codecs::gif::{GifEncoder, Repeat};
        use image::{Rgba, RgbaImage};
        let mut buf = Vec::new();
        {
            let mut enc = GifEncoder::new_with_speed(&mut buf, 10);
            enc.set_repeat(Repeat::Infinite).unwrap();
            for colour in [[0u8, 0, 0, 255], [255, 255, 255, 255]] {
                let mut frame = RgbaImage::new(4, 4);
                for p in frame.pixels_mut() {
                    *p = Rgba(colour);
                }
                enc.encode_frame(image::Frame::from_parts(
                    frame,
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(10, 1),
                ))
                .unwrap();
            }
        }
        buf
    }

    #[test]
    fn a_real_multi_frame_gif_decodes_through_the_cache() {
        let tmp = std::env::temp_dir().join(format!("start-menu-anim-{}.gif", std::process::id()));
        std::fs::write(&tmp, two_frame_gif()).unwrap();
        let path = tmp.to_string_lossy().into_owned();

        let a = load(&path, 16, 16, 4.0).expect("two-frame gif should decode");
        assert_eq!(a.frames.len(), 2);
        assert_eq!(a.delays.len(), 2);
        // The hand-coded 10ms delay is below the 20ms clamp.
        assert_eq!(a.delays[0], Duration::from_millis(20));

        // Second call reuses the Arc rather than re-decoding.
        let b = load(&path, 16, 16, 4.0).unwrap();
        assert!(std::sync::Arc::ptr_eq(&a, &b));
        assert!(is_animated(&path));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn a_non_gif_file_is_not_animated() {
        let tmp = std::env::temp_dir().join(format!("start-menu-png-{}.png", std::process::id()));
        std::fs::write(&tmp, b"\x89PNG\r\n\x1a\n").unwrap();
        let path = tmp.to_string_lossy().into_owned();
        assert!(load(&path, 16, 16, 0.0).is_none());
        assert!(!is_animated(&path));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn a_missing_file_is_graceful() {
        assert!(load("/does/not/exist.gif", 16, 16, 0.0).is_none());
        assert!(!is_animated("/does/not/exist.gif"));
    }
}
