//! Text measuring from outside the layout engine: azul's `text` module (api.json), the C ABI and
//! the generated Rust API of it.
//!
//! [`TextLayout::create`] lays a text out in a stack of fonts the way the DOM's text is laid out
//! (`text3`): each grapheme cluster set in the first font of the stack that has all of its
//! characters, the bidi algorithm over the whole text (its direction from its first strong
//! letter, as `dir="auto"`, unless given), each bidi run shaped in its font (joining forms,
//! ligatures, kerning, mark positions), the lines broken at spaces when a width is given and put
//! in visual order. It answers every glyph where the layout put it - its font, its advance, the
//! offset of a mark on its letter, the text it stands for and the box of its outline - so an app
//! can measure, draw or hit-test the text itself (a PDF writer, a map's labels). The `FontRef`
//! functions of the `font` module answer a font's own measures (a glyph's index, advance and
//! box, the space's width).
//!
//! Everything here is plain `#[repr(C)]` data; [`TextLayout::create`] never unwinds (a shaper
//! panic is a `None`), so it is safe to call across the C ABI.

use std::{collections::BTreeMap, sync::Arc};

use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq,
    props::basic::{FontRef, FontRefVec},
    AzString, OptionF32, OptionU16, OptionU32, U32Vec,
};

use super::cache::{
    is_cursive_script_cluster, AvailableSpace, BidiDirection, ClusterFlags, FontStack,
    InlineContent, LayoutFragment, LoadedFonts, PositionedItem, ShapedItem, Spacing,
    StyleProperties, StyledRun, TextShapingCache, UnicodeBidi, UnifiedConstraints,
    WhiteSpaceMode,
};

/// The direction a paragraph of text reads in.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TextLayoutDirection {
    /// The direction of the text's first strong letter (`dir="auto"`): an Arabic or Hebrew text
    /// reads right to left; left to right without a strong letter.
    #[default]
    Auto,
    /// Left to right, whatever the text's letters.
    LeftToRight,
    /// Right to left, whatever the text's letters.
    RightToLeft,
}

/// How [`TextLayout::create`] lays a text out.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TextLayoutOptions {
    /// The font size, in the unit the layout's positions come in (px, mm, ...).
    pub font_size: f32,
    /// The width the lines are broken to fit, at spaces (the spaces are kept as written:
    /// `white-space: pre-wrap`). None: a line per hard break (`white-space: pre`).
    pub max_width: OptionF32,
    /// The direction the paragraph reads in.
    pub direction: TextLayoutDirection,
    /// Added after every letter (CSS `letter-spacing`).
    pub letter_spacing: f32,
    /// Added to every space (CSS `word-spacing`).
    pub word_spacing: f32,
    /// A tab's width, in spaces (CSS `tab-size`).
    pub tab_size: f32,
}

impl Default for TextLayoutOptions {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            max_width: OptionF32::None,
            direction: TextLayoutDirection::Auto,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            tab_size: 8.0,
        }
    }
}

impl TextLayoutOptions {
    /// The options for a text at `font_size`: one line per hard break, its direction from its
    /// first strong letter, no extra spacing, tabs 8 spaces wide.
    #[must_use]
    pub fn create(font_size: f32) -> Self {
        Self {
            font_size,
            ..Self::default()
        }
    }
}

/// The box of a glyph's outline: from its origin, x to the right, y up (font units from
/// `FontRef::get_glyph_bounds`, the layout's unit in a [`TextGlyph`]).
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Default)]
#[repr(C)]
pub struct GlyphBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl_option!(
    GlyphBounds,
    OptionGlyphBounds,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

/// A glyph where [`TextLayout::create`] put it.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TextGlyph {
    /// The box of its outline at the font size, from its origin (x right, y up); None for a
    /// glyph without one (a space).
    pub ink: OptionGlyphBounds,
    /// Its font: an index into the font stack the text was laid out in.
    pub font_index: u32,
    /// Where the pen stands at it, from the left edge of the layout: its advance box starts
    /// here.
    pub x: f32,
    /// How far it moves the pen on: its advance with its kerning (nothing for a mark on a
    /// letter).
    pub advance: f32,
    /// Where it is drawn from the pen (the font's GPOS: a mark on its letter): x to the right.
    pub offset_x: f32,
    /// Where it is drawn from the pen: y up.
    pub offset_y: f32,
    /// The byte range of its grapheme cluster's text in the laid out text (several glyphs of
    /// one cluster share it).
    pub cluster_start: u32,
    /// The end of that byte range (exclusive).
    pub cluster_end: u32,
    /// The glyph in its font.
    pub glyph_id: u16,
    /// It is the first glyph of its cluster in reading order (the one a copied text takes the
    /// cluster's text from).
    pub first_in_cluster: bool,
    /// Its cluster is white space (a space, a tab, a line break).
    pub whitespace: bool,
    /// It stands for a tab: `advance` is the tab's width, `glyph_id` 0.
    pub tab: bool,
    /// Its cluster is of a script whose letters join (Arabic, Syriac, N'Ko, ...): no spacing
    /// between its letters.
    pub cursive: bool,
    /// Its cluster continues the grapheme before it.
    pub grapheme_continuation: bool,
    /// Its cluster reads right to left.
    pub right_to_left: bool,
}

impl_option!(
    TextGlyph,
    OptionTextGlyph,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);
impl_vec!(
    TextGlyph,
    TextGlyphVec,
    TextGlyphVecDestructor,
    TextGlyphVecDestructorType,
    TextGlyphVecSlice,
    OptionTextGlyph
);
impl_vec_clone!(TextGlyph, TextGlyphVec, TextGlyphVecDestructor);
impl_vec_debug!(TextGlyph, TextGlyphVec);
impl_vec_partialeq!(TextGlyph, TextGlyphVec);

/// A line of a [`TextLayout`].
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextLine {
    /// Its glyphs in visual order, left to right (its spaces' too).
    pub glyphs: TextGlyphVec,
    /// Where the line stands, from the top of the layout (the top of its line box).
    pub y: f32,
}

impl_option!(TextLine, OptionTextLine, copy = false, [Debug, Clone, PartialEq]);
impl_vec!(
    TextLine,
    TextLineVec,
    TextLineVecDestructor,
    TextLineVecDestructorType,
    TextLineVecSlice,
    OptionTextLine
);
impl_vec_clone!(TextLine, TextLineVec, TextLineVecDestructor);
impl_vec_debug!(TextLine, TextLineVec);
impl_vec_partialeq!(TextLine, TextLineVec);

/// A text laid out in a stack of fonts ([`TextLayout::create`]).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextLayout {
    /// Its lines, from the top.
    pub lines: TextLineVec,
}

impl_option!(TextLayout, OptionTextLayout, copy = false, [Debug, Clone, PartialEq]);

impl TextLayout {
    /// `text` laid out in `fonts` (a stack, best first) with `options`: each grapheme cluster in
    /// the first font of the stack that has all of its characters, else in the one that misses
    /// the fewest (white space in the font of the run it is in); the bidi algorithm over the
    /// whole text; each run shaped in its font; the lines broken at spaces to fit
    /// `options.max_width` and put in visual order. None without a font, or when the text cannot
    /// be shaped (a shaper panic is caught here: the call never unwinds).
    #[must_use]
    pub fn create(text: AzString, fonts: FontRefVec, options: TextLayoutOptions) -> OptionTextLayout {
        layout_text(text.as_str(), fonts.as_ref(), &options).into()
    }

    /// The order in which the pieces of a one-line text are seen from left to right
    /// ([`super::cache::visual_order_of_pieces`]): `starts` are the pieces' byte offsets into
    /// `text`, ascending; the answer holds indices into `starts`. A text set piece by piece -
    /// the words of a label along a curve, each placed on its own - lies in this order: the
    /// words of an Arabic name from the last to the first, the Arabic part of a Latin name
    /// reversed in its place.
    #[must_use]
    pub fn visual_order_of_pieces(text: AzString, starts: U32Vec, direction: TextLayoutDirection) -> U32Vec {
        let starts: Vec<usize> = starts.as_ref().iter().map(|s| *s as usize).collect();
        let direction = match direction {
            TextLayoutDirection::Auto => None,
            TextLayoutDirection::LeftToRight => Some(BidiDirection::Ltr),
            TextLayoutDirection::RightToLeft => Some(BidiDirection::Rtl),
        };
        let order = super::cache::visual_order_of_pieces(text.as_str(), &starts, direction);
        order
            .into_iter()
            .map(|i| u32::try_from(i).unwrap_or(u32::MAX))
            .collect::<Vec<u32>>()
            .into()
    }

    /// How many lines the layout has.
    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// The glyph index `font` maps `codepoint` to (its cmap): None if the font has no glyph for it.
#[must_use]
pub fn font_glyph_index(font: &FontRef, codepoint: u32) -> OptionU16 {
    crate::font_ref_to_parsed_font(font)
        .lookup_glyph_index(codepoint)
        .into()
}

/// The advance of a glyph of `font`, in font units (its `hmtx` width).
#[must_use]
pub fn font_glyph_advance(font: &FontRef, glyph_id: u16) -> u16 {
    crate::font_ref_to_parsed_font(font).get_horizontal_advance(glyph_id)
}

/// The box of a glyph's outline, in font units: None for a glyph without an outline (a space)
/// or a glyph id past the font's glyphs.
#[must_use]
pub fn font_glyph_bounds(font: &FontRef, glyph_id: u16) -> OptionGlyphBounds {
    glyph_bounds(crate::font_ref_to_parsed_font(font), glyph_id, 1.0).into()
}

/// The advance of `font`'s space glyph, in font units: None if it has no space.
#[must_use]
pub fn font_space_width(font: &FontRef) -> OptionU32 {
    crate::font_ref_to_parsed_font(font)
        .get_space_width()
        .map(|w| u32::try_from(w).unwrap_or(u32::MAX))
        .into()
}

/// A glyph's outline box at `scale` (font size / units per em): None for a glyph whose box has
/// no height (no outline)
fn glyph_bounds(font: &crate::ParsedFont, glyph_id: u16, scale: f32) -> Option<GlyphBounds> {
    let glyph = font.get_or_decode_glyph(glyph_id)?;
    let b = &glyph.bounding_box;
    if b.max_y == 0 && b.min_y == 0 {
        return None;
    }
    Some(GlyphBounds {
        min_x: f32::from(b.min_x) * scale,
        min_y: f32::from(b.min_y) * scale,
        max_x: f32::from(b.max_x) * scale,
        max_y: f32::from(b.max_y) * scale,
    })
}

/// Whether a font of the stack has every character of `text` that is not white space or a
/// control character: how many it misses
fn missing_chars(font: &FontRef, text: &str) -> usize {
    let parsed = crate::font_ref_to_parsed_font(font);
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control() && !parsed.has_glyph(*c as u32))
        .count()
}

/// The font of `fonts` (a stack, best first) to set `text` in: the first that has all of its
/// characters, else the one that misses the fewest (the first on a tie)
fn pick_font(fonts: &[FontRef], text: &str) -> usize {
    let mut best = (0, usize::MAX);
    for (i, font) in fonts.iter().enumerate() {
        let missing = missing_chars(font, text);
        if missing == 0 {
            return i;
        }
        if missing < best.1 {
            best = (i, missing);
        }
    }
    best.0
}

/// `text` cut into runs of one font each, as a browser sets a text in a font stack: each
/// grapheme cluster in the first font of `fonts` that has all of it ([`pick_font`]), white space
/// and control characters in the font of the run they are in (at the start: of the first run
/// after them). Runs of one font are joined: `(font index, byte start, byte end)`, in logical
/// order.
pub(crate) fn font_runs(fonts: &[FontRef], text: &str) -> Vec<(usize, usize, usize)> {
    use unicode_segmentation::UnicodeSegmentation;

    if fonts.len() <= 1 || text.is_empty() {
        return vec![(0, 0, text.len())];
    }
    // the clusters: (byte start, byte end, its font; None for white space)
    let clusters: Vec<(usize, usize, Option<usize>)> = text
        .grapheme_indices(true)
        .map(|(start, g)| {
            let blank = g.chars().all(|c| c.is_whitespace() || c.is_control());
            (start, start + g.len(), (!blank).then(|| pick_font(fonts, g)))
        })
        .collect();
    let mut current = clusters.iter().find_map(|c| c.2).unwrap_or(0);
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for (start, end, font) in clusters {
        let font = font.unwrap_or(current);
        current = font;
        match runs.last_mut() {
            Some(last) if last.0 == font => last.2 = end,
            _ => runs.push((font, start, end)),
        }
    }
    runs
}

/// [`TextLayout::create`] on Rust types: None without a font or if the text cannot be shaped
#[must_use]
pub fn layout_text(text: &str, fonts: &[FontRef], options: &TextLayoutOptions) -> Option<TextLayout> {
    if fonts.is_empty() {
        return None;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        layout_text_unguarded(text, fonts, options)
    }))
    .ok()
    .flatten()
}

fn layout_text_unguarded(
    text: &str,
    fonts: &[FontRef],
    options: &TextLayoutOptions,
) -> Option<TextLayout> {
    let size = options.font_size;
    let max_width = options.max_width.into_option();
    // the runs as text3's content: a styled run per run of the stack, in its font
    let runs = font_runs(fonts, text);
    let mut content = Vec::with_capacity(runs.len());
    // per content run: its font and where its text starts
    let mut content_runs: Vec<(usize, usize)> = Vec::with_capacity(runs.len());
    for &(font, start, end) in runs.iter().filter(|r| r.2 > r.1) {
        let spacing = |px: f32| if px == 0.0 { Spacing::default() } else { Spacing::PxF(px) };
        let style = StyleProperties {
            font_stack: FontStack::Ref(fonts[font].clone()),
            font_size_px: size,
            tab_size: options.tab_size,
            letter_spacing: spacing(options.letter_spacing),
            word_spacing: spacing(options.word_spacing),
            ..StyleProperties::default()
        };
        content.push(InlineContent::Text(StyledRun {
            text: Arc::from(&text[start..end]),
            style: Arc::new(style),
            logical_start_byte: start,
            source_node_id: None,
        }));
        content_runs.push((font, start));
    }

    let (direction, unicode_bidi) = match options.direction {
        TextLayoutDirection::Auto => (None, UnicodeBidi::Plaintext),
        TextLayoutDirection::LeftToRight => (Some(BidiDirection::Ltr), UnicodeBidi::Normal),
        TextLayoutDirection::RightToLeft => (Some(BidiDirection::Rtl), UnicodeBidi::Normal),
    };
    let constraints = UnifiedConstraints {
        available_width: max_width.map_or(AvailableSpace::MaxContent, AvailableSpace::Definite),
        // the spaces as written; broken at them only with a width to fit
        white_space_mode: if max_width.is_some() {
            WhiteSpaceMode::PreWrap
        } else {
            WhiteSpaceMode::Pre
        },
        direction,
        unicode_bidi,
        ..UnifiedConstraints::default()
    };
    let fragments = [LayoutFragment {
        id: String::new(),
        constraints,
    }];
    // every font is in the runs (`FontStack::Ref`): no font chains, no loaded fonts, no
    // system fonts
    let loaded: LoadedFonts<crate::ParsedFont> = LoadedFonts::new();
    let fc_cache = rust_fontconfig::FcFontCache::default();
    let chains = std::collections::HashMap::new();
    let flow = TextShapingCache::new()
        .layout_flow(&content, &[], &fragments, &chains, &fc_cache, &loaded, &mut None)
        .ok()?;

    // the positioned items per line, in visual order
    let mut lines: BTreeMap<usize, Vec<&PositionedItem>> = BTreeMap::new();
    for layout in flow.fragment_layouts.values() {
        for item in &layout.items {
            lines.entry(item.line_index).or_default().push(item);
        }
    }
    // each font of the stack's scale (font size / units per em)
    let scales: Vec<f32> = fonts
        .iter()
        .map(|f| {
            let upem = crate::font_ref_to_parsed_font(f).pdf_font_metrics.units_per_em;
            size / f32::from(upem.max(1))
        })
        .collect();
    let run_of = |index: u32| content_runs.get(index as usize).copied().unwrap_or((0, 0));

    let mut out = Vec::with_capacity(lines.len());
    for items in lines.into_values() {
        let y = items
            .iter()
            .map(|i| i.position.y)
            .fold(f32::INFINITY, f32::min);
        let mut glyphs: Vec<TextGlyph> = Vec::new();
        for item in items {
            match &item.item {
                ShapedItem::Cluster(c) => {
                    let (font, run_start) = run_of(c.source_content_index.run_index);
                    let parsed = crate::font_ref_to_parsed_font(&fonts[font]);
                    let start = run_start
                        + c.source_content_index.item_index as usize
                        + c.source_cluster_id.start_byte_in_run as usize;
                    let end = start + usize::from(c.source_byte_len);
                    let cluster_text = c.text();
                    let whitespace = !cluster_text.is_empty()
                        && cluster_text.chars().all(char::is_whitespace);
                    let cursive = is_cursive_script_cluster(c);
                    let continuation = c.flags.0 & ClusterFlags::GRAPHEME_CONTINUATION != 0;
                    let right_to_left = c.direction.is_rtl();
                    // a right-to-left cluster holds its glyphs right to left: its first glyph in
                    // reading order is its last
                    let first = if right_to_left {
                        c.glyphs.len().saturating_sub(1)
                    } else {
                        0
                    };
                    let mut pen = item.position.x;
                    for (k, g) in c.glyphs.iter().enumerate() {
                        let advance = g.advance + g.kerning;
                        let ink = if whitespace {
                            None
                        } else {
                            glyph_bounds(parsed, g.glyph_id, scales[font])
                        };
                        glyphs.push(TextGlyph {
                            glyph_id: g.glyph_id,
                            font_index: u32::try_from(font).unwrap_or(0),
                            x: pen,
                            advance,
                            offset_x: g.offset.x,
                            offset_y: g.offset.y,
                            cluster_start: u32::try_from(start).unwrap_or(u32::MAX),
                            cluster_end: u32::try_from(end).unwrap_or(u32::MAX),
                            ink: ink.into(),
                            first_in_cluster: k == first,
                            whitespace,
                            tab: false,
                            cursive,
                            grapheme_continuation: continuation,
                            right_to_left,
                        });
                        pen += advance;
                    }
                }
                ShapedItem::Tab { source, bounds } => {
                    let (font, run_start) = run_of(source.run_index);
                    let start = run_start + source.item_index as usize;
                    glyphs.push(TextGlyph {
                        glyph_id: 0,
                        font_index: u32::try_from(font).unwrap_or(0),
                        x: item.position.x,
                        advance: bounds.width,
                        offset_x: 0.0,
                        offset_y: 0.0,
                        cluster_start: u32::try_from(start).unwrap_or(u32::MAX),
                        cluster_end: u32::try_from(start + 1).unwrap_or(u32::MAX),
                        ink: OptionGlyphBounds::None,
                        first_in_cluster: true,
                        whitespace: true,
                        tab: true,
                        cursive: false,
                        grapheme_continuation: false,
                        right_to_left: false,
                    });
                }
                _ => {}
            }
        }
        out.push(TextLine {
            glyphs: glyphs.into(),
            y: if y.is_finite() { y } else { 0.0 },
        });
    }
    Some(TextLayout { lines: out.into() })
}

#[cfg(test)]
mod tests {
    //! The text module on the mock fonts (`text3::mock_fonts`: Azul Mock Mono and Wide, Latin
    //! letters of known advances) and printpdf's CID-keyed CFF mock.
    use super::*;
    use crate::text3::mock_fonts::{MOCK_MONO_TTF, MOCK_WIDE_TTF};

    fn font(bytes: &[u8]) -> FontRef {
        crate::parsed_font_to_font_ref(
            crate::ParsedFont::from_bytes(bytes, 0, &mut Vec::new()).expect("the mock parses"),
        )
    }

    fn glyphs(l: &TextLayout) -> Vec<TextGlyph> {
        l.lines.iter().flat_map(|l| l.glyphs.iter().copied()).collect()
    }

    #[test]
    fn a_text_is_laid_out_glyph_by_glyph_left_to_right() {
        let mono = font(MOCK_MONO_TTF);
        let options = TextLayoutOptions::create(10.0);
        let layout = layout_text("Ab c", std::slice::from_ref(&mono), &options).expect("laid out");
        assert_eq!(layout.lines.len(), 1);
        let g = glyphs(&layout);
        assert_eq!(g.len(), 4, "{g:?}");
        // every glyph after the one before, as wide as it moves the pen
        for w in g.windows(2) {
            assert!((w[1].x - (w[0].x + w[0].advance)).abs() < 1e-4, "{g:?}");
        }
        // the clusters' text: "A", "b", " ", "c"
        let text = "Ab c";
        let parts: Vec<&str> = g
            .iter()
            .map(|g| &text[g.cluster_start as usize..g.cluster_end as usize])
            .collect();
        assert_eq!(parts, ["A", "b", " ", "c"]);
        assert!(g[2].whitespace && g[2].ink.into_option().is_none());
        assert!(g.iter().all(|g| g.first_in_cluster && !g.right_to_left && g.font_index == 0));
        // a letter's ink: its outline at the font size, above the baseline
        let a = g[0].ink.into_option().expect("A has an outline");
        assert!(a.max_y > 0.0 && a.max_y < 10.0, "{a:?}");
        // the glyph and advance the font itself gives
        let gid = font_glyph_index(&mono, 'A' as u32).into_option().unwrap();
        assert_eq!(g[0].glyph_id, gid);
        let upem = f32::from(crate::font_ref_to_parsed_font(&mono).pdf_font_metrics.units_per_em);
        assert!((g[0].advance - f32::from(font_glyph_advance(&mono, gid)) * 10.0 / upem).abs() < 1e-3);
    }

    #[test]
    fn a_cluster_goes_to_the_first_font_that_has_it() {
        let (mono, wide) = (font(MOCK_MONO_TTF), font(MOCK_WIDE_TTF));
        // both mocks cover the same letters: everything in the first
        assert_eq!(font_runs(&[mono.clone(), wide.clone()], "ab cd"), vec![(0, 0, 5)]);
        // one font: one run, whatever it lacks
        assert_eq!(font_runs(std::slice::from_ref(&mono), "a\u{4E00}"), vec![(0, 0, 4)]);
        // a character neither has: the one that misses the fewest, the first on a tie
        assert_eq!(pick_font(&[mono.clone(), wide.clone()], "\u{4E00}"), 0);
        // laid out in the stack, each glyph carries its font
        let layout = layout_text("ab", &[mono, wide], &TextLayoutOptions::create(10.0)).unwrap();
        assert!(glyphs(&layout).iter().all(|g| g.font_index == 0));
    }

    #[test]
    fn white_space_takes_the_font_of_its_run() {
        let (mono, cff) = (
            font(MOCK_MONO_TTF),
            font(include_bytes!("../../assets/fonts/test/printpdf-mock-cff-cid.otf")),
        );
        // the CFF mock has only "A".."J" and the space; Mock Mono has the lowercase letters
        let stack = [cff, mono];
        assert_eq!(font_runs(&stack, "AB ab"), vec![(0, 0, 3), (1, 3, 5)]);
        // white space at the start: the font of what follows it
        assert_eq!(font_runs(&stack, " ab AB"), vec![(1, 0, 4), (0, 4, 6)]);
        let layout = layout_text("AB ab", &stack, &TextLayoutOptions::create(10.0)).unwrap();
        let fonts: Vec<u32> = glyphs(&layout).iter().map(|g| g.font_index).collect();
        assert_eq!(fonts, [0, 0, 0, 1, 1]);
        // the CFF glyphs have their outlines' ink
        assert!(glyphs(&layout)[0].ink.into_option().is_some_and(|b| b.max_y > 6.9 && b.max_y < 7.1));
    }

    #[test]
    fn a_width_breaks_the_lines_at_spaces() {
        let mono = font(MOCK_MONO_TTF);
        let one = layout_text("aa bb cc", std::slice::from_ref(&mono), &TextLayoutOptions::create(10.0)).unwrap();
        assert_eq!(one.lines.len(), 1);
        let width: f32 = glyphs(&one).iter().map(|g| g.advance).sum();
        let options = TextLayoutOptions {
            max_width: OptionF32::Some(width * 0.5),
            ..TextLayoutOptions::create(10.0)
        };
        let broken = layout_text("aa bb cc", &[mono], &options).unwrap();
        assert!(broken.lines.len() >= 2, "{broken:?}");
        assert!(broken.lines.iter().zip(broken.lines.iter().skip(1)).all(|(a, b)| a.y < b.y));
    }

    #[test]
    fn no_font_lays_nothing_out() {
        assert!(layout_text("a", &[], &TextLayoutOptions::default()).is_none());
        assert!(TextLayout::create("a".into(), Vec::<FontRef>::new().into(), TextLayoutOptions::default())
            .into_option()
            .is_none());
    }

    #[test]
    fn the_font_functions_answer_the_fonts_own_measures() {
        let mono = font(MOCK_MONO_TTF);
        let parsed = crate::font_ref_to_parsed_font(&mono);
        let space = font_glyph_index(&mono, ' ' as u32).into_option().expect("a space");
        assert_eq!(
            font_space_width(&mono).into_option(),
            Some(u32::from(parsed.get_horizontal_advance(space)))
        );
        assert!(font_glyph_bounds(&mono, space).into_option().is_none(), "a space has no outline");
        let h = font_glyph_index(&mono, 'H' as u32).into_option().unwrap();
        let b = font_glyph_bounds(&mono, h).into_option().expect("H has an outline");
        let decoded = parsed.get_or_decode_glyph(h).unwrap().bounding_box;
        assert_eq!(b.max_y, f32::from(decoded.max_y));
        assert!(font_glyph_index(&mono, 0x10_FFFF).into_option().is_none());
        assert!(font_glyph_bounds(&mono, u16::MAX).into_option().is_none());
    }

    #[test]
    fn the_pieces_of_a_text_in_visual_order_through_the_api() {
        let text = "Avenue \u{0634}\u{0627}\u{0631}\u{0639} \u{0627}\u{0644}\u{062D}\u{0633}\u{0646}";
        let starts: Vec<u32> = vec![0, 7, 16];
        let order = TextLayout::visual_order_of_pieces(text.into(), starts.into(), TextLayoutDirection::Auto);
        assert_eq!(order.as_ref(), &[0, 2, 1]);
    }
}
