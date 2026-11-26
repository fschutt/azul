//! The whiteboard's file: what a board is kept as, and what a file is read back into. Pure - the
//! bytes in, the bytes out; the zip around them and the PNG codec are azul's and live in the glue
//! (`whiteboard_archive`), the disk is the data drive's.
//!
//! A board is a set of entries: `board.json` (the document, `azboard/1`: [`crate::whiteboard_azboard`]) and one
//! `img-<id>.png` for each cutout that has a picture, written as it was made (the PNG is encoded once, when the cutout
//! is captured). The document is the publishable one: the zip is only the way to keep it with its pictures. Reading is
//! tolerant and bounded: a key this version does not know is ignored, an item it does not understand is left out and
//! counted, a file of a newer major version is refused with a sentence, a missing or damaged picture leaves its cutout
//! with the lines it was made from, a damaged manifest is an error and never a panic, and what is read is kept inside
//! the board's caps. The format the first version wrote (`azreview-board/1`) is still read.
//!
//! Each board has a file of its own: [`board_name`] names the file of the one-board version after the workspace's
//! folder and a hash of its whole path (the first board of a workspace is kept in it still), `whiteboard_boards` names
//! the others. One write at a time ([`SaveGate`]); a file that cannot be read is kept as it is under
//! [`quarantine_name`] before the board that replaces it is written.

use crate::{
    whiteboard::{Arrow, BoardData, Cutout, ImageSlot, Item, ItemId, Limits, Source},
    whiteboard_azboard::{self as azboard, end_of, f32_of, point_of, Meta},
    whiteboard_cutout::Raster,
    whiteboard_geom::{Pt, Rect, View},
    whiteboard_history::History,
    whiteboard_json::{self as json, Json},
};

/// The format the first version wrote, which is still read.
const LEGACY_PREFIX: &str = "azreview-board/";
/// The newest major version of it, which is the only one there was.
const LEGACY_MAJOR: u32 = 1;
/// The manifest's name among the entries.
pub const MANIFEST: &str = "board.json";
/// The largest picture file that is read, bytes.
pub const MAX_PICTURE_BYTES: usize = 64 * 1024 * 1024;
/// What an archive may declare for all of its entries unpacked, over the pixels a board may hold (its PNGs are not
/// larger than their pixels) and its manifest: room for the names and the PNGs' own overhead.
const UNPACKED_SLACK: u64 = 16 * 1024 * 1024;
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

/// The entries a board is kept as: the document, then a picture for each cutout that has a PNG. `meta` says which
/// board it is and where it lies.
#[must_use]
pub fn write(data: &BoardData, meta: &Meta) -> Vec<Entry> {
    write_history(data, meta, &History::default())
}

/// The entries a board and its history are kept as.
#[must_use]
pub fn write_history(data: &BoardData, meta: &Meta, history: &History) -> Vec<Entry> {
    let written = azboard::document_with(data, meta, &azboard::Pictures::Entries, history);
    let mut entries = vec![Entry { name: MANIFEST.to_string(), bytes: written.text.into_bytes() }];
    entries.extend(written.pictures);
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
    /// What the file says besides the items: which board it is, when it was kept, where its code lies.
    pub meta: Meta,
    /// The snapshots of the board, oldest first.
    pub history: History,
    /// Snapshots of the file that could not be used (no id, an id twice, more than a board keeps).
    pub snapshots_left_out: usize,
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
    /// Its manifest is longer than any manifest may be, or the file says it unpacks to more than a board may be
    /// ([`check_unpacked`]).
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
            Self::TooLarge { bytes } => format!("This whiteboard's file is far larger than a whiteboard is ({bytes} bytes)."),
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

fn source_of(j: &Json) -> Option<Source> {
    let file = j.get("file")?.as_str().filter(|f| azboard::safe_path(f))?;
    let hash = |key: &str| -> u64 {
        j.get(key).and_then(Json::as_str).filter(|h| h.len() <= 16).and_then(|h| u64::from_str_radix(h, 16).ok()).unwrap_or(0)
    };
    let line = |key: &str| u32::try_from(j.get(key).and_then(Json::as_u64)?).ok();
    Some(Source {
        rev: String::new(),
        sha256: String::new(),
        file: file.to_string(),
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
pub struct Budget<'a> {
    limits: &'a Limits,
    pixels: u64,
    unreadable: usize,
}

impl<'a> Budget<'a> {
    #[must_use]
    pub const fn new(limits: &'a Limits) -> Self {
        Self { limits, pixels: 0, unreadable: 0 }
    }

    /// How many pictures were named and could not be had.
    #[must_use]
    pub const fn unreadable(&self) -> usize {
        self.unreadable
    }
}

/// A picture that could not be had: counted, and the reason kept with the cutout.
pub fn failed(budget: &mut Budget<'_>, why: &str) -> ImageSlot {
    budget.unreadable += 1;
    ImageSlot::Failed(why.to_string())
}

/// The picture in the PNG `bytes` (shown at `scale`), if it is one that fits what is left of the board's room.
pub fn decode_slot(bytes: Vec<u8>, scale: f32, budget: &mut Budget<'_>, codec: &dyn ImageCodec) -> ImageSlot {
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
    let Some(raster) = Raster::from_straight(decoded.width, decoded.height, decoded.rgba) else {
        return failed(budget, "the picture's pixels do not fit its size");
    };
    budget.pixels += px;
    ImageSlot::Ready(raster.into_shared_image(scale, bytes))
}

/// The picture of an item of the first version's manifest.
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
    let scale = f32_of(j, "scale").filter(|s| *s > 0.0).unwrap_or(1.0);
    decode_slot(bytes, scale, budget, codec)
}

/// The central directory of a zip archive, as far as sizes go: each entry's name and the size it declares for itself
/// unpacked. `None` for bytes that are no zip, a directory that cannot be read to its end, or a ZIP64 archive
/// (a board never needs one). Only the directory is read: nothing is unpacked.
#[must_use]
pub fn declared_sizes(zip: &[u8]) -> Option<Vec<(String, u64)>> {
    const END: u32 = 0x0605_4b50;
    const ENTRY: u32 = 0x0201_4b50;
    // `off` bytes after `at`: nothing for what lies outside the file (a directory may point anywhere).
    let u16_at = |at: usize, off: usize| -> Option<u16> {
        let from = at.checked_add(off)?;
        zip.get(from..from.checked_add(2)?).map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    let u32_at = |at: usize, off: usize| -> Option<u32> {
        let from = at.checked_add(off)?;
        zip.get(from..from.checked_add(4)?).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    // The end record is in the last 22 bytes and the comment that may follow it (at most 65535).
    let last = zip.len().checked_sub(22)?;
    let first = last.saturating_sub(65_535);
    let end = (first..=last).rev().find(|at| u32_at(*at, 0) == Some(END))?;
    let count = usize::from(u16_at(end, 10)?);
    let mut at = usize::try_from(u32_at(end, 16)?).ok()?;
    let mut sizes = Vec::with_capacity(count.min(MAX_ITEMS_READ));
    for _ in 0..count {
        if u32_at(at, 0)? != ENTRY {
            return None;
        }
        let size = u32_at(at, 24)?;
        if size == u32::MAX {
            return None;
        }
        let (name, extra, comment) = (usize::from(u16_at(at, 28)?), usize::from(u16_at(at, 30)?), usize::from(u16_at(at, 32)?));
        let name_from = at.checked_add(46)?;
        let name_bytes = zip.get(name_from..name_from.checked_add(name)?)?;
        sizes.push((String::from_utf8_lossy(name_bytes).into_owned(), u64::from(size)));
        at = name_from.checked_add(name)?.checked_add(extra)?.checked_add(comment)?;
    }
    Some(sizes)
}

/// Whether the archive declares more than a board may unpack to: an entry larger than the largest picture file
/// ([`MAX_PICTURE_BYTES`]), or all of them together larger than the pixels a board may hold (4 bytes each) and its
/// manifest. The reader unpacks every entry at once, so a file that lied about nothing but its sizes could take all
/// the memory: this looks at the directory first. `Err` holds the declared total. An archive whose directory cannot
/// be read passes: the reader says what is wrong with it.
///
/// # Errors
/// The declared sizes are beyond what a board may be.
pub fn check_unpacked(zip: &[u8], limits: &Limits) -> Result<(), u64> {
    let Some(sizes) = declared_sizes(zip) else {
        return Ok(());
    };
    let total: u64 = sizes.iter().map(|(_, size)| *size).sum();
    let room = limits.max_total_px.saturating_mul(4).saturating_add(json::MAX_TEXT as u64).saturating_add(UNPACKED_SLACK);
    if total > room || sizes.iter().any(|(_, size)| *size > MAX_PICTURE_BYTES as u64) {
        return Err(total);
    }
    Ok(())
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
    read_capped(manifest, picture, codec, limits, json::MAX_TEXT)
}

/// [`read`] for a document that may be longer than a manifest is: `max_text` bytes at most (a file that holds its
/// pictures in itself, [`crate::whiteboard_export`]).
///
/// # Errors
/// The manifest is no board file, is of a newer major version, or is longer than `max_text`.
pub fn read_capped(
    manifest: &[u8],
    picture: &dyn Fn(&str) -> Option<Vec<u8>>,
    codec: &dyn ImageCodec,
    limits: &Limits,
    max_text: usize,
) -> Result<Loaded, StoreError> {
    if manifest.len() > max_text {
        return Err(StoreError::TooLarge { bytes: manifest.len() });
    }
    let text = std::str::from_utf8(manifest).map_err(|_| StoreError::NotABoard("it is not text".to_string()))?;
    let root = json::parse_with(text, max_text).map_err(|e| StoreError::NotABoard(format!("{} (byte {})", e.why, e.at)))?;
    let Json::Obj(_) = root else {
        return Err(StoreError::NotABoard("it is not an object".to_string()));
    };
    let format = root.get("format").and_then(Json::as_str).ok_or_else(|| StoreError::NotABoard("it names no format".to_string()))?;
    let major_of = |prefix: &str| format.strip_prefix(prefix).and_then(|v| v.parse::<u32>().ok()).filter(|m| *m >= 1);
    if let Some(major) = major_of(azboard::FORMAT_PREFIX) {
        if major > azboard::FORMAT_MAJOR {
            return Err(StoreError::TooNew { found: major });
        }
        return Ok(azboard::read_root(&root, picture, codec, limits));
    }
    let Some(major) = major_of(LEGACY_PREFIX) else {
        return Err(StoreError::NotABoard(format!("its format \"{format}\" is not one of AzReview's boards")));
    };
    if major > LEGACY_MAJOR {
        return Err(StoreError::TooNew { found: major });
    }
    Ok(read_legacy(&root, picture, codec, limits))
}

/// The folder's own name of a workspace path (what the first version kept the whole path of).
pub fn folder_name(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').rsplit('/').next().unwrap_or_default().chars().filter(|c| !c.is_control()).take(200).collect()
}

/// The board the first version's manifest holds.
fn read_legacy(root: &Json, picture: &dyn Fn(&str) -> Option<Vec<u8>>, codec: &dyn ImageCodec, limits: &Limits) -> Loaded {
    let mut budget = Budget::new(limits);
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
    let meta = Meta {
        workspace_name: folder_name(root.get("workspace").and_then(Json::as_str).unwrap_or_default()),
        modified_ms: root.get("saved_ms").and_then(Json::as_u64).unwrap_or(0),
        ..Meta::default()
    };
    Loaded { data: BoardData { items, view, next_id }, meta, history: History::default(), snapshots_left_out: 0, left_out, unreadable_pictures: budget.unreadable }
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
