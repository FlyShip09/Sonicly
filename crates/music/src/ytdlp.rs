//! Downloads from a link through the listener's own yt-dlp (`docs/DESIGN.md` 8).
//!
//! Sonicly never ships, fetches or updates yt-dlp. It finds one the listener installed and runs
//! it as a child process, with a fixed list of arguments and no shell, in the folder the
//! download goes to. It asks for the audio stream as the site serves it, so nothing is
//! re-encoded and no ffmpeg is needed, and it tags the file itself afterwards.
//!
//! Progress and the facts about each file are read from stdout, in a form the arguments ask
//! for: one line per event, each behind a prefix of this module's. Everything yt-dlp prints is
//! data. It is parsed, cut to size and shown as plain text, and a file name it reports is only
//! ever used inside the folder the listener chose.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile as _, TaggedFileExt as _};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::Accessor as _;
use lofty::probe::Probe;
use lofty::tag::Tag;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use crate::cover::{self, CoverLimits};

/// What the program is called, on `PATH` and in what the listener reads.
pub const PROGRAM: &str = "yt-dlp";

/// The audio stream as the site serves it, an M4A where there is one, since that is the
/// container Sonicly can tag. Asking for a stream rather than a conversion is what keeps
/// ffmpeg out of it.
const FORMAT: &str = "bestaudio[ext=m4a]/bestaudio";
/// What each file is called, inside the folder yt-dlp runs in. The id keeps two uploads of one
/// title apart.
const OUTPUT: &str = "%(title)s [%(id)s].%(ext)s";

/// The start of a line that names the file a download is about to write.
const FILE: &str = "SONICLY-FILE ";
/// The start of a progress line: bytes arrived, bytes in all, an estimate of them, bytes a
/// second and seconds left, each a number or `NA`.
const PROGRESS: &str = "SONICLY-PROGRESS ";
/// The start of a line that says a file is in place, with what the site says about it.
const DONE: &str = "SONICLY-DONE ";
/// What comes after [`DONE`]: a JSON list, each field written as JSON by yt-dlp itself, so a
/// title cannot break out of its place whatever it holds. A missing field is an empty string.
const FACTS: &str = "[%(filepath)j,%(title|)j,%(track|)j,%(artist|)j,%(creator|)j,\
                     %(uploader|)j,%(channel|)j,%(album|)j]";
/// How yt-dlp starts the line that says why it gave up.
const ERROR: &str = "ERROR:";

/// The longest line read from yt-dlp. Nothing this module asks for comes near it.
const LINE: usize = 64 * 1024;
/// The longest title, artist or error kept from yt-dlp, in characters.
const TEXT: usize = 300;
/// What yt-dlp may save a thumbnail as.
const THUMBNAILS: [&str; 4] = ["webp", "jpg", "jpeg", "png"];
/// A thumbnail is embedded as a cover no larger than a big screen shows it.
const COVER: CoverLimits = CoverLimits {
    side: 1200,
    bytes: 2 * 1024 * 1024,
};
/// How much older than the download a thumbnail's time may read and still count as written by
/// it, for a filesystem that keeps whole seconds.
const SLACK: Duration = Duration::from_secs(2);
/// Keeps a console window from opening for the child on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// One download: which program fetches which link into which folder.
#[derive(Clone, Debug)]
pub struct Request {
    pub program: PathBuf,
    pub link: String,
    pub folder: PathBuf,
    /// Whether a link to a playlist fetches all of it rather than the one item it points at.
    pub playlist: bool,
}

/// What a running download reports, as yt-dlp said it.
#[derive(Clone, Debug, PartialEq)]
pub enum Update {
    /// The file now being written, by its name without the extension.
    File(String),
    Progress(Progress),
}

/// How far one file has got. Each part is there only when yt-dlp reported it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Progress {
    /// The share that has arrived, from 0 to 1.
    pub fraction: Option<f32>,
    /// Bytes a second.
    pub speed: Option<f64>,
    /// Seconds left.
    pub eta: Option<u64>,
}

/// A file a download left in the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub path: PathBuf,
    pub title: String,
    /// Whether title, artist, album and cover could be written into it. A container Sonicly
    /// cannot tag, such as WebM, is kept as it came.
    pub tagged: bool,
}

/// Why a download did not happen. Each one has its own explanation in the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// No yt-dlp on `PATH` or at the path the listener set.
    Missing,
    /// The link is not an http or https address.
    Link,
    /// The folder to save into is not there, or is not one of the library's.
    Folder,
    /// yt-dlp knows no site the link belongs to.
    Unsupported,
    /// The media is DRM-protected, which yt-dlp refuses and Sonicly does too.
    Protected,
    /// The site serves this media with no audio stream of its own, only picture and sound in
    /// one file, and taking the sound out of that would need ffmpeg.
    NoAudio,
    /// The site could not be reached.
    Network,
    /// The site answered that this media is not to be had: private, removed, behind a sign-in
    /// or not offered in this country.
    Unavailable,
    /// The listener stopped it.
    Cancelled,
    /// Anything else, with yt-dlp's own last error line when it gave one.
    Failed(String),
}

/// The yt-dlp to run: the file the listener `chose` when it is there, else the first one on
/// `PATH`.
pub fn locate(chosen: Option<&Path>) -> Option<PathBuf> {
    if let Some(chosen) = chosen.filter(|chosen| chosen.is_file()) {
        return Some(chosen.to_path_buf());
    }
    let name = format!("{PROGRAM}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|folder| folder.join(&name))
        .find(|candidate| candidate.is_file())
}

/// The link as yt-dlp is given it, or `None` unless it is an http or https address with a
/// host. Anything else, a file path or an option among them, never reaches the program.
pub fn link(text: &str) -> Option<String> {
    let url = reqwest::Url::parse(text.trim()).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.has_host()).then(|| url.into())
}

/// The arguments yt-dlp is run with for `link`, which [`link`] has checked. The listener's own
/// yt-dlp configuration is left out, so a setting of theirs cannot run a program after the
/// download or move the file elsewhere, and the link comes after `--`, where nothing is read as
/// an option.
pub fn arguments(link: &str, playlist: bool) -> Vec<String> {
    let items = match playlist {
        true => "--yes-playlist",
        false => "--no-playlist",
    };
    let progress = format!(
        "download:{PROGRESS}%(progress.downloaded_bytes)s %(progress.total_bytes)s \
         %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s"
    );
    [
        "--ignore-config",
        "--no-exec",
        items,
        "--format",
        FORMAT,
        "--no-restrict-filenames",
        "--no-mtime",
        "--write-thumbnail",
        "--encoding",
        "utf-8",
        "--no-simulate",
        "--progress",
        "--newline",
        "--progress-template",
        &progress,
        "--print",
        &format!("before_dl:{FILE}%(filename)j"),
        "--print",
        &format!("after_move:{DONE}{FACTS}"),
        "--output",
        OUTPUT,
        "--",
        link,
    ]
    .map(str::to_owned)
    .to_vec()
}

/// Runs one download to its end and tags what it saved. `update` hears what yt-dlp reports as
/// it goes. Sending on `cancel`, or dropping its other end, stops the program and removes the
/// files it had not finished.
///
/// A playlist that saved some of its items counts as done, since one that is gone from the
/// site is the usual case there.
pub async fn download(
    request: Request,
    update: impl Fn(Update) + Send + 'static,
    mut cancel: oneshot::Receiver<()>,
) -> Result<Vec<Saved>, Trouble> {
    let link = link(&request.link).ok_or(Trouble::Link)?;
    if !request.folder.is_dir() {
        return Err(Trouble::Folder);
    }
    let started = SystemTime::now();
    let mut command = Command::new(&request.program);
    command
        .args(arguments(&link, request.playlist))
        .current_dir(&request.folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => Trouble::Missing,
        _ => Trouble::Failed(clean(&format!("cannot run {PROGRAM}: {error}"))),
    })?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(Trouble::Failed(String::new()));
    };

    // Both pipes are read to their end, so the program never waits on a full one, and into one
    // queue, since which of them carries the progress has changed between yt-dlp versions.
    let (lines, mut heard) = unbounded_channel();
    tokio::spawn(pump(stdout, lines.clone()));
    tokio::spawn(pump(stderr, lines));
    let mut begun: Vec<String> = Vec::new();
    let mut done: Vec<Facts> = Vec::new();
    let mut complaint: Option<String> = None;
    loop {
        tokio::select! {
            _ = &mut cancel => {
                end(&mut child).await;
                let folder = request.folder.clone();
                let finished: Vec<String> = done.iter().map(|facts| facts.file.clone()).collect();
                tokio::task::spawn_blocking(move || sweep(&folder, &begun, &finished, started))
                    .await
                    .ok();
                return Err(Trouble::Cancelled);
            }
            line = heard.recv() => {
                let Some(line) = line else { break };
                match read(&line) {
                    Some(Line::File(name)) => {
                        update(Update::File(stem(&name)));
                        begun.push(name);
                    }
                    Some(Line::Progress(progress)) => update(Update::Progress(progress)),
                    Some(Line::Done(facts)) => done.push(facts),
                    Some(Line::Error(error)) => complaint = Some(error),
                    None => {}
                }
            }
        }
    }
    let status = child.wait().await;
    if !status.as_ref().is_ok_and(|status| status.success()) {
        log::warn!("downloads: {PROGRAM} ended with {status:?}");
    }

    // Whatever a file that failed left behind goes too, so the music folder only ever gains
    // whole files.
    let folder = request.folder;
    let saved = tokio::task::spawn_blocking(move || {
        let finished: Vec<String> = done.iter().map(|facts| facts.file.clone()).collect();
        sweep(&folder, &begun, &finished, started);
        done.iter()
            .filter_map(|facts| keep(&folder, facts, started))
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    match saved.is_empty() {
        true => Err(sort(complaint.as_deref())),
        false => Ok(saved),
    }
}

/// One line of yt-dlp's output that this module asked for, or its word on why it gave up.
#[derive(Debug, PartialEq)]
enum Line {
    /// The name of the file about to be written.
    File(String),
    Progress(Progress),
    Done(Facts),
    /// What follows `ERROR:`, cleaned.
    Error(String),
}

/// What yt-dlp said about a file it finished. Each text is cleaned and may be empty.
#[derive(Debug, Default, PartialEq)]
struct Facts {
    /// The file's name alone, whatever path yt-dlp gave.
    file: String,
    title: String,
    artist: String,
    album: String,
}

/// Reads one line of output. A line without one of this module's prefixes or yt-dlp's `ERROR:`,
/// or one that does not hold what its prefix promises, is nothing.
fn read(line: &str) -> Option<Line> {
    let line = line.trim();
    if let Some(error) = line.strip_prefix(ERROR) {
        return Some(Line::Error(clean(error)));
    }
    if let Some(name) = line.strip_prefix(FILE) {
        let path: String = serde_json::from_str(name).ok()?;
        return file_name(&path).map(Line::File);
    }
    if let Some(numbers) = line.strip_prefix(PROGRESS) {
        return progress(numbers).map(Line::Progress);
    }
    let facts: Vec<Value> = serde_json::from_str(line.strip_prefix(DONE)?).ok()?;
    let text = |place: usize| clean(facts.get(place).and_then(Value::as_str).unwrap_or_default());
    let first = |places: &[usize]| {
        places
            .iter()
            .map(|place| text(*place))
            .find(|text| !text.is_empty())
            .unwrap_or_default()
    };
    Some(Line::Done(Facts {
        file: file_name(facts.first()?.as_str()?)?,
        // a music upload names its track and artist, and anything else has a title and whoever
        // uploaded it
        title: first(&[2, 1]),
        artist: topic(&first(&[3, 4, 5, 6])).to_owned(),
        album: text(7),
    }))
}

/// Reads the five numbers of a progress line. The share is worked out from the bytes yt-dlp
/// counted, against the size it knows or else the one it estimates.
fn progress(numbers: &str) -> Option<Progress> {
    let mut numbers = numbers.split_whitespace().map(|number| {
        number
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite())
    });
    let (arrived, total, estimate, speed, eta) = (
        numbers.next()?,
        numbers.next()?,
        numbers.next()?,
        numbers.next()?,
        numbers.next()?,
    );
    let fraction = arrived
        .zip(total.or(estimate).filter(|total| *total > 0.0))
        .map(|(arrived, total)| (arrived / total).clamp(0.0, 1.0) as f32);
    Some(Progress {
        fraction,
        speed: speed.filter(|speed| *speed >= 0.0),
        eta: eta.filter(|eta| *eta >= 0.0).map(|eta| eta as u64),
    })
}

/// Hands every line of one of yt-dlp's pipes to `lines`, until the pipe closes. A line too long
/// to be one this module asked for is dropped, and bytes that are not UTF-8 are replaced.
async fn pump(pipe: impl AsyncRead + Unpin, lines: UnboundedSender<String>) {
    let mut read = BufReader::new(pipe).split(b'\n');
    while let Ok(Some(line)) = read.next_segment().await {
        if line.len() <= LINE
            && lines
                .send(String::from_utf8_lossy(&line).into_owned())
                .is_err()
        {
            return;
        }
    }
}

/// Tells which trouble yt-dlp's last error line means, from the words it uses for each.
fn sort(error: Option<&str>) -> Trouble {
    let Some(error) = error else {
        return Trouble::Failed(String::new());
    };
    let lowered = error.to_lowercase();
    let says = |words: &[&str]| words.iter().any(|word| lowered.contains(word));
    if says(&["drm"]) {
        Trouble::Protected
    } else if says(&["unsupported url", "is not a valid url"]) {
        Trouble::Unsupported
    } else if says(&["requested format is not available"]) {
        Trouble::NoAudio
    } else if says(&[
        "getaddrinfo",
        "name resolution",
        "name or service not known",
        "timed out",
        "connection refused",
        "connection reset",
        "network is unreachable",
        "no route to host",
        "urlopen error",
        "transporterror",
        "unable to connect",
    ]) {
        Trouble::Network
    } else if says(&[
        "private",
        "unavailable",
        "sign in",
        "log in",
        "login",
        "members-only",
        "not available",
        "removed",
        "http error 403",
        "http error 404",
    ]) {
        Trouble::Unavailable
    } else {
        Trouble::Failed(error.to_owned())
    }
}

/// Stops yt-dlp and waits until it is gone, so the files it held can be removed.
async fn end(child: &mut Child) {
    // yt-dlp's Windows build unpacks itself and runs as a second process, which killing the
    // first would leave downloading. `taskkill /T` ends the whole tree.
    // shortcut: quitting Sonicly mid-download kills only the first process (`kill_on_drop`),
    // so the second finishes its file; a job object would end both.
    #[cfg(windows)]
    if let Some(id) = child.id() {
        let system =
            std::env::var_os("SystemRoot").map_or_else(|| "C:\\Windows".into(), PathBuf::from);
        let _ = Command::new(system.join("System32").join("taskkill.exe"))
            .args(["/PID", &id.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .await;
    }
    let _ = child.kill().await;
}

/// Removes what a stopped download left in `folder`: the partial files of every file it had
/// `begun` and not `finished`, and the thumbnails it wrote for them.
fn sweep(folder: &Path, begun: &[String], finished: &[String], started: SystemTime) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let unfinished: Vec<&String> = begun
        .iter()
        .filter(|name| !finished.contains(name))
        .collect();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let partial = unfinished.iter().any(|begun| {
            name.strip_prefix(begun.as_str())
                .is_some_and(|rest| rest.starts_with(".part") || rest == ".ytdl")
        });
        if partial && let Err(error) = std::fs::remove_file(entry.path()) {
            log::warn!("downloads: cannot remove {name}: {error}");
        }
    }
    for name in unfinished {
        thumbnail(&folder.join(name), started);
    }
}

/// Tags one finished file and says what it is. `None` when the file yt-dlp named is not in
/// `folder`, which is the only place it is looked for.
fn keep(folder: &Path, facts: &Facts, started: SystemTime) -> Option<Saved> {
    let path = folder.join(&facts.file);
    if !path.is_file() {
        log::warn!("downloads: {PROGRAM} reported a file that is not in the folder");
        return None;
    }
    let cover = thumbnail(&path, started).and_then(|picture| {
        cover::square_jpeg(&picture, COVER)
            .inspect_err(|error| log::warn!("downloads: cannot use the thumbnail: {error:#}"))
            .ok()
    });
    let tagged = tag(&path, facts, cover)
        .inspect_err(|error| log::warn!("downloads: cannot tag the download: {error:#}"))
        .is_ok();
    let title = match facts.title.is_empty() {
        true => stem(&facts.file),
        false => facts.title.clone(),
    };
    Some(Saved {
        path,
        title,
        tagged,
    })
}

/// Takes the thumbnail yt-dlp wrote beside `media` off the disk and returns its bytes. A
/// picture older than the download was there before it and is someone else's, so it is left
/// alone.
fn thumbnail(media: &Path, started: SystemTime) -> Option<Vec<u8>> {
    let written = started.checked_sub(SLACK).unwrap_or(started);
    THUMBNAILS.iter().find_map(|extension| {
        let picture = media.with_extension(extension);
        let fresh = std::fs::metadata(&picture)
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| modified >= written);
        if !fresh {
            return None;
        }
        let bytes = std::fs::read(&picture).ok();
        if let Err(error) = std::fs::remove_file(&picture) {
            log::warn!("downloads: cannot remove a thumbnail: {error}");
        }
        bytes
    })
}

/// Writes the title, artist, album and cover into the file's own tag, adding one when it has
/// none. A text yt-dlp did not report is left as the file has it.
fn tag(path: &Path, facts: &Facts, cover: Option<Vec<u8>>) -> Result<()> {
    let mut tagged = Probe::open(path)
        .context("cannot open the download")?
        .read()
        .context("cannot read the download's tags")?;
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
    }
    let tag = tagged
        .primary_tag_mut()
        .context("the download cannot hold tags")?;
    if !facts.title.is_empty() {
        tag.set_title(facts.title.clone());
    }
    if !facts.artist.is_empty() {
        tag.set_artist(facts.artist.clone());
    }
    if !facts.album.is_empty() {
        tag.set_album(facts.album.clone());
    }
    if let Some(cover) = cover {
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(
            Picture::unchecked(cover)
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
    }
    tagged
        .save_to_path(path, WriteOptions::default())
        .context("cannot save the download's tags")
}

/// The last part of a path yt-dlp reported, which is all of it that is trusted. `None` for a
/// path that ends in no name.
fn file_name(path: &str) -> Option<String> {
    let name = path.rsplit(std::path::is_separator).next()?;
    // On Windows a name can still carry a drive or a stream after a colon.
    let odd =
        name.is_empty() || name == "." || name == ".." || (cfg!(windows) && name.contains(':'));
    (!odd).then(|| name.to_owned())
}

/// A file's name without its extension, as text to show.
fn stem(name: &str) -> String {
    let stem = Path::new(name).file_stem().unwrap_or_default();
    clean(&stem.to_string_lossy())
}

/// The artist of a channel YouTube made for one, which it names `Artist - Topic`.
fn topic(channel: &str) -> &str {
    channel.strip_suffix(" - Topic").unwrap_or(channel)
}

/// A text from yt-dlp made fit to store and show: no control characters, no space at either
/// end, and no longer than [`TEXT`] characters.
fn clean(text: &str) -> String {
    let kept: String = text
        .chars()
        .map(|letter| match letter.is_control() {
            true => ' ',
            false => letter,
        })
        .collect();
    kept.trim()
        .chars()
        .take(TEXT)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_and_https_links_pass() {
        assert_eq!(
            link("  https://example.com/watch?v=1 ").as_deref(),
            Some("https://example.com/watch?v=1")
        );
        assert!(link("http://example.com/a").is_some());
        for refused in [
            "",
            "example.com/watch",
            "file:///etc/passwd",
            "ftp://example.com/a.mp3",
            "--exec=calc",
            "-o /tmp/x https://example.com",
            "javascript:alert(1)",
            "https://",
        ] {
            assert_eq!(link(refused), None, "{refused}");
        }
    }

    #[test]
    fn the_link_is_the_last_argument_after_the_options_end() {
        let arguments = arguments("https://example.com/a", false);
        let last = arguments.len() - 1;
        assert_eq!(arguments[last], "https://example.com/a");
        assert_eq!(arguments[last - 1], "--");
        assert_eq!(arguments[0], "--ignore-config");
        assert!(arguments.contains(&"--no-exec".to_owned()));
        assert!(arguments.contains(&"--no-playlist".to_owned()));
        assert!(!arguments.contains(&"--yes-playlist".to_owned()));
        let format = arguments.iter().position(|argument| argument == "--format");
        assert_eq!(arguments[format.unwrap() + 1], FORMAT);
        // the file name is a template with no folder in it, so it lands where yt-dlp runs
        let output = arguments.iter().position(|argument| argument == "--output");
        assert!(!arguments[output.unwrap() + 1].contains(['/', '\\']));

        assert!(
            super::arguments("https://example.com/a", true).contains(&"--yes-playlist".to_owned())
        );
    }

    #[test]
    fn reads_progress_as_reported() {
        assert_eq!(
            read("SONICLY-PROGRESS 500 2000 NA 1024.5 12"),
            Some(Line::Progress(Progress {
                fraction: Some(0.25),
                speed: Some(1024.5),
                eta: Some(12),
            }))
        );
        // the estimate stands in for a size the site did not send
        assert_eq!(
            read("SONICLY-PROGRESS 300 NA 600.0 NA NA\r"),
            Some(Line::Progress(Progress {
                fraction: Some(0.5),
                speed: None,
                eta: None,
            }))
        );
        // nothing known is still a line, with nothing in it to show
        assert_eq!(
            read("SONICLY-PROGRESS NA NA NA NA NA"),
            Some(Line::Progress(Progress::default()))
        );
        assert_eq!(read("SONICLY-PROGRESS 1 2"), None);
        assert_eq!(read("[download] 50% of 3MiB"), None);
    }

    #[test]
    fn reads_the_file_about_to_be_written_by_its_name_alone() {
        assert_eq!(
            read(r#"SONICLY-FILE "Song [abc].m4a""#),
            Some(Line::File("Song [abc].m4a".to_owned()))
        );
        // a path that points elsewhere keeps only its last part
        assert_eq!(
            read(r#"SONICLY-FILE "../../elsewhere/Song.m4a""#),
            Some(Line::File("Song.m4a".to_owned()))
        );
        assert_eq!(read(r#"SONICLY-FILE "..""#), None);
        assert_eq!(read("SONICLY-FILE not json"), None);
    }

    #[test]
    fn reads_what_the_site_says_about_a_finished_file() {
        let line = r#"SONICLY-DONE ["/music/Song [abc].m4a","Artist - Song (Official Video)","Song","Artist","","Artist - Topic","Artist - Topic","Album"]"#;
        assert_eq!(
            read(line),
            Some(Line::Done(Facts {
                file: "Song [abc].m4a".to_owned(),
                title: "Song".to_owned(),
                artist: "Artist".to_owned(),
                album: "Album".to_owned(),
            }))
        );

        // an upload with no music metadata falls back on its title and its channel
        let plain = r#"SONICLY-DONE ["Talk [x].webm","A\u0000 talk\n","","","","","Some Channel - Topic",""]"#;
        assert_eq!(
            read(plain),
            Some(Line::Done(Facts {
                file: "Talk [x].webm".to_owned(),
                title: "A  talk".to_owned(),
                artist: "Some Channel".to_owned(),
                album: String::new(),
            }))
        );
        assert_eq!(read("SONICLY-DONE []"), None);
        assert_eq!(read(r#"SONICLY-DONE {"filepath":"a.m4a"}"#), None);
    }

    #[test]
    fn sorts_the_error_yt_dlp_gave() {
        assert_eq!(
            sort(Some("Unsupported URL: https://example.com/")),
            Trouble::Unsupported
        );
        assert_eq!(
            sort(Some("[site] 1: This video is DRM protected")),
            Trouble::Protected
        );
        assert_eq!(
            sort(Some(
                "Unable to download webpage: <urlopen error [Errno 11001] getaddrinfo failed>"
            )),
            Trouble::Network
        );
        assert_eq!(
            sort(Some(
                "[youtube] abc: Private video. Sign in if you've been granted access"
            )),
            Trouble::Unavailable
        );
        assert_eq!(
            sort(Some(
                "[generic] Requested format is not available. Use --list-formats"
            )),
            Trouble::NoAudio
        );
        assert_eq!(
            sort(Some("something new")),
            Trouble::Failed("something new".to_owned())
        );
        // the line itself is read off either pipe, cut clean of what a terminal would act on
        assert_eq!(
            read("ERROR: \u{1b}[0;31mUnsupported URL\u{1b}[0m: x\r"),
            Some(Line::Error("[0;31mUnsupported URL [0m: x".to_owned()))
        );
        assert_eq!(sort(None), Trouble::Failed(String::new()));
    }

    #[test]
    fn cleans_text_before_it_is_kept() {
        assert_eq!(clean("  a\tb\u{1b}[31m  "), "a b [31m");
        assert_eq!(clean(&"x".repeat(TEXT + 50)).chars().count(), TEXT);
        assert_eq!(stem("Song [abc].m4a"), "Song [abc]");
        assert_eq!(topic("Artist - Topic"), "Artist");
        assert_eq!(topic("Artist"), "Artist");
    }

    #[test]
    fn a_stopped_download_leaves_only_what_was_finished() {
        let folder = std::env::temp_dir().join(format!("sonicly-ytdlp-test-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&folder).unwrap();
        let started = SystemTime::now();
        for name in [
            "Done [a].m4a",
            "Half [b].m4a.part",
            "Half [b].m4a.part-Frag3",
            "Half [b].m4a.ytdl",
            "Half [b].webp",
            "Other.m4a.part",
        ] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        let begun = ["Done [a].m4a".to_owned(), "Half [b].m4a".to_owned()];
        sweep(&folder, &begun, &begun[..1], started);

        let mut left: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        // another download's partial file is not this one's to remove
        assert_eq!(left, ["Done [a].m4a", "Other.m4a.part"]);
        std::fs::remove_dir_all(&folder).ok();
    }
}
