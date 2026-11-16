//! How the lyrics page looks: what lies behind the lines, the face they are set in, and how the
//! line being sung stands out. Kept in `settings.json` under `lyrics_style`, with named presets
//! beside it. Every field has a default, so a file written before a field existed still loads.

use std::path::PathBuf;

use gpui::Context;
use serde::{Deserialize, Serialize};

use super::{AppSettings, SYSTEM_FONT};

/// The least share of the picture the scrim hides. Below it a busy picture eats the lines whatever
/// its brightness says, so the dim slider stops here.
pub const MIN_DIM: f32 = 0.2;
pub const MAX_DIM: f32 = 0.85;
pub const MIN_SPACING: f32 = 0.8;
pub const MAX_SPACING: f32 = 1.6;
/// The faintest the lines around the sung one may go, as a share of how they are drawn by default.
pub const MIN_INACTIVE: f32 = 0.3;
/// The weights a lyrics face may be set in, lightest first.
pub const LYRICS_WEIGHTS: [u16; 5] = [400, 500, 600, 700, 800];
/// How many presets the list keeps, so a hand-edited file cannot grow it without end.
const MOST_PRESETS: usize = 24;
/// How long a preset name may be, in characters.
const LONGEST_NAME: usize = 40;

/// What the lyrics page paints behind its lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsBackground {
    /// The slow waves sampled from the cover, the ambient field the rest of Now playing wears.
    #[default]
    Waves,
    /// A softer flow of the cover's colours that swells a little with the music.
    Fluid,
    /// The cover itself, blurred and dimmed.
    Cover,
    /// A picture the listener chose.
    Image,
    /// A video the listener chose, looped and muted. Windows only.
    Video,
}

impl LyricsBackground {
    /// Every background this build can paint, in the order the settings offer them.
    pub fn offered() -> impl Iterator<Item = Self> {
        [
            Self::Waves,
            Self::Fluid,
            Self::Cover,
            Self::Image,
            Self::Video,
        ]
        .into_iter()
        .filter(|background| *background != Self::Video || cfg!(target_os = "windows"))
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Waves => "waves",
            Self::Fluid => "fluid",
            Self::Cover => "cover",
            Self::Image => "image",
            Self::Video => "video",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Waves => "settings-lyrics-background-waves",
            Self::Fluid => "settings-lyrics-background-fluid",
            Self::Cover => "settings-lyrics-background-cover",
            Self::Image => "settings-lyrics-background-image",
            Self::Video => "settings-lyrics-background-video",
        }
    }

    /// Whether the background is a picture, which the dim and blur act on. The two fields are
    /// drawn dark enough for the lines by their own shaders.
    pub fn pictured(self) -> bool {
        matches!(self, Self::Cover | Self::Image | Self::Video)
    }
}

/// Where the lines sit across the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsAlign {
    #[default]
    Left,
    Center,
}

impl LyricsAlign {
    pub const ALL: [Self; 2] = [Self::Left, Self::Center];

    pub fn key(self) -> &'static str {
        match self {
            Self::Left => "settings-lyrics-align-left",
            Self::Center => "settings-lyrics-align-center",
        }
    }
}

/// How the line being sung stands out from the ones around it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsEmphasis {
    /// It grows a little and brightens.
    #[default]
    Scale,
    /// It is set one weight heavier and brightens.
    Weight,
    /// It only brightens.
    Color,
}

impl LyricsEmphasis {
    pub const ALL: [Self; 3] = [Self::Scale, Self::Weight, Self::Color];

    pub fn key(self) -> &'static str {
        match self {
            Self::Scale => "settings-lyrics-emphasis-scale",
            Self::Weight => "settings-lyrics-emphasis-weight",
            Self::Color => "settings-lyrics-emphasis-color",
        }
    }
}

/// The look of the lyrics page. Read whole by the views and written one field at a time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricsStyle {
    pub background: LyricsBackground,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<PathBuf>,
    /// How much of a picture background the scrim hides, from `MIN_DIM` to `MAX_DIM`. The page
    /// may hide more where the picture is bright, so the lines keep their contrast.
    pub dim: f32,
    /// How soft a picture background is drawn, from 0 sharp to 1 at the strongest blur.
    pub blur: f32,
    /// The face the lines are set in, `SYSTEM_FONT` for the app's own.
    pub font: String,
    pub weight: u16,
    /// The space between and inside verses, as a share of the default.
    pub spacing: f32,
    pub align: LyricsAlign,
    pub emphasis: LyricsEmphasis,
    /// How strongly the lines around the sung one show, as a share of the default.
    pub inactive: f32,
    /// Whether a video background holds still while the music is paused.
    pub video_follows_playback: bool,
}

impl Default for LyricsStyle {
    fn default() -> Self {
        Self {
            background: LyricsBackground::Waves,
            image: None,
            video: None,
            dim: 0.4,
            blur: 0.5,
            font: SYSTEM_FONT.to_owned(),
            weight: 600,
            spacing: 1.,
            align: LyricsAlign::Left,
            emphasis: LyricsEmphasis::Scale,
            inactive: 1.,
            video_follows_playback: true,
        }
    }
}

impl LyricsStyle {
    /// The style with every number held within its range and the weight on one of
    /// `LYRICS_WEIGHTS`, whatever a hand edit left.
    pub fn settled(mut self) -> Self {
        self.dim = clamp(self.dim, MIN_DIM, MAX_DIM, 0.4);
        self.blur = clamp(self.blur, 0., 1., 0.5);
        self.spacing = clamp(self.spacing, MIN_SPACING, MAX_SPACING, 1.);
        self.inactive = clamp(self.inactive, MIN_INACTIVE, 1., 1.);
        self.weight = LYRICS_WEIGHTS
            .into_iter()
            .min_by_key(|weight| weight.abs_diff(self.weight))
            .unwrap_or(600);
        if !cfg!(target_os = "windows") && self.background == LyricsBackground::Video {
            self.background = LyricsBackground::Waves;
        }
        self
    }
}

/// A look saved under a name: the style and the lyrics settings that shape the lines with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricsPreset {
    pub name: String,
    pub style: LyricsStyle,
    pub scale: f32,
    pub karaoke: bool,
    pub romanized: bool,
    pub haze: bool,
}

impl Default for LyricsPreset {
    fn default() -> Self {
        Self {
            name: String::new(),
            style: LyricsStyle::default(),
            scale: super::DEFAULT_LYRICS_SCALE,
            karaoke: true,
            romanized: true,
            haze: true,
        }
    }
}

/// One field of the style, named so a single setter and a single reset serve them all.
#[derive(Clone, Debug)]
pub enum LyricsStyleChange {
    Background(LyricsBackground),
    Image(Option<PathBuf>),
    Video(Option<PathBuf>),
    Dim(f32),
    Blur(f32),
    Font(String),
    Weight(u16),
    Spacing(f32),
    Align(LyricsAlign),
    Emphasis(LyricsEmphasis),
    Inactive(f32),
    VideoFollowsPlayback(bool),
}

impl AppSettings {
    /// The look of the lyrics page, held within range.
    pub fn lyrics_style(&self) -> LyricsStyle {
        self.values.lyrics_style.clone().settled()
    }

    /// Changes one field of the lyrics style and saves it.
    pub fn set_lyrics_style(&mut self, change: LyricsStyleChange, cx: &mut Context<Self>) {
        let style = &mut self.values.lyrics_style;
        match change {
            LyricsStyleChange::Background(background) => style.background = background,
            LyricsStyleChange::Image(image) => style.image = image,
            LyricsStyleChange::Video(video) => style.video = video,
            LyricsStyleChange::Dim(dim) => style.dim = dim,
            LyricsStyleChange::Blur(blur) => style.blur = blur,
            LyricsStyleChange::Font(font) => style.font = font,
            LyricsStyleChange::Weight(weight) => style.weight = weight,
            LyricsStyleChange::Spacing(spacing) => style.spacing = spacing,
            LyricsStyleChange::Align(align) => style.align = align,
            LyricsStyleChange::Emphasis(emphasis) => style.emphasis = emphasis,
            LyricsStyleChange::Inactive(inactive) => style.inactive = inactive,
            LyricsStyleChange::VideoFollowsPlayback(follows) => {
                style.video_follows_playback = follows
            }
        }
        self.values.lyrics_style = self.values.lyrics_style.clone().settled();
        self.schedule_save(cx);
    }

    /// Whether fullscreen paints the lyrics page's own background: the lyrics panel is up and its
    /// style asks for something other than the ambient waves.
    pub fn lyrics_backdrop(&self) -> bool {
        self.fullscreen_tab() == Some(super::SideTab::Lyrics)
            && self.lyrics_style().background != LyricsBackground::Waves
    }

    /// The saved looks, in the order they were saved.
    pub fn lyrics_presets(&self) -> &[LyricsPreset] {
        &self.values.lyrics_presets
    }

    /// The look as it stands, under `name`.
    fn lyrics_look(&self, name: String) -> LyricsPreset {
        LyricsPreset {
            name,
            style: self.lyrics_style(),
            scale: self.fullscreen_lyrics_scale(),
            karaoke: self.karaoke_lyrics(),
            romanized: self.romanized_lyrics(),
            haze: self.blur_lyrics(),
        }
    }

    /// The name of the saved look the page wears now, if it wears one.
    pub fn worn_lyrics_preset(&self) -> Option<&str> {
        let worn = self.lyrics_look(String::new());
        self.values
            .lyrics_presets
            .iter()
            .find(|preset| {
                LyricsPreset {
                    name: String::new(),
                    ..(*preset).clone()
                } == worn
            })
            .map(|preset| preset.name.as_str())
    }

    /// Saves the look as it stands under `name`, replacing a preset of the same name. Returns
    /// false for a blank name or a full list.
    pub fn save_lyrics_preset(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let Some(name) = preset_name(name) else {
            return false;
        };
        let look = self.lyrics_look(name.clone());
        let presets = &mut self.values.lyrics_presets;
        match presets.iter().position(|preset| preset.name == name) {
            Some(place) => presets[place] = look,
            None if presets.len() < MOST_PRESETS => presets.push(look),
            None => return false,
        }
        self.schedule_save(cx);
        true
    }

    /// Puts on the look saved as `name`.
    pub fn apply_lyrics_preset(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(preset) = self
            .values
            .lyrics_presets
            .iter()
            .find(|preset| preset.name == name)
            .cloned()
        else {
            return;
        };
        self.values.lyrics_style = preset.style.settled();
        self.values.fullscreen_lyrics_scale = preset
            .scale
            .clamp(ui::MIN_LYRICS_SCALE, ui::MAX_LYRICS_SCALE);
        self.values.karaoke_lyrics = preset.karaoke;
        self.values.romanized_lyrics = preset.romanized;
        self.values.blur_lyrics = preset.haze;
        self.schedule_save(cx);
    }

    /// Puts the default look back on, the presets left as they are.
    pub fn reset_lyrics_look(&mut self, cx: &mut Context<Self>) {
        let preset = LyricsPreset::default();
        self.values.lyrics_style = preset.style;
        self.values.fullscreen_lyrics_scale = preset.scale;
        self.values.karaoke_lyrics = preset.karaoke;
        self.values.romanized_lyrics = preset.romanized;
        self.values.blur_lyrics = preset.haze;
        self.schedule_save(cx);
    }

    /// Renames the preset `from` to `to`. A blank name, or one another preset already has, is
    /// refused with false.
    pub fn rename_lyrics_preset(&mut self, from: &str, to: &str, cx: &mut Context<Self>) -> bool {
        let Some(to) = preset_name(to) else {
            return false;
        };
        let presets = &mut self.values.lyrics_presets;
        if to != from && presets.iter().any(|preset| preset.name == to) {
            return false;
        }
        let Some(preset) = presets.iter_mut().find(|preset| preset.name == from) else {
            return false;
        };
        preset.name = to;
        self.schedule_save(cx);
        true
    }

    pub fn delete_lyrics_preset(&mut self, name: &str, cx: &mut Context<Self>) {
        self.values
            .lyrics_presets
            .retain(|preset| preset.name != name);
        self.schedule_save(cx);
    }
}

/// A preset name trimmed and cut to `LONGEST_NAME`, or none when nothing is left.
fn preset_name(name: &str) -> Option<String> {
    let name = name.trim().chars().take(LONGEST_NAME).collect::<String>();
    (!name.trim().is_empty()).then(|| name.trim().to_owned())
}

/// `value` within `low..=high`, or `fallback` when it is not a number at all.
fn clamp(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    match value.is_finite() {
        true => value.clamp(low, high),
        false => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_settings_file_takes_the_default_look() {
        let style: LyricsStyle = serde_json::from_str("{}").unwrap();
        assert_eq!(style, LyricsStyle::default());
        let style: LyricsStyle =
            serde_json::from_str(r#"{"background":"fluid","weight":650,"dim":9}"#).unwrap();
        let style = style.settled();
        assert_eq!(style.background, LyricsBackground::Fluid);
        assert!(LYRICS_WEIGHTS.contains(&style.weight));
        assert_eq!(style.dim, MAX_DIM);
    }

    #[test]
    fn preset_names_are_trimmed_and_never_blank() {
        assert_eq!(preset_name("  Night  ").as_deref(), Some("Night"));
        assert_eq!(preset_name("   "), None);
        assert_eq!(
            preset_name(&"x".repeat(80)).map(|name| name.len()),
            Some(LONGEST_NAME)
        );
    }
}
