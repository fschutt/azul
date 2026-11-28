//! The companion's file without a window: what is read of it, and that what is written is for the user only, whole, and not longer
//! than the companion's file may be.

use std::path::PathBuf;

use super::*;

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("azreview-companion-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

#[test]
fn no_file_is_no_text_and_no_problem() {
    let dir = scratch("none");
    assert_eq!(look(&dir.join("nothing.json")), Loaded { text: None, problem: None });
}

#[test]
fn a_file_is_read_as_text() {
    let dir = scratch("read");
    std::fs::write(dir.join("c.json"), "{\"enabled\": true}\n").unwrap();
    assert_eq!(look(&dir.join("c.json")), Loaded { text: Some("{\"enabled\": true}\n".to_string()), problem: None });
}

#[test]
fn a_folder_where_the_file_should_be_is_a_problem_and_not_a_crash() {
    let dir = scratch("folder");
    std::fs::create_dir_all(dir.join("c.json")).unwrap();
    let found = look(&dir.join("c.json"));
    assert_eq!(found.text, None);
    assert!(found.problem.is_some(), "{found:?}");
}

#[test]
fn a_file_longer_than_any_the_companion_writes_is_not_used_and_the_cap_itself_is() {
    let dir = scratch("long");
    let cap = usize::try_from(MAX_FILE_BYTES).unwrap();
    std::fs::write(dir.join("long.json"), vec![b'x'; cap + 4_000]).unwrap();
    let found = look(&dir.join("long.json"));
    assert_eq!(found.text, None, "only its first bytes were looked at, and that was enough to tell");
    assert!(found.problem.as_deref().is_some_and(|p| p.contains("longer")), "{found:?}");
    std::fs::write(dir.join("cap.json"), vec![b'x'; cap]).unwrap();
    assert_eq!(look(&dir.join("cap.json")).text.map(|t| t.len()), Some(cap));
}

#[test]
fn a_file_that_is_not_text_is_a_problem() {
    let dir = scratch("binary");
    std::fs::write(dir.join("b.json"), [0xFF, 0xFE, 0x00, 0x80]).unwrap();
    let found = look(&dir.join("b.json"));
    assert_eq!(found.text, None);
    assert!(found.problem.as_deref().is_some_and(|p| p.contains("not text")), "{found:?}");
}

#[test]
fn a_write_makes_its_folders_and_the_file_reads_back() {
    let dir = scratch("write");
    let path = dir.join(".azlin").join("companion").join("review.json");
    write_private(&path, b"{\"a\": 1}\n").expect("written");
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"a\": 1}\n");
}

#[cfg(unix)]
#[test]
fn what_is_written_is_for_the_user_only_the_folders_and_the_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = scratch("private");
    let path = dir.join(".azlin").join("companion").join("review.json");
    write_private(&path, b"secret").expect("written");
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path) & 0o077, 0, "no access for the group or the others: {:o}", mode(&path));
    assert_eq!(mode(path.parent().unwrap()) & 0o077, 0, "nor to the folder it is in: {:o}", mode(path.parent().unwrap()));
    assert_eq!(mode(path.parent().unwrap().parent().unwrap()) & 0o077, 0, "nor to the one above it");
}

#[cfg(unix)]
#[test]
fn a_file_that_was_open_to_others_is_not_after_it_is_written_again() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = scratch("widen");
    let path = dir.join("c.json");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    write_private(&path, b"new").expect("written");
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
}

#[test]
fn a_write_replaces_the_whole_file_and_leaves_nothing_beside_it() {
    let dir = scratch("replace");
    let path = dir.join("review.json");
    write_private(&path, &vec![b'a'; 5_000]).unwrap();
    write_private(&path, b"short").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"short");
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["review.json".to_string()], "no temporary file is left: {names:?}");
}

#[test]
fn a_write_longer_than_the_file_may_be_is_refused_and_the_old_file_stays() {
    let dir = scratch("toolong");
    let path = dir.join("review.json");
    write_private(&path, b"kept").unwrap();
    let cap = usize::try_from(MAX_FILE_BYTES).unwrap();
    let refused = write_private(&path, &vec![b'x'; cap + 1]);
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(std::fs::read(&path).unwrap(), b"kept");
    write_private(&path, &vec![b'x'; cap]).expect("the cap itself is written");
}

#[test]
fn a_write_under_a_file_says_so_and_leaves_nothing() {
    let dir = scratch("underfile");
    std::fs::write(dir.join("file"), "x").unwrap();
    let refused = write_private(&dir.join("file").join("c.json"), b"x");
    assert!(refused.is_err(), "{refused:?}");
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, vec!["file".to_string()], "{names:?}");
}

#[test]
fn a_message_that_is_not_the_answer_is_none_and_the_answer_is_taken_whole() {
    let mut not_it = RefAny::new(5_u32);
    assert!(take_loaded(&mut not_it).is_none());
    assert!(take_saved(&mut not_it).is_none());
    let mut loaded = RefAny::new(Some(Loaded { text: Some("x".to_string()), problem: None }));
    assert_eq!(take_loaded(&mut loaded).and_then(|l| l.text), Some("x".to_string()));
    assert!(take_loaded(&mut loaded).is_none(), "taken once");
    let mut saved = RefAny::new(Some(Saved { error: None }));
    assert_eq!(take_saved(&mut saved), Some(Saved { error: None }));
    assert_eq!(take_saved(&mut saved), None);
}
