use gpui::prelude::*;
use gpui::{App, Context, ElementId, Hsla};
use router::NavEntry;
use state::AppSettings;
use ui::ActiveTheme as _;
use ui::Button;

/// A navigation row as the sidebars draw it, a row of a source list: a ghost button one
/// `nav_row` tall with its label at the start, in `tint`, that takes the shared row hover while
/// the pointer is over it and `accent` while pressed. The caller adds the icon; the sidebar
/// marks the current row with a highlight of its own that glides between rows.
pub(crate) fn nav_row(
    id: impl Into<ElementId>,
    key: &'static str,
    tint: Hsla,
    accent: Hsla,
    cx: &App,
) -> Button {
    Button::new(id)
        .ghost()
        .label(i18n::lookup(key, None))
        .tint(tint)
        .h(cx.theme().metrics.nav_row)
        .px_2()
        .gap_2()
        .justify_start()
        .lit(false, accent)
        .active(move |style| style.bg(accent))
}

/// A navigation row reduced to its icon, named by a tooltip beside it.
pub(crate) fn nav_icon(
    id: impl Into<ElementId>,
    key: &'static str,
    icon: &'static str,
    tint: Hsla,
    accent: Hsla,
) -> Button {
    Button::new(id)
        .ghost()
        .icon(icon)
        .tooltip_trailing(key)
        .tint(tint)
        .w_full()
        .lit(false, accent)
        .active(move |style| style.bg(accent))
}

/// Whether the sidebar shows `entry`. An optional row shows once the listener adds it, any other
/// until they hide it.
pub(crate) fn entry_shown(entry: NavEntry, settings: &AppSettings) -> bool {
    match entry.optional() {
        true => settings.nav_added(entry.id()),
        false => settings.nav_shown(entry.id()),
    }
}

/// Shows or hides `entry` in the sidebar, the counterpart of `entry_shown`.
pub(crate) fn set_entry_shown(
    entry: NavEntry,
    shown: bool,
    settings: &mut AppSettings,
    cx: &mut Context<AppSettings>,
) {
    match entry.optional() {
        true => settings.set_nav_added(entry.id(), shown, cx),
        false => settings.set_nav_shown(entry.id(), shown, cx),
    }
}
