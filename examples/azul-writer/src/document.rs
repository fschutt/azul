//! ############################################################################
//! #  DOCUMENT PIPELINE — the real markdown -> Dom -> pages implementation.  #
//! ############################################################################
//!
//! The pipeline the shell was seamed for:
//!
//! 1. [`load_markdown`]  — markdown file -> pulldown-cmark -> HTML ->
//!    azul XML parser -> content `Dom` (with the document stylesheet
//!    attached to `Dom.css`, cascading inside the app's own DOM).
//! 2. [`save_markdown`]  — document model -> file.
//! 3. [`paginate`]       — content DOM -> `Vec<Page>` via the ENGINE's
//!    pagination: `Pdf::compute_pagination` estimates the breaks and hands
//!    back one root-to-node child-index path per page boundary
//!    (`PaginationSnapshot::break_path`), and `DomSplit::at_path` cuts the
//!    content along those paths (reverse order, so earlier paths stay
//!    valid). One DOM per page — the classic office-suite page model,
//!    driven by the real layout engine.

use std::path::{Path, PathBuf};

use azul::css::{BoxOrStaticString, Css, LayoutSize, LogicalSize};
use azul::dom::{Dom, DomSplit, NodeType};
use azul::error::XmlError;
use azul::misc::PaginationSnapshot;
use azul::pdf::Pdf;
use azul::xml::Xml;

/// Snapshot handles the pagination call takes. Re-exported through this
/// module so every caller names ONE source.
pub use azul::css::FontCacheSnapshot;
pub use azul::image::ImageCacheSnapshot;

/// #28 (b): THE single source of page geometry. A4 @96dpi CSS px with the
/// classic office-suite default 1" margins.
///
/// (The engine's `PageSetup` type is not part of the public api.json
/// surface, so the geometry lives here as plain constants; the pagination
/// content box below is page minus margins, the same math
/// `PageSetup::content_width/height()` did.)
pub const A4_PAGE_W: f32 = 794.0;
pub const A4_PAGE_H: f32 = 1123.0;
/// 1" margin on every side, @96dpi.
pub const A4_MARGIN: f32 = 96.0;

/// Content box the engine paginates into (page minus margins/decoration).
pub fn page_content_size() -> LogicalSize {
    LogicalSize {
        width: A4_PAGE_W - 2.0 * A4_MARGIN,
        height: A4_PAGE_H - 2.0 * A4_MARGIN,
    }
}

/// An EMPTY font-cache handle: the engine then resolves fresh system fonts.
/// Contexts with neither `from_layout_info` (layout callbacks) nor
/// `get_font_cache_clone` (event callbacks) — tests, worker fallbacks —
/// use the engine's own empty constructor.
pub fn empty_font_cache() -> FontCacheSnapshot {
    FontCacheSnapshot::empty()
}

/// An EMPTY image-cache handle. The markdown pipeline registers no app
/// images, so pagination and PDF export always ran against an empty image
/// cache — this preserves that exactly.
pub fn empty_image_cache() -> ImageCacheSnapshot {
    ImageCacheSnapshot::empty()
}

/// Read the text out of a `NodeType::Text` / `NodeType::Icon` payload.
/// `BoxOrStaticString` is the ABI's boxed-or-static pointer enum; both
/// variants point at a live `AzString` for the lifetime of the node.
fn box_str(s: &BoxOrStaticString) -> &str {
    unsafe {
        match s {
            BoxOrStaticString::Boxed(p) => (**p).as_str(),
            BoxOrStaticString::Static(p) => (**p).as_str(),
        }
    }
}

/// The document stylesheet: the Office-2013-era default look for markdown content.
/// Calibri 11pt body (14.7px, the classic default), Calibri Light headings in
/// the classic office-suite accent blue, 8pt spacing after paragraphs.
///
/// FIXED in both themes, on purpose. The sheet is a preview of a PRINTED
/// page: the paper stays paper (`crate::palette`'s `sheet`, dimmed but not
/// inverted in a dark session) and the document's own styling stays what it
/// will print as - the same reasoning as the ribbon's style previews. Only
/// the chrome AROUND the page follows the desktop.
///
/// This sheet is also baked into the CACHED content DOM
/// (`DocumentModel::content`, the source of truth the C11 edit loop mutates),
/// which a theme switch does not rebuild - the document did not change, only
/// the desktop did. Keeping it theme-independent is what makes that safe.
const DOC_CSS: &str = "
    body { font-family: 'Liberation Sans', sans-serif; font-size: 15px;
           color: #1a1a1a; line-height: 1.35; }
    p    { margin-bottom: 11px; }
    h1   { font-size: 28px; color: #2e74b5; margin-bottom: 12px; margin-top: 4px; }
    h2   { font-size: 21px; color: #2e74b5; margin-bottom: 10px; margin-top: 4px; }
    h3   { font-size: 17px; color: #1f4d78; margin-bottom: 9px;  margin-top: 4px; }
    ul, ol { margin-bottom: 11px; margin-left: 36px; }
    li   { margin-bottom: 2px; }
    blockquote { margin-left: 36px; margin-bottom: 11px; color: #555555;
                 border-left: 3px solid #cccccc; padding-left: 10px; }
    code { font-family: 'Liberation Mono', monospace; font-size: 13px;
           background: #f2f2f2; }
    pre  { font-family: 'Liberation Mono', monospace; font-size: 13px;
           background: #f6f6f6; padding: 8px; margin-bottom: 11px; }
    hr   { border-bottom: 1px solid #bbbbbb; margin-bottom: 11px; }
    strong { font-weight: bold; }
    em   { font-style: italic; }

";

/// The application-side document model: origin path, raw markdown source,
/// and the parsed content DOM (cached so `layout()` never re-parses).
#[derive(Clone)]
pub struct DocumentModel {
    /// Where the document came from / was last saved to. `None` = the
    /// unsaved "Document1".
    pub path: Option<PathBuf>,
    /// Raw markdown source (the save round-trip writes this back).
    pub markdown: String,
    /// Parsed content DOM, rebuilt whenever `markdown` changes. This is the
    /// SOURCE OF TRUTH the C11 edit loop mutates; `markdown` is re-serialized
    /// from it after every applied edit.
    pub content: Dom,
    /// Unsaved changes since the last save/load.
    pub dirty: bool,
    /// Bumped on every mutation of `content`. The pagination memo is keyed
    /// on it: page geometry is FIXED (the A4 content box), so pagination
    /// depends on the DOCUMENT, never on the window — a resize must not
    /// re-run it.
    pub generation: u64,
}

impl DocumentModel {
    /// The blank, unsaved "Document1".
    #[must_use]
    pub fn untitled() -> Self {
        Self {
            path: None,
            markdown: String::new(),
            content: blank_document(),
            dirty: false,
            generation: next_generation(),
        }
    }

    /// Loads the model from a markdown file and parses it through the
    /// pipeline once (the DOM is cached on the model).
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        // A failed read must be VISIBLE, not a silent blank document —
        // a relative AZWRITER_OPEN path broke against the harness's cd
        // and the blank page was misread as four separate engine bugs
        // (no text, no caret, dead click, no scrollbar). The error text
        // renders as the document body, so the mistake explains itself.
        let markdown = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[azwriter] cannot open {}: {e}", path.display());
                format!(
                    "# Cannot open document\n\n`{}`\n\n{}\n\n(cwd: `{}`)",
                    path.display(),
                    e,
                    std::env::current_dir()
                        .map_or_else(|_| "?".into(), |d| d.display().to_string()),
                )
            }
        };
        let content = markdown_to_content_dom(&markdown);
        Self {
            path: Some(path.to_path_buf()),
            markdown,
            content,
            dirty: false,
            generation: next_generation(),
        }
    }

    /// Re-parse `markdown` into the cached content DOM (call after edits).
    pub fn reparse(&mut self) {
        self.content = markdown_to_content_dom(&self.markdown);
        self.generation = next_generation();
    }

    /// #28(c): page count for the FIRST paint, bounded by what a monitor
    /// could possibly show (USER design: at most monitor-height LINES, and
    /// at most monitor-width × monitor-height CHARACTERS for the
    /// one-massive-line case — so opening a huge file never paginates
    /// unbounded content up front). Small documents take the exact path.
    /// Large ones paginate only a markdown-derived PREFIX of blocks and
    /// scale by character proportion; the background exact-pagination
    /// thread corrects the count afterwards (`SetVirtualViewGeometry` +
    /// status-bar update). Returns `(count, exact)`.
    pub fn page_count_bounded(
        &self,
        fonts: Option<FontCacheSnapshot>,
        monitor_px: Option<LayoutSize>,
    ) -> (usize, bool) {
        let (mon_w, mon_h) = match monitor_px {
            Some(m) if m.width > 0 && m.height > 0 => (m.width as usize, m.height as usize),
            // No monitor info (headless/web/startup race): the exact path —
            // correctness over speed when the bound is unknowable.
            _ => {
                return (
                    page_count_cached(&self.content, self.generation, fonts),
                    true,
                )
            }
        };
        let char_budget = mon_w.saturating_mul(mon_h);
        let line_budget = mon_h; // 1px minimum font ⇒ ≥ this many lines never fit

        if self.markdown.len() <= char_budget && self.markdown.lines().count() <= line_budget {
            return (
                page_count_cached(&self.content, self.generation, fonts),
                true,
            );
        }

        // Walk the SOURCE to find how many top-level blocks fit the budget
        // (blank-line separated ≈ content blocks; an estimate bound, not a
        // layout truth — the thread delivers the truth).
        let mut chars = 0usize;
        let mut lines = 0usize;
        let mut prefix_blocks = 0usize;
        let mut in_block = false;
        for line in self.markdown.lines() {
            lines += 1;
            chars += line.len() + 1;
            if line.trim().is_empty() {
                in_block = false;
            } else if !in_block {
                in_block = true;
                prefix_blocks += 1;
            }
            if chars > char_budget || lines > line_budget {
                break;
            }
        }
        let prefix_blocks = prefix_blocks.max(1);

        // First `prefix_blocks` children of the content DOM = the prefix.
        let split = DomSplit::at_path(&self.content, vec![prefix_blocks as u32]);
        let prefix = split.head;
        // Direct computation — NOT page_count_cached: that path memoizes
        // COMPLETE entries only.
        let prefix_paths = compute_break_paths_with_fonts(&prefix, fonts);
        let prefix_pages = prefix_paths.len() + 1;
        // Seed the memo (prefix-only) so the VirtualView's first
        // materialization — pages 0..3, inside the prefix by construction —
        // also skips the full-document break computation (#28c instant open).
        seed_break_paths(self.generation, prefix_paths, false);

        // Scale by character proportion (never below what the prefix proved).
        let total_chars = self.markdown.len().max(1);
        let est = (prefix_pages as f64 * total_chars as f64 / chars.max(1) as f64).ceil() as usize;
        (est.max(prefix_pages), false)
    }

    /// Mark `content` as structurally changed (call after applying an edit)
    /// so the pagination memo recomputes exactly once.
    pub fn touch(&mut self) {
        self.generation = next_generation();
    }

    /// "Document1" or the file stem ("Notes" for /tmp/Notes.md).
    #[must_use]
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document1".to_string())
    }

    /// Whitespace-separated word count of the markdown source (the status
    /// bar's "N WORDS").
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.markdown.split_whitespace().count()
    }
}

/// One laid-out page: the page's content subtree, cut at the engine's
/// structural break paths.
pub struct Page {
    /// The page's content subtree.
    pub dom: Dom,
}

// ============================================================================
// The empty document
// ============================================================================

/// Content DOM of the blank, unsaved "Document1": an empty body-shaped
/// container carrying the document stylesheet (so typed text inherits it).
#[must_use]
pub fn blank_document() -> Dom {
    // Through the normal pipeline so the implicit empty paragraph is
    // seeded here too — a bare div gives the caret nothing to anchor to
    // (same blank-page/dead-click failure as an empty loaded file).
    markdown_to_content_dom("")
}

// ============================================================================
// SEAM 1: load  (markdown -> HTML -> azul XML parser -> Dom)
// ============================================================================

/// Coarse human-readable tag for an [`XmlError`] (the ABI enum carries no
/// Display impl; the exact byte position matters less than WHICH failure).
fn xml_error_kind(e: &XmlError) -> &'static str {
    match e {
        XmlError::NoParserAvailable => "no XML parser available",
        XmlError::NoRootNode => "no root node",
        XmlError::SizeLimit => "size limit reached",
        XmlError::DtdDetected => "DTD detected",
        XmlError::MalformedHierarchy(_) => "malformed hierarchy",
        XmlError::ParserError(_) => "parser error",
        XmlError::UnclosedRootNode => "unclosed root node",
        XmlError::UnexpectedCloseTag(_) => "unexpected close tag",
        XmlError::UnknownEntityReference(_) => "unknown entity reference",
        XmlError::DuplicatedAttribute(_) => "duplicated attribute",
        XmlError::InvalidAttributeValue(_) => "invalid attribute value",
        XmlError::UnexpectedEndOfStream => "unexpected end of stream",
        _ => "invalid XML",
    }
}

/// Where `--dump-xml` writes the generated document XML.
///
/// A one-time store rather than a parameter because the only caller is
/// [`markdown_to_content_dom`], reached from the document model, the edit
/// loop and the tests alike - threading a debug path through all three
/// signatures would put it in production code that has no use for it. The
/// VALUE still comes from the command line (`args`), which is the part that
/// matters: nothing here reads the environment.
static DUMP_XML: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

/// Called once from `start`, before any document is parsed.
pub fn init_dump_xml(path: Option<PathBuf>) {
    let _ = DUMP_XML.set(path);
}

fn dump_xml_path() -> Option<&'static Path> {
    DUMP_XML.get()?.as_deref()
}

/// markdown source -> content `Dom`, through the full pipeline:
/// pulldown-cmark renders HTML, the azul XML parser turns it into an
/// unstyled `Dom` with the `<style>` document stylesheet attached to
/// `Dom.css` (applied by the normal cascade inside the app's DOM).
#[must_use]
pub fn markdown_to_content_dom(markdown: &str) -> Dom {
    use pulldown_cmark::{html, Options, Parser};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(markdown, opts);
    let mut body = String::new();
    html::push_html(&mut body, parser);

    // the classic office-suite implicit empty paragraph: an empty document must still have
    // ONE paragraph node, or the caret has nothing to anchor to — a blank
    // page with a dead click is exactly what that looks like.
    if body.trim().is_empty() {
        body = "<p></p>".to_string();
    }

    let xml = format!("<html><head><style>{DOC_CSS}</style></head><body>{body}</body></html>");
    if let Some(dump) = dump_xml_path() {
        let _ = std::fs::write(dump, &xml);
    }
    match Xml::from_str(xml).into_result() {
        Ok(parsed) => {
            let full = Dom::create_from_parsed_xml(parsed);
            unwrap_html_shell(full)
        }
        Err(e) => Dom::create_div()
            .with_css(DOC_CSS)
            .with_child(Dom::create_p_with_text(format!(
                "markdown parse error: {}",
                xml_error_kind(&e)
            ))),
    }
}

/// Drop whitespace-only text nodes RECURSIVELY. CSS 2.1 §9.2.2.1 collapses
/// them away (they generate no boxes), and keeping them makes child indices
/// disagree with the visible children at EVERY level — the top-level case
/// broke changeset resume paths, the nested case (inter-`<li>` newlines
/// inside a `<ul>`) breaks the serializer's `[block, child]` paths and the
/// ids built from them.
///
/// `pre` is skipped: there whitespace is content, not markup formatting.
fn strip_layout_whitespace(children: &[Dom]) -> Vec<Dom> {
    children
        .iter()
        .filter(|c| match &c.root.node_type {
            NodeType::Text(t) => !box_str(t).trim().is_empty(),
            _ => true,
        })
        .map(|c| {
            let mut kept = c.clone();
            if !matches!(kept.root.node_type, NodeType::Pre) {
                kept.children = strip_layout_whitespace(kept.children.as_ref()).into();
                kept.fixup_children_estimated();
            }
            kept
        })
        .collect()
}

/// The XML parser returns the full `<html><body>…` document; the editor
/// embeds CONTENT inside its own DOM, so the shell nodes must go: a nested
/// `Html`/`Body` measures as an empty box inside an app tree (and the page
/// stayed blank). Take body's children as the content, and carry the
/// `<style>` blocks (attached to the html node) on the new root so the
/// document stylesheet still scopes the subtree.
fn unwrap_html_shell(full: Dom) -> Dom {
    let mut content = Dom::create_div();
    // The document is editable: the flag lives on the content ROOT, so the
    // per-page split clones (same NodeData shape) are all editable too.
    content.root.set_contenteditable(true);
    content.css = full.css.clone();
    let body = full
        .children
        .as_ref()
        .iter()
        .find(|c| matches!(c.root.node_type, NodeType::Body));
    if let Some(body) = body {
        // Drop inter-block whitespace-only text nodes. CSS 2.1 §9.2.2.1
        // collapses them away (they generate no boxes), and keeping them
        // would make the model's child indices disagree with the visible
        // block indices — the page->model path mapping, the changeset
        // resume paths and the markdown serializer all address blocks by
        // child index, so a phantom node between every pair shifts every
        // edit by one.
        content.children = strip_layout_whitespace(body.children.as_ref()).into();
        // body-level <style> blocks (rare) must survive too.
        let mut css: Vec<Css> = content.css.as_ref().to_vec();
        css.extend(body.css.as_ref().iter().cloned());
        content.css = css.into();
    } else {
        // No shell (already a fragment): keep as-is.
        return full;
    }
    content.fixup_children_estimated();
    content
}

/// Loads a markdown file and returns the document CONTENT DOM (not yet
/// paginated — feed it to [`paginate`]). Prefer `DocumentModel::from_path`,
/// which caches the result.
#[must_use]
pub fn load_markdown(path: &Path) -> Dom {
    let source = std::fs::read_to_string(path).unwrap_or_default();
    markdown_to_content_dom(&source)
}

// ============================================================================
// SEAM 2: save
// ============================================================================

/// Saves the document model as markdown (the source of truth the editor
/// mutates; the DOM is derived from it).
pub fn save_markdown(path: &Path, model: &DocumentModel) -> Result<(), String> {
    let contents = if model.markdown.is_empty() {
        format!("# {}\n", model.display_name())
    } else {
        model.markdown.clone()
    };
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

// ============================================================================
// SEAM 3: paginate  (content Dom -> engine breaks -> Vec<Page>)
// ============================================================================

/// Monotonic document generation. A fresh value invalidates the pagination
/// memo below.
pub fn next_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static G: AtomicU64 = AtomicU64::new(1);
    G.fetch_add(1, Ordering::Relaxed)
}

/// [`paginate`], memoized on the document generation.
///
/// WHY THIS EXISTS: the layout callback runs on every frame — including
/// every frame of a window-resize drag — and full pagination measured
/// ~335 ms per call on a 3 KB document. Re-running it per frame made the
/// app unable to acknowledge the compositor's configure/ping, so KWin
/// dropped the surface and left the process running with no window.
///
/// Pagination cannot depend on the window: pages are a FIXED A4 content
/// box. Only the document's own content can change the break paths, so
/// the memo keys on `generation` and a resize costs nothing. The stored
/// value is the break-path list (a few small Vec<u32>), not the pages —
/// splitting the tree per call is cheap next to laying it out.
#[must_use]
pub fn paginate_cached(content: &Dom, generation: u64) -> Vec<Page> {
    paginate_cached_with_fonts(content, generation, None)
}

/// [`paginate_cached`], reusing the window's already-built font cache.
///
/// Without this the first pagination scans every font on the machine —
/// ~5 SECONDS on the first frame, long enough for the compositor to drop
/// the surface. A layout callback builds the handle from the engine's own
/// cache via `FontCacheSnapshot::from_layout_info`.
#[must_use]
pub fn paginate_cached_with_fonts(
    content: &Dom,
    generation: u64,
    fonts: Option<FontCacheSnapshot>,
) -> Vec<Page> {
    let paths = cached_break_paths(content, generation, fonts);
    let _p = crate::perf::Phase::start("  split_content_at");
    split_content_at(content.clone(), &paths)
}

use std::cell::RefCell;
thread_local! {
    /// Generation-memoized break paths. `complete = false` means the paths
    /// cover only a monitor-bounded PREFIX of the document (#28c bounded
    /// first paint) — callers needing more recompute in full, and the
    /// background pagination thread's writeback upgrades the entry wholesale
    /// via [`seed_break_paths`] (same thread_local: the writeback runs on
    /// the main thread).
    static BREAK_MEMO: RefCell<Option<(u64, Vec<Vec<u32>>, bool)>> =
        const { RefCell::new(None) };
}

/// Break paths guaranteed COMPLETE for `generation` (computes on a miss, a
/// stale generation, or a prefix-only entry).
fn cached_break_paths(
    content: &Dom,
    generation: u64,
    fonts: Option<FontCacheSnapshot>,
) -> Vec<Vec<u32>> {
    BREAK_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        if let Some((g, paths, complete)) = m.as_ref() {
            if *g == generation && *complete {
                return paths.clone();
            }
        }
        let _p = crate::perf::Phase::start("  break_paths (MISS)");
        let paths = compute_break_paths_with_fonts(content, fonts);
        *m = Some((generation, paths.clone(), true));
        paths
    })
}

/// #28(c): the memo WITHOUT computing — `(paths, complete)` if the
/// generation matches.
fn try_cached_break_paths(generation: u64) -> Option<(Vec<Vec<u32>>, bool)> {
    BREAK_MEMO.with(|m| {
        m.borrow()
            .as_ref()
            .and_then(|(g, p, c)| (*g == generation).then(|| (p.clone(), *c)))
    })
}

/// #28(c): whether the memo holds COMPLETE break paths for `generation`
/// (the mount callback skips spawning the background thread when the
/// document is already fully paginated).
pub fn pagination_is_complete(generation: u64) -> bool {
    matches!(try_cached_break_paths(generation), Some((_, true)))
}

/// #28(c): raw break-path computation for the BACKGROUND thread's chunk
/// loop (no memo interaction — the writeback seeds the memo on the main
/// thread).
pub fn break_paths_for(content: &Dom, fonts: Option<FontCacheSnapshot>) -> Vec<Vec<u32>> {
    compute_break_paths_with_fonts(content, fonts)
}

/// #28(c): top-level block count of a markdown source (blank-line separated
/// — the same walk `page_count_bounded` uses), for chunking the background
/// pagination.
pub fn markdown_block_count(markdown: &str) -> usize {
    let mut blocks = 0usize;
    let mut in_block = false;
    for line in markdown.lines() {
        if line.trim().is_empty() {
            in_block = false;
        } else if !in_block {
            in_block = true;
            blocks += 1;
        }
    }
    blocks
}

/// #28(c): seed the break-path memo — the bounded first count stores its
/// PREFIX paths (valid on the full DOM: the prefix is the first K root
/// children, so its root-child-index paths are identical), and the
/// background thread's writeback stores its COMPLETE paths. Never
/// downgrades a complete entry to a prefix.
pub fn seed_break_paths(generation: u64, paths: Vec<Vec<u32>>, complete: bool) {
    BREAK_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        let would_downgrade =
            matches!(m.as_ref(), Some((g, _, true)) if *g == generation) && !complete;
        if !would_downgrade {
            *m = Some((generation, paths, complete));
        }
    });
}

/// #28: page COUNT without building a single page DOM — the break-path memo
/// answers it (`paths.len() + 1`). Feeds the status bar and the
/// `VirtualView`'s virtual scroll size.
pub fn page_count_cached(
    content: &Dom,
    generation: u64,
    fonts: Option<FontCacheSnapshot>,
) -> usize {
    cached_break_paths(content, generation, fonts).len() + 1
}

/// #28: build ONLY pages `[first, first+count)` (clamped), block-id-tagged
/// with their GLOBAL offsets so saves keep addressing the model correctly.
///
/// v1 still splits the whole content clone (same cost the eager path always
/// paid) and returns just the window — the win is downstream: the engine
/// styles, lays out and renders 3 pages instead of the whole document.
pub fn paginate_range_cached(
    content: &Dom,
    generation: u64,
    fonts: Option<FontCacheSnapshot>,
    first: usize,
    count: usize,
) -> Vec<Page> {
    // #28(c): a prefix-only memo COVERS the range when every requested page
    // index stays strictly below the prefix's tail-glob page (k paths ⇒
    // pages 0..=k, where page k is the unsplit remainder): first paint's
    // pages 0..3 fit the monitor-bounded prefix by construction, so a huge
    // document's open never computes full break paths on the main thread.
    let paths = match try_cached_break_paths(generation) {
        Some((paths, true)) => paths,
        Some((paths, false)) if first + count <= paths.len() => paths,
        _ => cached_break_paths(content, generation, fonts),
    };
    let _p = crate::perf::Phase::start("  split_content_at (range)");
    let mut pages = split_content_at(content.clone(), &paths);
    let offsets = page_block_offsets(&pages);
    let a = first.min(pages.len());
    let b = (first + count).min(pages.len());
    let mut window: Vec<Page> = pages.drain(a..b).collect();
    tag_pages_with_block_ids(&mut window, &offsets[a..b]);
    window
}

/// Splits the content DOM into pages using the ENGINE's pagination:
/// `Pdf::compute_pagination` produces one structural (root-to-node
/// child-index) path per page boundary, and `DomSplit::at_path` cuts the
/// content there. Splitting runs in REVERSE break order so earlier paths
/// stay valid on the shrinking head.
#[must_use]
pub fn paginate(content: Dom) -> Vec<Page> {
    let break_paths = compute_break_paths(&content);
    split_content_at(content, &break_paths)
}

/// Cut `content` at the given break paths (reverse order, so earlier paths
/// stay valid on the shrinking head).
fn split_content_at(content: Dom, break_paths: &[Vec<u32>]) -> Vec<Page> {
    let mut head = content;
    let mut tails: Vec<Dom> = Vec::new();
    for path in break_paths.iter().rev() {
        let split = DomSplit::at_path(&head, path.clone());
        head = split.head;
        tails.push(split.tail);
    }
    let mut pages = vec![Page { dom: head }];
    pages.extend(tails.into_iter().rev().map(|dom| Page { dom }));
    pages
}

/// Run the engine's break estimation over a styled clone of the content
/// at the classic A4 content box and return one DOM child-index path per page
/// boundary (breaks without a structural position are skipped).
fn compute_break_paths(content: &Dom) -> Vec<Vec<u32>> {
    compute_break_paths_with_fonts(content, None)
}

fn compute_break_paths_with_fonts(
    content: &Dom,
    fonts: Option<FontCacheSnapshot>,
) -> Vec<Vec<u32>> {
    use azul::css::StyledDom;

    // The styled copy the engine measures. Same tree shape as `content`, so
    // the returned paths address `content`'s children directly.
    let styled_dom = {
        let _p = crate::perf::Phase::start("    dom_clone+cascade");
        StyledDom::create_from_dom(content.clone())
    };

    // PERF NOTE (public-API migration): pagination now goes through
    // `Pdf::compute_pagination`, and the ENGINE owns the layout + shaping
    // caches behind that snapshot API. The old in-app thread-local
    // `Solver3LayoutCache`/`TextLayoutCache` reuse (which halved repeated
    // paginations, 224ms -> 105ms measured) is no longer reachable from the
    // public surface — repeat paginations therefore re-measure from scratch.
    // The app-side BREAK_MEMO above still guarantees at most ONE pagination
    // per document generation, which is what keeps resize drags free.
    let content_size = page_content_size();
    let _p_pag = crate::perf::Phase::start("    compute_pagination");
    let pdf = Pdf::new();
    let snapshot: PaginationSnapshot = pdf.compute_pagination(
        styled_dom,
        content_size.width,
        content_size.height,
        fonts.unwrap_or_else(empty_font_cache),
        empty_image_cache(),
    );
    drop(_p_pag);

    let _p_brk = crate::perf::Phase::start("    break_paths_extract");
    let mut out: Vec<Vec<u32>> = Vec::new();
    for i in 0..snapshot.break_count() {
        let path: Vec<u32> = snapshot.break_path(i).as_ref().to_vec();
        // An EMPTY path = the engine found no structural position for this
        // break — skipped, exactly like the old `filter_map(|b| b.path)`.
        if !path.is_empty() {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Human-readable tag for the node types the markdown pipeline emits
    /// (the ABI enum carries no Debug impl).
    pub(super) fn node_type_label(nt: &NodeType) -> String {
        match nt {
            NodeType::Html => "Html".into(),
            NodeType::Head => "Head".into(),
            NodeType::Body => "Body".into(),
            NodeType::Div => "Div".into(),
            NodeType::P => "P".into(),
            NodeType::H1 => "H1".into(),
            NodeType::H2 => "H2".into(),
            NodeType::H3 => "H3".into(),
            NodeType::Ul => "Ul".into(),
            NodeType::Ol => "Ol".into(),
            NodeType::Li => "Li".into(),
            NodeType::Pre => "Pre".into(),
            NodeType::BlockQuote => "BlockQuote".into(),
            NodeType::Hr => "Hr".into(),
            NodeType::Style => "Style".into(),
            NodeType::Text(t) => format!("Text({:?})", box_str(t)),
            _ => "<other>".into(),
        }
    }

    fn walk(d: &Dom, depth: usize, out: &mut String) {
        use std::fmt::Write;
        let _ = writeln!(
            out,
            "{:indent$}{} css={} kids={}",
            "",
            node_type_label(&d.root.node_type),
            d.css.as_ref().len(),
            d.children.as_ref().len(),
            indent = depth * 2
        );
        for c in d.children.as_ref() {
            walk(c, depth + 1, out);
        }
    }

    #[test]
    fn markdown_parses_to_a_populated_dom() {
        let dom = markdown_to_content_dom(
            "# Title\n\nHello **world** paragraph.\n\n- item one\n- item two\n",
        );
        let mut dump = String::new();
        walk(&dom, 0, &mut dump);
        println!("{dump}");
        assert!(
            dom.children.as_ref().len() >= 3,
            "h1 + p + ul expected under the content root, got:\n{dump}"
        );
    }

    #[test]
    fn pagination_splits_long_documents() {
        // ~60 fat paragraphs must overflow one A4 content box (931px).
        let md = (0..60)
            .map(|i| format!("Paragraph number {i} with a reasonable amount of text inside it.\n"))
            .collect::<Vec<_>>()
            .join("\n");
        let dom = markdown_to_content_dom(&md);
        let n_children = dom.children.as_ref().len();
        let pages = paginate(dom);
        assert!(
            pages.len() >= 2,
            "60 paragraphs ({n_children} blocks) on 931px pages must span pages, got {}",
            pages.len()
        );
        // Negative control: a one-liner stays on one page.
        let single = paginate(markdown_to_content_dom("just one line\n"));
        assert_eq!(single.len(), 1);
    }
}

#[cfg(test)]
mod sample_tests {
    use super::*;

    /// The demo document, generated. It used to be `read_to_string` of an
    /// absolute path under one machine's scratchpad directory, which meant the
    /// test failed everywhere else and had never run in CI — the crate is in no
    /// test job (fixed 2026-08-20). Generating it keeps the assertion honest and
    /// machine-independent.
    pub(super) fn sample_markdown() -> String {
        let mut md = String::from("# AzWriter sample document\n\n");
        for section in 0..8 {
            md.push_str(&format!("## Section {section}\n\n"));
            for para in 0..6 {
                md.push_str(&format!(
                    "Paragraph {para} of section {section}. The quick brown fox jumps over \
                     the lazy dog, repeatedly, so that this document is long enough to be \
                     paginated into more than a single page by the layout engine.\n\n"
                ));
            }
        }
        md
    }

    #[test]
    fn the_sample_doc_spans_two_pages() {
        let md = sample_markdown();
        let dom = markdown_to_content_dom(&md);
        let pages = paginate(dom);
        assert!(
            pages.len() >= 2,
            "the demo document must span pages, got {}",
            pages.len()
        );
    }
}

// ============================================================================
// C11 editing loop: markdown serialization + page->model mapping
// ============================================================================

/// Serialize the content DOM back to markdown. STRUCTURE comes from the
/// app's model (`content`, the same tree `paginate` splits); TEXT comes
/// from `text_of`, a provider called with the model child-index path of
/// each leaf block — the app passes an engine readback
/// (`CallbackInfo::get_node_text_content`, which sees the text overlay's
/// live edits), tests pass a pure model walk.
pub fn dom_to_markdown(content: &Dom, text_of: &mut dyn FnMut(&[u32]) -> Option<String>) -> String {
    fn own_text(d: &Dom) -> String {
        let mut s = String::new();
        for c in d.children.as_ref() {
            match &c.root.node_type {
                NodeType::Text(t) => s.push_str(box_str(t)),
                _ => s.push_str(&own_text(c)),
            }
        }
        s
    }

    let mut out = String::new();
    for (i, block) in content.children.as_ref().iter().enumerate() {
        let path = [i as u32];
        let text = |p: &mut dyn FnMut(&[u32]) -> Option<String>| {
            p(&path).unwrap_or_else(|| own_text(block))
        };
        match &block.root.node_type {
            NodeType::H1 => {
                out.push_str("# ");
                out.push_str(text(text_of).trim());
                out.push_str("\n\n");
            }
            NodeType::H2 => {
                out.push_str("## ");
                out.push_str(text(text_of).trim());
                out.push_str("\n\n");
            }
            NodeType::H3 => {
                out.push_str("### ");
                out.push_str(text(text_of).trim());
                out.push_str("\n\n");
            }
            NodeType::P => {
                out.push_str(text(text_of).trim());
                out.push_str("\n\n");
            }
            NodeType::Ul | NodeType::Ol => {
                let ordered = matches!(block.root.node_type, NodeType::Ol);
                let mut li_index = 0usize;
                for (j, li) in block.children.as_ref().iter().enumerate() {
                    if !matches!(li.root.node_type, NodeType::Li) {
                        continue;
                    }
                    li_index += 1;
                    let li_path = [i as u32, j as u32];
                    let t = text_of(&li_path).unwrap_or_else(|| own_text(li));
                    if ordered {
                        out.push_str(&format!("{li_index}. "));
                    } else {
                        out.push_str("- ");
                    }
                    out.push_str(t.trim());
                    out.push('\n');
                }
                out.push('\n');
            }
            NodeType::BlockQuote => {
                out.push_str("> ");
                out.push_str(text(text_of).trim());
                out.push_str("\n\n");
            }
            NodeType::Pre => {
                out.push_str("```\n");
                out.push_str(text(text_of).trim_end());
                out.push_str("\n```\n\n");
            }
            NodeType::Hr => out.push_str("---\n\n"),
            // Whitespace text between blocks and anything unknown: skip.
            _ => {}
        }
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The CSS id a rendered block carries so the app can read its LIVE text
/// back out of the engine. Blocks are addressed by their MODEL child index,
/// which is stable across the page split (pages are contiguous slices).
#[must_use]
pub fn block_dom_id(model_index: usize) -> String {
    format!("mw-blk-{model_index}")
}

/// The id of a nested child (list item) inside block `model_index`.
/// Serializer paths are `[block, child]`, so the id names both levels.
#[must_use]
pub fn nested_dom_id(model_index: usize, child_index: usize) -> String {
    format!("mw-blk-{model_index}-{child_index}")
}

/// Id for any serializer path: one level -> block, two -> nested child.
#[must_use]
pub fn path_dom_id(path: &[u32]) -> Option<String> {
    match path {
        [b] => Some(block_dom_id(*b as usize)),
        [b, c] => Some(nested_dom_id(*b as usize, *c as usize)),
        _ => None,
    }
}

/// Tag every block of every page with [`block_dom_id`] so a later save can
/// pull the on-screen text back through the engine. Called by the canvas
/// after pagination; `offsets` comes from [`page_block_offsets`].
pub fn tag_pages_with_block_ids(pages: &mut [Page], offsets: &[usize]) {
    for (p, page) in pages.iter_mut().enumerate() {
        let base = offsets.get(p).copied().unwrap_or(0);
        let children: Vec<Dom> = page
            .dom
            .children
            .as_ref()
            .iter()
            .enumerate()
            .map(|(i, child)| {
                let model_index = base + i;
                let mut tagged = child.clone().with_id(block_dom_id(model_index));
                // Containers (ul/ol) also tag their direct children so the
                // serializer's [block, child] paths resolve too - list items
                // are edited like any other text and must round-trip.
                let nested: Vec<Dom> = tagged
                    .children
                    .as_ref()
                    .iter()
                    .enumerate()
                    .map(|(j, sub)| sub.clone().with_id(nested_dom_id(model_index, j)))
                    .collect();
                tagged.children = nested.into();
                tagged.fixup_children_estimated();
                tagged
            })
            .collect();
        page.dom.children = children.into();
        page.dom.fixup_children_estimated();
    }
}

/// Per-page block offsets for the page->model path mapping: entry `p` is
/// the model child index of page `p`'s FIRST block. A changeset recorded
/// against page `p` with resume path `[k, rest..]` addresses model path
/// `[k + offsets[p], rest..]`.
#[must_use]
pub fn page_block_offsets(pages: &[Page]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(pages.len());
    let mut acc = 0usize;
    for p in pages {
        offsets.push(acc);
        acc += p.dom.children.as_ref().len();
    }
    offsets
}

/// Shift a page-relative child-index path into model coordinates.
#[must_use]
pub fn page_path_to_model_path(page_offset: usize, page_path: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(page_path.len());
    for (level, &idx) in page_path.iter().enumerate() {
        if level == 0 {
            out.push(idx + page_offset as u32);
        } else {
            out.push(idx);
        }
    }
    out
}

// ============================================================================
// Test helpers shared by the cfg(test) modules below (public-API shapes)
// ============================================================================

#[cfg(test)]
mod test_edit_support {
    use azul::callbacks::DocumentChangeset;
    use azul::css::DocumentOperation;
    use azul::dom::{Dom, DomId, DomNodeId, NodeHierarchyItemId};
    use azul::error::DocumentEditError;
    use azul::misc::EditResumePoint;
    use azul::time::Instant;

    pub(super) use azul::app::AppliedEdit;
    pub(super) use azul::css::NodePosition;
    pub(super) use azul::dom::DocOpSplitNode;

    /// The "no node" id (the crate-internal `from_crate_internal(None)`
    /// encoding: inner 0).
    pub(super) fn null_node() -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_raw(0),
        }
    }

    /// A changeset targeting the model root, resuming at `resume_path`.
    pub(super) fn changeset(op: DocumentOperation, resume_path: Vec<u32>) -> DocumentChangeset {
        DocumentChangeset::new(
            null_node(),
            op,
            EditResumePoint {
                anchor_key: 0,
                node_path: resume_path.into(),
                position: NodePosition {
                    child_index: 0,
                    text_byte: Some(0).into(),
                },
            },
            Instant::now(),
        )
    }

    /// `DocumentChangeset::apply_to_dom` as a plain Rust `Result`. The error
    /// is mapped to a label because the ABI enum implements no `Debug` (so
    /// `.expect()` would not compile on it).
    pub(super) fn apply(
        model: &mut Dom,
        host_path: &[u32],
        cs: &DocumentChangeset,
    ) -> Result<AppliedEdit, &'static str> {
        cs.apply_to_dom(model, host_path.to_vec())
            .into_result()
            .map_err(|e| match e {
                DocumentEditError::HostNotFound => "host not found",
                DocumentEditError::TargetNotFound => "target not found",
                DocumentEditError::Unsupported => "unsupported operation",
            })
    }

    /// Split-`op` shorthand used by several tests.
    pub(super) fn split_op(text_byte: u32) -> DocumentOperation {
        DocumentOperation::SplitNode(DocOpSplitNode {
            node: null_node(),
            at: NodePosition {
                child_index: 0,
                text_byte: Some(text_byte).into(),
            },
        })
    }
}

#[cfg(test)]
mod edit_loop_tests {
    use super::test_edit_support::{apply, changeset, split_op};
    use super::*;
    use azul::css::DocumentOperation;

    fn model_text_provider(content: &Dom) -> impl FnMut(&[u32]) -> Option<String> + '_ {
        move |path: &[u32]| {
            let mut node = content;
            for &i in path {
                node = node.children.as_ref().get(i as usize)?;
            }
            fn own_text(d: &Dom) -> String {
                let mut s = String::new();
                for c in d.children.as_ref() {
                    match &c.root.node_type {
                        NodeType::Text(t) => s.push_str(box_str(t)),
                        _ => s.push_str(&own_text(c)),
                    }
                }
                s
            }
            Some(own_text(node))
        }
    }

    #[test]
    fn markdown_round_trips_through_the_serializer() {
        let md = "# Title\n\nHello **world** paragraph.\n\n## Section\n\n- item one\n- item two\n\n> quoted line\n";
        let dom = markdown_to_content_dom(md);
        let mut provider = model_text_provider(&dom);
        let out = dom_to_markdown(&dom, &mut provider);
        assert_eq!(
            out,
            "# Title\n\nHello world paragraph.\n\n## Section\n\n- item one\n- item two\n\n> quoted line\n",
            "structure + text round-trip (inline markup flattens to text)"
        );
        // Negative control: a DIFFERENT text provider changes the output.
        let mut fake = |_: &[u32]| Some("REPLACED".to_string());
        let out2 = dom_to_markdown(&dom, &mut fake);
        assert!(out2.contains("# REPLACED"));
        assert_ne!(out, out2);
    }

    #[test]
    fn page_paths_shift_by_the_page_block_offset() {
        let md = (0..40)
            .map(|i| format!("Paragraph number {i} filling space with several words.\n"))
            .collect::<Vec<_>>()
            .join("\n");
        let dom = markdown_to_content_dom(&md);
        let pages = paginate(dom);
        assert!(
            pages.len() >= 2,
            "need a multi-page doc, got {}",
            pages.len()
        );
        let offsets = page_block_offsets(&pages);
        assert_eq!(offsets[0], 0);
        assert_eq!(
            offsets[1],
            pages[0].dom.children.as_ref().len(),
            "page 1 starts where page 0's blocks end"
        );
        // A path addressing block 2 of page 1 maps to model index offset+2.
        let mapped = page_path_to_model_path(offsets[1], &[2, 0]);
        assert_eq!(mapped[0] as usize, offsets[1] + 2);
        assert_eq!(mapped[1], 0);
        // The mapped path resolves to the SAME node type in the model.
        // (pages are clones of model subtrees, so this is the invariant the
        // whole edit loop rides on)
        let model = markdown_to_content_dom(&md);
        let page_block = &pages[1].dom.children.as_ref()[2];
        let model_block = &model.children.as_ref()[mapped[0] as usize];
        assert_eq!(
            std::mem::discriminant(&page_block.root.node_type),
            std::mem::discriminant(&model_block.root.node_type),
        );
    }

    #[test]
    fn structural_apply_updates_the_serialized_markdown() {
        // Forge the C11 loop app-side: split the first paragraph, apply via
        // the engine helper with the page->model mapped host path, reserialize.
        let md = "First paragraph here.\n\nSecond paragraph.\n";
        let mut model = markdown_to_content_dom(md);

        // Split block 0's text at byte 5 ("First| paragraph here.").
        // Resume names the NEW second node.
        let cs = changeset(split_op(5), vec![1u32]);

        // Page 0 hosts everything (single page); offset 0; host = model root.
        let host_path = page_path_to_model_path(0, &[]);
        let applied = apply(&mut model, &host_path, &cs).expect("apply split");
        assert!(matches!(applied.inverse, DocumentOperation::MergeNodes(_)));

        let mut provider = model_text_provider(&model);
        let out = dom_to_markdown(&model, &mut provider);
        assert_eq!(
            out, "First\n\nparagraph here.\n\nSecond paragraph.\n",
            "the split materialized as two markdown paragraphs"
        );
    }
}

#[cfg(test)]
mod save_round_trip_tests {
    use super::test_edit_support::{apply, changeset, split_op};
    use super::*;

    /// load -> (structural edit on the model) -> serialize -> save -> reload:
    /// the document that comes back must carry the edit, and paginate the
    /// same way. This is the capstone's f(markdown -> Dom -> pages -> markdown)
    /// closing on itself.
    #[test]
    fn save_round_trip_preserves_an_applied_edit() {
        let dir = std::env::temp_dir().join("azwriter_round_trip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.md");
        std::fs::write(
            &path,
            "# Title\n\nFirst paragraph here.\n\nSecond paragraph.\n",
        )
        .unwrap();

        let mut model = DocumentModel::from_path(&path);
        let before_pages = paginate(model.content.clone()).len();
        assert_eq!(before_pages, 1);

        // Split block 1 ("First paragraph here.") after "First".
        // NEW second part sits at index 2.
        let cs = changeset(split_op(5), vec![2u32]);
        apply(&mut model.content, &[], &cs).expect("apply");

        // Re-serialize from the model (what the live loop + save do).
        let mut none_provider = |_: &[u32]| None;
        model.markdown = dom_to_markdown(&model.content, &mut none_provider);
        assert!(
            model.markdown.contains("First\n\nparagraph here."),
            "the edit must be in the serialized markdown:\n{}",
            model.markdown
        );

        save_markdown(&path, &model).expect("save");

        // RELOAD: the file on disk carries the edit and re-parses to the
        // same block structure.
        let reloaded = DocumentModel::from_path(&path);
        assert_eq!(reloaded.markdown, model.markdown);
        assert_eq!(
            reloaded.content.children.as_ref().len(),
            model.content.children.as_ref().len(),
            "the reloaded DOM has the same block count as the edited model"
        );
        assert_eq!(paginate(reloaded.content.clone()).len(), before_pages);

        // Negative control: an UNSAVED model must not match a stale file.
        std::fs::write(&path, "# Different\n").unwrap();
        let stale = DocumentModel::from_path(&path);
        assert_ne!(stale.markdown, model.markdown);
    }
}

#[cfg(test)]
mod pdf_export_tests {
    use super::*;
    use azul::css::StyledDom;

    /// The capstone's f(markdown -> Dom -> PDF): the content DOM the canvas
    /// shows, styled and handed to the engine's typed PDF path, must yield
    /// a real multi-page PDF (header + page objects + embedded text).
    #[test]
    fn markdown_exports_to_a_real_pdf() {
        let md = "# Report\n\nFirst paragraph with several words in it.\n\n\
                  ## Section\n\n- alpha\n- beta\n\nClosing paragraph.\n";
        let content = markdown_to_content_dom(md);

        let mut doc = Dom::create_body().with_css("margin: 0; padding: 96px; background: white;");
        doc.add_child(content);
        let styled = StyledDom::create_from_dom(doc);

        // Same call the Export button makes, with empty resource handles
        // (the engine falls back to fresh system fonts).
        let pdf = Pdf::new();
        let bytes = pdf.from_styled_dom_with_resources(
            styled,
            794.0,
            1123.0,
            empty_font_cache(),
            empty_image_cache(),
        );
        let out = bytes.as_ref();

        assert!(
            out.len() > 1000,
            "expected a real PDF, got {} bytes",
            out.len()
        );
        assert_eq!(&out[..5], b"%PDF-", "PDF header");
        let text = String::from_utf8_lossy(out);
        assert!(
            text.contains("/Type /Page") || text.contains("/Type/Page"),
            "page objects"
        );

        // GEOMETRY: every text-positioning op must land INSIDE the page box.
        // A4 = 842pt tall; the export used to hand printpdf the page height
        // in PX (1123), so `page_height - y*0.75` put content above the page
        // and the PDF rendered BLANK while still carrying correct text ops,
        // fonts and page objects. Assert on the Tm y-coordinates.
        let mut ys: Vec<f32> = Vec::new();
        for line in text.lines() {
            // "1 0 0 1 <x> <y> Tm"
            if let Some(rest) = line.strip_suffix(" Tm") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() == 6 {
                    if let Ok(y) = parts[5].parse::<f32>() {
                        ys.push(y);
                    }
                }
            }
        }
        assert!(
            !ys.is_empty(),
            "expected text-matrix ops in the content stream"
        );
        let a4_h_pt = 1123.0 * 72.0 / 96.0;
        let above = ys.iter().filter(|y| **y > a4_h_pt).count();
        assert_eq!(
            above,
            0,
            "{above}/{} text ops sit ABOVE the {a4_h_pt}pt page box (px/pt \
             confusion); max y = {:?}",
            ys.len(),
            ys.iter().cloned().fold(f32::MIN, f32::max)
        );
        // Negative control: an EMPTY document must produce fewer bytes than
        // the real one (i.e. the content actually reached the PDF).
        let empty_styled = StyledDom::create_from_dom(
            Dom::create_body().with_css("margin: 0; padding: 96px;"),
        );
        let empty = pdf.from_styled_dom_with_resources(
            empty_styled,
            794.0,
            1123.0,
            empty_font_cache(),
            empty_image_cache(),
        );
        assert!(
            empty.as_ref().len() < out.len(),
            "content must add bytes: empty={} full={}",
            empty.as_ref().len(),
            out.len()
        );
        if let Ok(dst) = std::env::var("AZWRITER_DUMP_PDF") {
            let _ = std::fs::write(&dst, out);
            eprintln!("[test] wrote {} bytes to {dst}", out.len());
        }
    }
}

#[cfg(test)]
mod live_text_tests {
    use super::*;
    use azul::dom::IdOrClass;

    /// The `id` attributes on a node, through the public ids-and-classes
    /// accessor (the tagging path writes ids there via `Dom::with_id`).
    fn ids_of(d: &Dom) -> Vec<String> {
        d.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|ic| match ic {
                IdOrClass::Id(s) => Some(s.as_str().to_string()),
                IdOrClass::Class(_) => None,
            })
            .collect()
    }

    #[test]
    fn block_ids_are_model_indices_across_pages() {
        let md = (0..40)
            .map(|i| format!("Paragraph number {i} filling space with several words.\n"))
            .collect::<Vec<_>>()
            .join("\n");
        let content = markdown_to_content_dom(&md);
        let model_blocks = content.children.as_ref().len();

        let mut pages = paginate(content);
        assert!(pages.len() >= 2, "need multiple pages, got {}", pages.len());
        let offsets = page_block_offsets(&pages);
        tag_pages_with_block_ids(&mut pages, &offsets);

        // Every block on every page carries the id of its MODEL index, in
        // order, with no gaps and no repeats — that identity is what makes
        // the save-time text readback address the right paragraph.
        let mut seen: Vec<String> = Vec::new();
        for page in &pages {
            for block in page.dom.children.as_ref() {
                let ids = ids_of(block);
                assert_eq!(ids.len(), 1, "each block gets exactly one id, got {ids:?}");
                seen.push(ids[0].clone());
            }
        }
        let expected: Vec<String> = (0..model_blocks).map(block_dom_id).collect();
        assert_eq!(
            seen, expected,
            "block ids must enumerate the model's blocks exactly once, in order"
        );

        // Negative control: WITHOUT tagging there are no ids at all (so a
        // stale render can't accidentally satisfy the lookup).
        let untagged = paginate(markdown_to_content_dom(&md));
        let any_id = untagged.iter().any(|p| {
            p.dom
                .children
                .as_ref()
                .iter()
                .any(|b| !ids_of(b).is_empty())
        });
        assert!(!any_id, "untagged pages must carry no block ids");
    }

    #[test]
    fn nested_list_items_are_addressable_and_round_trip() {
        let content = markdown_to_content_dom("# T\n\n- alpha\n- beta\n");
        let mut pages = paginate(content);
        let offsets = page_block_offsets(&pages);
        tag_pages_with_block_ids(&mut pages, &offsets);

        // The ul is model block 1; its li children carry [1, j] ids.
        let ul = pages[0]
            .dom
            .children
            .as_ref()
            .iter()
            .find(|b| matches!(b.root.node_type, NodeType::Ul))
            .expect("ul present");
        let li_ids: Vec<String> = ul
            .children
            .as_ref()
            .iter()
            .flat_map(ids_of)
            .collect();
        assert!(
            li_ids.contains(&nested_dom_id(1, 0)) && li_ids.contains(&nested_dom_id(1, 1)),
            "list items must carry [block, child] ids, got {li_ids:?}"
        );
        // path_dom_id must produce exactly those ids for the serializer's paths.
        assert_eq!(
            path_dom_id(&[1, 0]).as_deref(),
            Some(nested_dom_id(1, 0).as_str())
        );
        assert_eq!(path_dom_id(&[1]).as_deref(), Some(block_dom_id(1).as_str()));
        assert_eq!(
            path_dom_id(&[1, 0, 2]),
            None,
            "deeper paths are unsupported"
        );

        // A live provider addressing a LIST ITEM must reach the markdown.
        let doc = markdown_to_content_dom("# T\n\n- alpha\n- beta\n");
        let mut p = |path: &[u32]| -> Option<String> {
            (path == [1u32, 0u32]).then(|| "EDITED alpha".to_string())
        };
        let out = dom_to_markdown(&doc, &mut p);
        assert!(
            out.contains("- EDITED alpha") && out.contains("- beta"),
            "the edited list item must round-trip, got:\n{out}"
        );
    }

    #[test]
    fn a_live_provider_overrides_model_text_block_by_block() {
        let content = markdown_to_content_dom("# Title\n\nOriginal body.\n");
        // Simulate the engine readback: block 1 was edited on screen.
        let mut provider = |path: &[u32]| -> Option<String> {
            match path.first() {
                Some(1) => Some("EDITED body text".to_string()),
                _ => None, // not rendered -> model text
            }
        };
        let out = dom_to_markdown(&content, &mut provider);
        assert_eq!(
            out, "# Title\n\nEDITED body text\n",
            "the live text must replace the model's for that block only"
        );
    }
}

#[cfg(test)]
mod resize_cost_tests {
    use super::*;

    #[test]
    fn memoized_pagination_is_free_on_repeat() {
        // The resize path: same document, many layout calls. The memo must
        // make every call after the first cost a tree split, not a layout.
        let md = "# T\n\n".to_string()
            + &(0..40)
                .map(|i| format!("Paragraph {i} with a reasonable number of words.\n"))
                .collect::<Vec<_>>()
                .join("\n");
        let content = markdown_to_content_dom(&md);
        let gen = next_generation();

        let t0 = std::time::Instant::now();
        let first = paginate_cached(&content, gen);
        let cold = t0.elapsed();

        let t1 = std::time::Instant::now();
        const N: u32 = 20;
        for _ in 0..N {
            let again = paginate_cached(&content, gen);
            assert_eq!(again.len(), first.len(), "same pages every time");
        }
        let warm = t1.elapsed() / N;
        eprintln!("[MEMO] cold={cold:?} warm={warm:?}");
        assert!(
            warm * 10 < cold,
            "a repeat layout must be at least 10x cheaper than the cold \
             pagination (cold={cold:?}, warm={warm:?}) - otherwise a resize \
             drag starves the compositor handshake again"
        );

        // Negative control: a NEW generation must recompute (not serve a
        // stale page split for an edited document).
        let edited = markdown_to_content_dom(&(md.clone() + "\nExtra tail paragraph.\n"));
        let pages2 = paginate_cached(&edited, next_generation());
        let last_before = first.last().unwrap().dom.children.as_ref().len();
        let last_after = pages2.last().unwrap().dom.children.as_ref().len();
        assert!(
            pages2.len() > first.len() || last_after > last_before,
            "a new generation must re-paginate the edited document"
        );
    }

    #[test]
    fn pagination_cost_per_layout_call() {
        // Was `read_to_string("/tmp/azwriter-sample.md")` with a two-line
        // fallback, i.e. on every machine but one it timed a document that is
        // not a document. Use the same generated sample the pagination test uses.
        let md = super::sample_tests::sample_markdown();
        let content = markdown_to_content_dom(&md);
        // Warm (first call parses fonts).
        let _ = paginate(content.clone());
        let t0 = std::time::Instant::now();
        const N: u32 = 10;
        for _ in 0..N {
            let _ = paginate(content.clone());
        }
        let per = t0.elapsed() / N;
        eprintln!("[COST] paginate() = {per:?} per call");
        assert!(per < std::time::Duration::from_millis(500), "sanity bound");
    }
}

#[cfg(test)]
mod undo_api_validation {
    use super::test_edit_support::{apply, changeset, split_op};
    use super::*;

    fn texts(d: &Dom) -> Vec<String> {
        fn own(d: &Dom) -> String {
            let mut s = String::new();
            for c in d.children.as_ref() {
                match &c.root.node_type {
                    NodeType::Text(t) => s.push_str(box_str(t)),
                    _ => s.push_str(&own(c)),
                }
            }
            s
        }
        d.children.as_ref().iter().map(own).collect()
    }

    /// Undo -> redo -> undo must be stable, and each direction must hand
    /// back the operation for the other (this is what makes the app's two
    /// stacks a single code path).
    #[test]
    fn undo_and_redo_are_symmetric_replays() {
        let mut model = markdown_to_content_dom("First paragraph here.\n\nSecond.\n");
        let original = texts(&model);

        let cs_split = changeset(split_op(5), vec![1]);
        let applied = apply(&mut model, &[], &cs_split).expect("split");
        let split_result = texts(&model);
        assert_ne!(split_result, original);

        // UNDO
        let undo_cs = changeset(
            applied.inverse.clone(),
            applied.inverse_resume.node_path.as_ref().to_vec(),
        );
        let undone = apply(&mut model, &[], &undo_cs).expect("undo");
        assert_eq!(texts(&model), original, "undo restores");

        // REDO: replay what the undo handed back.
        let redo_cs = changeset(
            undone.inverse.clone(),
            undone.inverse_resume.node_path.as_ref().to_vec(),
        );
        let redone = apply(&mut model, &[], &redo_cs).expect("redo");
        assert_eq!(texts(&model), split_result, "redo re-applies the split");

        // UNDO again, from the redo's own inverse.
        let undo2 = changeset(
            redone.inverse.clone(),
            redone.inverse_resume.node_path.as_ref().to_vec(),
        );
        apply(&mut model, &[], &undo2).expect("undo 2");
        assert_eq!(texts(&model), original, "the cycle is stable");
    }

    /// THE VALIDATION: can an app undo a structural edit using only what the
    /// engine hands back? Apply a split, then apply its inverse, and require
    /// the document to return to its exact starting shape.
    #[test]
    fn an_app_can_undo_a_structural_edit_with_the_returned_inverse() {
        let mut model = markdown_to_content_dom("First paragraph here.\n\nSecond.\n");
        let before = texts(&model);
        assert_eq!(before, vec!["First paragraph here.", "Second."]);

        // DO: split block 0 after "First" (resume names the NEW second part).
        let cs = changeset(split_op(5), vec![1]);
        let applied = apply(&mut model, &[], &cs).expect("apply split");
        assert_eq!(texts(&model), vec!["First", " paragraph here.", "Second."]);

        // UNDO: re-record the inverse through the same loop. The app knows
        // only `applied.inverse` and the original changeset's resume point.
        // The engine hands back the resume point the inverse must be
        // replayed with (index resolution is asymmetric between split and
        // merge) — an app must not do that arithmetic itself.
        let undo_cs = changeset(
            applied.inverse.clone(),
            applied.inverse_resume.node_path.as_ref().to_vec(),
        );
        let undone = apply(&mut model, &[], &undo_cs);

        assert!(undone.is_ok(), "the inverse must apply");
        assert_eq!(
            texts(&model),
            before,
            "undo must restore the document EXACTLY (the inverse's resume \
             point must be usable as handed back)"
        );
    }
}
