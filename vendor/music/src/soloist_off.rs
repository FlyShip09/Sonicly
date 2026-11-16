//! Sonicly: what stands in for `soloist` while the `soloist` feature is off, which it always is
//! in Sonicly's builds.
//!
//! Upstream's module downloads a program from a CDN and runs it with no signature or hash to
//! check it against, and replaces it unasked as it ages. None of that is compiled here. This
//! keeps the names `state`, `views` and the Spotify provider use, and every one of them answers
//! that Soloist is unsupported, not installed and not wanted. It reads and writes no file,
//! starts no program and opens no connection. With [`supported`] false the settings row and the
//! setup dialog never show, and Spotify plays through librespot.

use anyhow::{Result, bail};
use tokio::sync::oneshot;

use crate::Offer;

pub const DASHBOARD: &str = "https://developer.spotify.com/dashboard/soloist";
pub const TERMS: &str = "https://www.spotify.com/legal/end-user-agreement/";
pub const DEVICE: &str = "Sonicly Soloist";

/// No level is served, since no engine is ever started.
pub(crate) const QUALITIES: &[Offer] = &[];

/// The build of the installed program. Never constructed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub version: String,
}

impl Build {
    pub fn stale(&self) -> bool {
        false
    }
}

/// Why pairing did not finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trouble {
    Key,
    Account(String),
    Waited,
    Failed(String),
}

pub fn supported() -> bool {
    false
}

pub fn set_up() -> bool {
    false
}

pub fn ready() -> bool {
    false
}

pub fn wanted() -> bool {
    false
}

pub fn installed() -> Option<Build> {
    None
}

pub fn enable(_on: bool) -> Result<()> {
    bail!("this build carries no soloist")
}

pub async fn fetch() -> Result<Build> {
    bail!("this build carries no soloist")
}

pub async fn pair(
    _key: String,
    _account: Option<String>,
    _listening: oneshot::Sender<u16>,
) -> Result<(), Trouble> {
    Err(Trouble::Failed("this build carries no soloist".into()))
}

/// Nothing ever runs, so nothing has to stop.
pub async fn stopped() {}

pub fn reload() {}

/// Nothing is ever stored, so there is nothing to forget.
pub fn forget() -> Result<()> {
    Ok(())
}

pub fn remove() -> Result<()> {
    Ok(())
}

pub(crate) fn denied() {}

pub(crate) fn granted() {}
