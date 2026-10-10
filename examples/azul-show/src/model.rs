//! The deck model: a deck holds slides, a slide holds elements (text boxes,
//! shapes, pictures, tables, chart and video placeholders, groups) in z-order
//! (the first element is at the back), every element has a frame (x, y, w, h,
//! rotation) in slide units on a fixed canvas (1920 x 1080 for 16:9, 1440 x
//! 1080 for 4:3). A layout places role-tagged placeholders (title, subtitle,
//! body, ...), so re-applying a layout keeps the content (bento.page's rule).
//! The deck's theme is a colour scheme and a font scheme.
//!
//! Durable data is a file: the deck serializes to `show/<uuid>/deck.json`
//! ([`Deck::to_json`] / [`Deck::from_json`]); pictures live next to it under
//! `show/<uuid>/media/`. No azul types here: the model is tested without a
//! window.

use serde::{Deserialize, Serialize};

/// The `format` of a deck file.
pub const DECK_FORMAT: &str = "azshow.deck";
/// The newest `version` of a deck file this app reads and writes.
pub const DECK_VERSION: u32 = 1;
/// Every slide is this tall, in slide units; the width follows the size.
pub const SLIDE_HEIGHT: f32 = 1080.0;

// ==== Colours ====

/// An sRGB colour, written as `#rrggbb` (or `#rrggbbaa`) in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The colour at opacity `a`.
    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// The colour `t` (0..1) of the way from this one to `to`, channel by
    /// channel (the Morph transition recolours a shape so).
    #[must_use]
    pub fn lerp(self, to: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Color {
            r: mix(self.r, to.r),
            g: mix(self.g, to.g),
            b: mix(self.b, to.b),
            a: mix(self.a, to.a),
        }
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    #[must_use]
    pub fn hex(&self) -> String {
        azul::css::ColorU {
            r: self.r,
            g: self.g,
            b: self.b,
            a: self.a,
        }
        .to_hex()
        .to_string()
    }

    /// Reads `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` (a deck writes the
    /// `#`, so a text without it is no colour).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if !text.starts_with('#') {
            return None;
        }
        azul::css::ColorU::parse_hex(text)
            .into_option()
            .map(|c| Self::rgba(c.r, c.g, c.b, c.a))
    }

    /// The colour as a CSS value (`#rrggbb` / `#rrggbbaa`).
    #[must_use]
    pub fn css(&self) -> String {
        self.hex()
    }

    /// Relative luminance (0 = black, 1 = white), for picking a readable ink.
    #[must_use]
    pub fn luminance(&self) -> f32 {
        let lin = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

impl From<Color> for String {
    fn from(c: Color) -> Self {
        c.hex()
    }
}

impl TryFrom<String> for Color {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        Color::parse(&text).ok_or_else(|| format!("not a colour: {text:?}"))
    }
}

// ==== Theme ====

/// The deck's colours: the slide's ground, the inks and the accents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColorScheme {
    pub name: String,
    pub background: Color,
    pub text: Color,
    pub title: Color,
    pub accent: Color,
    pub accent2: Color,
    pub accent3: Color,
}

/// The deck's fonts: one for titles, one for the rest (CSS font families).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FontScheme {
    pub name: String,
    pub heading: String,
    pub body: String,
}

/// A deck theme: a colour scheme and a font scheme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub colors: ColorScheme,
    pub fonts: FontScheme,
}

impl Theme {
    /// The plain theme: white slides, near-black ink, a blue accent.
    #[must_use]
    pub fn office() -> Self {
        Self {
            name: String::from("Office"),
            colors: ColorScheme {
                name: String::from("Office"),
                background: Color::rgb(0xff, 0xff, 0xff),
                text: Color::rgb(0x26, 0x26, 0x26),
                title: Color::rgb(0x1f, 0x1f, 0x1f),
                accent: Color::rgb(0x2b, 0x57, 0x9a),
                accent2: Color::rgb(0xc5, 0x5a, 0x11),
                accent3: Color::rgb(0x70, 0xad, 0x47),
            },
            fonts: FontScheme::all().into_iter().next().unwrap_or_else(|| FontScheme {
                name: String::from("Office"),
                heading: String::from("sans-serif"),
                body: String::from("sans-serif"),
            }),
        }
    }
}

impl FontScheme {
    /// The font schemes the Design tab offers.
    #[must_use]
    pub fn all() -> Vec<Self> {
        let scheme = |name: &str, heading: &str, body: &str| FontScheme {
            name: name.to_string(),
            heading: heading.to_string(),
            body: body.to_string(),
        };
        vec![
            scheme("Office", "Helvetica Neue, Arial, sans-serif", "Helvetica Neue, Arial, sans-serif"),
            scheme("Garamond", "EB Garamond, Georgia, serif", "EB Garamond, Georgia, serif"),
            scheme("Georgia / Helvetica", "Georgia, serif", "Helvetica Neue, Arial, sans-serif"),
            scheme("Avenir", "Avenir Next, Avenir, sans-serif", "Avenir Next, Avenir, sans-serif"),
            scheme("Mono", "Menlo, Consolas, monospace", "Helvetica Neue, Arial, sans-serif"),
        ]
    }
}

// ==== Geometry ====

/// The slide size: 16:9 (1920 x 1080) or 4:3 (1440 x 1080).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideSize {
    #[default]
    Wide,
    Standard,
}

impl SlideSize {
    #[must_use]
    pub const fn width(self) -> f32 {
        match self {
            Self::Wide => 1920.0,
            Self::Standard => 1440.0,
        }
    }

    #[must_use]
    pub const fn height(self) -> f32 {
        SLIDE_HEIGHT
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Wide => "Widescreen (16:9)",
            Self::Standard => "Standard (4:3)",
        }
    }
}

/// Where an element sits: its box in slide units and its rotation in
/// degrees, clockwise, about the box's centre.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default)]
    pub rotation: f32,
}

impl Frame {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            rotation: 0.0,
        }
    }

    #[must_use]
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// The axis-aligned box around the rotated frame.
    #[must_use]
    pub fn bounds(&self) -> Frame {
        if self.rotation.abs() < f32::EPSILON {
            return Frame::new(self.x, self.y, self.w, self.h);
        }
        let (cx, cy) = self.center();
        let (s, c) = self.rotation.to_radians().sin_cos();
        let (hw, hh) = (self.w / 2.0, self.h / 2.0);
        let ex = (hw * c).abs() + (hh * s).abs();
        let ey = (hw * s).abs() + (hh * c).abs();
        Frame::new(cx - ex, cy - ey, ex * 2.0, ey * 2.0)
    }

    /// The axis-aligned box around all of `frames` (their rotated bounds).
    #[must_use]
    pub fn union<'a>(frames: impl IntoIterator<Item = &'a Frame>) -> Option<Frame> {
        let mut out: Option<(f32, f32, f32, f32)> = None;
        for f in frames {
            let b = f.bounds();
            let (x0, y0, x1, y1) = (b.x, b.y, b.x + b.w, b.y + b.h);
            out = Some(match out {
                None => (x0, y0, x1, y1),
                Some((a0, b0, a1, b1)) => (a0.min(x0), b0.min(y0), a1.max(x1), b1.max(y1)),
            });
        }
        out.map(|(x0, y0, x1, y1)| Frame::new(x0, y0, x1 - x0, y1 - y0))
    }

    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> Frame {
        Frame {
            x: self.x + dx,
            y: self.y + dy,
            ..*self
        }
    }

    /// This frame mapped from the box `from` onto the box `to` (a group's
    /// child when the group is resized): position and size scale with the
    /// box, the rotation stays.
    #[must_use]
    pub fn mapped(&self, from: &Frame, to: &Frame) -> Frame {
        let sx = if from.w.abs() > f32::EPSILON { to.w / from.w } else { 1.0 };
        let sy = if from.h.abs() > f32::EPSILON { to.h / from.h } else { 1.0 };
        Frame {
            x: to.x + (self.x - from.x) * sx,
            y: to.y + (self.y - from.y) * sy,
            w: self.w * sx,
            h: self.h * sy,
            rotation: self.rotation,
        }
    }

    /// The frame `t` (0..1) of the way from this one to `to`: position,
    /// size and rotation (the Morph transition moves a shape so).
    #[must_use]
    pub fn lerp(&self, to: &Frame, t: f32) -> Frame {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Frame {
            x: mix(self.x, to.x),
            y: mix(self.y, to.y),
            w: mix(self.w, to.w),
            h: mix(self.h, to.h),
            rotation: mix(self.rotation, to.rotation),
        }
    }
}

// ==== Text ====

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

/// A run of text in one format.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Run {
    pub text: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strike: bool,
}

impl Run {
    #[must_use]
    pub fn plain(text: &str) -> Self {
        Self {
            text: text.to_string(),
            ..Self::default()
        }
    }
}

/// A paragraph: runs, an alignment, and - in a body - a bullet at a level.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    #[serde(default)]
    pub align: Align,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bullet: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub level: u8,
}

impl Paragraph {
    #[must_use]
    pub fn plain(text: &str) -> Self {
        Self {
            runs: if text.is_empty() { Vec::new() } else { vec![Run::plain(text)] },
            ..Self::default()
        }
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// The text of a text box, a placeholder or a shape.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TextBody {
    pub paragraphs: Vec<Paragraph>,
    /// Font size in slide units (px on the 1080-high canvas).
    pub size: f32,
    /// The ink; `None` takes the theme's (title ink for a title).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// The font family; `None` takes the theme's (heading font for a title).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(default)]
    pub valign: VAlign,
    /// What an empty placeholder shows while editing ("Click to add title").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,
}

impl TextBody {
    /// Text at `size`, one paragraph per line.
    #[must_use]
    pub fn plain(text: &str, size: f32) -> Self {
        let mut body = Self {
            size,
            ..Self::default()
        };
        body.set_text(text);
        body
    }

    /// The text, paragraphs joined by `\n`.
    #[must_use]
    pub fn text(&self) -> String {
        self.paragraphs
            .iter()
            .map(Paragraph::text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.text.is_empty()))
    }

    /// Replaces the text, one paragraph per line; each paragraph keeps the
    /// bullet, level and alignment of the paragraph that stood there (the
    /// last one for new lines), its runs take the first run's format.
    pub fn set_text(&mut self, text: &str) {
        let template = self.paragraphs.first().cloned().unwrap_or_default();
        let run_format = template.runs.first().cloned().unwrap_or_default();
        let old = core::mem::take(&mut self.paragraphs);
        for (i, line) in text.split('\n').enumerate() {
            let shape = old.get(i).or_else(|| old.last()).unwrap_or(&template);
            let mut run = run_format.clone();
            run.text = line.to_string();
            self.paragraphs.push(Paragraph {
                runs: if line.is_empty() { Vec::new() } else { vec![run] },
                align: shape.align,
                bullet: shape.bullet,
                level: shape.level,
            });
        }
    }
}

// ==== Elements ====

/// What a placeholder holds; a layout places placeholders by role, and
/// re-applying a layout moves content by role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceholderRole {
    Title,
    Subtitle,
    Body,
    Body2,
    Heading,
    Heading2,
}

impl PlaceholderRole {
    #[must_use]
    pub const fn prompt(self) -> &'static str {
        match self {
            Self::Title => "Click to add title",
            Self::Subtitle => "Click to add subtitle",
            Self::Body | Self::Body2 => "Click to add text",
            Self::Heading | Self::Heading2 => "Click to add heading",
        }
    }

    /// Whether the role's text takes the heading font and the title ink.
    #[must_use]
    pub const fn is_heading(self) -> bool {
        matches!(self, Self::Title | Self::Heading | Self::Heading2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rect,
    RoundRect,
    Ellipse,
    Triangle,
    Line,
    Arrow,
    Diamond,
    Chevron,
    /// A line with an arrowhead at its right end (a connector).
    LineArrow,
}

impl ShapeKind {
    pub const ALL: [ShapeKind; 9] = [
        Self::Rect,
        Self::RoundRect,
        Self::Ellipse,
        Self::Triangle,
        Self::Line,
        Self::Arrow,
        Self::Diamond,
        Self::Chevron,
        Self::LineArrow,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Rect => "Rectangle",
            Self::RoundRect => "Rounded rectangle",
            Self::Ellipse => "Oval",
            Self::Triangle => "Triangle",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::Diamond => "Diamond",
            Self::Chevron => "Chevron",
            Self::LineArrow => "Line Arrow",
        }
    }

    /// The glyph the ribbon shows for it.
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Rect => "crop_square",
            Self::RoundRect => "rounded_corner",
            Self::Ellipse => "circle",
            Self::Triangle => "change_history",
            Self::Line => "horizontal_rule",
            Self::Arrow => "arrow_right_alt",
            Self::Diamond => "diamond",
            Self::Chevron => "double_arrow",
            Self::LineArrow => "east",
        }
    }

    /// Whether it is drawn as a stroke (its colour is the outline, not a
    /// fill): a line, a line with an arrowhead.
    #[must_use]
    pub const fn is_line(self) -> bool {
        matches!(self, Self::Line | Self::LineArrow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    #[default]
    Contain,
    Cover,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
}

/// What an element is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ElementKind {
    /// A text box (also every text placeholder).
    Text { body: TextBody },
    /// A shape, with optional text inside.
    Shape {
        shape: ShapeKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<Color>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<Color>,
        #[serde(default)]
        stroke_width: f32,
        #[serde(default)]
        body: TextBody,
    },
    /// A picture: `media` is its key under the deck's folder (`media/<uuid>.png`).
    Image {
        media: String,
        #[serde(default)]
        fit: ImageFit,
    },
    /// A table of plain cells; the first row is the header when `header`.
    Table { rows: Vec<Vec<String>>, header: bool },
    /// A chart placeholder (no chart engine yet).
    Chart { chart: ChartKind, title: String },
    /// A video placeholder: `media` is its key under the deck's folder.
    Video { media: String },
    /// A group: its children keep their own frames, in slide units.
    Group { children: Vec<Element> },
}

/// An animation effect: entrance, emphasis or exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationEffect {
    Appear,
    Fade,
    FlyIn,
    Zoom,
    Pulse,
    Spin,
    Disappear,
    FadeOut,
    FlyOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationClass {
    Entrance,
    Emphasis,
    Exit,
}

impl AnimationEffect {
    pub const ALL: [AnimationEffect; 9] = [
        Self::Appear,
        Self::Fade,
        Self::FlyIn,
        Self::Zoom,
        Self::Pulse,
        Self::Spin,
        Self::Disappear,
        Self::FadeOut,
        Self::FlyOut,
    ];

    #[must_use]
    pub const fn class(self) -> AnimationClass {
        match self {
            Self::Appear | Self::Fade | Self::FlyIn | Self::Zoom => AnimationClass::Entrance,
            Self::Pulse | Self::Spin => AnimationClass::Emphasis,
            Self::Disappear | Self::FadeOut | Self::FlyOut => AnimationClass::Exit,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Appear => "Appear",
            Self::Fade => "Fade",
            Self::FlyIn => "Fly In",
            Self::Zoom => "Zoom",
            Self::Pulse => "Pulse",
            Self::Spin => "Spin",
            Self::Disappear => "Disappear",
            Self::FadeOut => "Fade Out",
            Self::FlyOut => "Fly Out",
        }
    }
}

/// An element's build: its effect, its place in the slide's click order,
/// and how long it plays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Animation {
    pub effect: AnimationEffect,
    pub order: u32,
    pub duration_ms: u32,
}

/// One thing on a slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: u64,
    pub frame: Frame,
    #[serde(flatten)]
    pub kind: ElementKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<PlaceholderRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<Animation>,
}

impl Element {
    #[must_use]
    pub fn new(id: u64, frame: Frame, kind: ElementKind) -> Self {
        Self {
            id,
            frame,
            kind,
            placeholder: None,
            animation: None,
        }
    }

    /// The text body, for a text box or a shape.
    #[must_use]
    pub fn body(&self) -> Option<&TextBody> {
        match &self.kind {
            ElementKind::Text { body } | ElementKind::Shape { body, .. } => Some(body),
            _ => None,
        }
    }

    pub fn body_mut(&mut self) -> Option<&mut TextBody> {
        match &mut self.kind {
            ElementKind::Text { body } | ElementKind::Shape { body, .. } => Some(body),
            _ => None,
        }
    }

    /// Moves the element to `frame`; a group maps its children from its
    /// old box onto the new one.
    pub fn set_frame(&mut self, frame: Frame) {
        let from = self.frame;
        if let ElementKind::Group { children } = &mut self.kind {
            // A turn of the group turns every member about the group's
            // centre and adds to its own rotation.
            let turn = frame.rotation - from.rotation;
            let (cx, cy) = frame.center();
            let (s, c) = turn.to_radians().sin_cos();
            for child in children.iter_mut() {
                let mut f = child.frame.mapped(&from, &frame);
                if turn.abs() > f32::EPSILON {
                    let (x, y) = f.center();
                    let (dx, dy) = (x - cx, y - cy);
                    let (nx, ny) = (cx + dx * c - dy * s, cy + dx * s + dy * c);
                    f.x = nx - f.w / 2.0;
                    f.y = ny - f.h / 2.0;
                    f.rotation += turn;
                }
                child.set_frame(f);
            }
        }
        self.frame = frame;
    }

    /// The element and, for a group, every element inside it, depth first.
    pub fn walk<'a>(&'a self, out: &mut Vec<&'a Element>) {
        out.push(self);
        if let ElementKind::Group { children } = &self.kind {
            for c in children {
                c.walk(out);
            }
        }
    }

    /// A short name for the selection pane and the screen reader.
    #[must_use]
    pub fn name(&self) -> String {
        match &self.kind {
            ElementKind::Text { body } => {
                let text = body.text();
                let first = text.lines().next().unwrap_or("").trim();
                if first.is_empty() {
                    match self.placeholder {
                        Some(role) => format!("{role:?} placeholder"),
                        None => String::from("Text box"),
                    }
                } else {
                    first.chars().take(40).collect()
                }
            }
            ElementKind::Shape { shape, .. } => shape.label().to_string(),
            ElementKind::Image { .. } => String::from("Picture"),
            ElementKind::Table { .. } => String::from("Table"),
            ElementKind::Chart { .. } => String::from("Chart"),
            ElementKind::Video { .. } => String::from("Video"),
            ElementKind::Group { children } => format!("Group of {}", children.len()),
        }
    }
}

// ==== Slides ====

/// A slide layout: which placeholders a new slide gets, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutKind {
    TitleSlide,
    TitleAndContent,
    SectionHeader,
    TwoContent,
    Comparison,
    TitleOnly,
    Blank,
}

impl LayoutKind {
    pub const ALL: [LayoutKind; 7] = [
        Self::TitleSlide,
        Self::TitleAndContent,
        Self::SectionHeader,
        Self::TwoContent,
        Self::Comparison,
        Self::TitleOnly,
        Self::Blank,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TitleSlide => "Title Slide",
            Self::TitleAndContent => "Title and Content",
            Self::SectionHeader => "Section Header",
            Self::TwoContent => "Two Content",
            Self::Comparison => "Comparison",
            Self::TitleOnly => "Title Only",
            Self::Blank => "Blank",
        }
    }

    /// The layout's placeholders on a slide of `size`: role, frame, the
    /// text's size and alignment, and whether its paragraphs are bullets.
    #[must_use]
    pub fn placeholders(self, size: SlideSize) -> Vec<PlaceholderSpec> {
        use PlaceholderRole::{Body, Body2, Heading, Heading2, Subtitle, Title};

        let width = size.width();
        let margin = (width * 0.0625).round();
        let full = width - 2.0 * margin;
        let gap = 60.0;
        let column = (full - gap) / 2.0;
        let right = margin + column + gap;
        let spec = |role: PlaceholderRole,
                    frame: Frame,
                    text_size: f32,
                    align: Align,
                    valign: VAlign,
                    bullets: bool| PlaceholderSpec {
            role,
            frame,
            size: text_size,
            align,
            valign,
            bullets,
        };
        let title = spec(
            Title,
            Frame::new(margin, 60.0, full, 160.0),
            60.0,
            Align::Left,
            VAlign::Middle,
            false,
        );
        match self {
            Self::TitleSlide => vec![
                spec(Title, Frame::new(margin, 300.0, full, 240.0), 88.0, Align::Center, VAlign::Bottom, false),
                spec(Subtitle, Frame::new(margin, 570.0, full, 160.0), 40.0, Align::Center, VAlign::Top, false),
            ],
            Self::TitleAndContent => vec![
                title,
                spec(Body, Frame::new(margin, 260.0, full, 740.0), 36.0, Align::Left, VAlign::Top, true),
            ],
            Self::SectionHeader => vec![
                spec(Title, Frame::new(margin, 380.0, full, 220.0), 80.0, Align::Left, VAlign::Bottom, false),
                spec(Subtitle, Frame::new(margin, 620.0, full, 140.0), 36.0, Align::Left, VAlign::Top, false),
            ],
            Self::TwoContent => vec![
                title,
                spec(Body, Frame::new(margin, 260.0, column, 740.0), 32.0, Align::Left, VAlign::Top, true),
                spec(Body2, Frame::new(right, 260.0, column, 740.0), 32.0, Align::Left, VAlign::Top, true),
            ],
            Self::Comparison => vec![
                title,
                spec(Heading, Frame::new(margin, 250.0, column, 100.0), 40.0, Align::Left, VAlign::Bottom, false),
                spec(Body, Frame::new(margin, 360.0, column, 640.0), 32.0, Align::Left, VAlign::Top, true),
                spec(Heading2, Frame::new(right, 250.0, column, 100.0), 40.0, Align::Left, VAlign::Bottom, false),
                spec(Body2, Frame::new(right, 360.0, column, 640.0), 32.0, Align::Left, VAlign::Top, true),
            ],
            Self::TitleOnly => vec![title],
            Self::Blank => Vec::new(),
        }
    }
}

impl PlaceholderRole {
    /// The role whose content fills this one when a new layout lacks the
    /// old role: a subtitle becomes the body and the body the subtitle.
    #[must_use]
    pub const fn stand_in(self) -> Option<PlaceholderRole> {
        match self {
            Self::Body => Some(Self::Subtitle),
            Self::Subtitle => Some(Self::Body),
            _ => None,
        }
    }
}

/// One placeholder a layout places.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaceholderSpec {
    pub role: PlaceholderRole,
    pub frame: Frame,
    pub size: f32,
    pub align: Align,
    pub valign: VAlign,
    pub bullets: bool,
}

impl PlaceholderSpec {
    /// An empty placeholder element of this spec.
    #[must_use]
    pub fn element(&self, id: u64) -> Element {
        let mut body = TextBody {
            size: self.size,
            valign: self.valign,
            prompt: self.role.prompt().to_string(),
            ..TextBody::default()
        };
        body.paragraphs.push(Paragraph {
            runs: Vec::new(),
            align: self.align,
            bullet: self.bullets,
            level: 0,
        });
        let mut element = Element::new(id, self.frame, ElementKind::Text { body });
        element.placeholder = Some(self.role);
        element
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    #[default]
    None,
    Fade,
    Push,
    Wipe,
    /// PowerPoint's Morph (Keynote's Magic Move): an object on both slides
    /// moves, resizes and recolours from its old place to its new one, the
    /// others fade out and in ([`morph_pairs`] says which are the same).
    Morph,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 5] = [Self::None, Self::Fade, Self::Push, Self::Wipe, Self::Morph];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Fade => "Fade",
            Self::Push => "Push",
            Self::Wipe => "Wipe",
            Self::Morph => "Morph",
        }
    }
}

/// How the show enters a slide.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration_ms: u32,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            kind: TransitionKind::None,
            duration_ms: 500,
        }
    }
}

/// A slide's ground when it is not the theme's.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Background {
    Solid { color: Color },
    Gradient { from: Color, to: Color },
}

/// One slide.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub id: u64,
    pub layout: LayoutKind,
    /// In z-order: the first element is at the back.
    pub elements: Vec<Element>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub transition: Transition,
    /// Skipped by the show.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// A section starts at this slide, titled so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

/// Which way a z-order command moves the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZOrder {
    BringToFront,
    BringForward,
    SendBackward,
    SendToBack,
}

impl Slide {
    /// A slide `id` with `layout`'s empty placeholders, their ids from `mint`.
    #[must_use]
    pub fn new(id: u64, layout: LayoutKind, size: SlideSize, mint: &mut dyn FnMut() -> u64) -> Self {
        let elements = layout
            .placeholders(size)
            .iter()
            .map(|spec| spec.element(mint()))
            .collect();
        Self {
            id,
            layout,
            elements,
            background: None,
            notes: String::new(),
            transition: Transition::default(),
            hidden: false,
            section: None,
        }
    }

    #[must_use]
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.elements.iter().position(|e| e.id == id)
    }

    #[must_use]
    pub fn element(&self, id: u64) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn element_mut(&mut self, id: u64) -> Option<&mut Element> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    #[must_use]
    pub fn placeholder(&self, role: PlaceholderRole) -> Option<&Element> {
        self.elements.iter().find(|e| e.placeholder == Some(role))
    }

    /// The title placeholder's text, or empty.
    #[must_use]
    pub fn title(&self) -> String {
        self.placeholder(PlaceholderRole::Title)
            .and_then(Element::body)
            .map(|b| b.text().lines().next().unwrap_or("").to_string())
            .unwrap_or_default()
    }

    /// Removes the elements `ids`.
    pub fn remove(&mut self, ids: &[u64]) {
        self.elements.retain(|e| !ids.contains(&e.id));
    }

    /// Moves the elements `ids` in the z-order (PowerPoint's Arrange
    /// commands; the selection keeps its own order).
    pub fn reorder(&mut self, ids: &[u64], how: ZOrder) {
        let selected = |e: &Element| ids.contains(&e.id);
        match how {
            ZOrder::BringToFront | ZOrder::SendToBack => {
                let (mut picked, mut rest): (Vec<Element>, Vec<Element>) =
                    core::mem::take(&mut self.elements).into_iter().partition(|e| selected(e));
                if how == ZOrder::BringToFront {
                    rest.append(&mut picked);
                    self.elements = rest;
                } else {
                    picked.append(&mut rest);
                    self.elements = picked;
                }
            }
            ZOrder::BringForward => {
                // From the top down: a selected element steps over the
                // unselected one right above it.
                for i in (0..self.elements.len().saturating_sub(1)).rev() {
                    if selected(&self.elements[i]) && !selected(&self.elements[i + 1]) {
                        self.elements.swap(i, i + 1);
                    }
                }
            }
            ZOrder::SendBackward => {
                for i in 1..self.elements.len() {
                    if selected(&self.elements[i]) && !selected(&self.elements[i - 1]) {
                        self.elements.swap(i, i - 1);
                    }
                }
            }
        }
    }

    /// Groups the elements `ids` (two or more) into one group element
    /// `group_id`, in the place of the topmost of them; its frame is the box
    /// around them. `None` when fewer than two of `ids` are on the slide.
    pub fn group(&mut self, ids: &[u64], group_id: u64) -> Option<u64> {
        let members: Vec<usize> = self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, e)| ids.contains(&e.id))
            .map(|(i, _)| i)
            .collect();
        if members.len() < 2 {
            return None;
        }
        let top = *members.last()?;
        // The group stands where the topmost member stood, among the others.
        let at = top + 1 - members.len();
        let mut children = Vec::with_capacity(members.len());
        for &i in members.iter().rev() {
            children.push(self.elements.remove(i));
        }
        children.reverse();
        let frame = Frame::union(children.iter().map(|c| &c.frame))?;
        self.elements
            .insert(at, Element::new(group_id, frame, ElementKind::Group { children }));
        Some(group_id)
    }

    /// Ungroups `group_id`: its children take its place in the z-order.
    /// Returns the children's ids (empty when it is no group).
    pub fn ungroup(&mut self, group_id: u64) -> Vec<u64> {
        let Some(at) = self.index_of(group_id) else {
            return Vec::new();
        };
        if !matches!(self.elements[at].kind, ElementKind::Group { .. }) {
            return Vec::new();
        }
        let group = self.elements.remove(at);
        let ElementKind::Group { children } = group.kind else {
            return Vec::new();
        };
        let ids = children.iter().map(|c| c.id).collect();
        for (k, child) in children.into_iter().enumerate() {
            self.elements.insert(at + k, child);
        }
        ids
    }

    /// The click steps of the slide's builds: each step is the ids of the
    /// elements that animate on that click, in order.
    #[must_use]
    pub fn build_steps(&self) -> Vec<Vec<u64>> {
        let mut builds: Vec<(u32, usize, u64)> = self
            .elements
            .iter()
            .enumerate()
            .filter_map(|(i, e)| e.animation.map(|a| (a.order, i, e.id)))
            .collect();
        builds.sort_by_key(|&(order, i, _)| (order, i));
        let mut steps: Vec<Vec<u64>> = Vec::new();
        let mut last = None;
        for (order, _, id) in builds {
            if last == Some(order) {
                if let Some(step) = steps.last_mut() {
                    step.push(id);
                }
            } else {
                steps.push(vec![id]);
                last = Some(order);
            }
        }
        steps
    }

    /// Whether `element` is on screen after `step` clicks of the builds.
    #[must_use]
    pub fn visible_at(&self, element: &Element, step: usize) -> bool {
        let Some(build) = element.animation else {
            return true;
        };
        let Some(played_on) = self
            .build_steps()
            .iter()
            .position(|s| s.contains(&element.id))
        else {
            return true;
        };
        match build.effect.class() {
            AnimationClass::Entrance => step > played_on,
            AnimationClass::Exit => step <= played_on,
            AnimationClass::Emphasis => true,
        }
    }

    /// The build step (0-based click) on which `element` plays, if any.
    #[must_use]
    pub fn build_step_of(&self, element: u64) -> Option<usize> {
        self.build_steps().iter().position(|s| s.contains(&element))
    }
}

/// What makes an element on one slide "the same" as one on the next for the
/// Morph transition: its placeholder role, else its kind and content (the
/// shapes of a duplicated slide keep both, under new ids).
fn morph_signature(e: &Element) -> String {
    if let Some(role) = e.placeholder {
        return format!("placeholder:{role:?}");
    }
    match &e.kind {
        ElementKind::Text { body } => format!("text:{}", body.text()),
        ElementKind::Shape { shape, body, .. } => format!("shape:{shape:?}:{}", body.text()),
        ElementKind::Image { media, .. } => format!("image:{media}"),
        ElementKind::Table { rows, .. } => {
            format!("table:{}", rows.first().map(|r| r.join("\t")).unwrap_or_default())
        }
        ElementKind::Chart { chart, title } => format!("chart:{chart:?}:{title}"),
        ElementKind::Video { media } => format!("video:{media}"),
        ElementKind::Group { children } => format!(
            "group:{}",
            children.iter().map(morph_signature).collect::<Vec<_>>().join("|")
        ),
    }
}

/// The elements the Morph transition from `from` to `to` moves instead of
/// fading: pairs of indices (into `from.elements`, into `to.elements`), in
/// the z-order of `to`. The same id first, then the same placeholder role or
/// the same kind and content; every element is in one pair at most.
#[must_use]
pub fn morph_pairs(from: &Slide, to: &Slide) -> Vec<(usize, usize)> {
    let mut used = vec![false; from.elements.len()];
    let mut pairs = Vec::new();
    let mut open = Vec::new();
    for (j, b) in to.elements.iter().enumerate() {
        match from.elements.iter().position(|a| a.id == b.id) {
            Some(i) if !used[i] => {
                used[i] = true;
                pairs.push((i, j));
            }
            _ => open.push(j),
        }
    }
    let signatures: Vec<String> = from.elements.iter().map(morph_signature).collect();
    for j in open {
        let wanted = morph_signature(&to.elements[j]);
        if let Some(i) = (0..from.elements.len()).find(|&i| !used[i] && signatures[i] == wanted) {
            used[i] = true;
            pairs.push((i, j));
        }
    }
    pairs.sort_by_key(|&(_, j)| j);
    pairs
}

// ==== Deck ====

/// The whole presentation, as `deck.json` holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deck {
    pub format: String,
    pub version: u32,
    /// The deck's folder name: `show/<id>/`.
    pub id: String,
    pub title: String,
    pub size: SlideSize,
    pub theme: Theme,
    pub slides: Vec<Slide>,
    /// The next id [`Deck::mint`] hands out (slides and elements share it).
    pub next_id: u64,
}

impl Deck {
    /// A deck `id` with one title slide.
    #[must_use]
    pub fn new(id: &str, title: &str, theme: Theme, size: SlideSize) -> Self {
        let mut deck = Self {
            format: DECK_FORMAT.to_string(),
            version: DECK_VERSION,
            id: id.to_string(),
            title: title.to_string(),
            size,
            theme,
            slides: Vec::new(),
            next_id: 1,
        };
        deck.add_slide(0, LayoutKind::TitleSlide);
        deck
    }

    /// A fresh id for a slide or an element.
    pub fn mint(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Inserts a new slide of `layout` at `at` (clamped); returns its index.
    pub fn add_slide(&mut self, at: usize, layout: LayoutKind) -> usize {
        let at = at.min(self.slides.len());
        let id = self.mint();
        let mut next = self.next_id;
        let slide = Slide::new(id, layout, self.size, &mut || {
            let v = next;
            next += 1;
            v
        });
        self.next_id = next;
        self.slides.insert(at, slide);
        at
    }

    /// New ids for `element` and everything inside it.
    fn renumber(&mut self, element: &mut Element) {
        element.id = self.mint();
        if let ElementKind::Group { children } = &mut element.kind {
            for child in children.iter_mut() {
                self.renumber(child);
            }
        }
    }

    /// A copy of `element` with new ids (a paste, Ctrl+D).
    pub fn copy_element(&mut self, element: &Element) -> Element {
        let mut copy = element.clone();
        self.renumber(&mut copy);
        copy
    }

    /// A copy of slide `index` (new ids) right after it; returns its index.
    pub fn duplicate_slide(&mut self, index: usize) -> Option<usize> {
        let mut copy = self.slides.get(index)?.clone();
        copy.id = self.mint();
        copy.section = None;
        for element in copy.elements.iter_mut() {
            self.renumber(element);
        }
        self.slides.insert(index + 1, copy);
        Some(index + 1)
    }

    /// Deletes the slides `indices`; the deck keeps at least one slide.
    pub fn delete_slides(&mut self, indices: &[usize]) {
        let mut doomed: Vec<usize> = indices.to_vec();
        doomed.sort_unstable();
        doomed.dedup();
        for i in doomed.into_iter().rev() {
            if i < self.slides.len() && self.slides.len() > 1 {
                let gone = self.slides.remove(i);
                // A section that started on the deleted slide starts on the next one.
                if let (Some(section), Some(next)) = (gone.section, self.slides.get_mut(i)) {
                    if next.section.is_none() {
                        next.section = Some(section);
                    }
                }
            }
        }
    }

    /// Moves the slides `indices` (keeping their order) so they stand
    /// before the slide that was at `to` (`to == len` = the end). Returns
    /// their new indices.
    pub fn move_slides(&mut self, indices: &[usize], to: usize) -> Vec<usize> {
        let mut picked: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| i < self.slides.len())
            .collect();
        picked.sort_unstable();
        picked.dedup();
        if picked.is_empty() {
            return Vec::new();
        }
        let to = to.min(self.slides.len());
        let before = picked.iter().filter(|&&i| i < to).count();
        let mut moving = Vec::with_capacity(picked.len());
        for &i in picked.iter().rev() {
            moving.push(self.slides.remove(i));
        }
        moving.reverse();
        let at = to - before;
        let n = moving.len();
        for (k, slide) in moving.into_iter().enumerate() {
            self.slides.insert(at + k, slide);
        }
        (at..at + n).collect()
    }

    /// Gives slide `index` the layout `layout`: the content of every
    /// placeholder moves to the new layout's placeholder of the same role
    /// (a body to the first body, a heading to the first heading...); a
    /// placeholder the new layout has no role for stays where it is when it
    /// holds text and goes when empty; new roles get empty placeholders.
    pub fn apply_layout(&mut self, index: usize, layout: LayoutKind) {
        if index >= self.slides.len() {
            return;
        }
        let specs = layout.placeholders(self.size);
        let elements = core::mem::take(&mut self.slides[index].elements);
        let (mut old, free): (Vec<Element>, Vec<Element>) =
            elements.into_iter().partition(|e| e.placeholder.is_some());
        let mut out = Vec::with_capacity(specs.len() + old.len() + free.len());
        for spec in &specs {
            let found = old
                .iter()
                .position(|e| e.placeholder == Some(spec.role))
                .or_else(|| {
                    spec.role
                        .stand_in()
                        .and_then(|r| old.iter().position(|e| e.placeholder == Some(r)))
                });
            match found {
                Some(i) => {
                    let mut e = old.remove(i);
                    e.frame = spec.frame;
                    e.placeholder = Some(spec.role);
                    if let Some(body) = e.body_mut() {
                        body.size = spec.size;
                        body.valign = spec.valign;
                        body.prompt = spec.role.prompt().to_string();
                        for p in &mut body.paragraphs {
                            p.bullet = spec.bullets;
                            p.align = spec.align;
                        }
                    }
                    out.push(e);
                }
                None => {
                    let id = self.mint();
                    out.push(spec.element(id));
                }
            }
        }
        // Placeholders the new layout has no room for: kept when they hold
        // something, dropped when empty.
        out.extend(
            old.into_iter()
                .filter(|e| e.body().map_or(true, |b| !b.is_empty())),
        );
        out.extend(free);
        let slide = &mut self.slides[index];
        slide.elements = out;
        slide.layout = layout;
    }

    /// Puts slide `index`'s placeholders back where its layout puts them.
    pub fn reset_slide(&mut self, index: usize) {
        let Some(slide) = self.slides.get_mut(index) else {
            return;
        };
        let specs = slide.layout.placeholders(self.size);
        for e in &mut slide.elements {
            let Some(role) = e.placeholder else {
                continue;
            };
            if let Some(spec) = specs.iter().find(|s| s.role == role) {
                e.set_frame(spec.frame);
                if let Some(body) = e.body_mut() {
                    body.size = spec.size;
                    body.valign = spec.valign;
                }
            }
        }
    }

    /// Changes the slide size: every frame scales horizontally with the
    /// width (the height is the same in both sizes).
    pub fn set_size(&mut self, size: SlideSize) {
        fn scale(e: &mut Element, k: f32) {
            e.frame.x *= k;
            e.frame.w *= k;
            if let ElementKind::Group { children } = &mut e.kind {
                for c in children.iter_mut() {
                    scale(c, k);
                }
            }
        }
        let k = size.width() / self.size.width();
        for slide in &mut self.slides {
            for e in &mut slide.elements {
                scale(e, k);
            }
        }
        self.size = size;
    }

    /// The indices of the slides the show visits, in order.
    #[must_use]
    pub fn shown_slides(&self) -> Vec<usize> {
        (0..self.slides.len()).filter(|&i| !self.slides[i].hidden).collect()
    }

    /// The section each slide is in (`None` before the first section).
    #[must_use]
    pub fn section_of(&self, index: usize) -> Option<&str> {
        self.slides[..=index.min(self.slides.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|s| s.section.as_deref())
    }

    /// `deck.json`.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Reads `deck.json`; a file of another format or a newer version is refused.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let deck: Deck = serde_json::from_str(text).map_err(|e| format!("deck.json: {e}"))?;
        if deck.format != DECK_FORMAT {
            return Err(format!("not a deck file (format {:?})", deck.format));
        }
        if deck.version > DECK_VERSION {
            return Err(format!(
                "deck.json version {} is newer than this app reads ({DECK_VERSION})",
                deck.version
            ));
        }
        Ok(deck)
    }
}

// ==== The slide show ====

/// Where the show is: the slide, the number of build steps played on it,
/// and the blank screen (B: black, W: white) if one is up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowState {
    pub slide: usize,
    pub step: usize,
    pub blank: Option<Blank>,
    pub ended: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blank {
    Black,
    White,
}

/// What one show key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowMove {
    /// A build step played (or was taken back) on the same slide.
    Step,
    /// Another slide is up.
    Slide,
    /// Nothing to go to.
    Stay,
    /// The show is over (past the last slide).
    End,
}

impl ShowState {
    /// The show from slide `from` (the next shown slide at or after it).
    #[must_use]
    pub fn start(deck: &Deck, from: usize) -> Self {
        let shown = deck.shown_slides();
        let slide = shown
            .iter()
            .copied()
            .find(|&i| i >= from)
            .or_else(|| shown.last().copied())
            .unwrap_or(0);
        Self {
            slide,
            step: 0,
            blank: None,
            ended: false,
        }
    }

    fn steps_of(deck: &Deck, slide: usize) -> usize {
        deck.slides.get(slide).map_or(0, |s| s.build_steps().len())
    }

    /// Space / Right / Down / click: the next build step, else the next shown slide.
    pub fn next(&mut self, deck: &Deck) -> ShowMove {
        if self.blank.take().is_some() || self.ended {
            return ShowMove::Stay;
        }
        if self.step < Self::steps_of(deck, self.slide) {
            self.step += 1;
            return ShowMove::Step;
        }
        match self.upcoming(deck) {
            Some(next) => {
                self.slide = next;
                self.step = 0;
                ShowMove::Slide
            }
            None => {
                self.ended = true;
                ShowMove::End
            }
        }
    }

    /// Left / Up / Backspace: the build step before, else the previous
    /// shown slide with all its steps played.
    pub fn prev(&mut self, deck: &Deck) -> ShowMove {
        if self.blank.take().is_some() {
            return ShowMove::Stay;
        }
        if self.ended {
            self.ended = false;
            return ShowMove::Slide;
        }
        if self.step > 0 {
            self.step -= 1;
            return ShowMove::Step;
        }
        match deck
            .shown_slides()
            .into_iter()
            .rev()
            .find(|&i| i < self.slide)
        {
            Some(prev) => {
                self.slide = prev;
                self.step = Self::steps_of(deck, prev);
                ShowMove::Slide
            }
            None => ShowMove::Stay,
        }
    }

    /// The presenter's strip or a number + Enter: slide `index`, no builds played.
    pub fn goto(&mut self, deck: &Deck, index: usize) {
        if index < deck.slides.len() {
            self.slide = index;
            self.step = 0;
            self.ended = false;
            self.blank = None;
        }
    }

    /// B / W: a black or white screen, again to take it down.
    pub fn toggle_blank(&mut self, blank: Blank) {
        self.blank = if self.blank == Some(blank) {
            None
        } else {
            Some(blank)
        };
    }

    /// The shown slide after the current one, for the presenter's "next".
    #[must_use]
    pub fn upcoming(&self, deck: &Deck) -> Option<usize> {
        deck.shown_slides().into_iter().find(|&i| i > self.slide)
    }
}

// ==== Sample deck ====

/// The "Azlin Workspace" sample deck (the plan's sample data): title,
/// agenda, bullets with builds, two-column comparison, a quote, section
/// headers, shapes, a hidden slide, notes on every slide.
#[must_use]
pub fn sample_deck(id: &str, theme: Theme) -> Deck {
    let mut deck = Deck::new(id, "Azlin Workspace", theme, SlideSize::Wide);
    let set = |deck: &mut Deck, slide: usize, role: PlaceholderRole, text: &str| {
        if let Some(e) = deck.slides[slide]
            .elements
            .iter_mut()
            .find(|e| e.placeholder == Some(role))
        {
            if let Some(body) = e.body_mut() {
                body.set_text(text);
            }
        }
    };
    set(&mut deck, 0, PlaceholderRole::Title, "Azlin Workspace");
    set(&mut deck, 0, PlaceholderRole::Subtitle, "Local-first apps on one toolkit");
    deck.slides[0].notes = String::from("Welcome everyone. Introduce the team in one sentence.");
    deck.slides[0].section = Some(String::from("Introduction"));

    let agenda = deck.add_slide(1, LayoutKind::TitleAndContent);
    set(&mut deck, agenda, PlaceholderRole::Title, "Agenda");
    set(&mut deck, agenda, PlaceholderRole::Body, "Why local-first\nThe apps\nThe toolkit\nWhat comes next");
    deck.slides[agenda].notes = String::from("Four parts, about five minutes each.");

    let why = deck.add_slide(2, LayoutKind::TitleAndContent);
    set(&mut deck, why, PlaceholderRole::Title, "Why local-first");
    set(&mut deck, why, PlaceholderRole::Body, "Your data stays with you\nWorks offline\nSyncs when you want it to");
    if let Some(body) = deck.slides[why]
        .elements
        .iter_mut()
        .find(|e| e.placeholder == Some(PlaceholderRole::Body))
    {
        body.animation = Some(Animation {
            effect: AnimationEffect::Fade,
            order: 0,
            duration_ms: 500,
        });
    }
    deck.slides[why].transition = Transition {
        kind: TransitionKind::Fade,
        duration_ms: 500,
    };
    deck.slides[why].notes = String::from("Mention the offline demo before switching to the next slide.");

    let apps = deck.add_slide(3, LayoutKind::SectionHeader);
    set(&mut deck, apps, PlaceholderRole::Title, "The apps");
    set(&mut deck, apps, PlaceholderRole::Subtitle, "Writer, Sheets, Show, Mail, Drive");
    deck.slides[apps].section = Some(String::from("The apps"));
    deck.slides[apps].notes = String::from("A short tour; the demos come later.");

    let compare = deck.add_slide(4, LayoutKind::Comparison);
    set(&mut deck, compare, PlaceholderRole::Title, "Cloud office vs. Azlin");
    set(&mut deck, compare, PlaceholderRole::Heading, "Cloud office");
    set(&mut deck, compare, PlaceholderRole::Body, "Files on someone else's server\nNeeds a connection\nSubscription");
    set(&mut deck, compare, PlaceholderRole::Heading2, "Azlin");
    set(&mut deck, compare, PlaceholderRole::Body2, "Files in your own bucket\nWorks offline\nOpen source");
    deck.slides[compare].transition = Transition {
        kind: TransitionKind::Push,
        duration_ms: 400,
    };
    deck.slides[compare].notes = String::from("Keep it fair: the cloud office is convenient.");

    let shapes = deck.add_slide(5, LayoutKind::TitleOnly);
    set(&mut deck, shapes, PlaceholderRole::Title, "How a file travels");
    let accent = deck.theme.colors.accent;
    let accent2 = deck.theme.colors.accent2;
    let ink = deck.theme.colors.background;
    let mut boxes = Vec::new();
    for (i, label) in ["Your laptop", "Your bucket", "Your phone"].iter().enumerate() {
        let id = deck.mint();
        let mut body = TextBody::plain(label, 40.0);
        body.valign = VAlign::Middle;
        body.color = Some(ink);
        if let Some(p) = body.paragraphs.first_mut() {
            p.align = Align::Center;
        }
        let x = 160.0 + i as f32 * 600.0;
        boxes.push(Element::new(
            id,
            Frame::new(x, 460.0, 400.0, 220.0),
            ElementKind::Shape {
                shape: ShapeKind::RoundRect,
                fill: Some(if i == 1 { accent2 } else { accent }),
                stroke: None,
                stroke_width: 0.0,
                body,
            },
        ));
    }
    for i in 0..2 {
        let id = deck.mint();
        let x = 580.0 + i as f32 * 600.0;
        boxes.push(Element::new(
            id,
            Frame::new(x, 540.0, 160.0, 60.0),
            ElementKind::Shape {
                shape: ShapeKind::Arrow,
                fill: Some(accent),
                stroke: None,
                stroke_width: 0.0,
                body: TextBody::default(),
            },
        ));
    }
    deck.slides[shapes].elements.extend(boxes);
    deck.slides[shapes].notes = String::from("The bucket is S3: AWS, R2 or a MinIO at home.");

    // The same picture rearranged, entered with Morph: the bucket rises to
    // the middle and grows, the devices and the arrows follow it.
    if let Some(morph) = deck.duplicate_slide(shapes) {
        let slide = &mut deck.slides[morph];
        let frames = [
            Frame::new(160.0, 700.0, 400.0, 220.0),
            Frame::new(660.0, 300.0, 600.0, 300.0),
            Frame::new(1360.0, 700.0, 400.0, 220.0),
            Frame {
                rotation: -30.0,
                ..Frame::new(470.0, 560.0, 220.0, 60.0)
            },
            Frame {
                rotation: 30.0,
                ..Frame::new(1230.0, 560.0, 220.0, 60.0)
            },
        ];
        // The title first, then the three boxes and the two arrows.
        for (element, frame) in slide.elements.iter_mut().skip(1).zip(frames) {
            element.frame = frame;
        }
        slide.transition = Transition {
            kind: TransitionKind::Morph,
            duration_ms: 900,
        };
        slide.notes = String::from("Everything goes through the bucket - Morph moves the picture.");
    }

    let quote = deck.add_slide(7, LayoutKind::Blank);
    let id = deck.mint();
    let mut body = TextBody::plain("\u{201c}The best file format is the one you can still open in twenty years.\u{201d}", 64.0);
    body.valign = VAlign::Middle;
    body.font = Some(deck.theme.fonts.heading.clone());
    if let Some(p) = body.paragraphs.first_mut() {
        p.align = Align::Center;
        if let Some(r) = p.runs.first_mut() {
            r.italic = true;
        }
    }
    deck.slides[quote]
        .elements
        .push(Element::new(id, Frame::new(200.0, 300.0, 1520.0, 480.0), ElementKind::Text { body }));
    deck.slides[quote].notes = String::from("Pause after the quote.");

    let hidden = deck.add_slide(8, LayoutKind::TitleAndContent);
    set(&mut deck, hidden, PlaceholderRole::Title, "Backup: pricing details");
    set(&mut deck, hidden, PlaceholderRole::Body, "Storage at cost\nNo seat licences");
    deck.slides[hidden].hidden = true;
    deck.slides[hidden].notes = String::from("Only if someone asks about money.");

    let next = deck.add_slide(9, LayoutKind::TitleAndContent);
    set(&mut deck, next, PlaceholderRole::Title, "What comes next");
    set(&mut deck, next, PlaceholderRole::Body, "Import .pptx\nCharts\nPresenting from the phone");
    deck.slides[next].section = Some(String::from("Outlook"));
    deck.slides[next].notes = String::from("Ask for feedback on the order.");

    let end = deck.add_slide(10, LayoutKind::TitleSlide);
    set(&mut deck, end, PlaceholderRole::Title, "Thank you");
    set(&mut deck, end, PlaceholderRole::Subtitle, "Questions?");
    deck.slides[end].notes = String::from("Leave this up during the questions.");
    deck
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> Deck {
        Deck::new("d1", "Test", Theme::office(), SlideSize::Wide)
    }

    fn text_of(slide: &Slide, role: PlaceholderRole) -> String {
        slide
            .placeholder(role)
            .and_then(Element::body)
            .map(TextBody::text)
            .unwrap_or_default()
    }

    fn set_text(slide: &mut Slide, role: PlaceholderRole, text: &str) {
        let e = slide
            .elements
            .iter_mut()
            .find(|e| e.placeholder == Some(role))
            .expect("the placeholder");
        e.body_mut().expect("a text body").set_text(text);
    }

    fn shape(deck: &mut Deck, x: f32) -> Element {
        let id = deck.mint();
        Element::new(
            id,
            Frame::new(x, 100.0, 100.0, 50.0),
            ElementKind::Shape {
                shape: ShapeKind::Rect,
                fill: Some(Color::rgb(1, 2, 3)),
                stroke: None,
                stroke_width: 0.0,
                body: TextBody::default(),
            },
        )
    }

    fn ids(slide: &Slide) -> Vec<u64> {
        slide.elements.iter().map(|e| e.id).collect()
    }

    // ---- layouts ----

    #[test]
    fn every_layout_but_blank_has_a_title_and_its_placeholders_lie_on_the_slide() {
        for size in [SlideSize::Wide, SlideSize::Standard] {
            for layout in LayoutKind::ALL {
                let specs = layout.placeholders(size);
                let has_title = specs.iter().any(|s| s.role == PlaceholderRole::Title);
                assert_eq!(has_title, layout != LayoutKind::Blank, "{layout:?}");
                for s in &specs {
                    let f = s.frame;
                    assert!(f.w > 0.0 && f.h > 0.0, "{layout:?} {:?}: empty", s.role);
                    assert!(f.x >= 0.0 && f.y >= 0.0, "{layout:?} {:?}: off the slide", s.role);
                    assert!(f.x + f.w <= size.width() + 0.5, "{layout:?} {:?}: too wide", s.role);
                    assert!(f.y + f.h <= size.height() + 0.5, "{layout:?} {:?}: too tall", s.role);
                }
                let mut roles: Vec<_> = specs.iter().map(|s| s.role).collect();
                roles.dedup();
                assert_eq!(roles.len(), specs.len(), "{layout:?}: a role twice");
            }
        }
    }

    #[test]
    fn the_layouts_place_the_placeholders_powerpoint_places() {
        let roles = |l: LayoutKind| -> Vec<PlaceholderRole> {
            l.placeholders(SlideSize::Wide).iter().map(|s| s.role).collect()
        };
        use PlaceholderRole::*;
        assert_eq!(roles(LayoutKind::TitleSlide), vec![Title, Subtitle]);
        assert_eq!(roles(LayoutKind::TitleAndContent), vec![Title, Body]);
        assert_eq!(roles(LayoutKind::SectionHeader), vec![Title, Subtitle]);
        assert_eq!(roles(LayoutKind::TwoContent), vec![Title, Body, Body2]);
        assert_eq!(roles(LayoutKind::Comparison), vec![Title, Heading, Body, Heading2, Body2]);
        assert_eq!(roles(LayoutKind::TitleOnly), vec![Title]);
        assert!(roles(LayoutKind::Blank).is_empty());
        let body = LayoutKind::TitleAndContent
            .placeholders(SlideSize::Wide)
            .into_iter()
            .find(|s| s.role == Body)
            .expect("a body");
        assert!(body.bullets, "a content body holds bullets");
    }

    #[test]
    fn a_new_slide_gets_its_layouts_empty_placeholders_with_fresh_ids() {
        let mut d = deck();
        let at = d.add_slide(1, LayoutKind::TwoContent);
        assert_eq!(at, 1);
        assert_eq!(d.slides.len(), 2);
        let slide = &d.slides[1];
        assert_eq!(slide.layout, LayoutKind::TwoContent);
        assert_eq!(slide.elements.len(), 3);
        assert!(slide.elements.iter().all(|e| e.body().is_some_and(TextBody::is_empty)));
        let mut all: Vec<u64> = d.slides.iter().flat_map(|s| s.elements.iter().map(|e| e.id)).collect();
        all.extend(d.slides.iter().map(|s| s.id));
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n, "every id is unique");
        assert_eq!(text_of(slide, PlaceholderRole::Body), "");
        assert_eq!(
            slide.placeholder(PlaceholderRole::Title).and_then(Element::body).map(|b| b.prompt.clone()),
            Some(String::from("Click to add title"))
        );
    }

    #[test]
    fn re_applying_a_layout_keeps_the_content_by_role() {
        let mut d = deck();
        let i = d.add_slide(1, LayoutKind::TitleAndContent);
        set_text(&mut d.slides[i], PlaceholderRole::Title, "Why local-first");
        set_text(&mut d.slides[i], PlaceholderRole::Body, "Offline\nYours");
        d.apply_layout(i, LayoutKind::TwoContent);
        let slide = &d.slides[i];
        assert_eq!(slide.layout, LayoutKind::TwoContent);
        assert_eq!(text_of(slide, PlaceholderRole::Title), "Why local-first");
        assert_eq!(text_of(slide, PlaceholderRole::Body), "Offline\nYours");
        assert_eq!(text_of(slide, PlaceholderRole::Body2), "", "the new role is an empty placeholder");
        let body2 = LayoutKind::TwoContent
            .placeholders(SlideSize::Wide)
            .into_iter()
            .find(|s| s.role == PlaceholderRole::Body)
            .expect("a body");
        assert_eq!(
            slide.placeholder(PlaceholderRole::Body).map(|e| e.frame),
            Some(body2.frame),
            "the kept content moves to the new layout's frame"
        );

        // Back to Title Only: the body has text, so it stays (as PowerPoint
        // keeps orphaned content); the empty Body2 goes.
        d.apply_layout(i, LayoutKind::TitleOnly);
        let slide = &d.slides[i];
        assert_eq!(text_of(slide, PlaceholderRole::Title), "Why local-first");
        assert_eq!(text_of(slide, PlaceholderRole::Body), "Offline\nYours");
        assert!(slide.placeholder(PlaceholderRole::Body2).is_none());
    }

    #[test]
    fn a_layout_change_keeps_the_free_elements_and_their_z_order() {
        let mut d = deck();
        let s = shape(&mut d, 10.0);
        let sid = s.id;
        d.slides[0].elements.push(s);
        d.apply_layout(0, LayoutKind::TitleAndContent);
        assert!(d.slides[0].element(sid).is_some());
        assert_eq!(d.slides[0].elements.last().map(|e| e.id), Some(sid), "the shape stays on top");
    }

    #[test]
    fn reset_puts_the_placeholders_back_where_the_layout_has_them() {
        let mut d = deck();
        let title = d.slides[0].placeholder(PlaceholderRole::Title).expect("title").clone();
        if let Some(e) = d.slides[0].element_mut(title.id) {
            e.frame = Frame::new(5.0, 5.0, 50.0, 50.0);
            e.frame.rotation = 30.0;
        }
        d.reset_slide(0);
        assert_eq!(d.slides[0].element(title.id).map(|e| e.frame), Some(title.frame));
    }

    // ---- slides ----

    #[test]
    fn moving_slides_keeps_their_order_and_reports_where_they_went() {
        let mut d = deck();
        for _ in 0..4 {
            let n = d.slides.len();
            d.add_slide(n, LayoutKind::Blank);
        }
        let order: Vec<u64> = d.slides.iter().map(|s| s.id).collect();
        // [a b c d e] - move b and d before a
        let moved = d.move_slides(&[1, 3], 0);
        let now: Vec<u64> = d.slides.iter().map(|s| s.id).collect();
        assert_eq!(now, vec![order[1], order[3], order[0], order[2], order[4]]);
        assert_eq!(moved, vec![0, 1]);
        // move the first to the end
        let moved = d.move_slides(&[0], d.slides.len());
        let now: Vec<u64> = d.slides.iter().map(|s| s.id).collect();
        assert_eq!(now, vec![order[3], order[0], order[2], order[4], order[1]]);
        assert_eq!(moved, vec![4]);
        // dropping a slide on itself changes nothing
        let before = d.slides.clone();
        assert_eq!(d.move_slides(&[2], 2), vec![2]);
        assert_eq!(d.slides, before);
    }

    #[test]
    fn duplicating_a_slide_copies_it_with_new_ids_and_deleting_keeps_one_slide() {
        let mut d = deck();
        set_text(&mut d.slides[0], PlaceholderRole::Title, "Hello");
        let copy = d.duplicate_slide(0).expect("a copy");
        assert_eq!(copy, 1);
        assert_eq!(text_of(&d.slides[1], PlaceholderRole::Title), "Hello");
        assert_ne!(d.slides[1].id, d.slides[0].id);
        for (a, b) in d.slides[0].elements.iter().zip(&d.slides[1].elements) {
            assert_ne!(a.id, b.id, "an element of the copy has a new id");
        }
        d.delete_slides(&[0, 1]);
        assert_eq!(d.slides.len(), 1, "the deck keeps one slide");
    }

    #[test]
    fn a_new_slide_size_scales_every_frame_horizontally() {
        let mut d = deck();
        let before = d.slides[0].elements[0].frame;
        d.set_size(SlideSize::Standard);
        let after = d.slides[0].elements[0].frame;
        let k = 1440.0 / 1920.0;
        assert!((after.x - before.x * k).abs() < 0.01);
        assert!((after.w - before.w * k).abs() < 0.01);
        assert_eq!(after.y, before.y);
        assert_eq!(after.h, before.h);
        assert_eq!(d.size, SlideSize::Standard);
    }

    // ---- z-order ----

    #[test]
    fn z_order_moves_the_selection_and_keeps_its_own_order() {
        let mut d = deck();
        d.slides[0].elements.clear();
        let shapes: Vec<Element> = (0..5).map(|i| shape(&mut d, i as f32)).collect();
        let [a, b, c, e, f] = [shapes[0].id, shapes[1].id, shapes[2].id, shapes[3].id, shapes[4].id];
        d.slides[0].elements = shapes;
        let slide = &mut d.slides[0];

        slide.reorder(&[b, c], ZOrder::BringToFront);
        assert_eq!(ids(slide), vec![a, e, f, b, c]);
        slide.reorder(&[f, c], ZOrder::SendToBack);
        assert_eq!(ids(slide), vec![f, c, a, e, b]);
        slide.reorder(&[c], ZOrder::BringForward);
        assert_eq!(ids(slide), vec![f, a, c, e, b], "one step up, past one neighbour");
        slide.reorder(&[a, c], ZOrder::BringForward);
        assert_eq!(ids(slide), vec![f, e, a, c, b], "a selected pair steps over the next unselected");
        slide.reorder(&[b], ZOrder::BringForward);
        assert_eq!(ids(slide), vec![f, e, a, c, b], "the top stays the top");
        slide.reorder(&[e], ZOrder::SendBackward);
        assert_eq!(ids(slide), vec![e, f, a, c, b]);
        slide.reorder(&[e], ZOrder::SendBackward);
        assert_eq!(ids(slide), vec![e, f, a, c, b], "the bottom stays the bottom");
    }

    // ---- groups ----

    #[test]
    fn grouping_replaces_the_members_with_one_group_at_the_topmost_place() {
        let mut d = deck();
        d.slides[0].elements.clear();
        let mut shapes: Vec<Element> = (0..4).map(|i| shape(&mut d, i as f32 * 200.0)).collect();
        shapes[2].frame = Frame::new(400.0, 300.0, 100.0, 100.0);
        let all: Vec<u64> = shapes.iter().map(|s| s.id).collect();
        d.slides[0].elements = shapes;
        let gid = d.mint();
        let slide = &mut d.slides[0];
        assert_eq!(slide.group(&[all[0]], gid), None, "one element is no group");
        assert_eq!(slide.group(&[all[0], all[2]], gid), Some(gid));
        assert_eq!(ids(slide), vec![all[1], gid, all[3]], "the group stands where the topmost member stood");
        let group = slide.element(gid).expect("the group");
        assert_eq!(group.frame, Frame::new(0.0, 100.0, 500.0, 300.0), "the box around the members");
        match &group.kind {
            ElementKind::Group { children } => {
                assert_eq!(children.iter().map(|c| c.id).collect::<Vec<_>>(), vec![all[0], all[2]]);
            }
            other => panic!("not a group: {other:?}"),
        }

        // Moving the group moves its members.
        let mut moved = group.frame;
        moved.x += 10.0;
        moved.y += 20.0;
        slide.element_mut(gid).expect("the group").set_frame(moved);
        // Resizing it scales them.
        let mut wider = moved;
        wider.w *= 2.0;
        slide.element_mut(gid).expect("the group").set_frame(wider);
        let children = match &slide.element(gid).expect("the group").kind {
            ElementKind::Group { children } => children.clone(),
            _ => unreachable!(),
        };
        assert_eq!(children[0].frame, Frame::new(10.0, 120.0, 200.0, 50.0));
        assert_eq!(children[1].frame, Frame::new(810.0, 320.0, 200.0, 100.0));

        let back = slide.ungroup(gid);
        assert_eq!(back, vec![all[0], all[2]]);
        assert_eq!(ids(slide), vec![all[1], all[0], all[2], all[3]], "the members take the group's place");
        assert_eq!(slide.element(all[2]).map(|e| e.frame), Some(Frame::new(810.0, 320.0, 200.0, 100.0)));
        assert!(slide.ungroup(all[1]).is_empty(), "a shape is no group");
    }

    #[test]
    fn a_rotated_member_counts_with_its_rotated_box() {
        let f = Frame {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
            rotation: 45.0,
        };
        let b = f.bounds();
        let half_diagonal = 50.0 * 2f32.sqrt();
        assert!((b.w - 2.0 * half_diagonal).abs() < 0.01, "{b:?}");
        assert!((b.x - (50.0 - half_diagonal)).abs() < 0.01, "{b:?}");
    }

    // ---- builds and the show ----

    fn deck_with_builds() -> Deck {
        let mut d = deck();
        let i = d.add_slide(1, LayoutKind::TitleAndContent);
        let a = shape(&mut d, 0.0);
        let b = shape(&mut d, 1.0);
        let (aid, bid) = (a.id, b.id);
        d.slides[i].elements.push(a);
        d.slides[i].elements.push(b);
        d.slides[i].element_mut(bid).expect("b").animation = Some(Animation {
            effect: AnimationEffect::FlyIn,
            order: 0,
            duration_ms: 300,
        });
        d.slides[i].element_mut(aid).expect("a").animation = Some(Animation {
            effect: AnimationEffect::FadeOut,
            order: 1,
            duration_ms: 300,
        });
        let hidden = d.add_slide(2, LayoutKind::Blank);
        d.slides[hidden].hidden = true;
        d.add_slide(3, LayoutKind::Blank);
        d
    }

    #[test]
    fn builds_play_in_their_order_entrances_appear_and_exits_leave() {
        let d = deck_with_builds();
        let slide = &d.slides[1];
        let (a, b) = (slide.elements[2].id, slide.elements[3].id);
        assert_eq!(slide.build_steps(), vec![vec![b], vec![a]]);
        let ea = slide.element(a).expect("a");
        let eb = slide.element(b).expect("b");
        assert!(!slide.visible_at(eb, 0), "an entrance waits for its click");
        assert!(slide.visible_at(ea, 0), "an exit is there until its click");
        assert!(slide.visible_at(eb, 1));
        assert!(slide.visible_at(ea, 1));
        assert!(!slide.visible_at(ea, 2), "the exit played");
        assert!(slide.visible_at(&slide.elements[0], 0), "an element without a build is always there");
    }

    #[test]
    fn the_show_steps_through_builds_skips_hidden_slides_and_ends() {
        let d = deck_with_builds();
        let mut show = ShowState::start(&d, 0);
        assert_eq!((show.slide, show.step), (0, 0));
        assert_eq!(show.upcoming(&d), Some(1));
        assert_eq!(show.next(&d), ShowMove::Slide);
        assert_eq!((show.slide, show.step), (1, 0));
        assert_eq!(show.next(&d), ShowMove::Step);
        assert_eq!(show.next(&d), ShowMove::Step);
        assert_eq!((show.slide, show.step), (1, 2));
        assert_eq!(show.upcoming(&d), Some(3), "the hidden slide is skipped");
        assert_eq!(show.next(&d), ShowMove::Slide);
        assert_eq!(show.slide, 3);
        assert_eq!(show.prev(&d), ShowMove::Slide);
        assert_eq!((show.slide, show.step), (1, 2), "back onto a slide with its builds played");
        assert_eq!(show.prev(&d), ShowMove::Step);
        assert_eq!(show.step, 1);
        show.slide = 3;
        show.step = 0;
        assert_eq!(show.next(&d), ShowMove::End);
        assert!(show.ended);
        let mut first = ShowState::start(&d, 0);
        assert_eq!(first.prev(&d), ShowMove::Stay);
        assert_eq!(ShowState::start(&d, 2).slide, 3, "starting on a hidden slide starts at the next shown one");
        first.toggle_blank(Blank::Black);
        assert_eq!(first.blank, Some(Blank::Black));
        first.toggle_blank(Blank::Black);
        assert_eq!(first.blank, None);
        first.toggle_blank(Blank::White);
        assert_eq!(first.next(&d), ShowMove::Stay, "a key on a blank screen takes it down first");
        assert_eq!(first.blank, None);
    }

    // ---- the file ----

    #[test]
    fn a_deck_survives_the_json_round_trip() {
        let d = sample_deck("6f1c", Theme::office());
        let json = d.to_json();
        assert!(json.contains("\"format\": \"azshow.deck\""), "{}", &json[..json.len().min(200)]);
        assert!(json.contains("\"type\": \"shape\""));
        assert!(json.contains("#2b579a"), "colours are hex strings");
        let back = Deck::from_json(&json).expect("the deck reads back");
        assert_eq!(back, d);
    }

    #[test]
    fn a_file_of_another_format_or_a_newer_version_is_refused() {
        let d = deck();
        let mut other = d.clone();
        other.format = String::from("something.else");
        assert!(Deck::from_json(&other.to_json()).is_err());
        let mut newer = d.clone();
        newer.version = DECK_VERSION + 1;
        assert!(Deck::from_json(&newer.to_json()).is_err());
        assert!(Deck::from_json("{ not json").is_err());
    }

    #[test]
    fn colours_read_and_write_as_hex() {
        assert_eq!(Color::parse("#2b579a"), Some(Color::rgb(0x2b, 0x57, 0x9a)));
        assert_eq!(Color::parse("#fff"), Some(Color::rgb(255, 255, 255)));
        assert_eq!(Color::parse("#00000080"), Some(Color::rgba(0, 0, 0, 0x80)));
        assert_eq!(Color::parse("blue"), None);
        assert_eq!(Color::rgba(1, 2, 3, 4).hex(), "#01020304");
    }

    #[test]
    fn the_sample_deck_has_sections_a_hidden_slide_builds_and_notes_everywhere() {
        let d = sample_deck("s", Theme::office());
        assert_eq!(d.slides.len(), 11);
        assert!(d.slides.iter().all(|s| !s.notes.is_empty()), "notes on every slide");
        assert_eq!(d.slides.iter().filter(|s| s.hidden).count(), 1);
        assert_eq!(d.section_of(0), Some("Introduction"));
        assert_eq!(d.section_of(4), Some("The apps"));
        assert!(d.slides.iter().any(|s| !s.build_steps().is_empty()));
        assert_eq!(d.slides[1].title(), "Agenda");
        assert_eq!(d.shown_slides().len(), 10);
        // Slide 7 morphs out of slide 6: every object of it is one of slide 6's, moved.
        assert_eq!(d.slides[6].transition.kind, TransitionKind::Morph);
        let pairs = morph_pairs(&d.slides[5], &d.slides[6]);
        assert_eq!(pairs.len(), d.slides[6].elements.len());
        assert!(pairs.iter().any(|&(i, j)| d.slides[5].elements[i].frame != d.slides[6].elements[j].frame));
    }

    // ---- the Morph transition ----

    #[test]
    fn morph_pairs_the_same_objects_and_leaves_the_others_to_fade() {
        let mut d = deck();
        let a = d.add_slide(1, LayoutKind::TitleOnly);
        let left = shape(&mut d, 100.0);
        let right = shape(&mut d, 900.0);
        let mut other = shape(&mut d, 500.0);
        if let ElementKind::Shape { shape, .. } = &mut other.kind {
            *shape = ShapeKind::Ellipse;
        }
        d.slides[a].elements.extend([left, right, other]);
        let b = d.duplicate_slide(a).expect("the copy");
        // On the copy the two rectangles swap places and the oval goes.
        let copy = &mut d.slides[b];
        copy.elements.remove(3);
        copy.elements[1].frame.x = 900.0;
        copy.elements[2].frame.x = 100.0;
        // New ids, the same title and shapes: the title and both rectangles
        // pair (in order), the oval fades out.
        assert_ne!(d.slides[a].elements[1].id, d.slides[b].elements[1].id);
        assert_eq!(morph_pairs(&d.slides[a], &d.slides[b]), vec![(0, 0), (1, 1), (2, 2)]);
        // A slide with its own ids pairs by id first, whatever moved.
        let same = d.slides[a].clone();
        let mut shuffled = same.clone();
        shuffled.elements.reverse();
        assert_eq!(morph_pairs(&same, &shuffled), vec![(3, 0), (2, 1), (1, 2), (0, 3)]);
    }

    #[test]
    fn a_frame_and_a_colour_go_part_of_the_way() {
        let a = Frame::new(0.0, 0.0, 100.0, 50.0);
        let b = Frame {
            rotation: 90.0,
            ..Frame::new(200.0, 100.0, 300.0, 150.0)
        };
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
        let half = a.lerp(&b, 0.5);
        assert_eq!((half.x, half.y, half.w, half.h, half.rotation), (100.0, 50.0, 200.0, 100.0, 45.0));
        assert_eq!(a.lerp(&b, 7.0), b, "clamped");
        let c = Color::rgb(0, 100, 200).lerp(Color::rgb(200, 100, 0), 0.5);
        assert_eq!(c, Color::rgb(100, 100, 100));
    }

    #[test]
    fn set_text_keeps_the_bullets_of_the_paragraphs() {
        let mut body = TextBody::default();
        body.paragraphs.push(Paragraph {
            runs: Vec::new(),
            align: Align::Left,
            bullet: true,
            level: 0,
        });
        body.set_text("one\ntwo");
        assert_eq!(body.paragraphs.len(), 2);
        assert!(body.paragraphs.iter().all(|p| p.bullet));
        assert_eq!(body.text(), "one\ntwo");
    }
}
