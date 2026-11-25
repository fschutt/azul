use azul::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Semantic {
    Scope,
    Issue,
    Question,
    Duplicate,
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

    /// The meaning a session file's English name stands for.
    #[must_use]
    pub fn from_label(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.label() == name)
    }

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

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Scope => "highlight",
            Self::Issue => "report_problem",
            Self::Question => "help_outline",
            Self::Duplicate => "difference",
            Self::Praise => "star",
        }
    }

    pub const fn is_highlighter(self) -> bool {
        matches!(self, Self::Scope)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkPoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub page: usize,
    pub semantic: Semantic,
    pub points: Vec<InkPoint>,
    pub id: u64,
    pub epoch: u64,
}

/// How far (`x`, `y`) is from the segment `a` to `b`.
fn distance_to_segment(a: &InkPoint, b: &InkPoint, x: f32, y: f32) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((x - a.x) * dx + (y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    (a.x + t * dx - x).hypot(a.y + t * dy - y)
}

impl Stroke {
    /// Whether (`x`, `y`) is within `tolerance` px of the stroke's line (of its dab, for one point):
    /// the eraser's hit, which a bounding box is no measure of (a long diagonal's box is a page).
    #[must_use]
    pub fn is_near(&self, x: f32, y: f32, tolerance: f32) -> bool {
        match self.points.as_slice() {
            [] => false,
            [p] => (p.x - x).hypot(p.y - y) <= tolerance,
            points => points
                .windows(2)
                .any(|w| distance_to_segment(&w[0], &w[1], x, y) <= tolerance),
        }
    }

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

#[derive(Debug, Clone)]
pub struct Finding {
    pub semantic: Semantic,
    pub file: String,
    pub first_line: usize,
    pub last_line: usize,
    pub voice_note: Option<String>,
    pub stroke_count: usize,
    pub epoch: u64,
}

/// Where a note's dictation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteState {
    /// Speech is being recognised into it: the text grows, the pen's rest is its anchor.
    Listening,
    /// The dictation is over: the text is the user's to edit.
    Done,
}

/// A note dictated at a spot of a page: the spot is its anchor (a marker on the page), the text
/// is what was said (live while it is heard, editable afterwards), and it is kept in the session
/// archive with the strokes.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: u64,
    /// The page of the open file the anchor is on.
    pub page: usize,
    /// The anchor, in the page's own coordinates (the pen layer's), px.
    pub x: f32,
    pub y: f32,
    pub text: String,
    /// The language the speech was recognised in (a BCP 47 tag; empty: the system's).
    pub language: String,
    pub state: NoteState,
}

impl Note {
    /// A note that begins listening at (`x`, `y`) of `page`.
    #[must_use]
    pub fn listening(id: u64, page: usize, x: f32, y: f32, language: &str) -> Self {
        Self {
            id,
            page,
            x,
            y,
            text: String::new(),
            language: language.to_string(),
            state: NoteState::Listening,
        }
    }
}

#[derive(Debug, Default)]
pub struct VoiceClip {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub stroke_ids: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Marker,
    Pen,
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

    pub const fn icon(self) -> &'static str {
        match self {
            Self::Marker => "brush",
            Self::Pen => "draw",
            Self::AudioPen => "record_voice_over",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Marker => Self::Pen,
            Self::Pen => Self::AudioPen,
            Self::AudioPen => Self::Marker,
        }
    }

    pub const fn prev(self) -> Self {
        match self {
            Self::Marker => Self::AudioPen,
            Self::Pen => Self::Marker,
            Self::AudioPen => Self::Pen,
        }
    }

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
