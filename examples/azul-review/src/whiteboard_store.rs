//! The whiteboard's file: what a board is kept as, and what a file is read back into. Pure - the
//! bytes in, the bytes out; the zip around them and the PNG codec are azul's and live in the glue
//! (`whiteboard_archive`), the disk is the data drive's.
//!
//! A board is a set of entries: `board.json` (the manifest: format, workspace, the view, the items
//! in the order they are drawn) and one `img-<id>.png` for each cutout that has a picture, written
//! as it was made (the PNG is encoded once, when the cutout is captured). The format is versioned
//! (`azreview-board/1`): a key this version does not know is ignored, an item it does not understand
//! is left out and counted, a file of a newer major version is refused with a sentence. Reading is
//! tolerant and bounded: a missing or damaged picture leaves its cutout with the lines it was made
//! from, a damaged manifest is an error and never a panic, and what is read is kept inside the board's
//! caps.
//!
//! One board per workspace: [`board_name`] names its file after the workspace's folder and a hash of
//! its whole path. One write at a time ([`SaveGate`]); a file that cannot be read is kept as it is
//! under [`quarantine_name`] before the board that replaces it is written.

use crate::{
    whiteboard::{Anchor, Arrow, BoardData, Cutout, End, ImageSlot, Item, ItemId, Limits, Source},
    whiteboard_cutout::Raster,
    whiteboard_geom::{Pt, Rect, Side, View},
    whiteboard_json::{self as json, num, push_string, Json},
};

/// The format a manifest names, and the one this version reads and writes.
pub const FORMAT: &str = "azreview-board/1";
const FORMAT_PREFIX: &str = "azreview-board/";
/// The newest major version this version reads.
pub const FORMAT_MAJOR: u32 = 1;
/// The manifest's name among the entries.
pub const MANIFEST: &str = "board.json";
/// The largest picture file that is read, bytes.
pub const MAX_PICTURE_BYTES: usize = 64 * 1024 * 1024;
/// The most items read from a file, whatever the board's own cap: a file cannot make the reader hold more.
pub const MAX_ITEMS_READ: usize = 5_000;

/// One file inside the archive.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// The name of the entry that holds cutout `id`'s picture.
#[must_use]
pub fn picture_name(id: ItemId) -> String {
    format!("img-{id}.png")
}

fn hex(v: u64) -> String {
    format!("{v:016x}")
}

fn push_point(out: &mut String, p: Pt) {
    out.push('[');
    out.push_str(&num(p.x));
    out.push(',');
    out.push_str(&num(p.y));
    out.push(']');
}

fn push_end(out: &mut String, end: &End) {
    match end {
        End::Free(p) => {
            out.push_str("{\"free\":");
            push_point(out, *p);
            out.push('}');
        }
        End::Item { id, anchor } => {
            out.push_str(&format!("{{\"item\":{id},\"anchor\":"));
            match anchor {
                Anchor::Auto => out.push_str("\"auto\""),
                Anchor::Side(s) => {
                    out.push_str("\"side\",\"side\":");
                    push_string(out, s.name());
                }
                Anchor::Point { u, v } => {
                    out.push_str(&format!("\"point\",\"u\":{},\"v\":{}", num(*u), num(*v)));
                }
            }
            out.push('}');
        }
    }
}

fn push_source(out: &mut String, s: &Source) {
    out.push_str("{\"file\":");
    push_string(out, &s.file);
    out.push_str(&format!(
        ",\"page\":{},\"first_line\":{},\"last_line\":{},\"file_hash\":\"{}\",\"range_hash\":\"{}\",\"shape\":[",
        s.page,
        s.first_line,
        s.last_line,
        hex(s.file_hash),
        hex(s.range_hash)
    ));
    for (i, p) in s.shape.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_point(out, *p);
    }
    out.push_str("],\"lines\":[");
    for (i, l) in s.lines.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_string(out, l);
    }
    out.push_str("]}");
}

fn push_item(out: &mut String, item: &Item, has_png: bool) {
    match item {
        Item::Cutout(c) => {
            out.push_str(&format!(
                "{{\"kind\":\"cutout\",\"id\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"label\":",
                c.id,
                num(c.rect.x),
                num(c.rect.y),
                num(c.rect.w),
                num(c.rect.h)
            ));
            push_string(out, &c.label);
            match &c.image {
                ImageSlot::Ready(img) if has_png => out.push_str(&format!(
                    ",\"image\":{},\"scale\":{},\"image_w\":{},\"image_h\":{}",
                    json_string(&picture_name(c.id)),
                    num(img.scale),
                    img.width,
                    img.height
                )),
                ImageSlot::Ready(_) => out.push_str(",\"image\":null,\"state\":\"failed\",\"why\":\"no picture was kept\""),
                ImageSlot::Pending => out.push_str(",\"image\":null,\"state\":\"pending\""),
                ImageSlot::Failed(why) => {
                    out.push_str(",\"image\":null,\"state\":\"failed\",\"why\":");
                    push_string(out, why);
                }
            }
            out.push_str(",\"source\":");
            push_source(out, &c.source);
            out.push('}');
        }
        Item::Arrow(a) => {
            out.push_str(&format!("{{\"kind\":\"arrow\",\"id\":{},\"from\":", a.id));
            push_end(out, &a.from);
            out.push_str(",\"to\":");
            push_end(out, &a.to);
            out.push_str(&format!(",\"bend\":{},\"label\":", num(a.bend)));
            push_string(out, &a.label);
            out.push('}');
        }
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::new();
    push_string(&mut out, s);
    out
}

/// The entries a board is kept as: the manifest, then a picture for each cutout that has a PNG. The
/// workspace is the folder's path as the person knows it; `saved_ms` is the time (Unix milliseconds).
#[must_use]
pub fn write(data: &BoardData, workspace: &str, saved_ms: u64) -> Vec<Entry> {
    let mut pictures = Vec::new();
    let mut out = String::with_capacity(512 + data.items.len() * 256);
    out.push_str("{\n  \"format\": ");
    push_string(&mut out, FORMAT);
    out.push_str(",\n  \"workspace\": ");
    push_string(&mut out, workspace);
    out.push_str(&format!(",\n  \"saved_ms\": {saved_ms},\n  \"next_id\": {},\n", data.next_id));
    out.push_str(&format!(
        "  \"view\": {{\"x\": {}, \"y\": {}, \"zoom\": {}}},\n  \"items\": [",
        num(data.view.pan.x),
        num(data.view.pan.y),
        num(data.view.zoom)
    ));
    for (i, item) in data.items.iter().enumerate() {
        out.push_str(if i > 0 { ",\n    " } else { "\n    " });
        let png = match item {
            Item::Cutout(Cutout { image: ImageSlot::Ready(img), id, .. }) if !img.png.is_empty() => {
                pictures.push(Entry { name: picture_name(*id), bytes: img.png.clone() });
                true
            }
            _ => false,
        };
        push_item(&mut out, item, png);
    }
    out.push_str("\n  ]\n}\n");
    let mut entries = vec![Entry { name: MANIFEST.to_string(), bytes: out.into_bytes() }];
    entries.extend(pictures);
    entries
}

/// A picture decoded: straight (not multiplied) RGBA, rows from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedPng {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// What reads a PNG (azul's decoder in the app, a stand-in in the tests).
pub trait ImageCodec {
    /// # Errors
    /// A sentence for what is wrong with the bytes.
    fn decode_png(&self, png: &[u8]) -> Result<DecodedPng, String>;
}

/// A board read from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub data: BoardData,
    pub workspace: String,
    pub saved_ms: u64,
    /// Items of the file that could not be used (unknown kind, missing what they need, more than
    /// [`MAX_ITEMS_READ`]).
    pub left_out: usize,
    /// Pictures that the file names and that could not be had (missing, damaged, over the caps): their
    /// cutouts are there with the lines they were made from.
    pub unreadable_pictures: usize,
}

/// Why a file is not a board this version can open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// It is no board file at all (and what is wrong with it).
    NotABoard(String),
    /// It was written by a newer major version.
    TooNew { found: u32 },
    /// Its manifest is longer than any manifest may be.
    TooLarge { bytes: usize },
}

impl StoreError {
    /// The sentence, in English.
    #[must_use]
    pub fn english(&self) -> String {
        match self {
            Self::NotABoard(why) => format!("This file is not a whiteboard of this version: {why}."),
            Self::TooNew { found } => format!(
                "This whiteboard was saved by a newer AzReview (its format is {found}); it is left as it is."
            ),
            Self::TooLarge { bytes } => format!("This whiteboard's manifest has {bytes} bytes, more than a whiteboard has."),
        }
    }

    /// The key of the sentence in the resources.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            Self::NotABoard(_) => "azreview-whiteboard-store-not-a-board",
            Self::TooNew { .. } => "azreview-whiteboard-store-too-new",
            Self::TooLarge { .. } => "azreview-whiteboard-store-too-large",
        }
    }

    /// What is wrong, for the sentence's `{ $why }`.
    #[must_use]
    pub fn why(&self) -> Option<&str> {
        match self {
            Self::NotABoard(why) => Some(why),
            _ => None,
        }
    }

    /// The numbers of the sentence.
    #[must_use]
    pub fn args(&self) -> Vec<(&'static str, u64)> {
        match self {
            Self::NotABoard(_) => Vec::new(),
            Self::TooNew { found } => vec![("found", u64::from(*found))],
            Self::TooLarge { bytes } => vec![("bytes", *bytes as u64)],
        }
    }
}

fn f32_of(j: &Json, key: &str) -> Option<f32> {
    j.get(key)?.as_f32().filter(|v| v.is_finite())
}

fn point_of(j: &Json) -> Option<Pt> {
    let a = j.as_arr()?;
    let (x, y) = (a.first()?.as_f32()?, a.get(1)?.as_f32()?);
    (x.is_finite() && y.is_finite()).then(|| Pt::new(x, y))
}

fn end_of(j: &Json) -> Option<End> {
    if let Some(p) = j.get("free") {
        return point_of(p).map(End::Free);
    }
    let id = j.get("item")?.as_u64()?;
    let anchor = match j.get("anchor").and_then(Json::as_str) {
        Some("side") => j.get("side").and_then(Json::as_str).and_then(Side::from_name).map_or(Anchor::Auto, Anchor::Side),
        Some("point") => match (f32_of(j, "u"), f32_of(j, "v")) {
            (Some(u), Some(v)) => Anchor::Point { u, v },
            _ => Anchor::Auto,
        },
        _ => Anchor::Auto,
    };
    Some(End::Item { id, anchor })
}

fn source_of(j: &Json) -> Option<Source> {
    let hash = |key: &str| -> u64 {
        j.get(key).and_then(Json::as_str).filter(|h| h.len() <= 16).and_then(|h| u64::from_str_radix(h, 16).ok()).unwrap_or(0)
    };
    let line = |key: &str| u32::try_from(j.get(key).and_then(Json::as_u64)?).ok();
    Some(Source {
        file: j.get("file")?.as_str()?.to_string(),
        page: line("page").unwrap_or(0),
        first_line: line("first_line")?,
        last_line: line("last_line")?,
        shape: j.get("shape").and_then(Json::as_arr).map(|a| a.iter().filter_map(point_of).collect()).unwrap_or_default(),
        file_hash: hash("file_hash"),
        range_hash: hash("range_hash"),
        lines: j
            .get("lines")
            .and_then(Json::as_arr)
            .map(|a| a.iter().filter_map(|l| l.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
    })
}

/// What reading the pictures has used up.
struct Budget<'a> {
    limits: &'a Limits,
    pixels: u64,
    unreadable: usize,
}

/// A picture that could not be had: counted, and the reason kept with the cutout.
fn failed(budget: &mut Budget<'_>, why: &str) -> ImageSlot {
    budget.unreadable += 1;
    ImageSlot::Failed(why.to_string())
}

fn picture_of(
    j: &Json,
    budget: &mut Budget<'_>,
    picture: &dyn Fn(&str) -> Option<Vec<u8>>,
    codec: &dyn ImageCodec,
) -> ImageSlot {
    let Some(name) = j.get("image").and_then(Json::as_str) else {
        return match j.get("state").and_then(Json::as_str) {
            Some("pending") => ImageSlot::Failed("not drawn when the board was saved".to_string()),
            _ => ImageSlot::Failed(j.get("why").and_then(Json::as_str).unwrap_or("no picture was kept").to_string()),
        };
    };
    let Some(bytes) = picture(name) else {
        return failed(budget, "the picture is missing from the file");
    };
    if bytes.len() > MAX_PICTURE_BYTES {
        return failed(budget, "the picture's file is larger than a picture may be");
    }
    let decoded = match codec.decode_png(&bytes) {
        Ok(d) => d,
        Err(why) => return failed(budget, &format!("the picture could not be read: {why}")),
    };
    let px = u64::from(decoded.width) * u64::from(decoded.height);
    if px > budget.limits.max_image_px {
        return failed(budget, "the picture has more pixels than the board may keep");
    }
    if budget.pixels + px > budget.limits.max_total_px {
        return failed(budget, "there is no room for it among the pictures of the board");
    }
    let scale = f32_of(j, "scale").filter(|s| *s > 0.0).unwrap_or(1.0);
    let Some(raster) = Raster::from_straight(decoded.width, decoded.height, decoded.rgba) else {
        return failed(budget, "the picture's pixels do not fit its size");
    };
    budget.pixels += px;
    ImageSlot::Ready(raster.into_shared_image(scale, bytes))
}

/// The board a manifest holds, with the pictures `picture` gives by name, decoded by `codec`, kept inside
/// `limits`. The board is not yet made consistent: [`crate::whiteboard::Board::from_data`] does that.
///
/// # Errors
/// The manifest is no board file, is of a newer major version, or is larger than any is.
pub fn read(
    manifest: &[u8],
    picture: &dyn Fn(&str) -> Option<Vec<u8>>,
    codec: &dyn ImageCodec,
    limits: &Limits,
) -> Result<Loaded, StoreError> {
    if manifest.len() > json::MAX_TEXT {
        return Err(StoreError::TooLarge { bytes: manifest.len() });
    }
    let text = std::str::from_utf8(manifest).map_err(|_| StoreError::NotABoard("it is not text".to_string()))?;
    let root = json::parse(text).map_err(|e| StoreError::NotABoard(format!("{} (byte {})", e.why, e.at)))?;
    let Json::Obj(_) = root else {
        return Err(StoreError::NotABoard("it is not an object".to_string()));
    };
    let format = root.get("format").and_then(Json::as_str).ok_or_else(|| StoreError::NotABoard("it names no format".to_string()))?;
    let major = format
        .strip_prefix(FORMAT_PREFIX)
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|m| *m >= 1)
        .ok_or_else(|| StoreError::NotABoard(format!("its format \"{format}\" is not one of AzReview's boards")))?;
    if major > FORMAT_MAJOR {
        return Err(StoreError::TooNew { found: major });
    }

    let mut budget = Budget { limits, pixels: 0, unreadable: 0 };
    let mut items = Vec::new();
    let mut left_out = 0;
    let mut max_id = 0;
    let listed = root.get("items").and_then(Json::as_arr).unwrap_or(&[]);
    for (n, j) in listed.iter().enumerate() {
        if n >= MAX_ITEMS_READ {
            left_out += listed.len() - n;
            break;
        }
        let Some(id) = j.get("id").and_then(Json::as_u64).filter(|id| *id > 0) else {
            left_out += 1;
            continue;
        };
        let item = match j.get("kind").and_then(Json::as_str) {
            Some("cutout") => {
                let rect = (f32_of(j, "x"), f32_of(j, "y"), f32_of(j, "w"), f32_of(j, "h"));
                let (Some(x), Some(y), Some(w), Some(h)) = rect else {
                    left_out += 1;
                    continue;
                };
                let Some(source) = j.get("source").and_then(source_of) else {
                    left_out += 1;
                    continue;
                };
                let image = picture_of(j, &mut budget, picture, codec);
                Item::Cutout(Cutout {
                    id,
                    rect: Rect::new(x, y, w, h),
                    image,
                    label: j.get("label").and_then(Json::as_str).unwrap_or_default().to_string(),
                    source,
                })
            }
            Some("arrow") => {
                let ends = (j.get("from").and_then(end_of), j.get("to").and_then(end_of));
                let (Some(from), Some(to)) = ends else {
                    left_out += 1;
                    continue;
                };
                Item::Arrow(Arrow {
                    id,
                    from,
                    to,
                    bend: f32_of(j, "bend").unwrap_or(0.0),
                    label: j.get("label").and_then(Json::as_str).unwrap_or_default().to_string(),
                })
            }
            _ => {
                left_out += 1;
                continue;
            }
        };
        max_id = max_id.max(id);
        items.push(item);
    }
    let view = root.get("view").map_or_else(View::default, |v| View {
        pan: Pt::new(f32_of(v, "x").unwrap_or(0.0), f32_of(v, "y").unwrap_or(0.0)),
        zoom: f32_of(v, "zoom").unwrap_or(1.0),
    });
    let next_id = root.get("next_id").and_then(Json::as_u64).unwrap_or(0).max(max_id + 1);
    Ok(Loaded {
        data: BoardData { items, view, next_id },
        workspace: root.get("workspace").and_then(Json::as_str).unwrap_or_default().to_string(),
        saved_ms: root.get("saved_ms").and_then(Json::as_u64).unwrap_or(0),
        left_out,
        unreadable_pictures: budget.unreadable,
    })
}

/// FNV-1a over `text`'s bytes.
fn fnv(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The name of the file of the board of the workspace at `root`, among the app's files: the folder's
/// own name (letters and digits only, 24 at most) and a hash of the whole path, so that two folders of
/// one name are two boards and nothing in a path can lead out of the folder the boards are kept in.
#[must_use]
pub fn board_name(root: &str) -> String {
    let path = root.replace('\\', "/");
    let path = path.trim_end_matches('/');
    let last = path.rsplit('/').next().unwrap_or("");
    let mut slug = String::new();
    for c in last.chars() {
        let c = c.to_ascii_lowercase();
        let keep = if c.is_ascii_alphanumeric() { c } else { '-' };
        if keep == '-' && (slug.is_empty() || slug.ends_with('-')) {
            continue;
        }
        slug.push(keep);
    }
    let slug: String = slug.chars().take(24).collect();
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "board" } else { slug };
    format!("boards/{slug}-{:016x}.azboard.zip", fnv(path))
}

/// The name a board file that could not be read is kept under (`now_ms`: when), so that the board
/// that replaces it never takes its place.
#[must_use]
pub fn quarantine_name(name: &str, now_ms: u64) -> String {
    format!("{name}.corrupt-{now_ms}")
}

/// One write at a time: a write asked for while one runs waits for it, and runs after it (once, whatever
/// the number of times it was asked for).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaveGate {
    in_flight: bool,
    pending: bool,
}

impl SaveGate {
    /// A write is wanted: `true` when it may start now, `false` when one runs and this one waits.
    pub fn request(&mut self) -> bool {
        if self.in_flight {
            self.pending = true;
            return false;
        }
        self.in_flight = true;
        self.pending = false;
        true
    }

    /// The running write is over: `true` when another was asked for meanwhile (call [`Self::request`]).
    pub fn finished(&mut self) -> bool {
        self.in_flight = false;
        self.pending
    }

    #[must_use]
    pub const fn is_busy(&self) -> bool {
        self.in_flight
    }
}

#[cfg(test)]
#[path = "whiteboard_store_test.rs"]
mod whiteboard_store_test;
