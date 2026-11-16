//! The Widevine module, as the rest of the app has to talk about it.
//!
//! Sonicly: upstream re-exports its `widevine` crate here. Sonicly does not import that crate, so
//! this is a stub with the same names. It reports that no module is supported or available, and
//! it never searches the machine, downloads, loads or decrypts anything. With [`supported`] false
//! `state::Drm` never leaves `Missing`, and the settings row and the prompt stay hidden.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// The environment variable upstream reads a module path from. Nothing reads it here.
pub const CDM_PATH: &str = "SONICLY_WIDEVINE_CDM";

/// How a CDM was come by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Configured,
    Installed,
    Fetched,
}

/// A CDM on disk and where it came from. Never constructed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub origin: Origin,
}

/// A downloaded module waiting for the user's yes. Uninhabited: [`offer`] always fails.
pub enum Offer {}

impl Offer {
    pub fn version(&self) -> &str {
        match *self {}
    }

    pub fn license(&self) -> &str {
        match *self {}
    }

    pub fn install(self) -> Result<Found> {
        match self {}
    }
}

/// Whether this build has a host for a CDM. Never.
pub fn supported() -> bool {
    false
}

/// Whether this build and this machine can open a CDM. Never.
pub fn available() -> bool {
    false
}

/// The CDM this process would use. Always none, and nothing is searched.
pub fn find() -> Option<Found> {
    None
}

/// The module this process was started to host. Always none.
pub fn hosted() -> Option<PathBuf> {
    None
}

pub fn host(module: &Path) -> Result<()> {
    bail!(
        "this build carries no widevine host, so {} cannot be hosted",
        module.display()
    )
}

pub async fn offer() -> Result<Offer> {
    bail!("this build carries no widevine support")
}

pub async fn fetch() -> Result<Found> {
    bail!("this build carries no widevine support")
}

/// Nothing is ever installed, so there is nothing to remove.
pub fn uninstall() -> Result<()> {
    Ok(())
}
