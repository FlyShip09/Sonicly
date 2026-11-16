//! Apple Music, played by Apple's own web player.
//!
//! The account arrives as one cookie, `media-user-token`, from the app's own sign-in window, where
//! the listener signs in on Apple's page. It is kept sealed like every other sign-in.
//!
//! The library, search, albums, artists, playlists, stations and what Apple recommends all come
//! from the same endpoints the web player calls, described in [`client`].
//!
//! Playback is not Sonicly's. Apple's streams are protected, and Sonicly holds no content
//! decryption module, asks for no licence and decrypts nothing. A track plays inside
//! music.apple.com in a hidden page of the system web view, whose own licensed module does all of
//! that, and Sonicly only tells the page what to play: see [`player`]. So an Apple Music track
//! never passes through the equalizer, the visualizer or the chosen output device, and it cannot
//! be downloaded or cached.
//!
//! One thing is missing rather than unfinished: lyrics, which the token read off the page has no
//! permission for.

mod auth;
mod client;
mod links;
mod player;
mod recommend;
mod wire;

use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;

use crate::{
    Arrival, Capabilities, Catalog, InputSource, MusicApi as _, MusicProvider, PromptSink,
    ProviderSession, Shape, SignIn, SignInFailure, SignInProblem, SignInPrompt, UserProfile,
    WebSignIn,
};

pub use client::AppleClient;

/// The page the sign-in window opens. Apple finishes on music.apple.com with the account
/// cookie set, which is the one thing this needs.
const SIGN_IN_URL: &str = "https://music.apple.com/login";
/// Where the window ends up, and what cookie reads are scoped to.
const LANDING: &str = "music.apple.com";
/// The cookie filter runs on bare domains, so no leading dot. The account cookie is set on
/// `music.apple.com`, which this covers.
const COOKIE_DOMAIN: &str = "apple.com";

#[derive(Default)]
pub struct AppleProvider {
    visitor: bool,
}

impl AppleProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Offers a session with no account, which browses the catalog and plays the 30-second
    /// previews Apple's web player gives any visitor. It exists so the web player can be checked
    /// without an account, and only a debug launch turns it on.
    pub fn visitor(mut self, visitor: bool) -> Self {
        self.visitor = visitor;
        self
    }

    async fn connect(secret: &str) -> Result<ProviderSession> {
        let token = auth::user_token(secret)?;
        let client = AppleClient::connect(&token)
            .await
            .context("cannot reach apple music with that account")?;
        entitled(&client).await?;
        let profile = client.profile().await?;
        auth::store(&token)?;
        Ok(session(client, profile))
    }

    async fn restore_stored() -> Result<Option<ProviderSession>> {
        let Some(token) = auth::load() else {
            return Ok(None);
        };
        match AppleClient::connect(&token).await {
            Ok(client) => {
                entitled(&client).await?;
                let profile = client.profile().await?;
                Ok(Some(session(client, profile)))
            }
            Err(error) if crate::trouble::offline(&format!("{error:#}")) => Err(error),
            Err(error) => {
                log::warn!("apple: the stored account is no longer usable: {error:#}");
                Ok(None)
            }
        }
    }
}

/// Refuses an account Apple says has no active subscription, since every library read and
/// every play would fail for it. A check that cannot be made lets the account through.
async fn entitled(client: &AppleClient) -> Result<()> {
    match client.subscribed().await {
        Ok(Some(false)) => {
            log::warn!(
                "apple: the account has no active subscription in storefront {}",
                client.storefront()
            );
            Err(anyhow::Error::new(SignInFailure(
                SignInProblem::Subscription,
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if crate::trouble::offline(&format!("{error:#}")) => Err(error),
        Err(error) => {
            log::debug!("apple: cannot read the subscription status: {error:#}");
            Ok(())
        }
    }
}

fn session(client: AppleClient, profile: UserProfile) -> ProviderSession {
    let account = client.account();
    let playback = player::Factory {
        account: account.clone(),
        storefront: client.storefront().into(),
    };
    ProviderSession {
        profile,
        api: Arc::new(client),
        playback: Arc::new(playback),
        // The library is what the listener added, and favorites are a separate set on top of
        // it. Songs and albums go into it apart from the star, and a favorited song still
        // lands in it through the account's Add Favorite Songs to Library setting, which is on
        // by default.
        shape: Shape::Catalog(Catalog::Editable),
        authenticated: account.is_some(),
        // Apple keeps no play counts. Stations it does have, through the same endpoint the web
        // player's autoplay uses. An artist can be favorited like anything else, and pins live
        // behind `me/library/pins`. Nothing is downloaded or kept: the audio never reaches
        // Sonicly, so there is nothing of it to keep. A library song or one in your own playlist
        // carries no date, but both are listed in the order they were added, oldest first.
        capabilities: match account.is_some() {
            true => Capabilities {
                radio: true,
                follow_artists: true,
                pins: true,
                song_arrival: Some(Arrival::OldestFirst),
                playlist_arrival: Some(Arrival::OldestFirst),
                ..Capabilities::NONE
            },
            false => Capabilities::NONE,
        },
    }
}

#[async_trait]
impl MusicProvider for AppleProvider {
    fn name(&self) -> &'static str {
        "Apple Music"
    }

    fn slug(&self) -> &'static str {
        "apple"
    }

    fn link(&self, link: &str) -> Option<crate::link::Linked> {
        links::read(link)
    }

    fn reach(&self) -> Option<String> {
        Some("music.apple.com".to_owned())
    }

    fn sign_in_options(&self) -> Vec<SignIn> {
        match self.visitor {
            true => vec![SignIn::Secret, SignIn::Anonymous],
            false => vec![SignIn::Secret],
        }
    }

    fn stored(&self) -> bool {
        auth::stored()
    }

    fn public_art(&self) -> bool {
        // artwork lives on mzstatic, open to anyone with the url
        true
    }

    async fn restore(&self) -> Result<Option<ProviderSession>> {
        Self::restore_stored().await
    }

    async fn sign_in(
        &self,
        method: SignIn,
        prompt: PromptSink,
        mut input: InputSource,
    ) -> Result<ProviderSession> {
        match method {
            SignIn::Secret | SignIn::Default => {
                prompt(SignInPrompt::Secret);
                let cookies = input.recv().await.context("sign-in was cancelled")?;
                Self::connect(&cookies).await
            }
            SignIn::Anonymous if self.visitor => {
                let client = AppleClient::visit().await?;
                let profile = client.profile().await?;
                Ok(session(client, profile))
            }
            SignIn::Anonymous => anyhow::bail!("apple music has no anonymous sign-in"),
            SignIn::Path(_) => anyhow::bail!("apple music does not sign in with a folder path"),
            SignIn::Credentials { .. } => {
                anyhow::bail!("apple music does not sign in with a server address")
            }
        }
    }

    fn sign_out(&self) {
        auth::forget();
    }

    fn web_sign_in(&self) -> Option<WebSignIn> {
        Some(WebSignIn {
            url: SIGN_IN_URL,
            landing: LANDING,
            domain: COOKIE_DOMAIN,
            proof: auth::PROOF,
            // Apple's sign-in page is happy with whatever the backend's own engine says.
            agent: None,
        })
    }
}
