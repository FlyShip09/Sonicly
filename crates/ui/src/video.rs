//! Pictures and videos a page paints behind itself, decoded off the UI thread into images GPUI
//! paints directly. A picture is decoded once. A video is decoded by Media Foundation on a thread
//! of its own, which only runs while a page keeps asking for frames, so a video nobody draws costs
//! nothing but the memory of its last frame.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gpui::RenderImage;
use image::Frame;
#[cfg(target_os = "windows")]
use image::RgbaImage;

/// The largest a video frame is decoded at. A background under a scrim and a blur gains nothing
/// from more, and each frame is uploaded whole.
pub const VIDEO_MAX: (u32, u32) = (1280, 720);
/// The shortest gap between two frames handed to the page, which caps a video at 30 frames a
/// second whatever it was encoded at.
pub const VIDEO_GAP: Duration = Duration::from_micros(33_333);
/// How long a decoder keeps going after the page last asked for a frame. A page that draws the
/// video asks at least ten times a second, so this outlasts a late frame and stops soon after
/// the page is gone, minimized or paused.
const DEMAND: Duration = Duration::from_millis(400);
/// How many threads the system decoder may decode a background video on.
#[cfg(target_os = "windows")]
const DECODE_THREADS: u32 = 1;
/// How often the brightness of the playing frame is measured again.
#[cfg(target_os = "windows")]
const MEASURE: Duration = Duration::from_secs(1);
/// The relative luminance white lines may sit over and keep a contrast of 4.5 to 1.
const LEGIBLE: f32 = 0.183;
/// How many points across and down the brightness of a picture is sampled at.
const SAMPLES: (u32, u32) = (48, 27);
/// Which share of the sampled points the brightness is read at, counting from the darkest, so a
/// few bright points can raise it but one stray highlight does not.
const BRIGHT_SHARE: f32 = 0.95;

/// Why a video cannot be shown, which the page says in its own words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoProblem {
    /// This platform has no video decoder Sonicly uses.
    Unsupported,
    /// The file is missing or could not be opened.
    Unreadable,
    /// The file holds no video stream.
    NoVideo,
    /// No decoder installed on the system reads this kind of video.
    Undecodable,
}

/// A picture decoded for a background, with how bright its brighter parts are.
#[derive(Clone)]
pub struct Picture {
    pub image: Arc<RenderImage>,
    /// The relative luminance most of the picture stays under, from 0 to 1.
    pub luma: f32,
}

impl Picture {
    /// Decodes the picture at `path` no larger than `edge` on its longer side. Runs on the
    /// caller's thread, so a caller spawns it.
    pub fn load(path: &Path, edge: u32) -> image::ImageResult<Self> {
        let decoded = image::open(path)?;
        let fitted = match decoded.width().max(decoded.height()) > edge {
            true => decoded.thumbnail(edge, edge),
            false => decoded,
        };
        let mut pixels = fitted.into_rgba8();
        for pixel in pixels.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        let luma = picture_luma(
            pixels.as_raw(),
            pixels.width(),
            pixels.height(),
            pixels.width() * 4,
        );
        Ok(Self {
            image: Arc::new(RenderImage::new(vec![Frame::new(pixels)])),
            luma,
        })
    }
}

/// How bright the brighter part of a BGRA picture is, as relative luminance from 0 to 1, read
/// off a sparse grid so a whole video frame costs a few microseconds.
pub fn picture_luma(bgra: &[u8], width: u32, height: u32, stride: u32) -> f32 {
    if width == 0 || height == 0 {
        return 0.;
    }
    let (across, down) = SAMPLES;
    let mut levels = Vec::with_capacity((across * down) as usize);
    for row in 0..down {
        let y = (row * 2 + 1) * height / (down * 2);
        for column in 0..across {
            let x = (column * 2 + 1) * width / (across * 2);
            let at = (y * stride + x * 4) as usize;
            let Some(pixel) = bgra.get(at..at + 4) else {
                continue;
            };
            levels.push(
                0.2126 * linear(pixel[2]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[0]),
            );
        }
    }
    if levels.is_empty() {
        return 0.;
    }
    levels.sort_by(f32::total_cmp);
    levels[((levels.len() - 1) as f32 * BRIGHT_SHARE) as usize]
}

/// The least share of a picture a black scrim has to hide for white lines over it to keep a
/// contrast of 4.5 to 1, given how bright the picture is.
pub fn legible_dim(luma: f32) -> f32 {
    match luma > LEGIBLE {
        true => (1. - LEGIBLE / luma).clamp(0., 1.),
        false => 0.,
    }
}

/// A video looping on a thread of its own, muted, its frames decoded no larger than `VIDEO_MAX`
/// and no more often than `VIDEO_GAP`. Dropping it stops the thread, which releases the decoder
/// and every frame the thread holds.
///
/// The thread holds at most one frame it is filling and the page at most the one it shows and
/// the one before it until the atlas lets go of it, so frames take no more than three times
/// `VIDEO_MAX` at four bytes a pixel, about 11 MB, on top of what the system decoder keeps.
pub struct VideoFrames {
    shared: Arc<Shared>,
}

struct Shared {
    slot: Mutex<Slot>,
    wake: Condvar,
}

#[derive(Default)]
struct Slot {
    /// The newest frame and its number, counting up from one.
    frame: Option<(u64, Arc<RenderImage>)>,
    luma: f32,
    problem: Option<VideoProblem>,
    /// When the page last asked for frames.
    wanted: Option<Instant>,
    stop: bool,
}

impl VideoFrames {
    /// Starts decoding `path` on its own thread. A file that cannot be shown reports why
    /// through `problem` rather than failing here.
    pub fn open(path: PathBuf) -> Self {
        let shared = Arc::new(Shared {
            slot: Mutex::new(Slot::default()),
            wake: Condvar::new(),
        });
        let thread = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("video".to_owned())
            .spawn(move || {
                if let Err(problem) = decode(&path, &thread) {
                    log::warn!("video: cannot play {}: {problem:?}", path.display());
                    thread.with(|slot| slot.problem = Some(problem));
                }
            });
        if spawned.is_err() {
            shared.with(|slot| slot.problem = Some(VideoProblem::Unsupported));
        }
        Self { shared }
    }

    /// Keeps the decoder going for a little while longer. A page calls it from every frame it
    /// draws the video in, so the video stops on its own once the page stops drawing it.
    pub fn want(&self) {
        let woke = self.shared.with(|slot| {
            let idle = slot.wanted.is_none_or(|at| at.elapsed() > DEMAND);
            slot.wanted = Some(Instant::now());
            idle
        });
        if woke {
            self.shared.wake.notify_all();
        }
    }

    /// The newest frame and its number, which changes with every frame.
    pub fn latest(&self) -> Option<(u64, Arc<RenderImage>)> {
        self.shared.with(|slot| slot.frame.clone())
    }

    /// How bright the brighter part of the playing frame is, as `Picture::luma`.
    pub fn luma(&self) -> f32 {
        self.shared.with(|slot| slot.luma)
    }

    pub fn problem(&self) -> Option<VideoProblem> {
        self.shared.with(|slot| slot.problem)
    }
}

impl Drop for VideoFrames {
    fn drop(&mut self) {
        self.shared.with(|slot| slot.stop = true);
        self.shared.wake.notify_all();
    }
}

impl Shared {
    fn with<T>(&self, work: impl FnOnce(&mut Slot) -> T) -> T {
        let mut slot = self
            .slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&mut slot)
    }

    /// Waits until the page wants frames or the video stops. Returns false once it stops.
    #[cfg(target_os = "windows")]
    fn wait_for_demand(&self) -> bool {
        let mut slot = self
            .slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if slot.stop {
                return false;
            }
            if slot.wanted.is_some_and(|at| at.elapsed() <= DEMAND) {
                return true;
            }
            slot = self
                .wake
                .wait_timeout(slot, DEMAND)
                .map(|(slot, _)| slot)
                .unwrap_or_else(|poisoned| poisoned.into_inner().0);
        }
    }

    /// Hands the page a new frame, measuring its brightness when the last measure is old.
    #[cfg(target_os = "windows")]
    fn publish(&self, pixels: RgbaImage, measured: &mut Option<Instant>) {
        let luma = match measured.is_none_or(|at| at.elapsed() >= MEASURE) {
            true => {
                *measured = Some(Instant::now());
                Some(picture_luma(
                    pixels.as_raw(),
                    pixels.width(),
                    pixels.height(),
                    pixels.width() * 4,
                ))
            }
            false => None,
        };
        let image = Arc::new(RenderImage::new(vec![Frame::new(pixels)]));
        self.with(|slot| {
            let number = slot.frame.as_ref().map_or(1, |(number, _)| number + 1);
            slot.frame = Some((number, image));
            if let Some(luma) = luma {
                slot.luma = luma;
            }
        });
    }
}

/// How many times smaller a `width` by `height` frame is copied so it fits within `VIDEO_MAX`, a
/// whole number so each copied pixel averages a whole block.
#[cfg(any(target_os = "windows", test))]
fn thinning(width: u32, height: u32) -> u32 {
    let (most_wide, most_high) = VIDEO_MAX;
    width
        .div_ceil(most_wide)
        .max(height.div_ceil(most_high))
        .max(1)
}

fn linear(channel: u8) -> f32 {
    let value = channel as f32 / 255.;
    match value <= 0.04045 {
        true => value / 12.92,
        false => ((value + 0.055) / 1.055).powf(2.4),
    }
}

#[cfg(not(target_os = "windows"))]
fn decode(_path: &Path, _shared: &Shared) -> Result<(), VideoProblem> {
    Err(VideoProblem::Unsupported)
}

/// Decodes with Media Foundation's Source Reader: the video stream alone, so the file's sound is
/// never even decoded, converted to 32 bit BGR by the reader's own video processor and scaled by
/// it to `fitted` where it can, otherwise thinned out while copied. Paced by the frames' own
/// times, looped at the end.
#[cfg(target_os = "windows")]
fn decode(path: &Path, shared: &Shared) -> Result<(), VideoProblem> {
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows::core::{GUID, HSTRING, Interface as _};

    /// Undoes `MFStartup` and `CoInitializeEx` when the thread is done, whatever way it leaves.
    struct Started;
    impl Drop for Started {
        fn drop(&mut self) {
            unsafe {
                MFShutdown().ok();
                CoUninitialize();
            }
        }
    }

    const STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let unreadable = |_| VideoProblem::Unreadable;
    let undecodable = |_| VideoProblem::Undecodable;

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|_| VideoProblem::Unsupported)?;
        if MFStartup(MF_VERSION, MFSTARTUP_LITE).is_err() {
            CoUninitialize();
            return Err(VideoProblem::Unsupported);
        }
        let _started = Started;

        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1).map_err(|_| VideoProblem::Unsupported)?;
        let attributes = attributes.ok_or(VideoProblem::Unsupported)?;
        // The plain processor converts the decoder's YUV to 32 bit BGR and does nothing else. The
        // advanced one could scale too, but holds a full-size pool of its own, about 30 MB more
        // for a 1080p video, so a large frame is thinned while it is copied instead.
        attributes
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(|_| VideoProblem::Unsupported)?;
        // A frame comes out as soon as it is decoded, rather than after a queue of them.
        attributes.SetUINT32(&MF_LOW_LATENCY, 1).ok();
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attributes)
            .map_err(unreadable)?;
        reader
            .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
            .map_err(unreadable)?;
        reader
            .SetStreamSelection(STREAM, true)
            .map_err(|_| VideoProblem::NoVideo)?;
        reader
            .GetNativeMediaType(STREAM, 0)
            .map_err(|_| VideoProblem::NoVideo)?;

        // Opaque BGR first, which copies as it is, and plain 32 bit BGR otherwise, whose fourth
        // byte is set while copying.
        let output = MFCreateMediaType().map_err(|_| VideoProblem::Unsupported)?;
        output
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(undecodable)?;
        output
            .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32)
            .map_err(undecodable)?;
        if reader.SetCurrentMediaType(STREAM, None, &output).is_err() {
            output
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(undecodable)?;
            reader
                .SetCurrentMediaType(STREAM, None, &output)
                .map_err(undecodable)?;
        }
        let mut layout = Layout::of(&reader.GetCurrentMediaType(STREAM).map_err(undecodable)?)?;
        // The system decoder runs a thread with frames of its own per core unless told otherwise,
        // which for a 1080p background is most of a hundred megabytes doing nothing useful.
        if let Ok(extended) = reader.cast::<IMFSourceReaderEx>() {
            let mut decoder = None;
            if extended
                .GetTransformForStream(STREAM, 0, None, &mut decoder)
                .is_ok()
                && let Some(codec) = decoder.and_then(|decoder| decoder.cast::<ICodecAPI>().ok())
            {
                let threads = windows::Win32::System::Variant::VARIANT::from(DECODE_THREADS);
                codec
                    .SetValue(&CODECAPI_AVDecNumWorkerThreads, &threads)
                    .ok();
            }
        }

        let mut clock: Option<(Instant, Duration)> = None;
        let mut shown: Option<Duration> = None;
        let mut measured = None;
        while shared.wait_for_demand() {
            let mut flags = 0u32;
            let mut stamp = 0i64;
            let mut sample = None;
            reader
                .ReadSample(
                    STREAM,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut stamp),
                    Some(&mut sample),
                )
                .map_err(undecodable)?;
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                layout = Layout::of(&reader.GetCurrentMediaType(STREAM).map_err(undecodable)?)?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                let start = PROPVARIANT::from(0i64);
                reader
                    .SetCurrentPosition(&GUID::zeroed(), &start)
                    .map_err(undecodable)?;
                clock = None;
                shown = None;
                continue;
            }
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err(VideoProblem::Undecodable);
            }
            let Some(sample) = sample else {
                continue;
            };
            let at = Duration::from_nanos(stamp.max(0) as u64 * 100);
            if shown.is_some_and(|shown| at < shown + VIDEO_GAP) {
                continue;
            }
            // The clock starts at the first frame shown and restarts after a stall, such as the
            // page asking for nothing for a while, so a resumed video does not race to catch up.
            let (started, origin) = *clock.get_or_insert_with(|| (Instant::now(), at));
            let due = started + at.saturating_sub(origin);
            let now = Instant::now();
            match due.checked_duration_since(now) {
                Some(wait) if wait > Duration::from_millis(250) => clock = Some((now, at)),
                Some(wait) => std::thread::sleep(wait),
                None if now - due > Duration::from_millis(250) => clock = Some((now, at)),
                None => {}
            }
            shown = Some(at);

            let buffer = sample.ConvertToContiguousBuffer().map_err(undecodable)?;
            let pixels = match buffer.cast::<IMF2DBuffer>() {
                Ok(flat) => {
                    let mut line = std::ptr::null_mut();
                    let mut pitch = 0i32;
                    flat.Lock2D(&mut line, &mut pitch).map_err(undecodable)?;
                    let pixels = layout.copy(line, pitch as isize);
                    flat.Unlock2D().ok();
                    pixels
                }
                Err(_) => {
                    let mut data = std::ptr::null_mut();
                    buffer.Lock(&mut data, None, None).map_err(undecodable)?;
                    let pixels = layout.copy(data, layout.stride);
                    buffer.Unlock().ok();
                    pixels
                }
            };
            if let Some(pixels) = pixels {
                shared.publish(pixels, &mut measured);
            }
        }
        Ok(())
    }
}

/// The shape of the frames the reader hands over, and how they are thinned to fit `VIDEO_MAX`
/// when the reader could not scale them itself.
#[cfg(target_os = "windows")]
struct Layout {
    width: u32,
    height: u32,
    /// Bytes from one row to the next in a contiguous buffer. Negative for a bottom-up frame.
    stride: isize,
    /// How many source pixels each copied pixel stands for, across and down.
    step: u32,
    /// Whether the reader hands over opaque pixels, which need no fourth byte set.
    opaque: bool,
}

#[cfg(target_os = "windows")]
impl Layout {
    unsafe fn of(
        media: &windows::Win32::Media::MediaFoundation::IMFMediaType,
    ) -> Result<Self, VideoProblem> {
        use windows::Win32::Media::MediaFoundation::{
            MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_SUBTYPE, MFVideoFormat_ARGB32,
        };

        let (width, height) = unpack(
            unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) }.map_err(|_| VideoProblem::Undecodable)?,
        );
        let stride = unsafe { media.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
            .map(|stride| stride as i32 as isize)
            .unwrap_or(width as isize * 4);
        let opaque = unsafe { media.GetGUID(&MF_MT_SUBTYPE) }
            .is_ok_and(|subtype| subtype == MFVideoFormat_ARGB32);
        Ok(Self {
            width,
            height,
            stride,
            step: thinning(width, height),
            opaque,
        })
    }

    /// Copies one frame out of the decoder's buffer, which is the one copy a frame takes on its
    /// way to the screen. A frame larger than `VIDEO_MAX` is shrunk `step` times on the way, each
    /// pixel the average of the block it stands for, and every pixel is made opaque, since 32 bit
    /// BGR leaves its fourth byte undefined. A frame of the wrong size gives nothing.
    unsafe fn copy(&self, first_row: *mut u8, pitch: isize) -> Option<RgbaImage> {
        if first_row.is_null() || self.width == 0 || self.height == 0 {
            return None;
        }
        let (wide, high) = (self.width / self.step, self.height / self.step);
        let mut pixels = vec![0u8; (wide * high * 4) as usize];
        for row in 0..high {
            let source = unsafe { first_row.offset((row * self.step) as isize * pitch) };
            let target = &mut pixels[(row * wide * 4) as usize..((row + 1) * wide * 4) as usize];
            match self.step {
                1 => {
                    let line = unsafe { std::slice::from_raw_parts(source, (wide * 4) as usize) };
                    target.copy_from_slice(line);
                    if !self.opaque {
                        for pixel in target.as_chunks_mut::<4>().0 {
                            pixel[3] = u8::MAX;
                        }
                    }
                }
                step => {
                    let block = step as usize;
                    let area = step * step;
                    for (column, pixel) in target.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                        let mut sum = [0u32; 3];
                        for down in 0..block {
                            let line = unsafe {
                                source.offset(down as isize * pitch).add(column * block * 4)
                            };
                            let texels = unsafe { std::slice::from_raw_parts(line, block * 4) };
                            for texel in texels.as_chunks::<4>().0 {
                                sum[0] += u32::from(texel[0]);
                                sum[1] += u32::from(texel[1]);
                                sum[2] += u32::from(texel[2]);
                            }
                        }
                        *pixel = [
                            (sum[0] / area) as u8,
                            (sum[1] / area) as u8,
                            (sum[2] / area) as u8,
                            u8::MAX,
                        ];
                    }
                }
            }
        }
        RgbaImage::from_raw(wide, high, pixels)
    }
}

#[cfg(target_os = "windows")]
fn unpack(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_thinned_to_fit_the_cap() {
        assert_eq!(thinning(1280, 720), 1);
        assert_eq!(thinning(640, 360), 1);
        assert_eq!(thinning(1920, 1080), 2);
        assert_eq!(thinning(3840, 2160), 3);
        let step = thinning(2560, 1440);
        assert!(2560 / step <= VIDEO_MAX.0 && 1440 / step <= VIDEO_MAX.1);
    }

    #[test]
    fn a_bright_picture_asks_for_a_heavier_scrim() {
        let white = vec![u8::MAX; 64 * 64 * 4];
        let black = vec![0u8; 64 * 64 * 4];
        assert!(picture_luma(&white, 64, 64, 256) > 0.99);
        assert_eq!(picture_luma(&black, 64, 64, 256), 0.);
        assert!(legible_dim(1.) > 0.8);
        assert_eq!(legible_dim(0.1), 0.);
    }
}
