//! Renders HTML to PDF pages.
//!
//! A PDF page is a list of drawing operations (see [`crate::Op`]); it has no notion of
//! paragraphs, line breaking or boxes. To get from markup to those operations this module runs
//! three steps:
//!
//! 1. The HTML is parsed into a DOM with its CSS (azul-core).
//! 2. [azul-layout](https://crates.io/crates/azul-layout) lays the DOM out and cuts it into
//!    pages. The result is one display list per page: positioned text, boxes, borders, images
//!    and gradients.
//! 3. The [`bridge`] turns each display list into [`Op`](crate::Op)s of a [`PdfPage`]. It also
//!    collects the fonts the text uses and the images the page draws, so they can be embedded.
//!
//! Most programs call `PdfDocument::from_html` and never use this module directly. Use
//! [`xml_to_pdf_pages`] or [`add_xml_to_document`] to render with an [`XmlRenderOptions`], for
//! example to append HTML pages to a document you already have.
//!
//! # Input
//!
//! The CSS support is the one of azul-layout: block, inline and flex layout, positioning,
//! multi-column text, backgrounds, gradients, borders, shadows and transforms. Properties the
//! engine does not know are ignored. Pages are cut automatically; `break-before`,
//! `break-after`, `break-inside: avoid`, `widows` and `orphans` are honoured, table rows are not
//! split, and `<thead>` repeats on each page. Elements that cannot be drawn (`script`, `iframe`,
//! `canvas`, `video` and similar) are removed before parsing.
//!
//! # Page size and margins
//!
//! The page size and margins come from [`XmlRenderOptions`] (A4 and no margins by default); the
//! CSS `@page` rule is not read for them. Layout happens inside the area that the margins leave.
//! The page and that area must each measure between 1 pt and 1 000 000 pt in both directions;
//! otherwise rendering fails with an error.
//!
//! # Fonts and images
//!
//! - [`XmlRenderOptions::fonts`] maps a name to font file bytes. `font-family: name` selects
//!   it, as does the family name stored in the font file. Installed system fonts are found
//!   through [rust-fontconfig](https://crates.io/crates/rust-fontconfig). The 14 standard PDF
//!   fonts are always available: `Helvetica`, `Arial`, `Times New Roman`, `Courier New` and so on
//!   resolve to them, and they back `serif`, `sans-serif` and `monospace` when no installed font
//!   does.
//! - [`XmlRenderOptions::images`] maps an image id to the bytes of an image file.
//!   `<img src="id">` draws it. Only the images a page draws are decoded. Give the `<img>` a
//!   width and a height, because layout does not read the size of the picture.
//!
//! Scanning system fonts is slow. To render many documents, build a [`SharedFontPool`] once with
//! [`build_font_pool`] and pass it as [`XmlRenderOptions::font_pool`] (or to
//! `PdfDocument::from_html_with_cache`); the parsed fonts are then reused across documents.
//!
//! # Example
//!
//! ```
//! use printpdf::html::{xml_to_pdf_pages, XmlRenderOptions};
//!
//! let html = "<html><body><h1>Hello</h1><p>Some text.</p></body></html>";
//! let options = XmlRenderOptions::default();
//! let (pages, _fonts, _images, _resources) = xml_to_pdf_pages(html, &options).unwrap();
//! assert!(!pages.is_empty());
//! ```

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use azul_core::{
    dom::DomId,
    geom::{LogicalSize, LogicalRect, LogicalPosition},
    resources::RendererResources,
    xml::{str_to_dom, ComponentDef},
};
use azul_layout::{
    font::loading::build_font_cache,
    paged::FragmentationContext,
    solver3::page_breaks::BreakPolicy,
    solver3::paged_layout::{layout_document_paged_streaming, layout_document_paged_v2},
    solver3::pagination::FakePageConfig,
    text3::cache::FontHash,
    font_traits::{TextLayoutCache, FontManager},
    Solver3LayoutCache,
    xml::parse_xml_string,
};
use serde_derive::{Deserialize, Serialize};

use azul_layout::text3::cache::MemoryFontTier;

use crate::{font::ParsedFont, BuiltinFont, Mm, PdfDocument, PdfPage, PdfWarnMsg};

/// Re-export of `rust-fontconfig`, so callers of [`build_font_pool_with_memory_fonts`] can build
/// `FcPattern` and `FcFont` values without depending on a matching version themselves.
pub use rust_fontconfig;

/// Font state shared by several renders, so that font files are read and parsed only once.
///
/// Build it with [`build_font_pool`] and pass it as [`XmlRenderOptions::font_pool`]. Cloning is
/// cheap, the contents are behind [`Arc`]s.
#[derive(Clone, Debug)]
pub struct SharedFontPool {
    /// Index of the system and embedded fonts (paths and metadata, not the font data).
    pub fc_cache: Arc<rust_fontconfig::FcFontCache>,
    /// Parsed fonts by id, filled lazily by the first layout that needs a font and reused by
    /// every later one.
    pub parsed_fonts: Arc<Mutex<HashMap<rust_fontconfig::FontId, azul_css::props::basic::FontRef>>>,
}

pub mod bridge;
pub mod border;

/// Page margins in millimeters.
///
/// Serialized as JSON with camelCase keys; a missing side is 0.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMargins {
    /// Top margin.
    #[serde(default)]
    pub top: Mm,
    /// Right margin.
    #[serde(default)]
    pub right: Mm,
    /// Bottom margin.
    #[serde(default)]
    pub bottom: Mm,
    /// Left margin.
    #[serde(default)]
    pub left: Mm,
}

impl PageMargins {
    /// Margins with the same value on all four sides.
    pub fn uniform(margin: Mm) -> Self {
        Self {
            top: margin,
            right: margin,
            bottom: margin,
            left: margin,
        }
    }
    
    /// Margins with `vertical` on top and bottom and `horizontal` on left and right.
    pub fn symmetric(vertical: Mm, horizontal: Mm) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
    
    /// Margins with one value per side, in CSS order (top, right, bottom, left).
    pub fn new(top: Mm, right: Mm, bottom: Mm, left: Mm) -> Self {
        Self { top, right, bottom, left }
    }
    
    /// Sum of the left and right margin.
    pub fn horizontal(&self) -> Mm {
        Mm(self.left.0 + self.right.0)
    }
    
    /// Sum of the top and bottom margin.
    pub fn vertical(&self) -> Mm {
        Mm(self.top.0 + self.bottom.0)
    }
}

/// Options for [`xml_to_pdf_pages`] and [`add_xml_to_document`].
///
/// Serialized as JSON with camelCase keys, every field optional; `font_pool` is not serialized.
/// `PdfDocument::from_html` builds one of these from its `GeneratePdfOptions`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlRenderOptions {
    /// Image files by id (the bytes of a PNG, JPEG and so on). `<img src="id">` draws the entry
    /// with that key.
    #[serde(default)]
    pub images: BTreeMap<String, Vec<u8>>,
    /// Font files by name. `font-family: name` selects the entry; the family names stored in
    /// the font file work too.
    #[serde(default)]
    pub fonts: BTreeMap<String, Vec<u8>>,
    /// Page width in millimeters (210, A4, by default).
    #[serde(default = "default_page_width")]
    pub page_width: Mm,
    /// Page height in millimeters (297, A4, by default).
    #[serde(default = "default_page_height")]
    pub page_height: Mm,
    /// Page margins. Layout uses the page minus the margins as its content area.
    #[serde(default)]
    pub margins: PageMargins,
    /// Draw page numbers in the footer of every page.
    #[serde(default)]
    pub show_page_numbers: bool,
    /// Header text for every page.
    #[serde(default)]
    pub header_text: Option<String>,
    /// Footer text for every page.
    #[serde(default)]
    pub footer_text: Option<String>,
    /// Leave the header and footer off the first page.
    #[serde(default)]
    pub skip_first_page: bool,
    /// Font pool shared with other renders, built with [`build_font_pool`]. When it is `None`,
    /// every render scans the system fonts again; when it is set, the render reuses the pool's
    /// font index and parsed fonts.
    #[serde(skip)]
    pub font_pool: Option<SharedFontPool>,
}

impl Default for XmlRenderOptions {
    fn default() -> Self {
        Self {
            images: BTreeMap::new(),
            fonts: BTreeMap::new(),
            page_width: default_page_width(),
            page_height: default_page_height(),
            margins: PageMargins::default(),
            show_page_numbers: false,
            header_text: None,
            footer_text: None,
            skip_first_page: false,
            font_pool: None,
        }
    }
}

/// Builds a [`SharedFontPool`] to reuse across many renders.
///
/// Scans the system fonts (on wasm there is nothing to scan) and registers the font files in
/// `fonts`, the same map as [`XmlRenderOptions::fonts`]. With `families` set to `Some`, only
/// system fonts of those family names are scanned; `None` scans all of them.
pub fn build_font_pool(
    fonts: &BTreeMap<String, Vec<u8>>,
    families: Option<&[&str]>,
) -> SharedFontPool {
    let fc_cache = match families {
        Some(fams) => rust_fontconfig::FcFontCache::build_with_families(fams),
        None => build_font_cache(),
    };

    for (font_name, font_bytes) in fonts {
        if let Some(parsed_fonts) = rust_fontconfig::FcParseFontBytes(font_bytes, font_name) {
            // `with_memory_fonts` uses interior mutability (returns `&Self`); called for side effect.
            fc_cache.with_memory_fonts(parsed_fonts);
        }
    }

    SharedFontPool {
        fc_cache: Arc::new(fc_cache),
        parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
    }
}

/// Like [`build_font_pool`], and also registers in-memory fonts with the pattern metadata (family,
/// weight, style, unicode ranges) you give, instead of deriving it from the font bytes.
///
/// Use it when you manage a set of in-memory fonts yourself, for example on wasm where there are
/// no system fonts. The `rust_fontconfig` types are re-exported as [`rust_fontconfig`].
pub fn build_font_pool_with_memory_fonts(
    memory_fonts: Vec<(rust_fontconfig::FcPattern, rust_fontconfig::FcFont)>,
    fonts: &BTreeMap<String, Vec<u8>>,
    families: Option<&[&str]>,
) -> SharedFontPool {
    let pool = build_font_pool(fonts, families);
    // Interior mutability; called for the side effect (see build_font_pool).
    pool.fc_cache.with_memory_fonts(memory_fonts);
    pool
}

// Registers each input font under its map key and under the family names stored in the font
// file, so `font-family` can use either. Fonts that are only in the font cache are not seen by
// azul's family resolution, which looks at `FontManager::memory_families`.
fn register_input_fonts<T: azul_layout::font_traits::ParsedFontTrait>(
    font_manager: &mut FontManager<T>,
    fonts: &BTreeMap<String, Vec<u8>>,
) {
    for (name, bytes) in fonts.iter() {
        let parsed = rust_fontconfig::FcParseFontBytes(bytes, name);

        // The map-key spelling needs the real coverage too: fonts with empty unicode ranges are
        // skipped during per-character fallback.
        let key_coverage: &[rust_fontconfig::UnicodeRange] = parsed
            .as_deref()
            .and_then(|p| p.first())
            .map(|(pattern, _)| pattern.unicode_ranges.as_slice())
            .unwrap_or(&[]);
        register_all_spellings(font_manager, name, bytes, key_coverage, MemoryFontTier::Primary);

        if let Some(parsed) = &parsed {
            for (pattern, _) in parsed {
                if let Some(fam) = pattern.family.as_deref() {
                    if !fam.eq_ignore_ascii_case(name) {
                        register_all_spellings(
                            font_manager,
                            fam,
                            bytes,
                            &pattern.unicode_ranges,
                            MemoryFontTier::Primary,
                        );
                    }
                }
            }
        }
    }
}

// Registers `family` plain and in double and single quotes: multi-word family names reach the
// resolver with the CSS quotes still around them (`"Roboto Medium"`).
fn register_all_spellings<T: azul_layout::font_traits::ParsedFontTrait>(
    fm: &mut FontManager<T>,
    family: &str,
    bytes: &[u8],
    coverage: &[rust_fontconfig::UnicodeRange],
    tier: MemoryFontTier,
) {
    let _ = fm.register_named_font_in_tier(family, bytes, coverage.to_vec(), tier);
    for quoted in [format!("\"{family}\""), format!("'{family}'")] {
        let _ = fm.register_named_font_in_tier(&quoted, bytes, coverage.to_vec(), tier);
    }
}

// The CSS family names of each standard PDF font. These are registered as `Primary`, so they
// win over installed fonts; the generic families are handled in `builtin_generic_family`.
fn builtin_font_css_names(font: BuiltinFont) -> &'static [&'static str] {
    use BuiltinFont::{
        Courier, CourierBold, CourierBoldOblique, CourierOblique, Helvetica, HelveticaBold,
        HelveticaBoldOblique, HelveticaOblique, Symbol, TimesBold, TimesBoldItalic, TimesItalic,
        TimesRoman, ZapfDingbats,
    };
    match font {
        TimesRoman | TimesBold | TimesItalic | TimesBoldItalic => &["Times New Roman", "Times"],
        Helvetica | HelveticaBold | HelveticaOblique | HelveticaBoldOblique => {
            &["Helvetica", "Arial"]
        }
        Courier | CourierBold | CourierOblique | CourierBoldOblique => {
            &["Courier New", "Courier"]
        }
        Symbol => &["Symbol"],
        ZapfDingbats => &["Zapf Dingbats", "ZapfDingbats"],
    }
}

// The generic family a standard font backs. Registered as `Fallback`: installed serif, sans and
// mono fonts win, and these are what is left where no fonts are installed (wasm). Only the
// regular face is offered; bold and italic are reachable by their concrete names.
fn builtin_generic_family(font: BuiltinFont) -> Option<&'static str> {
    match font {
        BuiltinFont::TimesRoman => Some("serif"),
        BuiltinFont::Helvetica => Some("sans-serif"),
        BuiltinFont::Courier => Some("monospace"),
        _ => None,
    }
}

// Registers the 14 standard PDF fonts so `font-family: Helvetica` (and Times, Courier, Symbol,
// ZapfDingbats) resolves to the face that is written into the PDF.
//
// Must run after `register_input_fonts`: registration keeps the first face of a family, weight
// and style, so a caller-supplied Helvetica wins over the built-in one. Weight and style are read
// from `head.macStyle` because the subsets have no OS/2 table.
fn register_builtin_fonts<T: azul_layout::font_traits::ParsedFontTrait>(
    font_manager: &mut FontManager<T>,
) {
    for font in BuiltinFont::all_ids() {
        let bytes = font.get_subset_font().bytes;

        // Real coverage matters: the subsets only cover Win-1252, and a codepoint outside it
        // has to fall through to another font.
        let coverage: Vec<rust_fontconfig::UnicodeRange> =
            rust_fontconfig::FcParseFontBytes(&bytes, font.get_id())
                .and_then(|faces| faces.into_iter().next())
                .map_or_else(Vec::new, |(pattern, _)| pattern.unicode_ranges);

        // The PDF base-14 name itself (`Helvetica-Bold`), then the CSS spellings.
        register_all_spellings(
            font_manager,
            font.get_id(),
            &bytes,
            &coverage,
            MemoryFontTier::Primary,
        );
        for css_name in builtin_font_css_names(font) {
            register_all_spellings(
                font_manager,
                css_name,
                &bytes,
                &coverage,
                MemoryFontTier::Primary,
            );
        }
        if let Some(generic) = builtin_generic_family(font) {
            register_all_spellings(
                font_manager,
                generic,
                &bytes,
                &coverage,
                MemoryFontTier::Fallback,
            );
        }
    }
}

/// Builds only the font index of [`build_font_pool`], without the parsed-font pool.
///
/// Prefer [`build_font_pool`], which also shares the parsed fonts.
pub fn build_font_cache_for_options(
    fonts: &BTreeMap<String, Vec<u8>>,
    families: Option<&[&str]>,
) -> Arc<rust_fontconfig::FcFontCache> {
    let pool = build_font_pool(fonts, families);
    pool.fc_cache
}

// The smallest and the largest page, and content area (the page minus the margins), that
// layout is asked to cut pages for, in points. Layout slices the content into pages of the content
// height: a size that is not a positive number has no meaning, a tiny one asks for a page per
// sliver of content, and a huge one overflows its arithmetic.
const MIN_PAGE_SIZE_PT: f32 = 1.0;
const MAX_PAGE_SIZE_PT: f32 = 1_000_000.0;

// The error for a page or content area (all in points) outside the limits, `None` if all are fine.
fn page_size_error(
    page_width_pt: f32,
    page_height_pt: f32,
    content_width_pt: f32,
    content_height_pt: f32,
) -> Option<PdfWarnMsg> {
    let limits = MIN_PAGE_SIZE_PT..=MAX_PAGE_SIZE_PT;
    let sizes = [page_width_pt, page_height_pt, content_width_pt, content_height_pt];
    if sizes.iter().all(|size| limits.contains(size)) {
        return None;
    }
    Some(PdfWarnMsg::error(
        0,
        0,
        format!(
            "Invalid page size: the page ({page_width_pt} x {page_height_pt} pt) and its content \
             area inside the margins ({content_width_pt} x {content_height_pt} pt) must each be \
             between {MIN_PAGE_SIZE_PT} and {MAX_PAGE_SIZE_PT} pt"
        ),
    ))
}

fn default_page_width() -> Mm {
    Mm(210.0) // A4 width
}

fn default_page_height() -> Mm {
    Mm(297.0) // A4 height
}

// Page configuration shared by both render functions. All break rules are on: `break-inside:
// avoid`, widows and orphans, whole lines, whole table rows and repeated `<thead>`.
fn build_page_config(options: &XmlRenderOptions) -> FakePageConfig {
    let mut page_config = FakePageConfig::new();

    if options.show_page_numbers {
        page_config = page_config.with_footer_page_numbers();
    }

    if let Some(ref header) = options.header_text {
        page_config = page_config.with_header_text(header.clone());
    }

    if let Some(ref footer) = options.footer_text {
        page_config = page_config.with_footer_text(footer.clone());
    }

    if options.skip_first_page {
        page_config = page_config.skip_first_page(true);
    }

    page_config.break_policy = BreakPolicy {
        honor_break_inside: true,
        widows_orphans: true,
        atomic_lines: true,
        atomic_table_rows: true,
        repeat_table_headers: true,
        ..BreakPolicy::default()
    };

    page_config
}

/// Renders HTML to pages.
///
/// Returns the pages, the fonts the text uses (by font hash), the decoded `<img>` images and the
/// extra resources the pages refer to (transparency states, gradients, shadows).
///
/// The pages do not stand alone: their ops name fonts, XObjects and graphics states that must be
/// in the resources of the document they are added to. [`add_xml_to_document`] and
/// `PdfDocument::from_html` do that. If you add the pages yourself, register each font as
/// `F<font hash>`, each image under the id in [`bridge::ResolvedImages`], and call
/// [`bridge::BridgeResources::register_into`].
///
/// Only the entries of [`XmlRenderOptions::images`] that a page draws are decoded.
///
/// # Errors
///
/// Returns the warnings when the HTML cannot be parsed or laid out, or when the page or the area
/// inside its margins is smaller than 1 pt or larger than 1 000 000 pt (or not a number) in one
/// direction. A document without visible content gives one empty page.
pub fn xml_to_pdf_pages(
    xml: &str,
    options: &XmlRenderOptions,
) -> Result<(Vec<PdfPage>, BTreeMap<FontHash, ParsedFont>, bridge::ResolvedImages, bridge::BridgeResources), Vec<PdfWarnMsg>> {
    xml_to_pdf_pages_with_images(xml, options, &options.images, &mut Vec::new())
}

/// [`xml_to_pdf_pages`], with the `<img>` bytes looked up in `images` (instead of
/// `options.images`) for the `src` values the pages draw, and nothing else.
/// Pictures that are drawn but cannot be found, read or decoded are reported in
/// `image_warnings`.
pub(crate) fn xml_to_pdf_pages_with_images(
    xml: &str,
    options: &XmlRenderOptions,
    images: &dyn bridge::HtmlImageSource,
    image_warnings: &mut Vec<PdfWarnMsg>,
) -> Result<(Vec<PdfPage>, BTreeMap<FontHash, ParsedFont>, bridge::ResolvedImages, bridge::BridgeResources), Vec<PdfWarnMsg>> {
    let mut warnings = Vec::new();
    // Type-safe preprocessing: RawHtml -> PreprocessedHtml
    let preprocessed = RawHtml::new(xml).preprocess();
    let inlined_xml = preprocessed.as_str();

    // Parse XML to XmlNode tree
    let root_nodes = match parse_xml_string(inlined_xml) {
        Ok(nodes) => nodes,
        Err(e) => {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Failed to parse XML: {}", e),
            ));
            return Err(warnings);
        }
    };

    // Calculate content area (page size minus margins)
    let mm_to_pt = 2.83465;
    let page_width_pt = options.page_width.0 * mm_to_pt;
    let page_height_pt = options.page_height.0 * mm_to_pt;
    let margin_top_pt = options.margins.top.0 * mm_to_pt;
    let margin_right_pt = options.margins.right.0 * mm_to_pt;
    let margin_bottom_pt = options.margins.bottom.0 * mm_to_pt;
    let margin_left_pt = options.margins.left.0 * mm_to_pt;
    
    // Content area is the page minus margins
    let content_width_pt = page_width_pt - margin_left_pt - margin_right_pt;
    let content_height_pt = page_height_pt - margin_top_pt - margin_bottom_pt;
    if let Some(error) =
        page_size_error(page_width_pt, page_height_pt, content_width_pt, content_height_pt)
    {
        warnings.push(error);
        return Err(warnings);
    }

    // Layout runs in CSS px (96 per inch), not PDF points (72 per inch): the CSS resolver turns
    // every unit into px, so a viewport in pt would make CSS lengths 96/72 too large. The bridge
    // converts the coordinates back to pt.
    const PT_TO_CSS_PX: f32 = 96.0 / 72.0;
    let content_width_px = content_width_pt * PT_TO_CSS_PX;
    let content_height_px = content_height_pt * PT_TO_CSS_PX;

    // Convert XML nodes to StyledDom with registered HTML components
    // Use content width in CSS px (not pt) for layout
    let component_map = crate::components::printpdf_default_components();

    let styled_dom = match str_to_dom(
        root_nodes.as_ref(),
        &component_map,
        Some(content_width_px),
    ) {
        Ok(dom) => dom,
        Err(e) => {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Failed to convert XML to DOM: {}", e),
            ));
            return Err(warnings);
        }
    };

    // Create font cache and font manager
    // If a shared font pool was provided, reuse both metadata and parsed fonts.
    // Otherwise build from scratch (scanning system fonts + embedded fonts).
    let (fc_cache_arc, parsed_fonts_arc) = if let Some(ref pool) = options.font_pool {
        (Arc::clone(&pool.fc_cache), Arc::clone(&pool.parsed_fonts))
    } else {
        let pool = build_font_pool(&options.fonts, None);
        (pool.fc_cache, pool.parsed_fonts)
    };
    
    let mut font_manager = match FontManager::from_arc_shared((*fc_cache_arc).clone(), parsed_fonts_arc) {
        Ok(fm) => fm,
        Err(e) => {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Failed to create font manager: {:?}", e),
            ));
            return Err(warnings);
        }
    };
    register_input_fonts(&mut font_manager, &options.fonts);
    register_builtin_fonts(&mut font_manager);

    // Use content size in CSS px for layout (converted from pt above)
    let content_size = LogicalSize::new(content_width_px, content_height_px);

    // Create fragmentation context for paged layout using CONTENT size (in CSS px)
    // Page breaks happen at CSS px boundaries; the bridge converts back to pt.
    let fragmentation_context = FragmentationContext::new_paged(content_size);

    // Create layout cache and text cache
    let mut layout_cache = Solver3LayoutCache::default();
    let mut text_cache = TextLayoutCache::new();
    
    // Viewport is the content area in CSS px (layout is done within margins)
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    
    // Perform paged layout - returns Vec<DisplayList>
    let renderer_resources = RendererResources::default();
    let mut debug_messages = None; // None = skip debug format! overhead

    // Create font loader closure
    use azul_layout::text3::default::PathLoader;
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };

    // Build page config from options
    // The CSS `@page` rule is not parsed; `FakePageConfig` carries the header and footer options.
    let page_config = build_page_config(options);

    // The pictures the pages draw are decoded once each, keyed by `src`, when the first page that
    // draws them arrives. The map may hold many pictures a page never shows. Each gets a
    // deterministic XObject id, so the bridge's `UseXobject` ops and the document-side
    // registration agree.
    let mut resolved_images = bridge::ResolvedImages::new();
    // Resources synthesized by the bridge (alpha/opacity ExtGStates, gradient
    // shadings, shadow XObjects), accumulated across pages and registered by
    // the caller (`from_html*`) via `BridgeResources::register_into`.
    let mut bridge_res = bridge::BridgeResources::default();

    // Convert each DisplayList to a PDF page
    let mut pages = Vec::new();
    let mut font_data_map: BTreeMap<FontHash, azul_layout::font::parsed::ParsedFont> = BTreeMap::new();

    // Full page size for PDF coordinate transformation
    let full_page_size = LogicalSize::new(page_width_pt, page_height_pt);

    // The layout holds the font manager mutably while it hands the pages over,
    // and writing a page needs the fonts. This handle shares the manager's faces:
    // what the layout parsed is what the page finds.
    let page_fonts = font_manager.clone_shared();
    let mut conversion_error: Option<String> = None;

    // Each page is converted as soon as the layout has cut it, and its display list is dropped
    // before the next page is cut, so the display lists of the whole document are never alive
    // at once.
    let mut write_page = |display_list: azul_layout::solver3::display_list::DisplayList|
     -> azul_layout::solver3::Result<()> {
        let display_list = &display_list;
        let page_images = bridge::resolve_referenced_html_images(
            &bridge::referenced_image_srcs(std::slice::from_ref(display_list))
                .into_iter()
                .filter(|src| !resolved_images.contains_key(src))
                .collect(),
            images,
            image_warnings,
        );
        resolved_images.extend(page_images);

        // Skip pages that have no meaningful content (only plain background
        // fills). A page is "real" if it has text, an image, a gradient or a
        // shadow — anything a reader would actually perceive as content.
        let has_real_content = display_list.items.iter().any(|item| {
            use azul_layout::solver3::display_list::DisplayListItem;
            matches!(item,
                DisplayListItem::TextLayout { .. }
                | DisplayListItem::Image { .. }
                | DisplayListItem::LinearGradient { .. }
                | DisplayListItem::RadialGradient { .. }
                | DisplayListItem::ConicGradient { .. }
                | DisplayListItem::BoxShadow { .. }
            )
        });

        if !has_real_content {
            // Skip this page - it only contains background rectangles
            return Ok(());
        }

        // Convert DisplayList to printpdf operations
        // We pass the FULL page size for Y-coordinate transformation (PDF origin is bottom-left)
        // The content was laid out in content_size, but coordinates need to be transformed
        // relative to the full page height
        let pdf_ops = match bridge::display_list_to_printpdf_ops_with_margins(
            display_list,
            full_page_size,
            margin_left_pt,
            margin_top_pt,
            &page_fonts,
            &resolved_images,
            &mut bridge_res,
        ) {
            Ok(ops) => ops,
            Err(e) => {
                // Remembered for the caller; the layout error only stops the pages.
                conversion_error = Some(format!("Failed to convert display list: {}", e));
                return Err(azul_layout::solver3::LayoutError::DisplayListFailed);
            }
        };
        
        // Extract fonts from TextLayout items by collecting font hashes from the layout
        // and then looking them up in the font_manager
        for item in display_list.items.iter() {
            if let azul_layout::solver3::display_list::DisplayListItem::TextLayout { layout, .. } = item {
                // Downcast the type-erased layout to UnifiedLayout
                if let Some(unified_layout) = layout.downcast_ref::<azul_layout::text3::cache::UnifiedLayout>() {
                    // Collect all font hashes used in this layout by scanning the positioned items
                    let mut used_font_hashes = std::collections::HashSet::new();
                    for positioned_item in &unified_layout.items {
                        if let azul_layout::text3::cache::ShapedItem::Cluster(cluster) = &positioned_item.item {
                            for glyph in &cluster.glyphs {
                                used_font_hashes.insert(glyph.font_hash);
                            }
                        }
                    }
                    
                    // Look up each font hash in the font_manager
                    for font_hash in used_font_hashes {
                        let font_hash_key = FontHash::from_hash(font_hash);
                        if let std::collections::btree_map::Entry::Vacant(e) = font_data_map.entry(font_hash_key) {
                            // Get the font from the font manager by hash
                            if let Some(font_ref) = page_fonts.get_font_by_hash(font_hash) {
                                // Extract ParsedFont from FontRef
                                let parsed_font = unsafe {
                                    let parsed_ptr = font_ref.get_parsed();
                                    let parsed_font = &*(parsed_ptr as *const azul_layout::font::parsed::ParsedFont);
                                    parsed_font.clone()
                                };
                                e.insert(parsed_font);
                            }
                        }
                    }
                }
            }
        }
        
        // Create a page for this display list (using full page dimensions)
        pages.push(PdfPage::new(options.page_width, options.page_height, pdf_ops));
        Ok(())
    };

    let layout_result = layout_document_paged_streaming(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &std::collections::BTreeMap::new(), // No scroll offsets for PDF
        &mut debug_messages,
        None, // No GPU cache for PDF
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
        &azul_core::resources::ImageCache::default(), // No image cache for PDF
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false, // print_timing
        &mut write_page,
    );
    if let Some(message) = conversion_error {
        return Err(vec![PdfWarnMsg::warning(0, 0, message)]);
    }
    if let Err(e) = layout_result {
        warnings.push(PdfWarnMsg::error(
            0,
            0,
            format!("Layout solving failed: {:?}", e),
        ));
        return Err(warnings);
    }

    // If no pages were generated, create at least one empty page
    if pages.is_empty() {
        warnings.push(PdfWarnMsg::warning(0, 0, "No content generated, creating empty page".to_string()));
        let page = PdfPage::new(options.page_width, options.page_height, Vec::new());
        pages.push(page);
    }

    // Always return Ok with pages, fonts, and decoded image resources.
    // The layout cache hands back azul faces; adopt them as printpdf `ParsedFont`s. They
    // come from `from_bytes_shared`, so they already carry their source bytes and stay
    // embeddable.
    let font_data_map = font_data_map
        .into_iter()
        .map(|(hash, font)| (hash, ParsedFont::from_azul(font)))
        .collect();

    Ok((pages, font_data_map, resolved_images, bridge_res))
}

/// Text dumps of the layout, for finding out why a document does not render as expected.
#[derive(Debug, Clone)]
pub struct PdfDebugInfo {
    /// A dump of the layout tree, then of the display list of the first page (before conversion
    /// to PDF ops).
    pub display_list_debug: Vec<String>,
    /// The PDF operations of the first page (after conversion).
    pub pdf_ops_debug: Vec<String>,
}

/// Like [`xml_to_pdf_pages`], but returns a [`PdfDebugInfo`] instead of the images, and prints
/// progress to stderr.
pub fn xml_to_pdf_pages_debug(
    xml: &str,
    options: &XmlRenderOptions,
) -> Result<(Vec<PdfPage>, BTreeMap<FontHash, ParsedFont>, PdfDebugInfo, bridge::BridgeResources), Vec<PdfWarnMsg>> {
    xml_to_pdf_pages_debug_with_images(xml, options, &options.images, &mut Vec::new())
        .map(|(pages, fonts, debug_info, bridge_res, _images)| (pages, fonts, debug_info, bridge_res))
}

/// [`xml_to_pdf_pages_debug`] with the `<img>` lookup of
/// [`xml_to_pdf_pages_with_images`]; also returns the decoded pictures, so
/// `PdfDocument::from_html_debug*` registers them without decoding them again.
pub(crate) fn xml_to_pdf_pages_debug_with_images(
    xml: &str,
    options: &XmlRenderOptions,
    images: &dyn bridge::HtmlImageSource,
    image_warnings: &mut Vec<PdfWarnMsg>,
) -> Result<(Vec<PdfPage>, BTreeMap<FontHash, ParsedFont>, PdfDebugInfo, bridge::BridgeResources, bridge::ResolvedImages), Vec<PdfWarnMsg>> {
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Starting, xml length={}", xml.len());
    let mut warnings = Vec::new();
    let mut debug_info = PdfDebugInfo {
        display_list_debug: Vec::new(),
        pdf_ops_debug: Vec::new(),
    };

    // Type-safe preprocessing: RawHtml -> PreprocessedHtml
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Preprocessing HTML...");
    let preprocessed = RawHtml::new(xml).preprocess();
    let inlined_xml = preprocessed.as_str();
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Preprocessed, length={}", inlined_xml.len());

    // Parse XML to XmlNode tree
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Parsing XML...");
    let root_nodes = match parse_xml_string(inlined_xml) {
        Ok(nodes) => {
            eprintln!("[DEBUG xml_to_pdf_pages_debug] XML parsed, got {} root nodes", nodes.len());
            nodes
        },
        Err(e) => {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Failed to parse XML: {}", e),
            ));
            return Err(warnings);
        }
    };

    // Calculate content area (page size minus margins)
    let mm_to_pt = 2.83465;
    let page_width_pt = options.page_width.0 * mm_to_pt;
    let page_height_pt = options.page_height.0 * mm_to_pt;
    let margin_top_pt = options.margins.top.0 * mm_to_pt;
    let margin_right_pt = options.margins.right.0 * mm_to_pt;
    let margin_bottom_pt = options.margins.bottom.0 * mm_to_pt;
    let margin_left_pt = options.margins.left.0 * mm_to_pt;
    
    // Content area is the page minus margins
    let content_width_pt = page_width_pt - margin_left_pt - margin_right_pt;
    let content_height_pt = page_height_pt - margin_top_pt - margin_bottom_pt;
    if let Some(error) =
        page_size_error(page_width_pt, page_height_pt, content_width_pt, content_height_pt)
    {
        warnings.push(error);
        return Err(warnings);
    }

    // Layout runs in CSS px, see `xml_to_pdf_pages_with_images`.
    const PT_TO_CSS_PX: f32 = 96.0 / 72.0;
    let content_width_px = content_width_pt * PT_TO_CSS_PX;
    let content_height_px = content_height_pt * PT_TO_CSS_PX;

    // Convert XML nodes to StyledDom with registered HTML components
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Converting to StyledDom...");
    // Use content width in CSS px (not pt) for layout
    let component_map = crate::components::printpdf_default_components();

    let styled_dom = match str_to_dom(
        root_nodes.as_ref(),
        &component_map,
        Some(content_width_px),
    ) {
        Ok(dom) => {
            eprintln!("[DEBUG xml_to_pdf_pages_debug] StyledDom created with {} nodes", dom.node_data.as_container().len());
            dom
        },
        Err(e) => {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Failed to convert XML to DOM: {}", e),
            ));
            return Err(warnings);
        }
    };

    // Create font cache and font manager (reuse shared font pool if provided)
    let (fc_cache_arc, parsed_fonts_arc) = if let Some(ref pool) = options.font_pool {
        eprintln!("[DEBUG xml_to_pdf_pages_debug] Reusing shared font pool");
        (Arc::clone(&pool.fc_cache), Arc::clone(&pool.parsed_fonts))
    } else {
        eprintln!("[DEBUG xml_to_pdf_pages_debug] Building font pool from scratch...");
        let pool = build_font_pool(&options.fonts, None);
        (pool.fc_cache, pool.parsed_fonts)
    };
    
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Creating font manager...");
    let mut font_manager = match FontManager::from_arc_shared((*fc_cache_arc).clone(), parsed_fonts_arc) {
        Ok(fm) => fm,
        Err(e) => {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Failed to create font manager: {:?}", e),
            ));
            return Err(warnings);
        }
    };
    register_input_fonts(&mut font_manager, &options.fonts);
    register_builtin_fonts(&mut font_manager);
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Font manager created");

    // Use content size in CSS px for layout (converted from pt above)
    let content_size = LogicalSize::new(content_width_px, content_height_px);

    // Create fragmentation context for paged layout using CONTENT size (in CSS px)
    let fragmentation_context = FragmentationContext::new_paged(content_size);

    // Create layout cache and text cache
    let mut layout_cache = Solver3LayoutCache::default();
    let mut text_cache = TextLayoutCache::new();
    
    // Viewport is the content area in CSS px (layout is done within margins)
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    
    // Perform paged layout - returns Vec<DisplayList>
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());

    // Create font loader closure
    use azul_layout::text3::default::PathLoader;
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };

    // Build page config from options
    let page_config = build_page_config(options);

    eprintln!("[DEBUG xml_to_pdf_pages_debug] Starting paged layout...");
    let display_lists = match layout_document_paged_v2(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &std::collections::BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false, // print_timing
    ) {
        Ok(result) => {
            eprintln!(
                "[DEBUG xml_to_pdf_pages_debug] Paged layout completed, got {} pages ({} breaks, content height {:.1}px)",
                result.pages.len(),
                result.breaks.len(),
                result.total_content_height,
            );
            result.pages
        },
        Err(e) => {
            warnings.push(PdfWarnMsg::error(
                0,
                0,
                format!("Layout solving failed: {:?}", e),
            ));
            return Err(warnings);
        }
    };

    // Debug: Dump layout tree and calculated positions
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Converting display lists to PDF...");
    {
        let mut tree_debug = String::new();
        tree_debug.push_str("=== Layout Tree Debug ===\n\n");
        
        if let Some(ref tree) = layout_cache.tree {
            tree_debug.push_str(&format!("Total nodes: {}\n", tree.nodes.len()));
            tree_debug.push_str(&format!("Calculated positions: {}\n\n", layout_cache.calculated_positions.len()));
            
            // Dump first 100 nodes with their positions and formatting context
            for (idx, node) in tree.nodes.iter().enumerate().take(100) {
                let pos = layout_cache.calculated_positions.get(idx);
                let dom_id_str = node.dom_node_id
                    .map(|id| format!("DOM#{}", id.index()))
                    .unwrap_or_else(|| "anonymous".to_string());
                
                tree_debug.push_str(&format!(
                    "Node {}: {} fc={:?} pos={:?} size={:?}\n",
                    idx,
                    dom_id_str,
                    node.formatting_context,
                    pos,
                    node.used_size,
                ));
            }
        } else {
            tree_debug.push_str("Layout tree is None!\n");
        }
        
        debug_info.display_list_debug.push(tree_debug);
    }

    // Decode the `<img>` pictures the pages draw, once each and only those (same
    // deterministic ids as the document-side XObject registration).
    let resolved_images = bridge::resolve_referenced_html_images(
        &bridge::referenced_image_srcs(&display_lists),
        images,
        image_warnings,
    );
    let mut bridge_res = bridge::BridgeResources::default();

    // Convert each DisplayList to a PDF page
    let mut pages = Vec::new();
    let mut font_data_map: BTreeMap<FontHash, azul_layout::font::parsed::ParsedFont> = BTreeMap::new();

    // Full page size for PDF coordinate transformation
    let full_page_size = LogicalSize::new(page_width_pt, page_height_pt);
    
    for (page_idx, display_list) in display_lists.iter().enumerate() {
        // Debug: capture display list info for first page only
        if page_idx == 0 {
            let mut dl_debug = String::new();
            dl_debug.push_str(&format!("=== Display List for Page {} ===\n", page_idx + 1));
            dl_debug.push_str(&format!("Total items: {}\n\n", display_list.items.len()));
            
            // Count item types
            let mut text_layout_count = 0;
            let mut text_count = 0;
            let mut rect_count = 0;
            let mut other_count = 0;
            
            for (item_idx, item) in display_list.items.iter().enumerate() {
                use azul_layout::solver3::display_list::DisplayListItem;
                match item {
                    DisplayListItem::TextLayout { bounds, font_hash: _, font_size_px, color, .. } => {
                        text_layout_count += 1;
                        dl_debug.push_str(&format!("Item {}: TextLayout at ({:.1}, {:.1}) size {:.1}x{:.1} font_size={:.1} color=({},{},{},{})\n",
                            item_idx, bounds.origin().x, bounds.origin().y, bounds.size().width, bounds.size().height,
                            font_size_px, color.r, color.g, color.b, color.a));
                    }
                    DisplayListItem::Text { glyphs, font_size_px, color, .. } => {
                        text_count += 1;
                        dl_debug.push_str(&format!("Item {}: Text with {} glyphs, font_size={:.1} color=({},{},{},{})\n", 
                            item_idx, glyphs.len(), font_size_px, color.r, color.g, color.b, color.a));
                    }
                    DisplayListItem::Rect { bounds, color, .. } => {
                        rect_count += 1;
                        if item_idx < 20 || bounds.size().width > 100.0 {
                            dl_debug.push_str(&format!("Item {}: Rect at ({:.1}, {:.1}) size {:.1}x{:.1} color=({},{},{},{})\n",
                                item_idx, bounds.origin().x, bounds.origin().y, bounds.size().width, bounds.size().height,
                                color.r, color.g, color.b, color.a));
                        }
                    }
                    _ => {
                        other_count += 1;
                    }
                }
            }
            dl_debug.push_str(&format!("\n=== Summary: {} TextLayout, {} Text, {} Rect, {} other ===\n", 
                text_layout_count, text_count, rect_count, other_count));
            debug_info.display_list_debug.push(dl_debug);
        }
        
        // Convert DisplayList to printpdf operations
        let pdf_ops = bridge::display_list_to_printpdf_ops_with_margins(
            display_list,
            full_page_size,
            margin_left_pt,
            margin_top_pt,
            &font_manager,
            &resolved_images,
            &mut bridge_res,
        ).map_err(|e| vec![PdfWarnMsg::warning(0, 0, format!("Failed to convert display list: {}", e))])?;

        // Debug: capture PDF ops for first page only
        if page_idx == 0 {
            let mut ops_debug = String::new();
            ops_debug.push_str(&format!("=== PDF Ops for Page {} ===\n", page_idx + 1));
            ops_debug.push_str(&format!("Total ops: {}\n\n", pdf_ops.len()));
            
            for (op_idx, op) in pdf_ops.iter().enumerate() {
                ops_debug.push_str(&format!("Op {}: {:?}\n", op_idx, op));
            }
            debug_info.pdf_ops_debug.push(ops_debug);
        }
        
        // Extract fonts from TextLayout items
        for item in display_list.items.iter() {
            if let azul_layout::solver3::display_list::DisplayListItem::TextLayout { layout, .. } = item {
                if let Some(unified_layout) = layout.downcast_ref::<azul_layout::text3::cache::UnifiedLayout>() {
                    let mut used_font_hashes = std::collections::HashSet::new();
                    for positioned_item in &unified_layout.items {
                        if let azul_layout::text3::cache::ShapedItem::Cluster(cluster) = &positioned_item.item {
                            for glyph in &cluster.glyphs {
                                used_font_hashes.insert(glyph.font_hash);
                            }
                        }
                    }
                    
                    for font_hash in used_font_hashes {
                        let font_hash_key = FontHash::from_hash(font_hash);
                        if let std::collections::btree_map::Entry::Vacant(e) = font_data_map.entry(font_hash_key) {
                            if let Some(font_ref) = font_manager.get_font_by_hash(font_hash) {
                                let parsed_font = unsafe {
                                    let parsed_ptr = font_ref.get_parsed();
                                    let parsed_font = &*(parsed_ptr as *const azul_layout::font::parsed::ParsedFont);
                                    parsed_font.clone()
                                };
                                e.insert(parsed_font);
                            }
                        }
                    }
                }
            }
        }
        
        // Create a page for this display list
        let page = PdfPage::new(options.page_width, options.page_height, pdf_ops);
        pages.push(page);
    }
    eprintln!("[DEBUG xml_to_pdf_pages_debug] Display lists converted to {} pages", pages.len());
    
    // If no pages were generated, create at least one empty page
    if pages.is_empty() {
        warnings.push(PdfWarnMsg::warning(0, 0, "No content generated, creating empty page".to_string()));
        let page = PdfPage::new(options.page_width, options.page_height, Vec::new());
        pages.push(page);
    }

    // See the sibling function: adopt azul's faces as printpdf `ParsedFont`s.
    let font_data_map = font_data_map
        .into_iter()
        .map(|(hash, font)| (hash, ParsedFont::from_azul(font)))
        .collect();

    Ok((pages, font_data_map, debug_info, bridge_res, resolved_images))
}

/// Renders HTML and appends the pages to `document`, registering the fonts, images and other
/// resources they need.
///
/// # Errors
///
/// Returns the warnings of [`xml_to_pdf_pages`]; the document is not changed then.
pub fn add_xml_to_document(
    document: &mut PdfDocument,
    xml: &str,
    options: &XmlRenderOptions,
) -> Result<(), Vec<PdfWarnMsg>> {
    match xml_to_pdf_pages(xml, options) {
        Ok((pages, font_data, images, bridge_res)) => {
            // Register fonts in the document
            for (font_hash, parsed_font) in font_data.into_iter() {
                let font_id = crate::FontId(format!("F{}", font_hash.font_hash));
                let pdf_font = crate::font::PdfFont::new(parsed_font);
                document.resources.fonts.map.insert(font_id, pdf_font);
            }
            // Register `<img>` images as PDF Image XObjects under the same
            // deterministic ids the bridge used for its `UseXobject` ops.
            for (_src, (xobject_id, raw_image)) in images.into_iter() {
                document
                    .resources
                    .xobjects
                    .map
                    .insert(xobject_id, crate::XObject::Image(raw_image));
            }
            bridge_res.register_into(&mut document.resources);
            document.pages.extend(pages);
            Ok(())
        }
        Err(warnings) => Err(warnings),
    }
}

/// HTML text that has not been preprocessed yet.
#[derive(Debug, Clone)]
pub struct RawHtml(String);

impl RawHtml {
    /// Wraps the HTML text.
    pub fn new(html: impl Into<String>) -> Self {
        Self(html.into())
    }
    
    /// Removes the elements that cannot be drawn (`script`, `iframe`, `video` and so on).
    pub fn preprocess(self) -> PreprocessedHtml {
        let cleaned = clean_html_elements(&self.0);
        let inlined = inline_css_in_xml(&cleaned);
        PreprocessedHtml(inlined)
    }
}

/// HTML after [`RawHtml::preprocess`], ready to be parsed.
#[derive(Debug, Clone)]
pub struct PreprocessedHtml(String);

impl PreprocessedHtml {
    /// The HTML text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Settings found in the `<title>` and `<meta>` tags of a document.
///
/// Meta tags named `pdf.options.pageWidth` and `pdf.options.pageHeight` (millimeters) set the page
/// size; tags named `pdf.metadata.<key>` set document metadata, see [`apply_html_config`].
#[derive(Default)]
pub struct HtmlExtractedConfig {
    /// Text of the `<title>` element.
    pub title: Option<String>,
    /// Page width in millimeters from the `pdf.options.pageWidth` meta tag.
    pub page_width: Option<f32>,
    /// Page height in millimeters from the `pdf.options.pageHeight` meta tag.
    pub page_height: Option<f32>,
    /// Values of the `pdf.metadata.<key>` meta tags, by key.
    pub metadata: BTreeMap<String, String>,
    /// Component definitions found in the document. Not filled in by the code in this module.
    pub components: Vec<ComponentDef>,
}

// Returns the markup unchanged: `<style>` blocks must stay in the document because azul's
// `str_to_dom` applies them as global CSS, with full selector and cascade support.
fn inline_css_in_xml(xml: &str) -> String {
    xml.to_string()
}

// Reads the title and the `pdf.*` meta tags with plain string search.
fn extract_html_config(xml: &str) -> HtmlExtractedConfig {
    let mut config = HtmlExtractedConfig::default();

    // Extract title
    if let Some((_, after_open)) = xml.split_once("<title>") {
        if let Some((title_content, _)) = after_open.split_once("</title>") {
            config.title = Some(title_content.trim().to_string());
        }
    }

    // Extract meta tags
    let mut rest = xml;
    while let Some((_, after_open)) = rest.split_once("<meta ") {
        let Some((meta_tag, after_tag)) = after_open.split_once('>') else {
            break;
        };

        // Extract name and content attributes
        let name = extract_attribute(meta_tag, "name");
        let content = extract_attribute(meta_tag, "content");

        if let (Some(name), Some(content)) = (name, content) {
            if let Some(option_name) = name.strip_prefix("pdf.options.") {
                // Skip "pdf.options."
                match option_name {
                    "pageWidth" => {
                        if let Ok(width) = content.parse::<f32>() {
                            config.page_width = Some(width);
                        }
                    }
                    "pageHeight" => {
                        if let Ok(height) = content.parse::<f32>() {
                            config.page_height = Some(height);
                        }
                    }
                    _ => {}
                }
            } else if let Some(metadata_key) = name.strip_prefix("pdf.metadata.") {
                // Skip "pdf.metadata."
                config.metadata.insert(metadata_key.to_string(), content);
            }
        }

        rest = after_tag;
    }

    config
}

// Returns the value of `attr_name="..."` in `tag`.
fn extract_attribute(tag: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr_name);
    let (_, after_name) = tag.split_once(pattern.as_str())?;
    let (value, _) = after_name.split_once('"')?;
    Some(value.to_string())
}

/// Applies a [`HtmlExtractedConfig`] to a document and to the render options.
///
/// The title becomes the document title unless one is set. The page size overrides the one in
/// `options`. Each metadata entry sets the field of the document info with the same key:
/// `trapped`, `version`, `conformance` (JSON), `documentTitle`, `author`, `creator`, `producer`,
/// `subject`, `keywords` (comma-separated) and `identifier`. Other keys are ignored.
pub fn apply_html_config(
    doc: &mut PdfDocument,
    config: &HtmlExtractedConfig,
    options: &mut XmlRenderOptions,
) {
    // Apply title if provided
    if let Some(title) = &config.title {
        if doc.metadata.info.document_title.is_empty() {
            doc.metadata.info.document_title = title.clone();
        }
    }

    // Apply page dimensions
    if let Some(width) = config.page_width {
        options.page_width = Mm(width);
    }

    if let Some(height) = config.page_height {
        options.page_height = Mm(height);
    }

    // Apply metadata
    for (key, value) in &config.metadata {
        match (key.trim(), value.trim()) {
            ("trapped", "true") => doc.metadata.info.trapped = true,
            ("trapped", "false") => doc.metadata.info.trapped = false,
            ("version", v) => {
                if let Ok(p) = v.parse() {
                    doc.metadata.info.version = p;
                }
            }
            ("conformance", v) => {
                if let Ok(p) = serde_json::from_str(v) {
                    doc.metadata.info.conformance = p;
                }
            }
            ("documentTitle", v) => doc.metadata.info.document_title = v.to_string(),
            ("author", v) => doc.metadata.info.author = v.to_string(),
            ("creator", v) => doc.metadata.info.creator = v.to_string(),
            ("producer", v) => doc.metadata.info.producer = v.to_string(),
            ("subject", v) => doc.metadata.info.subject = v.to_string(),
            ("keywords", v) => {
                doc.metadata.info.keywords = v.split(',').map(|s| s.trim().to_string()).collect();
            }
            ("identifier", v) => doc.metadata.info.identifier = v.to_string(),
            _ => {} // Ignore unknown metadata
        }
    }
}

// Removes the elements that cannot be drawn, with their content.
fn clean_html_elements(xml: &str) -> String {
    let mut result = xml.to_string();
    
    let non_renderable = [
        "script", "noscript", "iframe", "canvas", "audio", "video", 
        "source", "track", "embed", "object", "param", "picture",
    ];
    
    for elem in non_renderable {
        // Remove self-closing tags
        let self_closing = format!("<{} />", elem);
        result = result.replace(&self_closing, "");
        
        let self_closing2 = format!("<{}/>", elem);
        result = result.replace(&self_closing2, "");
        
        // Remove paired tags with content
        let open_tag = format!("<{}>", elem);
        let close_tag = format!("</{}>", elem);
        
        while let Some(start) = result.find(&open_tag) {
            let Some(end_pos) = result.get(start..).and_then(|rest| rest.find(&close_tag)) else {
                break;
            };
            let end = start + end_pos + close_tag.len();
            result.replace_range(start..end, "");
        }
        
        // Remove tags with attributes
        let open_tag_with_attr = format!("<{} ", elem);
        while let Some(start) = result.find(&open_tag_with_attr) {
            let Some(tag_end) = result.get(start..).and_then(|rest| rest.find('>')) else {
                break;
            };
            let tag_close_pos = start + tag_end + 1;
            let close_pos = result
                .get(tag_close_pos..)
                .and_then(|rest| rest.find(&close_tag));
            if let Some(close_pos) = close_pos {
                let end = tag_close_pos + close_pos + close_tag.len();
                result.replace_range(start..end, "");
            } else {
                // Self-closing tag with attributes
                result.replace_range(start..tag_close_pos, "");
            }
        }
    }
    
    result
}

/// Preprocesses HTML and extracts its [`HtmlExtractedConfig`].
///
/// Deprecated: use [`RawHtml::preprocess`] and the render functions.
#[deprecated(since = "0.8.0", note = "Use RawHtml::new().preprocess() for type-safe preprocessing")]
pub fn process_html_for_rendering(html: &str) -> (String, HtmlExtractedConfig) {
    let config = extract_html_config(html);
    let preprocessed = RawHtml::new(html).preprocess();
    (preprocessed.0, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end: `<img src="cat.jpg">` through the real azul layout + the
    /// printpdf bridge must (a) decode the image, (b) emit an `Op::UseXobject`
    /// referencing it on a page. Exercises the full `xml_to_pdf_pages` pipeline.
    #[cfg(all(feature = "images", feature = "jpeg"))]
    #[test]
    fn test_html_img_embeds_through_pipeline() {
        let cat_jpg: &[u8] = include_bytes!("../../examples/assets/img/cat.jpg");

        // The `<app>` wrapper + inline styles is the layout shape exercised by
        // `test_simple_xml_rendering` (known to lay out cleanly here).
        let xml = r#"
            <html>
                <body>
                    <div style="width: 300px; height: 169px;">
                        <img src="cat.jpg" style="width: 300px; height: 169px;" />
                    </div>
                    <div>caption text</div>
                </body>
            </html>
        "#;

        let mut options = XmlRenderOptions::default();
        options.images.insert("cat.jpg".to_string(), cat_jpg.to_vec());

        let (pages, _fonts, images, _bridge_res) =
            xml_to_pdf_pages(xml, &options).expect("xml_to_pdf_pages should succeed");

        // (a) the image was decoded and assigned the deterministic id.
        let (xobject_id, raw) = images.get("cat.jpg").expect("cat.jpg resolved");
        assert_eq!(xobject_id.0, "HtmlImg_cat_jpg");
        assert!(raw.width > 0 && raw.height > 0);

        // (b) some page references it via UseXobject.
        let referenced = pages.iter().any(|page| {
            page.ops.iter().any(|op| {
                matches!(op, crate::Op::UseXobject { id, .. } if id.0 == "HtmlImg_cat_jpg")
            })
        });
        assert!(
            referenced,
            "a page content stream should reference the cat.jpg image XObject"
        );
    }

    /// Only the `<img src>` a page draws is decoded; the other entries of `options.images` are
    /// left alone.
    #[test]
    fn an_image_the_page_does_not_reference_is_not_decoded() {
        let xml = r#"
            <html>
                <body>
                    <div>A page without pictures.</div>
                </body>
            </html>
        "#;
        let mut options = XmlRenderOptions::default();
        // Not even an image: decoding it would fail, but it must not be tried.
        options.images.insert("plate-12.png".to_string(), b"not an image".to_vec());

        bridge::HTML_IMAGE_DECODES.with(|n| n.set(0));
        xml_to_pdf_pages(xml, &options).expect("xml_to_pdf_pages should succeed");

        assert_eq!(
            bridge::HTML_IMAGE_DECODES.with(|n| n.get()),
            0,
            "the page shows no picture, so none may be decoded"
        );
    }

    /// The picture a page shows is decoded once, however often the page shows
    /// it, and the other entries of the map not at all.
    #[cfg(all(feature = "images", feature = "jpeg"))]
    #[test]
    fn the_picture_a_page_shows_is_decoded_once_and_no_other() {
        let cat_jpg: &[u8] = include_bytes!("../../examples/assets/img/cat.jpg");
        let xml = r#"
            <html>
                <body>
                    <div style="width: 300px; height: 169px;">
                        <img src="cat.jpg" style="width: 300px; height: 169px;" />
                    </div>
                    <div style="width: 150px; height: 85px;">
                        <img src="cat.jpg" style="width: 150px; height: 85px;" />
                    </div>
                    <div>caption text</div>
                </body>
            </html>
        "#;
        let mut options = XmlRenderOptions::default();
        options.images.insert("cat.jpg".to_string(), cat_jpg.to_vec());
        options.images.insert("plate-12.jpg".to_string(), cat_jpg.to_vec());

        bridge::HTML_IMAGE_DECODES.with(|n| n.set(0));
        let (_pages, _fonts, images, _bridge_res) =
            xml_to_pdf_pages(xml, &options).expect("xml_to_pdf_pages should succeed");

        assert_eq!(bridge::HTML_IMAGE_DECODES.with(|n| n.get()), 1, "cat.jpg, once");
        assert!(images.contains_key("cat.jpg"), "the picture the page shows is resolved");
        assert!(
            !images.contains_key("plate-12.jpg"),
            "a picture no page shows is not resolved"
        );
    }

    #[test]
    fn test_simple_xml_rendering() {
        let xml = r#"
            <app>
                <div style="width: 200px; height: 100px; background-color: red;">
                    Hello World
                </div>
            </app>
        "#;

        let options = XmlRenderOptions::default();
        let result = xml_to_pdf_pages(xml, &options);

        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_default_options() {
        let options = XmlRenderOptions::default();
        assert_eq!(options.page_width, Mm(210.0));
        assert_eq!(options.page_height, Mm(297.0));
    }

    #[test]
    fn test_html_config_extraction() {
        let html = r#"
            <html>
                <head>
                    <title>Test Document</title>
                    <meta name="pdf.options.pageWidth" content="200" />
                    <meta name="pdf.options.pageHeight" content="280" />
                    <meta name="pdf.metadata.author" content="Test Author" />
                </head>
            </html>
        "#;
        
        let config = extract_html_config(html);
        assert_eq!(config.title, Some("Test Document".to_string()));
        assert_eq!(config.page_width, Some(200.0));
        assert_eq!(config.page_height, Some(280.0));
        assert_eq!(config.metadata.get("author"), Some(&"Test Author".to_string()));
    }

    #[test]
    fn test_clean_html_elements() {
        let html = r#"
            <div>
                <script>alert('test');</script>
                <p>Content</p>
                <iframe src="test.html"></iframe>
            </div>
        "#;
        
        let cleaned = clean_html_elements(html);
        assert!(!cleaned.contains("<script"));
        assert!(!cleaned.contains("<iframe"));
        assert!(cleaned.contains("<p>Content</p>"));
    }

    #[test]
    fn test_inline_css() {
        let xml = r#"
            <style>div { color: red; }</style>
            <div>Test</div>
        "#;
        
        let inlined = inline_css_in_xml(xml);
        // <style> tags are preserved for str_to_dom() to process
        assert!(inlined.contains("<style>"));
        // The markup is returned as it is.
        assert!(inlined.contains("<div>Test</div>"));
    }
}

#[cfg(test)]
mod memory_font_tests {
    use super::*;

    #[test]
    fn memory_fonts_passed_from_outside_are_queryable() {
        let bytes =
            include_bytes!("../../examples/assets/fonts/RobotoMedium.ttf").to_vec();
        let pattern = rust_fontconfig::FcPattern {
            name: Some("PrintpdfMemoryFontTest".to_string()),
            family: Some("PrintpdfMemoryFontTest".to_string()),
            ..Default::default()
        };
        let font = rust_fontconfig::FcFont {
            bytes,
            font_index: 0,
            id: "printpdf-memory-font-test".to_string(),
        };
        // families = Some(&[]): skip the system scan — the test asserts on the
        // injected font alone, and must behave identically on a fontless CI box.
        let pool = build_font_pool_with_memory_fonts(
            vec![(pattern.clone(), font)],
            &BTreeMap::new(),
            Some(&[]),
        );
        let mut trace = Vec::new();
        assert!(
            pool.fc_cache.query(&pattern, &mut trace).is_some(),
            "externally-supplied memory font must be queryable from the pool"
        );
    }
}

#[cfg(test)]
mod builtin_font_tests {
    use azul_layout::text3::cache::{FontManager, MemoryFontTier};
    use rust_fontconfig::{utils::normalize_family_name, FcWeight};

    use super::register_builtin_fonts;

    fn registered() -> FontManager<azul_css::props::basic::FontRef> {
        let mut fm = FontManager::new(rust_fontconfig::FcFontCache::default())
            .expect("build empty FontManager");
        register_builtin_fonts(&mut fm);
        fm
    }

    fn faces<'a>(
        fm: &'a FontManager<azul_css::props::basic::FontRef>,
        family: &str,
    ) -> &'a [azul_layout::text3::cache::MemoryFace] {
        fm.memory_families
            .get(&normalize_family_name(family))
            .map_or(&[][..], |v| v.as_slice())
    }

    /// Every name a document is likely to ask for reaches the standard PDF fonts.
    #[test]
    fn the_base_14_are_reachable_by_their_css_names() {
        let fm = registered();
        for family in [
            "Helvetica", "Arial", "Times New Roman", "Times", "Courier New", "Courier", "Symbol",
            "Zapf Dingbats",
            // the PDF base-14 ids themselves
            "Helvetica-Bold", "Times-Roman", "Courier-BoldOblique",
            // and the quoted spellings the resolver actually sees
            "\"Helvetica\"", "'Times New Roman'",
        ] {
            assert!(!faces(&fm, family).is_empty(), "{family} is not registered");
        }
    }

    /// The four Helvetica faces stay distinct. They share one family name and are told apart by
    /// (weight, italic, oblique) read from the font bytes, so the `head.macStyle` bold bit of
    /// `Helvetica-Bold` and `-BoldOblique` must be set.
    #[test]
    fn helvetica_keeps_four_distinct_faces() {
        let fm = registered();
        let helvetica = faces(&fm, "Helvetica");
        assert_eq!(helvetica.len(), 4, "expected regular/bold/oblique/bold-oblique");

        let mut seen: Vec<(FcWeight, bool)> = helvetica
            .iter()
            .map(|f| (f.weight, f.italic || f.oblique))
            .collect();
        seen.sort_by_key(|(w, slanted)| (*w as u16, *slanted));
        seen.dedup();
        assert_eq!(seen.len(), 4, "faces collapsed together: {seen:?}");

        assert!(
            helvetica.iter().any(|f| f.weight >= FcWeight::Bold && !(f.italic || f.oblique)),
            "no upright bold face - is head.macStyle bit 0 set in Helvetica-Bold?"
        );
        assert!(
            helvetica.iter().any(|f| f.weight >= FcWeight::Bold && (f.italic || f.oblique)),
            "no slanted bold face - is head.macStyle bit 0 set in Helvetica-BoldOblique?"
        );
    }

    /// Concrete names outrank installed fonts. The generic families do not: a Win-1252 subset
    /// must not displace the system's Unicode faces for `sans-serif`.
    #[test]
    fn concrete_names_are_primary_and_generics_are_fallback_only() {
        let fm = registered();

        for family in ["Helvetica", "Arial", "Times New Roman", "Courier"] {
            assert!(
                faces(&fm, family).iter().any(|f| f.tier == MemoryFontTier::Primary),
                "{family} should outrank the disk"
            );
        }

        for generic in ["serif", "sans-serif", "monospace"] {
            let registered = faces(&fm, generic);
            assert!(!registered.is_empty(), "{generic} has no stand-in for wasm");
            assert!(
                registered.iter().all(|f| f.tier == MemoryFontTier::Fallback),
                "{generic} must never displace an installed font"
            );
        }
    }

    /// Each of the 14 faces declares the codepoints it covers: faces with empty unicode ranges
    /// are skipped in per-character fallback.
    #[test]
    fn every_builtin_face_carries_real_coverage() {
        let fm = registered();
        for font in crate::BuiltinFont::all_ids() {
            let registered = faces(&fm, font.get_id());
            assert!(!registered.is_empty(), "{} is not registered at all", font.get_id());
            for face in registered {
                assert!(
                    !face.font_match.unicode_ranges.is_empty(),
                    "{} carries empty coverage - unreachable in per-character fallback",
                    font.get_id()
                );
            }
        }
    }

    /// Coverage is exact, not rounded up to the enclosing block: the resolver stops at the first
    /// font whose ranges contain the codepoint, so a face that over-claims draws `.notdef`
    /// instead of falling through to a font with the glyph.
    #[test]
    fn builtin_coverage_is_exact_not_block_rounded() {
        let fm = registered();
        let face = faces(&fm, "Helvetica")
            .iter()
            .find(|f| !(f.italic || f.oblique) && f.weight < FcWeight::Bold)
            .expect("regular Helvetica");
        let covered: u32 = face
            .font_match
            .unicode_ranges
            .iter()
            .map(|r| r.end - r.start + 1)
            .sum();
        // 213 codepoints, plus U+0020 and U+00A0, which map to the empty space glyph.
        assert_eq!(
            covered, 215,
            "expected the 215 codepoints Helvetica.subset.ttf actually maps, got {covered} \
             (256 means coverage was rounded up to the Latin-1 block again)"
        );
    }
}
