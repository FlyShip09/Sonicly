//! The opening: Sonicly's mark settling in the middle of the page as the process starts, and
//! then giving way to the shell, which arrives in a short stagger. The real interface is built
//! under it from the first frame, so nothing waits on it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{App, Div, Global, Pixels, Stateful, div, px};
use i18n::t;
use ui::{Theme, ease_out_cubic};

/// One unit of the mark's 14-unit grid, the size of `assets/icon.svg`'s bars on screen.
const UNIT: Pixels = px(5.);
/// Where each bar of the mark starts and ends on that grid, top to bottom, left to right.
const BARS: [(f32, f32); 5] = [(3., 8.), (0., 8.), (2., 12.), (6., 14.), (6., 11.)];
/// How far apart, in units, the bars stand, and how wide each one is.
const PITCH: f32 = 3.;
const WIDTH: f32 = 2.;
const GRID: f32 = 14.;
/// How long after one bar the next starts to rise.
const STAGGER: Duration = Duration::from_millis(55);
/// How long one bar takes to rise and settle, and how far past its height it swings.
const RISE: f32 = 0.42;
const BOUNCE: f32 = 0.22;
/// When the mark reads as settled: the last bar has all but landed.
const SETTLED: Duration = Duration::from_millis(560);
/// The longest the mark stands in for the app. Past it the shell comes regardless and each
/// page shows its own loading state.
const HOLD: Duration = Duration::from_secs(3);
/// While the app is still getting ready, the bars move like a quiet level meter: how long one
/// swing takes and how much of each bar it moves.
const BREATH: f32 = 1.3;
const DEPTH: f32 = 0.16;
/// How the mark leaves: it shrinks by this much while it fades.
const LEAVE: Duration = Duration::from_millis(220);
const SHRINK: f32 = 0.12;
/// How the shell arrives once the mark leaves: the sidebar, then the page, then the player
/// bar, each this far behind the one before, travelling `TRAVEL` while it fades in.
const SHELL_STAGGER: Duration = Duration::from_millis(60);
const SHELL_SPAN: Duration = Duration::from_millis(240);
const TRAVEL: Pixels = px(8.);

/// Set once the opening has played, so a window opened again from the tray comes plainly.
static PLAYED: AtomicBool = AtomicBool::new(false);

/// When the shell started arriving, read by the shells to stagger themselves in.
struct Arrival(Instant);

impl Global for Arrival {}

/// Holds the app's readiness back until a moment, for a debug run that wants to see the mark
/// waiting.
struct Held(Instant);

impl Global for Held {}

/// The opening while it plays. Root steps it every frame and drops it once the shell is in.
pub(crate) struct Opening {
    start: Instant,
    leaving: Option<Instant>,
}

/// What one frame of the opening draws.
pub(crate) struct Frame {
    /// Each bar's height as a share of its own.
    bars: [f32; 5],
    /// How much of the mark and of the page colour over the shell still shows.
    shown: f32,
    /// The mark's size as a share of its own.
    scale: f32,
    /// Whether the shell is hidden under the page colour, which then takes the pointer.
    covering: bool,
}

impl Opening {
    /// The opening of a process that has not shown one yet, unless motion is reduced.
    pub(crate) fn begin(cx: &App) -> Option<Self> {
        if PLAYED.swap(true, Ordering::Relaxed) || cx.reduce_motion() {
            return None;
        }
        Some(Self {
            start: Instant::now(),
            leaving: None,
        })
    }

    /// Steps the opening to `now`. The mark leaves once it has settled and the app is `ready`,
    /// or once it has waited `HOLD`. Nothing once the mark has gone.
    pub(crate) fn step(&mut self, ready: bool, cx: &mut App) -> Option<Frame> {
        let now = Instant::now();
        let age = now - self.start;
        let ready = ready && !cx.try_global::<Held>().is_some_and(|held| now < held.0);
        if self.leaving.is_none() && ((ready && age >= SETTLED) || age >= HOLD) {
            self.leaving = Some(now);
            cx.set_global(Arrival(now));
        }
        let bars = std::array::from_fn(|index| bar(age, index));
        let Some(leaving) = self.leaving else {
            return Some(Frame {
                bars,
                shown: 1.,
                scale: 1.,
                covering: true,
            });
        };
        let gone = (now - leaving).as_secs_f32() / LEAVE.as_secs_f32();
        if gone >= 1. {
            return None;
        }
        let left = ease_out_cubic(gone);
        Some(Frame {
            bars,
            shown: 1. - left,
            scale: 1. - SHRINK * left,
            covering: false,
        })
    }
}

/// Holds the opening's mark up until `until`, for a debug run.
pub fn hold_opening(until: Instant, cx: &mut App) {
    cx.set_global(Held(until));
}

/// How far each part of the shell has arrived, sidebar, page and player bar, from 0 to 1, while
/// the opening hands over to it, or nothing once all three are in.
pub(crate) fn arrival(cx: &App) -> Option<[f32; 3]> {
    let since = cx.try_global::<Arrival>()?.0.elapsed();
    let parts: [f32; 3] = std::array::from_fn(|place| {
        let delay = SHELL_STAGGER * place as u32;
        let into = since.saturating_sub(delay).as_secs_f32() / SHELL_SPAN.as_secs_f32();
        ease_out_cubic(into.min(1.))
    });
    (parts[2] < 1.).then_some(parts)
}

/// Moves a part of the shell `TRAVEL` along as it arrives and fades it in. `across` moves it
/// from the left, otherwise it rises from below.
pub(crate) fn arriving(part: Div, arrived: f32, across: bool) -> Div {
    let away = TRAVEL * (1. - arrived);
    let part = part.relative().opacity(arrived);
    match across {
        true => part.left(-away),
        false => part.top(away),
    }
}

/// The page colour over the window with the mark in its middle. While it covers the shell it
/// takes the pointer, and screen readers hear that the app is starting.
pub(crate) fn overlay(frame: &Frame, theme: &Theme) -> Stateful<Div> {
    let unit = UNIT * frame.scale;
    // the mark goes ahead of the page colour, so it never lies faintly over the shell
    let ink = theme.foreground.opacity(frame.shown * frame.shown);
    let mark = div()
        .relative()
        .flex_none()
        .w(unit * GRID)
        .h(unit * GRID)
        .children(BARS.iter().zip(frame.bars).enumerate().map(
            |(place, (&(top, bottom), share))| {
                // a bar is never shorter than it is wide, so it starts as a dot
                let tall = ((bottom - top) * share).max(WIDTH);
                div()
                    .absolute()
                    .left(unit * (PITCH * place as f32))
                    .top(unit * (bottom - tall))
                    .w(unit * WIDTH)
                    .h(unit * tall)
                    .rounded_full()
                    .bg(ink)
                    .when(share <= 0., |bar| bar.invisible())
            },
        ));

    div()
        .id("opening")
        .role(Role::ProgressIndicator)
        .aria_label(t!("opening-busy"))
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.background.opacity(frame.shown))
        .when(frame.covering, |this| this.occlude())
        .child(mark)
}

/// How tall bar `index` stands at `age`, as a share of its height: rising on a spring after
/// its turn, then moving gently once the mark has settled, for as long as the app is not ready.
fn bar(age: Duration, index: usize) -> f32 {
    let since = age.saturating_sub(STAGGER * index as u32).as_secs_f32();
    let risen = settle(since, RISE, BOUNCE);
    let waiting = age.saturating_sub(SETTLED).as_secs_f32();
    let swing = (waiting / BREATH).min(1.);
    let phase = std::f32::consts::TAU * (waiting / BREATH) + index as f32 * 1.3;
    risen * (1. - DEPTH * swing * (0.5 - 0.5 * phase.cos()))
}

/// Where a spring that takes `duration` seconds and overshoots by `bounce` stands `time`
/// seconds after it was let go from 0 towards 1.
fn settle(time: f32, duration: f32, bounce: f32) -> f32 {
    if time <= 0. {
        return 0.;
    }
    let omega = std::f32::consts::TAU / duration;
    let damping = 1. - bounce;
    let decay = (-damping * omega * time).exp();
    let ringing = omega * (1. - damping * damping).sqrt();
    1. - decay * ((ringing * time).cos() + damping * omega / ringing * (ringing * time).sin())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{BOUNCE, RISE, bar, settle};

    #[test]
    fn a_bar_rises_past_its_height_and_settles_on_it() {
        assert_eq!(settle(0., RISE, BOUNCE), 0.);
        let peak = (1..100)
            .map(|step| settle(step as f32 * 0.01, RISE, BOUNCE))
            .fold(0., f32::max);
        assert!(peak > 1. && peak < 1.2, "{peak}");
        assert!((settle(2., RISE, BOUNCE) - 1.).abs() < 0.01);
    }

    #[test]
    fn bars_rise_in_turn() {
        let early = Duration::from_millis(60);
        assert!(bar(early, 0) > 0.);
        assert_eq!(bar(early, 4), 0.);
    }
}
