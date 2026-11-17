use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::{App, Context, Entity, Task};
use music::{AlbumDetail, Artist};

use crate::{Io, Session, SessionEvent, join};

/// What a hover card is about: an artist or an album, by its provider id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PeekKey {
    Artist(String),
    Album(String),
}

impl PeekKey {
    fn id(&self) -> &str {
        match self {
            Self::Artist(id) | Self::Album(id) => id,
        }
    }
}

/// The overviews hover cards show, read through the session's catalog so a card and the page
/// it opens share one fetch and one cache. A card asks once it is about to show, and redraws
/// when the overview lands; one the provider refused is not asked for again this session.
pub struct Peek {
    session: Entity<Session>,
    io: Io,
    asking: HashMap<PeekKey, Task<()>>,
    refused: HashSet<PeekKey>,
}

impl Peek {
    pub fn new(session: Entity<Session>, io: Io, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&session, |this, _, event, _| {
            if let SessionEvent::SignedOut | SessionEvent::SignedIn = event {
                this.asking.clear();
                this.refused.clear();
            }
        })
        .detach();
        Self {
            session,
            io,
            asking: HashMap::new(),
            refused: HashSet::new(),
        }
    }

    /// The artist's overview, once it has been fetched by anything.
    pub fn artist(&self, id: &str, cx: &App) -> Option<Arc<Artist>> {
        self.session.read(cx).catalog(id)?.peek_artist(id)
    }

    /// The album and its tracks, once they have been fetched by anything.
    pub fn album(&self, id: &str, cx: &App) -> Option<Arc<AlbumDetail>> {
        self.session.read(cx).catalog(id)?.peek_album(id)
    }

    /// Whether the overview for `key` is being fetched.
    pub fn is_asking(&self, key: &PeekKey) -> bool {
        self.asking.contains_key(key)
    }

    /// Fetches the overview for `key` unless it is cached, on its way, or was refused.
    pub fn ask(&mut self, key: PeekKey, cx: &mut Context<Self>) {
        if self.asking.contains_key(&key) || self.refused.contains(&key) {
            return;
        }
        let Some(catalog) = self.session.read(cx).catalog(key.id()) else {
            return;
        };
        let cached = match &key {
            PeekKey::Artist(id) => catalog.peek_artist(id).is_some(),
            PeekKey::Album(id) => catalog.peek_album(id).is_some(),
        };
        if cached {
            return;
        }
        let request = self.io.spawn({
            let key = key.clone();
            async move {
                match key {
                    PeekKey::Artist(id) => catalog.artist(&id).await.map(drop),
                    PeekKey::Album(id) => catalog.album(&id).await.map(drop),
                }
            }
        });
        let asked = key.clone();
        let task = cx.spawn(async move |this, cx| {
            let fetched = join(request).await;
            this.update(cx, |this, cx| {
                this.asking.remove(&asked);
                if let Err(error) = fetched {
                    log::debug!("peek: cannot read {asked:?}: {error:#}");
                    this.refused.insert(asked);
                }
                cx.notify();
            })
            .ok();
        });
        self.asking.insert(key, task);
    }
}
