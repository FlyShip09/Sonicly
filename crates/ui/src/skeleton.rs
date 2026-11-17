use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    App, Bounds, Div, Hsla, Interactivity, Pixels, SharedString, StyleRefinement, Window, canvas,
    div, fill, linear_color_stop, linear_gradient, point, px,
};

use crate::theme::ActiveTheme as _;

/// How strongly a placeholder's fill shows.
const STILL: f32 = 0.55;
/// How long the light takes to cross the window, slow enough to read as calm.
const SWEEP: Duration = Duration::from_millis(1800);
/// How wide each soft side of the light is.
const SWEEP_BAND: Pixels = px(160.);
/// How much lighter the middle of the light is, as the text colour laid over the fill.
const SWEEP_LIGHT: f32 = 0.06;

#[derive(IntoElement)]
pub struct Skeleton {
    base: Div,
    circle: bool,
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Skeleton {
    #[track_caller]
    pub fn new() -> Self {
        Self {
            base: div(),
            circle: false,
        }
    }

    pub fn circle(mut self) -> Self {
        self.circle = true;
        self
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Skeleton {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Skeleton {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { mut base, circle } = self;
        let theme = cx.theme();
        let overrides = std::mem::take(base.style());

        let mut skeleton = base
            .relative()
            .overflow_hidden()
            .bg(theme.muted.opacity(STILL))
            .when_else(
                circle,
                |this| this.rounded_full(),
                |this| this.rounded(theme.radius),
            );
        skeleton.style().refine(&overrides);

        // With motion reduced the placeholder holds still.
        if cx.reduce_motion() {
            return skeleton.into_any_element();
        }
        let light = theme.foreground.opacity(SWEEP_LIGHT);
        skeleton.child(sweep(light)).into_any_element()
    }
}

/// When the sweep's clock started, shared by every placeholder so the light crosses the whole
/// window as one band rather than each placeholder pulsing on its own.
static SWEEP_EPOCH: OnceLock<Instant> = OnceLock::new();

/// A soft band of light crossing the window from left to right, painted where it passes over
/// this placeholder. It asks for the next frame only while a placeholder is drawn, which is
/// only while something loads.
fn sweep(light: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.request_animation_frame();
            let epoch = *SWEEP_EPOCH.get_or_init(Instant::now);
            let turn = (epoch.elapsed().as_secs_f32() / SWEEP.as_secs_f32()).fract();
            let reach = window.viewport_size().width + SWEEP_BAND * 2.;
            let middle = reach * turn - SWEEP_BAND;
            if middle + SWEEP_BAND < bounds.left() || middle - SWEEP_BAND > bounds.right() {
                return;
            }
            let clear = light.opacity(0.);
            let side = |from: Pixels, to: Pixels, rising: bool| {
                let (start, end) = match rising {
                    true => (clear, light),
                    false => (light, clear),
                };
                fill(
                    Bounds::from_corners(point(from, bounds.top()), point(to, bounds.bottom())),
                    linear_gradient(
                        90.,
                        linear_color_stop(start, 0.),
                        linear_color_stop(end, 1.),
                    ),
                )
            };
            window.paint_quad(side(middle - SWEEP_BAND, middle, true));
            window.paint_quad(side(middle, middle + SWEEP_BAND, false));
        },
    )
    .absolute()
    .inset_0()
}

#[derive(IntoElement)]
pub struct Initials {
    base: Div,
    name: SharedString,
    size: Pixels,
}

impl Initials {
    #[track_caller]
    pub fn new(name: impl Into<SharedString>, size: Pixels) -> Self {
        Self {
            base: div(),
            name: name.into(),
            size,
        }
    }
}

impl Styled for Initials {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl InteractiveElement for Initials {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl RenderOnce for Initials {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            name,
            size,
        } = self;
        let letters = name
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase();

        let theme = cx.theme();
        let overrides = std::mem::take(base.style());

        let mut initials = base
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(size)
            .rounded_full()
            .bg(theme.secondary)
            .text_size(size * 0.34)
            .text_color(theme.muted_foreground)
            .child(letters);
        initials.style().refine(&overrides);
        initials
    }
}
