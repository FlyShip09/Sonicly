use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result};
use gpui::{App, Context, Entity, Subscription, Task};
use serde::{Deserialize, Serialize};
use storage::Cache;

use crate::{AppSettings, Io, Network, Sonicly, join};

/// Sonicly's repository page, which the star button and the source row open.
pub const REPO_URL: &str = "https://github.com/FlyShip09/Sonicly";
/// The one address the count is read from, without a sign-in. Nothing else is ever asked.
const API: &str = "https://api.github.com/repos/FlyShip09/Sonicly";
const AGENT: &str = concat!("sonicly/", env!("CARGO_PKG_VERSION"));
/// Where the last answer and the time it was asked for are kept in `cache.sqlite`.
const KEY: &str = "github/stars";
/// How many seconds pass before GitHub is asked again. A failed ask counts, so a network that
/// keeps failing is tried once a day and never in a loop.
const EVERY: i64 = 24 * 60 * 60;
/// How long the one request may take.
const PATIENCE: Duration = Duration::from_secs(10);

/// What `cache.sqlite` holds: the last count GitHub gave, and when it was last asked.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Kept {
    count: Option<u64>,
    asked: i64,
}

#[derive(Deserialize)]
struct Repository {
    stargazers_count: u64,
}

/// How many stars Sonicly's repository has, for the star button. It asks GitHub at most once
/// a day, only while the setting is on and the app is online, and shows the cached count in
/// between. An unknown count is `None`, and the button then stands without a number.
pub struct Stars {
    count: Option<u64>,
    /// Whether the setting allows asking, as it stood when last looked at.
    on: bool,
    io: Io,
    cache: Cache,
    task: Option<Task<()>>,
    _settings: Subscription,
}

impl Stars {
    pub fn new(
        settings: Entity<AppSettings>,
        io: Io,
        cache: Cache,
        cx: &mut Context<Self>,
    ) -> Self {
        let watch = cx.observe(&settings, |this: &mut Self, settings, cx| {
            let on = settings.read(cx).github_stars();
            if on == this.on {
                return;
            }
            this.on = on;
            this.count = None;
            this.task = None;
            cx.notify();
            this.look(cx);
        });
        let mut stars = Self {
            count: None,
            on: settings.read(cx).github_stars(),
            io,
            cache,
            task: None,
            _settings: watch,
        };
        stars.look(cx);
        stars
    }

    pub fn global(cx: &App) -> Entity<Self> {
        Sonicly::global(cx).stars.clone()
    }

    /// The count as the button shows it, such as `1.2k`, or `None` while it is unknown or the
    /// setting is off.
    pub fn label(&self) -> Option<String> {
        self.count.map(compact)
    }

    /// Shows the cached count, then asks GitHub when the last ask is a day old. Does nothing
    /// with the setting off.
    fn look(&mut self, cx: &mut Context<Self>) {
        if !self.on {
            return;
        }
        let io = self.io.clone();
        let cache = self.cache.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let read = cache.clone();
            let kept = join(io.spawn_blocking(move || {
                read.read(KEY, |text| serde_json::from_str::<Kept>(text).ok())
            }))
            .await
            .unwrap_or_else(|error| {
                log::warn!("stars: cannot read the cached count: {error:#}");
                None
            })
            .flatten()
            .unwrap_or_default();
            let shown = this.update(cx, |this, cx| {
                this.count = kept.count;
                cx.notify();
                Network::lost(cx)
            });
            // offline mode or a lost connection asks nothing, and tries on a later launch
            let Ok(false) = shown else { return };
            let now = now();
            if !due(kept.asked, now) {
                return;
            }
            let count = match join(io.spawn(ask())).await {
                Ok(count) => Some(count),
                Err(error) => {
                    log::warn!("stars: cannot ask github: {error:#}");
                    kept.count
                }
            };
            let kept = Kept { count, asked: now };
            let saved = join(io.spawn_blocking(move || {
                let text = serde_json::to_string(&kept).context("cannot encode the star count")?;
                cache.write(KEY, &text, now)
            }))
            .await;
            if let Err(error) = saved {
                log::warn!("stars: cannot cache the count: {error:#}");
            }
            this.update(cx, |this, cx| {
                this.count = count;
                cx.notify();
            })
            .ok();
        }));
    }
}

/// The repository's star count. One unauthenticated GET that carries the app's name and version
/// as its user agent and nothing else about the listener or the computer.
async fn ask() -> Result<u64> {
    let http = reqwest::Client::builder()
        .user_agent(AGENT)
        .timeout(PATIENCE)
        .build()
        .context("cannot set up the request")?;
    let repository: Repository = http
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("cannot reach github")?
        .error_for_status()
        .context("github refused the request")?
        .json()
        .await
        .context("cannot read the github answer")?;
    Ok(repository.stargazers_count)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() as i64)
}

/// Whether GitHub may be asked at `now`, given when it was last asked. A clock set back since
/// then waits for the clock to catch up rather than asking early.
fn due(asked: i64, now: i64) -> bool {
    now.saturating_sub(asked) >= EVERY
}

/// A count short enough for a button: `987`, `1.2k`, `15k`, `1.3M`.
fn compact(count: u64) -> String {
    let (unit, suffix) = match count {
        0..1_000 => return count.to_string(),
        1_000..1_000_000 => (1_000, "k"),
        _ => (1_000_000, "M"),
    };
    // floored, so 999 999 reads 999k and never 1000k
    let (whole, tenth) = (count / unit, count % unit * 10 / unit);
    match whole < 10 && tenth != 0 {
        true => format!("{whole}.{tenth}{suffix}"),
        false => format!("{whole}{suffix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_are_shortened() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(987), "987");
        assert_eq!(compact(1_000), "1k");
        assert_eq!(compact(1_249), "1.2k");
        assert_eq!(compact(15_800), "15k");
        assert_eq!(compact(999_999), "999k");
        assert_eq!(compact(1_300_000), "1.3M");
    }

    #[test]
    fn github_is_asked_once_a_day() {
        assert!(due(0, EVERY));
        assert!(!due(1_000, 1_000 + EVERY - 1));
        assert!(due(1_000, 1_000 + EVERY));
        // a clock set back does not ask early
        assert!(!due(5_000, 1_000));
    }

    #[test]
    fn the_cached_answer_reads_back() {
        let kept = Kept {
            count: Some(42),
            asked: 7,
        };
        let read: Kept = serde_json::from_str(&serde_json::to_string(&kept).unwrap()).unwrap();
        assert_eq!((read.count, read.asked), (Some(42), 7));
        let unknown: Kept = serde_json::from_str(r#"{"count":null,"asked":7}"#).unwrap();
        assert_eq!(unknown.count, None);
    }
}
