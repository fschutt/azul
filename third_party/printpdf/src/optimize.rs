//! Shrinks a finished PDF.
//!
//! It works on the bytes of a file, not on a [`PdfDocument`](crate::PdfDocument). Bitmaps are
//! encoded again (`optimize_images`) and redundant `/ActualText` entries are dropped
//! ([`optimize_text`]). Pages, fonts and the text a reader extracts stay as they are.
//!
//! Use them on PDFs from other programs, or on your own output when size matters. A document
//! made with this crate is already optimized when saved with
//! [`PdfSaveOptions::image_optimization`](crate::PdfSaveOptions::image_optimization).
//!
//! | Function | Needs | Lossy |
//! | --- | --- | --- |
//! | `optimize_images` | `images` feature | depends on the options: JPEG, scaling and greyscale are, Flate is not |
//! | [`optimize_text`] | nothing | no |
//!
//! # Examples
//!
//! ```
//! # #[cfg(feature = "images")] {
//! use printpdf::*;
//!
//! // A PDF with an uncompressed 64 x 64 grey image.
//! let image = RawImage {
//!     pixels: RawImageData::U8(vec![128; 64 * 64]),
//!     width: 64,
//!     height: 64,
//!     data_format: RawImageFormat::R8,
//!     tag: Vec::new(),
//! };
//! let mut doc = PdfDocument::new("Big image");
//! let id = doc.add_image(&image);
//! let op = Op::UseXobject { id, transform: XObjectTransform::default() };
//! let page = PdfPage::new(Mm(210.0), Mm(297.0), vec![op]);
//! let opts = PdfSaveOptions { image_optimization: None, ..Default::default() };
//! let pdf = doc.with_pages(vec![page]).save(&opts, &mut Vec::new());
//!
//! // Encode it again, losslessly.
//! let lossless = ImageOptimizationOptions {
//!     format: Some(ImageCompression::Flate),
//!     max_image_size: None,
//!     ..Default::default()
//! };
//! let smaller = optimize_images(&pdf, &lossless, &mut Vec::new()).unwrap();
//! assert!(smaller.len() < pdf.len());
//! # }
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::{
    cmap::{ToUnicodeCMap, MAX_CMAP_STREAM_BYTES},
    PdfWarnMsg,
};
#[cfg(feature = "images")]
use crate::{
    deserialize::raw_bitmap_from_stream, image::image_to_stream, ImageOptimizationOptions,
};

/// The widest `TJ` adjustment (thousandths of an em) between two glyphs that readers still take
/// for one word. A wider gap reads as a space (poppler: a tenth of the font size), so
/// letter-spaced text ("P R E F A C E") keeps its `/ActualText`.
const MAX_KERN: f32 = 100.0;

/// Drops the `/ActualText` entries that repeat what the glyphs already say, and returns the new
/// PDF.
///
/// `/ActualText` is a marked-content property that tells readers which text a run of glyphs
/// stands for, for copy and search (ISO 32000-1, 14.9.4). When the glyphs map to exactly that
/// text through the font's `/ToUnicode` and are set as plain text, the entry adds nothing and
/// only costs bytes. Nothing else changes, so the result is lossless. `warnings` gets one info
/// message with the number of entries dropped and kept.
///
/// # Errors
///
/// Returns a message when `pdf` cannot be parsed or the result cannot be written.
pub fn optimize_text(pdf: &[u8], warnings: &mut Vec<PdfWarnMsg>) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(pdf).map_err(|e| format!("Failed to load PDF: {e}"))?;
    let spans = drop_redundant_actual_text(&mut doc);
    for id in &spans.rewritten {
        if let Ok(Object::Stream(stream)) = doc.get_object_mut(*id) {
            let _ = stream.compress();
        }
    }
    warnings.push(PdfWarnMsg::info(
        0,
        0,
        format!(
            "/ActualText: {} spans dropped, {} kept",
            spans.dropped, spans.kept
        ),
    ));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| format!("Failed to write PDF: {e}"))?;
    Ok(bytes)
}

/// What `drop_redundant_actual_text` did
pub(crate) struct DroppedSpans {
    /// `/ActualText` spans dropped
    pub dropped: usize,
    /// `/ActualText` spans kept
    pub kept: usize,
    /// The content streams written again (uncompressed)
    pub rewritten: Vec<ObjectId>,
}

/// Drops the `/ActualText` of every `/Span` whose glyphs read as that text anyway.
///
/// A reader takes the text of a span with an `/ActualText` from that string, and other text from
/// the glyphs: each code through its font's `/ToUnicode`, with a space wherever the glyphs leave
/// a gap. The `/ActualText` adds nothing when
///
/// - it is all the span's properties hold (no `/MCID`, `/Lang`, ...), with nothing else marked
///   inside the span,
/// - every glyph in the span has text in its font's `/ToUnicode` (an Identity-H/V Type0 font or
///   a simple font), and together they spell the `/ActualText` exactly,
/// - and they are set as plain text: no character spacing, horizontal scaling or rise, no gap
///   wider than [`MAX_KERN`] between two glyphs unless after a space, and no `'`, `"` or form
///   XObject inside the span.
///
/// The span's `BDC` and `EMC` go, and what it shows stays. Pages whose content streams other
/// pages share, or that do not parse, are left as they are.
pub(crate) fn drop_redundant_actual_text(doc: &mut Document) -> DroppedSpans {
    let mut result = DroppedSpans {
        dropped: 0,
        kept: 0,
        rewritten: Vec::new(),
    };
    let pages: Vec<ObjectId> = crate::deserialize::numbered_pages(doc)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let mut uses: BTreeMap<ObjectId, usize> = BTreeMap::new();
    for page in &pages {
        for id in doc.get_page_contents(*page) {
            *uses.entry(id).or_default() += 1;
        }
    }
    let mut fonts = BTreeMap::new();

    for page in pages {
        let streams = doc.get_page_contents(page);
        let Some((&first, rest)) = streams.split_first() else {
            continue;
        };
        // A stream that is not counted once is shared, or changed since the count.
        if streams.iter().any(|id| uses.get(id) != Some(&1)) {
            continue;
        }
        let bytes = doc.get_page_content(page);
        if !bytes.windows(11).any(|w| w == b"/ActualText") {
            continue;
        }
        let Ok(content) = Content::decode(&bytes) else {
            continue;
        };
        let page_fonts = page_fonts(doc, page, &mut fonts);
        let (drop, kept) = redundant_spans(&content.operations, &page_fonts);
        result.kept += kept;
        if drop.is_empty() {
            continue;
        }
        let operations: Vec<Operation> = content
            .operations
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !drop.contains(i))
            .map(|(_, op)| op)
            .collect();
        let Ok(new) = (Content { operations }).encode() else {
            result.kept += drop.len() / 2;
            continue;
        };
        result.dropped += drop.len() / 2;
        // all of the page's content goes into its first stream
        doc.objects.insert(
            first,
            Object::Stream(Stream::new(Dictionary::new(), new)),
        );
        if !rest.is_empty() {
            for id in rest {
                doc.objects.remove(id);
            }
            if let Ok(page) = doc.get_dictionary_mut(page) {
                page.set("Contents", first);
            }
        }
        result.rewritten.push(first);
    }
    result
}

/// What a font's codes read as: the text its `/ToUnicode` gives each code, and how many bytes a
/// code takes (two in an Identity-H/V Type0 font, one in a simple font)
struct FontText {
    code_bytes: usize,
    to_unicode: ToUnicodeCMap,
}

impl FontText {
    /// None for other encodings and for fonts without a `/ToUnicode`: their text is not checked
    fn of(doc: &Document, font: &Dictionary) -> Option<FontText> {
        let name = |key: &[u8]| font.get(key).and_then(Object::as_name).ok();
        let code_bytes = match name(b"Subtype")? {
            b"Type0" if matches!(name(b"Encoding"), Some(b"Identity-H" | b"Identity-V")) => 2,
            b"Type1" | b"MMType1" | b"TrueType" | b"Type3" => 1,
            _ => return None,
        };
        let (_, cmap) = doc.dereference(font.get(b"ToUnicode").ok()?).ok()?;
        let cmap = cmap.as_stream().ok()?;
        let bytes = match cmap.decompressed_content_with_limit(MAX_CMAP_STREAM_BYTES) {
            Ok(bytes) => bytes,
            // A stream that inflates past the limit is not read.
            Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. })) => {
                return None;
            }
            Err(_) => cmap.content.clone(),
        };
        let to_unicode = ToUnicodeCMap::parse(&String::from_utf8_lossy(&bytes)).ok()?;
        Some(FontText {
            code_bytes,
            to_unicode,
        })
    }
}

/// The fonts a page's content can name, by resource name: from the page's own resources first,
/// then from those it inherits. `cache` keeps each font object's text across pages.
fn page_fonts(
    doc: &Document,
    page: ObjectId,
    cache: &mut BTreeMap<ObjectId, Option<Rc<FontText>>>,
) -> BTreeMap<Vec<u8>, Option<Rc<FontText>>> {
    let mut fonts = BTreeMap::new();
    let Ok((own, inherited)) = doc.get_page_resources(page) else {
        return fonts;
    };
    let resources = own.into_iter().chain(
        inherited
            .iter()
            .filter_map(|id| doc.get_dictionary(*id).ok()),
    );
    for resources in resources {
        let Some(font_dict) = resources
            .get(b"Font")
            .ok()
            .and_then(|fonts| doc.dereference(fonts).ok())
            .and_then(|(_, fonts)| fonts.as_dict().ok())
        else {
            continue;
        };
        for (name, font) in font_dict.iter() {
            if fonts.contains_key(name) {
                continue;
            }
            let text = match font {
                Object::Reference(id) => cache
                    .entry(*id)
                    .or_insert_with(|| {
                        let font = doc.get_dictionary(*id).ok()?;
                        FontText::of(doc, font).map(Rc::new)
                    })
                    .clone(),
                Object::Dictionary(font) => FontText::of(doc, font).map(Rc::new),
                _ => None,
            };
            fonts.insert(name.clone(), text);
        }
    }
    fonts
}

/// The text state that decides how glyphs read
#[derive(Clone)]
struct TextState {
    font: Option<Rc<FontText>>,
    char_spacing: f32,
    scaling: f32,
    rise: f32,
}

impl TextState {
    const INITIAL: TextState = TextState {
        font: None,
        char_spacing: 0.0,
        scaling: 100.0,
        rise: 0.0,
    };

    /// No character spacing, horizontal scaling or rise
    fn plain(&self) -> bool {
        self.char_spacing == 0.0 && self.scaling == 100.0 && self.rise == 0.0
    }
}

/// An open marked-content section
struct Span {
    /// Index of its `BDC` or `BMC`
    begin: usize,
    /// Its `/ActualText`, if it is a `/Span` with nothing else in its properties
    actual_text: Option<String>,
    /// What its glyphs read as, while they can be read
    text: Option<String>,
    /// Whether the last glyph read is a space
    after_space: bool,
}

/// The indices of the `BDC` and `EMC` of each `/Span` whose `/ActualText` its glyphs read as
/// anyway, and how many other `/ActualText` spans there are
fn redundant_spans(
    ops: &[Operation],
    fonts: &BTreeMap<Vec<u8>, Option<Rc<FontText>>>,
) -> (BTreeSet<usize>, usize) {
    let mut state = TextState::INITIAL;
    let mut saved = Vec::new();
    let mut open: Vec<Span> = Vec::new();
    let mut drop = BTreeSet::new();
    let mut kept = 0;
    for (i, op) in ops.iter().enumerate() {
        // an operand that is not a number leaves the text not plain (NaN is no number)
        let number = || {
            op.operands
                .first()
                .and_then(|n| n.as_float().ok())
                .unwrap_or(f32::NAN)
        };
        match op.operator.as_str() {
            "q" => saved.push(state.clone()),
            "Q" => {
                if let Some(s) = saved.pop() {
                    state = s;
                }
            }
            "Tf" => {
                state.font = op
                    .operands
                    .first()
                    .and_then(|name| name.as_name().ok())
                    .and_then(|name| fonts.get(name))
                    .cloned()
                    .flatten()
            }
            "Tc" => state.char_spacing = number(),
            "Tz" => state.scaling = number(),
            "Ts" => state.rise = number(),
            "BDC" | "BMC" => {
                // With marked content inside, a span is left as it is. The span that is open
                // is marked when its child opens; its own parents were marked when it opened.
                if let Some(span) = open.last_mut() {
                    span.text = None;
                }
                open.push(Span {
                    begin: i,
                    actual_text: actual_text(&op.operands),
                    text: Some(String::new()),
                    after_space: false,
                });
            }
            "EMC" => {
                if let Some(Span {
                    begin,
                    actual_text: Some(actual_text),
                    text,
                    ..
                }) = open.pop()
                {
                    if text.as_ref() == Some(&actual_text) {
                        drop.insert(begin);
                        drop.insert(i);
                    } else {
                        kept += 1;
                    }
                }
            }
            "Tj" | "TJ" => {
                if let Some(span) = open.last_mut() {
                    let readable = match &mut span.text {
                        Some(text) => read(text, &mut span.after_space, op, &state).is_some(),
                        None => true,
                    };
                    if !readable {
                        span.text = None;
                    }
                }
            }
            "'" | "\"" | "Do" => {
                if let Some(span) = open.last_mut() {
                    span.text = None;
                }
            }
            _ => {}
        }
    }
    (drop, kept)
}

/// The `/ActualText` of a `/Span` whose properties hold nothing else
fn actual_text(operands: &[Object]) -> Option<String> {
    let [Object::Name(tag), Object::Dictionary(properties)] = operands else {
        return None;
    };
    if tag != b"Span" || properties.len() != 1 {
        return None;
    }
    match properties.get(b"ActualText") {
        Ok(Object::String(bytes, _)) => text_string(bytes),
        _ => None,
    }
}

/// A PDF text string: UTF-16BE after its byte-order mark, UTF-8 after its (PDF 2.0), else
/// PDFDocEncoding, of which only the printable characters it shares with Latin-1 are read
fn text_string(bytes: &[u8]) -> Option<String> {
    if let Some(utf16) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        if utf16.len() % 2 != 0 {
            return None;
        }
        let units: Vec<u16> = utf16
            .chunks_exact(2)
            .filter_map(|pair| match *pair {
                [hi, lo] => Some(u16::from_be_bytes([hi, lo])),
                _ => None,
            })
            .collect();
        return String::from_utf16(&units).ok();
    }
    if let Some(utf8) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(utf8.to_vec()).ok();
    }
    bytes
        .iter()
        .map(|&b| matches!(b, 0x20..=0x7E | 0xA1..=0xAC | 0xAE..=0xFF).then_some(b as char))
        .collect()
}

/// Appends what the glyphs of a `Tj` or `TJ` read as to `text`. None if they cannot be read the
/// same way by every reader: a glyph without text, text not set plain, or a gap between two
/// glyphs that reads as a space.
fn read(
    text: &mut String,
    after_space: &mut bool,
    op: &Operation,
    state: &TextState,
) -> Option<()> {
    let font = state.font.as_ref().filter(|_| state.plain())?;
    let items: &[Object] = match (op.operator.as_str(), op.operands.first()?) {
        ("TJ", Object::Array(items)) => items,
        ("Tj", string @ Object::String(..)) => std::slice::from_ref(string),
        _ => return None,
    };
    // `code_bytes` is 1 or 2; the `max` keeps the length of a code from ever being 0.
    let code_bytes = font.code_bytes.max(1);
    for item in items {
        match item {
            Object::String(codes, _) => {
                if codes.len() % code_bytes != 0 {
                    return None;
                }
                for code in codes.chunks(code_bytes) {
                    let code = code.iter().fold(0u32, |c, &b| c << 8 | u32::from(b));
                    let glyph = font.to_unicode.lookup_string(code)?;
                    *after_space = glyph.chars().all(char::is_whitespace);
                    text.push_str(&glyph);
                }
            }
            Object::Integer(_) | Object::Real(_) => {
                // a TJ number moves the next glyph left: a negative one widens the gap
                let widening = -item.as_float().ok()?;
                if widening.abs() > MAX_KERN && !(widening > 0.0 && *after_space) {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(())
}

/// Encodes the bitmaps of a finished PDF again, as
/// [`PdfDocument::save`](crate::PdfDocument::save) does with `options`, and returns the new
/// PDF. A bitmap is only replaced when the new encoding is smaller, and keeps its object number,
/// so the pages that show it stay as they are. Fonts, pages and content streams are not touched.
///
/// Whether this is lossy depends on `options`: JPEG (the default for color images with the
/// `jpeg` feature), `max_image_size` scaling and greyscale conversion lose information,
/// Flate with `max_image_size: None` does not. `warnings` gets one info message per replaced
/// bitmap.
///
/// Only bitmaps that this crate reads and writes the same way are considered: 8-bit
/// `DeviceGray` or `DeviceRGB`, Flate-compressed or not, without a soft mask, mask, `/Decode`
/// array or predictor. JPEGs, masked images and everything else are kept as they are.
///
/// For scans of black-and-white prints, `dither_greyscale` turns grey pixels into black or
/// white and the bitmaps are then written with 1 bit per pixel.
///
/// Requires the `images` feature.
///
/// # Errors
///
/// Returns a message when `pdf` cannot be parsed or the result cannot be written.
#[cfg(feature = "images")]
pub fn optimize_images(
    pdf: &[u8],
    options: &ImageOptimizationOptions,
    warnings: &mut Vec<PdfWarnMsg>,
) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(pdf).map_err(|e| format!("Failed to load PDF: {e}"))?;

    // Masks are image streams too, but they belong to the image they mask.
    let masks: BTreeSet<ObjectId> = doc
        .objects
        .values()
        .filter_map(as_image)
        .flat_map(|image| {
            [b"SMask".as_slice(), b"Mask"]
                .into_iter()
                .filter_map(|key| image.dict.get(key).and_then(Object::as_reference).ok())
        })
        .collect();
    let pictures: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(id, object)| !masks.contains(id) && as_image(object).is_some_and(reencodable))
        .map(|(id, _)| *id)
        .collect();

    for id in pictures {
        let Some(old) = doc.objects.get(&id).and_then(as_image) else {
            continue;
        };
        let Some(picture) = raw_bitmap_from_stream(&doc, old) else {
            continue;
        };
        let old_dict = old.dict.clone();
        let old_len = old.content.len();
        let mut new = image_to_stream(picture, &mut doc, Some(options));
        if new.content.len() >= old_len {
            continue;
        }
        // Keep the other entries of the old image (/Intent, /Metadata, /OC, ...).
        for (key, value) in old_dict.iter() {
            if !new.dict.has(key) && !matches!(key.as_slice(), b"Filter" | b"DecodeParms") {
                new.dict.set(key.clone(), value.clone());
            }
        }
        warnings.push(PdfWarnMsg::info(
            0,
            0,
            format!(
                "picture {} {} R: {old_len} bytes, now {}",
                id.0,
                id.1,
                new.content.len()
            ),
        ));
        doc.objects.insert(id, Object::Stream(new));
    }

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| format!("Failed to write PDF: {e}"))?;
    Ok(bytes)
}

#[cfg(feature = "images")]
fn as_image(object: &Object) -> Option<&Stream> {
    match object {
        Object::Stream(s)
            if s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image") =>
        {
            Some(s)
        }
        _ => None,
    }
}

/// Whether the picture's pixels are read and written the same way: 8-bit DeviceGray or
/// DeviceRGB, Flate or no filter, and nothing else that changes how they are drawn
#[cfg(feature = "images")]
fn reencodable(image: &Stream) -> bool {
    let dict = &image.dict;
    let flate_or_none = match dict.get(b"Filter") {
        Err(_) => true,
        Ok(Object::Name(filter)) => filter == b"FlateDecode",
        Ok(Object::Array(filters)) => filters
            .iter()
            .all(|f| f.as_name().ok() == Some(b"FlateDecode")),
        Ok(_) => false,
    };
    flate_or_none
        && matches!(
            dict.get(b"ColorSpace").and_then(Object::as_name).ok(),
            Some(b"DeviceGray" | b"DeviceRGB")
        )
        && dict.get(b"BitsPerComponent").and_then(Object::as_i64).ok() == Some(8)
        && [
            b"SMask".as_slice(),
            b"Mask",
            b"Decode",
            b"DecodeParms",
            b"ImageMask",
        ]
        .iter()
        .all(|key| !dict.has(key))
}
