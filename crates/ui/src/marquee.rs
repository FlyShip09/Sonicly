use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, ElementId, HoverListenerMode, Pixels, Stateful, StyleRefinement, Window,
    canvas, div, px,
};

use crate::motion::{
    MARQUEE_DELAY, MARQUEE_PAUSE, MARQUEE_SPEED, Motion, animates, ease_in_out_cubic, paced,
};
use crate::veil::effects;

/// How wide the fade is at an edge the line runs on past.
const FADE: Pixels = px(16.);
/// How far the line has to overrun its box before it counts as clipped at all.
const SLACK: Pixels = px(0.5);

/// One line, such as a title, that a narrow box would cut short. At rest it is clipped with a
/// soft fade where it runs on. Once the pointer has rested on it for a moment it scrolls once to
/// show its end, pauses there and comes back, then holds still for as long as the pointer stays.
/// A line that fits never moves, the pointer leaving puts it back at once, and nothing runs while
/// the pointer is away, so it costs no frames at idle. It draws no node of its own for screen
/// readers; the element around it names what it says.
#[derive(IntoElement)]
pub struct Marquee {
    base: Stateful<Div>,
    id: ElementId,
    child: AnyElement,
}

/// When the pointer arrived, and how wide the box and the line last laid out.
struct Run {
    since: Option<Instant>,
    room: Rc<Cell<Pixels>>,
    line: Rc<Cell<Pixels>>,
}

impl Marquee {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            child: child.into_any_element(),
        }
    }
}

impl Styled for Marquee {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Marquee {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            id,
            child,
        } = self;
        let state = window.use_keyed_state((id, "marquee"), cx, |_, _| Run {
            since: None,
            room: Rc::default(),
            line: Rc::default(),
        });
        let (since, room, line) = {
            let run = state.read(cx);
            (run.since, run.room.clone(), run.line.clone())
        };
        let overrun = (line.get() - room.get()).max(Pixels::ZERO);
        let shift = match (since, overrun > SLACK && animates(cx)) {
            (Some(since), true) => {
                let (shift, moving) = travel(since, overrun);
                if moving {
                    window.request_animation_frame();
                }
                shift
            }
            _ => Pixels::ZERO,
        };
        let (leading, trailing) = match overrun > SLACK {
            true => (
                FADE * (shift / FADE).min(1.),
                FADE * ((overrun - shift) / FADE).min(1.),
            ),
            false => (Pixels::ZERO, Pixels::ZERO),
        };
        let overrides = std::mem::take(base.style());
        let hover = state;

        let mut marquee = base
            .relative()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            .on_hover(move |hovered, _, cx| {
                hover.update(cx, |run, cx| {
                    run.since = hovered.then(Instant::now);
                    cx.notify();
                });
            })
            .when(
                effects() && (leading > Pixels::ZERO || trailing > Pixels::ZERO),
                |this| this.fade_sides(leading, trailing),
            )
            .child(
                canvas(
                    move |bounds, _, _| room.set(bounds.size.width),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .relative()
                    .flex_none()
                    .whitespace_nowrap()
                    .left(-shift)
                    .child(child)
                    .child(
                        canvas(
                            move |bounds, _, _| line.set(bounds.size.width),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            );
        marquee.style().refine(&overrides);
        marquee
    }
}

/// How far a line that overruns its box by `overrun` has scrolled since the pointer arrived at
/// `since`, and whether it is still on its way: still for a moment, out to its end at an even
/// reading pace, a pause there, and back.
fn travel(since: Instant, overrun: Pixels) -> (Pixels, bool) {
    let delay = paced(MARQUEE_DELAY).as_secs_f32();
    let pause = paced(MARQUEE_PAUSE).as_secs_f32();
    let span = (overrun / MARQUEE_SPEED).max(Motion::Base.span().as_secs_f32());
    let at = since.elapsed().as_secs_f32() - delay;
    if at <= 0. {
        return (Pixels::ZERO, true);
    }
    if at < span {
        return (overrun * ease_in_out_cubic(at / span), true);
    }
    if at < span + pause {
        return (overrun, true);
    }
    let back = at - span - pause;
    if back < span {
        return (overrun * (1. - ease_in_out_cubic(back / span)), true);
    }
    (Pixels::ZERO, false)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_line_waits_runs_out_pauses_and_comes_back() {
        let overrun = px(120.);
        let now = Instant::now();
        let ago = |seconds: f32| now - Duration::from_secs_f32(seconds);

        assert_eq!(travel(now, overrun), (Pixels::ZERO, true));
        let span = overrun / MARQUEE_SPEED;
        let out = MARQUEE_DELAY.as_secs_f32() + span;
        let (end, moving) = travel(ago(out + 0.05), overrun);
        assert!(moving && (end - overrun).abs() < px(0.5));
        assert_eq!(
            travel(ago(out + MARQUEE_PAUSE.as_secs_f32() + span + 0.1), overrun),
            (Pixels::ZERO, false)
        );
    }
}
