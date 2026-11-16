use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{App, Div, ElementId, Hsla, Pixels, SharedString, Stateful, Window, div, svg};

use crate::artwork::Artwork;
use crate::theme::ActiveTheme as _;

/// Below this size the tiles of the foot row would be a few pixels wide, so the cover keeps its
/// field and its glyph alone, grown to fill the tile.
const DETAILED: f32 = 64.;
/// How much of the tile the glyph takes: in a corner at full size, centred on its own below it.
const GLYPH: f32 = 0.24;
const GLYPH_ALONE: f32 = 0.5;
/// How far in from the edge the glyph sits, and how tall the foot row is, as parts of the tile.
const INSET: f32 = 0.12;
const FOOT: f32 = 0.32;
/// The most tiles the foot row holds, one for each cover after the first.
const TILES: usize = 3;
/// The most saturation a sampled colour keeps, so no cover shouts beside real artwork.
const SATURATION: f32 = 0.42;
/// The lightness band a colour is pulled into on a dark page and on a light one, so the theme's
/// own foreground always reads on the field.
const DARK: (f32, f32) = (0.2, 0.36);
const LIGHT: (f32, f32) = (0.7, 0.84);
/// How far each tile of the foot row steps its lightness away from the field, so two covers of
/// one hue still part into separate tiles.
const STEP: f32 = 0.05;

/// The cover of a collection that has no artwork of its own, drawn rather than loaded: a field
/// in the colour of its first cover with the collection's glyph in a corner, and along its foot
/// a row of square-edged tiles in the colours of the next covers, a mosaic of what is inside.
/// The colours are settled into a calm range for the theme, so the same collection draws the
/// same cover every time and sits quietly beside real artwork in both themes. With no colours it
/// is a neutral tile with the glyph, and below `DETAILED` it drops the foot row.
#[derive(IntoElement)]
pub struct GeneratedCover {
    base: Stateful<Div>,
    name: SharedString,
    glyph: SharedString,
    colors: Vec<Hsla>,
    size: Pixels,
    radius: Option<Pixels>,
}

impl GeneratedCover {
    /// A cover named `name` for screen readers, with `glyph` for the collection's kind.
    pub fn new(
        id: impl Into<ElementId>,
        name: impl Into<SharedString>,
        glyph: impl Into<SharedString>,
    ) -> Self {
        Self {
            base: div().id(id),
            name: name.into(),
            glyph: glyph.into(),
            colors: Vec::new(),
            size: Pixels::ZERO,
            radius: None,
        }
    }

    /// The colours of the collection's first covers, the lead first.
    pub fn colors(mut self, colors: impl IntoIterator<Item = Hsla>) -> Self {
        self.colors = colors.into_iter().collect();
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn corner_radius(mut self, radius: Pixels) -> Self {
        self.radius = Some(radius);
        self
    }
}

impl Styled for GeneratedCover {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for GeneratedCover {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            name,
            glyph,
            colors,
            size,
            radius,
        } = self;
        let overrides = std::mem::take(base.style());
        let theme = cx.theme();
        let dark = theme.background.l < 0.5;
        let side = crate::snapped(size, window);
        let detailed = side >= gpui::px(DETAILED);
        let (field, ink) = match colors.first() {
            Some(lead) => (calm(*lead, dark, 0.), theme.foreground),
            None => (theme.secondary, theme.muted_foreground),
        };
        let tiles: Vec<Hsla> = match detailed {
            true => colors.iter().skip(1).take(TILES).copied().collect(),
            false => Vec::new(),
        };
        let inset = side * INSET;
        let radius = radius.unwrap_or_else(|| Artwork::rounding(cx));

        let mut cover = base
            .role(Role::Image)
            .aria_label(name)
            .relative()
            .flex_none()
            .size(side)
            .overflow_hidden()
            .rounded(radius)
            .bg(field)
            .child(svg().path(icons::path(glyph)).text_color(ink).map(|glyph| {
                match detailed {
                    true => glyph.absolute().top(inset).left(inset).size(side * GLYPH),
                    false => glyph
                        .absolute()
                        .top(side * (1. - GLYPH_ALONE) / 2.)
                        .left(side * (1. - GLYPH_ALONE) / 2.)
                        .size(side * GLYPH_ALONE),
                }
            }));
        if !tiles.is_empty() {
            // One row across the foot, edge to edge, each tile a step further from the field so
            // neighbours of one hue still part. GPUI clips children to a plain rectangle, so the
            // outer tiles round the cover's bottom corners themselves.
            let last = tiles.len() - 1;
            cover = cover.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(side * FOOT)
                    .flex()
                    .children(tiles.into_iter().enumerate().map(|(index, color)| {
                        div()
                            .flex_1()
                            .h_full()
                            .when(index == 0, |tile| tile.rounded_bl(radius))
                            .when(index == last, |tile| tile.rounded_br(radius))
                            .bg(calm(color, dark, (index + 1) as f32 * STEP))
                    })),
            );
        }
        cover.style().refine(&overrides);
        cover
    }
}

/// Settles a sampled colour for the page: its saturation capped, its lightness pulled into the
/// theme's band and then moved `step` further from the page colour.
fn calm(color: Hsla, dark: bool, step: f32) -> Hsla {
    let (low, high) = match dark {
        true => DARK,
        false => LIGHT,
    };
    let l = match dark {
        true => color.l.clamp(low, high) + step,
        false => color.l.clamp(low, high) - step,
    };
    Hsla {
        h: color.h,
        s: color.s.min(SATURATION),
        l: l.clamp(0., 1.),
        a: 1.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loud_colour_is_calmed_into_the_theme_band() {
        let loud = Hsla {
            h: 0.95,
            s: 1.,
            l: 0.5,
            a: 1.,
        };
        let dark = calm(loud, true, 0.);
        let light = calm(loud, false, 0.);

        assert_eq!(dark.s, SATURATION);
        assert!((DARK.0..=DARK.1).contains(&dark.l));
        assert!((LIGHT.0..=LIGHT.1).contains(&light.l));
        assert_eq!(dark.h, loud.h);
    }

    #[test]
    fn tiles_step_away_from_the_page() {
        let grey = Hsla {
            h: 0.,
            s: 0.,
            l: 0.3,
            a: 1.,
        };

        assert!(calm(grey, true, STEP).l > calm(grey, true, 0.).l);
        assert!(calm(grey, false, STEP).l < calm(grey, false, 0.).l);
    }
}
