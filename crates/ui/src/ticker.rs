use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{App, ElementId, SharedString, Window, div};

use crate::motion::{self, Motion, ease_out_cubic};
use crate::time::tabular;

/// The longest a count takes to run to its new value, however far it has to go.
const SPAN: Duration = Duration::from_millis(400);

/// A count that changes in place, such as a playlist's number of songs, running from its last
/// value to its new one instead of jumping, in tabular figures so the line does not shake.
/// The first value it shows appears as it is, and screen readers hear only the value it lands
/// on. With motion reduced it jumps.
#[derive(IntoElement)]
pub struct Ticker {
    id: ElementId,
    value: u64,
    words: Box<dyn Fn(u64) -> SharedString>,
}

impl Ticker {
    /// A count of `value`, worded by `words`, such as a Fluent string with a count selector.
    pub fn new(
        id: impl Into<ElementId>,
        value: u64,
        words: impl Fn(u64) -> SharedString + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            value,
            words: Box::new(words),
        }
    }
}

/// Where a ticker is running from and to, and since when.
struct Run {
    from: u64,
    to: u64,
    since: Instant,
}

impl Run {
    /// The value to draw now, and whether the run is still under way.
    fn at(&self, span: Duration) -> (u64, bool) {
        let done = span.is_zero() || self.since.elapsed() >= span;
        if done {
            return (self.to, false);
        }
        let progress = ease_out_cubic(self.since.elapsed().as_secs_f32() / span.as_secs_f32());
        let (from, to) = (self.from as f64, self.to as f64);
        (
            (from + (to - from) * f64::from(progress)).round() as u64,
            true,
        )
    }
}

impl RenderOnce for Ticker {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let span = match motion::animates(cx) {
            true => Motion::Slow.span().min(SPAN),
            false => Duration::ZERO,
        };
        let value = self.value;
        let run = window.use_keyed_state((self.id.clone(), "run"), cx, |_, _| Run {
            from: value,
            to: value,
            since: Instant::now(),
        });
        let (shown, running) = run.update(cx, |run, _| {
            if run.to != value {
                let (now, _) = run.at(span);
                *run = Run {
                    from: now,
                    to: value,
                    since: Instant::now(),
                };
            }
            run.at(span)
        });
        if running {
            window.request_animation_frame();
        }
        let landed = (self.words)(value);

        div()
            .id(self.id)
            .role(gpui::accesskit::Role::Label)
            .aria_value(landed)
            .font_features(tabular())
            .child((self.words)(shown))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::Run;

    #[test]
    fn a_run_lands_on_its_value_and_stops() {
        let run = Run {
            from: 3,
            to: 40,
            since: Instant::now() - Duration::from_secs(1),
        };
        assert_eq!(run.at(Duration::from_millis(400)), (40, false));
        let fresh = Run {
            from: 3,
            to: 40,
            since: Instant::now(),
        };
        let (shown, running) = fresh.at(Duration::from_secs(60));
        assert!(running && (3..=40).contains(&shown));
        assert_eq!(fresh.at(Duration::ZERO), (40, false));
    }
}
