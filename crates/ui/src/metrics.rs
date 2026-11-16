use gpui::{Pixels, SharedString, TextRun, Window, px};
use i18n::t;

const HEADER: f32 = 0.75;

pub const LEADING: f32 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    Square,
    Subtle,
    Rounded,
    Round,
}

impl Rounding {
    pub const ALL: [Self; 4] = [Self::Square, Self::Subtle, Self::Rounded, Self::Round];

    pub fn id(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::Subtle => "subtle",
            Self::Rounded => "rounded",
            Self::Round => "round",
        }
    }

    pub fn label(self) -> SharedString {
        match self {
            Self::Square => t!("corners-square"),
            Self::Subtle => t!("corners-subtle"),
            Self::Rounded => t!("corners-rounded"),
            Self::Round => t!("corners-round"),
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "square" => Self::Square,
            "rounded" => Self::Rounded,
            "round" => Self::Round,
            _ => Self::Subtle,
        }
    }

    pub fn radius(self) -> Pixels {
        match self {
            Self::Square => px(0.),
            Self::Subtle => px(6.),
            Self::Rounded => px(10.),
            Self::Round => px(20.),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Metrics {
    pub row: Pixels,
    pub header: Pixels,
    pub pad: Pixels,
    pub inset: Pixels,
    pub control: Pixels,
    pub control_small: Pixels,
    pub field: Pixels,
    pub title_bar: Pixels,
    pub player_bar: Pixels,
    pub list_row: Pixels,
    /// The room between one section of a page and the next, such as two shelves on Home.
    pub section: Pixels,
    pub thumb: Pixels,
    pub cover: Pixels,
    /// The widest a page grows, its side insets included. A wider content area centres the
    /// page and gives what is left to its side margins.
    pub page: Pixels,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new(px(13.))
    }
}

impl Metrics {
    pub fn new(base: Pixels) -> Self {
        let text = base / px(1.);
        let roomy = |step: f32, lines: f32| px(step.max((text * lines).round()));
        let row = roomy(42., 2.4);

        Self {
            row,
            header: px((row / px(1.) * HEADER).round()),
            pad: px(8.),
            inset: px(24.),
            control: roomy(32., 2.),
            control_small: roomy(26., 1.7),
            field: roomy(36., 2.6),
            title_bar: roomy(36., 2.4),
            player_bar: roomy(76., 5.),
            list_row: roomy(52., 3.2),
            section: roomy(32., 2.3),
            thumb: px(34.),
            cover: px(140.),
            page: px(1280.),
        }
    }
}

/// Sonicly's type scale, as steps of the listener's base size: 10, 11, 12, 13, 15, 18, 22 and
/// 30 at the default 13. The steps widen as they climb, so a page title, a section and a row
/// never read as near neighbours, and the artwork between them leads.
#[derive(Clone, Copy)]
pub enum Text {
    /// Badges and the smallest counts.
    Tiny,
    /// Meta lines under a title, eyebrows and captions.
    Small,
    /// Secondary labels beside controls.
    Label,
    /// Reading text and row titles.
    Body,
    /// A dialog's title.
    Heading,
    /// A section of a page, such as a shelf on Home or a group in Settings.
    Large,
    /// A page's own title.
    Title,
    /// The name an album, playlist, artist or genre page is about.
    Display,
}

impl Text {
    pub(crate) fn ratio(self) -> f32 {
        match self {
            Self::Tiny => 0.77,
            Self::Small => 0.85,
            Self::Label => 0.92,
            Self::Body => 1.,
            Self::Heading => 1.15,
            Self::Large => 1.38,
            Self::Title => 1.69,
            Self::Display => 2.29,
        }
    }

    /// The line height this step sets, as a share of its size. Large type sits tighter, since
    /// the room a body line needs reads as a gap between the lines of a title.
    pub fn leading(self) -> f32 {
        match self {
            Self::Display => 1.1,
            Self::Title => 1.15,
            Self::Large => 1.2,
            _ => LEADING,
        }
    }
}

const INSET: f32 = 0.25;
const BORDER: Pixels = px(1.);

pub fn snapped(value: Pixels, window: &Window) -> Pixels {
    let scale = window.scale_factor();
    px((value / px(1.) * scale).round() / scale)
}

pub fn text_width(text: impl Into<SharedString>, window: &Window) -> Pixels {
    let text = text.into();
    if text.is_empty() {
        return Pixels::ZERO;
    }

    let style = window.text_style();
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let size = style.font_size.to_pixels(window.rem_size());

    window
        .text_system()
        .shape_line(text, size, &[run], None)
        .width
}

pub fn tucked(radius: Pixels, window: &Window) -> Pixels {
    (radius - window.rem_size() * INSET - BORDER).max(Pixels::ZERO)
}
