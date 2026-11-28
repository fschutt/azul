//! The check of the font manifest: a hit only when the manifest, its key file and every font file are what they were,
//! and, for each way they can differ, a verdict that names it (a cut manifest, a damaged one, one of another process,
//! another build of azul, another OS build, other scanned directories, a font that changed, a font that is gone, a key
//! file that is missing, cut or damaged). The writers: the pair of files is written atomically, two writers never
//! share a temporary file, a reader never sees a mix. Every test works in a directory of its own below the temp
//! directory, and nothing it makes is deleted by it.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use super::*;

/// A directory that does not exist yet, named for the test (unique to this process, test and moment).
fn fresh_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "azul-fontcache-{}-{nanos}-{:?}-{name}",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// The key of the tests.
fn key() -> Key {
    Key::new(
        "0.2.0",
        3,
        "24F74",
        &[PathBuf::from("/System/Library/Fonts"), PathBuf::from("/Library/Fonts")],
    )
}

/// `count` font files (any bytes will do: the check reads sizes and times) in `dir/fonts`, of different sizes.
fn make_fonts(dir: &Path, count: usize) -> Vec<String> {
    let fonts = dir.join("fonts");
    std::fs::create_dir_all(&fonts).unwrap();
    (0..count)
        .map(|n| {
            let path = fonts.join(format!("Font {n}.ttf"));
            std::fs::write(&path, vec![u8::try_from(n % 251).unwrap(); 100 + n]).unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect()
}

/// The manifest `bytes` at `dir/manifest.bin` and the key file the persist step writes after it, for `fonts`.
fn write_pair(dir: &Path, bytes: &[u8], key: &Key, fonts: &[String]) -> PathBuf {
    let manifest = dir.join("rfc").join("fonts").join("manifest.bin");
    write_atomic(&manifest, bytes).unwrap();
    write_key_file(&manifest, key, stats_of(fonts.iter().cloned())).unwrap();
    manifest
}

/// A manifest-like blob of `len` bytes (a hash and a length are all that is asked of it).
fn blob(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 7 % 256) as u8).collect()
}

#[test]
fn a_manifest_with_its_key_file_under_the_same_key_is_a_hit() {
    let dir = fresh_dir("hit");
    let fonts = make_fonts(&dir, 12);
    let manifest = write_pair(&dir, &blob(5000), &key(), &fonts);
    let verdict = check(&manifest, &key());
    assert_eq!(verdict, Verdict::Hit, "{}", verdict.describe());
    assert!(verdict.is_hit());
    // Asking again changes nothing: a check writes nothing.
    assert_eq!(check(&manifest, &key()), Verdict::Hit);
}

#[test]
fn no_manifest_is_a_miss_even_when_a_key_file_is_left_behind() {
    let dir = fresh_dir("miss");
    let manifest = dir.join("rfc").join("fonts").join("manifest.bin");
    assert_eq!(check(&manifest, &key()), Verdict::Miss);
    // The key file of a manifest that was moved away says nothing about the one that is not there.
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(key_file_path(&manifest), "left behind").unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Miss);
}

#[test]
fn a_manifest_without_a_key_file_is_rebuilt() {
    // What a build without this check left behind: a manifest nobody vouches for.
    let dir = fresh_dir("nokey");
    let manifest = dir.join("rfc").join("fonts").join("manifest.bin");
    write_atomic(&manifest, &blob(900)).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Rebuilt(Reason::KeyFileMissing));
}

#[test]
fn a_cut_manifest_is_rebuilt() {
    let dir = fresh_dir("cut");
    let fonts = make_fonts(&dir, 8);
    let bytes = blob(4096);
    let manifest = write_pair(&dir, &bytes, &key(), &fonts);
    std::fs::write(&manifest, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Rebuilt(Reason::ManifestChanged));
    // Down to nothing at all.
    std::fs::write(&manifest, b"").unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Rebuilt(Reason::ManifestChanged));
}

#[test]
fn a_damaged_manifest_of_the_same_length_is_rebuilt() {
    let dir = fresh_dir("flip");
    let fonts = make_fonts(&dir, 8);
    let mut bytes = blob(4096);
    let manifest = write_pair(&dir, &bytes, &key(), &fonts);
    bytes[2000] ^= 0x01;
    std::fs::write(&manifest, &bytes).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Rebuilt(Reason::ManifestChanged));
}

#[test]
fn a_manifest_another_process_wrote_after_the_key_file_is_rebuilt() {
    let dir = fresh_dir("other");
    let fonts = make_fonts(&dir, 8);
    let manifest = write_pair(&dir, &blob(3000), &key(), &fonts);
    write_atomic(&manifest, &blob(3100)).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Rebuilt(Reason::ManifestChanged));
    // The process that wrote it writes its key file next: the pair is a hit again.
    write_key_file(&manifest, &key(), stats_of(fonts.iter().cloned())).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Hit);
}

#[test]
fn another_build_of_azul_or_another_manifest_format_is_rebuilt() {
    let dir = fresh_dir("version");
    let fonts = make_fonts(&dir, 4);
    let manifest = write_pair(&dir, &blob(800), &key(), &fonts);
    let mut newer_azul = key();
    newer_azul.azul_version = "0.2.1".to_string();
    let verdict = check(&manifest, &newer_azul);
    assert!(
        matches!(&verdict, Verdict::Rebuilt(Reason::VersionChanged { was, now })
            if was.contains("0.2.0") && now.contains("0.2.1")),
        "{verdict:?}"
    );
    let mut newer_format = key();
    newer_format.manifest_version = 4;
    assert!(matches!(
        check(&manifest, &newer_format),
        Verdict::Rebuilt(Reason::VersionChanged { .. })
    ));
}

#[test]
fn an_os_update_is_rebuilt_and_the_verdict_names_both_builds() {
    let dir = fresh_dir("os");
    let fonts = make_fonts(&dir, 4);
    let manifest = write_pair(&dir, &blob(800), &key(), &fonts);
    let updated = Key::new(
        "0.2.0",
        3,
        "24G90",
        &[PathBuf::from("/System/Library/Fonts"), PathBuf::from("/Library/Fonts")],
    );
    let verdict = check(&manifest, &updated);
    assert_eq!(
        verdict,
        Verdict::Rebuilt(Reason::OsBuildChanged {
            was: "24F74".to_string(),
            now: "24G90".to_string()
        })
    );
    assert_eq!(verdict.describe(), "rebuilt (os build changed: 24F74 -> 24G90)");
}

#[test]
fn other_scanned_directories_are_rebuilt() {
    let dir = fresh_dir("dirs");
    let fonts = make_fonts(&dir, 4);
    let manifest = write_pair(&dir, &blob(800), &key(), &fonts);
    let more = Key::new(
        "0.2.0",
        3,
        "24F74",
        &[
            PathBuf::from("/System/Library/Fonts"),
            PathBuf::from("/Library/Fonts"),
            PathBuf::from("/Applications/App.app/Contents/Resources/fonts"),
        ],
    );
    assert_eq!(check(&manifest, &more), Verdict::Rebuilt(Reason::ScanDirsChanged));
    // The order is part of the configuration: the registry's first directory wins a tie.
    let swapped = Key::new(
        "0.2.0",
        3,
        "24F74",
        &[PathBuf::from("/Library/Fonts"), PathBuf::from("/System/Library/Fonts")],
    );
    assert_eq!(check(&manifest, &swapped), Verdict::Rebuilt(Reason::ScanDirsChanged));
}

#[test]
fn a_font_that_changed_in_size_or_in_time_is_rebuilt() {
    let dir = fresh_dir("changed");
    let fonts = make_fonts(&dir, 10);
    let manifest = write_pair(&dir, &blob(1500), &key(), &fonts);

    // Another size (an OS update that replaced the file).
    std::fs::write(&fonts[3], vec![9u8; 777]).unwrap();
    let verdict = check(&manifest, &key());
    assert!(
        matches!(&verdict, Verdict::Rebuilt(Reason::FontsChanged { changed: 1, gone: 0, first }) if first == &fonts[3]),
        "{verdict:?}"
    );
    assert!(verdict.describe().contains("1 font files changed"), "{}", verdict.describe());

    // The same size and another time (a file written again).
    let manifest = write_pair(&fresh_dir("changed-time"), &blob(1500), &key(), &fonts);
    let (size, _) = stat_of(&fonts[5]).unwrap();
    let later = SystemTime::now() + Duration::from_secs(3600);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&fonts[5])
        .unwrap()
        .set_modified(later)
        .unwrap();
    assert_eq!(stat_of(&fonts[5]).unwrap().0, size, "premise: the size is the same");
    assert!(matches!(
        check(&manifest, &key()),
        Verdict::Rebuilt(Reason::FontsChanged { changed: 1, .. })
    ));
}

#[test]
fn a_font_that_is_gone_is_rebuilt() {
    let dir = fresh_dir("gone");
    let fonts = make_fonts(&dir, 6);
    let manifest = write_pair(&dir, &blob(1500), &key(), &fonts);
    // The font of this test's own directory, removed by the test.
    std::fs::remove_file(&fonts[1]).unwrap();
    let verdict = check(&manifest, &key());
    assert!(
        matches!(&verdict, Verdict::Rebuilt(Reason::FontsChanged { changed: 0, gone: 1, first }) if first == &fonts[1]),
        "{verdict:?}"
    );
}

#[test]
fn the_cheap_comparisons_come_first() {
    // A changed key and a changed font: the key is named, no font file is opened for it.
    let dir = fresh_dir("order");
    let fonts = make_fonts(&dir, 5);
    let manifest = write_pair(&dir, &blob(1500), &key(), &fonts);
    std::fs::write(&fonts[0], b"changed").unwrap();
    let mut other = key();
    other.os_build = "25A1".to_string();
    assert!(matches!(
        check(&manifest, &other),
        Verdict::Rebuilt(Reason::OsBuildChanged { .. })
    ));
}

#[test]
fn a_key_file_that_is_cut_or_damaged_is_rebuilt_whatever_is_wrong_with_it() {
    let dir = fresh_dir("keyfile");
    let fonts = make_fonts(&dir, 5);
    let manifest = write_pair(&dir, &blob(1500), &key(), &fonts);
    let good = std::fs::read_to_string(key_file_path(&manifest)).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Hit, "premise");

    // Cut after every line, and in the middle of every line: never a hit, always damaged.
    let mut cuts: Vec<usize> = good.match_indices('\n').map(|(at, _)| at + 1).collect();
    cuts.extend(good.match_indices('\n').map(|(at, _)| at.saturating_sub(3)));
    cuts.push(0);
    for cut in cuts {
        if cut >= good.len() {
            continue;
        }
        std::fs::write(key_file_path(&manifest), &good.as_bytes()[..cut]).unwrap();
        let verdict = check(&manifest, &key());
        assert!(
            matches!(verdict, Verdict::Rebuilt(Reason::KeyFileDamaged(_))),
            "cut at {cut} of {}: {verdict:?}",
            good.len()
        );
    }

    // Garbage, another format number, a count that is not the number of lines, a number that is not one, bytes that are no text.
    for damaged in [
        "garbage".to_string(),
        good.replacen("azul-font-cache 1", "azul-font-cache 2", 1),
        good.replacen("fonts 5", "fonts 4", 1),
        good.replacen("fonts 5", "fonts 6", 1),
        good.replacen("manifest 1500", "manifest fifteen", 1),
        good.replacen("dirs ", "dirs zz", 1),
        good.replace("end\n", ""),
        good.replace("\nos 24F74\n", "\n"),
    ] {
        std::fs::write(key_file_path(&manifest), damaged.as_bytes()).unwrap();
        let verdict = check(&manifest, &key());
        assert!(
            matches!(verdict, Verdict::Rebuilt(Reason::KeyFileDamaged(_))),
            "{damaged:?}: {verdict:?}"
        );
    }
    std::fs::write(key_file_path(&manifest), [0xff, 0xfe, 0x00, 0x80]).unwrap();
    assert!(matches!(check(&manifest, &key()), Verdict::Rebuilt(Reason::KeyFileDamaged(_))));
}

#[test]
fn the_key_file_round_trips_with_spaces_unicode_an_empty_os_build_and_no_fonts() {
    let file = KeyFile {
        key: Key::new("0.2.0", 3, "", &[]),
        manifest_len: 123_456,
        manifest_hash: 0xdead_beef_0123_4567,
        fonts: vec![
            FontStat {
                path: "/Library/Fonts/Some Font Bold Italic.ttf".to_string(),
                size: 1_048_576,
                mtime_secs: 1_700_000_000,
            },
            FontStat {
                path: "/home/nutzer/.fonts/Schrift \u{fc}ber alles.otf".to_string(),
                size: 7,
                mtime_secs: 0,
            },
        ],
    };
    assert_eq!(KeyFile::parse(&file.to_text()).unwrap(), file);
    let empty = KeyFile {
        key: Key::new("0.2.0", 3, "ubuntu 22.04", &[PathBuf::from("/usr/share/fonts")]),
        manifest_len: 0,
        manifest_hash: 0,
        fonts: Vec::new(),
    };
    assert_eq!(KeyFile::parse(&empty.to_text()).unwrap(), empty);
}

#[test]
fn a_path_with_a_line_break_is_left_to_the_scan_and_never_breaks_the_file() {
    let file = KeyFile {
        key: key(),
        manifest_len: 1,
        manifest_hash: 2,
        fonts: vec![
            FontStat {
                path: "/fonts/a\nb.ttf".to_string(),
                size: 1,
                mtime_secs: 1,
            },
            FontStat {
                path: "/fonts/ok.ttf".to_string(),
                size: 2,
                mtime_secs: 2,
            },
        ],
    };
    let parsed = KeyFile::parse(&file.to_text()).unwrap();
    assert_eq!(parsed.fonts.len(), 1);
    assert_eq!(parsed.fonts[0].path, "/fonts/ok.ttf");
}

#[test]
fn a_key_is_one_line_per_field() {
    let key = Key::new("0.2.0\n", 3, "24F74\r\nsecond line", &[]);
    assert_eq!(key.os_build, "24F74  second line");
    assert_eq!(key.azul_version, "0.2.0");
    let file = KeyFile {
        key,
        manifest_len: 1,
        manifest_hash: 1,
        fonts: Vec::new(),
    };
    assert_eq!(KeyFile::parse(&file.to_text()).unwrap(), file);
}

#[test]
fn the_hash_of_the_directories_follows_their_order_and_spelling() {
    let a = [PathBuf::from("/a"), PathBuf::from("/b")];
    let b = [PathBuf::from("/b"), PathBuf::from("/a")];
    let c = [PathBuf::from("/a/"), PathBuf::from("/b")];
    assert_eq!(dirs_hash(&a), dirs_hash(&a));
    assert_ne!(dirs_hash(&a), dirs_hash(&b));
    assert_ne!(dirs_hash(&a), dirs_hash(&c));
    // "/a" "/b" is not the one directory "/a\n/b" (a name cannot hold the separator, a pair is not a name).
    assert_ne!(dirs_hash(&a), dirs_hash(&[PathBuf::from("/a/b")]));
}

#[test]
fn many_fonts_are_checked_on_threads_and_agree_with_one_thread() {
    let dir = fresh_dir("parallel");
    let fonts = make_fonts(&dir, 300);
    let stats = stats_of(fonts.iter().cloned());
    assert_eq!(stats.len(), 300);
    let clean = check_fonts(&stats);
    assert_eq!((clean.changed, clean.gone), (0, 0));

    // Change a font in the second quarter and in the last, remove one in the third: the counts add up across the
    // threads and `first` is the first of the manifest's order, whichever thread found it.
    std::fs::write(&fonts[90], b"x").unwrap();
    std::fs::write(&fonts[299], b"y").unwrap();
    std::fs::remove_file(&fonts[170]).unwrap();
    let parallel = check_fonts(&stats);
    let sequential = check_slice(&stats);
    assert_eq!((parallel.changed, parallel.gone), (2, 1));
    assert_eq!((parallel.changed, parallel.gone), (sequential.changed, sequential.gone));
    assert_eq!(parallel.first.as_deref(), Some(fonts[90].as_str()));
    assert_eq!(parallel.first, sequential.first);
}

#[test]
fn a_font_listed_twice_or_unreadable_is_handled_by_the_stats() {
    let dir = fresh_dir("stats");
    let fonts = make_fonts(&dir, 3);
    let mut paths = fonts.clone();
    paths.push(fonts[0].clone());
    paths.push(dir.join("fonts").join("never made.ttf").to_string_lossy().into_owned());
    let stats = stats_of(paths);
    assert_eq!(stats.len(), 3, "once each, and not the one that cannot be opened");
}

#[test]
fn the_key_file_describes_the_manifest_that_is_on_disk_when_it_is_written() {
    let dir = fresh_dir("describes");
    let fonts = make_fonts(&dir, 4);
    let manifest = write_pair(&dir, &blob(2000), &key(), &fonts);
    // Another manifest replaces it: not a hit until a key file is written for it.
    write_atomic(&manifest, &blob(2200)).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Rebuilt(Reason::ManifestChanged));
    write_key_file(&manifest, &key(), stats_of(fonts.iter().cloned())).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Hit);
}

#[test]
fn writing_makes_the_directory_and_leaves_no_temporary_file() {
    let dir = fresh_dir("write");
    let path = dir.join("a").join("b").join("c.bin");
    write_atomic(&path, b"first").unwrap();
    write_atomic(&path, b"second, longer").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second, longer");
    let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["c.bin".to_string()], "no temporary file is left: {names:?}");
}

#[test]
fn a_write_that_cannot_happen_is_an_error_and_leaves_nothing() {
    let dir = fresh_dir("blocked");
    std::fs::create_dir_all(&dir).unwrap();
    // The directory of the file is a file.
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, b"a file").unwrap();
    assert!(write_atomic(&blocker.join("inner.bin"), b"x").is_err());
    // The target is a directory: the rename fails and the temporary file goes.
    let target = dir.join("target");
    std::fs::create_dir_all(&target).unwrap();
    assert!(write_atomic(&target, b"x").is_err());
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.contains(".tmp-")), "{names:?}");
}

#[test]
fn concurrent_writers_never_leave_a_mix_and_never_share_a_temporary_file() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let dir = fresh_dir("concurrent");
    let path = dir.join("manifest.bin");
    write_atomic(&path, &vec![0u8; 64 * 1024]).unwrap();
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut reads = 0u32;
            // At least one read, whenever the writers are done.
            loop {
                let bytes = std::fs::read(&path).unwrap();
                assert_eq!(bytes.len(), 64 * 1024, "a reader saw a file of another length");
                assert!(
                    bytes.iter().all(|b| *b == bytes[0]),
                    "a reader saw a mix of two writers"
                );
                reads += 1;
                if done.load(Ordering::Acquire) {
                    break;
                }
            }
            reads
        });
        let writers: Vec<_> = (1u8..=8)
            .map(|n| {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..40 {
                        write_atomic(path, &vec![n; 64 * 1024]).expect("a writer failed");
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        done.store(true, Ordering::Release);
        assert!(reader.join().unwrap() > 0);
    });
    let last = std::fs::read(&path).unwrap();
    assert!(last.iter().all(|b| *b == last[0]) && (1..=8).contains(&last[0]));
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["manifest.bin".to_string()], "temporary files left: {names:?}");
}

#[test]
fn two_processes_writing_the_pair_together_never_make_a_wrong_hit() {
    // Writer A: manifest, key file. Writer B: manifest, key file. Every interleaving of the four writes.
    let dir = fresh_dir("interleave");
    let fonts = make_fonts(&dir, 4);
    let manifest = dir.join("manifest.bin");
    let a = blob(3000);
    let b = blob(3001);
    let stats = || stats_of(fonts.iter().cloned());
    // A's manifest, B's manifest, B's key file, A's key file (which is written from the manifest on disk then: B's).
    write_atomic(&manifest, &a).unwrap();
    write_atomic(&manifest, &b).unwrap();
    write_key_file(&manifest, &key(), stats()).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Hit);
    assert_eq!(std::fs::read(&manifest).unwrap(), b, "the pair that is a hit describes the manifest that is there");
    // A's key file computed from A's bytes, written after B's manifest: a pair that does not match.
    let stale = KeyFile {
        key: key(),
        manifest_len: a.len() as u64,
        manifest_hash: checksum(&a),
        fonts: stats(),
    };
    write_atomic(&key_file_path(&manifest), stale.to_text().as_bytes()).unwrap();
    assert_eq!(check(&manifest, &key()), Verdict::Rebuilt(Reason::ManifestChanged));
}

#[test]
fn the_descriptions_are_what_the_startup_timeline_prints() {
    assert_eq!(Verdict::Hit.describe(), "hit");
    assert_eq!(Verdict::Miss.describe(), "miss");
    assert_eq!(Verdict::NoDirectory.describe(), "miss (no cache directory)");
    assert_eq!(
        Verdict::Rebuilt(Reason::KeyFileMissing).describe(),
        "rebuilt (no key file)"
    );
    assert_eq!(
        Verdict::Rebuilt(Reason::FontsChanged {
            changed: 2,
            gone: 1,
            first: "/System/Library/Fonts/SFNS.ttf".to_string()
        })
        .describe(),
        "rebuilt (2 font files changed, 1 gone, first: /System/Library/Fonts/SFNS.ttf)"
    );
    assert!(!Verdict::Miss.is_hit() && !Verdict::Rebuilt(Reason::ManifestChanged).is_hit());
}

#[test]
fn the_linux_os_build_is_the_distribution_and_its_version() {
    let ubuntu = "NAME=\"Ubuntu\"\nVERSION=\"22.04.5 LTS (Jammy Jellyfish)\"\nID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"22.04\"\n";
    assert_eq!(linux_os_build(ubuntu), "ubuntu 22.04");
    let arch = "NAME=\"Arch Linux\"\nID=arch\nBUILD_ID=rolling\n";
    assert_eq!(linux_os_build(arch), "arch rolling");
    // `ID_LIKE` and `VERSION` are not `ID` and `VERSION_ID`.
    assert_eq!(linux_os_build("ID_LIKE=debian\nVERSION=\"1\"\n"), "");
    assert_eq!(linux_os_build(""), "");
    assert_eq!(linux_os_build("ID='fedora'\nVERSION_ID=41\n"), "fedora 41");
}

#[test]
fn this_machines_os_build_is_one_line() {
    let build = os_build();
    assert!(!build.contains('\n'), "{build:?}");
    assert_eq!(build, os_build(), "the same twice");
}

#[test]
fn the_key_of_this_process_is_made_of_this_build() {
    let key = Key::current(3, &[PathBuf::from("/fonts")]);
    assert_eq!(key.azul_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(key.manifest_version, 3);
    assert_eq!(key.scan_dirs, dirs_hash(&[PathBuf::from("/fonts")]));
    assert_eq!(key.os_build, os_build());
}

#[test]
fn the_checksum_that_ties_the_pair_is_pinned() {
    // The hash of the manifest is written into files that outlive a build: it must not change with a refactor.
    assert_eq!(checksum(b""), 0xcb91_86f6_7c53_1eb0);
    assert_eq!(checksum(b"abc"), 0xbbf5_acb9_4b2b_632d);
}

/// The modification time and the bytes of a file.
fn state_of(path: &Path) -> (SystemTime, Vec<u8>) {
    (
        std::fs::metadata(path).unwrap().modified().unwrap(),
        std::fs::read(path).unwrap(),
    )
}

/// Sets the modification time of `path` two days into the past (so that a rewrite is visible in it).
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
fn a_hit_that_finds_nothing_new_writes_nothing() {
    let dir = fresh_dir("hit-writes-nothing");
    let fonts = make_fonts(&dir, 20);
    let manifest = write_pair(&dir, &blob(6000), &key(), &fonts);
    let key_file = key_file_path(&manifest);
    age(&manifest);
    age(&key_file);
    let (manifest_before, key_file_before) = (state_of(&manifest), state_of(&key_file));

    // The start: a hit (the check reads, it writes nothing).
    assert_eq!(check(&manifest, &key()), Verdict::Hit);
    assert_eq!(state_of(&manifest), manifest_before, "the check touched the manifest");
    assert_eq!(state_of(&key_file), key_file_before, "the check touched the key file");

    // The end of the scan: as many faces as the hit loaded. The registry is not even asked.
    let persisted = persist(
        &manifest,
        &key(),
        Some(800),
        800,
        |_| panic!("a hit that found nothing new saved the manifest"),
        || panic!("a hit that found nothing new listed the fonts"),
    );
    assert_eq!(persisted, Persisted::Unchanged);
    assert_eq!(state_of(&manifest), manifest_before, "the manifest was rewritten or touched");
    assert_eq!(state_of(&key_file), key_file_before, "the key file was rewritten or touched");
    // And nothing else appeared next to them (no temporary file).
    let names: Vec<String> = std::fs::read_dir(manifest.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn a_hit_whose_scan_found_a_new_font_writes_the_pair_again() {
    let dir = fresh_dir("hit-new-font");
    let fonts = make_fonts(&dir, 6);
    let manifest = write_pair(&dir, &blob(2000), &key(), &fonts);
    age(&manifest);
    age(&key_file_path(&manifest));
    let before = state_of(&manifest);
    let persisted = persist(
        &manifest,
        &key(),
        Some(800),
        801,
        |path| {
            write_atomic(path, &blob(2100)).unwrap();
            true
        },
        || fonts.clone(),
    );
    assert_eq!(persisted, Persisted::Written);
    assert_ne!(state_of(&manifest), before);
    assert_eq!(check(&manifest, &key()), Verdict::Hit, "the new pair describes the new manifest");
}

#[test]
fn a_miss_writes_the_pair_and_the_next_start_is_a_hit() {
    let dir = fresh_dir("miss-then-hit");
    let fonts = make_fonts(&dir, 9);
    let manifest = dir.join("rfc").join("fonts").join("manifest.bin");
    assert_eq!(check(&manifest, &key()), Verdict::Miss);
    let persisted = persist(
        &manifest,
        &key(),
        None,
        9,
        |path| {
            write_atomic(path, &blob(3000)).unwrap();
            true
        },
        || fonts.clone(),
    );
    assert_eq!(persisted, Persisted::Written);
    assert_eq!(check(&manifest, &key()), Verdict::Hit);
}

#[test]
fn a_rebuilt_cache_replaces_the_stale_pair() {
    let dir = fresh_dir("rebuilt-replaced");
    let fonts = make_fonts(&dir, 9);
    let manifest = write_pair(&dir, &blob(3000), &key(), &fonts);
    std::fs::write(&fonts[2], b"an updated font").unwrap();
    assert!(matches!(
        check(&manifest, &key()),
        Verdict::Rebuilt(Reason::FontsChanged { changed: 1, .. })
    ));
    // The scan parsed the new file: the pair is written for what is on disk now.
    let persisted = persist(
        &manifest,
        &key(),
        None,
        9,
        |path| {
            write_atomic(path, &blob(3010)).unwrap();
            true
        },
        || fonts.clone(),
    );
    assert_eq!(persisted, Persisted::Written);
    assert_eq!(check(&manifest, &key()), Verdict::Hit);
}

#[test]
fn a_scan_that_found_no_font_keeps_nothing_and_a_failed_save_writes_no_key_file() {
    let dir = fresh_dir("nothing-failed");
    let manifest = dir.join("rfc").join("fonts").join("manifest.bin");
    let persisted = persist(
        &manifest,
        &key(),
        None,
        0,
        |_| panic!("an empty manifest was saved"),
        || panic!("fonts were listed"),
    );
    assert_eq!(persisted, Persisted::NothingFound);
    assert!(!manifest.exists() && !key_file_path(&manifest).exists());

    let persisted = persist(&manifest, &key(), None, 5, |_| false, || panic!("a failed save listed the fonts"));
    assert_eq!(persisted, Persisted::Failed);
    assert!(!key_file_path(&manifest).exists(), "a key file for a manifest that was not written");
}
