//! Covers for what has none of its own. A playlist without art gets a mosaic image of its first
//! four covers, built once and kept on disk. Favorites, Local Music and Recents change with
//! every like, scan and play, so theirs are drawn instead (`ui::GeneratedCover`) from the
//! colours of their first covers, which [`Swatches`] samples and keeps.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use futures::AsyncReadExt as _;
use gpui::http_client::{AsyncBody, HttpClient};
use gpui::{App, AppContext as _, Context, Entity, Global, Hsla, Task};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, RgbaImage};
use music::Shape;
use tokio::sync::Semaphore;
use ui::CoverPalette;

use crate::{Shelf, Sonicly};

pub(crate) const TILES: usize = 4;
/// How many covers a generated cover takes its colours from: one for the field and one for each
/// band over its foot.
pub const SWATCHES: usize = 4;

const TILE: u32 = 256;
const SIDE: u32 = TILE * 2;
const COLUMNS: u32 = 2;

/// Lets one mosaic decode or compose at a time. A library of playlists without art asks for
/// all of them at once, and each decode briefly holds a full-size cover. Fetches wait outside
/// it, so one stalled download does not hold up every other playlist.
static BUILDS: Semaphore = Semaphore::const_new(1);

fn path(id: &str, stamp: u32) -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(storage::folder())
        .join("mosaics")
        .join(format!("{}-{stamp}.png", sanitised(id)))
}

fn sanitised(id: &str) -> String {
    id.chars()
        .map(|letter| match letter.is_ascii_alphanumeric() {
            true => letter,
            false => '_',
        })
        .collect()
}

fn located(path: &Path) -> String {
    format!("file://{}", path.display())
}

pub(crate) fn cached(id: &str, stamp: u32) -> Option<String> {
    let path = path(id, stamp);
    path.is_file().then(|| located(&path))
}

pub(crate) async fn build(
    http: Arc<dyn HttpClient>,
    id: &str,
    stamp: u32,
    covers: Vec<String>,
) -> Result<String> {
    if covers.len() < TILES {
        bail!("cannot build a mosaic from {} covers", covers.len());
    }

    let mut tiles = Vec::with_capacity(TILES);
    for url in covers.iter().take(TILES) {
        let bytes = fetch(&http, url).await?;
        let _turn = BUILDS.acquire().await.context("cannot queue a mosaic")?;
        tiles.push(tokio::task::spawn_blocking(move || tile(&bytes)).await??);
    }

    let path = path(id, stamp);
    tokio::task::spawn_blocking(move || {
        let canvas = compose(&tiles);
        let parent = path.parent().context("cannot place the mosaic")?;
        fs::create_dir_all(parent).context("cannot create the mosaic cache")?;
        canvas
            .save_with_format(&path, ImageFormat::Png)
            .context("cannot write the mosaic")?;
        forget_older(&path);

        Ok(located(&path))
    })
    .await?
}

/// Decodes a cover and cuts it down to one tile, so the full-size frame is gone before the
/// next cover is fetched.
fn tile(bytes: &[u8]) -> Result<DynamicImage> {
    let cover = image::load_from_memory(bytes).context("cannot decode a cover")?;

    Ok(cover.resize_to_fill(TILE, TILE, FilterType::Triangle))
}

fn compose(tiles: &[DynamicImage]) -> RgbaImage {
    let mut canvas = RgbaImage::new(SIDE, SIDE);
    for (index, tile) in tiles.iter().take(TILES).enumerate() {
        let slot = index as u32;
        let tile = tile
            .resize_to_fill(TILE, TILE, FilterType::Triangle)
            .to_rgba8();
        image::imageops::replace(
            &mut canvas,
            &tile,
            i64::from(slot % COLUMNS * TILE),
            i64::from(slot / COLUMNS * TILE),
        );
    }

    canvas
}

async fn fetch(http: &Arc<dyn HttpClient>, url: &str) -> Result<Vec<u8>> {
    let mut response = http
        .get(url, AsyncBody::empty(), true)
        .await
        .context("cannot fetch a cover")?;
    if !response.status().is_success() {
        bail!("a cover request answered {}", response.status());
    }

    let mut bytes = Vec::new();
    response
        .body_mut()
        .read_to_end(&mut bytes)
        .await
        .context("cannot read a cover")?;

    Ok(bytes)
}

/// Removes the mosaics older stamps left for the playlist whose current mosaic is `kept`. A
/// sanitised id holds no `-`, so the text before the last one names the playlist.
fn forget_older(kept: &Path) {
    let playlist = |path: &Path| {
        let name = path.file_name()?.to_str()?;
        Some(name.rsplit_once('-')?.0.to_owned())
    };
    let (Some(folder), Some(id)) = (kept.parent(), playlist(kept)) else {
        return;
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path != kept && playlist(&path).as_deref() == Some(id.as_str()) {
            fs::remove_file(path).ok();
        }
    }
}

/// A collection with no artwork of its own, whose cover is drawn from the colours of the covers
/// in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Generated {
    Favorites,
    Local,
    Recents,
}

impl Generated {
    pub const ALL: [Self; 3] = [Self::Favorites, Self::Local, Self::Recents];

    /// The glyph on the cover, the one the collection's page already stands for it with.
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Favorites => "icons/heart-filled.svg",
            Self::Local => "icons/file-music.svg",
            Self::Recents => "icons/rotate-ccw-clock.svg",
        }
    }

    /// The Fluent key of the collection's name.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Favorites => "library-liked-songs",
            Self::Local => "nav-local",
            Self::Recents => "nav-history",
        }
    }
}

/// The colours every generated cover is drawn in. Each collection's first covers are sampled
/// once, from the palette the artwork cache keeps or a small decode, and sampled again only when
/// those covers change, so a play or a like that leaves the front of the list alone costs nothing.
pub struct Swatches {
    held: HashMap<Generated, Sampled>,
}

/// The covers a collection's colours were sampled from, and the colours, lead first.
#[derive(Default)]
struct Sampled {
    covers: Vec<String>,
    colors: Vec<Hsla>,
    sampling: Option<Task<()>>,
}

struct Installed(Entity<Swatches>);

impl Global for Installed {}

impl Swatches {
    /// The one `Swatches`, made the first time it is asked for. It follows the library and the
    /// history from then on.
    pub fn entity(cx: &mut App) -> Entity<Self> {
        if let Some(installed) = cx.try_global::<Installed>() {
            return installed.0.clone();
        }
        let swatches = cx.new(|cx: &mut Context<Self>| {
            let (library, history) = {
                let sonicly = Sonicly::global(cx);
                (sonicly.library.clone(), sonicly.history.clone())
            };
            cx.observe(&library, |this, _, cx| this.refresh(cx))
                .detach();
            cx.observe(&history, |this, _, cx| this.refresh(cx))
                .detach();
            let mut swatches = Self {
                held: HashMap::new(),
            };
            swatches.refresh(cx);
            swatches
        });
        cx.set_global(Installed(swatches.clone()));
        swatches
    }

    /// The colours of `collection`'s first covers, lead first. Empty while it has no covers or
    /// while they are still being sampled, and before anything asked for [`Swatches::entity`].
    pub fn colors(collection: Generated, cx: &App) -> &[Hsla] {
        cx.try_global::<Installed>().map_or(&[], |installed| {
            installed
                .0
                .read(cx)
                .held
                .get(&collection)
                .map_or(&[], |sampled| sampled.colors.as_slice())
        })
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        for collection in Generated::ALL {
            let covers = covers(collection, cx);
            self.follow(collection, covers, cx);
        }
    }

    /// Samples `covers` for `collection` unless they are the ones its colours came from. The
    /// last colours stay until the new ones land, so a cover never flashes neutral in between.
    fn follow(&mut self, collection: Generated, covers: Vec<String>, cx: &mut Context<Self>) {
        let held = self.held.entry(collection).or_default();
        if held.covers == covers {
            return;
        }
        held.covers.clone_from(&covers);
        if covers.is_empty() {
            held.colors.clear();
            held.sampling = None;
            cx.notify();
            return;
        }
        let palettes: Vec<_> = covers
            .into_iter()
            .map(|cover| ui::palette(cover, cx))
            .collect();
        let task = cx.spawn(async move |this, cx| {
            let mut colors = Vec::with_capacity(palettes.len());
            for palette in palettes {
                colors.push(lead(palette.await));
            }
            this.update(cx, |this, cx| {
                if let Some(held) = this.held.get_mut(&collection) {
                    held.colors = colors;
                    held.sampling = None;
                }
                cx.notify();
            })
            .ok();
        });
        if let Some(held) = self.held.get_mut(&collection) {
            held.sampling = Some(task);
        }
    }
}

/// The first distinct covers of `collection`, in the order its page lists them. Favorites are the
/// streaming shelf only where that shelf is what the listener saved; a catalog has none.
fn covers(collection: Generated, cx: &App) -> Vec<String> {
    let sonicly = Sonicly::global(cx);
    let library = sonicly.library.read(cx);
    let tracks = match collection {
        Generated::Favorites => match library.shape(Shelf::Streaming) {
            Shape::Saved => library.state(Shelf::Streaming).tracks(),
            Shape::Catalog(_) => &[],
        },
        Generated::Local => library.state(Shelf::Local).tracks(),
        Generated::Recents => sonicly.history.read(cx).tracks(),
    };
    music::distinct_covers(tracks, SWATCHES)
}

/// The colour a cover stands for: its leading hue, or the grey of its mean lightness for art
/// with none, so a black sleeve and a white one still draw apart.
fn lead(palette: CoverPalette) -> Hsla {
    palette.primary.unwrap_or(Hsla {
        h: 0.,
        s: 0.,
        l: palette.lightness,
        a: 1.,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_without_a_hue_stands_for_its_lightness() {
        let white = lead(CoverPalette {
            primary: None,
            secondary: None,
            lightness: 0.9,
        });

        assert_eq!((white.s, white.l), (0., 0.9));
    }

    #[test]
    fn a_path_never_escapes_the_cache_directory() {
        let built = path("../../etc/passwd", 4);

        assert_eq!(
            built.file_name().and_then(|name| name.to_str()),
            Some("______etc_passwd-4.png")
        );
    }

    fn solid(red: u8, green: u8, blue: u8) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            600,
            600,
            image::Rgba([red, green, blue, 255]),
        ))
    }

    fn quarters() -> RgbaImage {
        compose(&[
            solid(255, 0, 0),
            solid(0, 255, 0),
            solid(0, 0, 255),
            solid(255, 255, 0),
        ])
    }

    #[test]
    fn every_pixel_of_a_mosaic_is_opaque() {
        let canvas = quarters();

        assert!(canvas.pixels().all(|pixel| pixel.0[3] == 255));
    }

    #[test]
    fn the_seam_leaves_no_gap_between_tiles() {
        let canvas = quarters();
        let last = TILE - 1;

        assert_eq!(canvas.get_pixel(last, 0).0, [255, 0, 0, 255]);
        assert_eq!(canvas.get_pixel(TILE, 0).0, [0, 255, 0, 255]);
        assert_eq!(canvas.get_pixel(0, last).0, [255, 0, 0, 255]);
        assert_eq!(canvas.get_pixel(0, TILE).0, [0, 0, 255, 255]);
    }

    #[test]
    fn each_tile_fills_its_own_quarter() {
        let canvas = quarters();
        let mid = TILE / 2;

        assert_eq!(canvas.get_pixel(mid, mid).0, [255, 0, 0, 255]);
        assert_eq!(canvas.get_pixel(TILE + mid, mid).0, [0, 255, 0, 255]);
        assert_eq!(canvas.get_pixel(mid, TILE + mid).0, [0, 0, 255, 255]);
        assert_eq!(
            canvas.get_pixel(TILE + mid, TILE + mid).0,
            [255, 255, 0, 255]
        );
    }
}
