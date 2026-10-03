//! The dominant brand colour of an app's icon, for a solid tile fill.
//!
//! An icon is rasterised to a small RGBA grid, its coloured pixels are
//! bucketed by hue, and the heaviest bucket's average colour — nudged a
//! little brighter and more saturated — is the tile's fill. A flat
//! monochrome glyph (a plain terminal, a grey cog) carries no brand colour
//! and gives `None`, so those tiles keep the theme's finish rather than being
//! forced into a muddy tint.
//!
//! Extraction is file I/O plus a decode, so it runs off the render path —
//! `App::brand` is filled by the background load, never inside `view` — and a
//! per-path cache means a reload re-rasterises nothing it has already seen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::apps::IconSource;

/// The grid an icon is reduced to before sampling. Small enough to extract in
/// well under a millisecond, large enough that a modest logo still lands
/// enough pixels to win its bucket.
const RASTER: u32 = 64;

/// Hue is split into this many buckets. 18 is 20 degrees each: wide enough
/// that anti-aliased edge pixels of one logo colour stay together, narrow
/// enough to separate two real brand hues.
const BUCKETS: usize = 18;

/// A pixel joins the sampling only if it is colourful enough: its RGB spread
/// clears this, it is not too dark, and not too light. Greys, near-black
/// outlines and near-white highlights carry no brand hue.
const MIN_CHROMA: i32 = 28;
const MIN_MAX: u8 = 40;
const MAX_MIN: u8 = 225;
/// A pixel below this alpha is transparent enough to ignore.
const MIN_ALPHA: u8 = 128;

/// The brand colour of a straight (non-premultiplied) RGBA buffer, or `None`
/// when nothing in it carries real colour.
///
/// `rgba` is a run of `[r, g, b, a]` bytes; its dimensions do not matter, only
/// the pixels. Pure, so the whole rule is a unit test over known pixels.
pub fn extract(rgba: &[u8]) -> Option<[u8; 3]> {
    let mut sum = [[0f64; 3]; BUCKETS];
    let mut weight = [0f64; BUCKETS];
    for px in rgba.chunks_exact(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        if a < MIN_ALPHA {
            continue;
        }
        let mx = r.max(g).max(b);
        let mn = r.min(g).min(b);
        if i32::from(mx) - i32::from(mn) < MIN_CHROMA || mx < MIN_MAX || mn > MAX_MIN {
            continue;
        }
        let (h, s, v) = rgb_to_hsv(r, g, b);
        let w = f64::from(s * v * (f32::from(a) / 255.0));
        // `h` is 0..1; at exactly 1.0 it wraps to bucket 0, as red should.
        let bucket = ((h * BUCKETS as f32) as usize) % BUCKETS;
        sum[bucket][0] += f64::from(r) * w;
        sum[bucket][1] += f64::from(g) * w;
        sum[bucket][2] += f64::from(b) * w;
        weight[bucket] += w;
    }
    let best = (0..BUCKETS).max_by(|&a, &b| weight[a].total_cmp(&weight[b]))?;
    if weight[best] <= 0.0 {
        return None;
    }
    let avg = |c: f64| (c / weight[best]) as f32 / 255.0;
    let (h, mut s, mut v) = rgb_to_hsv_f(avg(sum[best][0]), avg(sum[best][1]), avg(sum[best][2]));
    // A touch bolder than the icon's average: the fill is a background the
    // full-colour icon sits on, and the raw average reads a little washed.
    s = (s * 1.15).min(1.0);
    v = v.max(0.55);
    let (r, g, b) = hsv_to_rgb(h, s, v);
    Some([to_u8(r), to_u8(g), to_u8(b)])
}

fn to_u8(c: f32) -> u8 {
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// The brand colour of an app's icon, resolved through the normal
/// freedesktop lookup — the same icon the menu draws — and cached by the
/// file it lands on, so two apps sharing an icon rasterise it once and a
/// reload rasterises nothing already seen.
pub fn of_icon(source: &IconSource) -> Option<[u8; 3]> {
    static CACHE: Mutex<Option<HashMap<PathBuf, Option<[u8; 3]>>>> = Mutex::new(None);
    let path = resolve(source)?;
    if let Ok(mut guard) = CACHE.lock() {
        if let Some(hit) = guard.get_or_insert_with(HashMap::new).get(&path) {
            return *hit;
        }
    }
    let colour = rasterize(&path).and_then(|rgba| extract(&rgba));
    if let Ok(mut guard) = CACHE.lock() {
        guard.get_or_insert_with(HashMap::new).insert(path, colour);
    }
    colour
}

/// The icon file the menu would draw for `source`: a themed name goes through
/// the freedesktop lookup at the same size and SVG preference as the display
/// icon, a path is taken as is.
fn resolve(source: &IconSource) -> Option<PathBuf> {
    match source {
        IconSource::Name(name) => cosmic::widget::icon::from_name(name.as_str())
            .size(128)
            .prefer_svg(true)
            .path(),
        IconSource::Path(path) => Some(path.clone()),
    }
}

/// Rasterise an icon file to straight RGBA at [`RASTER`] square. SVG goes
/// through usvg/resvg (the user's Hatter theme is scalable); everything else
/// through the image decoder already in the tree.
fn rasterize(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    if is_svg(path, &data) {
        rasterize_svg(&data)
    } else {
        rasterize_raster(&data)
    }
}

fn is_svg(path: &Path, data: &[u8]) -> bool {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        return true;
    }
    let head = &data[..data.len().min(512)];
    let head = String::from_utf8_lossy(head);
    head.contains("<svg") || head.contains("<?xml")
}

fn rasterize_svg(data: &[u8]) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = RASTER as f32 / size.width().max(size.height());
    let mut pixmap = resvg::tiny_skia::Pixmap::new(RASTER, RASTER)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(demultiply(pixmap.data()))
}

fn rasterize_raster(data: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(data).ok()?;
    // A thumbnail keeps the aspect ratio: cropping is pointless when only the
    // colours matter, and a stretch would not change which hue wins.
    Some(img.thumbnail(RASTER, RASTER).to_rgba8().into_raw())
}

/// Un-premultiply tiny-skia's RGBA so the sampler sees true pixel colours.
fn demultiply(premultiplied: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(premultiplied.len());
    for px in premultiplied.chunks_exact(4) {
        let a = px[3];
        if a == 0 || a == 255 {
            out.extend_from_slice(px);
            continue;
        }
        let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
        out.extend_from_slice(&[un(px[0]), un(px[1]), un(px[2]), a]);
    }
    out
}

/// `colorsys.rgb_to_hsv`, with `h`, `s`, `v` all in 0..1.
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    rgb_to_hsv_f(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    )
}

fn rgb_to_hsv_f(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let v = maxc;
    if (maxc - minc).abs() < f32::EPSILON {
        return (0.0, 0.0, v);
    }
    let d = maxc - minc;
    let s = d / maxc;
    let (rc, gc, bc) = ((maxc - r) / d, (maxc - g) / d, (maxc - b) / d);
    let h = if (r - maxc).abs() < f32::EPSILON {
        bc - gc
    } else if (g - maxc).abs() < f32::EPSILON {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    ((h / 6.0).rem_euclid(1.0), s, v)
}

/// `colorsys.hsv_to_rgb`, inputs and outputs in 0..1.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer of `n` pixels of one colour.
    fn fill(rgba: [u8; 4], n: usize) -> Vec<u8> {
        rgba.iter().copied().cycle().take(n * 4).collect()
    }

    fn hue_deg(c: [u8; 3]) -> f32 {
        rgb_to_hsv(c[0], c[1], c[2]).0 * 360.0
    }

    #[test]
    fn the_heaviest_hue_bucket_wins() {
        let mut buf = fill([20, 110, 215, 255], 100); // blue, lots
        buf.extend(fill([230, 60, 50, 255], 20)); // red, a little
        let c = extract(&buf).unwrap();
        // The winner is a blue: hue near 210 degrees, not the red at ~5.
        assert!(
            (190.0..235.0).contains(&hue_deg(c)),
            "{c:?} -> {}",
            hue_deg(c)
        );
    }

    #[test]
    fn greys_dark_and_light_pixels_are_skipped() {
        // Mostly flat grey (no chroma), near-black, near-white — all ignored —
        // with a small island of orange that must still be found.
        let mut buf = fill([128, 128, 128, 255], 200);
        buf.extend(fill([10, 10, 10, 255], 50));
        buf.extend(fill([250, 250, 250, 255], 50));
        buf.extend(fill([240, 140, 20, 255], 15));
        let c = extract(&buf).unwrap();
        assert!(
            (20.0..45.0).contains(&hue_deg(c)),
            "{c:?} -> {}",
            hue_deg(c)
        );
    }

    #[test]
    fn a_flat_monochrome_glyph_has_no_brand_colour() {
        // A grey terminal glyph on transparency: nothing qualifies.
        let mut buf = fill([0, 0, 0, 0], 300);
        buf.extend(fill([200, 200, 200, 255], 80));
        assert_eq!(extract(&buf), None);
        assert_eq!(extract(&[]), None);
    }

    #[test]
    fn the_fill_is_bolder_than_a_washed_out_average() {
        // A pale, desaturated blue is pushed to at least mid brightness and
        // a little more saturated, so the tile reads as a brand colour.
        let pale = [150, 170, 200, 255];
        let c = extract(&fill(pale, 50)).unwrap();
        let (_, s0, _) = rgb_to_hsv(pale[0], pale[1], pale[2]);
        let (_, s1, v1) = rgb_to_hsv(c[0], c[1], c[2]);
        assert!(v1 >= 0.55, "value {v1} not lifted");
        assert!(s1 >= s0, "saturation {s1} below {s0}");
    }

    #[test]
    fn hsv_round_trips() {
        for (r, g, b) in [(18, 109, 225), (210, 46, 76), (0, 174, 66), (243, 102, 0)] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let (r2, g2, b2) = hsv_to_rgb(h, s, v);
            assert!((to_u8(r2) as i32 - r as i32).abs() <= 1);
            assert!((to_u8(g2) as i32 - g as i32).abs() <= 1);
            assert!((to_u8(b2) as i32 - b as i32).abs() <= 1);
        }
    }

    /// Resolves the user's real icons and prints the extracted colours, for
    /// checking against the expected brand list. Ignored by default: it reads
    /// the live icon theme, which only exists on the user's machine.
    #[test]
    #[ignore]
    fn dump_real_icon_colours() {
        cosmic::icon_theme::set_default(cosmic::config::icon_theme());
        println!("icon theme: {}", cosmic::config::icon_theme());
        for name in [
            "com.bitwarden.desktop",
            "com.bambulab.BambuStudio",
            "0ad",
            "localsend",
            "Alacritty",
            "org.cachyos.hello",
            "claude-desktop",
            "com.system76.CosmicTerm",
            "openttd",
        ] {
            let src = IconSource::from_unknown(name);
            let got = of_icon(&src);
            let hex = got.map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
            println!("{name}: {hex:?} (resolved {:?})", resolve(&src));
        }
    }
}
