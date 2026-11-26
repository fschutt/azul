//! A board as a file to share and a file to open: written atomically beside its pictures or with them inside, read back
//! whole, and what a file asks of the disk (a picture beside it) kept to the folder it lies in. Real folders in the
//! system's temp folder; no window, no azul: the PNG codec is a stand-in that keeps the pixels in the clear.

use std::{path::PathBuf, sync::Arc};

use super::*;
use crate::{
    whiteboard::{Arrow, Cutout, CutoutImage, End, ImageSlot, Item, Source, LIMITS},
    whiteboard_geom::{Pt, Rect, View},
    whiteboard_json::{parse, Json},
    whiteboard_store::DecodedPng,
};

const REV: &str = "0123456789abcdef0123456789abcdef01234567";

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("azreview-export-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("a folder");
    dir
}

fn done(dir: &Path) {
    let _gone = fs::remove_dir_all(dir);
}

fn put(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("a folder")).expect("made");
    fs::write(path, text).expect("written");
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
    let mut png = w.to_le_bytes().to_vec();
    png.extend_from_slice(&h.to_le_bytes());
    png.extend_from_slice(&straight);
    Arc::new(CutoutImage { width: w, height: h, scale: 2.0, rgba: straight, png })
}

fn board() -> BoardData {
    let lines = vec!["fn main() {".to_string(), "}".to_string()];
    let src = |file: &str| Source {
        file: file.to_string(),
        page: 1,
        first_line: 3,
        last_line: 4,
        shape: vec![Pt::new(1.0, 2.0), Pt::new(30.0, 2.0)],
        file_hash: 0xabcd,
        range_hash: 0x1234,
        sha256: crate::whiteboard_hash::lines_tagged(&lines),
        lines: lines.clone(),
        rev: REV.to_string(),
    };
    BoardData {
        items: vec![
            Item::Cutout(Cutout { id: 1, rect: Rect::new(0.0, 0.0, 200.0, 100.0), image: ImageSlot::Ready(picture(8, 4, 200)), label: "one".to_string(), source: src("src/main.rs") }),
            Item::Cutout(Cutout { id: 2, rect: Rect::new(300.0, 0.0, 200.0, 100.0), image: ImageSlot::Ready(picture(6, 3, 90)), label: String::new(), source: src("src/lib.rs") }),
            Item::Arrow(Arrow { id: 3, from: End::item(1), to: End::item(2), bend: 0.2, label: "calls".to_string() }),
        ],
        view: View { pan: Pt::new(5.0, -6.0), zoom: 1.25 },
        next_id: 4,
    }
}

/// A working tree with a `.git` folder: HEAD on branch `main` at `REV`, a remote and a name.
fn repo(tag: &str) -> PathBuf {
    let root = scratch(tag);
    put(&root.join(".git/HEAD"), "ref: refs/heads/main\n");
    put(&root.join(".git/refs/heads/main"), &format!("{REV}\n"));
    put(
        &root.join(".git/config"),
        "[remote \"origin\"]\n\turl = https://token@github.com/fschutt/azul.git\n[remote \"fork\"]\n\turl = git@github.com:ada/azul.git\n[user]\n\tname = Ada Lovelace\n\temail = ada@example.org\n",
    );
    root
}

fn request(folder: &Path, pictures: ExportPictures) -> ExportRequest {
    ExportRequest {
        data: board(),
        meta: Meta { board_id: "b1".to_string(), board_name: "Parser".to_string(), workspace_name: "azul".to_string(), ..Meta::default() },
        pictures,
        folder: folder.to_path_buf(),
        home: None,
        remote: None,
    }
}

const QUICK: Duration = Duration::from_secs(5);

// ---- names

#[test]
fn a_file_is_proposed_under_the_boards_name_and_its_pictures_folder_is_named_for_it() {
    assert_eq!(suggested_name("Parser"), "Parser.azboard.json");
    assert_eq!(suggested_name("The event loop / v2: why?"), "The-event-loop-v2-why.azboard.json");
    assert_eq!(suggested_name("  ../../etc/passwd  "), "etc-passwd.azboard.json", "nothing a path could climb by");
    assert_eq!(suggested_name(""), "board.azboard.json");
    assert_eq!(suggested_name("???"), "board.azboard.json");
    assert_eq!(suggested_name("Ünïcode ✓ naïve"), "Ünïcode-naïve.azboard.json");
    assert!(suggested_name(&"x".repeat(500)).len() < 100, "short");
    assert_eq!(pictures_folder_name("Parser.azboard.json"), "Parser.assets");
    assert_eq!(pictures_folder_name("Parser.json"), "Parser.assets");
    assert_eq!(pictures_folder_name("Parser"), "Parser.assets");
    assert_eq!(pictures_folder_name(".json"), "board.assets");
}

#[test]
fn a_file_is_written_whole_or_not_at_all_and_leaves_nothing_beside_it() {
    let dir = scratch("atomic");
    let path = dir.join("a/b/board.azboard.json");
    write_atomic(&path, b"first").expect("written");
    assert_eq!(fs::read(&path).expect("read"), b"first");
    write_atomic(&path, b"second, longer").expect("replaced");
    assert_eq!(fs::read(&path).expect("read"), b"second, longer");
    let names: Vec<_> = fs::read_dir(path.parent().expect("dir")).expect("listed").map(|e| e.expect("entry").file_name()).collect();
    assert_eq!(names.len(), 1, "no temporary file stays: {names:?}");
    assert!(write_atomic(&dir.join("a/b/board.azboard.json/inside"), b"x").is_err(), "a file is no folder");
    done(&dir);
}

// ---- export

#[test]
fn an_export_with_the_pictures_inside_is_one_file_that_reads_back_to_the_board() {
    let work = repo("embedded");
    let out = scratch("embedded-out");
    let target = out.join("Parser.azboard.json");
    let exported = export(request(&work, ExportPictures::Embedded), &target, QUICK).expect("exported");
    assert_eq!((exported.path.as_path(), exported.pictures, exported.links), (target.as_path(), 2, 2));
    assert_eq!(exported.pictures_folder, None);
    assert_eq!(exported.remote.as_deref(), Some("origin"));
    assert_eq!(fs::read_dir(&out).expect("listed").count(), 1, "one file and nothing else");
    // What is in it.
    let text = fs::read_to_string(&target).expect("read");
    assert_eq!(exported.bytes, text.len() as u64);
    let j = parse(&text).expect("JSON");
    let repo_json = j.get("workspace").and_then(|w| w.get("repo")).expect("the repository");
    assert_eq!(repo_json.get("rev").and_then(Json::as_str), Some(REV), "HEAD at the export");
    assert_eq!(repo_json.get("web_url").and_then(Json::as_str), Some("https://github.com/fschutt/azul"));
    assert_eq!(repo_json.get("remote").and_then(Json::as_str), Some("https://github.com/fschutt/azul.git"), "no token in it");
    assert!(!text.contains("token@") && !text.contains("ada@example.org"), "no secret, no e-mail address");
    assert!(text.contains("data:image/png;base64,"));
    // Back.
    let back = import(&target, &Codec, &LIMITS).expect("imported");
    assert_eq!(back.data.items, board().items);
    assert_eq!(back.data.view, board().view);
    assert_eq!((back.left_out, back.unreadable_pictures), (0, 0));
    assert_eq!(back.meta.board_name, "Parser");
    done(&work);
    done(&out);
}

#[test]
fn an_export_with_the_pictures_beside_it_writes_a_folder_first_and_reads_back_whole() {
    let work = repo("beside");
    let out = scratch("beside-out");
    let target = out.join("Parser.azboard.json");
    let exported = export(request(&work, ExportPictures::Beside), &target, QUICK).expect("exported");
    assert_eq!(exported.pictures_folder.as_deref(), Some(out.join("Parser.assets").as_path()));
    let mut names: Vec<_> = fs::read_dir(out.join("Parser.assets")).expect("listed").map(|e| e.expect("entry").file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, vec!["img-1.png", "img-2.png"]);
    assert_eq!(fs::read(out.join("Parser.assets/img-1.png")).expect("read"), picture(8, 4, 200).png);
    assert!(!fs::read_to_string(&target).expect("read").contains("base64"), "no picture in the file");
    let back = import(&target, &Codec, &LIMITS).expect("imported");
    assert_eq!(back.data.items, board().items);
    // Without the folder the board is there and its cutouts say what is missing.
    fs::remove_dir_all(out.join("Parser.assets")).expect("removed");
    let lost = import(&target, &Codec, &LIMITS).expect("imported");
    assert_eq!((lost.data.items.len(), lost.unreadable_pictures), (3, 2));
    done(&work);
    done(&out);
}

#[test]
fn the_remote_a_person_chose_is_the_one_the_links_are_made_from_and_the_name_git_has_is_not_the_files() {
    let work = repo("remote");
    let out = scratch("remote-out");
    let target = out.join("b.azboard.json");
    let mut req = request(&work, ExportPictures::Embedded);
    req.remote = Some("fork".to_string());
    let exported = export(req, &target, QUICK).expect("exported");
    assert_eq!(exported.remote.as_deref(), Some("fork"));
    let text = fs::read_to_string(&target).expect("read");
    assert!(text.contains("\"web_url\": \"https://github.com/ada/azul\""), "{text}");
    assert!(text.contains(&format!("https://github.com/ada/azul/blob/{REV}/src/main.rs#L3-L4")), "the links name that repository");
    assert!(!text.contains("Ada Lovelace"), "the author is the setting's to give (the meta's), not what git has");
    done(&work);
    done(&out);
}

#[test]
fn a_workspace_that_is_no_repository_gives_a_document_with_paths_and_lines_only() {
    let work = scratch("norepo");
    let out = scratch("norepo-out");
    let target = out.join("b.azboard.json");
    let exported = export(request(&work, ExportPictures::Embedded), &target, QUICK).expect("exported");
    assert_eq!((exported.links, exported.remote), (0, None));
    let text = fs::read_to_string(&target).expect("read");
    assert!(!text.contains("\"repo\"") && !text.contains("permalink"), "{text}");
    assert!(text.contains("\"path\": \"src/main.rs\"") && text.contains("\"line_start\": 3"));
    done(&work);
    done(&out);
}

#[test]
fn a_target_that_cannot_be_written_is_a_sentence_and_nothing_is_left_half_done() {
    let work = repo("denied");
    let out = scratch("denied-out");
    put(&out.join("blocker"), "a file where a folder should be");
    let err = export(request(&work, ExportPictures::Beside), &out.join("blocker/b.azboard.json"), QUICK).err().expect("an error");
    assert!(!err.is_empty());
    assert!(!out.join("blocker/b.assets").exists());
    done(&work);
    done(&out);
}

// ---- import

#[test]
fn a_file_that_is_no_board_or_cannot_be_read_is_told_and_never_a_panic() {
    let dir = scratch("nofile");
    assert!(matches!(import(&dir.join("missing.json"), &Codec, &LIMITS), Err(StoreError::NotABoard(_))));
    assert!(matches!(import(&dir, &Codec, &LIMITS), Err(StoreError::NotABoard(_))), "a folder");
    put(&dir.join("text.json"), "this is not json");
    assert!(matches!(import(&dir.join("text.json"), &Codec, &LIMITS), Err(StoreError::NotABoard(_))));
    put(&dir.join("new.json"), "{\"format\":\"azboard/2\"}");
    assert_eq!(import(&dir.join("new.json"), &Codec, &LIMITS).err(), Some(StoreError::TooNew { found: 2 }));
    fs::write(dir.join("binary.json"), [0xff_u8, 0xfe, 0, 1, 2]).expect("written");
    assert!(matches!(import(&dir.join("binary.json"), &Codec, &LIMITS), Err(StoreError::NotABoard(_))));
    done(&dir);
}

#[test]
fn a_file_larger_than_any_board_is_refused_before_it_is_read() {
    let dir = scratch("huge");
    let path = dir.join("huge.json");
    let file = fs::File::create(&path).expect("created");
    file.set_len(MAX_IMPORT_BYTES + 1).expect("a sparse file");
    drop(file);
    assert!(matches!(import(&path, &Codec, &LIMITS), Err(StoreError::TooLarge { .. })));
    done(&dir);
}

#[test]
fn a_document_only_takes_pictures_from_the_folder_it_lies_in() {
    let dir = scratch("sandbox");
    let inside = dir.join("inside");
    let secret = dir.join("secret.png");
    fs::write(&secret, &picture(2, 2, 9).png).expect("written");
    put(&inside.join("mine.assets/ok.png"), "");
    fs::write(inside.join("mine.assets/ok.png"), &picture(2, 2, 5).png).expect("written");
    let doc = |src: &str| {
        format!(
            r#"{{"format":"azboard/1","assets":{{"img-1":{{"src":{}}}}},"items":[{{"kind":"cutout","id":1,"x":0,"y":0,"width":50,"height":20,"image":{{"asset":"img-1","scale":1}},"source":{{"path":"a.rs","line_start":1,"line_end":2}}}}]}}"#,
            crate::whiteboard_json::pretty(&Json::Str(src.to_string())).trim_end()
        )
    };
    let unreadable = |src: &str| {
        put(&inside.join("board.azboard.json"), &doc(src));
        import(&inside.join("board.azboard.json"), &Codec, &LIMITS).expect("a board").unreadable_pictures
    };
    assert_eq!(unreadable("mine.assets/ok.png"), 0, "a file below the document's folder is taken");
    assert_eq!(unreadable("../secret.png"), 1, "one beside the folder is not");
    assert_eq!(unreadable(&secret.to_string_lossy()), 1, "nor one by its whole path");
    assert_eq!(unreadable("mine.assets/missing.png"), 1);
    assert_eq!(unreadable("file:///etc/passwd"), 1);
    #[cfg(unix)]
    {
        // A link inside the folder that leads out of it is not followed either.
        std::os::unix::fs::symlink(&secret, inside.join("mine.assets/link.png")).expect("a link");
        assert_eq!(unreadable("mine.assets/link.png"), 1);
    }
    done(&dir);
}

#[test]
fn a_picture_file_larger_than_a_picture_may_be_is_not_read() {
    let dir = scratch("bigpic");
    let doc = r#"{"format":"azboard/1","assets":{"img-1":{"src":"big.png"}},"items":[{"kind":"cutout","id":1,"x":0,"y":0,"width":50,"height":20,"image":{"asset":"img-1"},"source":{"path":"a.rs","line_start":1,"line_end":2}}]}"#;
    put(&dir.join("b.azboard.json"), doc);
    let file = fs::File::create(dir.join("big.png")).expect("created");
    file.set_len(store::MAX_PICTURE_BYTES as u64 + 1).expect("a sparse file");
    drop(file);
    let loaded = import(&dir.join("b.azboard.json"), &Codec, &LIMITS).expect("a board");
    assert_eq!(loaded.unreadable_pictures, 1);
    assert_eq!(loaded.data.items.len(), 1, "the cutout stays with its lines");
    done(&dir);
}
