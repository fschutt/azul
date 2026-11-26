//! `azboard/1`, the document a board is kept and published as: what it holds, that a board written is a board read back
//! whole in each way the pictures can go, what a link to code is made of, and that what is hostile or from a newer
//! version is left out or refused and never followed. No window, no azul: the PNG codec is a stand-in that keeps the
//! pixels in the clear.

use std::{collections::BTreeMap, sync::Arc};

use super::*;
use crate::{
    whiteboard::{Cutout, CutoutImage, LIMITS},
    whiteboard_json::parse,
    whiteboard_store::{read, read_capped, DecodedPng, StoreError, MANIFEST},
};

const REV: &str = "0123456789abcdef0123456789abcdef01234567";
const OLD_REV: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn fake_png(w: u32, h: u32, straight: &[u8]) -> Vec<u8> {
    let mut v = w.to_le_bytes().to_vec();
    v.extend_from_slice(&h.to_le_bytes());
    v.extend_from_slice(straight);
    v
}

struct Codec;

impl ImageCodec for Codec {
    fn decode_png(&self, png: &[u8]) -> Result<DecodedPng, String> {
        if png.len() < 8 {
            return Err("this is no picture".to_string());
        }
        let w = u32::from_le_bytes([png[0], png[1], png[2], png[3]]);
        let h = u32::from_le_bytes([png[4], png[5], png[6], png[7]]);
        if png.len() - 8 != (w as usize) * (h as usize) * 4 {
            return Err("the picture is cut short".to_string());
        }
        Ok(DecodedPng { width: w, height: h, rgba: png[8..].to_vec() })
    }
}

fn picture(w: u32, h: u32, rgb: u8) -> Arc<CutoutImage> {
    let straight: Vec<u8> = (0..w * h).flat_map(|_| [rgb, rgb / 2, 7, 255]).collect();
    Arc::new(CutoutImage { width: w, height: h, scale: 2.0, rgba: straight.clone(), png: fake_png(w, h, &straight) })
}

fn source(file: &str, first: u32, last: u32, rev: &str) -> Source {
    let lines = vec!["fn \"quoted\" () {}".to_string(), "    tab\tand ünïcode ✓".to_string()];
    Source {
        file: file.to_string(),
        page: 3,
        first_line: first,
        last_line: last,
        shape: vec![Pt::new(10.5, 10.25), Pt::new(90.0, 10.0), Pt::new(90.0, 50.75)],
        file_hash: 0xdead_beef_cafe_f00d,
        range_hash: u64::MAX,
        sha256: whiteboard_hash::lines_tagged(&lines),
        lines,
        rev: rev.to_string(),
    }
}

fn cutout(id: u64, x: f32, label: &str, image: ImageSlot, src: Source) -> Item {
    Item::Cutout(Cutout { id, rect: Rect::new(x, 20.5, 200.0, 100.0), image, label: label.to_string(), source: src })
}

fn sample() -> BoardData {
    BoardData {
        items: vec![
            cutout(1, 0.0, "the parser\nof two lines", ImageSlot::Ready(picture(8, 4, 200)), source("src/lib.rs", 10, 20, REV)),
            cutout(2, 400.0, "", ImageSlot::Ready(picture(6, 3, 90)), source("src/main.rs", 5, 5, OLD_REV)),
            cutout(3, 800.0, "never drawn", ImageSlot::Failed("the font was missing".to_string()), source("README.md", 1, 3, "")),
            Item::Arrow(Arrow { id: 10, from: End::item(1), to: End::item(2), bend: 0.25, label: "calls".to_string() }),
            Item::Arrow(Arrow {
                id: 11,
                from: End::Item { id: 2, anchor: Anchor::Side(Side::Left) },
                to: End::Item { id: 3, anchor: Anchor::Point { u: 0.25, v: 0.75 } },
                bend: -0.5,
                label: String::new(),
            }),
            Item::Arrow(Arrow { id: 12, from: End::item(3), to: End::Free(Pt::new(-30.5, 40.25)), bend: 0.0, label: "to nowhere".to_string() }),
        ],
        view: View { pan: Pt::new(-120.5, 40.0), zoom: 1.5 },
        next_id: 13,
    }
}

fn repo(prefix: &str) -> RepoFacts {
    RepoFacts {
        rev: Some(REV.to_string()),
        branch: Some("main".to_string()),
        remote_url: Some("git@github.com:fschutt/azul.git".to_string()),
        web_url: Some("https://github.com/fschutt/azul".to_string()),
        host: Host::GitHub,
        author: Some("must not be written here".to_string()),
        prefix: prefix.to_string(),
        dirty: Some(false),
    }
}

fn meta() -> Meta {
    Meta {
        board_id: "b3".to_string(),
        board_name: "Architecture".to_string(),
        created_ms: 1_700_000_000_000,
        modified_ms: 1_700_000_100_000,
        workspace_name: "azul".to_string(),
        repo: Some(repo("")),
        author: Some("Ada Lovelace".to_string()),
        template: None,
    }
}

fn json_of(w: &Written) -> Json {
    parse(&w.text).expect("the document is JSON")
}

fn files(w: &Written) -> BTreeMap<String, Vec<u8>> {
    w.pictures.iter().map(|e| (e.name.clone(), e.bytes.clone())).collect()
}

fn read_doc(w: &Written, limits: &Limits) -> Result<Loaded, StoreError> {
    let map = files(w);
    read(w.text.as_bytes(), &|n| map.get(n).cloned(), &Codec, limits)
}

fn items_of(j: &Json) -> &[Json] {
    j.get("items").and_then(Json::as_arr).expect("items")
}

// ---- what is written

#[test]
fn a_board_is_written_as_a_document_a_viewer_can_read_with_nothing_but_json() {
    let w = document(&sample(), &meta(), &Pictures::Entries);
    let j = json_of(&w);
    assert_eq!(j.get("format").and_then(Json::as_str), Some("azboard/1"));
    assert!(j.get("generator").and_then(Json::as_str).is_some_and(|g| g.starts_with("AzReview ")));
    let board = j.get("board").expect("board");
    assert_eq!(board.get("id").and_then(Json::as_str), Some("b3"));
    assert_eq!(board.get("name").and_then(Json::as_str), Some("Architecture"));
    assert_eq!(board.get("created_ms").and_then(Json::as_u64), Some(1_700_000_000_000));
    assert_eq!(board.get("modified_ms").and_then(Json::as_u64), Some(1_700_000_100_000));
    assert_eq!(j.get("author").and_then(Json::as_str), Some("Ada Lovelace"));
    let ws = j.get("workspace").expect("workspace");
    assert_eq!(ws.get("name").and_then(Json::as_str), Some("azul"));
    assert!(ws.get("path_in_repo").is_none(), "the repository's own folder has none");
    let r = ws.get("repo").expect("repo");
    assert_eq!(r.get("remote").and_then(Json::as_str), Some("git@github.com:fschutt/azul.git"));
    assert_eq!(r.get("web_url").and_then(Json::as_str), Some("https://github.com/fschutt/azul"));
    assert_eq!(r.get("host").and_then(Json::as_str), Some("github"));
    assert_eq!(r.get("rev").and_then(Json::as_str), Some(REV));
    assert_eq!(r.get("branch").and_then(Json::as_str), Some("main"));
    assert_eq!(r.get("dirty").and_then(Json::as_bool), Some(false));
    let canvas = j.get("canvas").expect("canvas");
    assert_eq!(canvas.get("unit").and_then(Json::as_str), Some("px"));
    assert_eq!(canvas.get("origin").and_then(Json::as_str), Some("top-left"));
    assert_eq!(canvas.get("view").and_then(|v| v.get("zoom")).and_then(Json::as_f32), Some(1.5));
    let b = canvas.get("bounds").expect("bounds");
    assert_eq!(b.get("x").and_then(Json::as_f32), Some(-30.5), "the free end of the last arrow is on the canvas too");
    assert_eq!(b.get("width").and_then(Json::as_f32), Some(1000.0 + 30.5));
    assert_eq!(j.get("next_id").and_then(Json::as_u64), Some(13));
    assert_eq!(j.get("snapshots").and_then(Json::as_arr).map(<[_]>::len), Some(0), "the history is there, empty");
    assert_eq!(items_of(&j).len(), 6);
}

#[test]
fn a_cutout_has_its_place_its_picture_and_the_code_it_shows_with_a_link_to_it() {
    let w = document(&sample(), &meta(), &Pictures::Entries);
    let j = json_of(&w);
    let first = &items_of(&j)[0];
    assert_eq!(first.get("kind").and_then(Json::as_str), Some("cutout"));
    assert_eq!(first.get("id").and_then(Json::as_u64), Some(1));
    for (k, v) in [("x", 0.0), ("y", 20.5), ("width", 200.0), ("height", 100.0)] {
        assert_eq!(first.get(k).and_then(Json::as_f32), Some(v), "{k}");
    }
    assert_eq!(first.get("label").and_then(Json::as_str), Some("the parser\nof two lines"));
    let image = first.get("image").expect("image");
    assert_eq!(image.get("asset").and_then(Json::as_str), Some("img-1"));
    assert_eq!(image.get("scale").and_then(Json::as_f32), Some(2.0));
    let asset = j.get("assets").and_then(|a| a.get("img-1")).expect("the asset");
    assert_eq!(asset.get("type").and_then(Json::as_str), Some("image/png"));
    assert_eq!((asset.get("width").and_then(Json::as_u64), asset.get("height").and_then(Json::as_u64)), (Some(8), Some(4)));
    assert_eq!(asset.get("src").and_then(Json::as_str), Some("img-1.png"), "in the archive: the entry's name");
    let s = first.get("source").expect("source");
    assert_eq!(s.get("path").and_then(Json::as_str), Some("src/lib.rs"));
    assert_eq!((s.get("line_start").and_then(Json::as_u64), s.get("line_end").and_then(Json::as_u64)), (Some(10), Some(20)));
    assert_eq!(s.get("rev").and_then(Json::as_str), Some(REV));
    assert_eq!(s.get("language").and_then(Json::as_str), Some("rust"));
    assert_eq!(s.get("permalink").and_then(Json::as_str), Some(format!("https://github.com/fschutt/azul/blob/{REV}/src/lib.rs#L10-L20").as_str()));
    assert!(s.get("hash").and_then(Json::as_str).is_some_and(whiteboard_hash::is_tagged));
    assert_eq!(s.get("code").and_then(Json::as_arr).map(<[_]>::len), Some(2));
    // What is the app's own is under its own name, for a round trip, and a viewer can leave it alone.
    let local = s.get("azreview").expect("the app's own");
    assert_eq!(local.get("page").and_then(Json::as_u64), Some(3));
    assert_eq!(local.get("range_hash").and_then(Json::as_str), Some("ffffffffffffffff"));
    // The arrows say where from and where to.
    let arrow = &items_of(&j)[3];
    assert_eq!(arrow.get("kind").and_then(Json::as_str), Some("arrow"));
    assert_eq!(arrow.get("from").and_then(|e| e.get("item")).and_then(Json::as_u64), Some(1));
    assert_eq!(arrow.get("label").and_then(Json::as_str), Some("calls"));
    let free = &items_of(&j)[5];
    assert_eq!(free.get("to").and_then(|e| e.get("free")).and_then(Json::as_arr).map(<[_]>::len), Some(2));
}

#[test]
fn a_link_names_the_revision_the_lines_were_circled_at_and_the_path_in_the_repository() {
    let mut m = meta();
    m.repo = Some(repo("examples/azul-review"));
    let w = document(&sample(), &m, &Pictures::Entries);
    let j = json_of(&w);
    assert_eq!(j.get("workspace").and_then(|w| w.get("path_in_repo")).and_then(Json::as_str), Some("examples/azul-review"));
    let link = |n: usize| items_of(&j)[n].get("source").and_then(|s| s.get("permalink")).and_then(Json::as_str).map(str::to_string);
    assert_eq!(link(0), Some(format!("https://github.com/fschutt/azul/blob/{REV}/examples/azul-review/src/lib.rs#L10-L20")));
    // A reference made at another revision keeps its own: the link goes on showing what was circled.
    assert_eq!(link(1), Some(format!("https://github.com/fschutt/azul/blob/{OLD_REV}/examples/azul-review/src/main.rs#L5")));
    // One whose revision is not known has no link (the lines at HEAD may not be the lines that were circled).
    assert_eq!(link(2), None);
    let path = |n: usize| items_of(&j)[n].get("source").and_then(|s| s.get("path")).and_then(Json::as_str).map(str::to_string);
    assert_eq!(path(0).as_deref(), Some("examples/azul-review/src/lib.rs"), "the path in the repository, not in the folder");
    // No repository: no link, no revision of the workspace; the path and the lines are all there is.
    m.repo = None;
    let j = json_of(&document(&sample(), &m, &Pictures::Entries));
    assert!(j.get("workspace").and_then(|w| w.get("repo")).is_none());
    assert!(items_of(&j)[0].get("source").and_then(|s| s.get("permalink")).is_none());
    // A host of no known form: the template of the settings, else the GitHub form.
    let mut other = repo("");
    other.host = Host::Other;
    other.web_url = Some("https://git.example.org/team/proj".to_string());
    m.repo = Some(other);
    let plain = json_of(&document(&sample(), &m, &Pictures::Entries));
    assert_eq!(
        items_of(&plain)[0].get("source").and_then(|s| s.get("permalink")).and_then(Json::as_str),
        Some(format!("https://git.example.org/team/proj/blob/{REV}/src/lib.rs#L10-L20").as_str())
    );
    m.template = Some("{web}/-/raw/{rev}/{path}?from={start}&to={end}".to_string());
    let templated = json_of(&document(&sample(), &m, &Pictures::Entries));
    assert_eq!(
        items_of(&templated)[0].get("source").and_then(|s| s.get("permalink")).and_then(Json::as_str),
        Some(format!("https://git.example.org/team/proj/-/raw/{REV}/src/lib.rs?from=10&to=20").as_str())
    );
}

#[test]
fn the_document_names_the_folder_and_the_git_name_and_nothing_else_that_is_personal() {
    let mut m = meta();
    m.workspace_name = "azul".to_string();
    let text = document(&sample(), &m, &Pictures::Entries).text;
    assert!(!text.contains("/Users/") && !text.contains("/home/") && !text.contains("C:\\"), "no path of the machine");
    assert!(!text.contains("must not be written here"), "the git name is the author setting's to give, not the repository facts'");
    assert!(!text.contains('@') || text.contains("git@github.com"), "no e-mail address, only the remote's own");
    m.author = None;
    let j = json_of(&document(&sample(), &m, &Pictures::Entries));
    assert!(j.get("author").is_none(), "no name when none is given");
}

#[test]
fn the_pictures_go_where_they_are_told_to() {
    let w = document(&sample(), &meta(), &Pictures::Entries);
    assert_eq!(w.pictures.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["img-1.png", "img-2.png"], "the third has none");
    assert_eq!(w.pictures[0].bytes, picture(8, 4, 200).png, "as they were made: no new encode");
    assert_eq!(w.assets, 2);
    let beside = document(&sample(), &meta(), &Pictures::Beside("Parser.assets".to_string()));
    let j = json_of(&beside);
    assert_eq!(j.get("assets").and_then(|a| a.get("img-2")).and_then(|a| a.get("src")).and_then(Json::as_str), Some("Parser.assets/img-2.png"));
    assert_eq!(beside.pictures.len(), 2);
    let embedded = document(&sample(), &meta(), &Pictures::Embedded);
    assert!(embedded.pictures.is_empty(), "nothing beside it");
    let j = json_of(&embedded);
    let src = j.get("assets").and_then(|a| a.get("img-1")).and_then(|a| a.get("src")).and_then(Json::as_str).expect("src");
    assert_eq!(b64::png_from_data_uri(src, 1 << 20), Ok(picture(8, 4, 200).png.clone()), "the picture is in the document");
}

#[test]
fn writing_a_board_twice_gives_the_same_text() {
    let a = document(&sample(), &meta(), &Pictures::Embedded);
    let b = document(&sample(), &meta(), &Pictures::Embedded);
    assert_eq!(a, b);
    assert!(a.text.ends_with("}\n"));
}

// ---- what is read back

fn same_board(loaded: &Loaded, board: &BoardData) {
    assert_eq!(loaded.data.items, board.items, "the same items, in the same order, with the same everything");
    assert_eq!(loaded.data.view, board.view);
    assert_eq!(loaded.data.next_id, board.next_id);
    assert_eq!((loaded.left_out, loaded.unreadable_pictures), (0, 0));
}

#[test]
fn a_board_written_into_an_archive_is_read_back_whole() {
    let board = sample();
    let loaded = read_doc(&document(&board, &meta(), &Pictures::Entries), &LIMITS).expect("read");
    same_board(&loaded, &board);
}

#[test]
fn a_board_with_its_pictures_in_the_document_is_read_back_whole() {
    let board = sample();
    let w = document(&board, &meta(), &Pictures::Embedded);
    let loaded = read(w.text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read: nothing was asked of the files");
    same_board(&loaded, &board);
}

#[test]
fn a_board_with_its_pictures_in_a_folder_beside_it_is_read_back_whole() {
    let board = sample();
    let w = document(&board, &meta(), &Pictures::Beside("Parser.assets".to_string()));
    let map: BTreeMap<String, Vec<u8>> = w.pictures.iter().map(|e| (format!("Parser.assets/{}", e.name), e.bytes.clone())).collect();
    let loaded = read(w.text.as_bytes(), &|n| map.get(n).cloned(), &Codec, &LIMITS).expect("read");
    same_board(&loaded, &board);
}

#[test]
fn what_the_document_says_of_the_board_and_the_repository_is_read_back() {
    let w = document(&sample(), &meta(), &Pictures::Entries);
    let loaded = read_doc(&w, &LIMITS).expect("read");
    let m = &loaded.meta;
    assert_eq!((m.board_id.as_str(), m.board_name.as_str()), ("b3", "Architecture"));
    assert_eq!((m.created_ms, m.modified_ms), (1_700_000_000_000, 1_700_000_100_000));
    assert_eq!(m.workspace_name, "azul");
    assert_eq!(m.author.as_deref(), Some("Ada Lovelace"));
    let r = m.repo.as_ref().expect("the repository");
    assert_eq!(r.rev.as_deref(), Some(REV));
    assert_eq!(r.branch.as_deref(), Some("main"));
    assert_eq!(r.web_url.as_deref(), Some("https://github.com/fschutt/azul"));
    assert_eq!((r.host, r.dirty), (Host::GitHub, Some(false)));
    // In a folder of the repository the paths in the document are the repository's, and the board's are the folder's.
    let mut sub = meta();
    sub.repo = Some(repo("examples/azul-review"));
    let loaded = read_doc(&document(&sample(), &sub, &Pictures::Entries), &LIMITS).expect("read");
    same_board(&loaded, &sample());
    assert_eq!(loaded.meta.repo.as_ref().map(|r| r.prefix.as_str()), Some("examples/azul-review"));
}

#[test]
fn a_document_of_another_tool_with_only_what_it_must_say_is_a_board() {
    let text = r#"{"format":"azboard/1","items":[
        {"kind":"cutout","id":4,"x":10,"y":20,"width":300,"height":120,
         "source":{"path":"src/a.rs","line_start":3,"line_end":9,"code":["fn a() {","}"]}},
        {"kind":"arrow","id":5,"from":{"item":4},"to":{"free":[0,0]}}
    ]}"#;
    let loaded = read(text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    assert_eq!((loaded.left_out, loaded.unreadable_pictures), (0, 0));
    assert_eq!(loaded.data.next_id, 6, "past the largest id");
    let Item::Cutout(c) = &loaded.data.items[0] else { panic!("a cutout") };
    assert_eq!((c.source.file.as_str(), c.source.first_line, c.source.last_line), ("src/a.rs", 3, 9));
    assert_eq!(c.source.range_hash, crate::whiteboard::lines_hash(&c.source.lines), "the hash of what the document holds of the lines");
    assert_eq!((c.source.page, c.source.file_hash, c.source.rev.as_str(), c.source.sha256.as_str()), (0, 0, "", ""));
    assert!(matches!(&c.image, ImageSlot::Failed(why) if why.contains("no picture")), "{:?}", c.image);
    assert_eq!(loaded.meta, Meta::default());
}

#[test]
fn unknown_keys_are_ignored_and_an_unknown_kind_is_left_out_and_counted() {
    let mut text = document(&sample(), &meta(), &Pictures::Embedded).text;
    text = text.replacen("{\n      \"kind\": \"arrow\"", "{\n      \"kind\": \"sticker\", \"id\": 99, \"shine\": true\n    },\n    {\n      \"future\": [1, 2],\n      \"kind\": \"arrow\"", 1);
    text = text.replacen("\"format\"", "\"made_by\": \"a newer minor\",\n  \"format\"", 1);
    let loaded = read(text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.left_out, 1);
    assert_eq!(loaded.data.items.len(), 6, "the six items of the board");
}

#[test]
fn an_item_missing_what_it_cannot_do_without_is_left_out_and_counted() {
    let text = r#"{"format":"azboard/1","items":[
        {"kind":"cutout","id":1,"x":0,"y":0,"width":50,"height":20,"source":{"path":"a.rs","line_start":1,"line_end":2}},
        {"kind":"cutout","id":2,"y":0,"width":50,"height":20,"source":{"path":"a.rs","line_start":1,"line_end":2}},
        {"kind":"cutout","id":3,"x":0,"y":0,"width":50,"height":20},
        {"kind":"cutout","x":0,"y":0,"width":50,"height":20,"source":{"path":"a.rs","line_start":1,"line_end":2}},
        {"kind":"cutout","id":5,"x":"left","y":0,"width":50,"height":20,"source":{"path":"a.rs","line_start":1,"line_end":2}},
        {"kind":"cutout","id":6,"x":0,"y":0,"width":50,"height":20,"source":{"path":"a.rs","line_start":0,"line_end":2}},
        {"kind":"cutout","id":7,"x":0,"y":0,"width":50,"height":20,"source":{"path":"a.rs","line_start":1}},
        {"kind":"arrow","id":8,"from":{"item":1},"to":{"nothing":true}},
        {"kind":"arrow","id":9,"from":{"item":1,"anchor":"auto"},"to":{"free":[5,6]},"bend":"much"}
    ]}"#;
    let loaded = read(text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.data.items.iter().map(Item::id).collect::<Vec<_>>(), vec![1, 9]);
    assert_eq!(loaded.left_out, 7);
    let Item::Arrow(a) = &loaded.data.items[1] else { panic!("an arrow") };
    assert_eq!(a.bend, 0.0, "a bend that is no number is straight");
    assert_eq!(a.to, End::Free(Pt::new(5.0, 6.0)));
}

#[test]
fn a_path_that_leaves_the_workspace_is_not_a_cutout() {
    let cut = |path: &str| {
        format!(
            r#"{{"format":"azboard/1","items":[{{"kind":"cutout","id":1,"x":0,"y":0,"width":50,"height":20,"source":{{"path":{},"line_start":1,"line_end":2}}}}]}}"#,
            crate::whiteboard_json::pretty(&Json::Str(path.to_string())).trim_end()
        )
    };
    for bad in ["../secret.rs", "/etc/passwd", "a/../../b.rs", "a\\b.rs", "C:/Windows/x.rs", "a//b.rs", "./a.rs", "", "a/./b.rs", "tab\there.rs", &"x/".repeat(600)] {
        let loaded = read(cut(bad).as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
        assert_eq!((loaded.data.items.len(), loaded.left_out), (0, 1), "{bad:?}");
    }
    for good in ["a.rs", "src/a b.rs", "dé/ü.rs", ".github/workflows/x.yml"] {
        let loaded = read(cut(good).as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
        assert_eq!(loaded.data.items.len(), 1, "{good:?}");
    }
    assert!(safe_path("a/b.rs") && !safe_path("..") && !safe_path("a/..") && !safe_path("a/") && !safe_path("/a"));
    assert!(safe_relative("Parser.assets/img-1.png") && !safe_relative("http://x/y.png") && !safe_relative("file:///etc/passwd") && !safe_relative("../x.png"));
}

#[test]
fn a_picture_is_only_taken_from_where_a_document_may_take_one_from() {
    let board = sample();
    let w = document(&board, &meta(), &Pictures::Embedded);
    // Point the first picture at places a hostile document would: none of them is asked of the files, none is fetched.
    let asked = std::cell::RefCell::new(Vec::<String>::new());
    for src in ["http://example.org/x.png", "https://example.org/x.png", "file:///etc/passwd", "../../etc/passwd", "/etc/passwd", "C:\\x.png", "data:image/jpeg;base64,AAAA", "data:image/png;base64,@@@@", "data:text/html,<b>"] {
        let mut j = json_of(&w);
        let Json::Obj(top) = &mut j else { panic!() };
        let assets = top.iter_mut().find(|(k, _)| k == "assets").map(|(_, v)| v).expect("assets");
        let Json::Obj(pairs) = assets else { panic!() };
        let Json::Obj(first) = &mut pairs[0].1 else { panic!() };
        first.iter_mut().find(|(k, _)| k == "src").expect("src").1 = Json::Str(src.to_string());
        let text = crate::whiteboard_json::pretty(&j);
        let loaded = read(text.as_bytes(), &|n| {
            asked.borrow_mut().push(n.to_string());
            None
        }, &Codec, &LIMITS)
        .expect("read");
        assert_eq!(loaded.unreadable_pictures, 1, "{src:?}");
        let Item::Cutout(c) = &loaded.data.items[0] else { panic!("a cutout") };
        assert!(matches!(&c.image, ImageSlot::Failed(_)), "{src:?}");
        assert_eq!(c.source.lines.len(), 2, "the lines it was made from are what is shown");
    }
    assert!(asked.borrow().is_empty(), "no file was asked for: {:?}", asked.borrow());
}

#[test]
fn a_picture_that_is_missing_or_damaged_leaves_the_cutout_with_the_lines_and_a_reason() {
    let w = document(&sample(), &meta(), &Pictures::Entries);
    let mut map = files(&w);
    map.remove("img-1.png");
    map.get_mut("img-2.png").map(|b| b.truncate(20));
    let loaded = read(w.text.as_bytes(), &|n| map.get(n).cloned(), &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.unreadable_pictures, 2);
    for (index, why) in [(0, "missing"), (1, "cut short")] {
        let Item::Cutout(c) = &loaded.data.items[index] else { panic!("a cutout") };
        assert!(matches!(&c.image, ImageSlot::Failed(w) if w.contains(why)), "{:?}", c.image);
    }
}

#[test]
fn a_picture_past_the_caps_is_not_loaded_and_the_cutout_stays() {
    let w = document(&sample(), &meta(), &Pictures::Entries);
    let one = Limits { max_image_px: 20, ..LIMITS };
    let loaded = read_doc(&w, &one).expect("read");
    assert_eq!(loaded.unreadable_pictures, 1, "the 32 pixel picture is over the cap of one picture");
    assert_eq!(loaded.data.items.len(), 6);
    let all = Limits { max_total_px: 40, ..LIMITS };
    assert_eq!(read_doc(&w, &all).expect("read").unreadable_pictures, 1, "32 and 18 pixels do not both fit in 40");
}

#[test]
fn a_document_of_a_newer_version_is_told_apart_and_what_is_not_one_is_refused() {
    let no = |text: &str| read(text.as_bytes(), &|_| None, &Codec, &LIMITS);
    assert_eq!(no("{\"format\":\"azboard/2\",\"items\":[]}").err(), Some(StoreError::TooNew { found: 2 }));
    assert_eq!(no("{\"format\":\"azboard/9\"}").err(), Some(StoreError::TooNew { found: 9 }));
    for bad in ["{\"format\":\"azboard/x\"}", "{\"format\":\"azboard/0\"}", "{\"format\":\"azboard\"}", "{\"format\":\"excalidraw/2\"}", "{\"items\":[]}", "[1]", ""] {
        assert!(matches!(no(bad), Err(StoreError::NotABoard(_))), "{bad:?}");
    }
    assert!(no("{\"format\":\"azboard/1\"}").is_ok(), "no items is an empty board");
    assert!(no("{\"format\":\"azreview-board/1\"}").is_ok(), "the first version's format is still read");
}

#[test]
fn what_is_over_the_caps_of_a_document_is_left_out_or_pulled_in_and_never_followed() {
    // More items than are read at all.
    let mut items = String::new();
    for i in 1..=(store::MAX_ITEMS_READ + 10) {
        items.push_str(&format!("{{\"kind\":\"arrow\",\"id\":{i},\"from\":{{\"free\":[0,0]}},\"to\":{{\"free\":[1,1]}}}},"));
    }
    items.pop();
    let text = format!("{{\"format\":\"azboard/1\",\"items\":[{items}]}}");
    let loaded = read(text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.data.items.len(), store::MAX_ITEMS_READ);
    assert_eq!(loaded.left_out, 10);
    // Numbers that make no sense are pulled in by the board.
    let text = r#"{"format":"azboard/1","canvas":{"view":{"x":1e30,"y":-1e30,"zoom":-4}},"items":[
        {"kind":"cutout","id":1,"x":1e30,"y":-1e30,"width":1e9,"height":0.001,
         "source":{"path":"a.rs","line_start":50,"line_end":3,"azreview":{"shape":[[1,2],[1e40,2],[3]]}}}]}"#;
    let loaded = read(text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    let (board, _) = crate::whiteboard::Board::from_data(loaded.data, LIMITS);
    let c = board.cutout(1).expect("the cutout");
    assert!(c.rect.x <= LIMITS.max_coord && c.rect.y >= -LIMITS.max_coord);
    assert!(c.rect.w <= LIMITS.max_cutout && c.rect.h >= LIMITS.min_cutout);
    assert_eq!((c.source.first_line, c.source.last_line), (3, 50));
    assert!(c.source.shape.iter().all(|p| p.is_finite()));
    assert!(board.view().zoom >= LIMITS.zoom_min);
    // Nesting too deep, and a document too long for the cap it is read with.
    let deep = format!("{{\"format\":\"azboard/1\",\"x\":{}{}}}", "[".repeat(40), "]".repeat(40));
    assert!(matches!(read(deep.as_bytes(), &|_| None, &Codec, &LIMITS), Err(StoreError::NotABoard(_))));
    let long = vec![b' '; 5_000];
    assert!(matches!(read_capped(&long, &|_| None, &Codec, &LIMITS, 4_000), Err(StoreError::TooLarge { .. })));
}

#[test]
fn a_document_cut_short_or_damaged_anywhere_is_never_a_panic() {
    let w = document(&sample(), &meta(), &Pictures::Embedded);
    let bytes = w.text.clone().into_bytes();
    let read_it = |m: &[u8]| {
        let _ = read(m, &|_| None, &Codec, &LIMITS);
    };
    for end in (0..bytes.len()).step_by(7) {
        read_it(&bytes[..end]);
    }
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    for _ in 0..600 {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let at = (state >> 33) as usize % bytes.len();
        let mut bad = bytes.clone();
        bad[at] = (state >> 20) as u8;
        read_it(&bad);
    }
}

#[test]
fn the_first_versions_manifest_is_still_read_and_its_path_is_only_a_folder_name_now() {
    let legacy = r#"{"format":"azreview-board/1","workspace":"/Users/someone/work/azul","saved_ms":42,"next_id":3,
        "view":{"x":1,"y":2,"zoom":1.5},"items":[
        {"kind":"cutout","id":1,"x":0,"y":0,"w":50,"h":20,"label":"old","image":null,"state":"failed","why":"x",
         "source":{"file":"a.rs","first_line":1,"last_line":2,"file_hash":"00000000000000ff","range_hash":"0000000000000001","lines":["x"]}},
        {"kind":"cutout","id":2,"x":0,"y":0,"w":50,"h":20,"source":{"file":"../../etc/passwd","first_line":1,"last_line":2}}]}"#;
    let loaded = read(legacy.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.meta.workspace_name, "azul", "the name of the folder, nothing of where it was");
    assert_eq!(loaded.meta.modified_ms, 42);
    assert_eq!(loaded.data.items.len(), 1);
    assert_eq!(loaded.left_out, 1, "its path left the workspace");
    let Item::Cutout(c) = &loaded.data.items[0] else { panic!("a cutout") };
    assert_eq!((c.label.as_str(), c.source.file_hash, c.source.rev.as_str()), ("old", 0xff, ""));
    // And what is written now is the new format, whatever was read.
    let again = document(&loaded.data, &loaded.meta, &Pictures::Entries);
    assert!(again.text.contains("\"format\": \"azboard/1\""));
}

#[test]
fn the_language_of_a_file_is_told_by_its_name() {
    for (path, lang) in [("src/lib.rs", "rust"), ("a/b.PY", "python"), ("x.tsx", "tsx"), ("m.mm", "objective-c"), ("Cargo.toml", "toml"), ("a.b.c.cpp", "cpp"), ("run.sh", "shell")] {
        assert_eq!(language_of(path), Some(lang), "{path}");
    }
    for none in ["Makefile", "a/b/LICENSE", "noext.", "x.unknownext", ".gitignore"] {
        assert_eq!(language_of(none), None, "{none}");
    }
}

#[test]
fn a_full_board_is_written_and_read_back_in_a_blink() {
    let mut items = Vec::new();
    for i in 1..=200_u64 {
        items.push(cutout(i, i as f32 * 220.0, "a label of some length", ImageSlot::Ready(picture(40, 30, (i % 250) as u8)), source("src/lib.rs", 10, 20, REV)));
    }
    for i in 1..200_u64 {
        items.push(Item::Arrow(Arrow { id: 1_000 + i, from: End::item(i), to: End::item(i + 1), bend: 0.2, label: String::new() }));
    }
    let data = BoardData { items, view: View::default(), next_id: 2_000 };
    for pictures in [Pictures::Entries, Pictures::Embedded] {
        let began = std::time::Instant::now();
        let w = document(&data, &meta(), &pictures);
        let wrote = began.elapsed();
        let map = files(&w);
        let began = std::time::Instant::now();
        let loaded = read(w.text.as_bytes(), &|n| map.get(n).cloned(), &Codec, &LIMITS).expect("read");
        let read_it = began.elapsed();
        eprintln!("{pictures:?}: {} items, {} bytes: written in {wrote:?}, read in {read_it:?}", data.items.len(), w.text.len());
        assert_eq!(loaded.data.items.len(), 399);
        assert!(wrote.as_millis() < 1_000 && read_it.as_millis() < 2_000, "{wrote:?} / {read_it:?}");
    }
}

#[test]
fn the_manifest_entry_is_named_as_the_archive_expects() {
    assert_eq!(MANIFEST, "board.json");
    assert_eq!(FORMAT, "azboard/1");
    assert!(FORMAT.starts_with(FORMAT_PREFIX));
}

// ---- the example of the documentation

/// A codec for the example's pictures: any PNG is one pixel (the example's pictures are stand-ins; what is looked at is
/// the document).
struct AnyPng;

impl ImageCodec for AnyPng {
    fn decode_png(&self, png: &[u8]) -> Result<DecodedPng, String> {
        if png.starts_with(b"\x89PNG\r\n\x1a\n") {
            Ok(DecodedPng { width: 1, height: 1, rgba: vec![0, 0, 0, 255] })
        } else {
            Err("this is no PNG".to_string())
        }
    }
}

const EXAMPLE: &str = include_str!("../docs/azboard-1.example.json");

#[test]
fn the_example_board_of_the_azul_tree_is_read_whole_and_its_hashes_and_links_are_what_the_format_says() {
    let loaded = read(EXAMPLE.as_bytes(), &|_| None, &AnyPng, &LIMITS).expect("the example is a board");
    assert_eq!((loaded.left_out, loaded.unreadable_pictures), (0, 0));
    assert_eq!(loaded.data.items.len(), 9, "five cutouts and the four arrows between them");
    assert_eq!(loaded.meta.board_name, "From the DOM to the pixels");
    // Every reference: its lines hash to its hash, its link is the one the format makes of its host, revision, path and lines.
    let doc = parse(EXAMPLE).expect("JSON");
    let repo = doc.get("workspace").and_then(|w| w.get("repo")).expect("a repository");
    let (web, rev) = (repo.get("web_url").and_then(Json::as_str).expect("web"), repo.get("rev").and_then(Json::as_str).expect("rev"));
    assert!(git::is_hash(rev));
    let mut cutouts = 0;
    for item in items_of(&doc) {
        if item.get("kind").and_then(Json::as_str) != Some("cutout") {
            continue;
        }
        cutouts += 1;
        let s = item.get("source").expect("source");
        let code: Vec<String> = s.get("code").and_then(Json::as_arr).expect("code").iter().filter_map(|l| l.as_str().map(str::to_string)).collect();
        let (a, b) = (s.get("line_start").and_then(Json::as_u64).expect("start") as u32, s.get("line_end").and_then(Json::as_u64).expect("end") as u32);
        assert_eq!(code.len() as u32, b - a + 1, "the lines are the lines");
        assert_eq!(s.get("hash").and_then(Json::as_str), Some(whiteboard_hash::lines_tagged(&code).as_str()), "the hash is of the lines");
        assert_eq!(s.get("rev").and_then(Json::as_str), Some(rev), "circled at the board's revision");
        let path = s.get("path").and_then(Json::as_str).expect("path");
        assert!(safe_path(path));
        assert_eq!(s.get("permalink").and_then(Json::as_str), git::permalink(web, Host::GitHub, rev, path, a, b, None).as_deref());
        assert_eq!(s.get("language").and_then(Json::as_str), language_of(path));
    }
    assert_eq!(cutouts, 5);
    // Written again by this version, it is the same board.
    let again = document(&loaded.data, &loaded.meta, &Pictures::Embedded);
    let back = read(again.text.as_bytes(), &|_| None, &AnyPng, &LIMITS).expect("read again");
    assert_eq!(back.data.items, loaded.data.items);
    assert_eq!(back.meta.repo, loaded.meta.repo);
    assert!(!again.text.contains("/Users/"), "no path of a machine");
}

