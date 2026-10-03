//! How the menu moves when it opens and closes: a fade with a short slide,
//! as pure arithmetic over elapsed time.
//!
//! One eased value drives both halves, so the card's opacity and its distance
//! from rest always agree: it rises the last few pixels as it becomes solid,
//! and sinks back as it fades. The surface itself carries the result — the
//! opacity through `wp_alpha_modifier_v1`, the slide through the layer
//! surface's margin — so nothing here knows about Wayland or iced, and every
//! rule is a unit test.

use std::time::Duration;

/// How far from its resting place the card starts, and finishes, in pixels:
/// enough to read as movement, little enough to read as settling rather than
/// travelling.
pub const SLIDE_PX: i32 = 12;
/// Opening is a touch slower than closing: arriving should feel deliberate,
/// leaving should get out of the way.
pub const OPEN: Duration = Duration::from_millis(160);
pub const CLOSE: Duration = Duration::from_millis(120);
/// The most one step may advance the motion, whatever the real gap.
///
/// The first frame after the content appears is the expensive one — text is
/// shaped and icons uploaded — and its gap can be several frames long. Taken
/// at face value it threw the card a third of the way up in one frame; capped
/// at one frame of a 60 Hz display, a slow frame merely holds the motion back
/// by the time it lost.
pub const MAX_STEP: Duration = Duration::from_millis(17);

/// Which way the card is moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Opening,
    Closing,
}

/// Where a motion is: its direction, and how far along it is in time, from
/// 0 to 1. Eased into opacity and offset by [`Motion::frame`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    phase: Phase,
    progress: f32,
}

/// What one step puts on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The surface's opacity, 0 (invisible) to 1.
    pub opacity: f32,
    /// How far the card sits from its resting place, towards the edge it is
    /// anchored to, in whole pixels — the layer surface's margin moves in
    /// whole pixels, and rounding here keeps the steps the caller sends equal
    /// to the steps the card actually takes.
    pub offset: i32,
    /// The motion has arrived: fully shown at rest, or fully gone.
    pub done: bool,
}

impl Motion {
    /// From invisible and low, rising into place.
    pub fn opening() -> Self {
        Self {
            phase: Phase::Opening,
            progress: 0.0,
        }
    }

    /// From fully shown at rest, sinking away.
    pub fn closing() -> Self {
        Self {
            phase: Phase::Closing,
            progress: 0.0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// What the motion shows now, without moving it.
    pub fn frame(&self) -> Frame {
        let opacity = opacity(self.phase, self.progress);
        Frame {
            opacity,
            offset: offset(opacity),
            done: self.progress >= 1.0,
        }
    }

    /// Move the motion on by `elapsed`, at most [`MAX_STEP`], and return what
    /// it shows.
    pub fn advance(&mut self, elapsed: Duration) -> Frame {
        let step = elapsed.min(MAX_STEP);
        let total = match self.phase {
            Phase::Opening => OPEN,
            Phase::Closing => CLOSE,
        };
        self.progress = (self.progress + step.as_secs_f32() / total.as_secs_f32()).min(1.0);
        self.frame()
    }

    /// The same card turned round mid-flight: a press during the fade-out
    /// brings it back up, and one during the fade-in sends it away again.
    ///
    /// The two directions use different curves, so the new progress is found
    /// by inverting the new curve at the opacity on screen now. Restarting the
    /// other curve from its own beginning instead made the card blink — fully
    /// solid for a frame on the way down, or gone on the way up.
    pub fn reversed(self) -> Self {
        let opacity = self.frame().opacity;
        match self.phase {
            Phase::Opening => Self {
                phase: Phase::Closing,
                progress: closing_progress_at(opacity),
            },
            Phase::Closing => Self {
                phase: Phase::Opening,
                progress: opening_progress_at(opacity),
            },
        }
    }
}

/// Ease-out to open (fast start, gentle landing); ease-in to close (gentle
/// start, quick exit).
fn opacity(phase: Phase, progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    match phase {
        Phase::Opening => 1.0 - (1.0 - p).powi(3),
        Phase::Closing => 1.0 - p * p,
    }
}

/// The inverse of the opening curve.
fn opening_progress_at(opacity: f32) -> f32 {
    1.0 - (1.0 - opacity.clamp(0.0, 1.0)).cbrt()
}

/// The inverse of the closing curve.
fn closing_progress_at(opacity: f32) -> f32 {
    (1.0 - opacity.clamp(0.0, 1.0)).sqrt()
}

fn offset(opacity: f32) -> i32 {
    (SLIDE_PX as f32 * (1.0 - opacity)).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One 60 Hz frame.
    const FRAME: Duration = Duration::from_micros(16_667);

    /// Run a motion frame by frame until it arrives, returning every frame.
    fn run(mut m: Motion) -> Vec<Frame> {
        let mut out = vec![m.frame()];
        for _ in 0..1000 {
            let f = m.advance(FRAME);
            out.push(f);
            if f.done {
                return out;
            }
        }
        panic!("the motion never arrived");
    }

    #[test]
    fn an_opening_starts_hidden_and_low_and_lands_solid_in_place() {
        let first = Motion::opening().frame();
        assert_eq!(first.opacity, 0.0);
        assert_eq!(first.offset, SLIDE_PX);
        assert!(!first.done);
        let last = *run(Motion::opening()).last().unwrap();
        assert_eq!(last.opacity, 1.0);
        assert_eq!(last.offset, 0);
        assert!(last.done);
    }

    #[test]
    fn a_close_starts_solid_in_place_and_ends_gone_and_low() {
        let first = Motion::closing().frame();
        assert_eq!((first.opacity, first.offset), (1.0, 0));
        let last = *run(Motion::closing()).last().unwrap();
        assert_eq!(last.opacity, 0.0);
        assert_eq!(last.offset, SLIDE_PX);
        assert!(last.done);
    }

    #[test]
    fn each_direction_takes_its_own_time() {
        // Ten 60 Hz frames for 160 ms, eight for 120 ms: rounding to a whole
        // frame either way.
        assert_eq!(run(Motion::opening()).len() - 1, 10);
        assert_eq!(run(Motion::closing()).len() - 1, 8);
    }

    /// Advance in millisecond steps, under the per-step cap.
    fn after(mut m: Motion, elapsed: Duration) -> Frame {
        for _ in 0..elapsed.as_millis() {
            m.advance(Duration::from_millis(1));
        }
        m.frame()
    }

    #[test]
    fn opening_eases_out_and_closing_eases_in() {
        let half = after(Motion::opening(), OPEN / 2);
        // Most of the way there by half time: a fast start that lands softly.
        assert!(half.opacity > 0.8, "{half:?}");
        let half = after(Motion::closing(), CLOSE / 2);
        // Barely begun by half time: a gentle start that leaves quickly.
        assert!(half.opacity > 0.7, "{half:?}");
        // Both move steadily one way: never back, never a jump of the slide.
        for frames in [run(Motion::opening()), run(Motion::closing())] {
            for pair in frames.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                assert!(
                    (b.offset - a.offset).abs() <= SLIDE_PX / 2,
                    "{a:?} -> {b:?}"
                );
            }
        }
        for pair in run(Motion::opening()).windows(2) {
            assert!(pair[1].opacity >= pair[0].opacity);
            assert!(pair[1].offset <= pair[0].offset);
        }
        for pair in run(Motion::closing()).windows(2) {
            assert!(pair[1].opacity <= pair[0].opacity);
            assert!(pair[1].offset >= pair[0].offset);
        }
    }

    #[test]
    fn a_slow_frame_moves_the_motion_by_one_frame_at_most() {
        let mut slow = Motion::opening();
        let mut fast = Motion::opening();
        // The first content frame took half a second to render.
        assert_eq!(
            slow.advance(Duration::from_millis(500)),
            fast.advance(MAX_STEP)
        );
        assert!(!slow.frame().done);
    }

    #[test]
    fn a_zero_step_shows_the_same_frame_again() {
        // How the clock is started: the first step after the content appears
        // moves nothing, so the motion begins from the frame that was ready.
        let mut m = Motion::opening();
        let before = m.frame();
        assert_eq!(m.advance(Duration::ZERO), before);
    }

    #[test]
    fn turning_round_mid_flight_never_jumps() {
        for steps in 0..12 {
            for start in [Motion::opening(), Motion::closing()] {
                let mut m = start;
                for _ in 0..steps {
                    m.advance(FRAME);
                }
                let before = m.frame();
                let turned = m.reversed();
                let after = turned.frame();
                assert_ne!(turned.phase(), m.phase());
                assert!(
                    (before.opacity - after.opacity).abs() < 1e-4,
                    "{steps} steps: {before:?} -> {after:?}"
                );
                assert!((before.offset - after.offset).abs() <= 1);
            }
        }
    }

    #[test]
    fn a_press_during_the_fade_out_brings_the_card_back_up() {
        let mut m = Motion::closing();
        m.advance(FRAME);
        m.advance(FRAME);
        let low = m.frame().opacity;
        let mut back = m.reversed();
        let next = back.advance(FRAME);
        // Up from where it was, not from nothing and not from full.
        assert!(
            next.opacity > low && next.opacity < 1.0,
            "{low} -> {next:?}"
        );
        let last = *run(back).last().unwrap();
        assert_eq!((last.opacity, last.offset), (1.0, 0));
    }

    #[test]
    fn a_finished_motion_turns_round_from_its_end() {
        let mut gone = Motion::closing();
        while !gone.advance(FRAME).done {}
        let again = gone.reversed();
        assert_eq!(again.frame().opacity, 0.0);
        assert_eq!(again.frame().offset, SLIDE_PX);
    }
}
