use std::cell::Cell;
use std::rc::Rc;

use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Pixels, Point, Render, ScrollHandle,
    SharedString, WeakEntity, Window, div, linear_color_stop, linear_gradient, px, relative,
};

use crate::chrome::Chrome;
use crate::shared::blend::Joined;
use crate::shared::cells;
use i18n::t;
use music::{Album, ReleaseType, SavedArtist, Track};
use state::{AppSettings, ArtistDetail, Origin, Playback, Sonicly};
use ui::ActiveTheme as _;
use ui::Listing as _;
use ui::{
    Artwork, Button, Deck, MIN_CONTENT, Modal, Mode, Pending, Picker, Pin, PinKind, Popovers,
    Popup, Scrollbar, Scroller, TableDelegate, TableEvent, TableState, effects, snapped, spoken,
    table,
};

use crate::chrome::tools;
use crate::chrome::{Toolbar, Tooled};
use crate::shared::about::{AboutArtist, about_modal};
use crate::shared::album_grid::CardGrid;
use crate::shared::cards;
use crate::shared::confirm::Confirm;
use crate::shared::cover_viewer::CoverViewer;
use crate::shared::hero::{HeroMetaStrip, HeroPlayButton, PageHero};
use crate::shared::menus::{ItemMenu, artist_menu};
use crate::shared::page;
use crate::shared::picks::{Picks, Shape};
use crate::shared::shelves::{Rail, RailSpec};
use crate::shared::tracks::{PlaybackStatus, TrackSource, Tracks, drop_picked, playback_status};
use crate::shared::trouble;

const SECTION: &str = "artist";
const LISTED: usize = 5;
const LISTED_MAX: usize = 10;
/// The favorites dialog gives two song columns enough room for titles and controls.
const FAVORITES_WIDTH: f32 = 5.;
/// The least and the most room the banner adds above the hero, as shares of the cover's edge.
/// In between, the banner box keeps `BANNER_BOX` as its aspect, so a narrow page lifts the
/// hero less.
const BANNER_RISE_LEAST: f32 = 0.35;
const BANNER_RISE_MOST: f32 = 1.;
const BANNER_BOX: f32 = 2.7;
/// The room above the hero when only a blurred portrait stands in for the banner. The wash has
/// no proportions to keep, so it stays this tall at any width.
const BANNER_RISE_WASH: f32 = 0.6;
/// The aspect the banner picture is drawn at. Spotify's banners are this wide, and the box is
/// a little wider still, so the picture loses a strip at its top and bottom but never its sides.
const BANNER_ASPECT: f32 = 2.33;
/// How much of what the banner box cannot show is cut off above rather than below. Banners keep
/// their subject a little above the middle.
const BANNER_ANCHOR: f32 = 0.35;
/// The tallest the banner picture grows, as a share of the banner's height. On a page wide enough
/// to grow it further it keeps this size in the middle, and both its sides fade into a washed out
/// copy over the room beside it, up to `BANNER_SIDE_FADE` of its width, so faces are never blown
/// up past the box.
const BANNER_OVERFLOW: f32 = 1.25;
const BANNER_SIDE_FADE: f32 = 0.2;
/// How much wider than the page the banner is drawn, so it still reaches both edges while the
/// page width it was measured from lags a resize by a frame.
const BANNER_SLACK: f32 = 0.06;
/// The blurs the banner steps through on its way down, each with the share of the stretch from
/// the hero's top to the banner's bottom at which it has fully taken over from the one above.
/// Everything above the hero stays sharp, wherever the page width puts it, and the blur grows
/// slowly at first so it has no visible start.
const BANNER_BLURS: [(f32, Pixels); 7] = [
    (0.25, px(1.)),
    (0.375, px(2.5)),
    (0.5, px(4.5)),
    (0.625, px(7.5)),
    (0.75, px(11.5)),
    (0.875, px(17.)),
    (1., px(24.)),
];
/// How far above its own mark each banner blur starts fading in, as a share of the same
/// stretch. It is longer than the gap between marks, so each blur is still fading in when the
/// next starts.
const BANNER_RAMP: f32 = 0.25;
/// How much of the page colour lies over the whole banner, so it sits back behind the page.
const BANNER_DIM: f32 = 0.3;
/// The shade that deepens toward the banner's bottom on top of `BANNER_DIM`: where it starts, as
/// a share of the way down to the hero's top, and how much of the page colour it reaches at the
/// bottom. The banner also fades out from the hero's top and is gone at its bottom.
const BANNER_SHADE_FROM: f32 = 0.5;
const BANNER_SHADE: f32 = 0.85;
/// The blur over a portrait standing in for a missing banner, enough to leave only its colours.
const BANNER_WASH: Pixels = px(48.);

struct ArtistTracks {
    detail: Entity<ArtistDetail>,
    shown: Rc<Cell<usize>>,
}

impl Tracks for ArtistTracks {
    fn tracks<'a>(&self, cx: &'a App) -> Joined<'a, Track> {
        let tracks = self.detail.read(cx).tracks();
        tracks[..self.shown.get().min(tracks.len())].into()
    }

    fn is_loading(&self, cx: &App) -> bool {
        self.detail.read(cx).is_loading()
    }

    /// As many rows as the list shows before it is expanded.
    fn pending(&self, cx: &App) -> Option<Pending> {
        self.is_loading(cx).then_some(Pending::Rows(LISTED))
    }
}

pub(crate) struct ArtistView {
    detail: Entity<ArtistDetail>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    artist_id: Option<String>,
    width: Pixels,
    scrollbar: Entity<Scrollbar>,
    about_bar: Entity<Scrollbar>,
    about_open: bool,
    favorites_open: bool,
    table: Entity<TableState<TrackSource>>,
    shown: Rc<Cell<usize>>,
    mode: Mode,
    popular: Rc<Vec<Track>>,
    popular_page: usize,
    popular_columns: usize,
    track_menu: ItemMenu,
    track_context: Option<(usize, Point<Pixels>)>,
    settings: Entity<AppSettings>,
    toolbar: Entity<Toolbar>,
    me: WeakEntity<Self>,
    popovers: Popovers,
    /// The rails under the songs, in page order: albums, singles and EPs, appears on.
    rails: Vec<Rail>,
    cover_viewer: Entity<CoverViewer>,
}

impl ArtistView {
    pub(crate) fn new(
        detail: Entity<ArtistDetail>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let width = MIN_CONTENT;
        let id = cx.entity_id();
        let scrollbar = cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(id));
        let playlist_scrollbar = cx.new(|_| Scrollbar::inset().watching(id));
        let settings = Sonicly::global(cx).settings.clone();
        let saved = settings.read(cx).table(SECTION);
        let sorting = settings.read(cx).sorting(SECTION);
        let mode = settings.read(cx).view_or(SECTION, Mode::List);
        let columns = crate::shared::tracks::artist_columns(
            Sonicly::global(cx)
                .session
                .read(cx)
                .capabilities()
                .playcounts,
        );
        let scroll = scrollbar.read(cx).scroll().clone();
        let shown = Rc::new(Cell::new(LISTED));
        let table = cx.new(|cx| {
            let menu_scrollbar = cx.new(|_| Scrollbar::inset().watching(id));
            crate::shared::tracks::follow_downloads(cx);
            let source = TrackSource::new(
                columns,
                ArtistTracks {
                    detail: detail.clone(),
                    shown: shown.clone(),
                },
                playback.clone(),
                menu_scrollbar,
                cx,
            )
            .from({
                let detail = detail.clone();
                move |cx: &App| {
                    let detail = detail.read(cx);
                    let name = detail.artist()?.name.clone();
                    Some(Origin::artist(detail.id()?).named(name))
                }
            })
            .with_liked(Sonicly::global(cx).library.clone());
            let source = source.table(cx.weak_entity());
            let mut delegate = TableDelegate::new(source, width, cx);
            delegate.set_layout(saved, cx);
            delegate.set_sorting(sorting.flatten(), cx);
            TableState::new(delegate, cx).follow(scroll)
        });

        cx.subscribe(&detail, |_, _, event, cx| {
            let state::ArtistDetailEvent::Gone(id) = event;
            if router::trail(cx).read(cx).current()
                == router::Destination::Artist(id.clone().into())
            {
                router::replace(
                    router::Destination::Library(router::LibraryTab::Artists),
                    cx,
                );
            }
        })
        .detach();

        cx.observe(&detail, |this, detail, cx| {
            let artist_id = detail.read(cx).id().map(str::to_owned);
            if this.artist_id != artist_id {
                this.artist_id = artist_id;
                this.about_open = false;
                this.favorites_open = false;
                this.rails.clear();
                this.shown.set(LISTED);
                this.scrollbar.update(cx, |bar, cx| {
                    bar.set_max_offset(None, cx);
                    bar.scroll().set_offset(gpui::Point::default());
                });
            }
            this.popular = Rc::new(detail.read(cx).tracks().to_vec());
            this.popular_page = 0;
            this.track_menu.reset(cx);
            this.track_context = None;
            this.rebuild(cx);
            cx.notify();
        })
        .detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let library = Sonicly::global(cx).library.clone();
        cx.observe(&library, |this, _, cx| {
            this.table.update(cx, |table, cx| table.refresh(cx));
            cx.notify();
        })
        .detach();
        let current_playback = playback_status(&playback, cx);
        let artist_id = detail.read(cx).id().map(str::to_owned);
        cx.observe(&playback, |this, playback, cx| {
            let current = playback_status(&playback, cx);
            if this.playback_status == current {
                return;
            }
            this.playback_status = current;
            this.table.update(cx, |table, cx| table.refresh(cx));
            cx.notify();
        })
        .detach();
        cx.subscribe(&table, |this, _, event, cx| match event {
            TableEvent::DoubleClicked(display) => {
                page::play(&this.table, &this.playback, *display, cx)
            }
            TableEvent::Activated(display) => {
                page::play_or_toggle(&this.table, &this.playback, *display, cx)
            }
            TableEvent::Removed => drop_picked(&this.table, cx),
            _ => this.persist(cx),
        })
        .detach();

        let me = cx.entity();
        let toolbar = Toolbar::tooled(&me, cx);
        let cover_viewer = cx.new(CoverViewer::new);
        // The hero portrait hides while the viewer shows it, until it lands back on it.
        cx.observe(&cover_viewer, |_, _, cx| cx.notify()).detach();

        Self {
            popular: Rc::new(detail.read(cx).tracks().to_vec()),
            detail,
            playback,
            playback_status: current_playback,
            artist_id,
            width,
            scrollbar,
            about_bar: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(id)),
            about_open: false,
            favorites_open: false,
            table,
            shown,
            mode,
            popular_page: 0,
            popular_columns: 0,
            track_menu: ItemMenu::new(playlist_scrollbar, cx),
            track_context: None,
            settings,
            toolbar,
            me: me.downgrade(),
            popovers: Popovers::default(),
            rails: Vec::new(),
            cover_viewer,
        }
    }

    fn persist(&mut self, cx: &mut Context<Self>) {
        page::store(
            &self.settings.clone(),
            &self.table.clone(),
            SECTION,
            SECTION,
            cx,
        );
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.table.update(cx, |table, cx| {
            table.rebuild(cx);
        });
    }

    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.mode = mode;
        if mode == Mode::List {
            self.table.clone().set_width(self.width, cx);
        }
        self.track_context = None;
        self.settings
            .update(cx, |settings, cx| settings.set_view(SECTION, mode, cx));
        cx.notify();
    }

    fn header(&self, cx: &Context<Self>) -> AnyElement {
        let artist = self.detail.read(cx).artist();
        let title = artist
            .map(|artist| SharedString::from(artist.name.clone()))
            .unwrap_or_default();
        let listeners = artist
            .and_then(|artist| artist.monthly_listeners)
            .map(|count| {
                let value = cells::count(count);
                t!("artist-monthly-listeners", count = count, value = &value)
            });
        let followers = artist.and_then(|artist| artist.followers).map(|count| {
            let value = cells::count(count);
            t!("artist-followers", count = count, value = &value)
        });
        // A local artist has no listener counts; what the library holds of it is the fact.
        let library = self
            .detail
            .read(cx)
            .id()
            .filter(|id| music::is_local_id(id))
            .and(artist)
            .map(|artist| {
                HeroMetaStrip::new()
                    .text(t!("count-albums", count = artist.albums.len()))
                    .text(t!("count-songs", count = artist.top_tracks.len()))
            });
        let overflow = self.saved_artist(cx).map(|artist| {
            Picker::icon("artist-overflow", &self.popovers, "icons/ellipsis.svg")
                .tooltip("common-more")
                .large()
                .left()
                .menu(artist_menu(artist, self.playback.clone(), true, cx))
        });
        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                HeroPlayButton::new(
                    "play-artist",
                    self.popular.as_ref().clone(),
                    self.playback.clone(),
                )
                .load_origin(self.playing_from(cx)),
            )
            .child(
                HeroPlayButton::shuffle(
                    "shuffle-artist",
                    self.popular.as_ref().clone(),
                    self.playback.clone(),
                )
                .load_origin(self.playing_from(cx)),
            )
            .children(self.favorite_button(cx))
            .children(overflow);

        let cover = artist.and_then(|artist| artist.cover_large.clone());
        let pin = self
            .detail
            .read(cx)
            .id()
            .map(|id| Pin::new(PinKind::Artist, id, title.clone()).cover(cover.clone()));

        let view_cover = cover.clone().map(|cover| {
            let view = self.me.clone();
            let title = title.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                view.update(cx, |this, cx| {
                    let owner = this.detail.read(cx).id().map(str::to_owned);
                    this.cover_viewer.update(cx, |viewer, cx| {
                        viewer.open(owner, title.clone(), cover.clone(), true, cx);
                    });
                })
                .ok();
            }
        });

        PageHero::new("artist-hero", title)
            .pending(artist.is_none() && self.detail.read(cx).is_loading())
            .pin(pin)
            .cover(cover)
            .cover_away(self.cover_viewer.read(cx).is_open())
            .when_some(view_cover, |hero, on_cover| hero.on_cover_click(on_cover))
            .map(
                |hero| match (listeners.is_some() || followers.is_some(), library) {
                    (true, _) => hero.meta(artist_meta(listeners, followers)),
                    (false, Some(library)) => hero.meta(library),
                    (false, None) => hero,
                },
            )
            .actions(actions)
            .circle()
            .into_any_element()
    }

    /// The artist's banner behind the hero, `height` tall and bled past the page's `inset` to
    /// its edges, with the hero's top `hero` below its own. It spans the side `margin` of a wide
    /// window too, so its parent has to be as wide as the content area, since its blurs never
    /// reach far past that parent. It is dimmed throughout and stays sharp above the hero, then
    /// blurs, darkens and fades harder until it is gone at its bottom.
    /// Without a banner the portrait stands in, blurred to a wash of its colours so it does not
    /// repeat the cover beside it. None when the artist has neither, or has only a portrait
    /// while blurs are off.
    fn banner(
        &self,
        inset: Pixels,
        margin: Pixels,
        height: Pixels,
        hero: Pixels,
        cx: &App,
    ) -> Option<AnyElement> {
        let artist = self.detail.read(cx).artist()?;
        let (url, wash) = match &artist.banner {
            Some(banner) => (banner.clone(), None),
            None if effects() => (artist.cover_large.clone()?, Some(BANNER_WASH)),
            None => return None,
        };
        // The box stretches to the page's edges at once, while the width measured here arrives a
        // frame later, so a window being resized would show the box's edge past the picture.
        // Everything is drawn a little wider than the box, centred in it, and the box clips the
        // rest.
        let width = (self.width + (inset + margin) * 2.) * (1. + BANNER_SLACK);
        let theme = cx.theme();
        let shade = div()
            .absolute()
            .inset_0()
            .bg(theme.background.opacity(BANNER_DIM))
            .child(div().absolute().inset_0().bg(linear_gradient(
                180.,
                linear_color_stop(
                    theme.background.opacity(0.),
                    hero / height * BANNER_SHADE_FROM,
                ),
                linear_color_stop(theme.background.opacity(BANNER_SHADE), 1.),
            )));
        // Every copy reaches past the box by twice the widest blur, so no blur pulls in the
        // transparent edge, and they all share one geometry so they line up.
        let bleed = wash.unwrap_or(BANNER_BLURS[BANNER_BLURS.len() - 1].1) * 2.;
        let natural = width / BANNER_ASPECT;
        let tall = match wash {
            Some(_) => natural,
            None => natural.min(height * BANNER_OVERFLOW),
        }
        .max(height);
        let across = (tall * BANNER_ASPECT).min(width);
        // The fade grows from nothing with the room either side, so a page whose width sits near
        // the point where the picture stops filling it does not flick the fade on and off.
        let side = bleed + ((width - across) / 2.).min(across * BANNER_SIDE_FADE);
        let lifted = (tall - height) * BANNER_ANCHOR;
        // The same picture lies under the banner across the whole box, washed out like a missing
        // banner's portrait, so a picture narrower than the page has its colours either side.
        let spread = BANNER_WASH * 2.;
        let backdrop = Artwork::new(Some(url.clone()))
            .size(width + spread * 2.)
            .wide(height + spread * 2.)
            .overlay()
            .absolute()
            .left(relative(0.5))
            .ml(-(width / 2. + spread))
            .top(-spread)
            .blur(BANNER_WASH);
        let picture = |top: Pixels| {
            Artwork::new(Some(url.clone()))
                .size(across + bleed * 2.)
                .wide(tall + bleed * 2.)
                .overlay()
                .absolute()
                .left(relative(0.5))
                .ml(-(across / 2. + bleed))
                .top(-top - lifted - bleed)
                .fade_sides(side, side)
        };
        let below = height - hero;
        let steps = BANNER_BLURS.iter().map(|&(mark, blur)| {
            let top = hero + below * (mark - BANNER_RAMP);
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(top)
                .bottom_0()
                .overflow_hidden()
                .fade_edges(below * BANNER_RAMP, Pixels::ZERO)
                .child(picture(top).blur(blur))
        });

        Some(
            div()
                .absolute()
                .top(-inset)
                .left(-inset)
                .right(-inset)
                .h(height)
                .overflow_hidden()
                .fade_edges(Pixels::ZERO, below)
                .map(|this| match wash {
                    Some(wash) => this.child(picture(Pixels::ZERO).blur(wash)),
                    None => this
                        .when(effects(), |this| this.child(backdrop))
                        .child(picture(Pixels::ZERO))
                        .when(effects(), |this| this.children(steps)),
                })
                .child(shade)
                .into_any_element(),
        )
    }

    fn saved_artist(&self, cx: &App) -> Option<SavedArtist> {
        let detail = self.detail.read(cx);
        let artist = detail.artist()?;

        Some(SavedArtist {
            id: detail.id()?.to_owned(),
            name: artist.name.clone(),
            cover: artist.cover_large.clone(),
            ..Default::default()
        })
    }

    fn favorite_button(&self, cx: &App) -> Option<Button> {
        let theme = *cx.theme();
        // A provider with no followed artists has nothing for this to toggle.
        if !Sonicly::global(cx)
            .session
            .read(cx)
            .capabilities()
            .follow_artists
        {
            return None;
        }
        let library = Sonicly::global(cx).library.clone();
        let target = self.saved_artist(cx)?;
        let saved = library.read(cx).saved_artist(&target.id);

        let heart = Button::new("artist-toggle-library")
            .outline()
            .bounce(saved)
            .icon(match saved {
                true => "icons/heart-filled.svg",
                false => "icons/heart.svg",
            })
            .tooltip(match saved {
                true => "menu-remove-from-library",
                false => "menu-add-to-library",
            })
            .disabled(library.read(cx).pending_artist(&target.id));

        Some(
            match saved {
                true => heart.tint(theme.primary),
                false => heart,
            }
            .on_click(move |_, _, cx| {
                match saved && !Confirm::unstarring(&target.id, cx) {
                    true => Confirm::artists(vec![target.clone()], cx),
                    false => {
                        library.update(cx, |library, cx| library.heart_artist(target.clone(), cx));
                    }
                }
            }),
        )
    }

    /// The rails under the songs, each drawn only when the artist has something for it: the
    /// albums, the singles and EPs, and what the artist guests on. A discography still on its
    /// way reads as skeletons only while nothing of it is up.
    fn sections(
        &self,
        window: &Window,
        notify: &Rc<dyn Fn(&mut App)>,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        let grid = CardGrid::layout(self.width);
        let card = grid.card;
        let columns = grid.columns.max(1);
        let detail = self.detail.read(cx);
        let loading = detail.is_loading() || (detail.albums().is_empty() && detail.is_filling());
        let singles =
            |album: &Album| matches!(album.release_type, ReleaseType::Single | ReleaseType::Ep);
        let albums: Rc<[usize]> = (0..detail.albums().len())
            .filter(|&index| !singles(&detail.albums()[index]))
            .collect();
        let short: Rc<[usize]> = (0..detail.albums().len())
            .filter(|&index| singles(&detail.albums()[index]))
            .collect();
        let appears: Rc<[usize]> = (0..detail.appears_on().len()).collect();
        let filling = detail.is_filling();

        let shelves = [
            (
                Shelf::Albums,
                "artist-albums",
                t!("artist-albums"),
                albums,
                loading,
            ),
            (
                Shelf::Singles,
                "artist-singles",
                t!("artist-singles"),
                short,
                false,
            ),
            (
                Shelf::Appears,
                "artist-appears",
                t!("artist-appears-on"),
                appears,
                filling,
            ),
        ];
        shelves
            .into_iter()
            .enumerate()
            .filter_map(|(place, (shelf, tag, title, shown, pending))| {
                if shown.is_empty() {
                    return pending.then(|| Rail::pending(title, card, columns, window, cx));
                }
                let opened = self.me.clone();
                let count = shown.len();
                Some(self.rails[place].render(
                    RailSpec {
                        tag,
                        place: 0,
                        title,
                        count,
                        tile: card,
                        columns,
                        tabs: None,
                    },
                    window,
                    cx,
                    notify,
                    move |index, _, cx| {
                        let Some(view) = opened.upgrade() else {
                            return div().into_any_element();
                        };
                        let held = view.read(cx);
                        let detail = held.detail.read(cx);
                        let listed = match shelf {
                            Shelf::Appears => detail.appears_on(),
                            Shelf::Albums | Shelf::Singles => detail.albums(),
                        };
                        let Some(album) = shown.get(index).and_then(|&at| listed.get(at)) else {
                            return div().into_any_element();
                        };
                        let id = SharedString::from(format!("{tag}-{}", album.id));
                        cards::album_card(id, album, &held.playback, cx)
                            .tile(card)
                            .flat()
                            .aria_position_in_set(index + 1)
                            .aria_size_of_set(count)
                            .into_any_element()
                    },
                ))
            })
            .map(|rail| div().pt_6().child(rail).into_any_element())
            .collect()
    }

    fn listed(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.theme();
        let expanded = self.shown.get() > LISTED;
        let more = (self.popular.len() > LISTED).then(|| {
            Button::new("artist-popular-more")
                .label(match expanded {
                    true => t!("artist-popular-less"),
                    false => t!("artist-popular-more"),
                })
                .trailing(chevron(expanded))
                .small()
                .ghost()
                .aria_expanded(expanded)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.shown.set(match expanded {
                        true => LISTED,
                        false => LISTED_MAX,
                    });
                    this.rebuild(cx);
                    cx.notify();
                }))
        });

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                table(&self.table)
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border),
            )
            .children(more)
            .into_any_element()
    }

    /// What the songs at the top are called: Popular where the provider ranks them, Songs for
    /// a local artist, whose songs are simply the ones the library holds.
    fn songs_title(&self, cx: &App) -> &'static str {
        match self.detail.read(cx).id().is_some_and(music::is_local_id) {
            true => "artist-songs",
            false => "artist-popular",
        }
    }

    fn playing_from(&self, cx: &App) -> Option<Origin> {
        let detail = self.detail.read(cx);
        let name = detail.artist()?.name.clone();

        Some(Origin::artist(detail.id()?).named(name))
    }

    fn popular(&self, cx: &mut Context<Self>) -> AnyElement {
        let tracks = self.popular.clone();
        let from = self.playing_from(cx);
        let pages = Shape::new(self.width, tracks.len()).pages;
        let queued = tracks.clone();
        let playback = self.playback.clone();
        let opened = cx.entity().downgrade();

        Picks::new(
            "artist-popular",
            tracks,
            self.playback.clone(),
            self.width,
            self.popular_page,
        )
        .title(self.songs_title(cx))
        .vacancy("artist-popular-empty")
        .detailed()
        .loading(self.detail.read(cx).is_loading())
        .on_previous(cx.listener(|this, _, _, cx| {
            this.popular_page = this.popular_page.saturating_sub(1);
            this.track_context = None;
            cx.notify();
        }))
        .on_next(cx.listener(move |this, _, _, cx| {
            this.popular_page = (this.popular_page + 1).min(pages.saturating_sub(1));
            this.track_context = None;
            cx.notify();
        }))
        .on_context_menu(move |place, event, _, cx| {
            let Some(view) = opened.upgrade() else {
                return;
            };
            view.update(cx, |this, cx| {
                this.track_menu.reset(cx);
                this.track_context = Some((place, event.position));
                cx.notify();
            });
        })
        .on_start(move |place, cx| {
            playback.update(cx, |playback, cx| {
                playback.start(queued.as_ref().clone(), place, from.clone(), cx)
            });
        })
        .into_any_element()
    }

    /// Opens the listener's favorites for this artist without adding songs to the page.
    fn favorites_link(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.detail.read(cx).id()?;
        let count = Sonicly::global(cx)
            .library
            .read(cx)
            .favorites_by_artist(id)
            .count();
        if count == 0 {
            return None;
        }
        Some(
            div()
                .pb_3()
                .child(
                    Button::new("artist-favorites")
                        .icon("icons/heart.svg")
                        .label(t!("artist-favorite-songs-count", count = count))
                        .trailing("icons/chevron-right.svg")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            Modal::rewind("artist-favorites-dialog", cx);
                            this.favorites_open = true;
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    /// Keeps a large favorites collection in its own scrollable, virtualized list.
    fn favorites_dialog(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.favorites_open {
            return None;
        }
        let detail = self.detail.read(cx);
        let tracks = Rc::new(
            Sonicly::global(cx)
                .library
                .read(cx)
                .favorites_by_artist(detail.id()?)
                .cloned()
                .collect::<Vec<_>>(),
        );
        let count = tracks.len();
        let from = self.playing_from(cx);
        let playback = self.playback.clone();
        let queued = tracks.clone();
        let start: crate::shared::track_card::StartHandler = Rc::new(move |place, cx| {
            playback.update(cx, |playback, cx| {
                playback.start(queued.as_ref().clone(), place, from.clone(), cx)
            });
        });
        let library = Sonicly::global(cx).library.clone();
        let listed = tracks.clone();
        let remove: crate::shared::track_card::RemoveHandler = Rc::new(move |place, cx| {
            let Some(track) = listed.get(place).cloned() else {
                return;
            };
            library.update(cx, |library, cx| {
                library.save_tracks(vec![track], false, cx)
            });
        });
        let row_playback = self.playback.clone();
        let height = snapped(cx.theme().metrics.list_row, window);
        let columns = match Chrome::room(window, cx).fits(ui::Room::Roomy) {
            true => 2,
            false => 1,
        };
        let rows = Deck::new("artist-favorite-songs")
            .rows(std::iter::repeat_n(height, count.div_ceil(columns)))
            .draw(move |row, _, cx| {
                div()
                    .flex()
                    .gap_2()
                    .children((0..columns).map(|column| {
                        let place = row * columns + column;
                        div().flex_1().min_w_0().when(place < count, |this| {
                            this.child(
                                crate::shared::track_card::TrackCard::new(
                                    "artist-favorite-song",
                                    place,
                                    tracks.clone(),
                                    row_playback.clone(),
                                )
                                .detailed(true)
                                .start(Some(start.clone()))
                                .remove("menu-remove-from-library", remove.clone())
                                .render(cx),
                            )
                        })
                    }))
                    .into_any_element()
            });
        Some(
            Modal::new("artist-favorites-dialog", t!("artist-favorite-songs"))
                .w(cx.theme().metrics.cover * FAVORITES_WIDTH)
                .child(rows)
                .when(count == 0, |modal| {
                    modal.child(ui::vacant(t!("library-no-songs"), cx))
                })
                .close_button()
                .on_dismiss(cx.listener(|this, _, _, cx| {
                    this.favorites_open = false;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }

    fn about(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let artist = self.detail.read(cx).artist()?;
        // An artist the provider says nothing about gets no card, rather than a stock line.
        artist
            .biography
            .as_deref()
            .filter(|bio| !bio.trim().is_empty())?;
        let card = AboutArtist::new("artist-about", artist.name.clone())
            .cover(artist.cover_large.clone())
            .biography(artist.biography.clone())
            .on_open(cx.listener(|this, _, _, cx| {
                this.about_open = true;
                cx.notify();
            }));

        Some(div().pt_6().child(card).into_any_element())
    }

    /// Closes the open artist dialog and says whether one was dismissed.
    pub(crate) fn escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.favorites_open {
            self.favorites_open = false;
            cx.notify();
            return true;
        }
        if !self.about_open {
            return false;
        }
        self.about_open = false;
        cx.notify();
        true
    }

    fn about_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.about_open {
            return None;
        }
        let artist = self.detail.read(cx).artist()?;

        Some(
            about_modal(
                artist.name.clone().into(),
                artist.biography.clone(),
                None,
                &self.about_bar,
                cx,
            )
            .action(
                Button::new("artist-about-close")
                    .label(t!("common-dismiss"))
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.about_open = false;
                        cx.notify();
                    })),
            )
            .on_dismiss(cx.listener(|this, _, _, cx| {
                this.about_open = false;
                cx.notify();
            }))
            .into_any_element(),
        )
    }

    /// The page an artist shows instead of its hero and its tables when it cannot be read: the
    /// No connection state as soon as the network is gone, whatever this page happens to hold,
    /// and the failure of its own load otherwise. Opening the same artist again is the retry.
    fn failure(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let id = self.detail.read(cx).id()?.to_owned();
        let reason = match trouble::unreachable(&id, cx) {
            true => None,
            false => Some(self.detail.read(cx).error()?.to_owned()),
        };
        let detail = self.detail.clone();

        Some(
            trouble::lost(
                "artist-lost",
                t!("trouble-not-loaded"),
                reason.as_deref(),
                move |_, _, cx| {
                    let id = id.clone();
                    detail.update(cx, |detail, cx| detail.open(&id, cx));
                },
                cx,
            )
            .size_full()
            .into_any_element(),
        )
    }
}

impl Tooled for ArtistView {
    fn toolbar(&self) -> Entity<Toolbar> {
        self.toolbar.clone()
    }

    fn tools(&self, _cx: &App) -> Vec<AnyElement> {
        let viewed = self.me.clone();

        vec![tools::views(&self.popovers, self.mode, move |mode, cx| {
            viewed.update(cx, |view, cx| view.set_mode(mode, cx)).ok();
        })]
    }
}

impl Render for ArtistView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let artist = self.detail.read(cx);
        let owner = artist.id().map(str::to_owned);
        let cover = artist
            .artist()
            .and_then(|artist| artist.cover_large.clone());
        self.cover_viewer.update(cx, |viewer, cx| {
            viewer.close_if_stale(owner.as_deref(), cover.as_deref(), window, cx)
        });
        let fetching = self.detail.read(cx).is_loading() || self.detail.read(cx).is_filling();
        ui::PageEdge::set_loading(fetching, cx);
        if let Some(failure) = self.failure(cx) {
            return div()
                .id("artist-failure")
                .role(Role::Alert)
                .relative()
                .size_full()
                .child(failure)
                .into_any_element();
        }

        let theme = *cx.theme();
        let inset = theme.metrics.inset;
        let previous = self.width;
        page::resize(&self.table, &mut self.width, inset, window, cx);

        let scroll = self.scrollbar.read(cx).scroll().clone();
        if self.width != previous {
            self.scrollbar
                .update(cx, |bar, cx| bar.set_max_offset(None, cx));
        }
        let columns = Shape::new(self.width, self.popular.len()).columns;
        if self.popular_columns != columns {
            self.popular_columns = columns;
            self.popular_page = 0;
        }
        if self.mode == Mode::List {
            self.table.claim(cx);
            let viewport = page::past_border(page::viewport(&scroll, inset, window));
            self.table
                .update(cx, |table, _| table.set_viewport(viewport));
        }

        let picked = self.track_context.and_then(|(place, position)| {
            self.popular
                .get(place)
                .cloned()
                .map(|track| (track, position))
        });
        let track_menu = picked.map(|(track, position)| {
            Popup::new(position, self.track_menu.for_track(&track, cx)).on_close(cx.listener(
                |this, _, _, cx| {
                    this.track_context = None;
                    cx.notify();
                },
            ))
        });

        let listed = self.mode == Mode::List;
        let songs_title = self.songs_title(cx);
        let head = self.header(cx);
        let cover = theme.metrics.cover;
        let banner_shown = self
            .detail
            .read(cx)
            .artist()
            .is_some_and(|artist| artist.banner.is_some());
        let margin = crate::chrome::Chrome::margin(window, cx);
        let rise = match banner_shown {
            true => ((self.width + (inset + margin) * 2.) / BANNER_BOX - inset - cover)
                .max(cover * BANNER_RISE_LEAST)
                .min(cover * BANNER_RISE_MOST),
            false => cover * BANNER_RISE_WASH,
        };
        let rise = snapped(rise, window);
        let banner = self.banner(inset, margin, inset + rise + cover, inset + rise, cx);
        let tracks = match self.mode {
            Mode::Grid => self.popular(cx),
            Mode::List => self.listed(cx),
        };
        let favorites = self.favorites_link(cx);
        let about = self.about(cx);
        while self.rails.len() < Shelf::COUNT {
            self.rails.push(Rail::new(cx.entity_id()));
        }
        for rail in &self.rails {
            rail.sync();
        }
        let weak = cx.entity().downgrade();
        let notify: Rc<dyn Fn(&mut App)> = Rc::new(move |cx: &mut App| {
            weak.update(cx, |_, cx| cx.notify()).ok();
        });
        let rails = self.sections(window, &notify, cx);
        // once the banner and the name under it have scrolled away, the title bar carries it
        let (title, portrait) = self
            .detail
            .read(cx)
            .artist()
            .map(|artist| {
                (
                    SharedString::from(artist.name.clone()),
                    artist.cover_large.clone().map(SharedString::from),
                )
            })
            .unwrap_or_default();
        let page = Scroller::new("artist-page", &self.scrollbar)
            .headed(inset + rise + cover, title, portrait)
            .bottom_inset(crate::chrome::Chrome::bottom(cx))
            .px(inset)
            .pt(inset)
            .pb(inset)
            .child(
                div()
                    .relative()
                    .px(margin)
                    .pt(rise)
                    .children(banner)
                    .child(head)
                    .children(favorites)
                    .when(listed, |this| {
                        let label = i18n::lookup(songs_title, None);
                        this.child(
                            ui::spoken_heading("artist-popular-heading", label, 2, cx).pb_3(),
                        )
                    }),
            )
            .child(
                div()
                    .px(margin)
                    .child(tracks)
                    .children(rails)
                    .children(about),
            );

        div()
            .relative()
            .size_full()
            .child(page)
            .when_some(track_menu, |this, menu| this.child(menu))
            .children(self.about_dialog(cx))
            .children(self.favorites_dialog(window, cx))
            .child(self.cover_viewer.clone())
            .into_any_element()
    }
}

/// The facts under the artist's name: the listener count, then the follower count. The follower
/// count takes its own id, so it keeps the same id whether or not a listener count is shown.
fn artist_meta(listeners: Option<SharedString>, followers: Option<SharedString>) -> HeroMetaStrip {
    let strip = HeroMetaStrip::new();
    let strip = match listeners {
        Some(listeners) => strip.text(listeners),
        None => strip,
    };
    match followers {
        Some(followers) => strip.item(spoken("artist-followers", followers)),
        None => strip,
    }
}

fn chevron(expanded: bool) -> &'static str {
    match expanded {
        true => "icons/chevron-up.svg",
        false => "icons/chevron-down.svg",
    }
}

/// The rails of an artist page, in page order.
#[derive(Clone, Copy)]
enum Shelf {
    Albums,
    Singles,
    Appears,
}

impl Shelf {
    const COUNT: usize = 3;
}
