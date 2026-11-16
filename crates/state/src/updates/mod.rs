mod build;

use anyhow::{Context as _, Result};
use gpui::{Context, Entity, Task};
use serde::Deserialize;

use crate::{AppSettings, Io, join};

/// The GitHub `owner/name` the releases are read from. `None` until Sonicly has a release
/// repository of its own, and while it is `None` nothing here ever makes a request. Sonicly never
/// downloads or installs an update itself: an offer only links to the release's page.
pub const RELEASE_REPO: Option<&str> = None;
/// What the nightly workflow puts before the version label in the nightly release's title.
const NIGHTLY_TITLE: &str = "Nightly ";
/// How many leading hex digits of a commit hash the version row shows, as `git` abbreviates it.
const SHORT_COMMIT: usize = 7;
const AGENT: &str = concat!("sonicly/", env!("CARGO_PKG_VERSION"));

/// The kind of build this is. Stable follows tagged releases, and Nightly is the build of main
/// the nightly workflow publishes every night.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Nightly,
}

impl Channel {
    /// The channel the running build came from.
    pub fn running() -> Self {
        build::Build::current().channel
    }

    fn url(self, repo: &str) -> String {
        let releases = format!("https://api.github.com/repos/{repo}/releases");
        match self {
            Self::Stable => format!("{releases}/latest"),
            Self::Nightly => format!("{releases}/tags/nightly"),
        }
    }
}

/// The package's displayed version, falling back to the compiled version for portable copies.
pub fn version() -> &'static str {
    &build::Build::current().version
}

/// The shortened hash of the commit a nightly was built from, or `None` for a stable release
/// and for a build that does not know its commit.
pub fn commit() -> Option<&'static str> {
    let build = build::Build::current();
    if build.channel != Channel::Nightly {
        return None;
    }
    let commit = build.commit.as_deref()?;
    Some(commit.get(..SHORT_COMMIT).unwrap_or(commit))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub page: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Quiet,
    Offered(Release),
}

pub struct Updates {
    state: UpdateState,
    settings: Entity<AppSettings>,
    http: reqwest::Client,
    io: Io,
    task: Option<Task<()>>,
}

impl Updates {
    pub fn new(settings: Entity<AppSettings>, io: Io, cx: &mut Context<Self>) -> Self {
        let mut updates = Self {
            state: UpdateState::Quiet,
            settings,
            http: reqwest::Client::new(),
            io,
            task: None,
        };
        updates.look(cx);
        updates
    }

    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    pub fn offered(&self) -> Option<&Release> {
        match &self.state {
            UpdateState::Offered(release) => Some(release),
            _ => None,
        }
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.task = None;
        self.state = UpdateState::Quiet;
        cx.notify();
    }

    /// Asks GitHub for the newest stable release when update checks are on. A nightly build
    /// never looks.
    fn look(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = RELEASE_REPO else {
            return;
        };
        let channel = Channel::running();
        if channel != Channel::Stable || !self.settings.read(cx).check_updates() {
            return;
        }
        let http = self.http.clone();
        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let found = join(io.spawn(async move { latest(&http, repo, channel).await })).await;
            this.update(cx, |this, cx| {
                this.task = None;
                match found {
                    Ok(Some(release)) => {
                        this.state = UpdateState::Offered(release);
                        cx.notify();
                    }
                    Ok(None) => {}
                    Err(error) => log::warn!("updates: cannot ask github: {error:#}"),
                }
            })
            .ok();
        }));
    }
}

#[derive(Deserialize)]
struct Published {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    html_url: String,
}

/// The newest build on `channel`, or `None` when the running build should stay.
async fn latest(http: &reqwest::Client, repo: &str, channel: Channel) -> Result<Option<Release>> {
    let published: Published = http
        .get(channel.url(repo))
        .header("User-Agent", AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("cannot reach github")?
        .error_for_status()
        .context("github refused the release request")?
        .json()
        .await
        .context("cannot read the github release")?;

    let version = match channel {
        Channel::Stable => published.tag_name.trim_start_matches('v').to_owned(),
        Channel::Nightly => published
            .name
            .as_deref()
            .and_then(|name| name.strip_prefix(NIGHTLY_TITLE))
            .context("the nightly release names no version")?
            .to_owned(),
    };
    if !wanted(channel, &version) {
        return Ok(None);
    }
    Ok(Some(Release {
        version,
        page: published.html_url,
    }))
}

/// Whether `offered` on `channel` should replace the running build. Within the running build's
/// own channel only a newer build counts. From the other channel any build does, since choosing
/// that channel asks to move to it.
fn wanted(channel: Channel, offered: &str) -> bool {
    if channel != Channel::running() {
        return true;
    }
    match (ordered(offered), ordered(version())) {
        (Some(offered), Some(running)) => offered > running,
        _ => false,
    }
}

/// Splits a version into its release numbers and, for a nightly label such as
/// `0.44.2-nightly.20261026`, the date it was built. A stable version has 0 for the date.
fn ordered(version: &str) -> Option<(u64, u64, u64, u64)> {
    let (core, rest) = version
        .trim()
        .split_once(['-', '+'])
        .unwrap_or_else(|| (version.trim(), ""));
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let date = rest
        .strip_prefix("nightly.")
        .and_then(|date| date.parse().ok())
        .unwrap_or(0);
    Some((parts.next()??, parts.next()??, parts.next()??, date))
}
