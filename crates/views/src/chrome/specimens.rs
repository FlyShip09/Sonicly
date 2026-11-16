//! A sheet of every generated cover at the sizes the app draws them, which the debug launch
//! argument `--debug-covers` lays over the window for a capture.

use gpui::prelude::*;
use gpui::{App, Global, Hsla, Pixels, Window, deferred, div, px};
use state::{Generated, Swatches};
use ui::{ActiveTheme as _, GeneratedCover, Text};

/// The sizes a cover is drawn at: a sidebar row, a card, the page hero and the largest card.
const SIZES: [f32; 4] = [32., 64., 140., 300.];

/// Whether the sheet is up.
struct Shown;

impl Global for Shown {}

/// Lays the sheet over the window from the next frame on.
pub fn show_cover_specimens(cx: &mut App) {
    Swatches::entity(cx);
    cx.set_global(Shown);
    cx.refresh_windows();
}

/// The sheet, or nothing while it is not up.
pub(crate) fn cover_specimens(window: &mut Window, cx: &App) -> Option<impl IntoElement> {
    cx.try_global::<Shown>()?;
    let theme = cx.theme();
    // A fresh profile has played a few songs and liked or scanned nothing, so a collection
    // with no colours of its own borrows Recents', turned, to show a different lead colour.
    let recents = Swatches::colors(Generated::Recents, cx);
    let rows = Generated::ALL
        .into_iter()
        .enumerate()
        .map(|(turn, collection)| {
            let mut colors: Vec<Hsla> = Swatches::colors(collection, cx).to_vec();
            if colors.is_empty() && !recents.is_empty() {
                colors = recents.to_vec();
                colors.rotate_left((turn + 1) % recents.len());
            }
            let name = i18n::lookup(collection.key(), None);
            div()
                .flex()
                .items_end()
                .gap(theme.metrics.inset)
                .child(
                    div()
                        .w(theme.metrics.cover)
                        .text_size(theme.text(Text::Body))
                        .child(name.clone()),
                )
                .children(SIZES.map(|side| {
                    GeneratedCover::new(
                        ("specimen", side as usize * 10 + turn),
                        name.clone(),
                        collection.glyph(),
                    )
                    .colors(colors.clone())
                    .size(px(side))
                }))
        });
    let viewport = window.viewport_size();
    // An off-screen window draws again only when asked, and the capture resizes it after its
    // first frame, so the sheet keeps asking while it is up. It is a capture aid only.
    window.request_animation_frame();

    Some(deferred(
        div()
            .absolute()
            .top(Pixels::ZERO)
            .left(Pixels::ZERO)
            .w(viewport.width)
            .h(viewport.height)
            .occlude()
            .bg(theme.background)
            .text_color(theme.foreground)
            .p(theme.metrics.inset)
            .flex()
            .flex_col()
            .gap(theme.metrics.inset)
            .children(rows),
    ))
}
