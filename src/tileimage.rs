//! A tile's background picture, cover-cropped to the tile and masked to its
//! rounded corners.
//!
//! iced's `clip(true)` is a rectangular scissor, so a picture under a
//! rounded-corner button showed square corners poking past the tile's radius.
//! The fix is to bake the rounding into the image's own alpha: cover-crop it
//! to the tile and multiply each pixel by a rounded-rectangle coverage mask,
//! so the corners simply are not drawn.
//!
//! Decoding and masking are cached by the picture and the exact size and
//! radius asked for, so a frame reuses the handle rather than rebuilding it.

use std::collections::HashMap;
use std::sync::Mutex;

use cosmic::widget::image::Handle;

/// What a cached mask is keyed by: the file, the tile pixel size, and the
/// corner radius (rounded to a whole pixel, which is all a mask can show).
#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    path: String,
    w: u32,
    h: u32,
    radius: u32,
}

/// The picture at `path`, cover-cropped to `w`x`h` and masked to a `radius`
/// rounded rectangle, as RGBA ready for the image widget. `None` if the file
/// cannot be decoded, in which case the caller falls back to the raw path.
pub fn masked(path: &str, w: f32, h: f32, radius: f32) -> Option<Handle> {
    let key = Key {
        path: path.to_owned(),
        w: w.round().max(1.0) as u32,
        h: h.round().max(1.0) as u32,
        radius: radius.round().max(0.0) as u32,
    };
    static CACHE: Mutex<Option<HashMap<Key, Handle>>> = Mutex::new(None);
    if let Ok(mut guard) = CACHE.lock() {
        if let Some(hit) = guard.get_or_insert_with(HashMap::new).get(&key) {
            return Some(hit.clone());
        }
    }
    let handle = build(&key)?;
    if let Ok(mut guard) = CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(key, handle.clone());
    }
    Some(handle)
}

fn build(key: &Key) -> Option<Handle> {
    let img = image::ImageReader::open(&key.path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    // Cover-crop: fill the tile, keeping aspect, cropping the overflow — the
    // same `ContentFit::Cover` the old rectangular picture used.
    let mut rgba = img
        .resize_to_fill(key.w, key.h, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (w, h) = (key.w as f32, key.h as f32);
    let r = (key.radius as f32).min(w / 2.0).min(h / 2.0);
    for (x, y, px) in rgba.enumerate_pixels_mut() {
        let cover = coverage(x as f32 + 0.5, y as f32 + 0.5, w, h, r);
        px.0[3] = (f32::from(px.0[3]) * cover).round() as u8;
    }
    Some(Handle::from_rgba(key.w, key.h, rgba.into_raw()))
}

/// How much of the pixel centred at `(px, py)` lies inside a `w`x`h`
/// rectangle with `r` rounded corners: 1 well inside, 0 well outside, a soft
/// edge of about a pixel between. A signed distance to the rounded rect, read
/// as coverage, which anti-aliases the curve.
fn coverage(px: f32, py: f32, w: f32, h: f32, r: f32) -> f32 {
    // How far the point reaches into a corner zone on each axis; 0 along the
    // straight edges, where the picture fills the tile and stays opaque.
    let dx = (r - px).max(px - (w - r)).max(0.0);
    let dy = (r - py).max(py - (h - r)).max(0.0);
    // Along a straight edge or in the interior the picture fills the tile, so
    // only the corner arcs — where both axes reach past the radius — are
    // rounded and anti-aliased. This also keeps a square (radius 0) tile's
    // corners intact.
    if dx == 0.0 || dy == 0.0 {
        return 1.0;
    }
    let dist = (dx * dx + dy * dy).sqrt() - r;
    (0.5 - dist).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_edges_stay_opaque_and_corners_are_cut() {
        let (w, h, r) = (100.0, 60.0, 16.0);
        // Centre, and the middle of each straight edge: fully covered.
        for (x, y) in [
            (50.0, 30.0),
            (50.0, 0.5),
            (50.0, 59.5),
            (0.5, 30.0),
            (99.5, 30.0),
        ] {
            assert_eq!(coverage(x, y, w, h, r), 1.0, "({x},{y})");
        }
        // The extreme corner pixels: well outside the arc, fully cut.
        for (x, y) in [(0.5, 0.5), (99.5, 0.5), (0.5, 59.5), (99.5, 59.5)] {
            assert_eq!(coverage(x, y, w, h, r), 0.0, "({x},{y})");
        }
    }

    #[test]
    fn the_corner_edge_is_antialiased_not_a_hard_step() {
        // Walking out along the diagonal of a corner crosses a soft band
        // rather than jumping 1 to 0 in one pixel.
        let (w, h, r) = (80.0, 80.0, 20.0);
        let soft = (0..30)
            .map(|i| {
                let d = i as f32;
                coverage(20.0 - d * 0.7, 20.0 - d * 0.7, w, h, r)
            })
            .filter(|c| *c > 0.0 && *c < 1.0)
            .count();
        assert!(soft >= 1, "corner edge had no partial-coverage pixels");
    }

    #[test]
    fn a_zero_radius_keeps_every_corner() {
        for (x, y) in [(0.5, 0.5), (99.5, 0.5), (0.5, 59.5), (99.5, 59.5)] {
            assert_eq!(coverage(x, y, 100.0, 60.0, 0.0), 1.0, "({x},{y})");
        }
    }
}
