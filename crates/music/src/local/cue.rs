//! Cue sheets, the text files that name the tracks inside one long audio file, as a ripped CD
//! kept as a single FLAC has beside it.
//!
//! A sheet next to the file it describes makes that file appear as its tracks. Each track's id
//! names the file and the stretch of it to play (`wire::cue_track_id`), and playback trims the
//! decoder to that stretch, so nothing else has to know the track is not a file of its own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::m3u;
use super::wire::{self, Tagged};

/// The extension a cue sheet carries.
const EXTENSION: &str = "cue";

/// What a cue sheet says: the album in its header, then each file with the tracks cut from it.
#[derive(Debug, Default, PartialEq)]
pub struct Sheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    /// The year of a `REM DATE` line.
    pub year: Option<i32>,
    pub files: Vec<File>,
}

/// One `FILE` of a sheet and the tracks that start in it.
#[derive(Debug, Default, PartialEq)]
pub struct File {
    pub name: String,
    pub tracks: Vec<Entry>,
}

/// One audio `TRACK` of a sheet.
#[derive(Debug, Default, PartialEq)]
pub struct Entry {
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    /// Where `INDEX 01` puts the track's first sample, in frames of 1/75 s from the start of
    /// the file.
    pub start: u64,
}

/// Whether `path` is named like a cue sheet.
pub fn is_sheet(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
}

/// Reads a cue sheet. Lines it does not know are skipped, a track with no `INDEX 01` or of a
/// kind other than audio is left out, and so is a pregap (`INDEX 00`), which stays the tail of
/// the track before it as on the disc.
pub fn parse(bytes: &[u8]) -> Sheet {
    let mut sheet = Sheet::default();
    // The track being read and whether its start has been seen, kept apart from the sheet
    // until it has one.
    let mut open: Option<(Entry, bool)> = None;

    for line in m3u::decode(bytes).lines() {
        let (command, rest) = split(line.trim());
        match command.to_ascii_uppercase().as_str() {
            "FILE" => {
                close(&mut sheet, &mut open);
                sheet.files.push(File {
                    name: file_name(rest),
                    tracks: Vec::new(),
                });
            }
            "TRACK" => {
                close(&mut sheet, &mut open);
                let (number, kind) = split(rest);
                if kind.eq_ignore_ascii_case("AUDIO") {
                    let number = number.parse().unwrap_or_default();
                    open = Some((
                        Entry {
                            number,
                            ..Entry::default()
                        },
                        false,
                    ));
                }
            }
            "TITLE" => match &mut open {
                Some((entry, _)) => entry.title = text(rest),
                None => sheet.title = text(rest),
            },
            "PERFORMER" => match &mut open {
                Some((entry, _)) => entry.performer = text(rest),
                None => sheet.performer = text(rest),
            },
            "INDEX" => {
                let (index, time) = split(rest);
                if index.parse() == Ok(1u32)
                    && let Some((entry, started)) = &mut open
                    && let Some(start) = frames(time)
                {
                    entry.start = start;
                    *started = true;
                }
            }
            "REM" => {
                let (key, value) = split(rest);
                if key.eq_ignore_ascii_case("DATE") && open.is_none() {
                    sheet.year = year(value);
                }
            }
            _ => {}
        }
    }
    close(&mut sheet, &mut open);
    sheet
}

/// Replaces every file in `parsed` that one of `sheets` cuts up with the tracks that sheet
/// names, in the file's place. A sheet that cannot be read, names several files, or names one
/// the scan did not find beside it changes nothing: several files are already a track each.
pub fn apply(parsed: Vec<Tagged>, sheets: &[PathBuf]) -> Vec<Tagged> {
    let mut cut: HashMap<usize, Vec<Tagged>> = HashMap::new();
    for path in sheets {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let sheet = parse(&bytes);
        let Some(whole) = described(&sheet, path, &parsed) else {
            continue;
        };
        let Some(audio) = file_of(&parsed[whole]) else {
            continue;
        };
        let tracks = tracks(&sheet, &parsed[whole], audio);
        if !tracks.is_empty() {
            cut.insert(whole, tracks);
        }
    }
    if cut.is_empty() {
        return parsed;
    }
    parsed
        .into_iter()
        .enumerate()
        .flat_map(|(index, whole)| cut.remove(&index).unwrap_or_else(|| vec![whole]))
        .collect()
}

/// The tracks `sheet` cuts out of the file at `path`, which the scan read as `whole`. Each one
/// keeps what the file's own tags say (cover, genre, disc) and takes its title, artist, number
/// and place in the file from the sheet, and its album and album artist from the sheet's
/// header. A track ends where the next one starts, and the last one where the file does.
pub fn tracks(sheet: &Sheet, whole: &Tagged, path: &Path) -> Vec<Tagged> {
    let [file] = sheet.files.as_slice() else {
        return Vec::new();
    };
    let length = (whole.track.duration.as_secs_f64() * wire::CUE_FRAMES).round() as u64;
    let album = sheet
        .title
        .clone()
        .or_else(|| Some(whole.track.album.clone()).filter(|album| !album.is_empty()))
        .unwrap_or_else(|| whole.track.name.clone());
    let album_artist = sheet
        .performer
        .clone()
        .or_else(|| whole.album_artist.clone());
    let owner = album_artist
        .clone()
        .unwrap_or_else(|| wire::album_folder(path).to_string_lossy().into_owned());

    file.tracks
        .iter()
        .enumerate()
        .filter_map(|(place, entry)| {
            // A file whose length is not known still plays its last track to the end, and a
            // sheet that runs past a shorter file has its last fitting track end with the file.
            let known = Some(length).filter(|length| *length > 0);
            let end = file
                .tracks
                .get(place + 1)
                .map(|next| next.start)
                .filter(|end| known.is_none_or(|length| *end < length));
            let stop = end.or(known);
            if stop.is_some_and(|stop| stop <= entry.start) {
                return None;
            }

            let mut tagged = whole.clone();
            let track = &mut tagged.track;
            track.id = Some(wire::cue_track_id(path, entry.start, end));
            track.name = entry
                .title
                .clone()
                .unwrap_or_else(|| format!("Track {:02}", entry.number));
            track.track_number = entry.number;
            track.duration =
                stop.map_or_else(Default::default, |stop| wire::cue_time(stop - entry.start));
            if let Some(artist) = entry.performer.as_ref().or(sheet.performer.as_ref()) {
                track.artists = artist.clone();
                track.artist_refs = vec![wire::artist_ref(artist)];
                // These named the file's artist as the main one.
                track.credits = Vec::new();
            }
            track.album = album.clone();
            track.album_artist = album_artist.clone().unwrap_or_default();
            track.album_id = Some(wire::album_id(&owner, &album));
            if let Some(performer) = &sheet.performer {
                tagged.album_artists = vec![wire::artist_ref(performer)];
            }
            tagged.album_artist = album_artist.clone();
            tagged.year = sheet.year.or(whole.year);
            Some(tagged)
        })
        .collect()
}

/// Which of `parsed` the one file of `sheet` is: the file it names beside itself, else one of
/// the same name with another extension, as after a WAV was packed into a FLAC, else the file
/// the sheet itself is named after (`Album.cue` or `Album.flac.cue`).
fn described(sheet: &Sheet, path: &Path, parsed: &[Tagged]) -> Option<usize> {
    let [file] = sheet.files.as_slice() else {
        return None;
    };
    let folder = path.parent()?;
    let named = Path::new(&file.name).file_name().map(Path::new);
    let beside: Vec<(usize, &Path)> = parsed
        .iter()
        .enumerate()
        .filter_map(|(index, tagged)| Some((index, file_of(tagged)?)))
        .filter(|(_, audio)| audio.parent() == Some(folder))
        .collect();
    let find = |matches: &dyn Fn(&Path) -> bool| {
        beside
            .iter()
            .find(|(_, audio)| matches(audio))
            .map(|(index, _)| *index)
    };
    let same = |left: Option<&std::ffi::OsStr>, right: Option<&std::ffi::OsStr>| {
        left.zip(right)
            .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
    };

    find(&|audio| same(audio.file_name(), named.map(Path::as_os_str)))
        .or_else(|| find(&|audio| same(audio.file_stem(), named.and_then(Path::file_stem))))
        .or_else(|| find(&|audio| same(audio.file_stem(), path.file_stem())))
        .or_else(|| find(&|audio| same(audio.file_name(), path.file_stem())))
}

/// The file a scanned track is, when it is a whole file.
fn file_of(tagged: &Tagged) -> Option<&Path> {
    let id = tagged.track.id.as_deref()?;
    match wire::window(id) {
        Some(_) => None,
        None => wire::path_from_track_id(id),
    }
}

/// Files the track being read under the sheet's last file, when it has a start and a file.
fn close(sheet: &mut Sheet, open: &mut Option<(Entry, bool)>) {
    if let Some((entry, true)) = open.take()
        && let Some(file) = sheet.files.last_mut()
    {
        file.tracks.push(entry);
    }
}

/// A line's first word and what follows it.
fn split(line: &str) -> (&str, &str) {
    let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    (word, rest.trim())
}

/// A value with the quotes around it taken off, or nothing when it is empty.
fn text(value: &str) -> Option<String> {
    let value = value
        .strip_prefix('"')
        .map_or(value, |rest| rest.strip_suffix('"').unwrap_or(rest))
        .trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// The name in a `FILE "name" WAVE` line: what the quotes hold, or everything but the type
/// when the name is not quoted.
fn file_name(rest: &str) -> String {
    let name = match rest.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"').map_or(quoted, |(name, _)| name),
        None => rest
            .rsplit_once(char::is_whitespace)
            .map_or(rest, |(name, _)| name),
    };
    name.trim().to_owned()
}

/// A cue time, `MM:SS:FF` with 75 frames to the second, as frames. The minutes run past 59 on
/// a long file.
fn frames(time: &str) -> Option<u64> {
    let mut parts = time.split(':').map(|part| part.trim().parse::<u64>().ok());
    let (minutes, seconds, frames) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() || seconds >= 60 || frames >= 75 {
        return None;
    }
    Some((minutes * 60 + seconds) * 75 + frames)
}

/// The year a `REM DATE` holds, which is a year alone or the start of a full date.
fn year(value: &str) -> Option<i32> {
    let value = text(value)?;
    value.get(..4)?.parse().ok().filter(|year| *year > 0)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::Track;

    const SHEET: &str = "\u{feff}REM GENRE Rock\r\nREM DATE 1997\r\nPERFORMER \"The Band\"\r\n\
        TITLE \"Live Album\"\r\nFILE \"Live Album.wav\" WAVE\r\n\
        \x20 TRACK 01 AUDIO\r\n    TITLE \"Opening\"\r\n    INDEX 01 00:00:00\r\n\
        \x20 TRACK 02 AUDIO\r\n    TITLE \"Second Song\"\r\n    PERFORMER \"A Guest\"\r\n\
        \x20   INDEX 00 03:58:10\r\n    INDEX 01 04:00:00\r\n\
        \x20 TRACK 03 AUDIO\r\n    INDEX 01 61:30:37\r\n";

    fn whole(path: &Path, seconds: u64) -> Tagged {
        Tagged {
            track: Track {
                id: Some(wire::track_id(path)),
                name: "Live Album".to_owned(),
                playable: true,
                artists: "Tagged Artist".to_owned(),
                artist_refs: vec![wire::artist_ref("Tagged Artist")],
                cover: Some("cover.jpg".to_owned()),
                duration: Duration::from_secs(seconds),
                ..Track::default()
            },
            album_artist: None,
            album_artists: Vec::new(),
            year: Some(2001),
            release: None,
            label: String::new(),
        }
    }

    #[test]
    fn reads_the_header_and_each_track() {
        let sheet = parse(SHEET.as_bytes());
        assert_eq!(sheet.title.as_deref(), Some("Live Album"));
        assert_eq!(sheet.performer.as_deref(), Some("The Band"));
        assert_eq!(sheet.year, Some(1997));
        assert_eq!(sheet.files.len(), 1);
        assert_eq!(sheet.files[0].name, "Live Album.wav");

        let tracks = &sheet.files[0].tracks;
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[0].title.as_deref(), Some("Opening"));
        assert_eq!(tracks[0].start, 0);
        // the pregap's INDEX 00 is not where the track starts
        assert_eq!(tracks[1].start, 4 * 60 * 75);
        assert_eq!(tracks[1].performer.as_deref(), Some("A Guest"));
        assert_eq!(tracks[2].number, 3);
        assert_eq!(tracks[2].title, None);
        assert_eq!(tracks[2].start, (61 * 60 + 30) * 75 + 37);
    }

    #[test]
    fn skips_data_tracks_and_tracks_with_no_start() {
        let sheet = parse(
            b"FILE disc.bin BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00\n\
              TRACK 02 AUDIO\nTITLE Unplaced\nTRACK 03 AUDIO\nINDEX 01 02:00:00\n",
        );
        assert_eq!(sheet.files[0].name, "disc.bin");
        assert_eq!(sheet.files[0].tracks.len(), 1);
        assert_eq!(sheet.files[0].tracks[0].number, 3);
        // a title after a TRACK line never names the album
        assert_eq!(sheet.title, None);
    }

    #[test]
    fn reads_cue_times_and_refuses_what_is_not_one() {
        assert_eq!(frames("00:02:00"), Some(150));
        assert_eq!(frames("01:00:74"), Some(60 * 75 + 74));
        assert_eq!(frames("00:00:75"), None);
        assert_eq!(frames("00:60:00"), None);
        assert_eq!(frames("12:00"), None);
        assert_eq!(frames("a:b:c"), None);
    }

    #[test]
    fn cuts_the_file_into_the_sheets_tracks() {
        let path = Path::new("music").join("Live Album.flac");
        let sheet = parse(SHEET.as_bytes());
        let tracks = tracks(&sheet, &whole(&path, 70 * 60), &path);

        assert_eq!(tracks.len(), 3);
        let first = &tracks[0].track;
        assert_eq!(first.name, "Opening");
        assert_eq!(first.artists, "The Band");
        assert_eq!(first.album, "Live Album");
        assert_eq!(first.album_artist, "The Band");
        assert_eq!(first.track_number, 1);
        assert_eq!(first.duration, Duration::from_secs(4 * 60));
        assert_eq!(first.cover.as_deref(), Some("cover.jpg"));
        assert_eq!(tracks[0].year, Some(1997));

        let second = &tracks[1].track;
        assert_eq!(second.artists, "A Guest");
        assert_eq!(second.album_artist, "The Band");
        assert_eq!(second.album_id, first.album_id);

        // the last track has no title and runs to the end of the file
        let last = &tracks[2].track;
        assert_eq!(last.name, "Track 03");
        let start = (61 * 60 + 30) * 75 + 37;
        assert_eq!(last.duration, wire::cue_time(70 * 60 * 75 - start));
    }

    #[test]
    fn a_track_id_carries_the_file_and_the_window() {
        let path = Path::new("music").join("Live Album.flac");
        let sheet = parse(SHEET.as_bytes());
        let tracks = tracks(&sheet, &whole(&path, 70 * 60), &path);

        let id = tracks[1].track.id.as_deref().unwrap();
        assert_eq!(wire::path_from_track_id(id), Some(path.as_path()));
        let window = wire::window(id).unwrap();
        assert_eq!(window.skip, Duration::from_secs(4 * 60));
        let end = (61 * 60 + 30) * 75 + 37;
        assert_eq!(window.take, Some(wire::cue_time(end - 4 * 60 * 75)));

        let last = wire::window(tracks[2].track.id.as_deref().unwrap()).unwrap();
        assert_eq!(last.take, None);

        // a whole file has no window, even one whose name holds the mark
        let odd = wire::track_id(Path::new("a#cue=b.flac"));
        assert_eq!(wire::window(&odd), None);
        assert_eq!(
            wire::path_from_track_id(&odd),
            Some(Path::new("a#cue=b.flac"))
        );
    }

    #[test]
    fn a_sheet_of_several_files_or_past_the_end_cuts_nothing() {
        let path = Path::new("music").join("Live Album.flac");
        let several = parse(
            b"FILE \"01.wav\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n\
              FILE \"02.wav\" WAVE\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n",
        );
        assert!(tracks(&several, &whole(&path, 600), &path).is_empty());

        // a track that starts after the file ends is left out, and the one before it runs to
        // the end of the file
        let sheet = parse(SHEET.as_bytes());
        let short = tracks(&sheet, &whole(&path, 5 * 60), &path);
        assert_eq!(short.len(), 2);
        assert_eq!(short[1].track.duration, Duration::from_secs(60));
        let id = short[1].track.id.as_deref().unwrap();
        assert_eq!(wire::window(id).unwrap().take, None);
    }

    #[test]
    fn finds_the_file_a_sheet_describes() {
        let folder = Path::new("music");
        let parsed = vec![
            whole(&folder.join("Other.flac"), 60),
            whole(&folder.join("Live Album.flac"), 60),
            whole(&Path::new("elsewhere").join("Live Album.wav"), 60),
        ];
        // the sheet names a .wav, and the file beside it is the .flac it was packed into
        let sheet = parse(SHEET.as_bytes());
        assert_eq!(
            described(&sheet, &folder.join("anything.cue"), &parsed),
            Some(1)
        );

        // a sheet naming a file that is gone falls back on its own name
        let renamed = parse(b"FILE \"gone.wav\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n");
        assert_eq!(
            described(&renamed, &folder.join("Other.flac.cue"), &parsed),
            Some(0)
        );
        assert_eq!(
            described(&renamed, &folder.join("nothing.cue"), &parsed),
            None
        );
    }

    #[test]
    fn unquoted_and_quoted_values_read_alike() {
        assert_eq!(text("\"Quoted Title\"").as_deref(), Some("Quoted Title"));
        assert_eq!(text("Bare Title").as_deref(), Some("Bare Title"));
        assert_eq!(text("\"\""), None);
        assert_eq!(file_name("\"My Album.flac\" WAVE"), "My Album.flac");
        assert_eq!(file_name("album.flac WAVE"), "album.flac");
        assert_eq!(year("\"1997-05-01\""), Some(1997));
        assert_eq!(year("unknown"), None);
    }
}
