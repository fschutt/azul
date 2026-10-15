//! A project is files under `videocut/<uuid>/` on a Drive.

use azul_pim::write_queue::WriteQueue;
use azul_storage::{local::LocalDrive, Drive};

use super::*;
use crate::model::{MediaItem, Pattern, Project};

fn temp_root(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "azvideocut-store-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn a_project_lives_under_videocut_and_its_uuid() {
    assert_eq!(project_key("1234"), "videocut/1234/project.json");
    assert_eq!(media_key("1234", "pier.mp4"), "videocut/1234/media/pier.mp4");
    assert_eq!(media_key("1234", "a/b\\c.mp4"), "videocut/1234/media/a-b-c.mp4");
    assert_eq!(export_key("1234", "cut.mp4"), "videocut/1234/exports/cut.mp4");
    assert_eq!(project_id_of("videocut/1234/project.json"), Some("1234"));
    assert_eq!(project_id_of("videocut/1234/media/x.mp4"), None);
}

#[test]
fn a_project_saves_and_loads_through_a_drive_and_the_projects_are_listed() {
    let root = temp_root("roundtrip");
    let drive = LocalDrive::new(&root);
    let mut p = Project::create("abcd".into(), "Teaser".into(), 64, 36, 25);
    p.add_media(MediaItem::generated("bars", Pattern::Bars, 50, 64, 36));
    save_project(&drive, &p).expect("save");
    assert!(root.join("videocut/abcd/project.json").is_file(), "a plain file on disk");
    let back = load_project(&drive, "abcd").expect("load");
    assert_eq!(back.media, p.media);
    assert_eq!(back.name, "Teaser");
    let mut other = Project::create("efgh".into(), "Other".into(), 64, 36, 25);
    other.name = "Other".into();
    save_project(&drive, &other).expect("save other");
    let mut ids = list_projects(&drive).expect("list");
    ids.sort();
    assert_eq!(ids, vec!["abcd".to_string(), "efgh".to_string()]);
    drive.put(&media_key("abcd", "x.bin"), b"123").expect("put media");
    assert_eq!(drive.get(&media_key("abcd", "x.bin")).expect("get"), b"123".to_vec());
    let _ = std::fs::remove_dir_all(&root);
}

/// Saves follow every edit; two that overlap must not land out of order. One save is in
/// flight at a time, and only the newest project waits behind it.
#[test]
fn overlapping_saves_land_in_order_and_the_last_edit_wins() {
    let mut queue = WriteQueue::new();
    let mut p = Project::create("abcd".into(), "Teaser".into(), 64, 36, 25);
    queue_project(&mut queue, &p);
    let first = queue.take().expect("the first save starts");
    assert_eq!(first.len(), 1);
    p.name = "Second".into();
    queue_project(&mut queue, &p);
    p.name = "Third".into();
    queue_project(&mut queue, &p);
    assert!(queue.take().is_none(), "one save in flight");
    queue.finish(Vec::new());
    let next = queue.take().expect("the newest save follows");
    assert_eq!(next.len(), 1, "the older waiting save was replaced");
    let azul_pim::write_queue::Write::Put { key, bytes } = &next[0] else {
        panic!("a save puts the file");
    };
    assert_eq!(key, &project_key("abcd"));
    let saved = Project::from_json(std::str::from_utf8(bytes).expect("utf-8")).expect("json");
    assert_eq!(saved.name, "Third");
}
