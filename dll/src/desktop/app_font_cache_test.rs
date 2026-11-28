//! The font cache through a REAL registry (a directory with a variable font in it, a scan, a manifest on disk): what a
//! start does at each of the three outcomes, and that a start which finds nothing new writes nothing.
//!
//! The case this exists for: the first layout of an app makes static INSTANCES of its variable fonts (macOS's
//! `SFNS.ttf` asks for several), and an instance is a face of the registry that is in no manifest. A start that
//! counted the registry's faces at the end of the scan against the faces its hit loaded found "something new" on every
//! start (the macOS timeline of 2026-11-28: `fonts:cache_written Written, 797 faces` after `hit, 791 faces`, nine
//! starts of nine) and wrote the manifest and the key file again each time. The count that decides is the number of
//! DISK faces.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

use rust_fontconfig::{config::FcScanConfig, disk_cache::FontManifest, registry::FcFontRegistry};

use super::{persist_font_cache, start_font_cache};
use crate::desktop::font_cache_guard::{key_file_path, write_key_file, Key, Persisted, Reason, Verdict};

/// A directory that does not exist yet, named for the test (unique to this process, test and moment).
fn fresh_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "azul-appfontcache-{}-{nanos}-{:?}-{name}",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// A directory of fonts with one variable font in it.
fn font_dir(dir: &Path) -> PathBuf {
    let fonts = dir.join("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../doc/fonts/RedHatMono-VariableFont_wght.ttf");
    std::fs::copy(&source, fonts.join("RedHatMono.ttf")).expect("the variable font of the repository");
    fonts
}

/// A registry that scans `fonts` only and writes nothing of its own (the app's registry is made the same way).
fn registry_over(fonts: &Path) -> Arc<FcFontRegistry> {
    let registry = FcFontRegistry::new_with_config(FcScanConfig {
        font_dirs: vec![fonts.to_path_buf()],
        priority_families: Vec::new(),
    });
    registry.set_persist_on_complete(false);
    registry
}

/// Runs the scan to its end.
fn scan(registry: &Arc<FcFontRegistry>) {
    registry.spawn_scout_and_builders();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !registry.is_build_complete() {
        assert!(Instant::now() < deadline, "the font scan did not complete in 60 s");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn key_for(registry: &FcFontRegistry) -> Key {
    Key::current(FontManifest::CURRENT_VERSION, registry.scan_dirs())
}

/// The modification time and the bytes of a file.
fn state_of(path: &Path) -> (SystemTime, Vec<u8>) {
    (
        std::fs::metadata(path).unwrap().modified().unwrap(),
        std::fs::read(path).unwrap(),
    )
}

/// Sets the modification time of `path` two days into the past (so that a rewrite shows in it).
fn age(path: &Path) {
    let past = SystemTime::now() - Duration::from_secs(2 * 24 * 3600);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(past)
        .unwrap();
}

#[test]
fn a_start_that_finds_nothing_new_writes_nothing_even_after_the_layout_made_instances() {
    let dir = fresh_dir("cycle");
    let fonts = font_dir(&dir);
    let manifest = dir.join("cache").join("rfc").join("fonts").join("manifest.bin");

    // Start 1: no manifest. The scan finds the font and the end of the scan writes the pair.
    let first = registry_over(&fonts);
    let key = key_for(&first);
    let started = start_font_cache(&first, Some(&manifest), &key);
    assert_eq!(started.verdict, Verdict::Miss);
    assert_eq!(started.loaded_faces, None);
    scan(&first);
    let (persisted, disk_faces) = persist_font_cache(&first, &manifest, &key, started.loaded_faces);
    assert_eq!(persisted, Persisted::Written);
    assert!(disk_faces >= 1, "the scan found the font");
    assert!(manifest.exists() && key_file_path(&manifest).exists());

    // Start 2: a hit. A layout then asks for an instance of the variable font: a face of the registry that is in no
    // manifest.
    let second = registry_over(&fonts);
    let started = start_font_cache(&second, Some(&manifest), &key);
    assert_eq!(started.verdict, Verdict::Hit, "{}", started.verdict.describe());
    assert_eq!(started.loaded_faces, Some(disk_faces));
    assert!(second.is_cache_loaded(), "a hit is a loaded cache: a layout does not wait for the scan");
    scan(&second);
    let font = second
        .list()
        .into_iter()
        .map(|(_, id)| id)
        .find(|id| second.get_disk_font_path(id).is_some())
        .expect("the font of the manifest");
    assert!(
        second.cache.variable_instance(&font, &[(*b"wght", 350.0)]).is_some(),
        "premise: the variable font makes an instance"
    );
    assert!(
        second.cache.len() > disk_faces,
        "premise: the registry holds a face that is not on disk"
    );

    // The end of the scan: nothing is written, not a byte and not a modification time.
    age(&manifest);
    age(&key_file_path(&manifest));
    let before = (state_of(&manifest), state_of(&key_file_path(&manifest)));
    let (persisted, again) = persist_font_cache(&second, &manifest, &key, started.loaded_faces);
    assert_eq!(persisted, Persisted::Unchanged, "the instances count as new faces");
    assert_eq!(again, disk_faces);
    assert_eq!(
        (state_of(&manifest), state_of(&key_file_path(&manifest))),
        before,
        "a hit that found nothing new rewrote the cache"
    );
}

#[test]
fn a_font_that_changed_is_rebuilt_and_the_next_start_is_a_hit() {
    let dir = fresh_dir("changed");
    let fonts = font_dir(&dir);
    let manifest = dir.join("cache").join("rfc").join("fonts").join("manifest.bin");
    let first = registry_over(&fonts);
    let key = key_for(&first);
    let started = start_font_cache(&first, Some(&manifest), &key);
    scan(&first);
    assert_eq!(
        persist_font_cache(&first, &manifest, &key, started.loaded_faces).0,
        Persisted::Written
    );

    // The file is replaced by another one of another size (an OS update).
    let file = fonts.join("RedHatMono.ttf");
    let mut bytes = std::fs::read(&file).unwrap();
    bytes.extend_from_slice(&[0u8; 16]);
    std::fs::write(&file, &bytes).unwrap();

    let second = registry_over(&fonts);
    let started = start_font_cache(&second, Some(&manifest), &key);
    assert!(
        matches!(&started.verdict, Verdict::Rebuilt(Reason::FontsChanged { changed: 1, gone: 0, .. })),
        "{}",
        started.verdict.describe()
    );
    assert_eq!(started.loaded_faces, None, "a stale cache puts nothing in the registry");
    assert!(!second.is_cache_loaded(), "a layout waits for the scan, as without a cache");
    scan(&second);
    let (persisted, _) = persist_font_cache(&second, &manifest, &key, started.loaded_faces);
    assert_eq!(persisted, Persisted::Written);

    let third = registry_over(&fonts);
    let started = start_font_cache(&third, Some(&manifest), &key);
    assert_eq!(started.verdict, Verdict::Hit, "{}", started.verdict.describe());
}

#[test]
fn a_new_font_is_found_by_the_scan_after_a_hit_and_the_pair_is_written_again() {
    let dir = fresh_dir("newfont");
    let fonts = font_dir(&dir);
    let manifest = dir.join("cache").join("rfc").join("fonts").join("manifest.bin");
    let first = registry_over(&fonts);
    let key = key_for(&first);
    let started = start_font_cache(&first, Some(&manifest), &key);
    scan(&first);
    let (_, one_font) = persist_font_cache(&first, &manifest, &key, started.loaded_faces);

    // A font of another family is installed while the app is not running.
    let other = Path::new(env!("CARGO_MANIFEST_DIR")).join("../layout/tests/fonts/azul-mock-kern.ttf");
    std::fs::copy(&other, fonts.join("AzulMockKern.ttf")).expect("a second font of the repository");

    let second = registry_over(&fonts);
    let started = start_font_cache(&second, Some(&manifest), &key);
    assert_eq!(started.verdict, Verdict::Hit, "a font that is not in the manifest does not make it stale");
    assert_eq!(started.loaded_faces, Some(one_font));
    scan(&second);
    let (persisted, two_fonts) = persist_font_cache(&second, &manifest, &key, started.loaded_faces);
    assert!(two_fonts > one_font, "the scan found the new file: {two_fonts} faces after {one_font}");
    assert_eq!(persisted, Persisted::Written);

    let third = registry_over(&fonts);
    let started = start_font_cache(&third, Some(&manifest), &key);
    assert_eq!(started.verdict, Verdict::Hit);
    assert_eq!(started.loaded_faces, Some(two_fonts));
}

#[test]
fn a_manifest_the_key_file_vouches_for_and_the_library_refuses_is_a_rebuild() {
    let dir = fresh_dir("refused");
    let fonts = font_dir(&dir);
    let manifest = dir.join("cache").join("rfc").join("fonts").join("manifest.bin");
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    // Bytes no manifest has, and a key file written for exactly them.
    std::fs::write(&manifest, [0xde, 0xad, 0xbe, 0xef, 1, 2, 3]).unwrap();
    let registry = registry_over(&fonts);
    let key = key_for(&registry);
    write_key_file(&manifest, &key, Vec::new()).unwrap();

    let started = start_font_cache(&registry, Some(&manifest), &key);
    assert_eq!(started.verdict, Verdict::Rebuilt(Reason::ManifestRejected));
    assert_eq!(started.loaded_faces, None);
    assert!(!registry.is_cache_loaded());
}

#[test]
fn no_cache_directory_is_no_cache_and_not_an_error() {
    let dir = fresh_dir("nodir");
    let fonts = font_dir(&dir);
    let registry = registry_over(&fonts);
    let key = key_for(&registry);
    let started = start_font_cache(&registry, None, &key);
    assert_eq!(started.verdict, Verdict::NoDirectory);
    assert_eq!(started.loaded_faces, None);
}
