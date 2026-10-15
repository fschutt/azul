//! Text into pixels: a string set in a font and rasterised into an RGBA8
//! [`RawImage`] - what a photo editor's text tool, a watermark or a label
//! drawn into a picture needs (the DOM draws text into the WINDOW; this draws
//! it into an image the app owns).
//!
//! One path, no twin: the lines are shaped by the text engine
//! (`text3::default::shape_text_for_parsed_font`: kerning, ligatures, marks)
//! and their glyphs rasterised by the CPU renderer's own glyph path
//! (`raster::render_text`) in GRAYSCALE coverage - the target is
//! transparent, so there is no background for LCD subpixels to sit on.
//!
//! - [`text_image`] / [`text_image_with`]: a straight-alpha RGBA8 image as
//!   big as the lines' boxes (`RawImage::from_text` in the C API).
//! - [`draw_text`] / [`draw_text_with`]: the same pixels composited into an
//!   RGBA8 / BGRA8 image (straight or premultiplied) with the first line's
//!   box at (`x`, `y`) (`RawImage::draw_text`).
//!
//! The `_with` forms take the font cache (tests hand in a memory font); the
//! plain forms use a font cache of the system fonts built once per thread
//! on first use (a scan of the font folders - the first call takes that
//! long, so an app that draws text in a callback may warm it on a thread).

use azul_core::resources::{OptionRawImage, RawImage, RawImageData, RawImageFormat};
use azul_css::{props::basic::ColorU, AzString};

/// The largest em size and the largest side of a text image, in pixels.
pub const TEXT_RASTER_MAX_SIZE_PX: f32 = 4096.0;
pub const TEXT_RASTER_MAX_SIDE_PX: u32 = 16384;

/// Where a set text's lines sit in their box ([`TextRasterStyle::with_align`]).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum TextRasterAlign {
    /// At the start of the line: the left of a left-to-right paragraph, the
    /// right of a right-to-left one.
    #[default]
    Start,
    /// In the middle.
    Center,
    /// At the end of the line: the right of a left-to-right paragraph.
    End,
    /// At the left, whatever the direction.
    Left,
    /// At the right, whatever the direction.
    Right,
}

/// The base direction of a set text's paragraphs
/// ([`TextRasterStyle::with_direction`]).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub enum TextRasterDirection {
    /// Each paragraph's first strong letter decides (Unicode bidi P2 / P3):
    /// Hebrew or Arabic runs right to left, Latin left to right.
    #[default]
    Auto,
    /// Left to right.
    Ltr,
    /// Right to left.
    Rtl,
}

/// How [`text_image`] / [`draw_text`] set a text: the font, its size, the
/// line spacing and the colour, the box it wraps in, where its lines sit
/// and its direction.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct TextRasterStyle {
    /// A family name: `sans-serif`, `serif`, `monospace` or an installed
    /// family. A family that is not there falls back to one that covers the
    /// text.
    pub font_family: AzString,
    /// The em size in image pixels.
    pub size_px: f32,
    /// The distance between baselines as a multiple of the font's own line
    /// height (1.0 = the font's).
    pub line_height: f32,
    /// The ink colour; its alpha scales the coverage.
    pub color: ColorU,
    /// The bold face of the family (the regular one where it has none).
    pub bold: bool,
    /// The italic face of the family (the regular one where it has none).
    pub italic: bool,
    /// The width the lines wrap in, in image pixels: they break at spaces
    /// (a word wider than it between its letters), and the image is this
    /// wide. 0 (or not a finite positive number): no wrapping, the image is
    /// as wide as the widest line.
    pub max_width: f32,
    /// Where each line sits in the box (or in the widest line).
    pub align: TextRasterAlign,
    /// The paragraphs' base direction.
    pub direction: TextRasterDirection,
}

impl TextRasterStyle {
    /// `font_family` at `size_px`, in `color`, the font's own line height.
    #[must_use]
    pub const fn create(font_family: AzString, size_px: f32, color: ColorU) -> Self {
        Self {
            font_family,
            size_px,
            line_height: 1.0,
            color,
            bold: false,
            italic: false,
            max_width: 0.0,
            align: TextRasterAlign::Start,
            direction: TextRasterDirection::Auto,
        }
    }

    /// Wraps the lines in `max_width` image pixels ([`Self::max_width`]).
    #[must_use]
    pub const fn with_max_width(mut self, max_width: f32) -> Self {
        self.max_width = max_width;
        self
    }

    /// Sets each line `align` in its box.
    #[must_use]
    pub const fn with_align(mut self, align: TextRasterAlign) -> Self {
        self.align = align;
        self
    }

    /// Sets the paragraphs' base direction.
    #[must_use]
    pub const fn with_direction(mut self, direction: TextRasterDirection) -> Self {
        self.direction = direction;
        self
    }

    /// The wrapping width in use, `None` for no wrapping.
    fn wrap_width(&self) -> Option<f32> {
        (self.max_width.is_finite() && self.max_width > 0.0).then_some(self.max_width)
    }

    #[must_use]
    pub const fn with_bold(mut self, bold: bool) -> Self {
        self.bold = bold;
        self
    }

    #[must_use]
    pub const fn with_italic(mut self, italic: bool) -> Self {
        self.italic = italic;
        self
    }

    /// Baselines `line_height` times the font's line height apart.
    #[must_use]
    pub const fn with_line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    /// The line-height multiple in use: a non-positive or non-finite one is 1.
    fn line_factor(&self) -> f32 {
        if self.line_height.is_finite() && self.line_height > 0.0 {
            self.line_height
        } else {
            1.0
        }
    }
}

/// A set text: PREMULTIPLIED RGBA8 rows (the renderer's own format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPixels {
    pub width: u32,
    pub height: u32,
    pub rgba_premultiplied: Vec<u8>,
}

// ==== Setting and rasterising (needs fonts) ====

#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
mod set {
    use azul_core::{
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::RendererResources,
        ui_solver::GlyphInstance,
    };
    use azul_css::props::basic::FontRef;
    use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, OwnedFontSource, PatternMatch};

    use super::{
        TextPixels, TextRasterAlign, TextRasterStyle, TEXT_RASTER_MAX_SIDE_PX,
        TEXT_RASTER_MAX_SIZE_PX,
    };
    use crate::{
        cpurender::AzulPixmap,
        font::parsed::ParsedFont,
        glyph_cache::GlyphCache,
        text3::{
            cache::{BidiDirection, FontHash, FontManager, StyleProperties},
            default::shape_text_for_parsed_font,
            script::{detect_script, Language, Script},
        },
    };

    /// The face for `style`: the family with the asked weight and slant,
    /// then the family as it is, then `sans-serif`; each through
    /// fontconfig's own fallback ladder (exact, family-relaxed, coverage).
    fn resolve_font(fc: &FcFontCache, style: &TextRasterStyle) -> Option<ParsedFont> {
        let family = style.font_family.as_str().trim();
        let family = if family.is_empty() {
            "sans-serif"
        } else {
            family
        };
        let styled = FcPattern {
            family: Some(family.to_string()),
            weight: if style.bold {
                FcWeight::Bold
            } else {
                FcWeight::Normal
            },
            bold: if style.bold {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            italic: if style.italic {
                PatternMatch::True
            } else {
                PatternMatch::DontCare
            },
            ..Default::default()
        };
        let plain = FcPattern {
            family: Some(family.to_string()),
            ..Default::default()
        };
        let generic = FcPattern {
            family: Some("sans-serif".to_string()),
            ..Default::default()
        };
        let mut trace = Vec::new();
        let matched = [styled, plain, generic]
            .iter()
            .find_map(|p| fc.query_with_fallback(p, &mut trace))?;
        let bytes = fc.get_font_bytes(&matched.id)?;
        let index = fc.get_font_by_id(&matched.id).map_or(0, |src| match src {
            OwnedFontSource::Disk(path) => path.font_index,
            OwnedFontSource::Memory(font) => font.font_index,
        });
        Some(
            ParsedFont::from_bytes(bytes.as_slice(), index, &mut Vec::new())?
                .with_source_bytes(bytes.clone()),
        )
    }

    /// One run of a set line in one direction: its glyphs in reading order.
    struct SetRun {
        glyphs: Vec<crate::text3::cache::Glyph>,
        /// Right to left: placed last glyph first.
        rtl: bool,
    }

    /// One line of a set text: its runs in visual order (left to right), how
    /// wide they are, and its paragraph's direction (for `Start` / `End`).
    struct SetLine {
        runs: Vec<SetRun>,
        width: f32,
        rtl: bool,
    }

    /// `text` shaped in `direction` with the text engine, `None` when it does
    /// not shape.
    fn shape(
        parsed: &ParsedFont,
        text: &str,
        rtl: bool,
        shaping: &StyleProperties,
    ) -> Option<Vec<crate::text3::cache::Glyph>> {
        if text.is_empty() {
            return Some(Vec::new());
        }
        let script = detect_script(text).unwrap_or(Script::Latin);
        let direction = if rtl {
            BidiDirection::Rtl
        } else {
            BidiDirection::Ltr
        };
        shape_text_for_parsed_font(
            parsed,
            text,
            script,
            Language::EnglishUS,
            direction,
            shaping,
        )
        .ok()
    }

    /// How far `glyphs` move the pen.
    fn advance(glyphs: &[crate::text3::cache::Glyph]) -> f32 {
        glyphs.iter().map(|g| g.advance + g.kerning).sum()
    }

    /// The byte ranges of `paragraph` (from `start` to `end` in it) its
    /// lines hold when they wrap in `max` (no wrapping when `None`): lines
    /// break after the spaces between words, the spaces at a break hang
    /// (left out of the line), and a word wider than `max` breaks between
    /// its letters. The last line keeps everything to `end`.
    fn wrap(
        paragraph: &str,
        start: usize,
        end: usize,
        max: Option<f32>,
        measure: &dyn Fn(&str) -> f32,
    ) -> Vec<core::ops::Range<usize>> {
        let Some(max) = max else {
            return vec![start..end];
        };
        let is_space = |c: char| c == ' ' || c == '\t';
        // (word start, word end, end after its spaces)
        let mut words = Vec::new();
        let text = &paragraph[start..end];
        let mut at = 0;
        while at < text.len() {
            let word_end = text[at..]
                .find(is_space)
                .map_or(text.len(), |i| at + i);
            let spaces_end = text[word_end..]
                .find(|c: char| !is_space(c))
                .map_or(text.len(), |i| word_end + i);
            words.push((start + at, start + word_end, start + spaces_end));
            at = spaces_end;
        }
        let slack = 0.01;
        let mut lines = Vec::new();
        let mut line_start = start;
        let mut line_end = start;
        let mut line_width = 0.0_f32;
        for (word_start, word_end, spaces_end) in words {
            let word = measure(&paragraph[word_start..word_end]);
            let spaces = measure(&paragraph[word_end..spaces_end]);
            if line_end > line_start && line_width + word > max + slack {
                lines.push(line_start..line_end);
                line_start = word_start;
                line_width = 0.0;
            }
            if line_width <= 0.0 && word > max + slack {
                // Between its letters: as many as fit on each line.
                let mut piece_start = word_start;
                let mut piece_width = 0.0_f32;
                for (i, ch) in paragraph[word_start..word_end].char_indices() {
                    let at = word_start + i;
                    let letter = measure(&paragraph[at..at + ch.len_utf8()]);
                    if piece_width > 0.0 && piece_width + letter > max + slack {
                        lines.push(piece_start..at);
                        piece_start = at;
                        piece_width = 0.0;
                    }
                    piece_width += letter;
                }
                line_start = piece_start;
                line_width = piece_width + spaces;
                line_end = word_end;
                continue;
            }
            line_width += word + spaces;
            line_end = word_end;
        }
        lines.push(line_start..end);
        lines
    }

    /// The lines of `text` set in `style`: its paragraphs (at `\n`), each in
    /// its direction (`style.direction`, `Auto` from its first strong
    /// letter), wrapped in `style.max_width`, each line's runs in the visual
    /// order of the Unicode bidi algorithm, shaped in their own direction.
    #[allow(clippy::too_many_lines)] // one pass: paragraphs, lines, runs
    fn set_lines(
        parsed: &ParsedFont,
        text: &str,
        style: &TextRasterStyle,
        shaping: &StyleProperties,
    ) -> Option<Vec<SetLine>> {
        use unicode_bidi::{BidiInfo, Level};

        use super::TextRasterDirection;

        let measure = |s: &str| shape(parsed, s, false, shaping).map_or(0.0, |g| advance(&g));
        let level = match style.direction {
            TextRasterDirection::Auto => None,
            TextRasterDirection::Ltr => Some(Level::ltr()),
            TextRasterDirection::Rtl => Some(Level::rtl()),
        };
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let paragraph = paragraph.strip_suffix('\r').unwrap_or(paragraph);
            if paragraph.is_empty() {
                lines.push(SetLine {
                    runs: Vec::new(),
                    width: 0.0,
                    rtl: style.direction == TextRasterDirection::Rtl,
                });
                continue;
            }
            let bidi = BidiInfo::new(paragraph, level);
            for info in &bidi.paragraphs {
                let rtl = info.level.is_rtl();
                for range in wrap(
                    paragraph,
                    info.range.start,
                    info.range.end,
                    style.wrap_width(),
                    &measure,
                ) {
                    if range.is_empty() {
                        lines.push(SetLine {
                            runs: Vec::new(),
                            width: 0.0,
                            rtl,
                        });
                        continue;
                    }
                    let (levels, visual) = bidi.visual_runs(info, range.clone());
                    let mut runs = Vec::with_capacity(visual.len());
                    let mut width = 0.0_f32;
                    for run in visual {
                        let run_rtl = levels.get(run.start).is_some_and(|l| l.is_rtl());
                        let glyphs = shape(parsed, &paragraph[run.clone()], run_rtl, shaping)?;
                        width += advance(&glyphs);
                        runs.push(SetRun {
                            glyphs,
                            rtl: run_rtl,
                        });
                    }
                    lines.push(SetLine { runs, width, rtl });
                }
            }
        }
        Some(lines)
    }

    /// `text` set in `style` with the first line's box at (`dx`, `dy`) (the
    /// fractional offset of a draw), as premultiplied RGBA8 as big as the
    /// lines' boxes plus that offset. `None`: no text, no usable size, no
    /// font, or an image larger than [`TEXT_RASTER_MAX_SIDE_PX`].
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    #[must_use]
    pub fn rasterize_text_at(
        fc: &FcFontCache,
        text: &str,
        style: &TextRasterStyle,
        dx: f32,
        dy: f32,
    ) -> Option<TextPixels> {
        let size = style.size_px;
        if text.is_empty() || !size.is_finite() || size <= 0.0 || size > TEXT_RASTER_MAX_SIZE_PX {
            return None;
        }
        let parsed = resolve_font(fc, style)?;
        let upm = f32::from(parsed.font_metrics.units_per_em);
        if upm <= 0.0 {
            return None;
        }
        let scale = size / upm;
        let ascent = parsed.font_metrics.ascent * scale;
        let descent = parsed.font_metrics.descent.abs() * scale;
        let gap = parsed.font_metrics.line_gap.max(0.0) * scale;
        let line_box = ascent + descent + gap;
        if !line_box.is_finite() || line_box <= 0.0 {
            return None;
        }
        let step = line_box * style.line_factor();
        let shaping = StyleProperties {
            font_size_px: size,
            ..StyleProperties::default()
        };

        let set_lines = set_lines(&parsed, text, style, &shaping)?;
        // The box the lines align in: the wrapping width, else the widest
        // line.
        let width = style.wrap_width().unwrap_or_else(|| {
            set_lines
                .iter()
                .map(|line| line.width)
                .fold(0.0_f32, f32::max)
        });
        let mut glyphs = Vec::new();
        for (i, line) in set_lines.iter().enumerate() {
            let baseline = dy + i as f32 * step + ascent;
            let free = (width - line.width).max(0.0);
            let offset = match (style.align, line.rtl) {
                (TextRasterAlign::Left, _)
                | (TextRasterAlign::Start, false)
                | (TextRasterAlign::End, true) => 0.0,
                (TextRasterAlign::Right, _)
                | (TextRasterAlign::Start, true)
                | (TextRasterAlign::End, false) => free,
                (TextRasterAlign::Center, _) => free * 0.5,
            };
            let mut pen = dx + offset;
            for run in &line.runs {
                // A right-to-left run's glyphs come in reading order: its
                // last one stands leftmost.
                let mut place = |g: &crate::text3::cache::Glyph| {
                    glyphs.push(GlyphInstance {
                        index: u32::from(g.glyph_id),
                        point: LogicalPosition {
                            x: pen + g.offset.x,
                            y: baseline - g.offset.y,
                        },
                        size: LogicalSize {
                            width: g.advance,
                            height: size,
                        },
                    });
                    pen += g.advance + g.kerning;
                };
                if run.rtl {
                    run.glyphs.iter().rev().for_each(&mut place);
                } else {
                    run.glyphs.iter().for_each(&mut place);
                }
            }
        }
        let lines = set_lines.len();
        let height = line_box + lines.saturating_sub(1) as f32 * step;
        let w = (width + dx).ceil();
        let h = (height + dy).ceil();
        if !(w.is_finite() && h.is_finite())
            || w > TEXT_RASTER_MAX_SIDE_PX as f32
            || h > TEXT_RASTER_MAX_SIDE_PX as f32
        {
            return None;
        }
        let (w, h) = ((w as u32).max(1), (h as u32).max(1));
        let mut pixmap = AzulPixmap::new(w, h)?;
        pixmap.fill(0, 0, 0, 0);
        if !glyphs.is_empty() {
            // The face, registered in a one-font manager the glyph path
            // resolves it from (`render_text_run_to_pixmap`'s way).
            let font_ref: FontRef = crate::parsed_font_to_font_ref(parsed);
            let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
            let fm: FontManager<FontRef> = FontManager::new(FcFontCache::default()).ok()?;
            fm.insert_font(rust_fontconfig::FontId::new(), font_ref);
            let clip_rect = LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize {
                    width: w as f32,
                    height: h as f32,
                },
            };
            let mut glyph_cache = GlyphCache::new();
            super::super::raster::render_text(
                &glyphs,
                FontHash::from_hash(hash),
                size,
                style.color,
                &mut pixmap,
                &clip_rect,
                None,
                &RendererResources::default(),
                &fm,
                1.0,
                &mut glyph_cache,
                (0.0, 0.0),
                // Transparent target: grayscale coverage, never LCD.
                true,
            );
        }
        Some(TextPixels {
            width: w,
            height: h,
            rgba_premultiplied: pixmap.data.to_vec(),
        })
    }

    /// The text of one SVG document, drawn straight into its pixmap: in the
    /// faces its `@font-face`s embed (by the family they declare), else in
    /// the installed family a `<text>` names. A face is looked up once per
    /// document and the glyph outlines are kept across its texts, so a PDF
    /// page of a thousand runs in three fonts makes three faces.
    pub(crate) struct SvgTextPainter {
        /// The faces the document embeds, by the family their `@font-face`
        /// declares.
        embedded: Vec<(String, FontRef)>,
        /// Installed faces looked up so far, by (family, bold, italic);
        /// `None` where nothing matched, so a miss is not looked up again.
        installed: Vec<((String, bool, bool), Option<FontRef>)>,
        /// The manager the glyph path resolves the faces from, made with the
        /// first text drawn.
        fonts: Option<FontManager<FontRef>>,
        /// The hashes of the faces `fonts` holds.
        registered: Vec<u64>,
        /// Glyph outlines, kept across the document's texts.
        glyph_cache: GlyphCache,
    }

    impl SvgTextPainter {
        /// A painter for a document that embeds `embedded`.
        #[must_use]
        pub(crate) fn new(embedded: Vec<(String, FontRef)>) -> Self {
            Self {
                embedded,
                installed: Vec::new(),
                fonts: None,
                registered: Vec::new(),
                glyph_cache: GlyphCache::new(),
            }
        }

        /// The face for the CSS family list `families`: the first family the
        /// document embeds, else the first family as an installed one (with
        /// fontconfig's fallback, `sans-serif` last).
        fn face(&mut self, families: &str, bold: bool, italic: bool) -> Option<FontRef> {
            let names: Vec<&str> = families
                .split(',')
                .map(|f| f.trim().trim_matches(|c| c == '"' || c == '\''))
                .filter(|f| !f.is_empty())
                .collect();
            for name in &names {
                if let Some((_, font)) = self
                    .embedded
                    .iter()
                    .find(|(f, _)| f.eq_ignore_ascii_case(name))
                {
                    return Some(font.clone());
                }
            }
            let family = names.first().map_or("sans-serif", |f| *f).to_string();
            let key = (family.clone(), bold, italic);
            if let Some((_, found)) = self.installed.iter().find(|(k, _)| *k == key) {
                return found.clone();
            }
            let style = TextRasterStyle::create(
                azul_css::AzString::from(family),
                16.0,
                azul_css::props::basic::ColorU::rgb(0, 0, 0),
            )
            .with_bold(bold)
            .with_italic(italic);
            let found = with_process_fonts(|fc| resolve_font(fc, &style))
                .map(crate::parsed_font_to_font_ref);
            self.installed.push((key, found.clone()));
            found
        }

        /// `text`, one line in `families` at `size_px`, its baseline starting
        /// at (`x`, `y`) in `pixmap`'s pixels, in `color`. Draws nothing when
        /// no face is found or the text does not shape.
        #[allow(clippy::too_many_arguments)] // one text run's whole description
        #[allow(clippy::cast_precision_loss)] // pixmap sides are far below 2^24
        pub(crate) fn draw(
            &mut self,
            pixmap: &mut AzulPixmap,
            text: &str,
            families: &str,
            bold: bool,
            italic: bool,
            size_px: f32,
            color: azul_css::props::basic::ColorU,
            x: f32,
            y: f32,
        ) {
            if text.is_empty()
                || !size_px.is_finite()
                || size_px <= 0.0
                || size_px > TEXT_RASTER_MAX_SIZE_PX
                || !(x.is_finite() && y.is_finite())
            {
                return;
            }
            let Some(font) = self.face(families, bold, italic) else {
                return;
            };
            let parsed = crate::font_ref_to_parsed_font(&font);
            let shaping = StyleProperties {
                font_size_px: size_px,
                ..StyleProperties::default()
            };
            let script = detect_script(text).unwrap_or(Script::Latin);
            let Ok(shaped) = shape_text_for_parsed_font(
                parsed,
                text,
                script,
                Language::EnglishUS,
                BidiDirection::Ltr,
                &shaping,
            ) else {
                return;
            };
            let mut glyphs = Vec::with_capacity(shaped.len());
            let mut pen = x;
            for g in &shaped {
                glyphs.push(GlyphInstance {
                    index: u32::from(g.glyph_id),
                    point: LogicalPosition {
                        x: pen + g.offset.x,
                        y: y - g.offset.y,
                    },
                    size: LogicalSize {
                        width: g.advance,
                        height: size_px,
                    },
                });
                pen += g.advance + g.kerning;
            }
            if glyphs.is_empty() {
                return;
            }
            let hash = parsed.hash;
            if self.fonts.is_none() {
                self.fonts = FontManager::new(FcFontCache::default()).ok();
            }
            let Some(fonts) = self.fonts.as_ref() else {
                return;
            };
            if !self.registered.contains(&hash) {
                fonts.insert_font(rust_fontconfig::FontId::new(), font.clone());
                self.registered.push(hash);
            }
            let clip_rect = LogicalRect {
                origin: LogicalPosition { x: 0.0, y: 0.0 },
                size: LogicalSize {
                    width: pixmap.width as f32,
                    height: pixmap.height as f32,
                },
            };
            super::super::raster::render_text(
                &glyphs,
                FontHash::from_hash(hash),
                size_px,
                color,
                pixmap,
                &clip_rect,
                None,
                &RendererResources::default(),
                fonts,
                1.0,
                &mut self.glyph_cache,
                (0.0, 0.0),
                // Coverage only: the page may be transparent under its text.
                true,
            );
        }
    }

    /// `f` with the installed fonts for SVG text, scanned once per PROCESS:
    /// the threads that draw pages come and go (a viewer's render jobs), and
    /// a scan per thread would cost each of them the scan of every font
    /// folder.
    #[cfg(not(target_arch = "wasm32"))]
    fn with_process_fonts<R>(f: impl FnOnce(&FcFontCache) -> R) -> R {
        static FONTS: std::sync::OnceLock<FcFontCache> = std::sync::OnceLock::new();
        f(FONTS.get_or_init(crate::font::loading::build_font_cache))
    }

    /// One thread on the web: the thread's own scan is the process's.
    #[cfg(target_arch = "wasm32")]
    fn with_process_fonts<R>(f: impl FnOnce(&FcFontCache) -> R) -> R {
        with_system_fonts(f)
    }

    thread_local! {
        /// The system fonts, scanned once per thread on first use.
        static SYSTEM_FONTS: core::cell::OnceCell<FcFontCache> = const { core::cell::OnceCell::new() };
    }

    /// `f` with this thread's cache of the system fonts.
    pub fn with_system_fonts<R>(f: impl FnOnce(&FcFontCache) -> R) -> R {
        SYSTEM_FONTS.with(|cell| f(cell.get_or_init(crate::font::loading::build_font_cache)))
    }
}

#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
pub(crate) use set::SvgTextPainter;
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
pub use set::{rasterize_text_at, with_system_fonts};

// ==== Into images ====

/// Straight-alpha RGBA8 rows from premultiplied ones.
#[must_use]
pub fn unpremultiply_rgba(premultiplied: &[u8]) -> Vec<u8> {
    let mut out = premultiplied.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a == 0 {
            px.copy_from_slice(&[0, 0, 0, 0]);
        } else if a < 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    out
}

/// `text` set in `style` as a straight-alpha RGBA8 image as big as its
/// lines' boxes (the widest line, every line's box), with the fonts of
/// `fc`. `None`: no text, no usable size or no font.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
#[must_use]
pub fn text_image_with(
    fc: &rust_fontconfig::FcFontCache,
    text: &str,
    style: &TextRasterStyle,
) -> Option<RawImage> {
    let set = rasterize_text_at(fc, text, style, 0.0, 0.0)?;
    let rgba = unpremultiply_rgba(&set.rgba_premultiplied);
    Some(RawImage::create_rgba8(
        set.width,
        set.height,
        rgba.into(),
        false,
    ))
}

/// The pixels of `text` in `style` composited into `image` with the first
/// line's box at (`x`, `y`), with the fonts of `fc`: source-over, into an
/// RGBA8 or BGRA8 image (straight or premultiplied alpha). `false`: an image
/// of another format, or nothing to draw.
#[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
pub fn draw_text_with(
    fc: &rust_fontconfig::FcFontCache,
    image: &mut RawImage,
    text: &str,
    style: &TextRasterStyle,
    x: f32,
    y: f32,
) -> bool {
    let bgr = match image.data_format {
        RawImageFormat::RGBA8 => false,
        RawImageFormat::BGRA8 => true,
        _ => return false,
    };
    if !(x.is_finite() && y.is_finite()) {
        return false;
    }
    let (ox, oy) = (x.floor(), y.floor());
    let Some(set) = rasterize_text_at(fc, text, style, x - ox, y - oy) else {
        return false;
    };
    let (iw, ih) = (image.width as i64, image.height as i64);
    let premultiplied = image.premultiplied_alpha;
    let RawImageData::U8(ref mut pixels) = image.pixels else {
        return false;
    };
    let dst = pixels.as_mut();
    let (ox, oy) = (ox as i64, oy as i64);
    for sy in 0..i64::from(set.height) {
        let ty = oy + sy;
        if ty < 0 || ty >= ih {
            continue;
        }
        for sx in 0..i64::from(set.width) {
            let tx = ox + sx;
            if tx < 0 || tx >= iw {
                continue;
            }
            let si = ((sy * i64::from(set.width) + sx) * 4) as usize;
            let di = ((ty * iw + tx) * 4) as usize;
            if di + 4 > dst.len() || si + 4 > set.rgba_premultiplied.len() {
                continue;
            }
            let s = &set.rgba_premultiplied[si..si + 4];
            if s[3] == 0 {
                continue;
            }
            let (ri, bi) = if bgr { (di + 2, di) } else { (di, di + 2) };
            let idx = [ri, di + 1, bi];
            let sa = f32::from(s[3]) / 255.0;
            let da = f32::from(dst[di + 3]) / 255.0;
            let inv = 1.0 - sa;
            let out_a = sa + da * inv;
            for k in 0..3 {
                let sc = f32::from(s[k]) / 255.0;
                let dc = f32::from(dst[idx[k]]) / 255.0;
                let c = if premultiplied {
                    sc + dc * inv
                } else if out_a > 0.0 {
                    (sc + dc * da * inv) / out_a
                } else {
                    0.0
                };
                dst[idx[k]] = (c * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            dst[di + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    true
}

/// [`text_image_with`] with the system fonts (`RawImage::from_text`).
#[must_use]
pub fn text_image(text: AzString, style: TextRasterStyle) -> OptionRawImage {
    #[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
    {
        with_system_fonts(|fc| text_image_with(fc, text.as_str(), &style)).into()
    }
    #[cfg(not(all(feature = "std", feature = "text_layout", feature = "font_loading")))]
    {
        let _ = (text, style);
        OptionRawImage::None
    }
}

/// [`draw_text_with`] with the system fonts (`RawImage::draw_text`).
pub fn draw_text(
    image: &mut RawImage,
    text: AzString,
    style: TextRasterStyle,
    x: f32,
    y: f32,
) -> bool {
    #[cfg(all(feature = "std", feature = "text_layout", feature = "font_loading"))]
    {
        with_system_fonts(|fc| draw_text_with(fc, image, text.as_str(), &style, x, y))
    }
    #[cfg(not(all(feature = "std", feature = "text_layout", feature = "font_loading")))]
    {
        let _ = (image, text, style, x, y);
        false
    }
}
