//! The whiteboard's file on azul's zip and PNG codec: a board goes in and comes out, what is no
//! archive is told so. Needs azul (a library of its own on the runner), no window.

use std::sync::Arc;

use super::*;
use crate::{
    whiteboard::{
        Anchor, Arrow, Cutout, CutoutImage, End, ImageSlot, Item, Source, LIMITS,
    },
    whiteboard_cutout::Raster,
    whiteboard_geom::{Pt, Rect, View},
    whiteboard_store::board_name,
};

/// A picture of `w` x `h` straight pixels with see-through ones in it, as the board keeps it: its pixels
/// multiplied, its PNG made by azul.
fn picture(w: u32, h: u32) -> Arc<CutoutImage> {
    let mut straight = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let a = if (x + y) % 5 == 0 { 120 } else { 255 };
            straight.extend_from_slice(&[(x * 7 % 256) as u8, (y * 11 % 256) as u8, 90, a]);
        }
    }
    let png = encode_png(w, h, &straight).expect("encoded");
    let raster = Raster::from_straight(w, h, straight).expect("raster");
    Arc::new(CutoutImage { width: w, height: h, scale: 2.0, rgba: raster.px, png })
}

fn cutout(id: u64, x: f32, image: ImageSlot) -> Item {
    Item::Cutout(Cutout {
        id,
        rect: Rect::new(x, 10.0, 120.0, 60.0),
        image,
        label: format!("cutout {id}"),
        source: Source {
            file: "src/lib.rs".to_string(),
            page: 1,
            first_line: 50,
            last_line: 55,
            shape: vec![Pt::new(1.0, 2.0), Pt::new(30.0, 2.0), Pt::new(30.0, 20.0)],
            file_hash: 0xabcd_ef01_2345_6789,
            range_hash: 7,
            lines: vec!["let x = 1;".to_string()],
        },
    })
}

fn board() -> BoardData {
    BoardData {
        items: vec![
            cutout(1, 0.0, ImageSlot::Ready(picture(40, 24))),
            cutout(2, 300.0, ImageSlot::Ready(picture(33, 17))),
            Item::Arrow(Arrow {
                id: 3,
                from: End::item(1),
                to: End::Item { id: 2, anchor: Anchor::Auto },
                bend: 0.3,
                label: "calls".to_string(),
            }),
        ],
        view: View { pan: Pt::new(5.0, -6.0), zoom: 1.25 },
        next_id: 4,
    }
}

#[test]
fn the_png_codec_gives_back_exactly_the_pixels_it_was_given() {
    let straight: Vec<u8> = (0..16_u8).flat_map(|i| [i * 15, 255 - i * 15, 7 * i, if i % 3 == 0 { 0 } else { 200 + i }]).collect();
    let png = encode_png(4, 4, &straight).expect("encoded");
    assert_eq!(&png[1..4], b"PNG");
    let back = decode_png(&png).expect("decoded");
    assert_eq!((back.width, back.height), (4, 4));
    // A PNG is exact for pixels with alpha, but a fully clear pixel has no colour to keep.
    for (a, b) in straight.chunks_exact(4).zip(back.rgba.chunks_exact(4)) {
        assert_eq!(a[3], b[3]);
        if a[3] != 0 {
            assert_eq!(a, b);
        }
    }
}

#[test]
fn what_is_no_picture_is_told_so_by_the_decoder() {
    assert!(decode_png(&[]).is_err());
    assert!(decode_png(b"not a png at all").is_err());
    let png = encode_png(4, 4, &[255; 64]).expect("encoded");
    assert!(decode_png(&png[..png.len() / 2]).is_err(), "cut short");
}

#[test]
fn a_board_goes_into_a_zip_and_comes_back_the_same() {
    let data = board();
    let (name, bytes) = archive(&data, "/work/azul", 1_700_000_000_000).expect("an archive");
    assert_eq!(name, board_name("/work/azul"));
    assert_eq!(&bytes[..2], b"PK", "a zip");
    let loaded = load(&bytes, &LIMITS).expect("read");
    assert_eq!((loaded.left_out, loaded.unreadable_pictures), (0, 0));
    assert_eq!(loaded.workspace, "/work/azul");
    assert_eq!(loaded.data.view, data.view);
    assert_eq!(loaded.data.next_id, 4);
    assert_eq!(loaded.data.items.len(), 3);
    for (a, b) in loaded.data.items.iter().zip(&data.items) {
        match (a, b) {
            (Item::Cutout(x), Item::Cutout(y)) => {
                assert_eq!((x.id, x.rect, &x.label, &x.source), (y.id, y.rect, &y.label, &y.source));
                let (ImageSlot::Ready(xi), ImageSlot::Ready(yi)) = (&x.image, &y.image) else {
                    panic!("pictures came back")
                };
                assert_eq!((xi.width, xi.height, xi.scale), (yi.width, yi.height, yi.scale));
                assert_eq!(xi.png, yi.png, "the PNG is kept as it was");
                for (p, q) in xi.rgba.iter().zip(&yi.rgba) {
                    assert!((i32::from(*p) - i32::from(*q)).abs() <= 3);
                }
            }
            (x, y) => assert_eq!(x, y),
        }
    }
}

#[test]
fn a_save_adds_next_to_nothing_to_the_pictures_it_holds() {
    let data = board();
    let entries = store::write(&data, "w", 1);
    let content: usize = entries.iter().map(|e| e.bytes.len()).sum();
    let bytes = pack(&entries).expect("packed");
    // The pictures are PNGs, which deflate cannot shrink: the archive is what it holds, the headers and a little more.
    assert!(bytes.len() < content + content / 20 + 600, "{} bytes for {content} of content", bytes.len());
}

#[test]
fn what_is_no_board_archive_is_told_so_and_never_a_panic() {
    assert!(matches!(load(&[], &LIMITS), Err(StoreError::NotABoard(_))));
    assert!(matches!(load(b"{\"format\": \"azreview-board/1\"}", &LIMITS), Err(StoreError::NotABoard(_))), "plain JSON is no archive");
    assert!(matches!(load(&[0x50, 0x4b, 3, 4, 9, 9, 9], &LIMITS), Err(StoreError::NotABoard(_))), "the start of a zip");
    let other = pack(&[Entry { name: "something-else.txt".to_string(), bytes: b"hi".to_vec() }]).expect("packed");
    assert!(matches!(load(&other, &LIMITS), Err(StoreError::NotABoard(_))), "a zip with no board in it");
    // The archive of a board, cut at every tenth byte, and damaged at some: an error or a board, no panic.
    let (_, good) = archive(&board(), "w", 1).expect("an archive");
    for end in (0..good.len()).step_by(10) {
        let _ = load(&good[..end], &LIMITS);
    }
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for _ in 0..40 {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let mut bad = good.clone();
        let at = (state >> 33) as usize % bad.len();
        bad[at] ^= 0xff;
        let _ = load(&bad, &LIMITS);
    }
}

#[test]
fn an_archive_whose_directory_declares_far_more_than_a_board_is_refused_before_it_is_unpacked() {
    let (_, mut zip) = archive(&board(), "w", 1).expect("an archive");
    assert!(load(&zip, &LIMITS).is_ok(), "the archive as it was written");
    // The first record of the central directory (50 4b 01 02) now declares 3 GB for its entry, as a zip bomb would.
    let at = zip.windows(4).position(|w| w == [0x50, 0x4b, 0x01, 0x02]).expect("a directory");
    zip[at + 24..at + 28].copy_from_slice(&3_000_000_000_u32.to_le_bytes());
    match load(&zip, &LIMITS) {
        Err(StoreError::TooLarge { bytes }) => assert!(bytes >= 3_000_000_000, "{bytes}"),
        other => panic!("expected TooLarge, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn a_picture_that_is_no_png_leaves_its_cutout_with_the_lines() {
    let data = board();
    let mut entries = store::write(&data, "w", 1);
    for e in &mut entries {
        if e.name == "img-2.png" {
            e.bytes = b"this is not a picture".to_vec();
        }
    }
    let loaded = load(&pack(&entries).expect("packed"), &LIMITS).expect("read");
    assert_eq!(loaded.unreadable_pictures, 1);
    let Item::Cutout(c) = &loaded.data.items[1] else { panic!("a cutout") };
    assert!(matches!(&c.image, ImageSlot::Failed(why) if why.contains("could not be read")), "{:?}", c.image);
    assert_eq!(c.source.lines, vec!["let x = 1;".to_string()]);
}

#[test]
fn a_load_on_a_thread_hands_the_board_back_through_one_message() {
    // The message the worker sends is taken out whole, once.
    let mut msg = RefAny::new(Some(LoadAnswer { result: load(&archive(&board(), "w", 1).expect("an archive").1, &LIMITS) }));
    let first = take_loaded(&mut msg).expect("an answer");
    assert!(first.is_ok());
    assert!(take_loaded(&mut msg).is_none(), "taken once");
    assert!(take_loaded(&mut RefAny::new(0_u8)).is_none(), "not a load's message");
}

/// A folder of its own for a test, removed by the test.
fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("azreview-wb-{tag}-{}-{nanos}", std::process::id()))
}

#[test]
fn a_file_is_written_whole_beside_itself_and_renamed_over_the_old_one() {
    let dir = scratch("atomic");
    let path = dir.join("review").join("boards").join("w.azboard.zip");
    // The folder is made; the first write is the file.
    write_atomic(&path, b"first").expect("written");
    assert_eq!(std::fs::read(&path).expect("read"), b"first");
    // The second replaces it, and nothing of the writing is left beside it.
    write_atomic(&path, b"second, longer").expect("written");
    assert_eq!(std::fs::read(&path).expect("read"), b"second, longer");
    let beside: Vec<String> = std::fs::read_dir(path.parent().expect("a folder"))
        .expect("listed")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert_eq!(beside, vec!["w.azboard.zip".to_string()], "{beside:?}");
    // A file that cannot be written is an error, and the old one is as it was.
    let blocked = dir.join("review").join("boards").join("w.azboard.zip").join("inside");
    assert!(write_atomic(&blocked, b"never").is_err(), "a folder is not made under a file");
    assert_eq!(std::fs::read(&path).expect("read"), b"second, longer");
    let _gone = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_zip_a_save_writes_is_the_board_and_a_save_on_a_thread_answers_through_one_message() {
    let dir = scratch("save");
    let path = dir.join("w.azboard.zip");
    let data = board();
    // What the Thread does, here: pack the store's entries and write them.
    let entries = store::write(&data, "w", 1);
    write_atomic(&path, &pack(&entries).expect("packed")).expect("written");
    let loaded = load(&std::fs::read(&path).expect("read"), &LIMITS).expect("the file is a board");
    assert_eq!(loaded.data.items.len(), data.items.len());
    let _gone = std::fs::remove_dir_all(&dir);
    // The message the worker sends is taken out whole, once.
    let mut msg = RefAny::new(Some(SaveAnswer { result: Ok(()) }));
    assert_eq!(take_saved(&mut msg), Some(Ok(())));
    assert!(take_saved(&mut msg).is_none(), "taken once");
    assert!(take_saved(&mut RefAny::new(0_u8)).is_none(), "not a save's message");
}
