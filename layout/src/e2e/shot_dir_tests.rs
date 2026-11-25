//! The folder of `AZ_E2E_SHOT_DIR` is the window's setting, not the process's: the thread that
//! encodes a screenshot's PNG has no window to ask, so the pending screenshot carries the folder
//! its window had.

use std::path::PathBuf;

use super::DebugResponseData;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-shot-dir-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    dir
}

#[test]
fn a_screenshot_is_copied_to_the_folder_its_window_was_given() {
    let dir = scratch("copied");
    let pixmap = crate::cpurender::AzulPixmap::new(4, 2).expect("a 4x2 pixmap");
    let response = DebugResponseData::pending_screenshot(pixmap, Some(dir.clone())).into_ready();
    assert!(!response.is_pending(), "the PNG was encoded");

    let names: Vec<String> = std::fs::read_dir(&dir)
        .expect("the folder was made")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1, "one screenshot, one file: {names:?}");
    assert!(
        names[0].starts_with("shot-") && names[0].ends_with(".png"),
        "numbered in capture order: {names:?}"
    );
    let bytes = std::fs::read(dir.join(&names[0])).expect("the file");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "it is a PNG");
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn a_screenshot_of_a_window_without_a_folder_is_copied_nowhere() {
    let dir = scratch("nowhere");
    let pixmap = crate::cpurender::AzulPixmap::new(4, 2).expect("a 4x2 pixmap");
    let response = DebugResponseData::pending_screenshot(pixmap, None).into_ready();
    assert!(!response.is_pending(), "the PNG was encoded all the same");
    assert!(!dir.exists(), "no folder, no copy");
}
