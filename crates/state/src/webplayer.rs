//! The hidden browser page a service's own web player runs in.
//!
//! Some services protect their streams and play them only in their own apps and web player.
//! Sonicly does not get around that. When a provider's tracks can only play that way, this entity
//! opens the service's player page in the system web view, off the screen, and the page plays the
//! track with the web view's own licensed module, straight to the system's output. The provider
//! says what to do through `music::webplayer`, and this entity only carries it out: open the
//! page, run a line of script in it, close it. Nothing here names a provider or reads a message.
//!
//! The page costs what a browser tab costs, a few hundred megabytes in processes of its own, so
//! it exists only while it is wanted: the first track asked to play opens it, and it closes
//! [`LINGER`] after the provider says it has nothing to play, or when the player that opened it
//! goes away.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{App, AppContext as _, Context, Entity, Task};
use music::webplayer::{Ask, Heard, Listener, Order, Site};

/// How long a page with nothing to play stays open, for a pause that ends in a play. Shorter
/// than the pause after which playback reloads its track anyway.
const LINGER: Duration = Duration::from_secs(3 * 60);
/// How long a fresh page gets to say anything at all before it is given up on.
const PATIENCE: Duration = Duration::from_secs(45);

/// The player page, while one is open, and who opened it.
pub struct WebPlayer {
    open: Option<Open>,
    _orders: Task<()>,
}

struct Open {
    /// The remote that opened the page. Orders from any other are an earlier player's, and are
    /// dropped.
    owner: u64,
    page: webview::Page,
    heard: Listener,
    /// Closes the page if it has said nothing [`PATIENCE`] after it opened.
    _waiting: Task<()>,
    /// Closes the page once it has had nothing to play for [`LINGER`].
    idle: Option<Task<()>>,
}

impl WebPlayer {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut orders = music::webplayer::serve();
        let pump = cx.spawn(async move |this, cx| {
            while let Some(order) = orders.recv().await {
                if this.update(cx, |this, cx| this.carry(order, cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            open: None,
            _orders: pump,
        }
    }

    fn carry(&mut self, order: Order, cx: &mut Context<Self>) {
        let Order { from, ask } = order;
        let ask = match ask {
            Ask::Open { site, heard } => return self.start(from, site, heard, cx),
            ask => ask,
        };
        let Some(open) = self.open.as_mut().filter(|open| open.owner == from) else {
            return;
        };
        match ask {
            Ask::Run(script) => open.page.run(&script),
            Ask::Muted(muted) => open.page.mute(muted),
            Ask::Idle(true) if open.idle.is_none() => {
                open.idle = Some(Self::close_after(from, LINGER, None, cx));
            }
            Ask::Idle(true) => {}
            Ask::Idle(false) => open.idle = None,
            Ask::Close | Ask::Open { .. } => {
                log::debug!("webplayer: closing the player page");
                self.open = None;
            }
        }
    }

    /// Opens `site` hidden, in place of whatever page is open, and gives it [`PATIENCE`] to say
    /// its first word.
    fn start(&mut self, owner: u64, site: Site, heard: Listener, cx: &mut Context<Self>) {
        self.open = None;
        let target = webview::Target {
            url: site.url,
            landing: site.host,
            domain: site.domain,
            proof: Vec::new(),
            title: String::new(),
            agent: None,
            script: Some(site.script),
            cookies: site.cookies,
            player: true,
        };
        let page = match webview::Page::open(target) {
            Ok(page) => page,
            Err(error) => {
                log::warn!("webplayer: cannot open the player page: {error:#}");
                return heard(Heard::Closed);
            }
        };
        log::debug!("webplayer: opened a player page");
        let spoke = Arc::new(AtomicBool::new(false));
        let (listener, answered) = (heard.clone(), spoke.clone());
        page.listen(move |message| {
            answered.store(true, Ordering::Relaxed);
            listener(Heard::Message(message));
        });
        self.open = Some(Open {
            owner,
            page,
            heard,
            _waiting: Self::close_after(owner, PATIENCE, Some(spoke), cx),
            idle: None,
        });
    }

    /// A timer that closes `owner`'s page after `wait` and tells its listener, unless it is
    /// dropped first or `spoke` says the page has answered by then.
    fn close_after(
        owner: u64,
        wait: Duration,
        spoke: Option<Arc<AtomicBool>>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            this.update(cx, |this, _| {
                let Some(open) = this.open.as_ref().filter(|open| open.owner == owner) else {
                    return;
                };
                if spoke.is_some_and(|spoke| spoke.load(Ordering::Relaxed)) && !open.page.closed() {
                    return;
                }
                log::debug!("webplayer: closing a player page nobody is using");
                let heard = open.heard.clone();
                this.open = None;
                heard(Heard::Closed);
            })
            .ok();
        })
    }
}

/// Starts carrying out web player orders, where this system's web view can run a player page.
/// Elsewhere nothing serves, and a provider that needs one says its tracks cannot play here.
pub fn attach(cx: &mut App) -> Option<Entity<WebPlayer>> {
    webview::plays().then(|| cx.new(WebPlayer::new))
}
