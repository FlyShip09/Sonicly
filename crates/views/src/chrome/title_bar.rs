use std::time::{Duration, Instant};

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use gpui::Decorations;
use gpui::accesskit::Role;
use gpui::prelude::*;
use gpui::{AnyElement, App, SharedString, Window, div, px};
use gpui::{
    AnyView, Context, Entity, EventEmitter, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Render,
};
use ui::TrafficLightControls;
use ui::WindowControls;
use ui::{
    ActiveTheme as _, Artwork, Button, HoverFade, MenuItem, Motion, PageEdge, Picker, Popovers,
    Room, Spin, Text, dim_span,
};

use crate::chrome::Chrome;
use crate::chrome::SidebarRight;
use crate::chrome::downloads::DownloadsButton;
use i18n::t;
use input::Refresh;
use router::{Destination, Navigation, SettingsPage, navigate, open_settings};
use state::{AppSettings, Sonicly, Stars};

const SYSTEM_ZOOMS: bool = cfg!(target_os = "windows");
/// A hairline: the bar's own bottom edge, which the page's surface reaches over, and the edge of
/// the right sidebar continued up through the bar.
const EDGE: Pixels = px(1.);
/// The cover beside a page's compact heading in the bar, and how far below its place the heading
/// starts as it comes in.
const HEADING_COVER: Pixels = px(20.);
const HEADING_RISE: Pixels = px(4.);
/// How long a page waits on the network before the load line shows, so a quick answer shows
/// nothing.
const LOAD_DELAY: Duration = Duration::from_millis(200);
/// How thick the load line is, how much of the bar's width its bright stretch covers, and how
/// long the stretch takes to cross.
const LOAD_LINE: Pixels = px(2.);
const LOAD_RUN: f32 = 0.3;
const LOAD_TURN: Duration = Duration::from_millis(1200);

/// When the load line's clock started, so a line that comes back carries on where it was.
static LOAD_EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Tells whether the page a refresh reloaded is still waiting for its data.
pub(crate) type Busy = Box<dyn Fn(&App) -> bool>;

/// The height of AppKit's traffic-light buttons, which the bar centres them by.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_SIZE: f32 = 14.;
/// The extra room round corners leave before the traffic lights, since their wider curve
/// otherwise crowds the close button.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_ROUND_PAD: f32 = 4.;
/// The bar's bottom border, which sits inside its height and so out of the centring.
#[cfg(target_os = "macos")]
const TITLE_BAR_BORDER: f32 = 1.;
/// Where hidden traffic lights go: past the window's left edge, where nothing draws or hits them.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_HIDDEN: f32 = -200.;
/// The width of the three traffic lights and the gap the title bar leaves after them.
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_SPAN: f32 = 65.;

#[derive(Clone, PartialEq)]
pub(crate) struct TitleBarOptions {
    pub navigation: bool,
    pub sidebar_open: bool,
    pub sidebar_right: Option<bool>,
    pub border: bool,
    pub content: Option<AnyView>,
    /// Lets the fullscreen ambient background show through behind the controls.
    pub transparent: bool,
}

impl Default for TitleBarOptions {
    fn default() -> Self {
        Self {
            navigation: false,
            sidebar_open: false,
            sidebar_right: None,
            border: true,
            content: None,
            transparent: false,
        }
    }
}

pub(crate) enum TitleBarEvent {
    ToggleSidebar,
    ToggleSidebarRight,
}

pub(crate) struct TitleBar {
    navigation: Entity<Navigation>,
    settings: Entity<AppSettings>,
    options: TitleBarOptions,
    grabbed: bool,
    /// Set by a refresh and dropped once the page has its data or the user moves on, so the
    /// refresh button turns for exactly that long.
    busy: Option<Busy>,
    /// The refresh icon's turn, which springs back to rest once `busy` is gone.
    spin: Spin,
    downloads: Entity<DownloadsButton>,
    /// The account menu's group, so it closes when another of the bar's menus opens.
    popovers: Popovers,
    /// The hairline under the bar, which shows while the page has scrolled beneath it.
    under: HoverFade,
    /// The page's compact heading, which shows once its large header has scrolled away.
    heading: HoverFade,
    /// Since when the page has been waiting on the network, while it is.
    waiting: Option<Instant>,
    /// The thin line under the bar that a wait long enough to notice raises.
    load: HoverFade,
}

impl EventEmitter<TitleBarEvent> for TitleBar {}

impl TitleBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let navigation = router::trail(cx);
        let settings = Sonicly::global(cx).settings.clone();

        cx.observe(&navigation, |this, _, cx| {
            this.busy = None;
            cx.notify();
        })
        .detach();
        cx.observe(&settings, |_, _, cx| cx.notify()).detach();
        cx.observe(&Sonicly::global(cx).session.clone(), |_, _, cx| cx.notify())
            .detach();
        cx.observe(&Stars::global(cx), |_, _, cx| cx.notify())
            .detach();
        // the star button drops its words when the window is narrow
        let chrome = Chrome::entity(cx);
        cx.observe(&chrome, |_, _, cx| cx.notify()).detach();
        let edge = PageEdge::entity(cx);
        cx.observe(&edge, |_, _, cx| cx.notify()).detach();
        Self {
            navigation,
            settings,
            options: TitleBarOptions::default(),
            grabbed: false,
            busy: None,
            spin: Spin::default(),
            downloads: cx.new(DownloadsButton::new),
            popovers: Popovers::default(),
            under: HoverFade::new(Motion::Quick.span()).leaving(dim_span()),
            heading: HoverFade::new(Motion::Quick.span()),
            waiting: None,
            load: HoverFade::new(Motion::Quick.span()),
        }
    }

    /// The page-load line: a short bright stretch running along the bar's foot while the page
    /// on show has waited on the network for `LOAD_DELAY`, fading out once it lands. It asks
    /// for frames only while it shows or is about to.
    fn load_line(
        &mut self,
        loading: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        let since = match loading {
            true => Some(*self.waiting.get_or_insert_with(Instant::now)),
            false => self.waiting.take().and(None),
        };
        let due = since.is_some_and(|since| since.elapsed() >= LOAD_DELAY);
        if since.is_some() && !due {
            window.request_animation_frame();
        }
        self.load.set(due);
        let shown = self.load.level(window, cx);
        if shown <= 0. {
            return None;
        }
        let theme = cx.theme();
        // a reduced-motion line holds still across the middle
        let at = match cx.reduce_motion() {
            true => (1. - LOAD_RUN) / 2.,
            false => {
                window.request_animation_frame();
                let epoch = *LOAD_EPOCH.get_or_init(Instant::now);
                let turn = (epoch.elapsed().as_secs_f32() / LOAD_TURN.as_secs_f32()).fract();
                turn * (1. + LOAD_RUN) - LOAD_RUN
            }
        };
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(LOAD_LINE)
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(gpui::relative(at))
                        .w(gpui::relative(LOAD_RUN))
                        .bg(theme.primary.opacity(shown)),
                ),
        )
    }

    /// Turns the refresh button until `busy` says the reloaded page has its data.
    pub fn refreshing(&mut self, busy: Busy, cx: &mut Context<Self>) {
        self.busy = Some(busy);
        cx.notify();
    }

    pub fn set_options(&mut self, options: TitleBarOptions, cx: &mut Context<Self>) {
        if self.options == options {
            return;
        }
        self.options = options;
        cx.notify();
    }

    fn history(&mut self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.busy.as_ref().is_some_and(|busy| !busy(cx)) {
            self.busy = None;
        }
        match self.busy.is_some() {
            true => self.spin.start(),
            false => self.spin.stop(),
        }
        let turn = self.spin.turn(window, cx);
        let hover = cx.theme().sidebar_accent;
        let navigation = self.navigation.read(cx);
        let (can_back, can_forward) = (navigation.can_go_back(), navigation.can_go_forward());
        let refreshes = navigation.current().refreshes();
        let back = self.navigation.clone();
        let forward = self.navigation.clone();

        div()
            .id("titlebar-history")
            .role(Role::Toolbar)
            .aria_label(i18n::lookup("nav-trail", None))
            .flex()
            .flex_none()
            .items_center()
            .gap_1()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("history-back")
                    .ghost()
                    .icon("icons/chevron-left.svg")
                    .tooltip("nav-back")
                    .disabled(!can_back)
                    .small()
                    .when(can_back, |button| {
                        button
                            .hover(move |style| style.bg(hover))
                            .active(move |style| style.bg(hover))
                    })
                    .on_click(move |_, _, cx| {
                        back.update(cx, |navigation, cx| navigation.back(cx))
                    }),
            )
            .child(
                Button::new("history-forward")
                    .ghost()
                    .icon("icons/chevron-right.svg")
                    .tooltip("nav-forward")
                    .disabled(!can_forward)
                    .small()
                    .when(can_forward, |button| {
                        button
                            .hover(move |style| style.bg(hover))
                            .active(move |style| style.bg(hover))
                    })
                    .on_click(move |_, _, cx| {
                        forward.update(cx, |navigation, cx| navigation.forward(cx))
                    }),
            )
            .when(refreshes, |history| {
                history.child(
                    Button::new("history-refresh")
                        .ghost()
                        .icon("icons/refresh-cw.svg")
                        .turned(turn)
                        .tooltip("app-refresh")
                        .small()
                        .hover(move |style| style.bg(hover))
                        .active(move |style| style.bg(hover))
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(Refresh), cx)),
                )
            })
    }

    fn lyrics_toggle(&self, open: bool, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_none()
            .items_center()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("sidebar-right-toggle")
                    .ghost()
                    .small()
                    .icon(match open {
                        true => "icons/panel-right-close.svg",
                        false => "icons/panel-right-open.svg",
                    })
                    .tooltip("nav-sidebar-right")
                    .aria_expanded(open)
                    .on_click(
                        cx.listener(|_, _, _, cx| cx.emit(TitleBarEvent::ToggleSidebarRight)),
                    ),
            )
    }

    /// The link to Sonicly's repository: a star and, once GitHub has answered, how many it has.
    /// Without a count it says what it does instead, and a narrow window keeps only the star.
    fn star(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = Stars::global(cx).read(cx).label();
        let wide = Chrome::room(window, cx).fits(Room::Wide);

        div()
            .flex()
            .flex_none()
            .items_center()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("github-star")
                    .ghost()
                    .small()
                    .icon("icons/star.svg")
                    .map(|button| match (count, wide) {
                        (Some(count), _) => button
                            .label(count)
                            .name(t!("github-star"))
                            .tooltip("github-star"),
                        (None, true) => button.label(t!("github-star")),
                        (None, false) => button.tooltip("github-star"),
                    })
                    .on_click(|_, _, cx| cx.open_url(state::REPO_URL)),
            )
    }

    /// The account button and the menu it opens: the connected services to switch between, a
    /// way to manage them, then History and Settings. A guest session shows as guest mode, and
    /// only while it is the one in use.
    fn account(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let session = Sonicly::global(cx).session.clone();
        let guest = session
            .read(cx)
            .providers()
            .any(|info| info.active && info.guest);
        let mut services: Vec<MenuItem> = session
            .read(cx)
            .connected()
            .map(|info| {
                let slug = info.slug;
                let session = session.clone();

                MenuItem::new(SharedString::from(format!("account-{slug}")), info.name)
                    .icon(crate::shared::provider_logo(slug))
                    .selected(info.active)
                    .on_click(move |_, _, cx| {
                        session.update(cx, |session, cx| session.switch(slug, cx));
                    })
            })
            .collect();
        if guest {
            services.push(
                MenuItem::new("account-guest", t!("login-guest-title"))
                    .icon("icons/hat-glasses.svg")
                    .selected(true),
            );
        }
        // with no service to switch between, the menu starts at its own rows, not a rule
        if !services.is_empty() {
            services.push(MenuItem::separator("account-services"));
        }

        div()
            .flex()
            .flex_none()
            .items_center()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Picker::icon("account", &self.popovers, "icons/circle-user-round.svg")
                    .tooltip("tool-account")
                    .width(Picker::REGULAR)
                    .items(services)
                    .item(
                        MenuItem::new("account-manage", t!("account-manage"))
                            .icon("icons/user-round.svg")
                            .on_click(|_, _, cx| {
                                navigate(Destination::Settings(SettingsPage::Providers), cx)
                            }),
                    )
                    .item(MenuItem::separator("account-pages"))
                    .item(
                        MenuItem::new("account-history", t!("nav-history"))
                            .icon("icons/rotate-ccw-clock.svg")
                            .on_click(|_, _, cx| navigate(Destination::History, cx)),
                    )
                    .item(
                        MenuItem::new("account-settings", t!("nav-settings"))
                            .icon("icons/settings.svg")
                            .on_click(|_, _, cx| open_settings(cx)),
                    ),
            )
    }

    fn toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let icon = match self.options.sidebar_open {
            true => "icons/panel-left-close.svg",
            false => "icons/panel-left-open.svg",
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("sidebar-toggle")
                    .ghost()
                    .flex()
                    .small()
                    .icon(icon)
                    .tooltip("nav-sidebar")
                    .aria_expanded(self.options.sidebar_open)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(TitleBarEvent::ToggleSidebar))),
            )
    }
}

impl Render for TitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.theme();
        let height = ui::snapped(theme.metrics.title_bar, window);
        let navigation = self.options.navigation;
        let sidebar_right = self
            .options
            .sidebar_right
            .filter(|_| SidebarRight::available(window));
        let content = self.options.content.clone();
        let window_fullscreen = window.is_fullscreen();
        let settings = self.settings.read(cx);
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let controls =
            matches!(window.window_decorations(), Decorations::Client { .. }) && !window_fullscreen;
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let controls = settings.window_controls() && !window_fullscreen;
        // Fullscreen hides the traffic lights, so the toggle sits as far from the left edge as
        // the right sidebar's toggle does from the right.
        let inset = traffic_lights_inset(settings, height).filter(|_| !window_fullscreen);
        let decorated = cfg!(not(target_os = "macos")) && controls;
        let leading = decorated && settings.controls_on_left();
        #[cfg(not(target_os = "macos"))]
        let traffic_light = settings.traffic_light_controls();
        #[cfg(target_os = "macos")]
        let traffic_light = false;
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let radius = crate::chrome::window_radius(settings, cx, window);
        #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
        let radius: Option<Pixels> = None;
        // In the workspace the page's own surface runs up under the bar from the sidebar's edge,
        // so the bar reads as the top of the page and the sidebar as one full-height column.
        let chrome = Chrome::get(cx);
        let unified = navigation && !self.options.transparent && !theme.transparent;
        let (left, right) = (chrome.left(), chrome.right());
        // the hairline and the compact heading follow the page under the bar
        let edge = PageEdge::get(cx);
        self.under.set(navigation && edge.under);
        self.heading
            .set(navigation && edge.past && edge.heading.is_some());
        let under = self.under.level(window, cx);
        let load_line = self.load_line(navigation && edge.loading, window, cx);
        let shown = self.heading.level(window, cx);
        let heading = edge.heading.filter(|_| shown > 0.).map(|(title, cover)| {
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .relative()
                .top(HEADING_RISE * (1. - shown))
                .opacity(shown)
                .child(Artwork::new(cover).size(HEADING_COVER).seed(title.clone()))
                .child(
                    ui::spoken("title-bar-heading", title)
                        .min_w_0()
                        .truncate()
                        .text_size(theme.text(Text::Label))
                        .font_weight(gpui::FontWeight::SEMIBOLD),
                )
        });

        div()
            .id("title-bar")
            .role(Role::Banner)
            .flex()
            .items_center()
            .w_full()
            .h(height)
            .flex_none()
            .when_some(radius, |this, radius| this.rounded_t(radius))
            .when_some(
                ui::frame_fill(cx).filter(|_| !self.options.transparent),
                |this, fill| this.bg(fill),
            )
            // The height is a border box, so a bar that drops its border centres its controls
            // one pixel lower than one that keeps it. The edge is always an edge; only its
            // colour goes, and fullscreen's controls land where the workspace's do.
            .border_b_1()
            .border_color(match self.options.border {
                true => theme.title_bar_border,
                false => gpui::transparent_black(),
            })
            .window_control_area(gpui::WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    |this, event: &MouseDownEvent, window, _| match event.click_count {
                        1 => this.grabbed = true,
                        2 if !SYSTEM_ZOOMS => titlebar_double_click(window),
                        _ => {}
                    },
                ),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| this.grabbed = false),
            )
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, _| this.grabbed = false))
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, window, _| {
                if this.grabbed {
                    this.grabbed = false;
                    window.start_window_move();
                }
            }))
            .when(unified, |this| {
                this.relative().child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom(-EDGE)
                        .left(left)
                        .right_0()
                        .bg(theme.background)
                        .when(left > Pixels::ZERO, |this| {
                            this.border_l_1().border_color(theme.sidebar_border)
                        })
                        .when(under > 0., |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .left_0()
                                    .right_0()
                                    .bottom_0()
                                    .h(EDGE)
                                    .bg(theme.title_bar_border.opacity(under)),
                            )
                        })
                        .when_some(load_line, |this, line| this.child(line))
                        .when(right > Pixels::ZERO, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .right(right - EDGE)
                                    .w(EDGE)
                                    .bg(theme.border),
                            )
                        }),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    // The sidebar's toggle stays over the sidebar, and the history starts where
                    // the page does.
                    .when(unified, |this| this.min_w(left))
                    .map(|this| match (leading, inset) {
                        (true, _) => this.pl_2(),
                        (false, Some(inset)) => this.pl(inset),
                        (false, None) => this.pl_3(),
                    })
                    // The sidebar toggle sits as close to the history buttons as they sit to
                    // each other.
                    .map(|this| match navigation {
                        true => this.pr_1(),
                        false => this.pr_3(),
                    })
                    .gap_1()
                    .when(leading, |this| {
                        this.child(window_controls(true, traffic_light))
                    })
                    .when(navigation, |this| this.child(self.toggle(cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .items_center()
                    .when(navigation, |this| this.child(self.history(window, cx)))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .px_2()
                            .overflow_hidden()
                            .children(heading),
                    )
                    .when(navigation, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .occlude()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(self.downloads.clone()),
                        )
                    })
                    .children(content)
                    .map(|this| match navigation || sidebar_right.is_some() {
                        true => this.pr_1(),
                        false => this.pr_3(),
                    }),
            )
            .when(navigation || sidebar_right.is_some(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap_1()
                        .pr_3()
                        .when(navigation, |this| this.pl_3())
                        .when(navigation, |this| {
                            this.child(self.star(window, cx)).child(self.account(cx))
                        })
                        .when_some(sidebar_right, |this, open| {
                            this.child(self.lyrics_toggle(open, cx))
                        }),
                )
            })
            .when(decorated && !leading, |this| {
                this.child(
                    div()
                        .flex_none()
                        .when(cfg!(target_os = "windows") && !traffic_light, |this| {
                            this.h_full().self_stretch()
                        })
                        .when(!cfg!(target_os = "windows") || traffic_light, |this| {
                            this.pr_2()
                        })
                        .child(window_controls(false, traffic_light)),
                )
            })
    }
}

/// Where AppKit's traffic lights sit for the current settings and a title bar of `height`,
/// measured from the window's top-left corner. They are centred in the bar and kept as far from
/// the left edge as from the top, which centres them in the corner, with a little more room for
/// round corners. Hidden controls are moved off the window, since AppKit rebuilds them.
#[cfg(target_os = "macos")]
pub fn traffic_lights(settings: &AppSettings, height: Pixels) -> gpui::Point<Pixels> {
    let left = match settings.window_controls() {
        true => traffic_lights_left(settings, height),
        false => gpui::px(TRAFFIC_LIGHTS_HIDDEN),
    };
    gpui::point(left, traffic_lights_offset(height))
}

#[cfg(target_os = "macos")]
fn traffic_lights_left(settings: &AppSettings, height: Pixels) -> Pixels {
    let pad = match settings.window_rounding() {
        ui::Rounding::Round => gpui::px(TRAFFIC_LIGHTS_ROUND_PAD),
        _ => Pixels::ZERO,
    };
    traffic_lights_offset(height) + pad
}

#[cfg(target_os = "macos")]
fn traffic_lights_offset(height: Pixels) -> Pixels {
    let room = height - gpui::px(TITLE_BAR_BORDER + TRAFFIC_LIGHTS_SIZE);
    room / 2.
}

/// How far past the traffic lights the title bar's content starts while macOS shows them, or
/// `None` when it takes the same padding as the right edge.
fn traffic_lights_inset(settings: &AppSettings, height: Pixels) -> Option<Pixels> {
    #[cfg(target_os = "macos")]
    if settings.window_controls() {
        return Some(traffic_lights_left(settings, height) + gpui::px(TRAFFIC_LIGHTS_SPAN));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (settings, height);
    None
}

fn window_controls(leading: bool, traffic_light: bool) -> AnyElement {
    match traffic_light {
        true => TrafficLightControls::new(leading).into_any_element(),
        false => WindowControls::new(leading).into_any_element(),
    }
}

/// Runs the title bar's double-click action. macOS performs the action set in System Settings;
/// the window zooms everywhere else, since there is no such preference.
fn titlebar_double_click(window: &Window) {
    #[cfg(target_os = "macos")]
    window.titlebar_double_click();
    #[cfg(not(target_os = "macos"))]
    window.zoom_window();
}
