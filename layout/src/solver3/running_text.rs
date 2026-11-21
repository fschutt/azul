//! The text of a page's header, its footer and its page numbers: the font it is
//! set in and how it is drawn.
//!
//! A paged layout sets it the way it sets the text of the document:
//!
//! - The font family (`HeaderFooterConfig::font_family`, a CSS `font-family` list; the document's
//!   default font when unset) becomes a font stack with the builder the document's text uses, its
//!   chain is resolved by the resolver the document's chains are, and its faces are loaded into the
//!   font manager the document was shaped with. A face the header is set in is therefore a face a
//!   PDF writer finds in that manager, whether the document uses it or not, and a family the system
//!   lacks falls back as it does in the document.
//! - The text is shaped by text3: shaping, kerning, bidi and a font per character where the first
//!   family lacks one.
//! - The lines are painted as the display list items of any other text: the glyph runs
//!   ([`DisplayListItem::Text`]) and the shaped layout ([`DisplayListItem::TextLayout`]) a PDF
//!   writer draws the text from.
//!
//! The registration table of the render API (`RendererResources`) is not consulted: a paged layout
//! for PDF or print never registers a font in it.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::StyledDom,
};
use azul_css::{
    props::basic::{font::parse_style_font_family, PhysicalSize},
    LayoutDebugMessage,
};
use rust_fontconfig::config::is_generic_family;

use crate::{
    font_traits::{
        AvailableSpace, FontHash, FontManager, FontSelector, InlineContent, LayoutError,
        LayoutFragment, ParsedFontTrait, StyleProperties, StyledRun, TextShapingCache,
        UnifiedConstraints, UnifiedLayout,
    },
    solver3::{
        display_list::{
            is_drawable_size, report_once, DisplayListItem, RunningTextSetter, SlicerConfig,
            TextAlignment,
        },
        getters,
        pagination::{CounterFormat, HeaderFooterConfig, MarginBoxContent, OptionPageSetup},
    },
    text3::{
        cache::{
            FontChainKey, FontChainKeyOrRef, FontStack, LineHeight, TextAlign, TextWrap,
            WhiteSpaceMode,
        },
        glyphs::get_glyph_runs_simple,
    },
};

/// The id of the one fragment a line of page text is laid out in.
const FRAGMENT_ID: &str = "running-text";

/// The pages sampled for the characters of a page-number format, after pages 1
/// to 26 (every letter of an alphabet: the Latin one has 26, the Greek one 24):
/// the Roman numerals' `l`, `c`, `d` and `m` and their repetitions.
const SAMPLE_PAGE_NUMBERS: [usize; 6] = [30, 50, 100, 500, 1000, 3999];

/// What a font stack of the page furniture is asked for: a family list
/// (`None` = the document's default font) at a size.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FaceKey {
    /// The CSS `font-family` list, trimmed; `None` when unset or blank.
    family: Option<String>,
    /// The bits of the font size in px (the key must be `Ord`).
    size_bits: u32,
}

impl FaceKey {
    fn of(style: &HeaderFooterConfig) -> Self {
        Self {
            family: style
                .font_family
                .as_ref()
                .map(|family| family.as_str().trim())
                .filter(|family| !family.is_empty())
                .map(str::to_owned),
            size_bits: style.font_size.to_bits(),
        }
    }

    const fn size_px(&self) -> f32 {
        f32::from_bits(self.size_bits)
    }
}

/// A kind of finding of the page furniture: what the process is told once,
/// whatever family, face or text ran into it (see [`Reporter::warn`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finding {
    /// No face was prepared for the family and size a page setup asks for.
    NoFacePrepared,
    /// Shaping the text failed.
    ShapingFailed,
    /// No face of the family can draw any of the text.
    NoGlyphs,
    /// No face can draw some of the text, and the rest of it was drawn.
    NoGlyphsForPartOfTheText,
    /// No family of a font family list matches a font.
    UnresolvedFamilies,
    /// A face of the chain of a family could not be loaded.
    FontLoadFailed,
}

impl Finding {
    /// The key the process remembers the kind by (`report_once`).
    const fn class(self) -> &'static str {
        match self {
            Self::NoFacePrepared => "paginate: no face prepared",
            Self::ShapingFailed => "paginate: shaping failed",
            Self::NoGlyphs => "paginate: no glyphs",
            Self::NoGlyphsForPartOfTheText => "paginate: no glyphs for part of the text",
            Self::UnresolvedFamilies => "paginate: unresolved families",
            Self::FontLoadFailed => "paginate: font load failed",
        }
    }
}

/// Where the warnings of the page furniture go: the layout's debug messages
/// when it collects them, and the process's diagnostics once per kind of
/// finding.
struct Reporter<'a> {
    sink: &'a mut Option<Vec<LayoutDebugMessage>>,
    said: BTreeSet<String>,
}

impl Reporter<'_> {
    /// Says a finding of `kind` with `message`; `detail` (the family, the face)
    /// is what tells it from the others of its kind in a layout.
    ///
    /// The layout's debug messages get each kind and detail once, however many
    /// pages run into it. The process gets each KIND once: the diagnostics'
    /// table of once-only keys holds 1024 and prints no new finding after
    /// that, so a key made of a family a caller passes in would let a service
    /// that lays out documents in arbitrary families silence every once-only
    /// finding of the process.
    fn warn(&mut self, kind: Finding, detail: &str, message: String) {
        if !self.said.insert(format!("{}: {detail}", kind.class())) {
            return;
        }
        if let Some(messages) = self.sink.as_mut() {
            messages.push(LayoutDebugMessage::warning(message.clone()));
        }
        report_once(kind.class(), || message);
    }
}

/// Sets the text of the headers and footers of a paged layout with the font
/// manager the document was shaped with (see the module docs).
pub(crate) struct RunningText<'a, T: ParsedFontTrait> {
    font_manager: &'a FontManager<T>,
    /// The font stack of every family and size the page setups ask for.
    faces: Vec<(FaceKey, FontStack)>,
    /// A shaped header is shaped once, however many pages carry it.
    cache: TextShapingCache,
    reporter: Reporter<'a>,
}

impl<'a, T: ParsedFontTrait> RunningText<'a, T> {
    /// Resolves the fonts every page setup of `slicer` sets its header and footer
    /// in - the stack, its chain, its loaded faces - in `font_manager`, ready
    /// for [`RunningTextSetter::set_text`].
    ///
    /// A stack the document's own text is set in already has its chain in the
    /// manager (and its faces): it is the chain the text of the page furniture
    /// uses, extended where that text holds characters it does not cover. A
    /// stack the document does not use gets a chain resolved as the
    /// document's are.
    pub(crate) fn new<F>(
        slicer: &SlicerConfig,
        styled_dom: &StyledDom,
        viewport: LogicalSize,
        font_manager: &'a mut FontManager<T>,
        font_loader: &F,
        debug_messages: &'a mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self
    where
        F: Fn(Arc<rust_fontconfig::FontBytes>, usize) -> Result<T, LayoutError>,
    {
        let mut reporter = Reporter {
            sink: debug_messages,
            said: BTreeSet::new(),
        };
        let mut faces = Vec::new();
        for (key, chars) in wanted_faces(slicer) {
            let stack = face_stack(&key, styled_dom, viewport);
            match &stack {
                FontStack::Stack(selectors) => {
                    ensure_fonts(font_manager, selectors, &chars, font_loader, &mut reporter);
                }
                FontStack::Ref(font_ref) => font_manager.register_embedded_font(font_ref),
            }
            faces.push((key, stack));
        }
        Self {
            font_manager,
            faces,
            cache: TextShapingCache::new(),
            reporter,
        }
    }

    /// The constraints of one line of text at `font_size`, `width` wide: the
    /// strut and the line height of the stack's first available font (CSS 2.2
    /// 10.8.1), as the inline layout of the document sets them for a block.
    fn constraints(
        &self,
        stack: &FontStack,
        width: f32,
        font_size: f32,
        alignment: TextAlignment,
    ) -> UnifiedConstraints {
        let metrics = self
            .font_manager
            .first_available_font_metrics(stack)
            .filter(|m| m.units_per_em > 0);
        let line = metrics.and_then(|m| m.line_metrics_px(font_size));
        let scale = metrics.map(|m| font_size / f32::from(m.units_per_em));
        let (strut_ascent, strut_descent) = line_box(line, font_size);
        UnifiedConstraints {
            available_width: AvailableSpace::Definite(width),
            text_align: match alignment {
                TextAlignment::Left => TextAlign::Left,
                TextAlignment::Center => TextAlign::Center,
                TextAlignment::Right => TextAlign::Right,
            },
            // One line: a header that does not fit runs on past its band
            // rather than wrapping onto the page's content.
            text_wrap: TextWrap::NoWrap,
            white_space_mode: WhiteSpaceMode::Nowrap,
            line_height: line.map_or(LineHeight::Normal, |(ascent, descent, gap)| {
                LineHeight::Px(ascent + descent + gap)
            }),
            strut_ascent,
            strut_descent,
            strut_x_height: metrics
                .and_then(|m| m.x_height)
                .zip(scale)
                .map_or(font_size * 0.5, |(x_height, scale)| x_height * scale),
            strut_cap_height: metrics
                .and_then(|m| m.cap_height)
                .zip(scale)
                .map_or(font_size * 0.7, |(cap_height, scale)| cap_height * scale),
            strut_font_size: font_size,
            ch_width: font_size * 0.5,
            ..UnifiedConstraints::default()
        }
    }

    /// `text` shaped and set in one line by text3, in `stack` at `style`'s
    /// size and color.
    fn shape(
        &mut self,
        text: &str,
        stack: &FontStack,
        style: &HeaderFooterConfig,
        width: f32,
        alignment: TextAlignment,
    ) -> Option<Arc<UnifiedLayout>> {
        let content = [InlineContent::Text(StyledRun {
            text: Arc::from(text),
            style: Arc::new(StyleProperties {
                font_stack: stack.clone(),
                font_size_px: style.font_size,
                color: style.text_color,
                ..StyleProperties::default()
            }),
            logical_start_byte: 0,
            source_node_id: None,
        })];
        let fragments = [LayoutFragment {
            id: FRAGMENT_ID.to_owned(),
            constraints: self.constraints(stack, width, style.font_size, alignment),
        }];
        let flow = self
            .cache
            .layout_flow(
                &content,
                &[],
                &fragments,
                &self.font_manager.font_chain_cache,
                &self.font_manager.fc_cache,
                &self.font_manager.get_loaded_fonts(),
                &mut *self.reporter.sink,
            )
            .ok()?;
        flow.fragment_layouts.get(FRAGMENT_ID).cloned()
    }
}

impl<T: ParsedFontTrait> RunningTextSetter for RunningText<'_, T> {
    fn set_text(
        &mut self,
        text: &str,
        band: LogicalRect,
        style: &HeaderFooterConfig,
        alignment: TextAlignment,
    ) -> Vec<DisplayListItem> {
        // Nothing to draw, and nothing to say: no text or blank text (it would
        // shape to an empty layout, and the empty layout be taken for a missing
        // font), a size that is no size, or text nobody can see.
        if text.trim().is_empty()
            || !is_drawable_size(style.font_size)
            || style.text_color.a == 0
        {
            return Vec::new();
        }
        let key = FaceKey::of(style);
        let family = key
            .family
            .clone()
            .unwrap_or_else(|| "the document's default font".to_owned());
        // `wanted_faces` mirrors the slicer's choice of the setup of every page,
        // so a face is prepared for every style the slicer asks about. This is
        // the guard for the day the two drift apart: a setup the slicer picks
        // and `wanted_faces` does not see is said, not a header that vanishes.
        let Some(stack) = self
            .faces
            .iter()
            .find(|(face, _)| *face == key)
            .map(|(_, stack)| stack.clone())
        else {
            self.reporter.warn(
                Finding::NoFacePrepared,
                &family,
                format!(
                    "[azul][paginate] the page text {text:?} was not drawn: no font was resolved \
                     for {family} at {}px",
                    key.size_px()
                ),
            );
            return Vec::new();
        };
        let Some(layout) = self.shape(text, &stack, style, band.size.width, alignment) else {
            self.reporter.warn(
                Finding::ShapingFailed,
                &family,
                format!(
                    "[azul][paginate] the page text {text:?} was not drawn: shaping it in \
                     {family} failed"
                ),
            );
            return Vec::new();
        };
        self.finish(layout, &stack, style, band, text, &family)
    }
}

/// What [`RunningText::paint`] made of a shaped layout.
#[derive(Default)]
struct Painted {
    /// The shaped layout and its glyph runs; empty when no run has a face to
    /// be drawn with.
    items: Vec<DisplayListItem>,
    /// The runs of glyphs the layout has.
    runs: usize,
    /// The runs of them left out: a glyph whose face never resolved carries no
    /// hash, and a renderer has nothing to draw it with.
    dropped: usize,
}

impl<T: ParsedFontTrait> RunningText<'_, T> {
    /// The display list items of the shaped `layout` of `text` in `band` (see
    /// [`Self::paint`]), saying what of the text could not be drawn: all of it,
    /// or part of it (a layout of two runs, one with a face and one without,
    /// draws one).
    fn finish(
        &mut self,
        layout: Arc<UnifiedLayout>,
        stack: &FontStack,
        style: &HeaderFooterConfig,
        band: LogicalRect,
        text: &str,
        family: &str,
    ) -> Vec<DisplayListItem> {
        let painted = self.paint(layout, stack, style, band);
        if painted.items.is_empty() {
            self.reporter.warn(
                Finding::NoGlyphs,
                family,
                format!(
                    "[azul][paginate] the page text {text:?} was not drawn: no face of {family} \
                     could be loaded to set it in (the chain of the family has none, or its \
                     files could not be read)"
                ),
            );
        } else if painted.dropped > 0 {
            self.reporter.warn(
                Finding::NoGlyphsForPartOfTheText,
                family,
                format!(
                    "[azul][paginate] part of the page text {text:?} was not drawn: {} of its {} \
                     runs of glyphs have no face of {family} to be drawn with",
                    painted.dropped, painted.runs
                ),
            );
        }
        painted.items
    }

    /// The display list items of a shaped `layout`, in `band`: its glyph runs
    /// and the layout itself. Empty when no glyph of it has a face to be drawn
    /// with; the runs that have none are left out and counted.
    ///
    /// The em box of the font is centred in the band, as the text of a header
    /// always was: the layout moves so that its first baseline lands where the
    /// box puts the baseline.
    fn paint(
        &self,
        layout: Arc<UnifiedLayout>,
        stack: &FontStack,
        style: &HeaderFooterConfig,
        band: LogicalRect,
    ) -> Painted {
        let runs = get_glyph_runs_simple(&layout);
        let (Some(first_baseline), Some(primary)) = (
            layout.first_baseline(),
            runs.iter().map(|run| run.font_hash).find(|hash| *hash != 0),
        ) else {
            return Painted::default();
        };
        let (ascent, descent) = line_box(
            self.font_manager
                .first_available_font_metrics(stack)
                .filter(|m| m.units_per_em > 0)
                .and_then(|m| m.line_metrics_px(style.font_size)),
            style.font_size,
        );
        // The plain `a + b * c`: layout output stays bit-reproducible (see the
        // note on `clippy::suboptimal_flops` in lib.rs).
        let baseline = band.origin.y + ascent + (band.size.height - (ascent + descent)) * 0.5;
        let origin = LogicalPosition::new(band.origin.x, baseline - first_baseline);

        let with_glyphs = runs.iter().filter(|run| !run.glyphs.is_empty()).count();
        let texts: Vec<DisplayListItem> = runs
            .into_iter()
            // A glyph whose face never resolved carries no hash: the renderer
            // has nothing to draw it with.
            .filter(|run| !run.glyphs.is_empty() && run.font_hash != 0)
            .map(|run| DisplayListItem::Text {
                glyphs: run
                    .glyphs
                    .into_iter()
                    .map(|mut glyph| {
                        glyph.point.x += origin.x;
                        glyph.point.y += origin.y;
                        glyph
                    })
                    .collect(),
                font_hash: FontHash::from_hash(run.font_hash),
                font_size_px: run.font_size_px,
                color: run.color,
                clip_rect: band.into(),
                line_box: band.into(),
                source_node_index: None,
            })
            .collect();
        if texts.is_empty() {
            return Painted::default();
        }
        let dropped = with_glyphs - texts.len();
        let extent = layout.bounds();
        let payload: Arc<dyn std::any::Any + Send + Sync> = layout;
        let mut items = Vec::with_capacity(texts.len() + 1);
        items.push(DisplayListItem::TextLayout {
            layout: payload,
            bounds: LogicalRect::new(origin, LogicalSize::new(extent.width, extent.height))
                .into(),
            font_hash: FontHash::from_hash(primary),
            font_size_px: style.font_size,
            color: style.text_color,
        });
        items.extend(texts);
        Painted {
            items,
            runs: with_glyphs,
            dropped,
        }
    }
}

/// The ascent and descent of the line box text at `font_size` is centred by:
/// the face's rounded metrics, 0.8 em / 0.2 em for a face without any.
fn line_box(line: Option<(f32, f32, f32)>, font_size: f32) -> (f32, f32) {
    line.map_or((font_size * 0.8, font_size * 0.2), |(ascent, descent, _gap)| {
        (ascent, descent)
    })
}

/// Every font stack the page setups of `slicer` ask for, with the characters
/// their texts hold (what the chains must cover): the headers and footers of
/// the setup of every page.
fn wanted_faces(slicer: &SlicerConfig) -> BTreeMap<FaceKey, BTreeSet<char>> {
    let mut configs: Vec<&HeaderFooterConfig> = Vec::new();
    match &slicer.page_sequence {
        Some(sequence) => {
            configs.extend(sequence.overrides.iter().map(|o| &o.setup.header_footer));
            configs.push(&sequence.default.header_footer);
            configs.extend(
                [&sequence.first_page, &sequence.odd_pages, &sequence.even_pages]
                    .into_iter()
                    .filter_map(OptionPageSetup::as_ref)
                    .map(|setup| &setup.header_footer),
            );
        }
        None => configs.push(&slicer.header_footer),
    }
    let mut wanted: BTreeMap<FaceKey, BTreeSet<char>> = BTreeMap::new();
    for config in configs {
        if !(config.show_header || config.show_footer) || !is_drawable_size(config.font_size) {
            continue;
        }
        let chars = wanted.entry(FaceKey::of(config)).or_default();
        if config.show_header {
            collect_chars(&config.header_content, chars);
        }
        if config.show_footer {
            collect_chars(&config.footer_content, chars);
        }
    }
    wanted
}

/// The characters `content` can say: its literal text and the digits, letters
/// or numerals of its page numbers. What a callback says is not known until
/// it is called.
fn collect_chars(content: &MarginBoxContent, out: &mut BTreeSet<char>) {
    match content {
        MarginBoxContent::None | MarginBoxContent::Custom(_) => {}
        MarginBoxContent::Text(text) => out.extend(text.as_str().chars()),
        MarginBoxContent::NamedString(name) => {
            out.extend(format!("[string:{}]", name.as_str()).chars());
        }
        MarginBoxContent::RunningElement(name) => {
            out.extend(format!("[element:{}]", name.as_str()).chars());
        }
        MarginBoxContent::PageCounter | MarginBoxContent::PagesCounter => {
            out.extend(CounterFormat::Decimal.format(1_234_567_890).chars());
            // "Page 1 of ?": the total is not known on a first pass.
            out.insert('?');
        }
        MarginBoxContent::PageCounterFormatted(format) => {
            out.extend(('1'..='9').chain(['0', '?']));
            for n in (1..=26).chain(SAMPLE_PAGE_NUMBERS) {
                out.extend(format.format(n).chars());
            }
        }
        MarginBoxContent::Combined(parts) => {
            for part in parts {
                collect_chars(part, out);
            }
        }
    }
}

/// The font stack `key` asks for: the CSS family list it names, or the
/// document's default font (the one the root element has) when it names none.
fn face_stack(key: &FaceKey, styled_dom: &StyledDom, viewport: LogicalSize) -> FontStack {
    let size = key.size_px();
    if let Some(families) = key
        .family
        .as_deref()
        .and_then(|list| parse_style_font_family(list).ok())
    {
        return FontStack::Stack(getters::font_stack_for_families(styled_dom, &families, size));
    }
    // The root element's own style: the font a node with no `font-family` of
    // its own is set in, which is what a margin box inherits (CSS Paged Media:
    // the page context takes its properties from the root).
    let root = NodeId::new(0);
    if styled_dom.styled_nodes.as_container().get(root).is_none() {
        return FontStack::Stack(vec![FontSelector::default()]);
    }
    match getters::get_style_properties(
        styled_dom,
        root,
        None,
        PhysicalSize::new(viewport.width, viewport.height),
    )
    .font_stack
    {
        FontStack::Stack(selectors) => FontStack::Stack(getters::at_optical_size(selectors, size)),
        font_ref @ FontStack::Ref(_) => font_ref,
    }
}

/// The font families of a stack (`families`, the names of its chain's key) that
/// nothing matched, when that is every family the stack names: the whole stack
/// is then the fallback, and the CSS list the caller wrote set the text in none
/// of what it asked for. Empty when some family matched, and for a stack of
/// generic families only (`serif`, the ones the stack ends in: the system
/// always has a face for them).
///
/// `"Georgia, Foo, serif"` is a good list where Georgia is installed, Foo
/// missing or not; a missing first choice is what such a list is for. (The
/// key leaves out an empty family name, so a trailing comma names nothing.)
/// `unresolved` is what the resolver found no font for: on the registry's path
/// the families ahead of the first match, on the legacy path every one.
fn families_without_a_font(families: &[String], unresolved: &BTreeSet<String>) -> Vec<String> {
    let named: Vec<String> = families
        .iter()
        .filter(|family| !is_generic_family(family))
        .cloned()
        .collect();
    if named.iter().all(|family| unresolved.contains(family)) {
        named
    } else {
        Vec::new()
    }
}

/// Makes sure `font_manager` has the chain of `selectors` and the faces of it
/// loaded, covering `chars`; says what could not be.
fn ensure_fonts<T, F>(
    font_manager: &mut FontManager<T>,
    selectors: &[FontSelector],
    chars: &BTreeSet<char>,
    font_loader: &F,
    reporter: &mut Reporter<'_>,
) where
    T: ParsedFontTrait,
    F: Fn(Arc<rust_fontconfig::FontBytes>, usize) -> Result<T, LayoutError>,
{
    let key = FontChainKey::from_selectors(selectors);
    let mut resolved = getters::ResolvedFontChains::default();
    let held = font_manager.font_chain_cache.contains_key(&key);
    if held {
        // The document's chain for this stack, extended for the characters of
        // the page furniture it does not cover (the faces that adds are loaded).
        if !chars.is_empty() {
            let content = [InlineContent::Text(StyledRun {
                text: Arc::from(chars.iter().collect::<String>()),
                style: Arc::new(StyleProperties {
                    font_stack: FontStack::Stack(selectors.to_vec()),
                    ..StyleProperties::default()
                }),
                logical_start_byte: 0,
                source_node_id: None,
            })];
            font_manager.extend_chains_for_content(&content, font_loader);
        }
        if let Some(chain) = font_manager.font_chain_cache.get(&key) {
            resolved
                .chains
                .insert(FontChainKeyOrRef::Chain(key), chain.clone());
        }
    } else {
        resolved = getters::resolve_font_stacks_covering(
            vec![selectors.to_vec()],
            chars,
            &font_manager.fc_cache,
            font_manager,
        );
        let lost = families_without_a_font(&key.font_families, &resolved.unresolved_families);
        if !lost.is_empty() {
            let list = lost.join(", ");
            reporter.warn(
                Finding::UnresolvedFamilies,
                &list,
                format!(
                    "[azul][paginate] none of the font families \"{list}\" of the header and \
                     footer matches an installed or registered font: the text is set in the \
                     fallback of its stack"
                ),
            );
        }
    }
    // Faces the chain names that are not loaded yet (the manager finds none
    // when the document loaded them).
    for (id, error) in font_manager.load_missing_for_chains(&resolved, font_loader) {
        let name = font_manager
            .fc_cache
            .get_metadata_by_id(&id)
            .and_then(|pattern| pattern.name.or(pattern.family))
            .unwrap_or_else(|| format!("{id:?}"));
        reporter.warn(
            Finding::FontLoadFailed,
            &format!("{id:?}"),
            format!(
                "[azul][paginate] failed to load the font {name} of the header and footer: \
                 {error}"
            ),
        );
    }
    // A chain for this stack the document did not hold is kept, for the text
    // set in it.
    if !held {
        font_manager.merge_font_chain_cache(resolved.into_fontconfig_chains());
    }
}

#[cfg(test)]
#[path = "running_text_test.rs"]
mod running_text_test;
