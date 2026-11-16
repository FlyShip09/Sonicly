//! Apple Music's playback: Apple's own web player, in a hidden page of the system web view.
//!
//! Apple's streams are protected, and Sonicly does not unprotect anything. What it does instead
//! is what a listener does in a browser: open music.apple.com, where Apple's MusicKit is already
//! set up, and press play. The page fetches the stream, the web view's own licensed module
//! decrypts it, and the web view plays it to the system's output. This file only tells the page
//! what to queue ([`PLAYER_SCRIPT`]) and turns what the page reports into [`PlaybackEvent`]s. No
//! audio, key or licence ever crosses into Sonicly, which is also why nothing of Sonicly's own
//! sound path reaches these tracks: no equalizer, no visualizer, no loudness normalisation, no
//! gapless join, no chosen output device, and no download.
//!
//! The page is opened by the first track that is asked to play and not before, so a restored
//! track waits paused without one. See [`crate::webplayer`] for the hand-off to the app.

use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::webplayer::{self, Ask, Heard, Listener, Remote, Site};
use crate::{
    Barrier, Encoding, Normalisation, PlaybackConfig, PlaybackEvent, PlaybackEvents,
    PlaybackFactory, Player, TrackSignal,
};

/// The script that drives the page, run in every document it loads.
const PLAYER_SCRIPT: &str = include_str!("player.js");
const HOST: &str = "music.apple.com";
/// The cookie Apple's page reads the account from.
const ACCOUNT_COOKIE: &str = "media-user-token";
/// The longest message the page is listened to. A report is a couple of hundred bytes.
const LONGEST: usize = 4096;
/// How close to its length a track that pauses by itself is taken to be ending.
const TAIL: Duration = Duration::from_millis(1500);
/// How far the page's volume may sit from the one asked for before it is set again. Apple's
/// page restores a volume of its own as it starts.
const VOLUME_SLACK: f64 = 0.02;
/// What the web player streams when it does not say: AAC at 44.1 kHz in stereo.
const RATE: u32 = 44_100;
const CHANNELS: u16 = 2;

/// Starts players for one account.
pub(super) struct Factory {
    /// The account's `media-user-token`, or none for a visitor, who hears Apple's previews.
    pub account: Option<Arc<str>>,
    pub storefront: Arc<str>,
}

impl PlaybackFactory for Factory {
    fn start(&self, config: PlaybackConfig) -> (Box<dyn Player>, Box<dyn PlaybackEvents>) {
        let (events, receiver) = unbounded_channel();
        let remote = Remote::new();
        let site = Site {
            url: format!(
                "https://{HOST}/{}/search",
                crate::escape::component(&self.storefront)
            ),
            host: HOST.to_owned(),
            domain: HOST.to_owned(),
            cookies: self
                .account
                .iter()
                .map(|token| (ACCOUNT_COOKIE.to_owned(), token.to_string()))
                .collect(),
            script: PLAYER_SCRIPT.to_owned(),
        };
        let machine = Arc::new_cyclic(|me: &Weak<Mutex<Machine>>| {
            let me = me.clone();
            let open = move || {
                let me = me.clone();
                let heard: Listener = Arc::new(move |heard| {
                    if let Some(machine) = me.upgrade() {
                        lock(&machine).hear(heard);
                    }
                });
                Ask::Open {
                    site: site.clone(),
                    heard,
                }
            };
            Mutex::new(Machine {
                ask: Box::new(move |ask| remote.ask(ask)),
                open: Box::new(open),
                events,
                visitor: self.account.is_none(),
                served: webplayer::served(),
                gain: config.gain,
                stage: Stage::Closed,
                held: None,
                serial: 0,
            })
        });
        (Box::new(WebPlayer(machine)), Box::new(Events(receiver)))
    }

    /// Where the app has no page to run Apple's player in, as on Linux, whose WebKitGTK ships
    /// no module for protected audio, nothing of Apple Music plays.
    fn barrier(&self) -> Option<Barrier> {
        (!webplayer::served()).then_some(Barrier::Player)
    }
}

struct WebPlayer(Arc<Mutex<Machine>>);

impl Player for WebPlayer {
    fn load(&self, track_id: &str, at: Duration, _seamless: bool) -> Result<()> {
        lock(&self.0).load(track_id, at);
        Ok(())
    }

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        lock(&self.0).hold(track_id, at);
        Ok(())
    }

    /// The page fetches a track when it is asked to play it and not before.
    fn preload(&self, _track_id: &str, _segue: bool) -> Result<()> {
        Ok(())
    }

    fn play(&self) {
        lock(&self.0).play();
    }

    fn pause(&self) {
        lock(&self.0).pause();
    }

    fn seek(&self, position: Duration) {
        lock(&self.0).seek(position);
    }

    fn set_gain(&self, gain: f32) {
        lock(&self.0).set_gain(gain);
    }
}

struct Events(UnboundedReceiver<PlaybackEvent>);

#[async_trait]
impl PlaybackEvents for Events {
    async fn next(&mut self) -> Option<PlaybackEvent> {
        self.0.recv().await
    }
}

/// Where the page is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Closed,
    /// Asked for, and yet to say its MusicKit is up.
    Opening,
    Ready,
}

/// How far the page has got with the load it was sent.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// The page has not been sent this track, or has not started it yet.
    Waiting,
    Playing,
    /// Started, and waiting on the network or a seek.
    Buffering,
    Paused,
    /// Played to its last second. The page pauses and rewinds before it says the track ended,
    /// and none of that is the listener's pause.
    Ending,
}

/// The track this player holds.
struct Held {
    id: String,
    at: Duration,
    /// Whether the listener wants it playing.
    playing: bool,
    /// Whether the page has been sent it under the current serial.
    sent: bool,
    phase: Phase,
    length: Duration,
    signalled: bool,
}

type Asker = Box<dyn Fn(Ask) + Send>;
type Opener = Box<dyn Fn() -> Ask + Send>;

/// What stands between the transport calls and the page: the one track held, where the page is
/// with it, and the translation of its reports into events.
struct Machine {
    ask: Asker,
    /// Builds the ask that opens the page with this machine as its listener.
    open: Opener,
    events: UnboundedSender<PlaybackEvent>,
    /// Whether nobody is signed in, so the 30-second previews Apple gives a visitor are what
    /// plays. For an account a preview means Apple will not play the track in full.
    visitor: bool,
    /// Whether the app has a browser page to play in on this system.
    served: bool,
    gain: f32,
    stage: Stage,
    held: Option<Held>,
    /// Counts loads. The page stamps what it reports with the load it belongs to.
    serial: u64,
}

/// One message from the page.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Message {
    Ready,
    State(Report),
    Seeked {
        serial: u64,
        #[serde(default)]
        at: f64,
    },
    Error {
        serial: u64,
        #[serde(default)]
        code: String,
        #[serde(default)]
        detail: String,
    },
}

#[derive(Deserialize)]
struct Report {
    serial: u64,
    state: String,
    #[serde(default)]
    at: f64,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    preview: bool,
    #[serde(default)]
    authorized: bool,
    #[serde(default)]
    volume: Option<f64>,
    #[serde(default)]
    kbps: u32,
}

impl Machine {
    fn emit(&self, event: PlaybackEvent) {
        self.events.send(event).ok();
    }

    fn run(&self, script: String) {
        (self.ask)(Ask::Run(format!(
            "window.soniclyPlayer&&window.soniclyPlayer.{script}"
        )));
    }

    fn load(&mut self, id: &str, at: Duration) {
        self.held = Some(Held::new(id, at, true));
        self.begin();
    }

    /// Keeps a track paused at `at` without a page. The first `play` opens one.
    fn hold(&mut self, id: &str, at: Duration) {
        self.stop_page();
        self.held = Some(Held::new(id, at, false));
    }

    /// Sends the held track to the page as a new load, opening the page first when there is
    /// none.
    fn begin(&mut self) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        self.serial += 1;
        held.sent = false;
        held.phase = Phase::Waiting;
        let (id, at) = (held.id.clone(), held.at);
        if !self.served {
            self.held = None;
            return self.emit(PlaybackEvent::Barred(Barrier::Player));
        }
        self.emit(PlaybackEvent::Loading { id: Some(id), at });
        (self.ask)(Ask::Idle(false));
        match self.stage {
            Stage::Closed => {
                self.stage = Stage::Opening;
                // muted before it exists, so nothing the page does on its way up is heard
                (self.ask)((self.open)());
                (self.ask)(Ask::Muted(self.gain <= 0.));
            }
            Stage::Opening => {}
            Stage::Ready => self.send(),
        }
    }

    /// Hands the page the held track, with the volume first so not a sample is louder than asked.
    fn send(&mut self) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        held.sent = true;
        let quoted = serde_json::to_string(&held.id).unwrap_or_default();
        let script = format!("load({quoted},{},{})", held.at.as_secs_f64(), self.serial);
        self.send_volume();
        self.run(script);
    }

    fn send_volume(&self) {
        (self.ask)(Ask::Muted(self.gain <= 0.));
        self.run(format!("volume({})", self.gain.clamp(0., 1.)));
    }

    fn play(&mut self) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        held.playing = true;
        match held.sent && self.stage == Stage::Ready {
            true => {
                (self.ask)(Ask::Idle(false));
                self.run("play()".to_owned());
            }
            false => self.begin(),
        }
    }

    fn pause(&mut self) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        held.playing = false;
        let (id, at) = (held.id.clone(), held.at);
        match held.phase {
            // the page says so itself once it has stopped, with where it stopped
            Phase::Playing | Phase::Buffering => self.run("pause()".to_owned()),
            Phase::Paused | Phase::Ending => {}
            Phase::Waiting => {
                // nothing sounds yet, and the page is told again if it starts after all
                self.run("pause()".to_owned());
                self.emit(PlaybackEvent::Paused { id: Some(id), at });
                (self.ask)(Ask::Idle(true));
            }
        }
    }

    fn seek(&mut self, position: Duration) {
        let Some(held) = self.held.as_mut() else {
            return;
        };
        held.at = position;
        match held.sent && held.phase != Phase::Waiting {
            true => self.run(format!("seek({})", position.as_secs_f64())),
            false => {
                // not started in the page: the load, now or at the next play, starts there
                let id = Some(held.id.clone());
                let reload = held.sent && held.playing;
                held.sent = false;
                self.emit(PlaybackEvent::Seeked { id, at: position });
                if reload {
                    self.begin();
                }
            }
        }
    }

    fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
        if self.stage != Stage::Closed {
            self.send_volume();
        }
    }

    /// Stops whatever the page plays and makes it deaf to the load it was on.
    fn stop_page(&mut self) {
        self.serial += 1;
        if self.stage == Stage::Ready {
            self.run("stop()".to_owned());
        }
        if self.stage != Stage::Closed {
            (self.ask)(Ask::Idle(true));
        }
    }

    /// Drops the held track after the page could not play it, and says why.
    fn give_up(&mut self, event: PlaybackEvent) {
        self.held = None;
        self.stop_page();
        self.emit(event);
    }

    fn hear(&mut self, heard: Heard) {
        match heard {
            Heard::Closed => self.closed(),
            Heard::Message(text) if text.len() > LONGEST => {
                log::debug!("apple: dropped an oversized message from the player page");
            }
            Heard::Message(text) => {
                log::trace!("apple: the player page said {text}");
                self.read(&text);
            }
        }
    }

    fn read(&mut self, text: &str) {
        match serde_json::from_str::<Message>(text) {
            Ok(Message::Ready) => self.ready(),
            Ok(Message::State(report)) => self.report(report),
            Ok(Message::Seeked { serial, at }) => self.seeked(serial, at),
            Ok(Message::Error {
                serial,
                code,
                detail,
            }) => self.errored(serial, &code, &detail),
            Err(error) => log::debug!("apple: cannot read a player page message: {error}"),
        }
    }

    fn ready(&mut self) {
        log::debug!("apple: the web player is up");
        self.stage = Stage::Ready;
        self.send_volume();
        match self.held.as_ref().is_some_and(|held| held.playing) {
            true => self.send(),
            false => (self.ask)(Ask::Idle(true)),
        }
    }

    /// The page is gone. A track that was meant to be playing has failed; a paused one keeps
    /// its place and the next play opens a new page for it.
    fn closed(&mut self) {
        log::debug!("apple: the web player page closed");
        self.stage = Stage::Closed;
        let Some(held) = self.held.as_mut() else {
            return;
        };
        held.sent = false;
        held.phase = Phase::Waiting;
        if held.playing {
            let id = Some(held.id.clone());
            self.held = None;
            self.emit(PlaybackEvent::Unavailable { id });
        }
    }

    fn seeked(&mut self, serial: u64, at: f64) {
        if serial != self.serial {
            return;
        }
        let Some(held) = self.held.as_mut() else {
            return;
        };
        held.at = seconds(at);
        let event = PlaybackEvent::Seeked {
            id: Some(held.id.clone()),
            at: held.at,
        };
        self.emit(event);
    }

    fn errored(&mut self, serial: u64, code: &str, detail: &str) {
        let Some(held) = self.held.as_ref().filter(|_| serial == self.serial) else {
            return;
        };
        let id = Some(held.id.clone());
        log::warn!("apple: the web player refused {}: {code}", held.id);
        log::debug!("apple: the web player said: {}", crate::redact(detail));
        match Trouble::of(code) {
            Trouble::Passing => {}
            Trouble::SignIn => self.give_up(PlaybackEvent::Gated),
            Trouble::Subscription => self.give_up(PlaybackEvent::Barred(Barrier::Subscription)),
            Trouble::Engine => self.give_up(PlaybackEvent::Barred(Barrier::Player)),
            Trouble::Track => self.give_up(PlaybackEvent::Unavailable { id }),
        }
    }

    fn report(&mut self, report: Report) {
        if report.serial != self.serial {
            return;
        }
        if report.volume.is_some_and(|volume| {
            (volume - f64::from(self.gain.clamp(0., 1.))).abs() > VOLUME_SLACK
        }) && report.state != "loading"
        {
            // the load itself holds the volume at zero until it stands where it should
            self.run(format!("volume({})", self.gain.clamp(0., 1.)));
        }
        let visitor = self.visitor;
        let Some(held) = self.held.as_mut() else {
            return;
        };
        let id = Some(held.id.clone());
        if report.preview && !visitor && report.state == "playing" {
            // Apple plays an account a preview when it will not play it the whole track
            return self.give_up(match report.authorized {
                true => PlaybackEvent::Barred(Barrier::Subscription),
                false => PlaybackEvent::Gated,
            });
        }
        let at = seconds(report.at);
        let length = seconds(report.duration);
        let mut events = Vec::new();
        if !length.is_zero() && length != held.length {
            held.length = length;
            events.push(PlaybackEvent::Length {
                id: id.clone(),
                duration: length,
            });
        }
        let started = held.phase != Phase::Waiting;
        let mut idle = None;
        match report.state.as_str() {
            "playing" => {
                if !held.signalled {
                    held.signalled = true;
                    events.push(PlaybackEvent::Signal {
                        id: held.id.clone(),
                        track: signal(report.kbps),
                    });
                }
                match held.phase {
                    // the page repeats itself, and a figure it already gave is by now stale
                    Phase::Playing if at == held.at => {}
                    Phase::Playing => events.push(PlaybackEvent::Position { id, at }),
                    _ => events.push(PlaybackEvent::Playing { id, at }),
                }
                held.at = at;
                held.phase = Phase::Playing;
                idle = Some(false);
                if !held.playing {
                    // it started after the listener paused
                    self.run("pause()".to_owned());
                }
            }
            "paused"
                if held.phase == Phase::Playing
                    && held.playing
                    && !held.length.is_zero()
                    && at + TAIL >= held.length =>
            {
                held.phase = Phase::Ending;
            }
            "paused" if started && !matches!(held.phase, Phase::Paused | Phase::Ending) => {
                held.at = at;
                held.phase = Phase::Paused;
                held.playing = false;
                events.push(PlaybackEvent::Paused { id, at });
                idle = Some(true);
            }
            "ended" | "completed" | "stopped" if held.phase == Phase::Ending => {
                self.held = None;
                events.push(PlaybackEvent::Ended { id });
                idle = Some(true);
            }
            "ended" | "completed" if started => {
                self.held = None;
                events.push(PlaybackEvent::Ended { id });
                idle = Some(true);
            }
            "loading" | "waiting" | "stalled" | "seeking" if held.phase == Phase::Playing => {
                held.phase = Phase::Buffering;
                events.push(PlaybackEvent::Loading { id, at });
            }
            _ => {}
        }
        for event in events {
            self.emit(event);
        }
        if let Some(idle) = idle {
            (self.ask)(Ask::Idle(idle));
        }
    }
}

impl Held {
    fn new(id: &str, at: Duration, playing: bool) -> Self {
        Self {
            id: id.to_owned(),
            at,
            playing,
            sent: false,
            phase: Phase::Waiting,
            length: Duration::ZERO,
            signalled: false,
        }
    }
}

/// What an error code of Apple's MusicKit means for the track.
#[derive(Debug, PartialEq, Eq)]
enum Trouble {
    /// Nothing that stops the music, such as a play report that did not go through.
    Passing,
    /// The page is not signed in, or Apple no longer takes the sign-in.
    SignIn,
    Subscription,
    /// The web view could not play protected audio.
    Engine,
    /// This track will not play: gone, restricted, out of region, or the network failed it.
    Track,
}

impl Trouble {
    fn of(code: &str) -> Self {
        match code {
            "PLAY_ACTIVITY" | "QUOTA_EXCEEDED" | "USER_INTERACTION_REQUIRED" => Self::Passing,
            "AUTHORIZATION_ERROR" | "ACCESS_DENIED" | "UNAUTHORIZED_ERROR" | "TOKEN_EXPIRED" => {
                Self::SignIn
            }
            "SUBSCRIPTION_ERROR" => Self::Subscription,
            "MEDIA_KEY"
            | "MEDIA_LICENSE"
            | "MEDIA_CERTIFICATE"
            | "MEDIA_SESSION"
            | "WIDEVINE_CDM_EXPIRED"
            | "UNSUPPORTED_ERROR"
            | "BROWSER_NOT_SUPPORTED"
            | "DEVICE_LIMIT" => Self::Engine,
            _ => Self::Track,
        }
    }
}

/// What can be said of the sound of a track the web player plays: its encoding as the player
/// names it, and that it goes to the system past Sonicly.
fn signal(kbps: u32) -> TrackSignal {
    TrackSignal {
        encoding: (kbps > 0).then_some(Encoding::Aac { kbps }),
        rate: RATE,
        channels: CHANNELS,
        normalisation: Normalisation::Off,
        direct: true,
    }
}

/// A time the page reported, in seconds, as a duration. Anything that is not a sane time is
/// zero, since the page is not trusted to send one.
fn seconds(value: f64) -> Duration {
    match value.is_finite() && value > 0. {
        true => Duration::from_secs_f64(value.min(86_400.)),
        false => Duration::ZERO,
    }
}

fn lock(machine: &Mutex<Machine>) -> std::sync::MutexGuard<'_, Machine> {
    machine
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine with nothing behind it, the scripts it runs and the events it sends.
    fn machine(
        visitor: bool,
    ) -> (
        Machine,
        std::sync::mpsc::Receiver<String>,
        UnboundedReceiver<PlaybackEvent>,
    ) {
        let (events, receiver) = unbounded_channel();
        let (scripts, ran) = std::sync::mpsc::channel();
        let machine = Machine {
            ask: Box::new(move |ask| {
                if let Ask::Run(script) = ask {
                    scripts.send(script).ok();
                }
            }),
            open: Box::new(|| Ask::Close),
            events,
            visitor,
            served: true,
            gain: 0.5,
            stage: Stage::Closed,
            held: None,
            serial: 0,
        };
        (machine, ran, receiver)
    }

    fn say(machine: &mut Machine, text: &str) {
        machine.hear(Heard::Message(text.to_owned()));
    }

    fn state(serial: u64, state: &str, at: f64) -> String {
        format!(
            r#"{{"t":"state","serial":{serial},"state":"{state}","at":{at},"duration":200,"preview":false,"authorized":true,"volume":0.5,"kbps":256}}"#
        )
    }

    fn drain(events: &mut UnboundedReceiver<PlaybackEvent>) -> Vec<PlaybackEvent> {
        std::iter::from_fn(|| events.try_recv().ok()).collect()
    }

    fn id() -> Option<String> {
        Some("123".to_owned())
    }

    #[test]
    fn a_load_waits_for_the_page_and_sets_the_volume_first() {
        let (mut machine, ran, mut events) = machine(false);
        machine.load("123", Duration::from_secs(7));
        assert!(ran.try_recv().is_err());
        assert_eq!(
            drain(&mut events),
            [PlaybackEvent::Loading {
                id: id(),
                at: Duration::from_secs(7)
            }]
        );
        say(&mut machine, r#"{"t":"ready"}"#);
        let scripts: Vec<String> = ran.try_iter().collect();
        assert!(scripts.last().unwrap().ends_with(r#"load("123",7,1)"#));
        assert!(
            scripts[..scripts.len() - 1]
                .iter()
                .all(|script| script.ends_with("volume(0.5)"))
        );
    }

    #[test]
    fn a_held_track_opens_nothing_until_play() {
        let (mut machine, ran, mut events) = machine(false);
        machine.hold("123", Duration::from_secs(30));
        assert!(machine.stage == Stage::Closed);
        assert!(drain(&mut events).is_empty());
        machine.play();
        assert!(machine.stage == Stage::Opening);
        say(&mut machine, r#"{"t":"ready"}"#);
        assert!(
            ran.try_iter()
                .any(|script| script.ends_with(r#"load("123",30,2)"#))
        );
    }

    #[test]
    fn reports_become_playing_then_positions_then_the_end() {
        let (mut machine, _ran, mut events) = machine(false);
        machine.load("123", Duration::ZERO);
        say(&mut machine, r#"{"t":"ready"}"#);
        // what the page says while it is still fetching is not a pause
        say(&mut machine, &state(1, "paused", 0.));
        say(&mut machine, &state(1, "playing", 0.1));
        say(&mut machine, &state(1, "playing", 0.1));
        say(&mut machine, &state(1, "playing", 1.));
        // the page pauses and rewinds at the end before it says the track ended
        say(&mut machine, &state(1, "paused", 200.));
        say(&mut machine, &state(1, "paused", 0.));
        say(&mut machine, &state(1, "ended", 0.));
        say(&mut machine, &state(1, "stopped", 0.));
        let events = drain(&mut events);
        assert!(matches!(events[1], PlaybackEvent::Length { .. }));
        assert!(matches!(
            &events[2],
            PlaybackEvent::Signal { track, .. } if track.direct
        ));
        assert!(matches!(events[3], PlaybackEvent::Playing { .. }));
        assert!(matches!(events[4], PlaybackEvent::Position { .. }));
        assert_eq!(events[5], PlaybackEvent::Ended { id: id() });
        assert_eq!(events.len(), 6);
    }

    #[test]
    fn reports_of_an_earlier_load_are_dropped() {
        let (mut machine, _ran, mut events) = machine(false);
        machine.load("123", Duration::ZERO);
        say(&mut machine, r#"{"t":"ready"}"#);
        machine.load("456", Duration::ZERO);
        drain(&mut events);
        say(&mut machine, &state(1, "playing", 3.));
        say(
            &mut machine,
            r#"{"t":"error","serial":1,"code":"CONTENT_UNAVAILABLE"}"#,
        );
        assert!(drain(&mut events).is_empty());
    }

    #[test]
    fn an_account_given_a_preview_is_told_it_needs_a_subscription() {
        let (mut machine, _ran, mut events) = machine(false);
        machine.load("123", Duration::ZERO);
        say(&mut machine, r#"{"t":"ready"}"#);
        drain(&mut events);
        say(
            &mut machine,
            r#"{"t":"state","serial":1,"state":"playing","at":0,"duration":30,"preview":true,"authorized":true}"#,
        );
        assert_eq!(
            drain(&mut events),
            [PlaybackEvent::Barred(Barrier::Subscription)]
        );
        assert!(machine.held.is_none());
    }

    #[test]
    fn a_visitor_hears_the_preview() {
        let (mut machine, _ran, mut events) = machine(true);
        machine.load("123", Duration::ZERO);
        say(&mut machine, r#"{"t":"ready"}"#);
        say(
            &mut machine,
            r#"{"t":"state","serial":1,"state":"playing","at":0,"duration":30,"preview":true,"authorized":false}"#,
        );
        assert!(
            drain(&mut events)
                .iter()
                .any(|event| matches!(event, PlaybackEvent::Playing { .. }))
        );
    }

    #[test]
    fn error_codes_sort_into_what_the_listener_can_do() {
        assert_eq!(Trouble::of("AUTHORIZATION_ERROR"), Trouble::SignIn);
        assert_eq!(Trouble::of("SUBSCRIPTION_ERROR"), Trouble::Subscription);
        assert_eq!(Trouble::of("MEDIA_LICENSE"), Trouble::Engine);
        assert_eq!(Trouble::of("GEO_BLOCK"), Trouble::Track);
        assert_eq!(Trouble::of("PLAY_ACTIVITY"), Trouble::Passing);
        assert_eq!(Trouble::of("anything else"), Trouble::Track);
    }

    #[test]
    fn a_page_that_closes_fails_a_playing_track_and_keeps_a_paused_one() {
        let (mut machine, _ran, mut events) = machine(false);
        machine.load("123", Duration::ZERO);
        say(&mut machine, r#"{"t":"ready"}"#);
        say(&mut machine, &state(1, "playing", 5.));
        say(&mut machine, &state(1, "paused", 9.));
        drain(&mut events);
        machine.hear(Heard::Closed);
        assert!(drain(&mut events).is_empty());
        assert_eq!(machine.held.as_ref().unwrap().at, Duration::from_secs(9));

        machine.play();
        machine.hear(Heard::Closed);
        assert_eq!(
            drain(&mut events).last(),
            Some(&PlaybackEvent::Unavailable { id: id() })
        );
    }

    #[test]
    fn what_the_page_sends_is_not_trusted() {
        let (mut machine, ran, mut events) = machine(false);
        machine.load("\");alert(1);//", Duration::ZERO);
        say(&mut machine, r#"{"t":"ready"}"#);
        assert!(
            ran.try_iter()
                .any(|script| script.ends_with(r#"load("\");alert(1);//",0,1)"#))
        );
        drain(&mut events);
        say(&mut machine, "not json");
        say(
            &mut machine,
            r#"{"t":"state","serial":1,"state":"playing","at":-4}"#,
        );
        say(&mut machine, &"x".repeat(LONGEST + 1));
        assert_eq!(seconds(f64::NAN), Duration::ZERO);
        assert_eq!(seconds(f64::INFINITY), Duration::ZERO);
        assert!(drain(&mut events).iter().all(|event| !matches!(
            event,
            PlaybackEvent::Position { at, .. } | PlaybackEvent::Playing { at, .. } if !at.is_zero()
        )));
    }
}
