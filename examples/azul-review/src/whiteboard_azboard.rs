//! `azboard/1`: the document a whiteboard is kept as, and published as. One JSON text per board - plain JSON types, stable
//! ids, no binary outside `data:` addresses and named files - that a person can read, a diff can follow and a web viewer
//! (an interactive architecture overview with notes and links into the code) can open. [`document`] writes it, [`read_root`]
//! reads it; `whiteboard_store` puts it in the zip a board is kept in (`board.json` and its pictures), `whiteboard_export`
//! writes it as a file to share and reads one back. The format is documented in `docs/azboard.md` and described by
//! `docs/azboard-1.schema.json`; a worked example is `docs/azboard-1.example.json`.
//!
//! - **The document.** `format`, `generator`, `board` (id, name, created and modified times), `workspace` (the folder's own
//!   name - nothing of its path - and, in a git repository, `repo`: the remote as git says it, its web address, the
//!   host, the full `rev` of HEAD, the branch, whether the tree had uncommitted changes), `author` (the setting), `canvas`
//!   (px, origin top-left, the view and the bounds of what is on it), `assets` (the pictures, by id), `items`, and
//!   `snapshots` (the history: save points with their reasons).
//! - **Items.** A cutout has its place, its label, its picture (`image.asset`) and the code it shows (`source`: the path in
//!   the repository, the lines, the SHA-256 of the covered lines, the revision they were circled at, the permalink to them
//!   at that revision, the language, the code itself); an arrow runs between two cutouts or from one to a free point.
//!   A kind this version does not know is left out and counted; a field it does not know is ignored.
//! - **Reading is tolerant and bounded.** A newer major version is refused, never rewritten; what is hostile (a path that
//!   leaves the folder, a picture too large, more items than a board holds, nesting too deep, a number that is none)
//!   is left out or pulled in, never followed.

use crate::{
    whiteboard::{lines_hash, Anchor, Arrow, BoardData, Cutout, End, ImageSlot, Item, Limits, Source},
    whiteboard_base64 as b64,
    whiteboard_geom::{Pt, Rect, Side, View},
    whiteboard_git::{self as git, Host, RepoFacts},
    whiteboard_hash,
    whiteboard_json::{from_f32, from_str, from_u64, object, pretty, Json},
    whiteboard_store::{self as store, Budget, Entry, ImageCodec, Loaded},
};

/// The format a document names, and the one this version writes.
pub const FORMAT: &str = "azboard/1";
/// What every version of the format starts with.
pub const FORMAT_PREFIX: &str = "azboard/";
/// The newest major version this version reads.
pub const FORMAT_MAJOR: u32 = 1;
/// Who wrote the document.
const GENERATOR: &str = concat!("AzReview ", env!("CARGO_PKG_VERSION"));
/// The longest a name, a branch or a label that is read is, characters.
const MAX_TEXT_CHARS: usize = 1_000;
/// The longest a path or a picture's name that is read is, bytes.
const MAX_PATH_BYTES: usize = 1_000;
/// The most pictures a document may name.
const MAX_ASSETS: usize = 5_000;
/// The most lines of code a reference carries in a document that is read (the board then keeps what its limits allow).
const MAX_CODE_LINES: usize = 2_000;
/// The most points of a lasso outline that are read.
const MAX_SHAPE_POINTS: usize = 2_000;

/// What a document says besides the items: who and what it is.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Meta {
    /// The board's id among the boards of its workspace (`b1`).
    pub board_id: String,
    pub board_name: String,
    pub created_ms: u64,
    /// When it was kept, or exported (Unix milliseconds).
    pub modified_ms: u64,
    /// The folder's own name, nothing of its path.
    pub workspace_name: String,
    /// The repository the code is in, as far as it is known.
    pub repo: Option<RepoFacts>,
    /// The name the document is written with ([`crate::whiteboard_settings`]); `None` writes none.
    pub author: Option<String>,
    /// The template of a link to code for a host of no known form.
    pub template: Option<String>,
}

/// Where the pictures of a document are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pictures {
    /// Entries of an archive beside the manifest, named `img-<id>.png`.
    Entries,
    /// In the document, as `data:image/png;base64,...`.
    Embedded,
    /// In files of a folder beside the document, `<folder>/img-<id>.png` (the folder as the document writes it, relative).
    Beside(String),
}

/// A document written: its text, and the pictures that are files of their own (for [`Pictures::Entries`] and
/// [`Pictures::Beside`], by name: `img-<id>.png`).
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    pub text: String,
    pub pictures: Vec<Entry>,
    /// How many pictures the document names.
    pub assets: usize,
}

// ---- writing

/// The language of a file by its name, for a viewer that colours code.
#[must_use]
pub fn language_of(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('/').next()?.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "m" | "mm" => "objective-c",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "toml" => "toml",
        "json" => "json",
        "md" | "markdown" => "markdown",
        "html" | "htm" => "html",
        "css" => "css",
        "sh" | "bash" | "zsh" => "shell",
        "yml" | "yaml" => "yaml",
        "xml" => "xml",
        "sql" => "sql",
        "lua" => "lua",
        "zig" => "zig",
        "wgsl" => "wgsl",
        _ => return None,
    })
}

fn join(prefix: &str, file: &str) -> String {
    if prefix.is_empty() {
        file.to_string()
    } else {
        format!("{prefix}/{file}")
    }
}

fn opt_str(fields: &mut Vec<(&'static str, Json)>, key: &'static str, value: Option<&str>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        fields.push((key, from_str(v)));
    }
}

fn hex(v: u64) -> String {
    format!("{v:016x}")
}

fn point_json(p: Pt) -> Json {
    Json::Arr(vec![from_f32(p.x), from_f32(p.y)])
}

fn end_json(end: &End) -> Json {
    match end {
        End::Free(p) => object(vec![("free", point_json(*p))]),
        End::Item { id, anchor } => match anchor {
            Anchor::Auto => object(vec![("item", from_u64(*id)), ("anchor", from_str("auto"))]),
            Anchor::Side(s) => object(vec![("item", from_u64(*id)), ("anchor", from_str("side")), ("side", from_str(s.name()))]),
            Anchor::Point { u, v } => {
                object(vec![("item", from_u64(*id)), ("anchor", from_str("point")), ("u", from_f32(*u)), ("v", from_f32(*v))])
            }
        },
    }
}

fn source_json(s: &Source, meta: &Meta) -> Json {
    let prefix = meta.repo.as_ref().map_or("", |r| r.prefix.as_str());
    let path = join(prefix, &s.file);
    let mut f = vec![("path", from_str(&path)), ("line_start", from_u64(u64::from(s.first_line))), ("line_end", from_u64(u64::from(s.last_line)))];
    opt_str(&mut f, "hash", Some(&s.sha256));
    opt_str(&mut f, "rev", Some(&s.rev));
    if let Some(repo) = &meta.repo {
        if let (Some(web), true) = (&repo.web_url, git::is_hash(&s.rev)) {
            let link = git::permalink(web, repo.host, &s.rev, &path, s.first_line, s.last_line, meta.template.as_deref());
            opt_str(&mut f, "permalink", link.as_deref());
        }
    }
    opt_str(&mut f, "language", language_of(&s.file));
    f.push(("code", Json::Arr(s.lines.iter().map(|l| from_str(l)).collect())));
    f.push((
        "azreview",
        object(vec![
            ("page", from_u64(u64::from(s.page))),
            ("shape", Json::Arr(s.shape.iter().map(|p| point_json(*p)).collect())),
            ("file_hash", from_str(&hex(s.file_hash))),
            ("range_hash", from_str(&hex(s.range_hash))),
        ]),
    ));
    object(f)
}

fn item_json(item: &Item, meta: &Meta, pictures: &Pictures, assets: &mut Vec<(String, Json)>, files: &mut Vec<Entry>) -> Json {
    match item {
        Item::Cutout(c) => {
            let mut f = vec![
                ("kind", from_str("cutout")),
                ("id", from_u64(c.id)),
                ("x", from_f32(c.rect.x)),
                ("y", from_f32(c.rect.y)),
                ("width", from_f32(c.rect.w)),
                ("height", from_f32(c.rect.h)),
            ];
            opt_str(&mut f, "label", Some(&c.label));
            match &c.image {
                ImageSlot::Ready(img) if !img.png.is_empty() => {
                    let key = format!("img-{}", c.id);
                    let src = match pictures {
                        Pictures::Entries => store::picture_name(c.id),
                        Pictures::Embedded => b64::data_uri_png(&img.png),
                        Pictures::Beside(dir) => format!("{dir}/{}", store::picture_name(c.id)),
                    };
                    if !matches!(pictures, Pictures::Embedded) {
                        files.push(Entry { name: store::picture_name(c.id), bytes: img.png.clone() });
                    }
                    assets.push((
                        key.clone(),
                        object(vec![
                            ("type", from_str("image/png")),
                            ("width", from_u64(u64::from(img.width))),
                            ("height", from_u64(u64::from(img.height))),
                            ("src", from_str(&src)),
                        ]),
                    ));
                    f.push(("image", object(vec![("asset", from_str(&key)), ("scale", from_f32(img.scale))])));
                }
                ImageSlot::Ready(_) => {
                    f.push(("image_state", from_str("failed")));
                    f.push(("image_note", from_str("no picture was kept")));
                }
                ImageSlot::Pending => f.push(("image_state", from_str("pending"))),
                ImageSlot::Failed(why) => {
                    f.push(("image_state", from_str("failed")));
                    f.push(("image_note", from_str(why)));
                }
            }
            f.push(("source", source_json(&c.source, meta)));
            object(f)
        }
        Item::Arrow(a) => {
            let mut f = vec![
                ("kind", from_str("arrow")),
                ("id", from_u64(a.id)),
                ("from", end_json(&a.from)),
                ("to", end_json(&a.to)),
                ("bend", from_f32(a.bend)),
            ];
            opt_str(&mut f, "label", Some(&a.label));
            object(f)
        }
    }
}

fn repo_json(r: &RepoFacts) -> Json {
    let mut f = Vec::new();
    opt_str(&mut f, "remote", r.remote_url.as_deref());
    opt_str(&mut f, "web_url", r.web_url.as_deref());
    f.push(("host", from_str(r.host.name())));
    opt_str(&mut f, "rev", r.rev.as_deref());
    opt_str(&mut f, "branch", r.branch.as_deref());
    if let Some(d) = r.dirty {
        f.push(("dirty", Json::Bool(d)));
    }
    object(f)
}

/// The rectangle that holds every cutout and every free end of an arrow, `None` for a board with nothing to hold.
fn bounds(data: &BoardData) -> Option<Rect> {
    let (mut lo, mut hi): (Option<Pt>, Option<Pt>) = (None, None);
    let mut take = |x: f32, y: f32| {
        lo = Some(lo.map_or(Pt::new(x, y), |p| Pt::new(p.x.min(x), p.y.min(y))));
        hi = Some(hi.map_or(Pt::new(x, y), |p| Pt::new(p.x.max(x), p.y.max(y))));
    };
    for item in &data.items {
        match item {
            Item::Cutout(c) => {
                take(c.rect.x, c.rect.y);
                take(c.rect.x + c.rect.w, c.rect.y + c.rect.h);
            }
            Item::Arrow(a) => {
                for end in [&a.from, &a.to] {
                    if let End::Free(p) = end {
                        take(p.x, p.y);
                    }
                }
            }
        }
    }
    let (lo, hi) = (lo?, hi?);
    Some(Rect::new(lo.x, lo.y, hi.x - lo.x, hi.y - lo.y))
}

fn canvas_json(data: &BoardData) -> Json {
    let mut f = vec![
        ("unit", from_str("px")),
        ("origin", from_str("top-left")),
        ("view", object(vec![("x", from_f32(data.view.pan.x)), ("y", from_f32(data.view.pan.y)), ("zoom", from_f32(data.view.zoom))])),
    ];
    if let Some(b) = bounds(data) {
        f.push(("bounds", object(vec![("x", from_f32(b.x)), ("y", from_f32(b.y)), ("width", from_f32(b.w)), ("height", from_f32(b.h))])));
    }
    object(f)
}

/// The document of the board `data`: `meta` says which board and where it lies, `pictures` where the pictures are.
#[must_use]
pub fn document(data: &BoardData, meta: &Meta, pictures: &Pictures) -> Written {
    let (mut assets, mut files) = (Vec::new(), Vec::new());
    let items: Vec<Json> = data.items.iter().map(|i| item_json(i, meta, pictures, &mut assets, &mut files)).collect();
    let mut workspace = vec![("name", from_str(&meta.workspace_name))];
    if let Some(repo) = &meta.repo {
        if !repo.prefix.is_empty() {
            workspace.push(("path_in_repo", from_str(&repo.prefix)));
        }
        workspace.push(("repo", repo_json(repo)));
    }
    let mut root = vec![
        ("format", from_str(FORMAT)),
        ("generator", from_str(GENERATOR)),
        (
            "board",
            object(vec![
                ("id", from_str(&meta.board_id)),
                ("name", from_str(&meta.board_name)),
                ("created_ms", from_u64(meta.created_ms)),
                ("modified_ms", from_u64(meta.modified_ms)),
            ]),
        ),
        ("workspace", object(workspace)),
    ];
    opt_str(&mut root, "author", meta.author.as_deref());
    root.push(("canvas", canvas_json(data)));
    root.push(("next_id", from_u64(data.next_id)));
    let n_assets = assets.len();
    root.push(("assets", Json::Obj(assets)));
    root.push(("items", Json::Arr(items)));
    root.push(("snapshots", Json::Arr(Vec::new())));
    Written { text: pretty(&object(root)), pictures: files, assets: n_assets }
}

// ---- reading

/// Whether `path` is a path of a file inside the workspace: relative, of plain segments, one line, not long. What a
/// document names as the file of a cutout is only ever such a path - a jump reads it.
#[must_use]
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path.as_bytes().get(1) != Some(&b':')
        && path.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

/// Whether `src` names a file beside the document and nothing else: [`safe_path`] and no scheme (`http:`, `file:`).
#[must_use]
pub fn safe_relative(src: &str) -> bool {
    safe_path(src) && !src.contains(':')
}

/// `text` without control characters, at most `max` characters.
fn clean(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect()
}

/// `text` as a label is: line feeds and tabs stay, every other control character goes, at most `max` characters.
fn clean_label(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t')).take(max).collect()
}

fn str_of(j: &Json, key: &str) -> Option<String> {
    j.get(key)?.as_str().map(|s| clean(s, MAX_TEXT_CHARS)).filter(|s| !s.is_empty())
}

fn ms_of(j: &Json, key: &str) -> u64 {
    j.get(key).and_then(Json::as_u64).unwrap_or(0)
}

fn board_id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn repo_of(w: &Json) -> Option<RepoFacts> {
    let r = w.get("repo")?;
    let prefix = w.get("path_in_repo").and_then(Json::as_str).filter(|p| safe_path(p)).unwrap_or_default().to_string();
    Some(RepoFacts {
        rev: str_of(r, "rev").filter(|v| git::is_hash(v)),
        branch: str_of(r, "branch"),
        remote_url: str_of(r, "remote").map(|u| git::public_remote(&u)),
        web_url: str_of(r, "web_url").filter(|u| u.starts_with("https://") || u.starts_with("http://")),
        host: r.get("host").and_then(Json::as_str).map_or(Host::Other, Host::from_name),
        author: None,
        prefix,
        dirty: r.get("dirty").and_then(Json::as_bool),
    })
}

pub(crate) fn f32_of(j: &Json, key: &str) -> Option<f32> {
    j.get(key)?.as_f32().filter(|v| v.is_finite())
}

pub(crate) fn point_of(j: &Json) -> Option<Pt> {
    let a = j.as_arr()?;
    let (x, y) = (a.first()?.as_f32()?, a.get(1)?.as_f32()?);
    (x.is_finite() && y.is_finite()).then(|| Pt::new(x, y))
}

pub(crate) fn end_of(j: &Json) -> Option<End> {
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

fn hex_of(j: &Json, key: &str) -> Option<u64> {
    j.get(key).and_then(Json::as_str).filter(|h| !h.is_empty() && h.len() <= 16).and_then(|h| u64::from_str_radix(h, 16).ok())
}

fn source_of(j: &Json, prefix: &str) -> Option<Source> {
    let path = j.get("path")?.as_str()?;
    if !safe_path(path) {
        return None;
    }
    let file = if prefix.is_empty() { path } else { path.strip_prefix(prefix).and_then(|p| p.strip_prefix('/')).unwrap_or(path) };
    let line = |key: &str| j.get(key).and_then(Json::as_u64).and_then(|n| u32::try_from(n).ok()).filter(|n| *n >= 1);
    let lines: Vec<String> = j
        .get("code")
        .and_then(Json::as_arr)
        .map(|a| a.iter().take(MAX_CODE_LINES).filter_map(|l| l.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let local = j.get("azreview");
    let (page, shape, file_hash, range_hash) = match local {
        Some(l) => (
            l.get("page").and_then(Json::as_u64).and_then(|n| u32::try_from(n).ok()).unwrap_or(0),
            l.get("shape").and_then(Json::as_arr).map(|a| a.iter().take(MAX_SHAPE_POINTS).filter_map(point_of).collect()).unwrap_or_default(),
            hex_of(l, "file_hash").unwrap_or(0),
            hex_of(l, "range_hash"),
        ),
        None => (0, Vec::new(), 0, None),
    };
    Some(Source {
        file: file.to_string(),
        page,
        first_line: line("line_start")?,
        last_line: line("line_end")?,
        shape,
        file_hash,
        range_hash: range_hash.unwrap_or_else(|| lines_hash(&lines)),
        lines,
        rev: j.get("rev").and_then(Json::as_str).filter(|r| git::is_hash(r)).unwrap_or_default().to_string(),
        sha256: j.get("hash").and_then(Json::as_str).filter(|h| whiteboard_hash::is_tagged(h)).unwrap_or_default().to_string(),
    })
}

/// The bytes a picture's `src` names: a `data:` address is decoded; a name beside the document is asked of `picture`
/// (when it is a plain relative name); anything else (another scheme, a path that leaves the folder) is nothing.
fn bytes_of(src: &str, picture: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Vec<u8>, &'static str> {
    if src.len() >= 5 && src[..5].eq_ignore_ascii_case("data:") {
        return b64::png_from_data_uri(src, store::MAX_PICTURE_BYTES).map_err(|e| match e {
            b64::Base64Error::TooLarge => "the picture's file is larger than a picture may be",
            _ => "the picture's address is no PNG",
        });
    }
    if !safe_relative(src) {
        return Err("the picture is not a file beside the document");
    }
    picture(src).ok_or("the picture is missing from the file")
}

fn picture_slot(
    j: &Json,
    assets: Option<&Json>,
    budget: &mut Budget<'_>,
    picture: &dyn Fn(&str) -> Option<Vec<u8>>,
    codec: &dyn ImageCodec,
) -> ImageSlot {
    let Some(image) = j.get("image").filter(|i| !matches!(i, Json::Null)) else {
        return match j.get("image_state").and_then(Json::as_str) {
            Some("pending") => ImageSlot::Failed("not drawn when the board was saved".to_string()),
            _ => ImageSlot::Failed(str_of(j, "image_note").unwrap_or_else(|| "no picture was kept".to_string())),
        };
    };
    let src = image.get("asset").and_then(Json::as_str).and_then(|a| assets?.get(a)).and_then(|a| a.get("src")).and_then(Json::as_str);
    let Some(src) = src else {
        return store::failed(budget, "the picture is not among the document's pictures");
    };
    let bytes = match bytes_of(src, picture) {
        Ok(b) => b,
        Err(why) => return store::failed(budget, why),
    };
    let scale = f32_of(image, "scale").filter(|s| *s > 0.0).unwrap_or(1.0);
    store::decode_slot(bytes, scale, budget, codec)
}

/// The board a document holds (`root` is the document, its `format` already known to be one of `azboard/` whose major
/// version this version reads), with the pictures `picture` gives by name, decoded by `codec`, kept inside `limits`.
/// The board is not yet made consistent: [`crate::whiteboard::Board::from_data`] does that.
#[must_use]
pub fn read_root(root: &Json, picture: &dyn Fn(&str) -> Option<Vec<u8>>, codec: &dyn ImageCodec, limits: &Limits) -> Loaded {
    let empty = Json::Obj(Vec::new());
    let (board, workspace) = (root.get("board").unwrap_or(&empty), root.get("workspace").unwrap_or(&empty));
    let repo = repo_of(workspace);
    let prefix = repo.as_ref().map_or("", |r| r.prefix.as_str());
    let meta = Meta {
        board_id: str_of(board, "id").filter(|i| board_id_ok(i)).unwrap_or_default(),
        board_name: str_of(board, "name").unwrap_or_default(),
        created_ms: ms_of(board, "created_ms"),
        modified_ms: ms_of(board, "modified_ms"),
        workspace_name: str_of(workspace, "name").unwrap_or_default(),
        author: str_of(root, "author").map(|a| clean(&a, 200)).filter(|a| !a.is_empty()),
        template: None,
        repo: repo.clone(),
    };
    let assets = root.get("assets").filter(|a| matches!(a, Json::Obj(p) if p.len() <= MAX_ASSETS));
    let mut budget = Budget::new(limits);
    let mut items = Vec::new();
    let mut left_out = 0;
    let mut max_id = 0;
    let listed = root.get("items").and_then(Json::as_arr).unwrap_or(&[]);
    for (n, j) in listed.iter().enumerate() {
        if n >= store::MAX_ITEMS_READ {
            left_out += listed.len() - n;
            break;
        }
        let Some(id) = j.get("id").and_then(Json::as_u64).filter(|id| *id > 0) else {
            left_out += 1;
            continue;
        };
        let item = match j.get("kind").and_then(Json::as_str) {
            Some("cutout") => {
                let rect = (f32_of(j, "x"), f32_of(j, "y"), f32_of(j, "width"), f32_of(j, "height"));
                let (Some(x), Some(y), Some(w), Some(h)) = rect else {
                    left_out += 1;
                    continue;
                };
                let Some(source) = j.get("source").and_then(|s| source_of(s, prefix)) else {
                    left_out += 1;
                    continue;
                };
                let image = picture_slot(j, assets, &mut budget, picture, codec);
                Item::Cutout(Cutout { id, rect: Rect::new(x, y, w, h), image, label: clean_label(j.get("label").and_then(Json::as_str).unwrap_or_default(), MAX_TEXT_CHARS), source })
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
                    label: clean_label(j.get("label").and_then(Json::as_str).unwrap_or_default(), MAX_TEXT_CHARS),
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
    let view = root.get("canvas").and_then(|c| c.get("view")).map_or_else(View::default, |v| View {
        pan: Pt::new(f32_of(v, "x").unwrap_or(0.0), f32_of(v, "y").unwrap_or(0.0)),
        zoom: f32_of(v, "zoom").unwrap_or(1.0),
    });
    let next_id = root.get("next_id").and_then(Json::as_u64).unwrap_or(0).max(max_id + 1);
    Loaded { data: BoardData { items, view, next_id }, meta, left_out, unreadable_pictures: budget.unreadable() }
}

#[cfg(test)]
#[path = "whiteboard_azboard_test.rs"]
mod whiteboard_azboard_test;
