//! The hand-off between a provider whose tracks only play inside the service's own web player
//! and the browser page the app opens for it.
//!
//! Such a service protects its streams, and the one licensed way to hear them outside its own
//! apps is its web player in a real browser engine, where the engine's own content decryption
//! module does the work. This crate has no browser engine, so a provider sends [`Order`]s here
//! and the app carries them out in a hidden page of the system web view: open the player page,
//! run a line of script in it, close it. What the page says comes back as text through the
//! [`Listener`] the provider opened it with. Nothing here, and nothing on either side of it,
//! ever sees a media key, a licence or a decrypted sample: the sound goes from the page straight
//! to the system's output.
//!
//! Nothing here knows what a browser is, and the app side never names a provider.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// The page a provider wants open: its player, the session cookies that make it the listener's,
/// and the script that drives it.
#[derive(Clone)]
pub struct Site {
    pub url: String,
    /// The host the page lives on. Only a document of this host is listened to.
    pub host: String,
    /// The domain `cookies` are set for.
    pub domain: String,
    /// Cookies put into the page's fresh session before it first loads, as name and value. This
    /// is how a stored sign-in reaches a session that starts empty every time.
    pub cookies: Vec<(String, String)>,
    /// Javascript run in every document the page loads, before the page's own. It speaks to the
    /// provider by calling `window.soniclyPost(text)`.
    pub script: String,
}

/// What the page told the provider.
pub enum Heard {
    /// One `window.soniclyPost` call. It is whatever a web page chose to say, so it is read as
    /// data and never trusted.
    Message(String),
    /// The page is gone: it could not open, it never answered, or it sat idle long enough to be
    /// closed. An `Order::Open` brings a new one.
    Closed,
}

pub type Listener = Arc<dyn Fn(Heard) + Send + Sync>;

/// One thing a provider asks of the app's browser page.
pub enum Ask {
    /// Opens the page, hidden, in place of any page open now.
    Open {
        site: Site,
        heard: Listener,
    },
    /// Runs a line of script in the page.
    Run(String),
    /// Silences the page at the engine, or lets it sound again. A page opened by a provider at
    /// zero volume never makes a sound, whatever its own script does with the volume.
    Muted(bool),
    /// Says whether the page has nothing to play. A page left idle for a few minutes is closed,
    /// since it costs a browser's worth of memory, and its listener hears `Closed`.
    Idle(bool),
    Close,
}

/// An [`Ask`] and the remote it came from, so the app can drop what a player that has since been
/// replaced still had on its way.
pub struct Order {
    pub from: u64,
    pub ask: Ask,
}

/// A provider's end of the hand-off. Each player takes its own, and the page belongs to the one
/// that opened it last.
pub struct Remote {
    id: u64,
}

/// Where orders go, once the app has a browser page to carry them out in.
static HOST: Mutex<Option<UnboundedSender<Order>>> = Mutex::new(None);
static REMOTES: AtomicU64 = AtomicU64::new(1);

/// Takes the orders, as the one host. The app calls this once, and only where its web view can
/// run a player page; where it never does, [`served`] stays false and a provider says so
/// instead of loading a track.
pub fn serve() -> UnboundedReceiver<Order> {
    let (sender, receiver) = unbounded_channel();
    *HOST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sender);
    receiver
}

/// Whether the app has a browser page for web players on this system.
pub fn served() -> bool {
    HOST.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .is_some_and(|host| !host.is_closed())
}

impl Remote {
    pub fn new() -> Self {
        Self {
            id: REMOTES.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// Sends one ask to the app. It goes nowhere while nothing serves.
    pub fn ask(&self, ask: Ask) {
        let host = HOST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(host) = host.as_ref() {
            host.send(Order { from: self.id, ask }).ok();
        }
    }
}

impl Default for Remote {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        self.ask(Ask::Close);
    }
}
