//! Interactive forms (AcroForm).
//!
//! A PDF form is a tree of *fields* (a text box, a check box, a drop-down, ...) listed under
//! the `/AcroForm` entry of the document catalog. Each field has a name, a value and one or
//! more *widgets*: annotations that say on which page, and in which rectangle, the field is
//! drawn. A widget shows its field through an *appearance stream*, a small content stream stored
//! with the widget. Viewers draw that stream, so a changed value is only visible if the
//! appearance changes with it.
//!
//! * [`parse_form_fields`] reads the fields of a PDF: kind, fully qualified name
//!   (`parent.child`), value, options and widgets.
//! * [`fill_form`] sets values, writes a new appearance stream for each changed widget and
//!   returns the new PDF. With `flatten` it instead draws every widget's appearance into its
//!   page and removes the widgets and the form, which leaves a plain PDF that shows the values.
//!   It can also draw [`FormStamp`]s (vector drawings, for example a signature) onto pages.
//! * [`fields_svg`] draws the values of parsed fields as SVG, for showing a form on a page
//!   that was rendered without its widgets.
//!
//! This module works on the bytes of the PDF with `lopdf`, not on [`PdfDocument`](crate::PdfDocument):
//! parsing a PDF into a `PdfDocument` drops the form, and saving it writes the document
//! from scratch. Here every object that is not changed is written back as it was.
//!
//! Page numbers in this module start at 0 (unlike the 1-based pages of the
//! [`annotation`](crate::annotation) module), and rectangles are `[llx, lly, urx, ury]` in
//! points from the bottom left corner of the page.
//!
//! Limits: generated appearances use the built-in Helvetica font with WinAnsiEncoding, and
//! characters outside Windows-1252 are drawn as `?`. A value is written as given: it is not
//! checked against `read_only`, `max_len` or a choice's options. Push buttons and signature
//! fields are not given a value or a generated appearance, and signing is not supported. Only
//! fields listed under `/AcroForm /Fields` are found; XFA form data is ignored.
//!
//! # Examples
//!
//! ```no_run
//! use printpdf::forms::{fill_form, parse_form_fields, FieldValue};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let bytes = std::fs::read("form.pdf")?;
//! for field in parse_form_fields(&bytes)? {
//!     println!("{} = {:?}", field.name, field.value);
//! }
//! let values = [FieldValue { name: "person.name".into(), value: "Grace".into() }];
//! // `false`: keep the form editable. Pass `true` to flatten it into the pages.
//! let filled = fill_form(&bytes, &values, &[], false)?;
//! std::fs::write("filled.pdf", filled)?;
//! # Ok(())
//! # }
//! ```

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

/// The kind of a form field, from its `/FT` entry and field flags (ISO 32000-1, 12.7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormFieldKind {
    /// Text box (`/FT /Tx`).
    Text,
    /// Check box (`/FT /Btn`, neither radio nor push button).
    CheckBox,
    /// Group of radio buttons (`/FT /Btn` with the Radio flag): one field with one widget per
    /// button. The value is the state name of the chosen button.
    RadioButton,
    /// Drop-down list (`/FT /Ch` with the Combo flag).
    ComboBox,
    /// List box (`/FT /Ch` without the Combo flag).
    ListBox,
    /// Push button (`/FT /Btn` with the Pushbutton flag). It has no value.
    PushButton,
    /// Signature field (`/FT /Sig`).
    Signature,
}

/// Where a field is drawn: one widget annotation of a field.
#[derive(Debug, Clone, PartialEq)]
pub struct FormWidget {
    /// Page the widget is on, starting at 0.
    pub page: usize,
    /// Bounds `[llx, lly, urx, ury]` in points, origin at the bottom left of the page.
    pub rect: [f32; 4],
    /// Name of the "on" state of a check box or radio button (`Yes`, `Choice1`), taken from its
    /// appearance dictionary. Empty for other fields and for widgets without appearances.
    pub on_state: String,
    /// The annotation has the Hidden or NoView flag, so viewers do not show it.
    pub hidden: bool,
}

/// A form field with its value and widgets, as read by [`parse_form_fields`].
#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    /// Fully qualified name (`parent.child`), the partial names of the field and its ancestors
    /// joined with dots. [`FieldValue::name`] refers to a field by this name.
    pub name: String,
    /// What kind of field this is.
    pub kind: FormFieldKind,
    /// Current value: the text of a text field, the display text of a choice, or the state
    /// name of a check box or radio group (`Off` when unchecked). Empty when unset.
    pub value: String,
    /// Default value (`/DV`), in the same form as `value`.
    pub default_value: String,
    /// Display texts of a choice's options; the on-state names of a radio group.
    pub options: Vec<String>,
    /// The places where the field is drawn.
    pub widgets: Vec<FormWidget>,
    /// The ReadOnly flag is set. [`fill_form`] does not check it.
    pub read_only: bool,
    /// The Required flag is set.
    pub required: bool,
    /// Text field with several lines (the Multiline flag).
    pub multiline: bool,
    /// Text field that shows asterisks instead of its text (the Password flag).
    pub password: bool,
    /// Maximum length of a text field in characters, `None` for no limit. [`fill_form`] does
    /// not check it.
    pub max_len: Option<u32>,
    /// Font size in points from the default appearance string (`/DA`); `0.0` selects the size
    /// automatically.
    pub font_size: f32,
    /// Text alignment from `/Q`: 0 left, 1 centered, 2 right.
    pub alignment: u8,
}

impl FormField {
    /// Whether the value of this check box or radio group means "checked" for `widget`.
    #[must_use]
    pub fn is_on(&self, widget: &FormWidget) -> bool {
        is_on_value(&self.value, widget)
    }
}

fn is_on_value(value: &str, widget: &FormWidget) -> bool {
    match value {
        "" | "Off" | "false" | "0" => false,
        "true" | "on" | "On" | "Yes" if widget.on_state.is_empty() => true,
        v => v == widget.on_state || (v == "true" && !widget.on_state.is_empty()),
    }
}

/// A value to set with [`fill_form`].
///
/// `value` is a text for a text field, the display text for a choice, and for a check box or
/// radio group the name of the on-state (`Yes`; `true` also works) or `Off` / `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldValue {
    /// Fully qualified name of the field, see [`FormField::name`].
    pub name: String,
    /// The new value.
    pub value: String,
}

/// One segment of a stamp's path, in the stamp's own coordinates (like SVG: y points down,
/// inside the stamp's `view_box`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StampSegment {
    /// Starts a new subpath at `(x, y)` (`m`).
    MoveTo(f32, f32),
    /// Straight line to `(x, y)` (`l`).
    LineTo(f32, f32),
    /// Cubic Bezier curve: two control points, then the end point (`c`).
    CubicTo(f32, f32, f32, f32, f32, f32),
    /// Closes the subpath with a straight line to its start (`h`).
    Close,
}

/// A path of a stamp, stroked, filled or both. Colors are RGB with components in `0.0..=1.0`.
/// A path with neither stroke nor fill is not drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct StampPath {
    /// The path, in view-box coordinates.
    pub segments: Vec<StampSegment>,
    /// Line width in view-box units and RGB color, `None` for no outline.
    pub stroke: Option<(f32, [f32; 3])>,
    /// RGB fill color, `None` for no fill.
    pub fill: Option<[f32; 3]>,
}

/// A vector drawing put on a page by [`fill_form`], for example a handwritten signature.
#[derive(Debug, Clone, PartialEq)]
pub struct FormStamp {
    /// Page to draw on, starting at 0. A page that does not exist is ignored.
    pub page: usize,
    /// Where the drawing goes on the page: `[llx, lly, urx, ury]` in points from the bottom
    /// left corner.
    pub rect: [f32; 4],
    /// Coordinate system of the paths: `[x, y, width, height]`, y down. It is scaled to fill
    /// `rect`. Nothing is drawn if its width or height is not positive.
    pub view_box: [f32; 4],
    /// The paths of the drawing, drawn in order.
    pub paths: Vec<StampPath>,
}

/// Annotation flags (`/F`): Hidden (bit 2) or NoView (bit 6).
const ANNOT_HIDDEN: i64 = 1 << 1;
const ANNOT_NO_VIEW: i64 = 1 << 5;
/// Field flags (`/Ff`), PDF 32000 12.7.4.
const FF_READ_ONLY: i64 = 1;
const FF_REQUIRED: i64 = 1 << 1;
const FF_MULTILINE: i64 = 1 << 12;
const FF_PASSWORD: i64 = 1 << 13;
const FF_RADIO: i64 = 1 << 15;
const FF_PUSHBUTTON: i64 = 1 << 16;
const FF_COMBO: i64 = 1 << 17;

/// What a field inherits from its ancestors (PDF 32000 12.7.3.1).
#[derive(Clone, Default)]
struct Inherited {
    ft: Option<Vec<u8>>,
    ff: i64,
    v: Option<Object>,
    dv: Option<Object>,
    da: Option<Vec<u8>>,
    q: i64,
    opt: Option<Object>,
    max_len: Option<i64>,
}

/// A terminal field as found in the document: its dictionary's id, the
/// widgets' ids and what it inherited.
struct FoundField {
    id: ObjectId,
    name: String,
    widget_ids: Vec<ObjectId>,
    inherited: Inherited,
}

/// Reads the form fields of the PDF `bytes`, in document order. A PDF without a form gives an
/// empty list.
///
/// A field with an unknown or missing `/FT` entry is left out.
///
/// # Errors
///
/// Returns an error when the bytes do not load as a PDF.
pub fn parse_form_fields(bytes: &[u8]) -> Result<Vec<FormField>, String> {
    let doc = Document::load_mem(bytes).map_err(|e| format!("not a PDF: {e}"))?;
    Ok(form_fields(&doc))
}

fn form_fields(doc: &Document) -> Vec<FormField> {
    let pages = page_index_of(doc);
    find_fields(doc)
        .into_iter()
        .filter_map(|found| field_of(doc, &found, &pages))
        .collect()
}

/// Page object id -> 0-based index.
fn page_index_of(doc: &Document) -> BTreeMap<ObjectId, usize> {
    crate::deserialize::numbered_pages(doc)
        .into_iter()
        .map(|(num, id)| (id, num.saturating_sub(1)))
        .collect()
}

fn dict_of<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    doc.dereference(object).ok()?.1.as_dict().ok()
}

/// The terminal fields under `/AcroForm /Fields`.
fn find_fields(doc: &Document) -> Vec<FoundField> {
    let Some(fields) = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|form| dict_of(doc, form))
        .and_then(|form| form.get(b"Fields").ok())
        .and_then(|fields| doc.dereference(fields).ok())
        .and_then(|(_, fields)| fields.as_array().ok())
    else {
        return Vec::new();
    };
    let da = doc
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .and_then(|form| dict_of(doc, form))
        .and_then(|form| form.get(b"DA").ok())
        .and_then(|da| da.as_str().ok())
        .map(<[u8]>::to_vec);
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for field in fields {
        if let Ok(id) = field.as_reference() {
            let inherited = Inherited {
                da: da.clone(),
                ..Inherited::default()
            };
            walk_field(doc, id, "", &inherited, &mut out, &mut seen, 0);
        }
    }
    out
}

fn walk_field(
    doc: &Document,
    id: ObjectId,
    parent_name: &str,
    parent: &Inherited,
    out: &mut Vec<FoundField>,
    seen: &mut BTreeSet<ObjectId>,
    depth: usize,
) {
    if depth > 32 || !seen.insert(id) {
        return;
    }
    let Ok(dict) = doc.get_dictionary(id) else {
        return;
    };
    let mut inherited = parent.clone();
    if let Ok(ft) = dict.get(b"FT").and_then(Object::as_name) {
        inherited.ft = Some(ft.to_vec());
    }
    if let Ok(ff) = dict.get(b"Ff").and_then(Object::as_i64) {
        inherited.ff = ff;
    }
    if let Ok(v) = dict.get(b"V") {
        inherited.v = Some(v.clone());
    }
    if let Ok(dv) = dict.get(b"DV") {
        inherited.dv = Some(dv.clone());
    }
    if let Ok(da) = dict.get(b"DA").and_then(Object::as_str) {
        inherited.da = Some(da.to_vec());
    }
    if let Ok(q) = dict.get(b"Q").and_then(Object::as_i64) {
        inherited.q = q;
    }
    if let Ok(opt) = dict.get(b"Opt") {
        inherited.opt = Some(opt.clone());
    }
    if let Ok(max_len) = dict.get(b"MaxLen").and_then(Object::as_i64) {
        inherited.max_len = Some(max_len);
    }
    let partial = dict
        .get(b"T")
        .and_then(Object::as_str)
        .map(decode_text_string)
        .unwrap_or_default();
    let name = match (parent_name.is_empty(), partial.is_empty()) {
        (_, true) => parent_name.to_string(),
        (true, false) => partial,
        (false, false) => format!("{parent_name}.{partial}"),
    };
    let kids: Vec<ObjectId> = dict
        .get(b"Kids")
        .ok()
        .and_then(|kids| doc.dereference(kids).ok())
        .and_then(|(_, kids)| kids.as_array().ok())
        .map(|kids| kids.iter().filter_map(|k| k.as_reference().ok()).collect())
        .unwrap_or_default();
    // A kid with a /T is a field of its own; one without is a widget.
    let (child_fields, widgets): (Vec<ObjectId>, Vec<ObjectId>) = kids.into_iter().partition(|kid| {
        doc.get_dictionary(*kid)
            .is_ok_and(|kid| kid.has(b"T"))
    });
    for child in child_fields {
        walk_field(doc, child, &name, &inherited, out, seen, depth + 1);
    }
    let is_widget = dict
        .get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|s| s == b"Widget");
    let widget_ids = if widgets.is_empty() && is_widget {
        vec![id]
    } else {
        widgets
    };
    if !widget_ids.is_empty() || (inherited.ft.is_some() && dict.has(b"T")) {
        out.push(FoundField {
            id,
            name,
            widget_ids,
            inherited,
        });
    }
}

fn field_of(
    doc: &Document,
    found: &FoundField,
    pages: &BTreeMap<ObjectId, usize>,
) -> Option<FormField> {
    let inherited = &found.inherited;
    let ff = inherited.ff;
    let kind = match inherited.ft.as_deref()? {
        b"Tx" => FormFieldKind::Text,
        b"Btn" if ff & FF_PUSHBUTTON != 0 => FormFieldKind::PushButton,
        b"Btn" if ff & FF_RADIO != 0 => FormFieldKind::RadioButton,
        b"Btn" => FormFieldKind::CheckBox,
        b"Ch" if ff & FF_COMBO != 0 => FormFieldKind::ComboBox,
        b"Ch" => FormFieldKind::ListBox,
        b"Sig" => FormFieldKind::Signature,
        _ => return None,
    };
    let widgets: Vec<FormWidget> = found
        .widget_ids
        .iter()
        .filter_map(|id| widget_of(doc, *id, pages))
        .collect();
    let options = match kind {
        FormFieldKind::RadioButton => widgets
            .iter()
            .map(|w| w.on_state.clone())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => inherited
            .opt
            .as_ref()
            .and_then(|opt| doc.dereference(opt).ok())
            .and_then(|(_, opt)| opt.as_array().ok())
            .map(|opt| opt.iter().map(|o| option_text(doc, o)).collect())
            .unwrap_or_default(),
    };
    Some(FormField {
        name: found.name.clone(),
        kind,
        value: inherited.v.as_ref().map(|v| value_text(doc, v)).unwrap_or_default(),
        default_value: inherited
            .dv
            .as_ref()
            .map(|v| value_text(doc, v))
            .unwrap_or_default(),
        options,
        widgets,
        read_only: ff & FF_READ_ONLY != 0,
        required: ff & FF_REQUIRED != 0,
        multiline: kind == FormFieldKind::Text && ff & FF_MULTILINE != 0,
        password: kind == FormFieldKind::Text && ff & FF_PASSWORD != 0,
        max_len: inherited.max_len.and_then(|m| u32::try_from(m).ok()),
        font_size: inherited
            .da
            .as_deref()
            .and_then(da_font_size)
            .unwrap_or(0.0),
        alignment: u8::try_from(inherited.q.clamp(0, 2)).unwrap_or(0),
    })
}

fn widget_of(doc: &Document, id: ObjectId, pages: &BTreeMap<ObjectId, usize>) -> Option<FormWidget> {
    let dict = doc.get_dictionary(id).ok()?;
    let rect = rect_of(doc, dict.get(b"Rect").ok()?)?;
    let page = dict
        .get(b"P")
        .and_then(Object::as_reference)
        .ok()
        .and_then(|p| pages.get(&p).copied())
        .or_else(|| page_holding(doc, id, pages))?;
    let on_state = dict
        .get(b"AP")
        .ok()
        .and_then(|ap| dict_of(doc, ap))
        .and_then(|ap| ap.get(b"N").ok())
        .and_then(|n| dict_of(doc, n))
        .and_then(|states| {
            states
                .iter()
                .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
                .find(|k| k != "Off")
        })
        .unwrap_or_default();
    let flags = dict.get(b"F").and_then(Object::as_i64).unwrap_or(0);
    Some(FormWidget {
        page,
        rect,
        on_state,
        hidden: flags & (ANNOT_HIDDEN | ANNOT_NO_VIEW) != 0,
    })
}

/// The page whose `/Annots` lists `widget` (a widget without `/P`).
fn page_holding(doc: &Document, widget: ObjectId, pages: &BTreeMap<ObjectId, usize>) -> Option<usize> {
    pages.iter().find_map(|(page_id, index)| {
        let annots = doc.get_dictionary(*page_id).ok()?.get(b"Annots").ok()?;
        let annots = doc.dereference(annots).ok()?.1.as_array().ok()?;
        annots
            .iter()
            .any(|a| a.as_reference().ok() == Some(widget))
            .then_some(*index)
    })
}

fn number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

fn rect_of(doc: &Document, object: &Object) -> Option<[f32; 4]> {
    let array = doc.dereference(object).ok()?.1.as_array().ok()?;
    let v: Vec<f32> = array.iter().filter_map(number).collect();
    let [x0, y0, x1, y1] = <[f32; 4]>::try_from(v).ok()?;
    Some([x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)])
}

/// Decodes a PDF text string (ISO 32000-1, 7.9.2.2): UTF-16BE or UTF-8 when it starts with a
/// byte order mark, else PDFDocEncoding, which is read as Latin-1.
#[must_use]
pub fn decode_text_string(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = utf16
            .chunks_exact(2)
            .filter_map(|c| <[u8; 2]>::try_from(c).ok())
            .map(u16::from_be_bytes)
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(utf8) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(utf8).into_owned();
    }
    bytes.iter().map(|b| char::from(*b)).collect()
}

/// `text` as a PDF text string: the bytes as they are when ASCII, else
/// UTF-16BE with a BOM.
fn encode_text_string(text: &str) -> Vec<u8> {
    if text.is_ascii() {
        return text.as_bytes().to_vec();
    }
    let mut out = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

// How deeply the arrays of a value may nest, and how many objects one value may visit. A real
// value is a string, a name or an array of them; the limits stop arrays that hold themselves
// and arrays that fan out into one another.
const MAX_VALUE_DEPTH: usize = 8;
const MAX_VALUE_OBJECTS: usize = 4096;

fn value_text(doc: &Document, value: &Object) -> String {
    let mut budget = MAX_VALUE_OBJECTS;
    value_text_within(doc, value, MAX_VALUE_DEPTH, &mut budget)
}

// `value_text` with `depth` more levels of arrays allowed and `budget` more objects to visit.
fn value_text_within(doc: &Document, value: &Object, depth: usize, budget: &mut usize) -> String {
    let Some(depth) = depth.checked_sub(1) else {
        return String::new();
    };
    if *budget == 0 {
        return String::new();
    }
    *budget = budget.saturating_sub(1);
    let Ok((_, value)) = doc.dereference(value) else {
        return String::new();
    };
    match value {
        Object::String(bytes, _) => decode_text_string(bytes),
        Object::Name(name) => String::from_utf8_lossy(name).into_owned(),
        Object::Array(items) => items
            .iter()
            .map(|item| value_text_within(doc, item, depth, budget))
            .collect::<Vec<_>>()
            .join(", "),
        Object::Stream(stream) => decode_text_string(&stream.content),
        _ => String::new(),
    }
}

/// A choice option: a text, or `[export value, display text]`.
fn option_text(doc: &Document, option: &Object) -> String {
    match doc.dereference(option).map(|(_, o)| o) {
        Ok(Object::Array(pair)) => pair.get(1).or(pair.first()).map_or_else(String::new, |o| value_text(doc, o)),
        Ok(other) => value_text(doc, other),
        Err(_) => String::new(),
    }
}

/// The font size of a default appearance string (`/Helv 12 Tf 0 g`).
fn da_font_size(da: &[u8]) -> Option<f32> {
    let da = String::from_utf8_lossy(da);
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let tf = tokens.iter().position(|t| *t == "Tf")?;
    tokens.get(tf.checked_sub(1)?)?.parse().ok()
}

/// The colour operator of a default appearance string (`0 g`, `1 0 0 rg`),
/// black when it has none.
fn da_colour(da: &[u8]) -> String {
    let da = String::from_utf8_lossy(da);
    let tokens: Vec<&str> = da.split_whitespace().collect();
    for (i, t) in tokens.iter().enumerate() {
        let n = match *t {
            "g" => 1,
            "rg" => 3,
            "k" => 4,
            _ => continue,
        };
        if let Some(operands) = i.checked_sub(n).and_then(|first| tokens.get(first..=i)) {
            return operands.join(" ");
        }
    }
    "0 g".to_string()
}

/// Helvetica's advance widths for the printable ASCII range (AFM, per 1000
/// units); a character outside it counts as 556.
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // ' '..'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // '0'..'?'
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // '@'..'O'
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 'P'..'_'
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // '`'..'o'
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 'p'..'~'
];

fn helvetica_width(text: &str, size: f32) -> f32 {
    let units: u64 = text
        .chars()
        .map(|c| {
            (c as u32)
                .checked_sub(0x20)
                .and_then(|index| HELVETICA_WIDTHS.get(index as usize))
                .map_or(556, |width| u64::from(*width))
        })
        .sum();
    units as f32 * size / 1000.0
}

/// `c` in Windows-1252 (WinAnsiEncoding), `?` when it has no code there.
fn win_ansi(c: char) -> u8 {
    match c as u32 {
        0x20..=0x7E | 0xA0..=0xFF => c as u32 as u8,
        0x20AC => 0x80,
        0x201A => 0x82,
        0x0192 => 0x83,
        0x201E => 0x84,
        0x2026 => 0x85,
        0x2020 => 0x86,
        0x2021 => 0x87,
        0x02C6 => 0x88,
        0x2030 => 0x89,
        0x0160 => 0x8A,
        0x2039 => 0x8B,
        0x0152 => 0x8C,
        0x017D => 0x8E,
        0x2018 => 0x91,
        0x2019 => 0x92,
        0x201C => 0x93,
        0x201D => 0x94,
        0x2022 => 0x95,
        0x2013 => 0x96,
        0x2014 => 0x97,
        0x02DC => 0x98,
        0x2122 => 0x99,
        0x0161 => 0x9A,
        0x203A => 0x9B,
        0x0153 => 0x9C,
        0x017E => 0x9E,
        0x0178 => 0x9F,
        _ => b'?',
    }
}

/// `text` as a literal string of a content stream, in WinAnsiEncoding.
fn pdf_literal(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in text.chars() {
        let b = win_ansi(c);
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

/// The name a generated appearance's font has in its resources.
const APPEARANCE_FONT: &[u8] = b"AzHelv";

/// The standard Helvetica, WinAnsiEncoding.
fn helvetica() -> Dictionary {
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    font
}

/// The content of a field's appearance, `w` x `h` points: its text (a text
/// field or a choice) or its check mark / dot (a check box / radio button).
fn appearance_content(field: &FormField, widget: &FormWidget, da: &[u8], w: f32, h: f32) -> Vec<u8> {
    let mut out = Vec::new();
    match field.kind {
        FormFieldKind::CheckBox | FormFieldKind::RadioButton => {
            if !field.is_on(widget) {
                return out;
            }
            let s = w.min(h);
            let (cx, cy) = (w / 2.0, h / 2.0);
            if field.kind == FormFieldKind::RadioButton {
                // A filled dot: four cubic arcs.
                let r = s * 0.3;
                let k = r * 0.552_284_8;
                out.extend_from_slice(
                    format!(
                        "q {colour} {x0} {cy} m {x0} {a} {b} {y1} {cx} {y1} c {c} {y1} {x1} {a} {x1} {cy} c \
                         {x1} {d} {c} {y0} {cx} {y0} c {b} {y0} {x0} {d} {x0} {cy} c f Q\n",
                        colour = da_colour(da),
                        x0 = cx - r,
                        x1 = cx + r,
                        y0 = cy - r,
                        y1 = cy + r,
                        a = cy + k,
                        d = cy - k,
                        b = cx - k,
                        c = cx + k,
                    )
                    .as_bytes(),
                );
            } else {
                // A check mark.
                let colour = da_colour(da).replace(" g", " G").replace(" rg", " RG").replace(" k", " K");
                out.extend_from_slice(
                    format!(
                        "q {colour} {lw} w 1 J 1 j {x0} {y0} m {x1} {y1} l {x2} {y2} l S Q\n",
                        lw = s * 0.12,
                        x0 = cx - s * 0.3,
                        y0 = cy,
                        x1 = cx - s * 0.08,
                        y1 = cy - s * 0.25,
                        x2 = cx + s * 0.32,
                        y2 = cy + s * 0.28,
                    )
                    .as_bytes(),
                );
            }
        }
        FormFieldKind::Text | FormFieldKind::ComboBox | FormFieldKind::ListBox => {
            let text = if field.password {
                "*".repeat(field.value.chars().count())
            } else {
                field.value.clone()
            };
            if text.is_empty() {
                return out;
            }
            let lines: Vec<&str> = if field.multiline {
                text.split('\n').collect()
            } else {
                vec![text.lines().next().unwrap_or("")]
            };
            let size = if field.font_size > 0.0 {
                field.font_size
            } else if field.multiline {
                12.0_f32.min((h - 4.0).max(4.0))
            } else {
                // Auto: as big as fits the height (and 12 pt at most).
                ((h - 4.0) * 0.75).clamp(4.0, 12.0)
            };
            let line_height = size * 1.15;
            out.extend_from_slice(b"/Tx BMC q 1 1 ");
            out.extend_from_slice(format!("{} {} re W n BT ", (w - 2.0).max(0.0), (h - 2.0).max(0.0)).as_bytes());
            out.push(b'/');
            out.extend_from_slice(APPEARANCE_FONT);
            out.extend_from_slice(format!(" {size} Tf {} ", da_colour(da)).as_bytes());
            for (i, line) in lines.iter().enumerate() {
                let width = helvetica_width(line, size);
                let x = match field.alignment {
                    1 => (w - width) / 2.0,
                    2 => w - 2.0 - width,
                    _ => 2.0,
                };
                let y = if field.multiline {
                    h - 2.0 - size - i as f32 * line_height
                } else {
                    // The cap height centred: Helvetica's ascent ~0.72 em.
                    (h - size * 0.72) / 2.0
                };
                out.extend_from_slice(format!("1 0 0 1 {x} {y} Tm ").as_bytes());
                out.extend_from_slice(&pdf_literal(line));
                out.extend_from_slice(b" Tj ");
            }
            out.extend_from_slice(b"ET Q EMC\n");
        }
        FormFieldKind::PushButton | FormFieldKind::Signature => {}
    }
    out
}

/// Draws the values of `fields` on `page` (starting at 0) as SVG elements, for showing a form
/// on a page that was rendered without its widgets.
///
/// The coordinates are those of an SVG page `page_height` points tall (y down). The result has
/// the same look as what [`fill_form`] draws: check marks, radio dots and text in Helvetica.
/// Widgets of other pages, hidden widgets and empty values draw nothing.
#[must_use]
pub fn fields_svg(fields: &[FormField], page: usize, page_height: f32) -> String {
    let mut svg = String::new();
    for field in fields {
        for widget in field.widgets.iter().filter(|w| w.page == page && !w.hidden) {
            let [llx, lly, urx, ury] = widget.rect;
            let (w, h) = (urx - llx, ury - lly);
            let top = page_height - ury;
            match field.kind {
                FormFieldKind::CheckBox | FormFieldKind::RadioButton if field.is_on(widget) => {
                    let s = w.min(h);
                    let (cx, cy) = (llx + w / 2.0, top + h / 2.0);
                    if field.kind == FormFieldKind::RadioButton {
                        svg.push_str(&format!(
                            r#"<circle cx="{cx}" cy="{cy}" r="{}" fill="rgb(0, 0, 0)" />"#,
                            s * 0.3
                        ));
                    } else {
                        svg.push_str(&format!(
                            r#"<path d="M{},{} L{},{} L{},{}" fill="none" stroke="rgb(0, 0, 0)" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round" />"#,
                            cx - s * 0.3,
                            cy,
                            cx - s * 0.08,
                            cy + s * 0.25,
                            cx + s * 0.32,
                            cy - s * 0.28,
                            s * 0.12
                        ));
                    }
                }
                FormFieldKind::Text | FormFieldKind::ComboBox | FormFieldKind::ListBox
                    if !field.value.is_empty() =>
                {
                    let text = if field.password {
                        "*".repeat(field.value.chars().count())
                    } else {
                        field.value.clone()
                    };
                    let size = if field.font_size > 0.0 {
                        field.font_size
                    } else {
                        ((h - 4.0) * 0.75).clamp(4.0, 12.0)
                    };
                    let lines: Vec<&str> = if field.multiline {
                        text.split('\n').collect()
                    } else {
                        vec![text.lines().next().unwrap_or("")]
                    };
                    for (i, line) in lines.iter().enumerate() {
                        let width = helvetica_width(line, size);
                        let x = llx
                            + match field.alignment {
                                1 => (w - width) / 2.0,
                                2 => w - 2.0 - width,
                                _ => 2.0,
                            };
                        let baseline = if field.multiline {
                            top + 2.0 + size + i as f32 * size * 1.15
                        } else {
                            top + h - (h - size * 0.72) / 2.0
                        };
                        svg.push_str(&format!(
                            r#"<text x="{x}" y="{baseline}" font-family="Helvetica, Arial, sans-serif" font-size="{size}" fill="rgb(0, 0, 0)">{}</text>"#,
                            xml_escape(line)
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    svg
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Sets form field values and draws stamps, and returns the new PDF.
///
/// Each field named in `values` gets its value and a new appearance stream for every widget,
/// and `/NeedAppearances` is set so that viewers may regenerate them. A name that the form does
/// not have is ignored. `stamps` are drawn on top of the page content.
///
/// With `flatten`, every visible widget (not only the filled ones) is drawn into its page,
/// using its own appearance or a generated one, and the widgets and the `/AcroForm` entry are
/// removed, so the result has no form fields left. Hidden widgets are not drawn.
///
/// # Errors
///
/// Returns an error when the bytes do not load as a PDF or the result cannot be written.
pub fn fill_form(
    bytes: &[u8],
    values: &[FieldValue],
    stamps: &[FormStamp],
    flatten: bool,
) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(bytes).map_err(|e| format!("not a PDF: {e}"))?;
    let pages = page_index_of(&doc);
    let page_ids: BTreeMap<usize, ObjectId> = pages.iter().map(|(id, i)| (*i, *id)).collect();
    let found = find_fields(&doc);
    let mut fields: Vec<(FoundField, FormField)> = found
        .into_iter()
        .filter_map(|f| {
            let field = field_of(&doc, &f, &pages)?;
            Some((f, field))
        })
        .collect();
    let wanted: BTreeMap<&str, &str> = values
        .iter()
        .map(|v| (v.name.as_str(), v.value.as_str()))
        .collect();

    for (found, field) in &mut fields {
        let Some(value) = wanted.get(field.name.as_str()) else {
            continue;
        };
        set_value(&mut doc, found, field, value);
    }

    let mut page_content: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    let mut page_xobjects: BTreeMap<usize, Vec<(Vec<u8>, ObjectId)>> = BTreeMap::new();
    if flatten {
        let mut n = 0usize;
        for (found, field) in &fields {
            for (widget_id, widget) in found.widget_ids.iter().zip(&field.widgets) {
                if widget.hidden {
                    continue;
                }
                // A widget without an appearance (the viewer was to make
                // one: /NeedAppearances) gets ours.
                let ap = match normal_appearance(&doc, *widget_id) {
                    Some(ap) => ap,
                    None => {
                        let da = found
                            .inherited
                            .da
                            .clone()
                            .unwrap_or_else(|| b"/Helv 0 Tf 0 g".to_vec());
                        add_appearance(&mut doc, field, widget, &da)
                    }
                };
                let Some(placement) = appearance_placement(&doc, ap, widget.rect) else {
                    continue;
                };
                n += 1;
                let name = format!("AzFlat{n}").into_bytes();
                let content = page_content.entry(widget.page).or_default();
                content.extend_from_slice(format!("q {placement} cm /").as_bytes());
                content.extend_from_slice(&name);
                content.extend_from_slice(b" Do Q\n");
                page_xobjects.entry(widget.page).or_default().push((name, ap));
            }
        }
    }

    for stamp in stamps {
        page_content
            .entry(stamp.page)
            .or_default()
            .extend_from_slice(&stamp_content(stamp));
    }

    for (page, content) in &page_content {
        let Some(page_id) = page_ids.get(page).copied() else {
            continue;
        };
        let xobjects = page_xobjects.remove(page).unwrap_or_default();
        append_page_content(&mut doc, page_id, content.clone(), &xobjects);
    }

    if flatten {
        let widget_ids: BTreeSet<ObjectId> = fields
            .iter()
            .flat_map(|(found, _)| found.widget_ids.iter().copied())
            .collect();
        for page_id in page_ids.values() {
            remove_widgets(&mut doc, *page_id, &widget_ids);
        }
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.remove(b"AcroForm");
        }
    } else if !values.is_empty() {
        set_need_appearances(&mut doc);
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(|e| format!("the PDF could not be written: {e}"))?;
    Ok(out)
}

/// Writes `value` into the field (`/V`, a widget's `/AS`) and gives each of
/// its widgets an appearance that shows it.
fn set_value(doc: &mut Document, found: &FoundField, field: &mut FormField, value: &str) {
    field.value = value.to_string();
    let v = match field.kind {
        FormFieldKind::CheckBox | FormFieldKind::RadioButton => {
            let on = field
                .widgets
                .iter()
                .find(|w| is_on_value(value, w))
                .map(|w| if w.on_state.is_empty() { "Yes".to_string() } else { w.on_state.clone() });
            field.value = on.clone().unwrap_or_else(|| "Off".to_string());
            Object::Name(field.value.clone().into_bytes())
        }
        FormFieldKind::PushButton | FormFieldKind::Signature => return,
        _ => Object::String(encode_text_string(value), StringFormat::Literal),
    };
    if let Ok(dict) = doc.get_object_mut(found.id).and_then(Object::as_dict_mut) {
        dict.set("V", v);
    }
    let da = found.inherited.da.clone().unwrap_or_else(|| b"/Helv 0 Tf 0 g".to_vec());
    for (widget_id, widget) in found.widget_ids.iter().zip(field.widgets.clone()) {
        match field.kind {
            FormFieldKind::CheckBox | FormFieldKind::RadioButton => {
                let on = field.is_on(&widget);
                let state = if on {
                    if widget.on_state.is_empty() {
                        "Yes".to_string()
                    } else {
                        widget.on_state.clone()
                    }
                } else {
                    "Off".to_string()
                };
                let has_own_appearance = !widget.on_state.is_empty();
                if let Ok(dict) = doc.get_object_mut(*widget_id).and_then(Object::as_dict_mut) {
                    dict.set("AS", Object::Name(state.into_bytes()));
                }
                // A check box drawn by the viewer from its own /AP keeps it;
                // one without gets ours.
                if !has_own_appearance {
                    let ap = add_appearance(doc, field, &widget, &da);
                    set_normal_appearance(doc, *widget_id, ap, on);
                }
            }
            _ => {
                let ap = add_appearance(doc, field, &widget, &da);
                set_normal_appearance(doc, *widget_id, ap, false);
            }
        }
    }
}

/// A new appearance stream (a Form XObject) for `widget` of `field`.
fn add_appearance(doc: &mut Document, field: &FormField, widget: &FormWidget, da: &[u8]) -> ObjectId {
    let [llx, lly, urx, ury] = widget.rect;
    let (w, h) = ((urx - llx).max(0.0), (ury - lly).max(0.0));
    let mut fonts = Dictionary::new();
    fonts.set(APPEARANCE_FONT.to_vec(), Object::Dictionary(helvetica()));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Form".to_vec()));
    dict.set(
        "BBox",
        Object::Array(vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)]),
    );
    dict.set("Resources", Object::Dictionary(resources));
    doc.add_object(Stream::new(dict, appearance_content(field, widget, da, w, h)))
}

/// Makes `ap` the widget's normal appearance: `/AP /N` (a check box's
/// on-state `/AP /N /Yes` when `on_state`, its off-state otherwise).
fn set_normal_appearance(doc: &mut Document, widget: ObjectId, ap: ObjectId, on_state: bool) {
    let Ok(dict) = doc.get_object_mut(widget).and_then(Object::as_dict_mut) else {
        return;
    };
    let mut appearances = Dictionary::new();
    let field_is_button = dict.has(b"AS");
    if field_is_button {
        let mut states = Dictionary::new();
        states.set(if on_state { "Yes" } else { "Off" }, Object::Reference(ap));
        appearances.set("N", Object::Dictionary(states));
    } else {
        appearances.set("N", Object::Reference(ap));
    }
    dict.set("AP", Object::Dictionary(appearances));
}

/// The widget's normal appearance stream for its current state.
fn normal_appearance(doc: &Document, widget: ObjectId) -> Option<ObjectId> {
    let dict = doc.get_dictionary(widget).ok()?;
    let n = dict_of(doc, dict.get(b"AP").ok()?)?.get(b"N").ok()?;
    if let Ok(id) = n.as_reference() {
        if doc.get_object(id).ok()?.as_stream().is_ok() {
            return Some(id);
        }
    }
    // A state dictionary: the stream of /AS.
    let state = dict.get(b"AS").and_then(Object::as_name).ok()?;
    dict_of(doc, n)?.get(state).ok()?.as_reference().ok()
}

/// The `cm` operands that put the appearance stream `ap` (its BBox through
/// its Matrix) onto `rect` (PDF 32000 12.5.5).
fn appearance_placement(doc: &Document, ap: ObjectId, rect: [f32; 4]) -> Option<String> {
    let stream = doc.get_object(ap).ok()?.as_stream().ok()?;
    let bbox = rect_of(doc, stream.dict.get(b"BBox").ok()?)?;
    let m: [f32; 6] = stream
        .dict
        .get(b"Matrix")
        .ok()
        .and_then(|m| m.as_array().ok())
        .map(|m| m.iter().filter_map(number).collect::<Vec<f32>>())
        .and_then(|m| m.try_into().ok())
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let corners = [
        (bbox[0], bbox[1]),
        (bbox[2], bbox[1]),
        (bbox[0], bbox[3]),
        (bbox[2], bbox[3]),
    ]
    .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in corners {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let (bw, bh) = (x1 - x0, y1 - y0);
    if bw <= 0.0 || bh <= 0.0 {
        return None;
    }
    let sx = (rect[2] - rect[0]) / bw;
    let sy = (rect[3] - rect[1]) / bh;
    Some(format!(
        "{sx} 0 0 {sy} {} {}",
        rect[0] - x0 * sx,
        rect[1] - y0 * sy
    ))
}

/// A stamp's paths as page content, mapped from its view box (y down) onto
/// its rect.
fn stamp_content(stamp: &FormStamp) -> Vec<u8> {
    let [vx, vy, vw, vh] = stamp.view_box;
    let [llx, lly, urx, ury] = stamp.rect;
    if vw <= 0.0 || vh <= 0.0 {
        return Vec::new();
    }
    let sx = (urx - llx) / vw;
    let sy = (ury - lly) / vh;
    let mut out = format!(
        "q {sx} 0 0 {} {} {} cm 1 J 1 j\n",
        -sy,
        llx - vx * sx,
        ury + vy * sy
    )
    .into_bytes();
    for path in &stamp.paths {
        if path.segments.is_empty() || (path.stroke.is_none() && path.fill.is_none()) {
            continue;
        }
        if let Some((width, [r, g, b])) = path.stroke {
            out.extend_from_slice(format!("{width} w {r} {g} {b} RG ").as_bytes());
        }
        if let Some([r, g, b]) = path.fill {
            out.extend_from_slice(format!("{r} {g} {b} rg ").as_bytes());
        }
        for segment in &path.segments {
            let op = match *segment {
                StampSegment::MoveTo(x, y) => format!("{x} {y} m "),
                StampSegment::LineTo(x, y) => format!("{x} {y} l "),
                StampSegment::CubicTo(x1, y1, x2, y2, x, y) => {
                    format!("{x1} {y1} {x2} {y2} {x} {y} c ")
                }
                StampSegment::Close => "h ".to_string(),
            };
            out.extend_from_slice(op.as_bytes());
        }
        out.extend_from_slice(match (path.fill.is_some(), path.stroke.is_some()) {
            (true, true) => b"B\n",
            (true, false) => b"f\n",
            _ => b"S\n",
        });
    }
    out.extend_from_slice(b"Q\n");
    out
}

/// The page's resources as a dictionary of its own (inherited or shared ones
/// copied in), so names can be added without touching other pages.
fn own_resources(doc: &Document, page: ObjectId) -> Dictionary {
    let mut node = Some(page);
    let mut depth = 0;
    while let Some(id) = node {
        let Ok(dict) = doc.get_dictionary(id) else {
            break;
        };
        if let Ok(resources) = dict.get(b"Resources") {
            if let Some(resources) = dict_of(doc, resources) {
                return resources.clone();
            }
        }
        node = dict.get(b"Parent").and_then(Object::as_reference).ok();
        depth += 1;
        if depth > 64 {
            break;
        }
    }
    Dictionary::new()
}

/// Appends `content` after the page's own content - the page's content
/// wrapped in `q` / `Q`, so its graphics state does not leak into ours - and
/// adds `xobjects` to the page's resources.
fn append_page_content(
    doc: &mut Document,
    page: ObjectId,
    content: Vec<u8>,
    xobjects: &[(Vec<u8>, ObjectId)],
) {
    let mut resources = own_resources(doc, page);
    if !xobjects.is_empty() {
        let mut existing = resources
            .get(b"XObject")
            .ok()
            .and_then(|x| dict_of(doc, x))
            .cloned()
            .unwrap_or_default();
        for (name, id) in xobjects {
            existing.set(name.clone(), Object::Reference(*id));
        }
        resources.set("XObject", Object::Dictionary(existing));
    }
    let old_contents: Vec<Object> = doc
        .get_dictionary(page)
        .ok()
        .and_then(|d| d.get(b"Contents").ok())
        .map(|c| match c {
            Object::Array(items) => items.clone(),
            other => vec![other.clone()],
        })
        .unwrap_or_default();
    let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut close = b"\nQ\n".to_vec();
    close.extend_from_slice(&content);
    let close = doc.add_object(Stream::new(Dictionary::new(), close));
    let mut contents = vec![Object::Reference(open)];
    contents.extend(old_contents);
    contents.push(Object::Reference(close));
    if let Ok(dict) = doc.get_object_mut(page).and_then(Object::as_dict_mut) {
        dict.set("Contents", Object::Array(contents));
        dict.set("Resources", Object::Dictionary(resources));
    }
}

/// Takes the form's widgets off the page's `/Annots`.
fn remove_widgets(doc: &mut Document, page: ObjectId, widgets: &BTreeSet<ObjectId>) {
    let annots = doc
        .get_dictionary(page)
        .ok()
        .and_then(|d| d.get(b"Annots").ok())
        .and_then(|a| doc.dereference(a).ok())
        .and_then(|(_, a)| a.as_array().ok())
        .cloned();
    let Some(annots) = annots else {
        return;
    };
    let kept: Vec<Object> = annots
        .into_iter()
        .filter(|a| a.as_reference().map_or(true, |id| !widgets.contains(&id)))
        .collect();
    if let Ok(dict) = doc.get_object_mut(page).and_then(Object::as_dict_mut) {
        if kept.is_empty() {
            dict.remove(b"Annots");
        } else {
            dict.set("Annots", Object::Array(kept));
        }
    }
}

/// `/AcroForm /NeedAppearances true`: viewers regenerate what they can.
fn set_need_appearances(doc: &mut Document) {
    let form = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"AcroForm").ok())
        .cloned();
    match form {
        Some(Object::Reference(id)) => {
            if let Ok(dict) = doc.get_object_mut(id).and_then(Object::as_dict_mut) {
                dict.set("NeedAppearances", Object::Boolean(true));
            }
        }
        Some(Object::Dictionary(_)) => {
            if let Ok(catalog) = doc.catalog_mut() {
                if let Ok(form) = catalog.get_mut(b"AcroForm").and_then(Object::as_dict_mut) {
                    form.set("NeedAppearances", Object::Boolean(true));
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use lopdf::{dictionary, Document, Object, Stream, StringFormat};

    use super::*;

    /// A one-page Letter PDF with a form: a text field `person.name` (a
    /// parent field with one child, Helvetica 10 pt, value "Grace"), a check
    /// box `agree` (its own /AP with the on-state `Yes`, unchecked) and a
    /// combo box `colour` (options red / green, value "red").
    fn form_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let on = doc.add_object(Stream::new(dictionary! {"BBox" => vec![0.into(), 0.into(), 12.into(), 12.into()]}, b"0 g 2 2 8 8 re f".to_vec()));
        let off = doc.add_object(Stream::new(dictionary! {"BBox" => vec![0.into(), 0.into(), 12.into(), 12.into()]}, Vec::new()));
        let person = doc.new_object_id();
        let name = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "P" => page_id, "Parent" => person,
            "T" => Object::String(b"name".to_vec(), StringFormat::Literal),
            "FT" => "Tx", "DA" => Object::String(b"/Helv 10 Tf 0 g".to_vec(), StringFormat::Literal),
            "V" => Object::String(b"Grace".to_vec(), StringFormat::Literal),
            "Rect" => vec![72.into(), 700.into(), 272.into(), 720.into()],
        });
        doc.objects.insert(person, Object::Dictionary(dictionary! {
            "T" => Object::String(b"person".to_vec(), StringFormat::Literal),
            "Kids" => vec![name.into()],
        }));
        let agree = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "P" => page_id,
            "T" => Object::String(b"agree".to_vec(), StringFormat::Literal),
            "FT" => "Btn", "V" => "Off", "AS" => "Off",
            "Rect" => vec![72.into(), 650.into(), 84.into(), 662.into()],
            "AP" => dictionary! {"N" => dictionary! {"Yes" => on, "Off" => off}},
        });
        let colour = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "P" => page_id,
            "T" => Object::String(b"colour".to_vec(), StringFormat::Literal),
            "FT" => "Ch", "Ff" => 1 << 17,
            "Opt" => vec![Object::String(b"red".to_vec(), StringFormat::Literal), Object::String(b"green".to_vec(), StringFormat::Literal)],
            "V" => Object::String(b"red".to_vec(), StringFormat::Literal),
            "Rect" => vec![72.into(), 600.into(), 172.into(), 620.into()],
        });
        let content = doc.add_object(Stream::new(dictionary! {}, b"0 0 1 rg 10 10 50 50 re f".to_vec()));
        doc.objects.insert(page_id, Object::Dictionary(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content, "Annots" => vec![name.into(), agree.into(), colour.into()],
        }));
        doc.objects.insert(pages_id, Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }));
        let form = doc.add_object(dictionary! {
            "Fields" => vec![person.into(), agree.into(), colour.into()],
            "DA" => Object::String(b"/Helv 0 Tf 0 g".to_vec(), StringFormat::Literal),
        });
        let catalog = doc.add_object(dictionary! {"Type" => "Catalog", "Pages" => pages_id, "AcroForm" => form});
        doc.trailer.set("Root", catalog);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn field<'a>(fields: &'a [FormField], name: &str) -> &'a FormField {
        fields.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}: {fields:?}"))
    }

    #[test]
    fn the_fields_of_a_form_are_read_with_their_names_values_and_places() {
        let fields = parse_form_fields(&form_pdf()).expect("a PDF");
        assert_eq!(fields.len(), 3, "{fields:?}");
        let name = field(&fields, "person.name");
        assert_eq!(name.kind, FormFieldKind::Text);
        assert_eq!(name.value, "Grace");
        assert_eq!(name.font_size, 10.0);
        assert_eq!(name.widgets.len(), 1);
        assert_eq!(name.widgets[0].page, 0);
        assert_eq!(name.widgets[0].rect, [72.0, 700.0, 272.0, 720.0]);
        let agree = field(&fields, "agree");
        assert_eq!(agree.kind, FormFieldKind::CheckBox);
        assert_eq!(agree.value, "Off");
        assert_eq!(agree.widgets[0].on_state, "Yes");
        assert!(!agree.is_on(&agree.widgets[0]));
        let colour = field(&fields, "colour");
        assert_eq!(colour.kind, FormFieldKind::ComboBox);
        assert_eq!(colour.options, ["red", "green"]);
        assert_eq!(colour.value, "red");
        assert!(parse_form_fields(b"not a pdf").is_err());
    }

    #[test]
    fn filled_values_are_in_the_fields_of_the_saved_pdf() {
        let filled = fill_form(
            &form_pdf(),
            &[
                FieldValue { name: "person.name".into(), value: "Ada Lovelace \u{e9}".into() },
                FieldValue { name: "agree".into(), value: "true".into() },
                FieldValue { name: "colour".into(), value: "green".into() },
                FieldValue { name: "nobody".into(), value: "x".into() },
            ],
            &[],
            false,
        )
        .expect("filled");
        let fields = parse_form_fields(&filled).expect("a PDF");
        assert_eq!(field(&fields, "person.name").value, "Ada Lovelace \u{e9}");
        assert_eq!(field(&fields, "agree").value, "Yes");
        assert_eq!(field(&fields, "colour").value, "green");
        // A fresh appearance shows the text (WinAnsi: e-acute is 0xE9).
        let doc = Document::load_mem(&filled).unwrap();
        let shown = doc.objects.values().filter_map(|o| o.as_stream().ok()).any(|s| {
            s.content.windows(15).any(|w| w == b"(Ada Lovelace \xE9")
        });
        assert!(shown, "an appearance stream draws the new text");
    }

    #[test]
    fn a_flattened_form_is_drawn_into_its_page_and_has_no_fields_left() {
        let filled = fill_form(
            &form_pdf(),
            &[FieldValue { name: "agree".into(), value: "Yes".into() }],
            &[],
            true,
        )
        .expect("flattened");
        assert!(parse_form_fields(&filled).expect("a PDF").is_empty(), "no form left");
        let doc = Document::load_mem(&filled).unwrap();
        let page = *doc.get_pages().values().next().unwrap();
        let page = doc.get_dictionary(page).unwrap();
        assert!(!page.has(b"Annots"), "the widgets are gone");
        let contents = page.get(b"Contents").unwrap().as_array().unwrap();
        assert_eq!(contents.len(), 3, "q, the page's own content, Q + the fields");
        let last = doc.get_object(contents[2].as_reference().unwrap()).unwrap().as_stream().unwrap();
        let text = String::from_utf8_lossy(&last.content).into_owned();
        // Three visible widgets, each placed onto its rect.
        assert_eq!(text.matches(" Do Q").count(), 3, "{text}");
        assert!(text.contains("1 0 0 1 72 650 cm /AzFlat"), "the check box's own /AP on its rect: {text}");
        let xobjects = page.get(b"Resources").unwrap().as_dict().unwrap().get(b"XObject").unwrap().as_dict().unwrap();
        assert_eq!(xobjects.len(), 3);
    }

    #[test]
    fn a_stamp_is_drawn_onto_its_rect_from_its_view_box() {
        let stamp = FormStamp {
            page: 0,
            rect: [100.0, 100.0, 300.0, 150.0],
            view_box: [0.0, 0.0, 400.0, 100.0],
            paths: vec![StampPath {
                segments: vec![StampSegment::MoveTo(0.0, 50.0), StampSegment::LineTo(400.0, 50.0)],
                stroke: Some((4.0, [0.0, 0.0, 0.5])),
                fill: None,
            }],
        };
        let filled = fill_form(&form_pdf(), &[], &[stamp], false).expect("stamped");
        let doc = Document::load_mem(&filled).unwrap();
        let page = *doc.get_pages().values().next().unwrap();
        let contents = doc.get_dictionary(page).unwrap().get(b"Contents").unwrap().as_array().unwrap().clone();
        let last = doc.get_object(contents.last().unwrap().as_reference().unwrap()).unwrap().as_stream().unwrap();
        let text = String::from_utf8_lossy(&last.content).into_owned();
        // 400 x 100 onto 200 x 50: half scale, y flipped, top-left at (100, 150).
        assert!(text.contains("q 0.5 0 0 -0.5 100 150 cm"), "{text}");
        assert!(text.contains("4 w 0 0 0.5 RG 0 50 m 400 50 l S"), "{text}");
        // The form itself is untouched.
        assert_eq!(parse_form_fields(&filled).unwrap().len(), 3);
    }

    #[test]
    fn the_values_draw_as_svg_on_their_page() {
        let mut fields = parse_form_fields(&form_pdf()).unwrap();
        fields.iter_mut().find(|f| f.name == "agree").unwrap().value = "Yes".into();
        let svg = fields_svg(&fields, 0, 792.0);
        assert!(svg.contains(">Grace</text>"), "{svg}");
        assert!(svg.contains(">red</text>"), "{svg}");
        assert!(svg.contains("<path"), "the check mark: {svg}");
        assert!(fields_svg(&fields, 1, 792.0).is_empty(), "nothing on another page");
    }
}
