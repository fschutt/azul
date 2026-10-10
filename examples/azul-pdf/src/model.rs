//! AzPdf's model, plain Rust (tested in `model_tests.rs` without a window):
//! the zoom, the vertical strip of pages, page size labels, the widths pages
//! are rendered at, the page cache, what to render next, search over the
//! pages' text, the recent documents, the form's fields and values, and small
//! parsers.

use std::{collections::BTreeMap, ops::Range};

use azul_appkit::find::{matches, TextMatch};
use serde::{Deserialize, Serialize};

/// CSS px per PDF point: a point is 1/72 inch, a CSS px 1/96 inch (azul's
/// `PDF_PX_PER_PT`; repeated here so the model needs no libazul).
pub const PX_PER_PT: f32 = 96.0 / 72.0;

/// The padding around the strip of pages, CSS px.
pub const VIEW_PAD: f32 = 24.0;
/// The gap between two pages, CSS px.
pub const PAGE_GAP: f32 = 16.0;

/// The smallest and largest scale a zoom gives.
pub const MIN_SCALE: f32 = 0.1;
pub const MAX_SCALE: f32 = 8.0;

/// The stops zoom in / zoom out step through, in percent.
pub const ZOOM_STEPS: [u32; 12] = [25, 33, 50, 67, 75, 100, 125, 150, 200, 300, 400, 800];

/// Pages are rendered at widths that are multiples of this (px), so a small
/// zoom change re-uses a render.
pub const RENDER_BUCKET: u32 = 64;
/// The widest render, px.
pub const MAX_RENDER_WIDTH: u32 = 4096;

/// Characters of context on each side of a search match.
pub const SNIPPET_CONTEXT: usize = 30;

/// How many documents the recent list keeps.
pub const MAX_RECENT: usize = 20;

/// A page's size in PDF points (azul's `PdfPageSize`, copied into the model).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PageSize {
    pub width_pt: f32,
    pub height_pt: f32,
}

impl PageSize {
    /// The size in CSS px at `scale` (1.0 = the paper size on screen).
    #[must_use]
    pub fn css(self, scale: f32) -> (f32, f32) {
        (
            self.width_pt * PX_PER_PT * scale,
            self.height_pt * PX_PER_PT * scale,
        )
    }
}

// ==== Zoom ====

/// How the page view sizes the pages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// The widest page fills the view's width.
    FitWidth,
    /// A whole page (the largest) fits in the view.
    FitPage,
    /// A fixed scale: 100 is the paper size on screen.
    Percent(u32),
}

impl Default for Zoom {
    fn default() -> Self {
        Zoom::FitWidth
    }
}

impl Zoom {
    /// The scale for a view of `view_w` x `view_h` CSS px showing `pages`,
    /// within [`MIN_SCALE`, `MAX_SCALE`]; 1.0 without pages.
    #[must_use]
    pub fn scale(self, view_w: f32, view_h: f32, pages: &[PageSize]) -> f32 {
        if pages.is_empty() {
            return 1.0;
        }
        let widest = pages.iter().map(|p| p.css(1.0).0).fold(0.0_f32, f32::max);
        let tallest = pages.iter().map(|p| p.css(1.0).1).fold(0.0_f32, f32::max);
        let fit_w = (view_w - 2.0 * VIEW_PAD) / widest.max(1.0);
        let fit_h = (view_h - 2.0 * VIEW_PAD) / tallest.max(1.0);
        let raw = match self {
            Zoom::FitWidth => fit_w,
            Zoom::FitPage => fit_w.min(fit_h),
            Zoom::Percent(p) => p as f32 / 100.0,
        };
        if raw.is_finite() {
            raw.clamp(MIN_SCALE, MAX_SCALE)
        } else {
            1.0
        }
    }

    /// The first stop above `scale`.
    #[must_use]
    pub fn zoom_in(scale: f32) -> Zoom {
        let percent = scale * 100.0;
        let step = ZOOM_STEPS
            .iter()
            .copied()
            .find(|s| *s as f32 > percent + 0.5)
            .unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1]);
        Zoom::Percent(step)
    }

    /// The first stop below `scale`.
    #[must_use]
    pub fn zoom_out(scale: f32) -> Zoom {
        let percent = scale * 100.0;
        let step = ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|s| (*s as f32) < percent - 0.5)
            .unwrap_or(ZOOM_STEPS[0]);
        Zoom::Percent(step)
    }

    /// What the zoom box shows.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Zoom::FitWidth => "Fit width".to_string(),
            Zoom::FitPage => "Fit page".to_string(),
            Zoom::Percent(p) => format!("{p} %"),
        }
    }

    /// The choices of the zoom drop-down: the two fits, then the stops.
    #[must_use]
    pub fn choices() -> Vec<Zoom> {
        let mut all = vec![Zoom::FitWidth, Zoom::FitPage];
        all.extend(ZOOM_STEPS.iter().map(|s| Zoom::Percent(*s)));
        all
    }
}

// ==== The strip of pages ====

/// The pages stacked top to bottom at one scale, in CSS px: each page's top
/// and size, and the strip's size (padding included).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Strip {
    pub tops: Vec<f32>,
    /// (width, height) per page.
    pub sizes: Vec<(f32, f32)>,
    pub width: f32,
    pub height: f32,
}

impl Strip {
    #[must_use]
    pub fn new(pages: &[PageSize], scale: f32) -> Strip {
        let mut tops = Vec::with_capacity(pages.len());
        let mut sizes = Vec::with_capacity(pages.len());
        let mut y = VIEW_PAD;
        let mut widest = 0.0_f32;
        for (i, page) in pages.iter().enumerate() {
            if i > 0 {
                y += PAGE_GAP;
            }
            let (w, h) = page.css(scale);
            tops.push(y);
            sizes.push((w, h));
            widest = widest.max(w);
            y += h;
        }
        Strip {
            tops,
            sizes,
            width: widest + 2.0 * VIEW_PAD,
            height: y + VIEW_PAD,
        }
    }

    /// The number of pages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tops.len()
    }

    /// No pages.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tops.is_empty()
    }

    fn bottom(&self, page: usize) -> f32 {
        self.tops[page] + self.sizes[page].1
    }

    /// The pages that meet the view `[y, y + h)`; an empty range past the end.
    #[must_use]
    pub fn visible(&self, y: f32, h: f32) -> Range<usize> {
        let n = self.len();
        let start = (0..n).find(|&i| self.bottom(i) > y).unwrap_or(n);
        let end = (start..n).find(|&i| self.tops[i] >= y + h).unwrap_or(n);
        start..end.max(start)
    }

    /// The page at offset `y`: each page owns half of the gaps around it.
    #[must_use]
    pub fn page_at(&self, y: f32) -> usize {
        (1..self.len())
            .rev()
            .find(|&i| self.tops[i] - PAGE_GAP / 2.0 <= y)
            .unwrap_or(0)
    }

    /// The scroll offset that shows `page` (clamped to the last) at the top.
    #[must_use]
    pub fn top_of(&self, page: usize) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let page = page.min(self.len() - 1);
        if page == 0 {
            0.0
        } else {
            self.tops[page] - PAGE_GAP / 2.0
        }
    }
}

// ==== Labels and render sizes ====

/// Paper sizes a label names: name, width x height in mm, shown in inches.
const PAPERS: [(&str, f32, f32, bool); 8] = [
    ("A3", 297.0, 420.0, false),
    ("A4", 210.0, 297.0, false),
    ("A5", 148.0, 210.0, false),
    ("A6", 105.0, 148.0, false),
    ("B5", 176.0, 250.0, false),
    ("Letter", 215.9, 279.4, true),
    ("Legal", 215.9, 355.6, true),
    ("Tabloid", 279.4, 431.8, true),
];

/// A number with at most one decimal, without a trailing `.0`.
fn short_number(v: f32) -> String {
    let rounded = (v * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < 0.05 {
        format!("{}", rounded.round() as i64)
    } else {
        format!("{rounded:.1}")
    }
}

/// "A4 · 210 × 297 mm", "A4 landscape · 297 × 210 mm", "Letter · 8.5 × 11 in",
/// or "176 × 176 mm" for a size that is no paper size.
#[must_use]
pub fn size_label(size: PageSize) -> String {
    let w_mm = size.width_pt * 25.4 / 72.0;
    let h_mm = size.height_pt * 25.4 / 72.0;
    let near = |a: f32, b: f32| (a - b).abs() < 2.0;
    for (name, pw, ph, inches) in PAPERS {
        let portrait = near(w_mm, pw) && near(h_mm, ph);
        let landscape = near(w_mm, ph) && near(h_mm, pw);
        if !(portrait || landscape) {
            continue;
        }
        let orientation = if landscape && !portrait {
            " landscape"
        } else {
            ""
        };
        let dims = if inches {
            format!(
                "{} \u{d7} {} in",
                short_number(size.width_pt / 72.0),
                short_number(size.height_pt / 72.0)
            )
        } else {
            format!("{} \u{d7} {} mm", w_mm.round() as i64, h_mm.round() as i64)
        };
        return format!("{name}{orientation} \u{b7} {dims}");
    }
    format!("{} \u{d7} {} mm", w_mm.round() as i64, h_mm.round() as i64)
}

/// The width in px a page is rendered at for `css_width` on a display of
/// `dpi_factor`: rounded up to a [`RENDER_BUCKET`], within
/// [`RENDER_BUCKET`, `MAX_RENDER_WIDTH`].
#[must_use]
pub fn render_width(css_width: f32, dpi_factor: f32) -> u32 {
    let px = css_width * dpi_factor;
    if !px.is_finite() || px <= 0.0 {
        return RENDER_BUCKET;
    }
    let buckets = (px / RENDER_BUCKET as f32).ceil().max(1.0);
    let width = (buckets as u64).saturating_mul(u64::from(RENDER_BUCKET));
    width.clamp(u64::from(RENDER_BUCKET), u64::from(MAX_RENDER_WIDTH)) as u32
}

// ==== The page cache ====

/// Rendered pages by (page, width), the least recently used one forgotten
/// first when `cap` is reached. Generic so the tests need no images.
#[derive(Clone, Debug)]
pub struct PageCache<T> {
    /// Oldest use first.
    entries: Vec<(usize, u32, T)>,
    cap: usize,
}

impl<T: Clone> PageCache<T> {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        PageCache {
            entries: Vec::new(),
            cap: cap.max(1),
        }
    }

    /// The render of `page` at `width`, marked as used now.
    pub fn get(&mut self, page: usize, width: u32) -> Option<T> {
        let i = self
            .entries
            .iter()
            .position(|(p, w, _)| *p == page && *w == width)?;
        let entry = self.entries.remove(i);
        let value = entry.2.clone();
        self.entries.push(entry);
        Some(value)
    }

    /// Whether `page` is rendered at `width`.
    #[must_use]
    pub fn has(&self, page: usize, width: u32) -> bool {
        self.entries
            .iter()
            .any(|(p, w, _)| *p == page && *w == width)
    }

    /// The render of `page` whose width is nearest `width` (to show while the
    /// sharp one is made).
    #[must_use]
    pub fn nearest(&self, page: usize, width: u32) -> Option<T> {
        self.entries
            .iter()
            .filter(|(p, _, _)| *p == page)
            .min_by_key(|(_, w, _)| w.abs_diff(width))
            .map(|(_, _, v)| v.clone())
    }

    /// Keeps a render (replacing one of the same page and width).
    pub fn insert(&mut self, page: usize, width: u32, value: T) {
        self.entries
            .retain(|(p, w, _)| !(*p == page && *w == width));
        while self.entries.len() >= self.cap {
            self.entries.remove(0);
        }
        self.entries.push((page, width, value));
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// What to render next: the `wanted` pages in order at `width`, minus those
/// `cached` and those `running` at that width, at most `max`.
#[must_use]
pub fn plan_renders(
    wanted: &[usize],
    width: u32,
    cached: impl Fn(usize) -> bool,
    running: &[(usize, u32)],
    max: usize,
) -> Vec<(usize, u32)> {
    wanted
        .iter()
        .copied()
        .filter(|page| !cached(*page) && !running.contains(&(*page, width)))
        .map(|page| (page, width))
        .take(max)
        .collect()
}

// ==== Search ====

/// One match: its page and the text around it on one line.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub page: usize,
    pub snippet: String,
}

/// `text` with every run of whitespace as one space.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every match of `needle` in the pages' texts (one string per page), in
/// page order, with [`SNIPPET_CONTEXT`] characters around it.
#[must_use]
pub fn search(pages: &[String], needle: &str, how: TextMatch) -> Vec<Hit> {
    let mut hits = Vec::new();
    if needle.is_empty() {
        return hits;
    }
    for (page, text) in pages.iter().enumerate() {
        for (start, end) in matches(text, needle, how) {
            let before: Vec<char> = text[..start].chars().collect();
            let after: Vec<char> = text[end..].chars().collect();
            let cut_before = before.len() > SNIPPET_CONTEXT;
            let cut_after = after.len() > SNIPPET_CONTEXT;
            let before: String = before[before.len().saturating_sub(SNIPPET_CONTEXT)..]
                .iter()
                .collect();
            let after: String = after[..after.len().min(SNIPPET_CONTEXT)].iter().collect();
            let mut snippet = String::new();
            if cut_before {
                snippet.push('\u{2026}');
            }
            snippet.push_str(&one_line(&format!("{before}{}{after}", &text[start..end])));
            if cut_after {
                snippet.push('\u{2026}');
            }
            hits.push(Hit { page, snippet });
        }
    }
    hits
}

// ==== The recent documents (pdf/recent.json) ====

/// One recently opened document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentDoc {
    /// Where it is on this computer.
    pub path: String,
    pub title: String,
    pub pages: usize,
    /// The page it was left at (0-based).
    pub last_page: usize,
    /// When it was last opened, seconds since 1970.
    pub opened: u64,
}

/// The recent documents, the latest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    pub docs: Vec<RecentDoc>,
}

impl Recent {
    /// Reads `recent.json`; anything unreadable is an empty list.
    #[must_use]
    pub fn parse(json: &str) -> Recent {
        serde_json::from_str(json).unwrap_or_default()
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// `doc` was opened: first in the list, once, at most [`MAX_RECENT`].
    pub fn touch(&mut self, doc: RecentDoc) {
        self.docs.retain(|d| d.path != doc.path);
        self.docs.insert(0, doc);
        self.docs.truncate(MAX_RECENT);
    }

    /// The page `path` was left at.
    pub fn set_page(&mut self, path: &str, page: usize) {
        if let Some(doc) = self.docs.iter_mut().find(|d| d.path == path) {
            doc.last_page = page;
        }
    }

    /// The entry of `path`.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&RecentDoc> {
        self.docs.iter().find(|d| d.path == path)
    }
}

// ==== Small parsers ====

/// A page number typed into the page field (1-based) as a page index,
/// clamped to the document; `None` for no number or no pages.
#[must_use]
pub fn parse_page_field(text: &str, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let n: usize = text.trim().parse().ok()?;
    Some(n.clamp(1, count) - 1)
}

/// A document's title from its path: the file name without `.pdf`.
#[must_use]
pub fn file_title(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".pdf") && name.len() > 4 {
        name[..name.len() - 4].to_string()
    } else {
        name.to_string()
    }
}

/// The bytes start like a PDF (`%PDF-` within the first 1024 bytes, as
/// readers accept).
#[must_use]
pub fn is_pdf_bytes(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    head.windows(5).any(|w| w == b"%PDF-")
}

/// The path names a PDF (by its extension).
#[must_use]
pub fn is_pdf_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".pdf")
}

// ==== The form ====

/// What a form field is (azul's `PdfFormFieldKind`, as the model needs it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    CheckBox,
    /// A radio group: one widget per button.
    Radio,
    /// A drop-down or a list: one choice of `options`.
    Choice,
    /// A push button or a signature field: nothing to type.
    Other,
}

/// Where a field is drawn: a page and a rect in points from the page's
/// top-left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldWidget {
    pub page: usize,
    /// x, y, width, height.
    pub rect: (f32, f32, f32, f32),
    /// A check box's / radio button's "on" state name.
    pub on_state: String,
}

/// One field of the PDF's form.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// The fully qualified name: what its value is saved under.
    pub name: String,
    pub kind: FieldKind,
    /// The value in the file.
    pub value: String,
    pub options: Vec<String>,
    pub widgets: Vec<FieldWidget>,
    pub read_only: bool,
    pub multiline: bool,
    pub password: bool,
    /// 0 = no limit.
    pub max_len: u32,
    /// Points; 0 = fit the field.
    pub font_size: f32,
}

impl Field {
    /// The font size in CSS px for `widget` at `px_per_pt` (the page's
    /// scale): the form's size, or one that fits the field's height.
    #[must_use]
    pub fn font_px(&self, widget: &FieldWidget, px_per_pt: f32) -> f32 {
        let pt = if self.font_size > 0.0 {
            self.font_size
        } else if self.multiline {
            12.0_f32.min((widget.rect.3 - 4.0).max(4.0))
        } else {
            ((widget.rect.3 - 4.0) * 0.75).clamp(4.0, 12.0)
        };
        pt * px_per_pt
    }

    /// The value that checks `widget` (a check box / radio button).
    #[must_use]
    pub fn on_value(widget: &FieldWidget) -> String {
        if widget.on_state.is_empty() {
            "Yes".to_string()
        } else {
            widget.on_state.clone()
        }
    }

    /// Whether `value` checks `widget`.
    #[must_use]
    pub fn is_checked(value: &str, widget: &FieldWidget) -> bool {
        !matches!(value, "" | "Off" | "false" | "0") && value == Self::on_value(widget)
    }
}

/// The form's values as the user has them, by field name, and the values in
/// the file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormValues {
    values: BTreeMap<String, String>,
    original: BTreeMap<String, String>,
}

impl FormValues {
    /// The values of `fields` as the file has them.
    #[must_use]
    pub fn new(fields: &[Field]) -> FormValues {
        let original: BTreeMap<String, String> = fields
            .iter()
            .map(|f| (f.name.clone(), f.value.clone()))
            .collect();
        FormValues {
            values: original.clone(),
            original,
        }
    }

    /// The value of field `name` (empty for an unknown one).
    #[must_use]
    pub fn get(&self, name: &str) -> &str {
        self.values.get(name).map_or("", String::as_str)
    }

    /// Sets field `name`'s value; `true` when that changed it.
    pub fn set(&mut self, name: &str, value: &str) -> bool {
        if self.get(name) == value {
            return false;
        }
        self.values.insert(name.to_string(), value.to_string());
        true
    }

    /// The values that differ from the file's, by name.
    #[must_use]
    pub fn changed(&self) -> Vec<(String, String)> {
        self.values
            .iter()
            .filter(|(name, value)| self.original.get(*name) != Some(*value))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    /// Every value, by name (what a filled copy is saved with).
    #[must_use]
    pub fn all(&self) -> Vec<(String, String)> {
        self.values
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    /// Whether the user changed anything.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.changed().is_empty()
    }
}

/// The name a filled copy of `title` is offered under.
#[must_use]
pub fn filled_name(title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        "filled.pdf".to_string()
    } else {
        format!("{title} (filled).pdf")
    }
}

// ==== The export switches (for the Chrome probe) ====

/// The width `--export-png` draws at without `--width` (Letter at 96 dpi).
pub const DEFAULT_EXPORT_WIDTH: u32 = 816;

/// What an export writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// The page drawn by azul (the viewer's render path).
    Png,
    /// The page's SVG (azul's PDF -> SVG).
    Svg,
    /// The whole PDF with its form filled from the `--set` values and
    /// flattened (the viewer's "Export filled PDF").
    Filled,
}

/// `--export-png OUT | --export-svg OUT [--page N] [--width W] FILE`: one page
/// written without a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportRequest {
    pub format: ExportFormat,
    pub out: String,
    /// 0-based (the switch is 1-based).
    pub page: usize,
    pub width: u32,
    pub file: String,
    /// `--set NAME=VALUE`: a form field's value (`--export-filled`).
    pub values: Vec<(String, String)>,
}

/// The export the command line asks for: `None` without an `--export-*`
/// switch (the window opens), `Some(Err(why))` for a malformed one.
#[must_use]
pub fn parse_export(args: &[String]) -> Option<Result<ExportRequest, String>> {
    if !args
        .iter()
        .any(|a| a == "--export-png" || a == "--export-svg" || a == "--export-filled")
    {
        return None;
    }
    let mut values = Vec::new();
    let mut format = ExportFormat::Png;
    let mut out = None;
    let mut page = 0;
    let mut width = DEFAULT_EXPORT_WIDTH;
    let mut file = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let value = args.get(i + 1);
        match arg {
            "--export-png" | "--export-svg" | "--export-filled" => {
                format = match arg {
                    "--export-png" => ExportFormat::Png,
                    "--export-svg" => ExportFormat::Svg,
                    _ => ExportFormat::Filled,
                };
                let Some(v) = value else {
                    return Some(Err(format!("{arg} needs an output file")));
                };
                out = Some(v.clone());
                i += 2;
            }
            "--page" => {
                let Some(n) = value.and_then(|v| v.parse::<usize>().ok()) else {
                    return Some(Err("--page needs a page number (1 = the first)".to_string()));
                };
                page = n.saturating_sub(1);
                i += 2;
            }
            "--set" => {
                let Some((name, value)) = value.and_then(|v| v.split_once('=')) else {
                    return Some(Err("--set needs NAME=VALUE (a form field and its value)".to_string()));
                };
                values.push((name.to_string(), value.to_string()));
                i += 2;
            }
            "--width" => {
                let Some(w) = value.and_then(|v| v.parse::<u32>().ok()).filter(|w| *w > 0) else {
                    return Some(Err("--width needs a width in pixels".to_string()));
                };
                width = w;
                i += 2;
            }
            _ => {
                file = Some(arg.to_string());
                i += 1;
            }
        }
    }
    match (out, file) {
        (Some(out), Some(file)) => Some(Ok(ExportRequest {
            format,
            out,
            page,
            width,
            file,
            values,
        })),
        _ => Some(Err(
            "usage: AzPdf --export-png OUT.png | --export-svg OUT.svg \
                       [--page N] [--width W] FILE.pdf | --export-filled OUT.pdf \
                       [--set NAME=VALUE]... FILE.pdf"
                .to_string(),
        )),
    }
}
