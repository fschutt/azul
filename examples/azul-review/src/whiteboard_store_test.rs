//! The whiteboard's file: a manifest and a picture per cutout, read back whole, tolerant of what a disk
//! and a newer version may leave in it, never a panic on a damaged one. No window, no azul: the PNG
//! codec is a stand-in that keeps the pixels in the clear.

use std::{collections::BTreeMap, sync::Arc};

use super::*;
use crate::{
    whiteboard::{Anchor, Arrow, Cutout, CutoutImage, End, ImageSlot, Item, Limits, Source, LIMITS},
    whiteboard_geom::{Pt, Rect, Side, View},
};

/// A "PNG" of the tests: 4 bytes width, 4 bytes height, then the straight RGBA.
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

fn source(file: &str, first: u32, last: u32) -> Source {
    Source {
        file: file.to_string(),
        page: 3,
        first_line: first,
        last_line: last,
        shape: vec![Pt::new(10.5, 10.25), Pt::new(90.0, 10.0), Pt::new(90.0, 50.75), Pt::new(10.0, 50.0)],
        file_hash: 0xdead_beef_cafe_f00d,
        range_hash: u64::MAX,
        lines: vec!["fn \"quoted\" () {}".to_string(), "    tab\tand ünïcode ✓".to_string()],
    }
}

fn cutout(id: ItemId, x: f32, label: &str, image: ImageSlot) -> Item {
    Item::Cutout(Cutout { id, rect: Rect::new(x, 20.5, 200.0, 100.0), image, label: label.to_string(), source: source("src/lib.rs", 10, 20) })
}

fn arrow(id: ItemId, from: End, to: End, bend: f32, label: &str) -> Item {
    Item::Arrow(Arrow { id, from, to, bend, label: label.to_string() })
}

fn sample() -> BoardData {
    BoardData {
        items: vec![
            cutout(1, 0.0, "the parser\nof two lines", ImageSlot::Ready(picture(8, 4, 200))),
            cutout(2, 400.0, "", ImageSlot::Ready(picture(6, 3, 90))),
            cutout(3, 800.0, "never drawn", ImageSlot::Failed("the font was missing".to_string())),
            arrow(10, End::item(1), End::item(2), 0.25, "calls"),
            arrow(11, End::Item { id: 2, anchor: Anchor::Side(Side::Left) }, End::Item { id: 3, anchor: Anchor::Point { u: 0.25, v: 0.75 } }, -0.5, ""),
            arrow(12, End::item(3), End::Free(Pt::new(-30.5, 40.25)), 0.0, "to nowhere"),
        ],
        view: View { pan: Pt::new(-120.5, 40.0), zoom: 1.5 },
        next_id: 13,
    }
}

fn blobs(entries: &[Entry]) -> BTreeMap<String, Vec<u8>> {
    entries.iter().map(|e| (e.name.clone(), e.bytes.clone())).collect()
}

fn read_entries(entries: &[Entry], limits: &Limits) -> Result<Loaded, StoreError> {
    let map = blobs(entries);
    let manifest = map.get(MANIFEST).cloned().unwrap_or_default();
    read(&manifest, &|name| map.get(name).cloned(), &Codec, limits)
}

#[test]
fn a_board_is_written_as_one_manifest_and_a_picture_for_each_cutout_that_has_one() {
    let entries = write(&sample(), "/work/azul", 1_700_000_000_123);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["board.json", "img-1.png", "img-2.png"], "the third has no picture, the arrows none");
    let manifest = String::from_utf8(entries[0].bytes.clone()).expect("text");
    let json = crate::whiteboard_json::parse(&manifest).expect("the manifest is JSON");
    assert_eq!(json.get("format").and_then(|f| f.as_str()), Some(FORMAT));
    assert_eq!(json.get("workspace").and_then(|f| f.as_str()), Some("/work/azul"));
    assert_eq!(json.get("saved_ms").and_then(|f| f.as_u64()), Some(1_700_000_000_123));
    assert_eq!(json.get("items").and_then(|i| i.as_arr()).map(<[_]>::len), Some(6));
    assert_eq!(entries[1].bytes, picture(8, 4, 200).png, "the picture is written as it was made: no new encode");
}

#[test]
fn what_is_written_is_read_back_whole() {
    let board = sample();
    let entries = write(&board, "/work/azul", 42);
    let loaded = read_entries(&entries, &LIMITS).expect("read");
    assert_eq!(loaded.workspace, "/work/azul");
    assert_eq!(loaded.saved_ms, 42);
    assert_eq!((loaded.left_out, loaded.unreadable_pictures), (0, 0));
    assert_eq!(loaded.data.items, board.items, "the same items, in the same order, with the same everything");
    assert_eq!(loaded.data.view, board.view);
    assert_eq!(loaded.data.next_id, 13);
}

#[test]
fn a_picture_with_see_through_pixels_comes_back_as_it_was_to_a_level() {
    let straight: Vec<u8> = [(200_u8, 100_u8, 50_u8, 128_u8), (10, 20, 30, 255), (0, 0, 0, 0), (255, 255, 255, 40)]
        .iter()
        .flat_map(|&(r, g, b, a)| [r, g, b, a])
        .collect();
    let png = fake_png(4, 1, &straight);
    let premultiplied = crate::whiteboard_cutout::Raster::from_straight(4, 1, straight).expect("raster");
    let image = Arc::new(CutoutImage { width: 4, height: 1, scale: 2.0, rgba: premultiplied.px.clone(), png });
    let data = BoardData { items: vec![cutout(1, 0.0, "", ImageSlot::Ready(image))], view: View::default(), next_id: 2 };
    let loaded = read_entries(&write(&data, "w", 0), &LIMITS).expect("read");
    let Item::Cutout(c) = &loaded.data.items[0] else { panic!("a cutout") };
    let ImageSlot::Ready(img) = &c.image else { panic!("a picture") };
    for (a, b) in premultiplied.px.iter().zip(&img.rgba) {
        assert!((i32::from(*a) - i32::from(*b)).abs() <= 3, "{a} against {b}");
    }
    assert_eq!((img.width, img.height), (4, 1));
    assert!((img.scale - 2.0).abs() < 1e-6);
}

#[test]
fn the_texts_and_numbers_of_a_board_survive_exactly() {
    let loaded = read_entries(&write(&sample(), "/work/ünï", 1), &LIMITS).expect("read");
    let Item::Cutout(first) = &loaded.data.items[0] else { panic!("a cutout") };
    assert_eq!(first.label, "the parser\nof two lines");
    assert_eq!(first.source.lines, source("x", 1, 2).lines, "quotes, a tab, umlauts, a tick");
    assert_eq!(first.source.file_hash, 0xdead_beef_cafe_f00d);
    assert_eq!(first.source.range_hash, u64::MAX, "a 64 bit number is kept whole");
    assert_eq!(first.source.shape[0], Pt::new(10.5, 10.25));
    assert_eq!(first.source.page, 3);
    assert_eq!(loaded.workspace, "/work/ünï");
}

#[test]
fn a_cutout_without_a_picture_says_why_it_has_none() {
    let data = BoardData {
        items: vec![
            cutout(1, 0.0, "", ImageSlot::Pending),
            cutout(2, 10.0, "", ImageSlot::Failed("no font".to_string())),
            // A picture whose PNG was never made: no file to write.
            cutout(3, 20.0, "", ImageSlot::Ready(Arc::new(CutoutImage { width: 1, height: 1, scale: 1.0, rgba: vec![0; 4], png: vec![] }))),
        ],
        view: View::default(),
        next_id: 4,
    };
    let entries = write(&data, "w", 0);
    assert_eq!(entries.len(), 1, "only the manifest");
    let loaded = read_entries(&entries, &LIMITS).expect("read");
    let reasons: Vec<String> = loaded
        .data
        .items
        .iter()
        .map(|i| match i {
            Item::Cutout(Cutout { image: ImageSlot::Failed(why), .. }) => why.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(reasons[0].contains("not drawn"), "{reasons:?}");
    assert_eq!(reasons[1], "no font");
    assert!(reasons[2].contains("no picture"), "{reasons:?}");
    assert_eq!(loaded.unreadable_pictures, 0, "they were never pictures, so none was lost");
}

#[test]
fn unknown_keys_are_ignored_and_an_unknown_kind_is_left_out_and_counted() {
    let mut text = String::from_utf8(write(&sample(), "w", 1)[0].bytes.clone()).expect("text");
    text = text.replacen("{\"kind\":\"arrow\"", "{\"kind\":\"sticker\",\"id\":99,\"shine\":true},\n  {\"kind\":\"arrow\",\"future\":[1,2]", 1);
    text = text.replacen("\"format\"", "\"made_by\":\"a newer minor\",\"format\"", 1);
    let map = blobs(&write(&sample(), "w", 1));
    let loaded = read(text.as_bytes(), &|n| map.get(n).cloned(), &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.left_out, 1);
    assert_eq!(loaded.data.items.len(), 6, "the six items of the board");
}

#[test]
fn an_item_missing_what_it_cannot_do_without_is_left_out_and_counted() {
    let text = r#"{"format":"azreview-board/1","items":[
        {"kind":"cutout","id":1,"x":0,"y":0,"w":50,"h":20,"image":null,"state":"failed","why":"x","source":{"file":"a.rs","first_line":1,"last_line":2}},
        {"kind":"cutout","id":2,"y":0,"w":50,"h":20,"source":{"file":"a.rs","first_line":1,"last_line":2}},
        {"kind":"cutout","id":3,"x":0,"y":0,"w":50,"h":20},
        {"kind":"cutout","x":0,"y":0,"w":50,"h":20,"source":{"file":"a.rs","first_line":1,"last_line":2}},
        {"kind":"cutout","id":5,"x":"left","y":0,"w":50,"h":20,"source":{"file":"a.rs","first_line":1,"last_line":2}},
        {"kind":"arrow","id":6,"from":{"item":1},"to":{"nothing":true}},
        {"kind":"arrow","id":7,"from":{"item":1,"anchor":"auto"},"to":{"free":[5,6]},"bend":"much"}
    ]}"#;
    let loaded = read(text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.data.items.iter().map(Item::id).collect::<Vec<_>>(), vec![1, 7]);
    assert_eq!(loaded.left_out, 5);
    let Item::Arrow(a) = &loaded.data.items[1] else { panic!("an arrow") };
    assert_eq!(a.bend, 0.0, "a bend that is no number is straight");
    assert_eq!(a.to, End::Free(Pt::new(5.0, 6.0)));
}

#[test]
fn a_picture_that_is_missing_or_damaged_leaves_the_cutout_with_the_lines_and_a_reason() {
    let entries = write(&sample(), "w", 1);
    // 1: gone from the file; 2: cut short.
    let mut map = blobs(&entries);
    map.remove("img-1.png");
    map.get_mut("img-2.png").map(|b| b.truncate(20));
    let manifest = map.get(MANIFEST).cloned().unwrap_or_default();
    let loaded = read(&manifest, &|n| map.get(n).cloned(), &Codec, &LIMITS).expect("read");
    assert_eq!(loaded.unreadable_pictures, 2);
    for (index, why) in [(0, "missing"), (1, "cut short")] {
        let Item::Cutout(c) = &loaded.data.items[index] else { panic!("a cutout") };
        assert!(matches!(&c.image, ImageSlot::Failed(w) if w.contains(why)), "{:?}", c.image);
        assert_eq!(c.source.lines.len(), 2, "the lines it was made from are what is shown");
    }
}

#[test]
fn a_picture_past_the_caps_is_not_loaded_and_the_cutout_stays() {
    // The pictures have 32 and 18 pixels.
    let entries = write(&sample(), "w", 1);
    let one = Limits { max_image_px: 20, ..LIMITS };
    let loaded = read_entries(&entries, &one).expect("read");
    assert_eq!(loaded.unreadable_pictures, 1, "the 32 pixel picture is over the cap of one picture");
    assert_eq!(loaded.data.items.len(), 6, "every item is still there");
    let all = Limits { max_total_px: 40, ..LIMITS };
    let loaded = read_entries(&entries, &all).expect("read");
    assert_eq!(loaded.unreadable_pictures, 1, "32 and 18 pixels do not both fit in 40: the second is left out");
    let Item::Cutout(second) = &loaded.data.items[1] else { panic!("a cutout") };
    assert!(matches!(&second.image, ImageSlot::Failed(why) if why.contains("room")), "{:?}", second.image);
}

#[test]
fn what_is_no_board_is_said_so_and_a_newer_board_is_told_apart() {
    let no = |text: &str| read(text.as_bytes(), &|_| None, &Codec, &LIMITS);
    assert!(matches!(no(""), Err(StoreError::NotABoard(_))));
    assert!(matches!(no("PK\u{3}\u{4}"), Err(StoreError::NotABoard(_))));
    assert!(matches!(no("[1,2]"), Err(StoreError::NotABoard(_))));
    assert!(matches!(no("{\"items\": []}"), Err(StoreError::NotABoard(_))), "no format");
    assert!(matches!(no("{\"format\":\"something-else/1\"}"), Err(StoreError::NotABoard(_))));
    assert!(matches!(no("{\"format\":\"azreview-board/x\"}"), Err(StoreError::NotABoard(_))));
    assert!(matches!(no("{\"format\":\"azreview-board/0\"}"), Err(StoreError::NotABoard(_))));
    assert_eq!(no("{\"format\":\"azreview-board/2\",\"items\":[]}").err(), Some(StoreError::TooNew { found: 2 }));
    assert_eq!(no("{\"format\":\"azreview-board/17\"}").err(), Some(StoreError::TooNew { found: 17 }));
    assert!(no("{\"format\":\"azreview-board/1\"}").is_ok(), "no items is an empty board");
    // Not text at all.
    assert!(matches!(read(&[0xff, 0xfe, 0x00], &|_| None, &Codec, &LIMITS), Err(StoreError::NotABoard(_))));
}

/// The bytes of a zip that is nothing but a central directory and its end record: one entry for each (name, the size
/// it declares unpacked). Enough for the directory to be read; nothing is in it to unpack.
fn directory_only(entries: &[(&str, u32)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, size) in entries {
        out.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        out.extend_from_slice(&[0_u8; 20]); // versions, flags, method, time, date, crc, compressed size
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&u16::try_from(name.len()).expect("a short name").to_le_bytes());
        out.extend_from_slice(&[0_u8; 16]); // extra, comment, disk, internal and external attributes, offset
        out.extend_from_slice(name.as_bytes());
    }
    let size = u32::try_from(out.len()).expect("small");
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&[0_u8; 4]); // disks
    out.extend_from_slice(&u16::try_from(entries.len()).expect("few").to_le_bytes());
    out.extend_from_slice(&u16::try_from(entries.len()).expect("few").to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0_u32.to_le_bytes()); // the directory starts at the start
    out.extend_from_slice(&[0_u8; 2]); // no comment
    out
}

#[test]
fn the_sizes_an_archive_declares_are_read_from_its_directory_alone() {
    let zip = directory_only(&[("board.json", 2_000), ("img-1.png", 9_000), ("img-2.png", 1)]);
    assert_eq!(
        declared_sizes(&zip),
        Some(vec![("board.json".to_string(), 2_000), ("img-1.png".to_string(), 9_000), ("img-2.png".to_string(), 1)])
    );
    assert_eq!(declared_sizes(&directory_only(&[])), Some(Vec::new()), "an empty archive");
    // What is no zip, or is cut short anywhere, or is ZIP64, is not read - and is never a panic.
    assert_eq!(declared_sizes(b"not a zip at all, but long enough to hold an end record"), None);
    assert_eq!(declared_sizes(&[]), None);
    for cut in 0..zip.len() {
        let _ = declared_sizes(&zip[..cut]);
    }
    assert_eq!(declared_sizes(&directory_only(&[("big.png", u32::MAX)])), None, "ZIP64 marks its sizes so");
    // A directory that points past the end of the file.
    let mut lying = zip.clone();
    let at = lying.len() - 6;
    lying[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(declared_sizes(&lying), None);
}

#[test]
fn an_archive_that_declares_more_than_a_board_may_be_is_refused_before_anything_is_unpacked() {
    let ok = directory_only(&[("board.json", 40_000), ("img-1.png", 5_000_000), ("img-2.png", 5_000_000)]);
    assert_eq!(check_unpacked(&ok, &LIMITS), Ok(()));
    // One entry larger than any picture file is.
    let big = directory_only(&[("board.json", 40_000), ("img-1.png", MAX_PICTURE_BYTES as u32 + 1)]);
    assert_eq!(check_unpacked(&big, &LIMITS), Err(40_000 + MAX_PICTURE_BYTES as u64 + 1));
    // Many entries that are each fine and together are not: 40 Mpx of 4 bytes is 160 MB, and all the room is that and a manifest.
    let many: Vec<(String, u32)> = (0..60).map(|i| (format!("img-{i}.png"), 60_000_000)).collect();
    let many: Vec<(&str, u32)> = many.iter().map(|(n, s)| (n.as_str(), *s)).collect();
    assert!(check_unpacked(&directory_only(&many), &LIMITS).is_err(), "3.6 GB of pictures");
    // What cannot be read as a directory is the reader's to judge.
    assert_eq!(check_unpacked(b"garbage", &LIMITS), Ok(()));
}

#[test]
fn a_manifest_that_is_too_large_is_refused_before_it_is_read() {
    let big = vec![b' '; crate::whiteboard_json::MAX_TEXT + 1];
    assert!(matches!(read(&big, &|_| None, &Codec, &LIMITS), Err(StoreError::TooLarge { .. })));
}

#[test]
fn a_file_cut_short_or_damaged_anywhere_is_never_a_panic() {
    let entries = write(&sample(), "/work/azul", 7);
    let map = blobs(&entries);
    let manifest = map.get(MANIFEST).cloned().unwrap_or_default();
    let read_it = |m: &[u8]| {
        let _ = read(m, &|n| map.get(n).cloned(), &Codec, &LIMITS);
    };
    // Every length: the file stops there.
    for end in 0..manifest.len() {
        read_it(&manifest[..end]);
    }
    // A byte flipped to something else, 400 times, the same ones every run.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    for _ in 0..400 {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let at = (state >> 33) as usize % manifest.len();
        let mut bad = manifest.clone();
        bad[at] = (state >> 20) as u8;
        read_it(&bad);
    }
}

#[test]
fn numbers_that_make_no_sense_are_pulled_in_by_the_board_not_trusted() {
    let text = r#"{"format":"azreview-board/1","view":{"x":1e30,"y":-1e30,"zoom":-4},"items":[
        {"kind":"cutout","id":1,"x":1e30,"y":-1e30,"w":1e9,"h":0.001,"image":null,"state":"failed","why":"x",
         "source":{"file":"a.rs","first_line":50,"last_line":3,"shape":[[1,2],[1e40,2],[3]]}}
    ]}"#;
    let loaded = read(text.as_bytes(), &|_| None, &Codec, &LIMITS).expect("read");
    let (board, _) = crate::whiteboard::Board::from_data(loaded.data, LIMITS);
    let c = board.cutout(1).expect("the cutout");
    assert!(c.rect.x <= LIMITS.max_coord && c.rect.y >= -LIMITS.max_coord);
    assert!(c.rect.w <= LIMITS.max_cutout && c.rect.h >= LIMITS.min_cutout);
    assert_eq!((c.source.first_line, c.source.last_line), (3, 50));
    assert!(c.source.shape.iter().all(|p| p.is_finite()), "{:?}", c.source.shape);
    assert!(board.view().zoom >= LIMITS.zoom_min);
}

// ---- the file's name, the one writer, the damaged file

#[test]
fn the_name_of_a_boards_file_is_the_folder_and_a_hash_of_its_whole_path() {
    let a = board_name("/Users/me/work/azul");
    assert!(a.starts_with("boards/azul-") && a.ends_with(".azboard.zip"), "{a}");
    assert_eq!(a, board_name("/Users/me/work/azul/"), "a slash at the end is the same folder");
    assert_eq!(board_name("C:\\work\\azul"), board_name("C:/work/azul"), "either slash");
    assert_ne!(a, board_name("/Users/me/other/azul"), "another folder of the same name is another board");
    assert_eq!(board_name("/x/azul"), board_name("/x/azul"), "stable");
}

#[test]
fn the_name_has_nothing_in_it_a_path_could_climb_out_by() {
    for root in ["/a/../../etc", "/a/b c/Dé&?*<>|:\"x", "/", "", "///", "/very/long/".repeat(30).as_str(), "..", "/a/\u{0}/b"] {
        let name = board_name(root);
        assert!(name.starts_with("boards/") && name.ends_with(".azboard.zip"), "{name}");
        let body = &name["boards/".len()..];
        assert!(!body.contains('/') && !body.contains('\\') && !body.contains(".."), "{root:?} gave {name}");
        assert!(body.len() <= 24 + 1 + 16 + ".azboard.zip".len(), "{}", body.len());
        assert!(body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.'), "{name}");
    }
    assert_ne!(board_name("/a/b"), board_name("/a/c"));
    assert!(board_name("/Users/me/Dé").starts_with("boards/d-") || board_name("/Users/me/Dé").starts_with("boards/d"), "{}", board_name("/Users/me/Dé"));
}

#[test]
fn a_damaged_file_is_kept_under_a_name_of_its_own() {
    let name = board_name("/work/azul");
    let q = quarantine_name(&name, 1_700_000_000_000);
    assert!(q.starts_with(&name) && q.contains("corrupt") && q != name, "{q}");
    assert_ne!(q, quarantine_name(&name, 1_700_000_000_001), "a second one does not take the first's place");
}

#[test]
fn only_one_write_runs_and_one_that_was_asked_meanwhile_runs_after_it() {
    let mut gate = SaveGate::default();
    assert!(gate.request(), "nothing is writing: write now");
    assert!(!gate.request(), "one is: wait");
    assert!(!gate.request(), "waiting twice is waiting once");
    assert!(gate.is_busy());
    assert!(gate.finished(), "one was asked meanwhile: write again");
    assert!(gate.request(), "the second write starts");
    assert!(!gate.finished(), "nobody asked meanwhile");
    assert!(!gate.is_busy());
    assert!(gate.request());
}

#[test]
fn the_sentences_of_a_store_error_say_what_happens_to_the_file() {
    let errors = [
        StoreError::NotABoard("a comma was expected (byte 7)".to_string()),
        StoreError::TooNew { found: 3 },
        StoreError::TooLarge { bytes: 99_999_999 },
    ];
    for e in &errors {
        assert!(e.english().len() > 20, "{e:?}");
        assert!(e.key().contains("whiteboard-store-"), "{}", e.key());
    }
    assert!(errors[1].english().contains('3'));
    assert!(errors[0].english().contains("byte 7"));
    assert_eq!(errors[0].why(), Some("a comma was expected (byte 7)"));
    assert_eq!(errors[1].args(), vec![("found", 3)]);
}

#[test]
fn a_full_board_is_written_and_read_back_in_a_blink() {
    // 200 cutouts with 40 x 30 pictures (a PNG of 4 KB each) and 199 arrows.
    let mut items = Vec::new();
    for i in 1..=200_u64 {
        items.push(cutout(i, i as f32 * 220.0, "a label of some length", ImageSlot::Ready(picture(40, 30, (i % 250) as u8))));
    }
    for i in 1..200_u64 {
        items.push(arrow(1_000 + i, End::item(i), End::item(i + 1), 0.2, ""));
    }
    let data = BoardData { items, view: View::default(), next_id: 2_000 };
    let began = std::time::Instant::now();
    let entries = write(&data, "/work/azul", 1);
    let wrote = began.elapsed();
    let began = std::time::Instant::now();
    let loaded = read_entries(&entries, &LIMITS).expect("read");
    let read_it = began.elapsed();
    let bytes: usize = entries.iter().map(|e| e.bytes.len()).sum();
    eprintln!("a board of {} items, {bytes} bytes: written in {wrote:?}, read in {read_it:?}", data.items.len());
    assert_eq!(loaded.data.items.len(), 399);
    assert!(wrote.as_millis() < 500 && read_it.as_millis() < 1_000, "{wrote:?} / {read_it:?}");
}
