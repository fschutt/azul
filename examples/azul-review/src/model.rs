//! The review model.
//!
//! THE INK IS THE SOURCE OF TRUTH. Every structured finding in this file is
//! DERIVED from strokes — never the other way round. That inversion is the
//! whole point: existing review tools make you produce structure first (click
//! a line, type a comment) and offer drawing as decoration. On paper the order
//! is reversed, and the paper workflow is the one that actually works.

use azul::prelude::*;

/// What a stroke MEANS, chosen by the toolbar or a pad ExpressKey.
///
/// Deliberately small. On paper the grammar that emerged unprompted was two
/// layers — highlighter marks *what*, pen says *something about it* — so the
/// palette encodes intent, not decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Semantic {
    /// Highlighter. Marks a REGION of interest and nothing more.
    Scope,
    /// "This is wrong."
    Issue,
    /// "Why is this like this?" — lands in the open-questions queue.
    Question,
    /// "This duplicates something else." Two of these can be linked.
    Duplicate,
    /// "This is good" — worth capturing; reviews that only ever say
    /// `issue:` train models to be uniformly negative.
    Praise,
}

impl Semantic {
    pub const ALL: [Self; 5] = [
        Self::Scope,
        Self::Issue,
        Self::Question,
        Self::Duplicate,
        Self::Praise,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Scope => "scope",
            Self::Issue => "issue",
            Self::Question => "question",
            Self::Duplicate => "duplicate",
            Self::Praise => "praise",
        }
    }

    /// The ink colour. `Scope` is the pink highlighter from the paper sheets;
    /// the rest are pen colours.
    pub const fn color(self) -> ColorU {
        match self {
            Self::Scope => ColorU {
                r: 255,
                g: 64,
                b: 152,
                a: 255,
            },
            Self::Issue => ColorU {
                r: 214,
                g: 45,
                b: 32,
                a: 255,
            },
            Self::Question => ColorU {
                r: 32,
                g: 92,
                b: 214,
                a: 255,
            },
            Self::Duplicate => ColorU {
                r: 150,
                g: 60,
                b: 200,
                a: 255,
            },
            Self::Praise => ColorU {
                r: 24,
                g: 140,
                b: 70,
                a: 255,
            },
        }
    }

    /// Material icon name for the palette button.
    ///
    /// A glyph rather than the word: five spelled-out labels ate most of the
    /// toolbar, and the palette is picked by muscle memory and pad key anyway —
    /// the word was only ever read once.
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Scope => "highlight",
            Self::Issue => "report_problem",
            Self::Question => "help_outline",
            Self::Duplicate => "difference",
            Self::Praise => "star",
        }
    }

    /// Highlighter strokes are wide and translucent and go UNDER the glyphs;
    /// pen strokes are narrow, opaque, and go over them. This single bool is
    /// what makes the two-layer grammar work.
    pub const fn is_highlighter(self) -> bool {
        matches!(self, Self::Scope)
    }
}

/// One sampled point of a stroke. Pressure/tilt come straight from `PenState`
/// and drive the metaball field, so a tilted pen paints a directional dab.
#[derive(Debug, Clone, Copy)]
pub struct InkPoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
}

/// A single continuous pen-down..pen-up stroke, in PAGE-LOCAL logical pixels.
///
/// Page-local, not viewport-local, on purpose: the page is the stable frame of
/// reference. Scrolling, zooming or re-paginating must not move ink relative
/// to the code it annotates.
#[derive(Debug, Clone)]
pub struct Stroke {
    pub page: usize,
    pub semantic: Semantic,
    pub points: Vec<InkPoint>,
    /// Monotonic id, also the stroke's temporal order — clustering uses it.
    pub id: u64,
    /// Which annotation this stroke belongs to.
    ///
    /// Bumped by the idle timer, NOT inferred from geometry. Whether two marks
    /// are one remark or two is a fact about how they were made, and the pause
    /// between them is the only honest record of it — a spatial rule cannot
    /// tell a second thought about the same line from a correction of the
    /// first, and both happen constantly.
    pub epoch: u64,
}

impl Stroke {
    /// Axis-aligned bounds in page-local pixels, used for spatial clustering
    /// and for inferring which lines the stroke covers.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &self.points {
            b.0 = b.0.min(p.x);
            b.1 = b.1.min(p.y);
            b.2 = b.2.max(p.x);
            b.3 = b.3.max(p.y);
        }
        if self.points.is_empty() {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            b
        }
    }
}

/// A finding DERIVED from a cluster of strokes.
///
/// Nothing constructs this directly. It is produced by `derive_findings`, and
/// re-derived whenever the ink changes — so editing ink edits the finding, and
/// the two can never disagree.
#[derive(Debug, Clone)]
pub struct Finding {
    pub semantic: Semantic,
    pub file: String,
    /// Inferred from where the ink sits, NOT declared by the user. This is
    /// "lazy anchoring": you never say "lines 899-911", the tool works it out.
    pub first_line: usize,
    pub last_line: usize,
    /// Spoken rationale bound to this cluster, if the mic was recording while
    /// the strokes were drawn.
    pub voice_note: Option<String>,
    /// How many strokes produced it — a cheap confidence signal.
    pub stroke_count: usize,
    /// The annotation burst this came from. Two findings can share a line
    /// range and still be separate remarks; the epoch is what says so.
    pub epoch: u64,
}

/// Audio captured while drawing, bound to the strokes made in that window.
///
/// Held as raw samples: transcription is out of scope here, but the binding
/// (WHICH strokes this audio belongs to) is the part that must be captured
/// live and cannot be reconstructed afterwards.
#[derive(Debug, Default)]
pub struct VoiceClip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    /// Stroke ids made while this clip was recording.
    pub stroke_ids: Vec<u64>,
}

/// Which nib is in hand.
///
/// On paper the two-layer grammar came from physically swapping pens, and the
/// swap is the friction: you put one down to pick another up. Here a left
/// click cycles, so the tool changes without the hand leaving the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// Fat translucent nib. Marks a REGION and forces `Semantic::Scope` —
    /// a marker cannot say anything, only point.
    Marker,
    /// Small pointy nib. Carries whatever semantic is selected.
    Pen,
    /// Pointy nib that also RECORDS while it draws, binding the audio to the
    /// strokes made in that window. The terse mark is the headline; the spoken
    /// part is the reasoning nobody wants to write by hand.
    AudioPen,
}

impl Tool {
    pub const ALL: [Self; 3] = [Self::Marker, Self::Pen, Self::AudioPen];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Marker => "marker",
            Self::Pen => "pen",
            Self::AudioPen => "audio pen",
        }
    }

    /// Material icon name for the nib readout.
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Marker => "brush",
            Self::Pen => "draw",
            Self::AudioPen => "record_voice_over",
        }
    }

    /// Next tool in the cycle.
    pub const fn next(self) -> Self {
        match self {
            Self::Marker => Self::Pen,
            Self::Pen => Self::AudioPen,
            Self::AudioPen => Self::Marker,
        }
    }

    /// The marker always means "region"; the others carry the palette choice.
    pub const fn semantic_for(self, selected: Semantic) -> Semantic {
        match self {
            Self::Marker => Semantic::Scope,
            Self::Pen | Self::AudioPen => selected,
        }
    }

    pub const fn records_audio(self) -> bool {
        matches!(self, Self::AudioPen)
    }
}
