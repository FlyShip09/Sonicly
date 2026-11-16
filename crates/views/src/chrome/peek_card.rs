use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{AnyView, App, Context, Entity, FontWeight, Render, SharedString, Window, div};
use i18n::t;
use state::{Origin, Peek, PeekKey, Playback, Sonicly};
use ui::ActiveTheme as _;
use ui::{Artwork, Button, Skeleton, Text};

use crate::shared::{cards, cells};

/// What a hover card on an artist's or an album's name holds: the cover, the name, one line of
/// facts from the provider's own overview and a play button. Until the overview lands it shows
/// what the name came with and a skeleton where the facts go; an overview the provider will not
/// give leaves the kind of thing it is.
pub(crate) struct PeekCard {
    key: PeekKey,
    /// The name as the link showed it, until the overview names it.
    name: SharedString,
    /// The cover the link came with, such as a song's for its album, until the overview's.
    cover: Option<String>,
    peek: Entity<Peek>,
    playback: Entity<Playback>,
}

impl PeekCard {
    /// The card of the artist `id`, named `name` where the link was.
    pub(crate) fn artist(id: SharedString, name: SharedString, cx: &mut App) -> AnyView {
        cx.new(|cx| Self::new(PeekKey::Artist(id.to_string()), name, None, cx))
            .into()
    }

    /// The card of the album `id`, with the cover the row already shows for it.
    pub(crate) fn album(
        id: SharedString,
        name: SharedString,
        cover: Option<String>,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| Self::new(PeekKey::Album(id.to_string()), name, cover, cx))
            .into()
    }

    fn new(
        key: PeekKey,
        name: SharedString,
        cover: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let sonicly = Sonicly::global(cx);
        let (peek, playback) = (sonicly.peek.clone(), sonicly.playback.clone());
        cx.observe(&peek, |_, _, cx| cx.notify()).detach();
        cx.observe(&playback, |_, _, cx| cx.notify()).detach();
        peek.update(cx, |peek, cx| peek.ask(key.clone(), cx));
        Self {
            key,
            name,
            cover,
            peek,
            playback,
        }
    }

    /// The name, cover, facts line and play origin, from the overview once it is here.
    fn facts(&self, cx: &App) -> (SharedString, Option<String>, Option<SharedString>, Origin) {
        let peek = self.peek.read(cx);
        match &self.key {
            PeekKey::Artist(id) => {
                let artist = peek.artist(id, cx);
                let name = artist
                    .as_ref()
                    .map_or_else(|| self.name.clone(), |artist| artist.name.clone().into());
                let facts = artist.as_ref().and_then(|artist| {
                    if let Some(count) = artist.monthly_listeners {
                        let value = cells::count(count);
                        return Some(t!(
                            "artist-monthly-listeners",
                            count = count,
                            value = &value
                        ));
                    }
                    if let Some(count) = artist.followers {
                        let value = cells::count(count);
                        return Some(t!("artist-followers", count = count, value = &value));
                    }
                    (music::is_local_id(id) && !artist.albums.is_empty()).then(|| {
                        format!(
                            "{} · {}",
                            t!("count-albums", count = artist.albums.len()),
                            t!("count-songs", count = artist.top_tracks.len())
                        )
                        .into()
                    })
                });
                let cover = artist
                    .as_ref()
                    .and_then(|artist| artist.cover_large.clone());
                let origin = Origin::artist(id.clone()).named(name.clone());
                (name, cover, facts, origin)
            }
            PeekKey::Album(id) => {
                let detail = peek.album(id, cx);
                let name = detail.as_ref().map_or_else(
                    || self.name.clone(),
                    |detail| detail.album.name.clone().into(),
                );
                let facts = detail.as_ref().map(|detail| {
                    let album = &detail.album;
                    let mut parts =
                        vec![i18n::lookup(cards::release_key(album.release_type), None)];
                    if album.year != 0 {
                        parts.push(album.year.to_string().into());
                    }
                    let songs = match detail.tracks.len() {
                        0 => album.track_count as usize,
                        listed => listed,
                    };
                    if songs > 0 {
                        parts.push(t!("count-songs", count = songs));
                    }
                    parts.join(" · ").into()
                });
                let cover = detail
                    .as_ref()
                    .and_then(|detail| {
                        let album = &detail.album;
                        album.cover_large.clone().or_else(|| album.cover.clone())
                    })
                    .or_else(|| self.cover.clone());
                let origin = Origin::album(id.clone()).named(name.clone());
                (name, cover, facts, origin)
            }
        }
    }
}

impl Render for PeekCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let (name, cover, facts, origin) = self.facts(cx);
        let asking = self.peek.read(cx).is_asking(&self.key);
        let artist = matches!(self.key, PeekKey::Artist(_));
        let kind = match artist {
            true => t!("kind-artist"),
            false => t!("kind-album"),
        };
        let playing = self.playback.read(cx).playing_from(&origin, cx) == Some(true);
        let (icon, tip) = match playing {
            true => ("icons/pause.svg", "play-pause"),
            false => ("icons/play.svg", "common-play"),
        };
        let art = theme.metrics.list_row;
        let playback = self.playback.clone();
        let id = SharedString::from(format!("peek-{:?}", self.key));

        div()
            .id(id.clone())
            .role(Role::Group)
            .aria_label(name.clone())
            .flex()
            .items_center()
            .gap_3()
            .p_3()
            .w(theme.metrics.cover * 2.)
            .child(
                Artwork::new(cover)
                    .seed(name.clone())
                    .size(art)
                    .flex_none()
                    .when(artist, Artwork::circle),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(name),
                    )
                    .child(match (facts, asking) {
                        (Some(facts), _) => div()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(facts)
                            .into_any_element(),
                        (None, true) => Skeleton::new()
                            .w(art * 2.)
                            .h(theme.text(Text::Small))
                            .into_any_element(),
                        (None, false) => div()
                            .text_size(theme.text(Text::Small))
                            .text_color(theme.muted_foreground)
                            .child(kind)
                            .into_any_element(),
                    }),
            )
            .child(
                Button::new(SharedString::from(format!("{id}-play")))
                    .icon(icon)
                    .primary()
                    .tooltip(tip)
                    .on_click(move |_, _, cx| {
                        playback.update(cx, |playback, cx| playback.toggle_origin(&origin, cx));
                    }),
            )
    }
}
