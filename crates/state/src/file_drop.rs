//! Files dragged onto the window from the system. While they hover, the audio files they hold
//! are counted off the main thread; on the drop they join Local Music or a local playlist. Files
//! already inside a music folder only need a scan. Files from elsewhere either have their folder
//! join the music folders or are copied into one, as the listener answers once. The originals
//! are never moved, renamed or deleted, and a copy never overwrites a file.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui::{App, AppContext as _, Context, Entity, Global, Task};
use music::MusicProvider;

use crate::{Addition, Outcome, Placement, Sonicly, Toast, Toasts};

/// The most songs one drop counts and adds, so a drive dropped by mistake stops somewhere.
const MOST: usize = 50_000;

/// Where a drop lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// Local Music: the files join the library.
    Library,
    /// A local playlist, by id and name. The files join the library too, since a local
    /// playlist only lists songs the library holds.
    Playlist { id: String, name: String },
}

/// What the files being dragged hold.
#[derive(Clone, Debug)]
pub struct Survey {
    /// The files and folders as the system handed them over.
    pub paths: Vec<PathBuf>,
    /// The audio files among them and inside the folders, `None` while they are being counted.
    pub songs: Option<Vec<PathBuf>>,
}

impl Survey {
    /// How many songs the drag holds, once counted.
    pub fn count(&self) -> Option<usize> {
        self.songs.as_ref().map(Vec::len)
    }
}

/// A drop that waits for the listener to say where songs from outside the music folders go.
#[derive(Clone, Debug)]
pub struct Ask {
    /// How many of the dropped songs lie outside the music folders.
    pub outside: usize,
    /// The folders that adding the songs' folder would add.
    pub folders: Vec<PathBuf>,
    /// The music folders a copy can go into.
    pub music_folders: Vec<PathBuf>,
    dropped: Dropped,
}

/// The songs of one drop, split by whether a music folder already holds them.
#[derive(Clone, Debug)]
struct Dropped {
    target: DropTarget,
    /// The files and folders as dropped, which a copy keeps the shape of.
    roots: Vec<PathBuf>,
    inside: Vec<PathBuf>,
    outside: Vec<PathBuf>,
}

/// Follows a drag of files from the system and carries out its drop.
pub struct FileDrop {
    survey: Option<Survey>,
    counting: Option<Task<()>>,
    asking: Option<Ask>,
    landing: Option<Task<()>>,
    /// Whether a debug launch asked for the drag-over state without a drag.
    previewing: bool,
}

struct Installed(Entity<FileDrop>);

impl Global for Installed {}

impl FileDrop {
    /// The one `FileDrop`, made the first time it is asked for.
    pub fn entity(cx: &mut App) -> Entity<Self> {
        if let Some(installed) = cx.try_global::<Installed>() {
            return installed.0.clone();
        }
        let entity = cx.new(|_| Self {
            survey: None,
            counting: None,
            asking: None,
            landing: None,
            previewing: false,
        });
        cx.set_global(Installed(entity.clone()));
        entity
    }

    /// What the files being dragged hold, while a drag is over the window.
    pub fn survey(&self) -> Option<&Survey> {
        self.survey.as_ref()
    }

    /// Whether the drag-over state shows without a drag, for a capture.
    pub fn previewing(&self) -> bool {
        self.previewing
    }

    /// The question a drop waits on, if one does.
    pub fn asking(&self) -> Option<&Ask> {
        self.asking.as_ref()
    }

    /// Follows a drag of `paths` over the window, counting what they hold the first time they
    /// come by.
    pub fn hover(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        if self
            .survey
            .as_ref()
            .is_some_and(|survey| survey.paths == paths)
        {
            return;
        }
        self.survey = Some(Survey {
            paths: paths.to_vec(),
            songs: None,
        });
        let plays = local(cx);
        let walked = paths.to_vec();
        self.counting = Some(cx.spawn(async move |this, cx| {
            let songs = cx
                .background_spawn(async move { songs(&walked, plays.as_ref()) })
                .await;
            this.update(cx, |this, cx| {
                log::debug!("drop: the drag holds {} songs", songs.len());
                if let Some(survey) = this.survey.as_mut() {
                    survey.songs = Some(songs);
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Forgets the drag once it has left the window or ended.
    pub fn leave(&mut self, cx: &mut Context<Self>) {
        if self.previewing || self.survey.is_none() {
            return;
        }
        self.survey = None;
        self.counting = None;
        cx.notify();
    }

    /// Shows the drag-over state for `paths` with no drag, for a capture.
    pub fn preview(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.previewing = true;
        self.hover(&paths, cx);
    }

    /// Adds the songs `paths` hold to `target`. Songs outside the music folders go where the
    /// listener said they should, and the drop waits on the question when they never said.
    pub fn drop_on(&mut self, paths: Vec<PathBuf>, target: DropTarget, cx: &mut Context<Self>) {
        self.survey = None;
        self.counting = None;
        cx.notify();
        let plays = local(cx);
        let folders = Sonicly::global(cx)
            .settings
            .read(cx)
            .local_folders()
            .to_vec();
        self.landing = Some(cx.spawn(async move |this, cx| {
            let dropped = cx
                .background_spawn(async move {
                    let found = songs(&paths, plays.as_ref());
                    let (inside, outside) = split(found, &folders);
                    Dropped {
                        target,
                        roots: paths,
                        inside,
                        outside,
                    }
                })
                .await;
            this.update(cx, |this, cx| this.land(dropped, cx)).ok();
        }));
    }

    /// Keeps `placement` as the answer for every later drop and carries out the one waiting.
    pub fn answer(&mut self, placement: Placement, cx: &mut Context<Self>) {
        let Some(ask) = self.asking.take() else {
            return;
        };
        let settings = Sonicly::global(cx).settings.clone();
        let kept = placement.clone();
        settings.update(cx, |settings, cx| {
            settings.set_dropped_files(Some(kept), cx)
        });
        self.place(ask.dropped, placement, cx);
        cx.notify();
    }

    /// Drops the question and the drop it held, adding nothing.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.asking.take().is_some() {
            cx.notify();
        }
    }

    /// Takes a counted drop the rest of the way: nothing to add is said so, songs from outside
    /// wait on the question unless it was answered before, and the rest go in.
    fn land(&mut self, dropped: Dropped, cx: &mut Context<Self>) {
        if dropped.inside.is_empty() && dropped.outside.is_empty() {
            Toasts::show(Outcome::Warning, "toast-drop-nothing", cx);
            return;
        }
        if dropped.outside.is_empty() {
            if dropped.target == DropTarget::Library {
                Toasts::post(
                    Toast::new(Outcome::Info, "toast-drop-adding").count(dropped.inside.len()),
                    cx,
                );
            }
            return self.finish(dropped.inside, &dropped.target, cx);
        }
        let settings = Sonicly::global(cx).settings.read(cx);
        let music_folders = settings.local_folders().to_vec();
        let remembered = settings.dropped_files().cloned().filter(|placement| {
            // a copy into a folder that has since left the list asks again
            !matches!(placement, Placement::Copy(folder) if !music_folders.contains(folder))
        });
        match (remembered, music_folders.is_empty()) {
            (Some(placement), _) => self.place(dropped, placement, cx),
            // With no music folder there is nothing to copy into, so there is nothing to ask.
            (None, true) => self.place(dropped, Placement::Folder, cx),
            (None, false) => {
                self.asking = Some(Ask {
                    outside: dropped.outside.len(),
                    folders: own_folders(&dropped.roots, &dropped.outside),
                    music_folders,
                    dropped,
                });
                cx.notify();
            }
        }
    }

    /// Puts the songs from outside where `placement` says, then adds every song of the drop.
    fn place(&mut self, dropped: Dropped, placement: Placement, cx: &mut Context<Self>) {
        match placement {
            Placement::Folder => {
                let folders = own_folders(&dropped.roots, &dropped.outside);
                let library = Sonicly::global(cx).library.clone();
                library.update(cx, |library, cx| library.add_local_folders(folders, cx));
                let mut songs = dropped.inside;
                songs.extend(dropped.outside);
                self.add_to_playlist(songs, &dropped.target, cx);
            }
            Placement::Copy(folder) => {
                let name = folder_name(&folder);
                let (roots, outside) = (dropped.roots.clone(), dropped.outside.clone());
                let into = folder.clone();
                self.landing = Some(cx.spawn(async move |this, cx| {
                    let copied = cx
                        .background_spawn(async move { copy_into(&roots, &outside, &into) })
                        .await;
                    this.update(cx, |this, cx| {
                        let (copies, failed) = copied;
                        if let Some(error) = failed {
                            log::warn!("drop: cannot copy into {}: {error:#}", folder.display());
                            Toasts::post(
                                Toast::new(Outcome::Failed, "toast-drop-copy-failed")
                                    .name(name.clone())
                                    .detail(format!("{error:#}")),
                                cx,
                            );
                        }
                        if !copies.is_empty() {
                            Toasts::post(
                                Toast::new(Outcome::Done, "toast-drop-copied")
                                    .name(name)
                                    .count(copies.len()),
                                cx,
                            );
                        }
                        let mut songs = dropped.inside;
                        songs.extend(copies);
                        this.finish(songs, &dropped.target, cx);
                    })
                    .ok();
                }));
            }
        }
    }

    /// Reads the music folders again so `songs`, all inside them now, show up, and adds them to
    /// the playlist when the drop was on one.
    fn finish(&mut self, songs: Vec<PathBuf>, target: &DropTarget, cx: &mut Context<Self>) {
        let library = Sonicly::global(cx).library.clone();
        library.update(cx, |library, cx| library.rescan_local(false, cx));
        self.add_to_playlist(songs, target, cx);
    }

    /// Adds `songs` to the playlist the drop was on, in the order they were found. Reading each
    /// file's id goes through the local provider, off the main thread.
    fn add_to_playlist(
        &mut self,
        songs: Vec<PathBuf>,
        target: &DropTarget,
        cx: &mut Context<Self>,
    ) {
        let DropTarget::Playlist { id, .. } = target.clone() else {
            return;
        };
        let provider = local(cx);
        cx.spawn(async move |_, cx| {
            let ids: Vec<String> = cx
                .background_spawn(async move {
                    songs
                        .iter()
                        .filter_map(|song| provider.track_from_path(song)?.id)
                        .collect()
                })
                .await;
            cx.update(|cx| {
                let library = Sonicly::global(cx).library.clone();
                library.update(cx, |library, cx| {
                    library.add_tracks_to_playlist(id, Addition::Tracks(ids), cx)
                });
            });
        })
        .detach();
    }
}

/// The local provider, which knows which files are audio and reads one as a track.
fn local(cx: &App) -> Arc<dyn MusicProvider> {
    Sonicly::global(cx).session.read(cx).local_provider()
}

/// Every audio file in `paths`, folders walked depth first in name order. Links to folders are
/// not followed, so a link back up the tree cannot loop.
fn songs(paths: &[PathBuf], provider: &dyn MusicProvider) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = paths.iter().rev().cloned().collect();
    while let Some(path) = stack.pop() {
        if found.len() >= MOST {
            break;
        }
        let Ok(kind) = fs::symlink_metadata(&path).map(|meta| meta.file_type()) else {
            continue;
        };
        if kind.is_dir() {
            let Ok(entries) = fs::read_dir(&path) else {
                continue;
            };
            let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
            children.sort();
            stack.extend(children.into_iter().rev());
        } else if provider.plays_file(&path) && path.is_file() {
            found.push(path);
        }
    }
    found
}

/// Splits `songs` into those a music folder holds and the rest. Both sides are compared as the
/// system resolves them, so a different spelling of one folder still counts as inside it.
fn split(songs: Vec<PathBuf>, folders: &[PathBuf]) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let folders: Vec<PathBuf> = folders.iter().map(|folder| resolved(folder)).collect();
    songs.into_iter().partition(|song| {
        let song = resolved(song);
        folders.iter().any(|folder| song.starts_with(folder))
    })
}

fn resolved(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// The folders adding `outside` as they are would add: each dropped folder that holds one of
/// them, and the folder of each song dropped on its own. A folder inside another one listed is
/// left out, since the outer one reads it, and so is a whole drive, which no drop should add.
fn own_folders(roots: &[PathBuf], outside: &[PathBuf]) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    for song in outside {
        let folder = roots
            .iter()
            .find(|root| root.is_dir() && song.starts_with(root))
            .cloned()
            .or_else(|| song.parent().map(Path::to_path_buf));
        if let Some(folder) = folder
            && folder.parent().is_some()
            && !folders.contains(&folder)
        {
            folders.push(folder);
        }
    }
    let all = folders.clone();
    folders.retain(|folder| {
        !all.iter()
            .any(|other| other != folder && folder.starts_with(other))
    });
    folders
}

/// Copies `songs` into `folder`, keeping the shape of a dropped folder under its own name and
/// putting a song dropped on its own straight in. A file already there with the same size is
/// taken as the same song and left alone; any other name clash gets a number. Returns the
/// copies, and the first failure when one or more songs could not be copied.
fn copy_into(
    roots: &[PathBuf],
    songs: &[PathBuf],
    folder: &Path,
) -> (Vec<PathBuf>, Option<anyhow::Error>) {
    let mut copies = Vec::with_capacity(songs.len());
    let mut failed = None;
    let mut taken: HashSet<PathBuf> = HashSet::new();
    for song in songs {
        let relative = roots
            .iter()
            .find(|root| root.is_dir() && song.starts_with(root))
            .and_then(|root| {
                let inner = song.strip_prefix(root).ok()?;
                Some(Path::new(root.file_name()?).join(inner))
            })
            .or_else(|| song.file_name().map(PathBuf::from));
        let Some(relative) = relative else {
            continue;
        };
        match copy_one(song, &folder.join(relative), &mut taken) {
            Ok(copy) => copies.push(copy),
            Err(error) => {
                failed.get_or_insert(error);
            }
        }
    }
    (copies, failed)
}

/// Copies `song` to `wanted`, or beside it under a free name. Never overwrites.
fn copy_one(song: &Path, wanted: &Path, taken: &mut HashSet<PathBuf>) -> Result<PathBuf> {
    let parent = wanted.parent().context("cannot place the copy")?;
    fs::create_dir_all(parent).context("cannot create the folder for the copy")?;
    let size = fs::metadata(song).context("cannot read the song")?.len();
    let mut target = wanted.to_path_buf();
    let mut number = 2;
    loop {
        match fs::metadata(&target) {
            Ok(existing) if existing.len() == size && !taken.contains(&target) => {
                taken.insert(target.clone());
                return Ok(target);
            }
            Ok(_) => {
                target = numbered(wanted, number);
                number += 1;
            }
            Err(_) => break,
        }
    }
    // `create_new` refuses a file that appeared since the check, so nothing is ever overwritten.
    let mut source = fs::File::open(song).context("cannot open the song")?;
    let mut copy = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .context("cannot create the copy")?;
    std::io::copy(&mut source, &mut copy).context("cannot copy the song")?;
    taken.insert(target.clone());
    Ok(target)
}

/// `name (n).ext` beside `path`.
fn numbered(path: &Path, number: usize) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(extension) => format!("{stem} ({number}).{}", extension.to_string_lossy()),
        None => format!("{stem} ({number})"),
    };
    path.with_file_name(name)
}

/// The last part of a folder's path, the way the toasts name it.
fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_song_dropped_alone_brings_its_own_folder_and_a_nested_folder_folds_in() {
        let roots = [
            PathBuf::from("/drop/album"),
            PathBuf::from("/loose/one.mp3"),
        ];
        let outside = [
            PathBuf::from("/drop/album/cd1/a.flac"),
            PathBuf::from("/drop/album/b.flac"),
            PathBuf::from("/loose/one.mp3"),
        ];

        // `is_dir` is false for paths that do not exist, so every song counts as dropped alone.
        assert_eq!(
            own_folders(&roots, &outside),
            vec![PathBuf::from("/drop/album"), PathBuf::from("/loose")]
        );
    }

    #[test]
    fn a_clash_gets_a_number_before_the_extension() {
        assert_eq!(
            numbered(Path::new("/music/song.flac"), 2),
            PathBuf::from("/music/song (2).flac")
        );
    }

    #[test]
    fn a_copy_never_overwrites_a_different_file() {
        let dir = std::env::temp_dir().join(format!("sonicly-drop-{}", std::process::id()));
        let source = dir.join("source").join("song.flac");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"new song").unwrap();
        let wanted = dir.join("music").join("song.flac");
        fs::create_dir_all(wanted.parent().unwrap()).unwrap();
        fs::write(&wanted, b"another song entirely").unwrap();

        let copy = copy_one(&source, &wanted, &mut HashSet::new()).unwrap();

        assert_eq!(copy, dir.join("music").join("song (2).flac"));
        assert_eq!(fs::read(&wanted).unwrap(), b"another song entirely");
        assert_eq!(fs::read(&source).unwrap(), b"new song");
        fs::remove_dir_all(&dir).ok();
    }
}
