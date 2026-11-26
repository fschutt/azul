//! The store of font instances: a second start makes nothing, a damaged file is made again, the directory is
//! bounded, and nothing is ever deleted. Every test works in a directory of its own below the temp directory.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, SystemTime},
};

use super::*;

/// A directory that does not exist yet, named for the test (unique to this process and test).
fn fresh_dir(name: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "azul-fontinst-{}-{nanos}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ))
}

/// A font that passes the checks of the store: an sfnt header with one table that lies within the bytes.
fn fake_font(seed: u8, size: usize) -> Vec<u8> {
    assert!(size >= 28);
    let mut font = vec![0u8; size];
    font[0..4].copy_from_slice(&[0, 1, 0, 0]);
    font[4..6].copy_from_slice(&1u16.to_be_bytes());
    font[12..16].copy_from_slice(b"test");
    font[20..24].copy_from_slice(&28u32.to_be_bytes());
    font[24..28].copy_from_slice(&((size - 28) as u32).to_be_bytes());
    for (i, byte) in font[28..].iter_mut().enumerate() {
        *byte = seed.wrapping_add(i as u8).wrapping_mul(31);
    }
    font
}

fn key(font_hash: u64, cells: &[i32]) -> InstanceKey {
    InstanceKey {
        font_len: 8_330_740,
        font_hash,
        font_index: 0,
        cells: cells.to_vec(),
    }
}

/// The files of the store directory with the extension `ext`, sorted by name.
fn files(dir: &std::path::Path, ext: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|e| e == ext))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// The instancer of a test: it counts how often it runs and makes the same font for the same seed.
struct Instancer {
    calls: AtomicUsize,
}

impl Instancer {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    fn make(&self, seed: u8) -> Option<Vec<u8>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Some(fake_font(seed, 4096))
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[test]
fn an_instance_is_made_once_and_the_second_start_reads_it_back() {
    let dir = fresh_dir("second-start");
    let instancer = Instancer::new();
    let wanted = key(0xABCD, &[17 * 64]);

    // The first start: nothing is kept, the instancer runs, the instance is kept.
    let first = FontInstanceStore::new(&dir);
    let made = first.get_or_make(&wanted, || instancer.make(7)).expect("made");
    assert_eq!(instancer.calls(), 1);
    assert_eq!(made, fake_font(7, 4096));

    // The second start is another process: a store of its own over the same directory.
    let second = FontInstanceStore::new(&dir);
    let kept = second.get_or_make(&wanted, || instancer.make(7)).expect("kept");
    assert_eq!(instancer.calls(), 1, "the second start instanced nothing");
    assert_eq!(kept, made);
    assert_eq!(second.get_or_make(&wanted, || instancer.make(7)).unwrap(), made);
    assert_eq!(instancer.calls(), 1);
}

#[test]
fn another_font_or_other_coordinates_are_another_instance() {
    let dir = fresh_dir("keys");
    let store = FontInstanceStore::new(&dir);
    let instancer = Instancer::new();
    let a = store.get_or_make(&key(1, &[17 * 64]), || instancer.make(1)).unwrap();
    let b = store.get_or_make(&key(1, &[20 * 64]), || instancer.make(2)).unwrap();
    let c = store.get_or_make(&key(2, &[17 * 64]), || instancer.make(3)).unwrap();
    assert_eq!(instancer.calls(), 3);
    assert_ne!(a, b);
    assert_ne!(a, c);
    // All three are kept.
    assert_eq!(store.load(&key(1, &[17 * 64])).unwrap(), a);
    assert_eq!(store.load(&key(1, &[20 * 64])).unwrap(), b);
    assert_eq!(store.load(&key(2, &[17 * 64])).unwrap(), c);
    assert_eq!(instancer.calls(), 3);
}

/// Every way a file can be wrong: it is no entry, the instance is made again and the file is replaced.
#[test]
fn a_damaged_file_is_made_again_and_replaced() {
    let wanted = key(0xBEEF, &[17 * 64, 400 * 64]);
    let intact = {
        let dir = fresh_dir("damage-model");
        let store = FontInstanceStore::new(&dir);
        assert!(store.store(&wanted, &fake_font(9, 4096)));
        std::fs::read(&files(&dir, "bin")[0]).unwrap()
    };
    let cut_in_the_font = intact[..intact.len() - 100].to_vec();
    let mut flipped = intact.clone();
    let last = flipped.len() - 10;
    flipped[last] ^= 0x01;
    let mut wrong_magic = intact.clone();
    wrong_magic[0] = b'X';
    let mut other_version = intact.clone();
    other_version[4] ^= 0xFF;
    let mut wrong_length = intact.clone();
    wrong_length.extend_from_slice(b"trailing");
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("an empty file", Vec::new()),
        ("a file of garbage", vec![0xAB; 500]),
        ("a header only", intact[..40].to_vec()),
        ("a file cut inside the font", cut_in_the_font),
        ("one flipped bit in the font", flipped),
        ("a wrong magic", wrong_magic),
        ("another format version", other_version),
        ("bytes after the end", wrong_length),
    ];
    for (what, bad) in cases {
        let dir = fresh_dir("damage");
        let store = FontInstanceStore::new(&dir);
        let instancer = Instancer::new();
        assert!(store.store(&wanted, &fake_font(9, 4096)));
        let file = files(&dir, "bin")[0].clone();
        std::fs::write(&file, &bad).unwrap();
        assert_eq!(store.load(&wanted), None, "{what}: it is no entry");
        let made = store.get_or_make(&wanted, || instancer.make(9)).expect("made again");
        assert_eq!(instancer.calls(), 1, "{what}: the instancer ran");
        assert_eq!(made, fake_font(9, 4096));
        // The file was replaced: the next start reads it.
        assert_eq!(FontInstanceStore::new(&dir).load(&wanted), Some(made), "{what}: replaced");
        assert_eq!(files(&dir, "bin").len(), 1, "{what}: in the same slot");
    }
}

#[test]
fn a_font_that_is_cut_is_not_kept() {
    let dir = fresh_dir("cut");
    let store = FontInstanceStore::new(&dir);
    let font = fake_font(1, 4096);
    assert!(!store.store(&key(1, &[0]), &font[..2000]), "its table runs past its end");
    assert!(!store.store(&key(1, &[0]), &[]), "no font at all");
    assert!(!store.store(&key(1, &[0]), b"not a font, but long enough to look like one"));
    assert!(files(&dir, "bin").is_empty());
}

#[test]
fn an_instance_over_the_entry_limit_is_not_kept() {
    let dir = fresh_dir("limit");
    let store = FontInstanceStore::with_limits(&dir, 4, 2, 5000);
    assert!(store.store(&key(1, &[0]), &fake_font(1, 5000)), "at the limit");
    assert!(!store.store(&key(2, &[0]), &fake_font(1, 5001)), "one byte over");
    assert_eq!(files(&dir, "bin").len(), 1);
}

#[test]
fn the_directory_is_bounded_and_nothing_is_deleted() {
    let dir = fresh_dir("bounded");
    // One set of two ways: every key shares it.
    let store = FontInstanceStore::with_limits(&dir, 1, 2, 1 << 20);
    let mut seen_files: Vec<PathBuf> = Vec::new();
    for n in 0..8u64 {
        assert!(store.store(&key(n, &[0]), &fake_font(n as u8, 2048)));
        std::thread::sleep(Duration::from_millis(15)); // distinct modification times
        for file in files(&dir, "bin") {
            if !seen_files.contains(&file) {
                seen_files.push(file);
            }
        }
        assert!(files(&dir, "bin").len() <= 2, "after {n} instances");
        assert!(files(&dir, "tmp").is_empty(), "no temporary file is left behind");
    }
    // What was there at any time is still there: files are replaced, never removed.
    for file in &seen_files {
        assert!(file.exists(), "{} was removed", file.display());
    }
    assert_eq!(seen_files.len(), 2);
    // The two newest are kept, the older ones were replaced.
    assert!(store.load(&key(7, &[0])).is_some());
    assert!(store.load(&key(6, &[0])).is_some());
    for old in 0..6u64 {
        assert!(store.load(&key(old, &[0])).is_none(), "instance {old} was replaced");
    }
}

#[test]
fn the_file_replaced_is_the_least_recently_used_one() {
    let dir = fresh_dir("lru");
    let store = FontInstanceStore::with_limits(&dir, 1, 2, 1 << 20);
    assert!(store.store(&key(1, &[0]), &fake_font(1, 2048)));
    std::thread::sleep(Duration::from_millis(30));
    assert!(store.store(&key(2, &[0]), &fake_font(2, 2048)));
    std::thread::sleep(Duration::from_millis(30));
    // Using the older instance makes it the recent one.
    assert!(store.load(&key(1, &[0])).is_some());
    std::thread::sleep(Duration::from_millis(30));
    assert!(store.store(&key(3, &[0]), &fake_font(3, 2048)));
    assert!(store.load(&key(1, &[0])).is_some(), "the one just used stays");
    assert!(store.load(&key(3, &[0])).is_some());
    assert!(store.load(&key(2, &[0])).is_none(), "the one unused for longest was replaced");
}

#[test]
fn the_same_instance_again_replaces_its_own_file() {
    let dir = fresh_dir("same");
    let store = FontInstanceStore::with_limits(&dir, 1, 4, 1 << 20);
    let wanted = key(1, &[0]);
    for seed in 0..5u8 {
        assert!(store.store(&wanted, &fake_font(seed, 2048)));
    }
    assert_eq!(files(&dir, "bin").len(), 1, "one file, not one per store");
    assert_eq!(store.load(&wanted).unwrap(), fake_font(4, 2048), "the newest bytes");
}

#[test]
fn a_file_of_another_key_in_the_same_set_is_no_entry_for_this_one() {
    // One set: every key meets every file. The header names the key a file holds.
    let dir = fresh_dir("echo");
    let store = FontInstanceStore::with_limits(&dir, 1, 2, 1 << 20);
    assert!(store.store(&key(1, &[17 * 64]), &fake_font(1, 2048)));
    assert!(store.load(&key(2, &[17 * 64])).is_none());
    assert!(store.load(&key(1, &[18 * 64])).is_none());
    let mut other_index = key(1, &[17 * 64]);
    other_index.font_index = 1;
    assert!(store.load(&other_index).is_none());
    assert!(store.load(&key(1, &[17 * 64])).is_some());
}

#[test]
fn writers_at_the_same_moment_leave_one_intact_file() {
    let dir = fresh_dir("writers");
    let store = std::sync::Arc::new(FontInstanceStore::new(&dir));
    let wanted = key(0xFEED, &[17 * 64]);
    let font = fake_font(5, 64 * 1024);
    let start = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (store, start, wanted, font) = (store.clone(), start.clone(), wanted.clone(), font.clone());
            std::thread::spawn(move || {
                start.wait();
                // Some win the slot, the others find it taken and keep nothing: none may break the file.
                store.store(&wanted, &font)
            })
        })
        .collect();
    let wrote: Vec<bool> = handles.into_iter().map(|h| h.join().expect("no panic")).collect();
    assert!(wrote.iter().any(|w| *w), "at least one writer wrote");
    assert_eq!(store.load(&wanted).as_deref(), Some(&font[..]), "the file is intact");
    assert!(files(&dir, "tmp").is_empty(), "no temporary file is left behind");
    assert_eq!(files(&dir, "bin").len(), 1);
}

#[test]
fn readers_and_a_writer_at_the_same_moment_never_see_half_a_file() {
    let dir = fresh_dir("readers");
    let store = std::sync::Arc::new(FontInstanceStore::new(&dir));
    let wanted = key(0xAAAA, &[17 * 64]);
    let font = fake_font(6, 256 * 1024);
    assert!(store.store(&wanted, &font));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let (store, done, wanted, font) = (store.clone(), done.clone(), wanted.clone(), font.clone());
            std::thread::spawn(move || {
                let mut reads = 0usize;
                while !done.load(Ordering::SeqCst) {
                    // Old file or new file, both the same font: never absent, never cut.
                    assert_eq!(store.load(&wanted).as_deref(), Some(&font[..]));
                    reads += 1;
                }
                reads
            })
        })
        .collect();
    for _ in 0..40 {
        assert!(store.store(&wanted, &font));
    }
    done.store(true, Ordering::SeqCst);
    for reader in readers {
        assert!(reader.join().expect("no panic") > 0);
    }
}

#[test]
fn a_temporary_file_of_a_writer_at_work_is_left_alone_and_an_old_one_is_taken_over() {
    let dir = fresh_dir("temporary");
    let store = FontInstanceStore::with_limits(&dir, 1, 1, 1 << 20);
    let wanted = key(3, &[0]);
    std::fs::create_dir_all(&dir).unwrap();
    let temporary = dir.join("fi-00-0.tmp");
    std::fs::write(&temporary, b"half a file of a writer at work").unwrap();
    assert!(!store.store(&wanted, &fake_font(3, 2048)), "a writer is at work: nothing is kept");
    assert!(store.load(&wanted).is_none());

    // The writer died minutes ago: the file is taken over.
    let long_ago = SystemTime::now() - Duration::from_secs(600);
    OpenOptions::new().write(true).open(&temporary).unwrap().set_modified(long_ago).unwrap();
    assert!(store.store(&wanted, &fake_font(3, 2048)));
    assert_eq!(store.load(&wanted).unwrap(), fake_font(3, 2048));
}

#[test]
fn a_store_that_cannot_write_keeps_nothing_and_says_so() {
    // The directory is a file: no directory can be made there.
    let blocker = fresh_dir("blocked");
    std::fs::write(&blocker, b"a file where the directory should be").unwrap();
    let store = FontInstanceStore::new(blocker.join("instances"));
    assert!(!store.store(&key(1, &[0]), &fake_font(1, 2048)));
    assert!(store.load(&key(1, &[0])).is_none());
    // A caller that makes the instance anyway gets it.
    let instancer = Instancer::new();
    assert!(store.get_or_make(&key(1, &[0]), || instancer.make(1)).is_some());
    assert_eq!(instancer.calls(), 1);
}

#[test]
fn an_instancer_that_cannot_make_the_instance_makes_no_entry() {
    let dir = fresh_dir("cannot");
    let store = FontInstanceStore::new(&dir);
    assert_eq!(store.get_or_make(&key(1, &[0]), || None), None);
    assert!(files(&dir, "bin").is_empty());
}

#[test]
fn the_checksum_is_stable_and_notices_any_change() {
    // Pinned: the files of an earlier run are read by the next one, so the function must not drift (a change of
    // it is a new `FORMAT_VERSION`).
    assert_eq!(checksum(b""), 0xcb91_86f6_7c53_1eb0);
    assert_eq!(checksum(b"abc"), 0xbbf5_acb9_4b2b_632d);
    let ascending: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    assert_eq!(checksum(&ascending), 0xf882_c5a2_ab7d_362d);
    assert_ne!(checksum(b""), checksum(b"\0"));
    let font = fake_font(1, 100_000);
    let base = checksum(&font);
    assert_eq!(base, checksum(&font.clone()), "deterministic");
    for position in [0usize, 1, 7, 31, 32, 33, 4096, 99_999] {
        let mut changed = font.clone();
        changed[position] ^= 0x80;
        assert_ne!(checksum(&changed), base, "a bit flipped at {position}");
    }
    let mut longer = font.clone();
    longer.push(0);
    assert_ne!(checksum(&longer), base, "one byte more");
    assert_eq!(checksum(&font[..31]), checksum(&font[..31]), "the tail is hashed too");
    assert_ne!(checksum(&font[..31]), checksum(&font[..30]));
}

#[test]
fn the_set_of_a_key_is_stable_across_runs() {
    let a = key(0x1234_5678_9ABC_DEF0, &[17 * 64, 400 * 64]);
    let store = FontInstanceStore::with_limits("unused", 16, 2, 1);
    assert_eq!(store.set_of(&a), store.set_of(&a.clone()));
    let spread: std::collections::HashSet<usize> =
        (0..200u64).map(|n| store.set_of(&key(n, &[17 * 64]))).collect();
    assert!(spread.len() > 8, "200 instances land in {} of 16 sets", spread.len());
}
