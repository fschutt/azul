//! An unpaid Azlin drive's last months in AzDrive: the stage the bar says (grace, read-only,
//! locked, restore pending) with its exact days, what a locked drive still owes, the copy of a
//! read-only drive to this computer (files already there skipped, what is missing named, too
//! little room said before anything is copied), and the honest words in English and German.
//! No window.

use azcloud_kit::{
    azlin_contract::{ArchiveState, ArchiveWire, Rfc3339},
    DriveStatus,
};
use azul_storage::{
    cap::Cap, key::System, testing::TempDir, ByteRange, Drive, DriveError, ListPage, ListRequest,
    LocalDrive, ObjectInfo,
};

use azul_appkit::phrase::{Arg, Phrase};

use crate::archive::{
    months_needed, run_rescue, stage_of, stage_phrase, target_of, target_of_on, Rescue, Stage,
};

/// A disk whose free space the test's system cannot tell.
const NO_PROBE: Cap<u64> = Cap::Unknown("no probe");

/// The argument `name` of `phrase` as text.
fn arg_text(phrase: &Phrase, name: &str) -> Option<String> {
    match phrase.get(name)? {
        Arg::Str(text) => Some(text.clone()),
        Arg::Int(n) => Some(n.to_string()),
        Arg::Word { fallback, .. } => Some(fallback.clone()),
    }
}

const DAY: u64 = 86_400;
/// 2026-07-01T00:00:00Z: the drive's last paid day.
const PAID: u64 = 1_782_864_000;

fn unpaid(read_only: bool) -> DriveStatus {
    DriveStatus {
        period_until: Some(PAID),
        read_only,
        ..DriveStatus::default()
    }
}

/// Locked 104 days after its last paid day, to be paid through three months after it.
fn locked(state: ArchiveState, proven: bool) -> ArchiveWire {
    ArchiveWire {
        state,
        locked_at: Rfc3339::from_unix(i64::try_from(PAID + 104 * DAY).unwrap()),
        pay_through: Rfc3339::from_unix(i64::try_from(PAID + 90 * DAY).unwrap()),
        proven,
    }
}

#[test]
fn an_unpaid_drive_is_in_grace_then_read_only_then_locked_with_its_days() {
    assert_eq!(stage_of(&unpaid(false), PAID - DAY), None, "paid");
    let grace = stage_of(&unpaid(false), PAID + DAY).expect("unpaid");
    assert_eq!(
        grace,
        Stage::Grace {
            period_until: PAID,
            read_only_at: PAID + 14 * DAY,
            locks_at: PAID + 104 * DAY,
        }
    );
    let read_only = stage_of(&unpaid(true), PAID + 20 * DAY).expect("read-only");
    assert_eq!(
        read_only,
        Stage::ReadOnly {
            period_until: PAID,
            locks_at: PAID + 104 * DAY,
        }
    );
    let status = DriveStatus {
        archive: Some(locked(ArchiveState::Locked, false)),
        ..unpaid(true)
    };
    let stage = stage_of(&status, PAID + 200 * DAY).expect("locked");
    assert_eq!(stage.name(), "locked");
    assert!(status.locked());
    // a banned drive has its own bar
    let banned = DriveStatus {
        ban: Some(azcloud_kit::Ban::default()),
        ..unpaid(true)
    };
    assert_eq!(stage_of(&banned, PAID + 20 * DAY), None);
}

#[test]
fn a_locked_drive_owes_three_months_of_thirty_days() {
    let wire = locked(ArchiveState::Locked, true);
    assert_eq!(months_needed(&wire, Some(PAID)), 3);
    assert_eq!(months_needed(&wire, Some(PAID + 60 * DAY)), 1, "one month left");
    assert_eq!(months_needed(&wire, Some(PAID + 90 * DAY)), 0, "paid through");
}

#[test]
fn the_bar_and_the_locked_page_say_what_happens_when_and_how_to_get_it_back() {
    let grace = stage_of(&unpaid(false), PAID + DAY).unwrap();
    let phrase = stage_phrase("Photos", &grace);
    assert_eq!(phrase.key, "azdrive-archive-grace");
    assert_eq!(arg_text(&phrase, "paid").as_deref(), Some("2026-07-01"));
    assert_eq!(arg_text(&phrase, "read_only").as_deref(), Some("2026-07-15"));
    assert_eq!(arg_text(&phrase, "locked").as_deref(), Some("2026-10-13"));
    let read_only = stage_of(&unpaid(true), PAID + 20 * DAY).unwrap();
    assert_eq!(stage_phrase("Photos", &read_only).key, "azdrive-archive-read-only");
    let lock = |state, proven| Stage::Locked {
        wire: locked(state, proven),
        period_until: Some(PAID),
    };
    let page = stage_phrase("Photos", &lock(ArchiveState::Locked, false));
    assert_eq!(page.key, "azdrive-archive-locked");
    assert_eq!(arg_text(&page, "months").as_deref(), Some("3"));
    assert_eq!(arg_text(&page, "paid").as_deref(), Some("2026-07-01"));
    assert_eq!(
        stage_phrase("Photos", &lock(ArchiveState::Locked, true)).key,
        "azdrive-archive-locked-proven"
    );
    assert_eq!(
        stage_phrase("Photos", &lock(ArchiveState::RestorePending, true)).key,
        "azdrive-archive-restore-pending"
    );
}

#[test]
fn the_words_tell_the_truth_in_english_and_german() {
    let en = include_str!("../resources/en.ftl");
    let de = include_str!("../resources/de.ftl");
    let line = |text: &str, key: &str| -> String {
        let start = text.find(&format!("\n{key} = ")).expect(key) + 1;
        text[start..].split("\nazdrive-").next().unwrap_or("").to_string()
    };
    // Each text of the lock says where the files go - one offline copy - and promises no time
    // of keeping it: no "nothing is deleted", no years.
    for key in [
        "azdrive-renew-closing",
        "azdrive-archive-grace",
        "azdrive-archive-read-only",
        "azdrive-archive-locked",
    ] {
        let english = line(en, key);
        assert!(english.contains("locked") && english.contains("offline copy"), "{english}");
        let german = line(de, key);
        assert!(german.contains("gesperrt") && german.contains("Offline-Kopie"), "{german}");
    }
    for (file, ftl) in [("en.ftl", en), ("de.ftl", de)] {
        for text in ftl.split("\nazdrive-").filter(|m| {
            m.starts_with("archive-") || m.starts_with("renew-") || m.starts_with("unpaid-")
        }) {
            for never in ["deleted", "gelöscht", "year", "Jahr"] {
                assert!(!text.contains(never), "{file}: {never} in {text}");
            }
        }
    }
    let locked_en = line(en, "azdrive-archive-locked");
    assert!(locked_en.contains("some days") && !locked_en.contains("104"), "{locked_en}");
    let locked_de = line(de, "azdrive-archive-locked");
    assert!(locked_de.contains("einige Tage") && !locked_de.contains("104"), "{locked_de}");
}

/// The promise to an unpaid drive's owner is the contract's own sentence, with the contract's
/// days: no number is typed into AzDrive's message, in English or German.
#[test]
fn the_unpaid_promise_is_the_contracts_sentence_with_its_days() {
    use azcloud_kit::azlin_contract::{unpaid_promise, UNPAID_DAYS};
    use azul_appkit::l10n::{set_locale, t_phrase};
    crate::l10n::in_english();
    let promise = crate::archive::promise_phrase();
    assert_eq!(t_phrase(&promise), unpaid_promise());
    set_locale("de-DE");
    let versprechen = t_phrase(&promise);
    for days in [UNPAID_DAYS.grace, UNPAID_DAYS.read_only, UNPAID_DAYS.lock] {
        assert!(versprechen.contains(&days.to_string()), "{days}: {versprechen}");
    }
    set_locale("en-US");
    for ftl in [include_str!("../resources/en.ftl"), include_str!("../resources/de.ftl")] {
        let start = ftl.find("\nazdrive-unpaid-promise = ").expect("the promise") + 1;
        let message = ftl[start..].lines().next().unwrap_or("");
        assert!(!message.chars().any(|c| c.is_ascii_digit()), "{message}");
    }
}

#[test]
fn a_key_that_cannot_be_a_path_is_never_written_outside_the_copy() {
    let folder = std::path::Path::new("/home/u/Azlin rescue/Photos");
    assert_eq!(
        target_of(folder, "2026/beach.jpg"),
        Some(folder.join("2026").join("beach.jpg"))
    );
    assert_eq!(target_of(folder, "../../etc/passwd"), None);
    assert_eq!(target_of(folder, "a//b"), None);
    assert_eq!(target_of(folder, "/abs"), None);
    assert_eq!(target_of(folder, "a\\..\\b"), None);
}

/// AZ-W2: the rescue copy builds its paths on every computer from keys other devices made. A key
/// a Windows disk reads as something else - a path relative to another drive (`D:Docs/x`, which
/// `PathBuf::push` takes as the new base), a device (`nul.txt`), a stream (`a.txt:s`), a name
/// whose trailing dot is dropped - is no path of the copy, whatever computer checks it.
#[test]
fn az_w2_a_key_windows_reads_as_something_else_is_no_path_of_the_copy() {
    let folder = std::path::Path::new("/home/u/Azlin rescue/Photos");
    for key in [
        "D:Docs/x",
        "D:/Docs/x",
        "docs/D:x",
        "nul.txt",
        "NUL",
        "con.log",
        "COM1",
        "dir/lpt9.dat",
        "a.txt:s",
        "dir/b.txt:stream",
        "name.",
        "dir/name ",
        "q?.txt",
        "a|b",
        "bell\u{7}",
    ] {
        assert_eq!(target_of_on(folder, key, System::Windows), None, "{key:?}");
    }
    // The names a Mac or a Linux computer takes as they are (a Windows device's files copied
    // there keep them), and the ordinary ones on Windows too.
    for key in ["D:Docs/x", "nul.txt", "a.txt:s", "name.", "dir/name "] {
        assert!(
            target_of_on(folder, key, System::Linux).is_some(),
            "{key:?} on Linux"
        );
        assert!(
            target_of_on(folder, key, System::Macos).is_some(),
            "{key:?} on macOS"
        );
    }
    assert_eq!(
        target_of_on(folder, "2026/beach.jpg", System::Windows),
        Some(folder.join("2026").join("beach.jpg"))
    );
    // Absolute and climbing keys are refused everywhere.
    for system in System::ALL {
        for key in ["../../etc/passwd", "a//b", "/abs", "C:/x", "a\\b", ""] {
            assert_eq!(
                target_of_on(folder, key, system),
                None,
                "{key:?} on {system:?}"
            );
        }
    }
}

/// Some objects of a drive, in memory: keys that one folder on APFS or NTFS could not hold.
struct Keys(std::collections::BTreeMap<String, Vec<u8>>);

impl Keys {
    fn new(files: &[(&str, &[u8])]) -> Keys {
        Keys(
            files
                .iter()
                .map(|(k, b)| (k.to_string(), b.to_vec()))
                .collect(),
        )
    }

    fn not_found(key: &str) -> DriveError {
        DriveError::NotFound {
            key: key.to_string(),
        }
    }
}

impl Drive for Keys {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        Ok(ListPage {
            folders: Vec::new(),
            objects: self
                .0
                .iter()
                .filter(|(key, _)| key.starts_with(&request.prefix))
                .map(|(key, bytes)| ObjectInfo {
                    key: key.clone(),
                    size: bytes.len() as u64,
                    modified: None,
                    etag: None,
                })
                .collect(),
            next: None,
        })
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.0.get(key).cloned().ok_or_else(|| Keys::not_found(key))
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.get(key)?;
        let start = usize::try_from(range.start)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = range
            .end
            .map_or(bytes.len(), |e| {
                usize::try_from(e).unwrap_or(usize::MAX).saturating_add(1)
            })
            .min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }
    fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
        Err(DriveError::Denied {
            message: String::from("a test source is read-only"),
        })
    }
    fn delete(&self, _key: &str) -> Result<(), DriveError> {
        Err(DriveError::Denied {
            message: String::from("a test source is read-only"),
        })
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let bytes = self.0.get(key).ok_or_else(|| Keys::not_found(key))?;
        Ok(ObjectInfo {
            key: key.to_string(),
            size: bytes.len() as u64,
            modified: None,
            etag: None,
        })
    }
}

/// Every file under `dir` as (its path below `dir`, its bytes).
fn files_below(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((folder, rel)) = stack.pop() {
        for entry in std::fs::read_dir(&folder).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                stack.push((entry.path(), format!("{rel}{name}/")));
            } else {
                out.push((format!("{rel}{name}"), std::fs::read(entry.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// AZ-W2: `Report.txt` and `report.txt` are two files of the drive and one on APFS or NTFS (so
/// are a name in two Unicode forms): the copy keeps both, and a copy made on a disk that tells
/// them apart does not lose one when it is moved to a disk that does not.
#[test]
fn az_w2_keys_that_differ_only_by_case_or_form_are_both_kept_in_the_copy() {
    use azcloud_kit::sync::names::{fold, Volume};
    let tmp = TempDir::new("azdrive-rescue-twins");
    let drive = Keys::new(&[
        ("Report.txt", b"upper"),
        ("report.txt", b"lower"),
        ("caf\u{e9}.txt", b"composed"),
        ("cafe\u{301}.txt", b"decomposed"),
    ]);
    let folder = tmp.path().join("copy");
    let state = tmp.path().join("d_x.rescue.json");
    let report = run_rescue(&drive, &folder, NO_PROBE, &state, &mut |_, _| {});
    assert_eq!((report.copied, report.total), (4, 4), "{report:?}");
    assert!(report.missing.is_empty(), "{report:?}");
    let found = files_below(&folder);
    let folds = Volume {
        case_insensitive: true,
        norm_insensitive: true,
        ..Volume::default()
    };
    let distinct: std::collections::BTreeSet<String> =
        found.iter().map(|(name, _)| fold(name, &folds)).collect();
    assert_eq!(
        distinct.len(),
        found.len(),
        "no two files of the copy are one file on a disk that ignores case and form: {:?}",
        found.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    let mut contents: Vec<&[u8]> = found.iter().map(|(_, bytes)| bytes.as_slice()).collect();
    contents.sort_unstable();
    assert_eq!(
        contents,
        [&b"composed"[..], b"decomposed", b"lower", b"upper"],
        "every file is in the copy"
    );
    // The next run finds all four there.
    let again = run_rescue(&drive, &folder, NO_PROBE, &state, &mut |_, _| {});
    assert_eq!((again.copied, again.total), (4, 4), "{again:?}");
    assert_eq!(files_below(&folder), found, "a second run changes nothing");
}

fn source_with(tmp: &TempDir, files: &[(&str, &[u8])]) -> LocalDrive {
    let drive = LocalDrive::without_manifest(tmp.path().join("drive"));
    for (key, bytes) in files {
        drive.put(key, bytes).unwrap();
    }
    drive
}

#[test]
fn a_read_only_drive_is_copied_whole_and_a_second_run_skips_what_is_there() {
    let tmp = TempDir::new("azdrive-rescue");
    let drive = source_with(&tmp, &[("a.txt", b"one"), ("deep/b.txt", b"two two"), ("e", b"")]);
    let folder = tmp.path().join("Azlin rescue").join("Photos");
    let state = tmp.path().join("state").join("d_x.rescue.json");
    let mut heard = Vec::new();
    let report = run_rescue(&drive, &folder, NO_PROBE, &state, &mut |copied, total| {
        heard.push((copied, total));
    });
    assert_eq!((report.copied, report.total), (3, 3), "{report:?}");
    assert!(report.missing.is_empty() && report.no_space.is_none());
    assert_eq!(std::fs::read(folder.join("deep").join("b.txt")).unwrap(), b"two two");
    assert!(!heard.is_empty(), "the progress is told");
    // kept for the next start
    let kept: Rescue = serde_json::from_str(&std::fs::read_to_string(&state).unwrap()).unwrap();
    assert_eq!(kept.copied, 3);
    // a file changed on this computer (another size) is copied again, the others are skipped
    std::fs::write(folder.join("a.txt"), b"x").unwrap();
    let again = run_rescue(&drive, &folder, NO_PROBE, &state, &mut |_, _| {});
    assert_eq!((again.copied, again.total), (3, 3));
    assert_eq!(std::fs::read(folder.join("a.txt")).unwrap(), b"one");
}

#[test]
fn too_little_room_is_said_before_anything_is_copied_and_names_what_is_missing() {
    let tmp = TempDir::new("azdrive-rescue-full");
    let drive = source_with(&tmp, &[("a.txt", b"one"), ("b.txt", b"two")]);
    let folder = tmp.path().join("copy");
    let state = tmp.path().join("d_x.rescue.json");
    let report = run_rescue(&drive, &folder, Cap::Known(10), &state, &mut |_, _| {});
    assert_eq!(report.no_space.map(|(needed, _)| needed), Some(6));
    assert_eq!(report.copied, 0);
    assert_eq!(report.missing, vec![String::from("a.txt"), String::from("b.txt")]);
    assert!(!folder.join("a.txt").exists(), "nothing half copied");
}

/// X-AR5a-12 (the rescue copy's site): a disk that cannot say its free space is not read as an
/// empty one nor as an endless one by accident - the posture is written down
/// ([`crate::archive`]'s `ROOM_POSTURE`: the copy begins), and a disk that says it has too little
/// still stops the copy before it starts.
#[test]
fn x_ar5a_12_an_unreadable_free_space_lets_the_rescue_begin_and_a_small_one_stops_it() {
    let tmp = TempDir::new("azdrive-rescue-unknown-room");
    let drive = source_with(&tmp, &[("a.txt", b"one"), ("b.txt", b"two")]);
    let state = tmp.path().join("d_x.rescue.json");
    let unknown = run_rescue(
        &drive,
        &tmp.path().join("unknown"),
        Cap::Unknown("the system did not say"),
        &state,
        &mut |_, _| {},
    );
    assert_eq!((unknown.copied, unknown.total), (2, 2), "{unknown:?}");
    assert!(unknown.no_space.is_none() && unknown.missing.is_empty());
    let small = run_rescue(
        &drive,
        &tmp.path().join("small"),
        Cap::Known(0),
        &state,
        &mut |_, _| {},
    );
    assert_eq!(small.copied, 0, "{small:?}");
    assert!(small.no_space.is_some());
}
