//! Tests of the shader disk cache's directory rules: where an Android app keeps it, what is refused, what is capped, and
//! that a failed write is one diagnostic.

use std::path::{Path, PathBuf};

use super::*;

/// The cache under `base`, as an app hands it a directory.
fn open_in(base: &Path, renderer: &str, version: &str) -> Result<ShaderDiskCache, String> {
    ShaderDiskCache::open(Some(base), renderer, version)
}

/// A directory of this test in the temporary directory, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-shader-cache-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

#[test]
fn the_caps_are_a_count_and_a_size() {
    assert!(!over_cap(0, 0));
    assert!(!over_cap(MAX_CACHED_PROGRAMS, MAX_CACHE_BYTES), "the caps themselves are allowed");
    assert!(over_cap(MAX_CACHED_PROGRAMS + 1, 0), "one file too many");
    assert!(over_cap(1, MAX_CACHE_BYTES + 1), "one byte too many");
}

#[test]
fn a_directory_the_platform_hands_over_must_be_absolute_and_plain() {
    assert!(validate_base_dir(Path::new("/data/user/0/com.azul.AzWidgets/cache/azul/shaders")).is_ok());
    let relative = validate_base_dir(Path::new("cache/shaders")).unwrap_err();
    assert!(relative.contains("not an absolute path"), "{relative}");
    let parent = validate_base_dir(Path::new("/data/user/0/x/../../../etc")).unwrap_err();
    assert!(parent.contains(".."), "{parent}");
    let long = format!("/{}", "a".repeat(2000));
    assert!(validate_base_dir(Path::new(&long)).unwrap_err().contains("1024"));
}

/// The cache lives in the app's `cache` directory beside its `files` directory (the system empties it when storage
/// runs low, which is what a cache wants); its own directory in `files` is the second choice.
#[test]
fn an_android_app_keeps_the_programs_in_its_cache_directory_first() {
    let files = Path::new("/data/user/0/com.azul.AzWidgets/files");
    assert_eq!(
        android_cache_candidates(files),
        vec![
            PathBuf::from("/data/user/0/com.azul.AzWidgets/cache/azul/shaders"),
            PathBuf::from("/data/user/0/com.azul.AzWidgets/files/azul-shaders"),
        ]
    );
    // A path with no parent (the root) has only the second.
    assert_eq!(
        android_cache_candidates(Path::new("/")),
        vec![PathBuf::from("/azul-shaders")]
    );
}

/// The directory an app really gets: the first candidate whose app directory exists. `cache` is made by the system for every app, so
/// it is the first; an app directory with no `cache` (a device that has none) gets `files/azul-shaders`; no app directory, no cache.
#[test]
fn the_cache_directory_is_the_first_candidate_whose_app_directory_exists() {
    let files = Path::new("/data/user/0/com.azul.AzWidgets/files");
    let app = Path::new("/data/user/0/com.azul.AzWidgets");
    let cache = Path::new("/data/user/0/com.azul.AzWidgets/cache");
    // The system's `cache` directory is there: it holds the programs.
    assert_eq!(
        choose_android_cache_dir(files, |dir| dir == app || dir == cache),
        Some(PathBuf::from("/data/user/0/com.azul.AzWidgets/cache/azul/shaders"))
    );
    // No `cache` but the app's directory: the programs go beside the files.
    assert_eq!(
        choose_android_cache_dir(files, |dir| dir == app),
        Some(PathBuf::from("/data/user/0/com.azul.AzWidgets/files/azul-shaders"))
    );
    // Neither exists: the app keeps no programs and says so.
    assert_eq!(choose_android_cache_dir(files, |_| false), None);
}

#[test]
fn the_cache_is_one_directory_per_renderer_and_version_under_the_base() {
    let base = scratch("per-renderer");
    let mali = open_in(&base,"Mali-G78", "OpenGL ES 3.2 v1.r32p1").expect("a cache");
    assert!(mali.dir().starts_with(&base));
    assert!(mali.dir().is_dir(), "the directory was made");
    let name = mali.dir().file_name().and_then(|n| n.to_str()).expect("a name");
    assert_eq!(name.len(), 16, "{name}");
    assert!(name.chars().all(|c| c.is_ascii_hexdigit()), "{name}");

    let again = open_in(&base,"Mali-G78", "OpenGL ES 3.2 v1.r32p1").expect("a cache");
    assert_eq!(again.dir(), mali.dir(), "the same driver, the same directory");
    let other = open_in(&base,"Mali-G78", "OpenGL ES 3.2 v1.r33p0").expect("a cache");
    assert_ne!(other.dir(), mali.dir(), "a new driver version starts a new directory");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_relative_directory_is_refused_with_the_reason() {
    let err = open_in(Path::new("relative/dir"), "r", "v")
        .err()
        .expect("refused");
    assert!(err.contains("absolute"), "{err}");
}

#[test]
fn a_directory_that_cannot_be_made_is_a_reason_and_not_a_crash() {
    let base = scratch("blocked");
    // A FILE where the directory should be.
    let file = base.join("not-a-directory");
    std::fs::write(&file, b"x").expect("a file");
    let err = open_in(&file.join("shaders"), "r", "v")
        .err()
        .expect("cannot be made");
    assert!(err.contains("cannot make"), "{err}");
    let _ = std::fs::remove_dir_all(&base);
}

/// A directory with more programs than the cap holds is not the cache of this driver any more: it is emptied once, at
/// the open, and the cache fills up again.
#[test]
fn a_directory_over_the_cap_is_emptied_when_it_is_opened() {
    let base = scratch("over-cap");
    let first = open_in(&base,"r", "v").expect("a cache");
    let dir = first.dir().to_path_buf();
    for i in 0..=MAX_CACHED_PROGRAMS {
        std::fs::write(dir.join(format!("{i:016x}.bin")), b"x").expect("a program");
    }
    let count = |dir: &Path| std::fs::read_dir(dir).map(Iterator::count).unwrap_or(0);
    assert_eq!(count(&dir), MAX_CACHED_PROGRAMS + 1);
    let second = open_in(&base,"r", "v").expect("a cache");
    assert_eq!(second.dir(), dir);
    assert_eq!(count(&dir), 0, "emptied");

    // Exactly at the cap is kept.
    for i in 0..MAX_CACHED_PROGRAMS {
        std::fs::write(dir.join(format!("{i:016x}.bin")), b"x").expect("a program");
    }
    let _third = open_in(&base,"r", "v").expect("a cache");
    assert_eq!(count(&dir), MAX_CACHED_PROGRAMS, "kept");
    let _ = std::fs::remove_dir_all(&base);
}

/// A write that fails reports once: a full or read-only disk is one line, not one per program.
#[test]
fn a_failed_write_is_reported_once() {
    let base = scratch("write-failure");
    let cache = open_in(&base,"r", "v").expect("a cache");
    let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only file system");
    assert!(!cache.write_failure_reported.load(std::sync::atomic::Ordering::Relaxed));
    cache.report_write_failure(&error);
    assert!(cache.write_failure_reported.load(std::sync::atomic::Ordering::Relaxed));
    cache.report_write_failure(&error);
    let _ = std::fs::remove_dir_all(&base);
}
