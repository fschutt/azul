use std::path::{Path, PathBuf};

pub use azul::font::FontCacheSnapshot;
pub use azul::image::ImageCacheSnapshot;
use azul::{
    css::{BoxOrStaticString, LayoutSize, LogicalSize},
    dom::{Dom, DomSplit, NodeType},
    misc::PaginationSnapshot,
    pdf::Pdf,
};

pub const A4_PAGE_W: f32 = 794.0;
pub const A4_PAGE_H: f32 = 1123.0;
pub const A4_MARGIN: f32 = 96.0;

pub fn page_content_size() -> LogicalSize {
    LogicalSize {
        width: A4_PAGE_W - 2.0 * A4_MARGIN,
        height: A4_PAGE_H - 2.0 * A4_MARGIN,
    }
}

pub fn empty_font_cache() -> FontCacheSnapshot {
    FontCacheSnapshot::empty()
}

pub fn empty_image_cache() -> ImageCacheSnapshot {
    ImageCacheSnapshot::empty()
}

#[must_use]
pub fn content_dom_from_ir(ir: &crate::ir::IrDocument) -> Dom {
    crate::ir::to_content_dom(ir, DOC_CSS)
}

fn box_str(s: &BoxOrStaticString) -> &str {
    unsafe {
        match s {
            BoxOrStaticString::Boxed(p) => (**p).as_str(),
            BoxOrStaticString::Static(p) => (**p).as_str(),
        }
    }
}

const DOC_CSS: &str = "
    body { font-family: 'Liberation Sans', sans-serif; font-size: 15px;
           color: #1a1a1a; line-height: 1.35; }
    /* min-height is one line box. An EMPTY paragraph has no inline content,
       so it lays out at zero height — invisible, and with no area to click,
       which means the caret can never be placed in a blank document and the
       soft keyboard can never be summoned. Every word processor gives an
       empty paragraph a full line for exactly this reason. 15px * 1.35
       line-height = 20px. */
    p    { margin-bottom: 11px; min-height: 20px; }
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

#[derive(Clone)]
pub struct DocumentModel {
    pub path: Option<PathBuf>,
    pub ir: crate::ir::IrDocument,
    pub markdown: String,
    pub content: Dom,
    pub dirty: bool,
    pub generation: u64,
}

impl DocumentModel {
    #[must_use]
    pub fn untitled() -> Self {
        let ir = crate::ir::from_markdown("");
        let content = crate::ir::to_content_dom(&ir, DOC_CSS);
        Self {
            path: None,
            ir,
            markdown: String::new(),
            content,
            dirty: false,
            generation: next_generation(),
        }
    }

    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        let error_doc = |e: String| {
            eprintln!("[azwriter] cannot open {}: {e}", path.display());
            crate::ir::from_markdown(&format!(
                "# Cannot open document\n\n`{}`\n\n{}\n\n(cwd: `{}`)",
                path.display(),
                e,
                std::env::current_dir().map_or_else(|_| "?".into(), |d| d.display().to_string()),
            ))
        };
        let is_docx = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"));
        let ir = if is_docx {
            match std::fs::read(path) {
                Ok(bytes) => match crate::ir::from_docx_bytes(&bytes) {
                    Ok(ir) => ir,
                    Err(e) => error_doc(e),
                },
                Err(e) => error_doc(e.to_string()),
            }
        } else {
            match std::fs::read_to_string(path) {
                Ok(md) => crate::ir::from_markdown(&md),
                Err(e) => error_doc(e.to_string()),
            }
        };
        let content = crate::ir::to_content_dom(&ir, DOC_CSS);
        let markdown = crate::ir::to_markdown(&ir);
        Self {
            path: Some(path.to_path_buf()),
            ir,
            markdown,
            content,
            dirty: false,
            generation: next_generation(),
        }
    }

    pub fn refresh_derived(&mut self) {
        self.content = crate::ir::to_content_dom(&self.ir, DOC_CSS);
        self.markdown = crate::ir::to_markdown(&self.ir);
        self.generation = next_generation();
    }

    pub fn reparse(&mut self) {
        self.ir = crate::ir::from_markdown(&self.markdown);
        self.content = crate::ir::to_content_dom(&self.ir, DOC_CSS);
        self.generation = next_generation();
    }

    pub fn page_count_bounded(
        &self,
        fonts: Option<FontCacheSnapshot>,
        monitor_px: Option<LayoutSize>,
    ) -> (usize, bool) {
        let (mon_w, mon_h) = match monitor_px {
            Some(m) if m.width > 0 && m.height > 0 => (m.width as usize, m.height as usize),
            _ => {
                return (
                    page_count_cached(&self.content, self.generation, fonts),
                    true,
                )
            }
        };
        let char_budget = mon_w.saturating_mul(mon_h);
        let line_budget = mon_h;

        if self.markdown.len() <= char_budget && self.markdown.lines().count() <= line_budget {
            return (
                page_count_cached(&self.content, self.generation, fonts),
                true,
            );
        }

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

        let split = DomSplit::at_path(&self.content, vec![prefix_blocks as u32]);
        let prefix = split.head;
        let prefix_paths = compute_break_paths_with_fonts(&prefix, fonts);
        let prefix_pages = prefix_paths.len() + 1;
        seed_break_paths(self.generation, prefix_paths, false);

        let total_chars = self.markdown.len().max(1);
        let est = (prefix_pages as f64 * total_chars as f64 / chars.max(1) as f64).ceil() as usize;
        (est.max(prefix_pages), false)
    }

    pub fn touch(&mut self) {
        self.generation = next_generation();
    }

    #[must_use]
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Document1".to_string())
    }

    #[must_use]
    pub fn word_count(&self) -> usize {
        self.markdown.split_whitespace().count()
    }
}

pub struct Page {
    pub dom: Dom,
}

#[must_use]
pub fn blank_document() -> Dom {
    markdown_to_content_dom("")
}

static DUMP_XML: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

pub fn init_dump_xml(path: Option<PathBuf>) {
    let _ = DUMP_XML.set(path);
}

#[must_use]
pub fn markdown_to_content_dom(markdown: &str) -> Dom {
    crate::ir::to_content_dom(&crate::ir::from_markdown(markdown), DOC_CSS)
}

#[must_use]
pub fn load_markdown(path: &Path) -> Dom {
    let source = std::fs::read_to_string(path).unwrap_or_default();
    markdown_to_content_dom(&source)
}

pub fn save_markdown(path: &Path, model: &DocumentModel) -> Result<(), String> {
    let contents = if model.markdown.is_empty() {
        format!("# {}\n", model.display_name())
    } else {
        model.markdown.clone()
    };
    std::fs::write(path, contents).map_err(|e| e.to_string())
}

pub fn next_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static G: AtomicU64 = AtomicU64::new(1);
    G.fetch_add(1, Ordering::Relaxed)
}

#[must_use]
pub fn paginate_cached(content: &Dom, generation: u64) -> Vec<Page> {
    paginate_cached_with_fonts(content, generation, None)
}

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
    static BREAK_MEMO: RefCell<Option<(u64, Vec<Vec<u32>>, bool)>> =
        const { RefCell::new(None) };
}

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

fn try_cached_break_paths(generation: u64) -> Option<(Vec<Vec<u32>>, bool)> {
    BREAK_MEMO.with(|m| {
        m.borrow()
            .as_ref()
            .and_then(|(g, p, c)| (*g == generation).then(|| (p.clone(), *c)))
    })
}

pub fn pagination_is_complete(generation: u64) -> bool {
    matches!(try_cached_break_paths(generation), Some((_, true)))
}

pub fn break_paths_for(content: &Dom, fonts: Option<FontCacheSnapshot>) -> Vec<Vec<u32>> {
    compute_break_paths_with_fonts(content, fonts)
}

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

pub fn page_count_cached(
    content: &Dom,
    generation: u64,
    fonts: Option<FontCacheSnapshot>,
) -> usize {
    cached_break_paths(content, generation, fonts).len() + 1
}

pub fn paginate_range_cached(
    content: &Dom,
    generation: u64,
    fonts: Option<FontCacheSnapshot>,
    first: usize,
    count: usize,
) -> Vec<Page> {
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

#[must_use]
pub fn paginate(content: Dom) -> Vec<Page> {
    let break_paths = compute_break_paths(&content);
    split_content_at(content, &break_paths)
}

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

fn compute_break_paths(content: &Dom) -> Vec<Vec<u32>> {
    compute_break_paths_with_fonts(content, None)
}

fn compute_break_paths_with_fonts(
    content: &Dom,
    fonts: Option<FontCacheSnapshot>,
) -> Vec<Vec<u32>> {
    use azul::css::StyledDom;

    let styled_dom = {
        let _p = crate::perf::Phase::start("    dom_clone+cascade");
        StyledDom::create_from_dom(content.clone())
    };

    let content_size = page_content_size();
    let _p_pag = crate::perf::Phase::start("    compute_pagination");
    let pdf = Pdf::create();
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
        if !path.is_empty() {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let single = paginate(markdown_to_content_dom("just one line\n"));
        assert_eq!(single.len(), 1);
    }
}

#[cfg(test)]
mod sample_tests {
    use super::*;

    pub(super) fn sample_markdown() -> String {
        let mut md = String::from("# AzWriter sample document\n\n");
        for section in 0..8 {
            md.push_str(&format!("## Section {section}\n\n"));
            for para in 0..6 {
                md.push_str(&format!(
                    "Paragraph {para} of section {section}. The quick brown fox jumps over the \
                     lazy dog, repeatedly, so that this document is long enough to be paginated \
                     into more than a single page by the layout engine.\n\n"
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

#[must_use]
pub fn block_dom_id(model_index: usize) -> String {
    format!("mw-blk-{model_index}")
}

#[must_use]
pub fn nested_dom_id(model_index: usize, child_index: usize) -> String {
    format!("mw-blk-{model_index}-{child_index}")
}

#[must_use]
pub fn path_dom_id(path: &[u32]) -> Option<String> {
    match path {
        [b] => Some(block_dom_id(*b as usize)),
        [b, c] => Some(nested_dom_id(*b as usize, *c as usize)),
        _ => None,
    }
}

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

#[cfg(test)]
mod test_edit_support {
    pub(super) use azul::{app::AppliedEdit, css::NodePosition, dom::DocOpSplitNode};
    use azul::{
        callbacks::DocumentChangeset,
        css::DocumentOperation,
        dom::{Dom, DomId, DomNodeId, NodeHierarchyItemId},
        error::DocumentEditError,
        misc::EditResumePoint,
        time::Instant,
    };

    pub(super) fn null_node() -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_raw(0),
        }
    }

    pub(super) fn changeset(op: DocumentOperation, resume_path: Vec<u32>) -> DocumentChangeset {
        DocumentChangeset::create(
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
    use azul::css::DocumentOperation;

    use super::{
        test_edit_support::{apply, changeset, split_op},
        *,
    };

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
        let md = "# Title\n\nHello **world** paragraph.\n\n## Section\n\n- item one\n- item \
                  two\n\n> quoted line\n";
        let dom = markdown_to_content_dom(md);
        let mut provider = model_text_provider(&dom);
        let out = dom_to_markdown(&dom, &mut provider);
        assert_eq!(
            out,
            "# Title\n\nHello world paragraph.\n\n## Section\n\n- item one\n- item two\n\n> \
             quoted line\n",
            "structure + text round-trip (inline markup flattens to text)"
        );
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
        let mapped = page_path_to_model_path(offsets[1], &[2, 0]);
        assert_eq!(mapped[0] as usize, offsets[1] + 2);
        assert_eq!(mapped[1], 0);
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
        let md = "First paragraph here.\n\nSecond paragraph.\n";
        let mut model = markdown_to_content_dom(md);

        let cs = changeset(split_op(5), vec![1u32]);

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
    use super::{
        test_edit_support::{apply, changeset, split_op},
        *,
    };

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

        let cs = changeset(split_op(5), vec![2u32]);
        apply(&mut model.content, &[], &cs).expect("apply");

        let mut none_provider = |_: &[u32]| None;
        model.markdown = dom_to_markdown(&model.content, &mut none_provider);
        assert!(
            model.markdown.contains("First\n\nparagraph here."),
            "the edit must be in the serialized markdown:\n{}",
            model.markdown
        );

        save_markdown(&path, &model).expect("save");

        let reloaded = DocumentModel::from_path(&path);
        assert_eq!(reloaded.markdown, model.markdown);
        assert_eq!(
            reloaded.content.children.as_ref().len(),
            model.content.children.as_ref().len(),
            "the reloaded DOM has the same block count as the edited model"
        );
        assert_eq!(paginate(reloaded.content.clone()).len(), before_pages);

        std::fs::write(&path, "# Different\n").unwrap();
        let stale = DocumentModel::from_path(&path);
        assert_ne!(stale.markdown, model.markdown);
    }
}

#[cfg(test)]
mod pdf_export_tests {
    use azul::css::StyledDom;

    use super::*;

    fn find_from(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        hay.get(from..)?
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
    }

    #[test]
    fn markdown_exports_to_a_real_pdf() {
        let md = "# Report\n\nFirst paragraph with several words in it.\n\n## Section\n\n- \
                  alpha\n- beta\n\nClosing paragraph.\n";
        let content = markdown_to_content_dom(md);

        let mut doc = Dom::create_body().with_css("margin: 0; padding: 96px; background: white;");
        doc.add_child(content);
        let styled = StyledDom::create_from_dom(doc);

        let pdf = Pdf::create();
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
        let mut text = String::from_utf8_lossy(out).into_owned();
        {
            use std::io::Read;
            let mut at = 0usize;
            while let Some(s_off) = find_from(out, b"stream", at) {
                let data_start = match out.get(s_off + 6) {
                    Some(b'\r') => s_off + 8,
                    Some(b'\n') => s_off + 7,
                    _ => s_off + 6,
                };
                let Some(e_off) = find_from(out, b"endstream", data_start) else {
                    break;
                };
                let mut inflated = String::new();
                let mut dec = flate2::read::ZlibDecoder::new(&out[data_start..e_off]);
                if dec.read_to_string(&mut inflated).is_ok() {
                    text.push('\n');
                    text.push_str(&inflated);
                }
                at = e_off + 9;
            }
        }
        let text = text;
        assert!(
            text.contains("/Type /Page") || text.contains("/Type/Page"),
            "page objects"
        );

        let mut ys: Vec<f32> = Vec::new();
        for line in text.lines() {
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
            "{above}/{} text ops sit ABOVE the {a4_h_pt}pt page box (px/pt confusion); max y = \
             {:?}",
            ys.len(),
            ys.iter().cloned().fold(f32::MIN, f32::max)
        );
        let empty_styled =
            StyledDom::create_from_dom(Dom::create_body().with_css("margin: 0; padding: 96px;"));
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
    use azul::dom::IdOrClass;

    use super::*;

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

        let ul = pages[0]
            .dom
            .children
            .as_ref()
            .iter()
            .find(|b| matches!(b.root.node_type, NodeType::Ul))
            .expect("ul present");
        let li_ids: Vec<String> = ul.children.as_ref().iter().flat_map(ids_of).collect();
        assert!(
            li_ids.contains(&nested_dom_id(1, 0)) && li_ids.contains(&nested_dom_id(1, 1)),
            "list items must carry [block, child] ids, got {li_ids:?}"
        );
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
        let mut provider = |path: &[u32]| -> Option<String> {
            match path.first() {
                Some(1) => Some("EDITED body text".to_string()),
                _ => None,
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
            "a repeat layout must be at least 10x cheaper than the cold pagination \
             (cold={cold:?}, warm={warm:?}) - otherwise a resize drag starves the compositor \
             handshake again"
        );

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
        let md = super::sample_tests::sample_markdown();
        let content = markdown_to_content_dom(&md);
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
    use super::{
        test_edit_support::{apply, changeset, split_op},
        *,
    };

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

    #[test]
    fn undo_and_redo_are_symmetric_replays() {
        let mut model = markdown_to_content_dom("First paragraph here.\n\nSecond.\n");
        let original = texts(&model);

        let cs_split = changeset(split_op(5), vec![1]);
        let applied = apply(&mut model, &[], &cs_split).expect("split");
        let split_result = texts(&model);
        assert_ne!(split_result, original);

        let undo_cs = changeset(
            applied.inverse.clone(),
            applied.inverse_resume.node_path.as_ref().to_vec(),
        );
        let undone = apply(&mut model, &[], &undo_cs).expect("undo");
        assert_eq!(texts(&model), original, "undo restores");

        let redo_cs = changeset(
            undone.inverse.clone(),
            undone.inverse_resume.node_path.as_ref().to_vec(),
        );
        let redone = apply(&mut model, &[], &redo_cs).expect("redo");
        assert_eq!(texts(&model), split_result, "redo re-applies the split");

        let undo2 = changeset(
            redone.inverse.clone(),
            redone.inverse_resume.node_path.as_ref().to_vec(),
        );
        apply(&mut model, &[], &undo2).expect("undo 2");
        assert_eq!(texts(&model), original, "the cycle is stable");
    }

    #[test]
    fn an_app_can_undo_a_structural_edit_with_the_returned_inverse() {
        let mut model = markdown_to_content_dom("First paragraph here.\n\nSecond.\n");
        let before = texts(&model);
        assert_eq!(before, vec!["First paragraph here.", "Second."]);

        let cs = changeset(split_op(5), vec![1]);
        let applied = apply(&mut model, &[], &cs).expect("apply split");
        assert_eq!(texts(&model), vec!["First", " paragraph here.", "Second."]);

        let undo_cs = changeset(
            applied.inverse.clone(),
            applied.inverse_resume.node_path.as_ref().to_vec(),
        );
        let undone = apply(&mut model, &[], &undo_cs);

        assert!(undone.is_ok(), "the inverse must apply");
        assert_eq!(
            texts(&model),
            before,
            "undo must restore the document EXACTLY (the inverse's resume point must be usable as \
             handed back)"
        );
    }
}
