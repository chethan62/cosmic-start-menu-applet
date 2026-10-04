//! Which frame of a tile's animated image to show, as a pure function of the
//! user's `motion` choice, whether the menu is open, whether the tile is
//! highlighted, and how long the clock has run.
//!
//! The animation itself is driven elsewhere — this says only what to draw.
//! Keeping the rule here means one test suite covers every combination, and
//! `src/ui/tiles.rs` never has to care about frame counts or delays.

use std::time::Duration;

/// How a tile's image plays.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    /// Play around and around while the menu is open. The default.
    #[default]
    Loop,
    /// Play through once per open, then hold the last frame.
    Once,
    /// Hold the first frame until the tile is highlighted; play while it is;
    /// return to the first frame when it is not.
    OnHighlight,
}

impl Motion {
    /// Serde skips the default so a config stays short.
    pub fn is_default(&self) -> bool {
        matches!(self, Motion::Loop)
    }
}

/// What the renderer needs to know about the surroundings right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// Whether the menu is on screen. False while the menu is closed, or while
    /// it is fading out — see `app.rs`, which stops advancing the clock there.
    pub menu_open: bool,
    /// Whether the pointer is over this tile, or the keyboard highlight is on
    /// it. Only read by `Motion::OnHighlight`.
    pub highlighted: bool,
}

/// The frame to draw now.
///
/// `frames` is how many the GIF has — one for a still — and `delays` their
/// per-frame durations in order. `elapsed` is time since the clock started
/// (open, for Loop; open or highlight transition, for the others), capped by
/// the caller so a stall does not jump the animation.
///
/// Returns `0` outside the menu's lifetime, so a tile that has never been
/// shown draws its first frame rather than a stale one.
pub fn frame(
    motion: Motion,
    ctx: Context,
    frames: usize,
    delays: &[Duration],
    elapsed: Duration,
) -> usize {
    if frames == 0 {
        return 0;
    }
    if frames == 1 || !ctx.menu_open {
        return 0;
    }
    match motion {
        Motion::OnHighlight if !ctx.highlighted => 0,
        Motion::Once => frame_at(elapsed, delays, frames).min(frames - 1),
        Motion::Loop | Motion::OnHighlight => {
            let total = total_duration(delays, frames);
            if total.is_zero() {
                return 0;
            }
            let into = elapsed.as_nanos() % total.as_nanos();
            frame_at(Duration::from_nanos(into as u64), delays, frames)
        }
    }
}

/// Whether the animation is still moving: the renderer only asks for frame
/// updates while this is true. A finished `Once`, an idle `OnHighlight` and a
/// still picture all return false and let the subscription sleep.
#[allow(dead_code)] // public helper with its own tests; the subscription
                    // currently ticks unconditionally while the popup is up,
                    // relying on iced's view diff to drop idle redraws.
pub fn is_animating(
    motion: Motion,
    ctx: Context,
    frames: usize,
    delays: &[Duration],
    elapsed: Duration,
) -> bool {
    if frames <= 1 || !ctx.menu_open {
        return false;
    }
    match motion {
        Motion::Loop => true,
        Motion::OnHighlight => ctx.highlighted,
        Motion::Once => frame_at(elapsed, delays, frames) < frames.saturating_sub(1),
    }
}

/// The frame reached after `elapsed`, walking the per-frame delays in order.
/// Capped at `frames - 1`, so a `Once` that has run out holds its last frame.
fn frame_at(elapsed: Duration, delays: &[Duration], frames: usize) -> usize {
    let mut acc = Duration::ZERO;
    for (i, d) in delays.iter().take(frames).enumerate() {
        acc += *d;
        if elapsed < acc {
            return i;
        }
    }
    frames.saturating_sub(1)
}

fn total_duration(delays: &[Duration], frames: usize) -> Duration {
    delays.iter().take(frames).copied().sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delays(count: usize, each_ms: u64) -> Vec<Duration> {
        (0..count).map(|_| Duration::from_millis(each_ms)).collect()
    }

    fn open(highlight: bool) -> Context {
        Context {
            menu_open: true,
            highlighted: highlight,
        }
    }

    #[test]
    fn a_still_is_always_frame_zero() {
        let d = delays(1, 100);
        assert_eq!(
            frame(Motion::Loop, open(false), 1, &d, Duration::from_secs(5)),
            0
        );
        assert!(!is_animating(
            Motion::Loop,
            open(false),
            1,
            &d,
            Duration::ZERO
        ));
    }

    #[test]
    fn a_closed_menu_holds_the_first_frame_whatever_motion() {
        let d = delays(4, 100);
        let closed = Context {
            menu_open: false,
            highlighted: true,
        };
        for m in [Motion::Loop, Motion::Once, Motion::OnHighlight] {
            assert_eq!(frame(m, closed, 4, &d, Duration::from_secs(10)), 0);
            assert!(!is_animating(m, closed, 4, &d, Duration::ZERO));
        }
    }

    #[test]
    fn loop_cycles_through_the_frames_and_wraps() {
        let d = delays(4, 100);
        // 0ms -> 0, 150ms -> 1, 250ms -> 2, 350ms -> 3, 450ms wraps to 0.
        assert_eq!(frame(Motion::Loop, open(false), 4, &d, Duration::ZERO), 0);
        assert_eq!(
            frame(Motion::Loop, open(false), 4, &d, Duration::from_millis(150)),
            1
        );
        assert_eq!(
            frame(Motion::Loop, open(false), 4, &d, Duration::from_millis(250)),
            2
        );
        assert_eq!(
            frame(Motion::Loop, open(false), 4, &d, Duration::from_millis(350)),
            3
        );
        assert_eq!(
            frame(Motion::Loop, open(false), 4, &d, Duration::from_millis(450)),
            0
        );
        assert!(is_animating(
            Motion::Loop,
            open(false),
            4,
            &d,
            Duration::ZERO
        ));
    }

    #[test]
    fn once_plays_through_and_holds_the_last_frame() {
        let d = delays(3, 100);
        assert_eq!(frame(Motion::Once, open(false), 3, &d, Duration::ZERO), 0);
        assert_eq!(
            frame(Motion::Once, open(false), 3, &d, Duration::from_millis(150)),
            1
        );
        assert_eq!(
            frame(Motion::Once, open(false), 3, &d, Duration::from_millis(250)),
            2
        );
        // Past the end: held, not wrapped.
        assert_eq!(
            frame(Motion::Once, open(false), 3, &d, Duration::from_secs(10)),
            2
        );
        // And no more frame updates needed once held.
        assert!(is_animating(
            Motion::Once,
            open(false),
            3,
            &d,
            Duration::from_millis(50)
        ));
        assert!(!is_animating(
            Motion::Once,
            open(false),
            3,
            &d,
            Duration::from_secs(10)
        ));
    }

    #[test]
    fn on_highlight_holds_frame_zero_until_the_tile_is_lit() {
        let d = delays(4, 100);
        // Not highlighted: frozen on 0, no updates needed.
        assert_eq!(
            frame(
                Motion::OnHighlight,
                open(false),
                4,
                &d,
                Duration::from_secs(5)
            ),
            0
        );
        assert!(!is_animating(
            Motion::OnHighlight,
            open(false),
            4,
            &d,
            Duration::ZERO
        ));
        // Highlighted: plays, needs updates.
        assert_eq!(
            frame(
                Motion::OnHighlight,
                open(true),
                4,
                &d,
                Duration::from_millis(150)
            ),
            1
        );
        assert!(is_animating(
            Motion::OnHighlight,
            open(true),
            4,
            &d,
            Duration::ZERO
        ));
    }

    #[test]
    fn uneven_frame_delays_land_on_the_right_frame() {
        // A real GIF has per-frame delays; the sampler walks them in order.
        let d = vec![
            Duration::from_millis(40),
            Duration::from_millis(200),
            Duration::from_millis(40),
        ];
        assert_eq!(
            frame(Motion::Once, open(false), 3, &d, Duration::from_millis(20)),
            0
        );
        assert_eq!(
            frame(Motion::Once, open(false), 3, &d, Duration::from_millis(60)),
            1
        );
        // Still on 1 at 200ms (frame 1 ends at 240ms).
        assert_eq!(
            frame(Motion::Once, open(false), 3, &d, Duration::from_millis(200)),
            1
        );
        assert_eq!(
            frame(Motion::Once, open(false), 3, &d, Duration::from_millis(250)),
            2
        );
    }

    #[test]
    fn zero_duration_frames_do_not_hang_the_loop() {
        // A broken GIF with all-zero delays would otherwise divide by zero.
        let d = delays(3, 0);
        assert_eq!(
            frame(Motion::Loop, open(false), 3, &d, Duration::from_millis(100)),
            0
        );
    }
}
