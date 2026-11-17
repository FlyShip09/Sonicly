mod resume;

use std::rc::Rc;

use crate::chrome::Chrome;
#[cfg(debug_assertions)]
use crate::shared::menus::CardMenu;
use crate::shared::menus::{Item, ItemMenu};
use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, MouseDownEvent, Pixels, Point, Render, ScrollHandle,
    WeakEntity, Window, div, px,
};
use i18n::t;
use music::GenreItem;
use state::{History, HistoryState, Home, Network, Playback, Scan, Sonicly};
use ui::{ActiveTheme as _, Lazy, Mode, Popup, Scrollbar, Scroller};

use crate::shared::album_grid::CardGrid;
use crate::shared::cards;
use crate::shared::cells;
use crate::shared::local;
use crate::shared::picks::{Picks, Shape};
use crate::shared::shelves::{Rail, RailSpec, Shelves};
use crate::shared::tracks::{PlaybackStatus, playback_status};
use crate::shared::trouble;
use resume::Resume;

const STEADY: Pixels = px(0.5);
/// How many albums and songs Recently played holds, as many as a feed's shelf.
const RECENT: usize = 15;
/// Where a capture run opens a context menu, from the window's top left on both axes.
#[cfg(debug_assertions)]
const STAGED_MENU: Pixels = px(360.);

pub(crate) struct HomeView {
    home: Entity<Home>,
    playback: Entity<Playback>,
    playback_status: PlaybackStatus,
    /// The second a paused song is held at, which Continue listening shows.
    held_at: Option<u64>,
    shelves: Entity<Shelves>,
    history: Entity<History>,
    /// Recently played as last drawn. It is taken again from the history only while the page is
    /// out of sight or has none, so a song started here does not shuffle the shelf in view.
    recent: Rc<Vec<GenreItem>>,
    /// Whether the history changed while the page was in view.
    stale: bool,
    visible: bool,
    recent_rail: Rail,
    width: Pixels,
    quick_picks: Pager,
    scrollbar: Entity<Scrollbar>,
    /// The submenu state of a track's context menu.
    menus: ItemMenu,
    context_menu: Option<(Item, Point<Pixels>)>,
}

#[derive(Default)]
struct Pager {
    columns: usize,
    page: usize,
}

impl Pager {
    fn fit(&mut self, columns: usize, pages: usize) -> usize {
        if self.columns != columns {
            self.columns = columns;
            self.page = 0;
        }
        self.page = self.page.min(pages.saturating_sub(1));
        self.page
    }
}

impl HomeView {
    pub(crate) fn new(
        home: Entity<Home>,
        playback: Entity<Playback>,
        cx: &mut Context<Self>,
    ) -> Self {
        let me = cx.entity_id();
        let playlist_scrollbar = cx.new(|_| Scrollbar::inset().watching(me));
        let menus = ItemMenu::new(playlist_scrollbar, cx);

        cx.observe(&home, |this, _, cx| {
            this.menus.reset(cx);
            this.context_menu = None;
            cx.notify();
        })
        .detach();
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();

        let library = Sonicly::global(cx).library.clone();
        cx.observe(&library, |_, _, cx| cx.notify()).detach();

        let shelves = cx.new(|_| Shelves::new("home-shelf", me, playback.clone()));
        cx.observe(&shelves, |_, _, cx| cx.notify()).detach();

        let history = Sonicly::global(cx).history.clone();
        cx.observe(&history, |this: &mut Self, history, cx| {
            match this.visible && !this.recent.is_empty() {
                true => this.stale = true,
                false => {
                    this.recent = Rc::new(history.read(cx).recent_items(RECENT));
                    this.recent_rail.rewind();
                    cx.notify();
                }
            }
        })
        .detach();
        let recent = Rc::new(history.read(cx).recent_items(RECENT));

        let current_playback = playback_status(&playback, cx);
        let current_hold = held_at(&playback, cx);
        cx.observe(&playback, |this, playback, cx| {
            let current = playback_status(&playback, cx);
            let held = held_at(&playback, cx);
            if this.playback_status != current || this.held_at != held {
                this.playback_status = current;
                this.held_at = held;
                cx.notify();
            }
        })
        .detach();

        Self {
            home,
            playback,
            playback_status: current_playback,
            held_at: current_hold,
            shelves,
            history,
            recent,
            stale: false,
            visible: false,
            recent_rail: Rail::new(me),
            width: Pixels::ZERO,
            quick_picks: Pager::default(),
            scrollbar: cx.new(|_| Scrollbar::new(ScrollHandle::new()).watching(me)),
            menus,
            context_menu: None,
        }
    }

    /// Tells the feed whether this page is the one on screen, so it lands changes to what is
    /// drawn only while nobody is looking.
    pub(crate) fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if !visible && std::mem::take(&mut self.stale) {
            self.recent = Rc::new(self.history.read(cx).recent_items(RECENT));
            self.recent_rail.rewind();
            cx.notify();
        }
        self.home
            .update(cx, |home, cx| home.set_visible(visible, cx));
    }

    /// Fetches the feed and the library it mixes from again.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.home.update(cx, |home, cx| home.refresh(cx));
    }

    /// Debug builds only, for capture runs: once the feed lists one, opens the first album,
    /// artist (an album's lead artist when the feed lists none) or playlist on it, or the
    /// context menu of the first album for `menu`.
    #[cfg(debug_assertions)]
    pub(crate) fn stage(&mut self, what: &str, cx: &mut Context<Self>) {
        use router::Destination;

        let what = what.to_owned();
        let done = Rc::new(std::cell::Cell::new(false));
        cx.observe(&self.home, move |this, home, cx| {
            if done.get() {
                return;
            }
            let sections = home.read(cx).sections();
            let items = || sections.iter().flat_map(|section| section.items.iter());
            let album = items().find_map(|item| match item {
                GenreItem::Album(album) => Some(album.clone()),
                _ => None,
            });
            let destination = match what.as_str() {
                "menu" => {
                    let Some(album) = album else {
                        return;
                    };
                    done.set(true);
                    let at = gpui::point(STAGED_MENU, STAGED_MENU);
                    CardMenu::open(Item::Album(album), this.playback.clone(), at, cx);
                    return;
                }
                "album" => album.map(|album| Destination::Album(album.id.into())),
                "artist" => items()
                    .find_map(|item| match item {
                        GenreItem::Artist(artist) => Some(artist.id.clone()),
                        _ => None,
                    })
                    .or_else(|| album?.artist_refs.into_iter().find_map(|artist| artist.id))
                    .map(|id| Destination::Artist(id.into())),
                "playlist" => items().find_map(|item| match item {
                    GenreItem::Playlist(list) => {
                        Some(Destination::Playlist(list.id.clone().into()))
                    }
                    _ => None,
                }),
                _ => None,
            };
            if let Some(destination) = destination {
                done.set(true);
                router::navigate(destination, cx);
            }
        })
        .detach();
    }

    /// Whether Quick picks or the shelves are still on their way.
    pub(crate) fn is_loading(&self, cx: &App) -> bool {
        let home = self.home.read(cx);
        home.is_loading(cx) || home.is_feeding()
    }

    /// The Quick picks deck, the one deck on the page: what the provider lists as played lately
    /// and its picks after them, a skeleton while they are on their way.
    fn quick_picks(&mut self, width: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let items = self.home.read(cx).quick_picks();
        let shape = Shape::new(width, items.len());
        let pages = shape.pages;
        let page = self.quick_picks.fit(shape.columns, shape.pages);
        let opened = items.clone();
        let home = cx.entity().downgrade();

        Picks::mixed("quick-picks", items, self.playback.clone(), width, page)
            .title("home-quick-picks")
            .vacancy(self.home.read(cx).vacancy(cx))
            .loading(self.home.read(cx).is_loading(cx))
            .on_previous(cx.listener(|this, _, _, cx| {
                this.quick_picks.page = this.quick_picks.page.saturating_sub(1);
                this.context_menu = None;
                cx.notify();
            }))
            .on_next(cx.listener(move |this, _, _, cx| {
                this.quick_picks.page = (this.quick_picks.page + 1).min(pages.saturating_sub(1));
                this.context_menu = None;
                cx.notify();
            }))
            .on_context_menu(move |place, event, _, cx| {
                open_item_menu(&home, &opened, place, event, cx);
            })
            .into_any_element()
    }

    /// Recently played as a rail of albums and songs: skeletons while the history is still being
    /// read, and nothing once it is read and holds no plays.
    fn recently_played(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let grid = CardGrid::layout(self.width);
        let tile = grid.card;
        let columns = grid.columns.max(1);
        // The song Continue listening shows leads the history; the rail lists what came before.
        let shown = self.playback.read(cx).shown_track(cx);
        let skip = self
            .recent
            .first()
            .is_some_and(|item| holds(item, shown.as_ref())) as usize;
        let count = self.recent.len() - skip;
        if count == 0 {
            let loading = matches!(self.history.read(cx).state(), HistoryState::Loading);
            return (loading && self.recent.is_empty())
                .then(|| Rail::pending(t!("home-recently-played"), tile, columns, window, cx));
        }
        self.recent_rail.sync();
        let weak = cx.entity().downgrade();
        let notify: Rc<dyn Fn(&mut App)> = Rc::new(move |cx: &mut App| {
            weak.update(cx, |_, cx| cx.notify()).ok();
        });
        let items = self.recent.clone();
        let playback = self.playback.clone();

        Some(self.recent_rail.render(
            RailSpec {
                tag: "home-recent",
                place: 0,
                title: t!("home-recently-played"),
                count,
                tile,
                columns,
                tabs: None,
            },
            window,
            cx,
            &notify,
            move |index, _, cx| {
                let Some(item) = items.get(index + skip) else {
                    return div().into_any_element();
                };
                cards::item_card(recent_id(item), item, &playback, cx)
                    .tile(tile)
                    .flat()
                    .aria_position_in_set(index + 1)
                    .aria_size_of_set(count)
                    .into_any_element()
            },
        ))
    }

    /// What a signed-out library with no music folder shows in place of Quick picks and the
    /// shelves: that a folder is what fills it, and the button that adds one.
    fn unconfigured(&self, cx: &App) -> Option<AnyElement> {
        let home = self.home.read(cx);
        let session = Sonicly::global(cx).session.read(cx);
        let empty = home.is_local(cx)
            && !session.is_pending()
            && !home.is_feeding()
            && home.quick_picks().is_empty()
            && home.sections().is_empty()
            && session.local_paths().is_empty()
            && Scan::global(cx).read(cx).progress().is_none();

        empty.then(|| {
            div()
                .pt(cx.theme().metrics.section)
                .child(local::unconfigured("home-unconfigured"))
                .into_any_element()
        })
    }

    /// The page home shows in place of its shelves: the No connection state the moment the
    /// network is gone, since every row here comes from the provider, and the reason its feed
    /// failed otherwise. Asking for the feed again is the retry, and it starts the pauses
    /// between tries over.
    fn failure(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let reason = match Network::lost(cx) {
            true => None,
            false => Some(self.home.read(cx).error()?.to_owned()),
        };
        let home = self.home.clone();

        Some(
            trouble::lost(
                "home-lost",
                t!("trouble-not-loaded"),
                reason.as_deref(),
                move |_, _, cx| {
                    home.update(cx, |home, cx| home.retry(cx));
                },
                cx,
            )
            .size_full()
            .into_any_element(),
        )
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(failure) = self.failure(cx) {
            return div()
                .id("home-failure")
                .role(Role::Alert)
                .size_full()
                .child(failure)
                .into_any_element();
        }

        let theme = *cx.theme();
        let available = cells::content_width(window, theme.metrics.inset * 2., cx);
        if (available - self.width).abs() >= STEADY {
            self.width = available;
        }
        let context_menu = self.context_menu.clone().map(|(item, position)| {
            let menu = item.menu(&self.menus, self.playback.clone(), false, cx);
            Popup::new(position, menu).on_close(cx.listener(|this, _, _, cx| {
                this.context_menu = None;
                cx.notify();
            }))
        });

        let view = cx.entity();
        let picks = Lazy::new("home-quick-picks", move |_, cx| {
            view.update(cx, |this, cx| this.quick_picks(available, cx))
        });
        let sections = self.home.read(cx).sections();
        let feeding = self.home.read(cx).is_feeding();
        // Quick picks that came back empty say so only on a page with nothing else on it.
        let pickless = !sections.is_empty()
            && self.home.read(cx).quick_picks().is_empty()
            && !self.home.read(cx).is_loading(cx);
        let width = self.width;
        let shelves = self
            .shelves
            .update(cx, |shelves, cx| match sections.is_empty() && feeding {
                true => shelves.pending(width, cx),
                false => vec![shelves.render(sections, Mode::Grid, width, window, cx)],
            });

        let resume = Resume::shown(&self.playback, cx).then(|| Resume::new(self.playback.clone()));
        let recent = self.recently_played(window, cx);
        let unconfigured = self.unconfigured(cx);

        Scroller::new("home-page", &self.scrollbar)
            .page()
            .hold()
            .bottom_inset(crate::chrome::Chrome::bottom(cx))
            .py(theme.metrics.inset)
            .px(theme.metrics.inset + crate::chrome::Chrome::margin(window, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(theme.metrics.section)
                    .children(resume)
                    .children(recent)
                    .map(|this| match unconfigured {
                        Some(vacancy) => this.child(vacancy),
                        None => this
                            .when(!pickless, |this| {
                                this.child(ui::held("home-quick-picks", picks))
                            })
                            .children(shelves),
                    }),
            )
            .when_some(context_menu, |this, menu| this.child(menu))
            .into_any_element()
    }
}

fn open_item_menu(
    home: &WeakEntity<HomeView>,
    items: &Rc<Vec<GenreItem>>,
    place: usize,
    event: &MouseDownEvent,
    cx: &mut gpui::App,
) {
    let Some(item) = items.get(place).and_then(Item::of) else {
        return;
    };
    let Some(home) = home.upgrade() else {
        return;
    };
    home.update(cx, |this, cx| {
        this.menus.reset(cx);
        this.context_menu = Some((item, event.position));
        cx.notify();
    });
}

/// The second a paused song is held at, or nothing while it plays.
fn held_at(playback: &Entity<Playback>, cx: &App) -> Option<u64> {
    let playback = playback.read(cx);
    match playback.control() == Some(true) {
        true => None,
        false => Some(playback.shown_time(cx).0.as_secs()),
    }
}

/// Whether a Recently played item is the song in the player, or the album it is playing from.
fn holds(item: &GenreItem, track: Option<&music::Track>) -> bool {
    let Some(track) = track else {
        return false;
    };
    match item {
        GenreItem::Album(album) => track.album_id.as_deref() == Some(album.id.as_str()),
        GenreItem::Track(played) => played.id.is_some() && played.id == track.id,
        _ => false,
    }
}

/// A Recently played card's id, from what it opens rather than where it stands.
fn recent_id(item: &GenreItem) -> gpui::ElementId {
    let key = match item {
        GenreItem::Album(album) => format!("home-recent-album-{}", album.id),
        GenreItem::Track(track) => {
            format!("home-recent-track-{}", track.id.as_deref().unwrap_or(""))
        }
        _ => String::from("home-recent-other"),
    };
    gpui::ElementId::Name(key.into())
}
