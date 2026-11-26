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

use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use crate::{
    whiteboard::{lines_hash, Anchor, Arrow, BoardData, Cutout, CutoutImage, End, ImageSlot, Item, ItemId, Limits, Source},
    whiteboard_base64 as b64,
    whiteboard_geom::{Pt, Rect, Side, View},
    whiteboard_git::{self as git, Host, RepoFacts},
    whiteboard_hash,
    whiteboard_history::{self as history, Brief, Changed, Code, Evidence, History, Moved, RefChange, Snapshot, Summary},
    whiteboard_json::{canonical, from_f32, from_str, from_u64, object, pretty, pretty_at, Json},
    whiteboard_relocate::RefStatus,
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

/// How the links of the code references are made: where the folder lies in the repository, the repository's web address and
/// its host, the template for a host of no known form.
struct Links<'a> {
    prefix: &'a str,
    web: Option<(&'a str, Host)>,
    template: Option<&'a str>,
}

impl<'a> Links<'a> {
    fn of_meta(meta: &'a Meta) -> Self {
        let repo = meta.repo.as_ref();
        Self {
            prefix: repo.map_or("", |r| r.prefix.as_str()),
            web: repo.and_then(|r| r.web_url.as_deref().map(|w| (w, r.host))),
            template: meta.template.as_deref(),
        }
    }

    fn of_code(code: &'a Code, template: Option<&'a str>) -> Self {
        Self { prefix: code.prefix.as_str(), web: code.web_url.as_deref().map(|w| (w, code.host)), template }
    }
}

fn source_json(s: &Source, links: &Links<'_>) -> Json {
    let path = join(links.prefix, &s.file);
    let mut f = vec![("path", from_str(&path)), ("line_start", from_u64(u64::from(s.first_line))), ("line_end", from_u64(u64::from(s.last_line)))];
    opt_str(&mut f, "hash", Some(&s.sha256));
    opt_str(&mut f, "rev", Some(&s.rev));
    if let (Some((web, host)), true) = (links.web, git::is_hash(&s.rev)) {
        let link = git::permalink(web, host, &s.rev, &path, s.first_line, s.last_line, links.template);
        opt_str(&mut f, "permalink", link.as_deref());
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

/// The key a cutout's picture has among the document's assets.
fn asset_key(id: ItemId) -> String {
    format!("img-{id}")
}

/// A picture among the document's assets (once, whatever the number of items and snapshots that name it), and its file when
/// it is a file of its own.
fn add_asset(assets: &mut Vec<(String, Json)>, files: &mut Vec<Entry>, key: &str, img: &CutoutImage, pictures: &Pictures) {
    if assets.iter().any(|(k, _)| k == key) {
        return;
    }
    let file = format!("{key}.png");
    let src = match pictures {
        Pictures::Entries => file.clone(),
        Pictures::Embedded => b64::data_uri_png(&img.png),
        Pictures::Beside(dir) => format!("{dir}/{file}"),
    };
    if !matches!(pictures, Pictures::Embedded) {
        files.push(Entry { name: file, bytes: img.png.clone() });
    }
    assets.push((
        key.to_string(),
        object(vec![
            ("type", from_str("image/png")),
            ("width", from_u64(u64::from(img.width))),
            ("height", from_u64(u64::from(img.height))),
            ("src", from_str(&src)),
        ]),
    ));
}

fn item_json(item: &Item, links: &Links<'_>, pictures: &Pictures, assets: &mut Vec<(String, Json)>, files: &mut Vec<Entry>) -> Json {
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
                    let key = asset_key(c.id);
                    add_asset(assets, files, &key, img, pictures);
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
            f.push(("source", source_json(&c.source, links)));
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

fn view_json(view: &View) -> Json {
    object(vec![("x", from_f32(view.pan.x)), ("y", from_f32(view.pan.y)), ("zoom", from_f32(view.zoom))])
}

fn canvas_json(data: &BoardData) -> Json {
    let mut f = vec![("unit", from_str("px")), ("origin", from_str("top-left")), ("view", view_json(&data.view))];
    if let Some(b) = bounds(data) {
        f.push(("bounds", object(vec![("x", from_f32(b.x)), ("y", from_f32(b.y)), ("width", from_f32(b.w)), ("height", from_f32(b.h))])));
    }
    object(f)
}

// ---- snapshots

fn brief_json(b: &Brief) -> Json {
    object(vec![("id", from_u64(b.id)), ("kind", from_str(b.kind)), ("name", from_str(&b.name))])
}

fn summary_json(s: &Summary) -> Json {
    object(vec![
        ("added", Json::Arr(s.added.iter().map(brief_json).collect())),
        ("removed", Json::Arr(s.removed.iter().map(brief_json).collect())),
        (
            "moved",
            Json::Arr(s.moved.iter().map(|m| object(vec![("id", from_u64(m.id)), ("from", point_json(m.from)), ("to", point_json(m.to))])).collect()),
        ),
        (
            "changed",
            Json::Arr(s.changed.iter().map(|c| object(vec![("id", from_u64(c.id)), ("what", Json::Arr(c.what.iter().map(|w| from_str(w)).collect()))])).collect()),
        ),
        ("references", Json::Arr(s.references.iter().map(ref_change_json).collect())),
    ])
}

fn lines_json(range: (u32, u32)) -> Json {
    Json::Arr(vec![from_u64(u64::from(range.0)), from_u64(u64::from(range.1))])
}

fn ref_change_json(r: &RefChange) -> Json {
    let mut f = vec![("id", from_u64(r.id)), ("status", from_str(r.status.name())), ("from", lines_json(r.from))];
    if r.status != RefStatus::Vanished {
        f.push(("to", lines_json(r.to)));
    }
    object(f)
}

fn code_json(c: &Code) -> Json {
    let mut repo = Vec::new();
    opt_str(&mut repo, "remote", c.remote.as_deref());
    opt_str(&mut repo, "web_url", c.web_url.as_deref());
    repo.push(("host", from_str(c.host.name())));
    let mut f = vec![("repo", object(repo))];
    if !c.prefix.is_empty() {
        f.push(("path_in_repo", from_str(&c.prefix)));
    }
    opt_str(&mut f, "rev", c.rev.as_deref());
    opt_str(&mut f, "branch", c.branch.as_deref());
    if let Some(d) = c.dirty {
        f.push(("dirty", Json::Bool(d)));
    }
    if let Some(t) = c.rev_time_ms {
        f.push(("rev_time_ms", from_u64(t)));
    }
    object(f)
}

/// The snapshot as it is written, without its hash. Its pictures are named, not carried (`image.asset`): the document's
/// assets hold them, so the text does not depend on where the pictures are.
fn snapshot_value(s: &Snapshot, template: Option<&str>) -> (Json, BTreeMap<String, Arc<CutoutImage>>) {
    let links = Links::of_code(&s.code, template);
    let (mut assets, mut files) = (Vec::new(), Vec::new());
    let items: Vec<Json> = s.data.items.iter().map(|i| item_json(i, &links, &Pictures::Entries, &mut assets, &mut files)).collect();
    let pictures: BTreeMap<String, Arc<CutoutImage>> = s
        .data
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Cutout(c) => match &c.image {
                ImageSlot::Ready(img) if !img.png.is_empty() => Some((asset_key(c.id), Arc::clone(img))),
                _ => None,
            },
            Item::Arrow(_) => None,
        })
        .collect();
    let mut f = vec![("id", from_str(&s.id))];
    if let Some(p) = &s.parent {
        f.push(("parent", from_str(p)));
    }
    f.push(("taken_ms", from_u64(s.taken_ms)));
    opt_str(&mut f, "author", s.author.as_deref());
    f.push(("code", code_json(&s.code)));
    f.push(("why", from_str(&s.why)));
    f.push(("tags", Json::Arr(s.tags.iter().map(|t| from_str(t)).collect())));
    f.push(("summary", summary_json(&s.summary)));
    if let Some(e) = &s.evidence {
        f.push((
            "evidence",
            object(vec![
                ("session", from_str(&e.session)),
                ("start_ms", from_u64(e.start_ms)),
                ("end_ms", from_u64(e.end_ms)),
                ("transcript", from_str(&e.transcript)),
            ]),
        ));
    }
    f.push(("canvas", object(vec![("view", view_json(&s.data.view))])));
    f.push(("next_id", from_u64(s.data.next_id)));
    f.push(("items", Json::Arr(items)));
    (object(f), pictures)
}

/// The hash of a snapshot's value: `sha256:` and the SHA-256 of its canonical form (RFC 8785).
#[must_use]
pub fn hash_of(value: &Json) -> String {
    whiteboard_hash::tagged(canonical(value).as_bytes())
}

/// The snapshot made final: its text written (links made with `template`), hashed, and its pictures named. What `s` holds in
/// `value`, `hash`, `text`, `verified` and `pictures` is replaced; nothing of it is changed afterwards.
#[must_use]
pub fn seal(mut s: Snapshot, template: Option<&str>) -> Snapshot {
    let (value, pictures) = snapshot_value(&s, template);
    s.hash = hash_of(&value);
    s.value = value;
    s.text = pretty_at(&snapshot_object(&s), 2);
    s.verified = true;
    s.pictures = pictures;
    s
}

/// The snapshot's object as the document holds it: its value and, last, its hash.
fn snapshot_object(s: &Snapshot) -> Json {
    match &s.value {
        Json::Obj(pairs) => {
            let mut pairs = pairs.clone();
            pairs.push(("hash".to_string(), from_str(&s.hash)));
            Json::Obj(pairs)
        }
        other => other.clone(),
    }
}

// ---- the document

/// The document of the board `data` with no history: `meta` says which board and where it lies, `pictures` where the
/// pictures are.
#[must_use]
pub fn document(data: &BoardData, meta: &Meta, pictures: &Pictures) -> Written {
    document_with(data, meta, pictures, &History::default())
}

/// The document of the board `data` and its history.
#[must_use]
pub fn document_with(data: &BoardData, meta: &Meta, pictures: &Pictures, history: &History) -> Written {
    let (mut assets, mut files) = (Vec::new(), Vec::new());
    let links = Links::of_meta(meta);
    let items: Vec<Json> = data.items.iter().map(|i| item_json(i, &links, pictures, &mut assets, &mut files)).collect();
    // The pictures of the snapshots: the ones the live board has are among the assets already.
    for s in &history.snapshots {
        for (key, img) in &s.pictures {
            add_asset(&mut assets, &mut files, key, img, pictures);
        }
    }
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
    // The snapshots are written already (each as text, once): the document is written with a mark where they go and they are put
    // in as they are.
    let mark = "__azboard_snapshots__";
    root.push(("snapshots", Json::Arr(if history.snapshots.is_empty() { Vec::new() } else { vec![from_str(mark)] })));
    let mut text = pretty(&object(root));
    if !history.snapshots.is_empty() {
        let mut list = String::with_capacity(history.bytes() + 16 * history.snapshots.len() + 8);
        list.push_str("[\n");
        for (i, s) in history.snapshots.iter().enumerate() {
            list.push_str("    ");
            list.push_str(&s.text);
            list.push_str(if i + 1 < history.snapshots.len() { ",\n" } else { "\n" });
        }
        list.push_str("  ]");
        text = text.replacen(&format!("[\"{mark}\"]"), &list, 1);
    }
    Written { text, pictures: files, assets: n_assets }
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

/// What reading a document shares between its board and its snapshots.
struct Reader<'a> {
    assets: Option<&'a Json>,
    budget: Budget<'a>,
    /// The pictures read, by asset and scale: one picture is decoded once however many items and snapshots name it.
    cache: HashMap<String, ImageSlot>,
    picture: &'a dyn Fn(&str) -> Option<Vec<u8>>,
    codec: &'a dyn ImageCodec,
}

/// Items read from a list.
struct ItemsRead {
    items: Vec<Item>,
    left_out: usize,
    max_id: u64,
    /// The asset each cutout with a picture names.
    keys: Vec<(ItemId, String)>,
}

impl Reader<'_> {
    /// The picture of the cutout `j`, and the key it names it by.
    fn slot(&mut self, j: &Json) -> (ImageSlot, Option<String>) {
        let Some(image) = j.get("image").filter(|i| !matches!(i, Json::Null)) else {
            return (
                match j.get("image_state").and_then(Json::as_str) {
                    Some("pending") => ImageSlot::Failed("not drawn when the board was saved".to_string()),
                    _ => ImageSlot::Failed(str_of(j, "image_note").unwrap_or_else(|| "no picture was kept".to_string())),
                },
                None,
            );
        };
        let Some(key) = image.get("asset").and_then(Json::as_str).map(str::to_string) else {
            return (store::failed(&mut self.budget, "the picture is not among the document's pictures"), None);
        };
        let scale = f32_of(image, "scale").filter(|s| *s > 0.0).unwrap_or(1.0);
        let cached = format!("{key}@{}", scale.to_bits());
        if let Some(slot) = self.cache.get(&cached) {
            return (slot.clone(), Some(key));
        }
        let src = self.assets.and_then(|a| a.get(&key)).and_then(|a| a.get("src")).and_then(Json::as_str);
        let slot = match src {
            None => store::failed(&mut self.budget, "the picture is not among the document's pictures"),
            Some(src) => match bytes_of(src, self.picture) {
                Ok(bytes) => store::decode_slot(bytes, scale, &mut self.budget, self.codec),
                Err(why) => store::failed(&mut self.budget, why),
            },
        };
        self.cache.insert(cached, slot.clone());
        (slot, Some(key))
    }

    /// The items of `listed` (a document's, or a snapshot's), at most `cap` of them; paths are the repository's, `prefix` is
    /// what is taken off to get the folder's.
    fn items(&mut self, listed: &[Json], prefix: &str, cap: usize) -> ItemsRead {
        let mut read = ItemsRead { items: Vec::new(), left_out: 0, max_id: 0, keys: Vec::new() };
        for (n, j) in listed.iter().enumerate() {
            if n >= cap {
                read.left_out += listed.len() - n;
                break;
            }
            let Some(id) = j.get("id").and_then(Json::as_u64).filter(|id| *id > 0) else {
                read.left_out += 1;
                continue;
            };
            let item = match j.get("kind").and_then(Json::as_str) {
                Some("cutout") => {
                    let rect = (f32_of(j, "x"), f32_of(j, "y"), f32_of(j, "width"), f32_of(j, "height"));
                    let (Some(x), Some(y), Some(w), Some(h)) = rect else {
                        read.left_out += 1;
                        continue;
                    };
                    let Some(source) = j.get("source").and_then(|s| source_of(s, prefix)) else {
                        read.left_out += 1;
                        continue;
                    };
                    let (image, key) = self.slot(j);
                    if let Some(key) = key {
                        read.keys.push((id, key));
                    }
                    Item::Cutout(Cutout {
                        id,
                        rect: Rect::new(x, y, w, h),
                        image,
                        label: clean_label(j.get("label").and_then(Json::as_str).unwrap_or_default(), MAX_TEXT_CHARS),
                        source,
                    })
                }
                Some("arrow") => {
                    let ends = (j.get("from").and_then(end_of), j.get("to").and_then(end_of));
                    let (Some(from), Some(to)) = ends else {
                        read.left_out += 1;
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
                    read.left_out += 1;
                    continue;
                }
            };
            read.max_id = read.max_id.max(id);
            read.items.push(item);
        }
        read
    }

    /// The snapshot `j`, if it is one: its items are read like the board's, its text kept as it was and hashed.
    fn snapshot(&mut self, j: &Json, items_so_far: usize) -> Option<(Snapshot, usize)> {
        let id = str_of(j, "id").filter(|i| board_id_ok(i))?;
        let code = code_of(j.get("code"));
        let room = history::MAX_HISTORY_ITEMS.saturating_sub(items_so_far).min(store::MAX_ITEMS_READ);
        let listed = j.get("items").and_then(Json::as_arr).unwrap_or(&[]);
        let read = self.items(listed, &code.prefix, room);
        let view = view_of(j.get("canvas").and_then(|c| c.get("view")));
        let next_id = j.get("next_id").and_then(Json::as_u64).unwrap_or(0).max(read.max_id + 1);
        let pictures: BTreeMap<String, Arc<CutoutImage>> = read
            .keys
            .iter()
            .filter_map(|(item, key)| match read.items.iter().find(|i| i.id() == *item) {
                Some(Item::Cutout(c)) => match &c.image {
                    ImageSlot::Ready(img) => Some((key.clone(), Arc::clone(img))),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let value = match j {
            Json::Obj(pairs) => Json::Obj(pairs.iter().filter(|(k, _)| k != "hash").cloned().collect()),
            other => other.clone(),
        };
        let stored = j.get("hash").and_then(Json::as_str).filter(|h| whiteboard_hash::is_tagged(h)).unwrap_or_default().to_string();
        let verified = !stored.is_empty() && stored == hash_of(&value);
        let held = read.items.len();
        let mut snapshot = Snapshot {
            id,
            parent: str_of(j, "parent").filter(|p| board_id_ok(p)),
            taken_ms: ms_of(j, "taken_ms"),
            author: str_of(j, "author").map(|a| clean(&a, 200)).filter(|a| !a.is_empty()),
            code,
            why: history::clean_why(j.get("why").and_then(Json::as_str).unwrap_or_default()),
            tags: history::parse_tags(&j.get("tags").and_then(Json::as_arr).map(|a| a.iter().filter_map(Json::as_str).collect::<Vec<_>>().join(",")).unwrap_or_default()),
            summary: summary_of(j.get("summary")),
            evidence: evidence_of(j.get("evidence")),
            data: BoardData { items: read.items, view, next_id },
            value,
            text: String::new(),
            hash: stored,
            verified,
            pictures,
        };
        snapshot.text = pretty_at(&snapshot_object(&snapshot), 2);
        Some((snapshot, held))
    }
}

fn view_of(v: Option<&Json>) -> View {
    v.map_or_else(View::default, |v| View {
        pan: Pt::new(f32_of(v, "x").unwrap_or(0.0), f32_of(v, "y").unwrap_or(0.0)),
        zoom: f32_of(v, "zoom").unwrap_or(1.0),
    })
}

fn code_of(c: Option<&Json>) -> Code {
    let Some(c) = c else {
        return Code::default();
    };
    let repo = c.get("repo");
    Code {
        rev: str_of(c, "rev").filter(|v| git::is_hash(v)),
        branch: str_of(c, "branch"),
        dirty: c.get("dirty").and_then(Json::as_bool),
        rev_time_ms: c.get("rev_time_ms").and_then(Json::as_u64),
        remote: repo.and_then(|r| str_of(r, "remote")).map(|u| git::public_remote(&u)),
        web_url: repo.and_then(|r| str_of(r, "web_url")).filter(|u| u.starts_with("https://") || u.starts_with("http://")),
        host: repo.and_then(|r| r.get("host")).and_then(Json::as_str).map_or(Host::Other, Host::from_name),
        prefix: c.get("path_in_repo").and_then(Json::as_str).filter(|p| safe_path(p)).unwrap_or_default().to_string(),
    }
}

fn evidence_of(e: Option<&Json>) -> Option<Evidence> {
    let e = e?;
    let transcript = e.get("transcript").and_then(Json::as_str).map(|t| clean_label(t, history::MAX_EVIDENCE_CHARS)).unwrap_or_default();
    let session = str_of(e, "session").unwrap_or_default();
    (!session.is_empty() || !transcript.is_empty()).then(|| Evidence { session, start_ms: ms_of(e, "start_ms"), end_ms: ms_of(e, "end_ms"), transcript })
}

fn briefs_of(list: Option<&Json>) -> Vec<Brief> {
    list.and_then(Json::as_arr)
        .map(|a| {
            a.iter()
                .take(store::MAX_ITEMS_READ)
                .filter_map(|b| {
                    let id = b.get("id").and_then(Json::as_u64)?;
                    let kind = if b.get("kind").and_then(Json::as_str) == Some("arrow") { "arrow" } else { "cutout" };
                    Some(Brief { id, kind, name: clean(b.get("name").and_then(Json::as_str).unwrap_or_default(), MAX_TEXT_CHARS) })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn summary_of(s: Option<&Json>) -> Summary {
    let Some(s) = s else {
        return Summary::default();
    };
    Summary {
        added: briefs_of(s.get("added")),
        removed: briefs_of(s.get("removed")),
        moved: s
            .get("moved")
            .and_then(Json::as_arr)
            .map(|a| {
                a.iter()
                    .take(store::MAX_ITEMS_READ)
                    .filter_map(|m| Some(Moved { id: m.get("id").and_then(Json::as_u64)?, from: m.get("from").and_then(point_of)?, to: m.get("to").and_then(point_of)? }))
                    .collect()
            })
            .unwrap_or_default(),
        changed: s
            .get("changed")
            .and_then(Json::as_arr)
            .map(|a| {
                a.iter()
                    .take(store::MAX_ITEMS_READ)
                    .filter_map(|c| {
                        let what: Vec<&'static str> = c
                            .get("what")
                            .and_then(Json::as_arr)
                            .map(|w| {
                                w.iter()
                                    .filter_map(Json::as_str)
                                    .filter_map(|w| ["label", "size", "reference", "ends", "bend"].into_iter().find(|k| *k == w))
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some(Changed { id: c.get("id").and_then(Json::as_u64)?, what })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        references: s
            .get("references")
            .and_then(Json::as_arr)
            .map(|a| {
                a.iter()
                    .take(store::MAX_ITEMS_READ)
                    .filter_map(|r| {
                        let status = r.get("status").and_then(Json::as_str).and_then(RefStatus::from_name).filter(|s| s.is_flag())?;
                        let lines = |key: &str| {
                            let a = r.get(key)?.as_arr()?;
                            Some((u32::try_from(a.first()?.as_u64()?).ok()?, u32::try_from(a.get(1)?.as_u64()?).ok()?))
                        };
                        let from = lines("from")?;
                        Some(RefChange { id: r.get("id").and_then(Json::as_u64)?, status, from, to: lines("to").unwrap_or(from) })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
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
    let mut reader = Reader { assets, budget: Budget::new(limits), cache: HashMap::new(), picture, codec };
    let listed = root.get("items").and_then(Json::as_arr).unwrap_or(&[]);
    let read = reader.items(listed, prefix, store::MAX_ITEMS_READ);
    let view = view_of(root.get("canvas").and_then(|c| c.get("view")));
    let next_id = root.get("next_id").and_then(Json::as_u64).unwrap_or(0).max(read.max_id + 1);
    // The history: the snapshots in the order the document has them, as many as a board keeps.
    let mut history = History::default();
    let (mut snapshots_left_out, mut held) = (0, 0);
    let snaps = root.get("snapshots").and_then(Json::as_arr).unwrap_or(&[]);
    for (n, j) in snaps.iter().enumerate() {
        // As many as a board keeps; and not every entry of a document that is all entries that cannot be used is looked at.
        if history.snapshots.len() >= history::MAX_SNAPSHOTS || n >= history::MAX_SNAPSHOTS * 4 || held >= history::MAX_HISTORY_ITEMS {
            snapshots_left_out += snaps.len() - n;
            break;
        }
        match reader.snapshot(j, held) {
            // (A history that is more text than the board's file can hold is cut here, as it would be when it was made.)
            Some((snapshot, items)) if history.get(&snapshot.id).is_none() && history.bytes() + snapshot.text.len() <= history::MAX_HISTORY_BYTES => {
                held += items;
                history.snapshots.push(snapshot);
            }
            _ => snapshots_left_out += 1,
        }
    }
    Loaded {
        data: BoardData { items: read.items, view, next_id },
        meta,
        history,
        snapshots_left_out,
        left_out: read.left_out,
        unreadable_pictures: reader.budget.unreadable(),
    }
}

#[cfg(test)]
#[path = "whiteboard_azboard_test.rs"]
mod whiteboard_azboard_test;
