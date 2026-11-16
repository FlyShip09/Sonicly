use gpui::prelude::*;
use gpui::{App, Hsla, Pixels, Rgba, ShaderInstance, Window};

use crate::shader::Effect;

/// How many units of the field fit in the geometric mean of the surface's sides. Fewer than
/// Waves, so the streams run long across the surface.
const DENSITY: f32 = 1.6;
/// How much darker the corners are than the middle.
const VIGNETTE: f32 = 0.55;
/// The sRGB luma above which the field is pressed back, the same as Waves.
const CEILING: f32 = 0.18;

static FLUID: Effect = Effect::new("sonicly-fluid", include_str!("../shaders/fluid.wgsl"));

/// Four colours carried along one slow current, curling into long streams. It fills its
/// positioned parent, is opaque, and swells a little with `energy`, the music's eased loudness.
#[derive(IntoElement)]
pub struct Fluid {
    colors: [Hsla; 4],
    stage: Hsla,
    time: f32,
    energy: f32,
}

impl Fluid {
    pub fn new(colors: [Hsla; 4]) -> Self {
        Self {
            colors,
            stage: Hsla::black(),
            time: 0.,
            energy: 0.,
        }
    }

    /// The colour the streams run over, black unless set.
    pub fn stage(mut self, stage: Hsla) -> Self {
        self.stage = stage;
        self
    }

    /// How many seconds the current has run. The owner advances it and asks for its frames.
    pub fn time(mut self, time: f32) -> Self {
        self.time = time;
        self
    }

    /// How loud the music is, from 0 to 1, already eased by the owner.
    pub fn energy(mut self, energy: f32) -> Self {
        self.energy = energy;
        self
    }
}

impl RenderOnce for Fluid {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let Rgba { r, g, b, a } = self.stage.into();
        let [first, second, third, fourth] = self.colors;
        let instance = ShaderInstance::at(self.time)
            .params(0, &[1., self.energy, DENSITY, CEILING])
            .params(4, &[r, g, b, a])
            .params(8, &[VIGNETTE, 0., 0., 0.])
            .color(0, first)
            .color(1, second)
            .color(2, third)
            .color(3, fourth);
        FLUID.fill(instance, Pixels::ZERO)
    }
}
