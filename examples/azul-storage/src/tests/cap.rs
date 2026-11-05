//! What a probe of this computer could not answer (ARCH28 RF49, X12): the free space of a disk is
//! a [`Cap`], never a number that stands for "unknown" - the caller matches it and applies the
//! posture written next to its check.

use super::TempDir;
use crate::{
    beneath::free_space,
    cap::{Cap, Posture},
};

#[test]
fn x_ar5a_12_an_unknown_probe_is_never_a_number() {
    let unknown: Cap<u64> = Cap::Unknown("no answer");
    assert!(!unknown.is_known());
    assert_eq!(unknown.map(|n| n + 1), Cap::Unknown("no answer"));
    assert!(Cap::Known(1u64).is_known());
    assert_eq!(Cap::Known(2u64).map(|n| n * 2), Cap::Known(4));
    // A posture is a value to match, with the sentence for the report inside.
    let degrade = Posture::Degrade("not checked");
    assert_ne!(degrade, Posture::FailClosed);
}

#[cfg(unix)]
#[test]
fn x_ar5a_12_the_free_space_of_a_disk_is_known_and_of_a_path_that_is_not_there_is_unknown() {
    let tmp = TempDir::new("cap-free-space");
    assert!(free_space(tmp.path()).is_known());
    let gone = tmp.path().join("not-there");
    // The probe used to answer `None` here and `unwrap_or(u64::MAX)` read it as an endless disk.
    assert!(
        matches!(free_space(&gone), Cap::Unknown(_)),
        "{:?}",
        free_space(&gone)
    );
}

/// A drive whose HEAD says the object is bigger than any disk (a lying listing, or a huge file).
#[cfg(unix)]
struct Huge(crate::LocalDrive);

#[cfg(unix)]
impl crate::Drive for Huge {
    fn list(&self, request: &crate::ListRequest) -> Result<crate::ListPage, crate::DriveError> {
        self.0.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, crate::DriveError> {
        self.0.get(key)
    }
    fn get_range(&self, key: &str, range: crate::ByteRange) -> Result<Vec<u8>, crate::DriveError> {
        self.0.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), crate::DriveError> {
        self.0.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), crate::DriveError> {
        self.0.delete(key)
    }
    fn head(&self, key: &str) -> Result<crate::ObjectInfo, crate::DriveError> {
        let mut info = self.0.head(key)?;
        info.size = 1 << 60;
        Ok(info)
    }
}

/// The UI download path checked nothing: an object that says it is bigger than the disk made a
/// hidden file of that length and a list of every range of it. Now the disk's room is looked at
/// first, and the download is refused with a sentence before a file is made.
#[cfg(unix)]
#[test]
fn x_ar5a_12_a_download_bigger_than_the_disk_is_refused_before_any_file_is_made() {
    use crate::{transfer::download_to_file, Drive, DriveError, LocalDrive};
    let tmp = TempDir::new("cap-download-room");
    let source = Huge(LocalDrive::without_manifest(tmp.path().join("drive")));
    source.put("film.bin", b"small in truth").unwrap();
    let dest = tmp.path().join("out").join("film.bin");
    let got = download_to_file(&source, "film.bin", None, &dest, 1 << 20);
    match got {
        Err(DriveError::Io(why)) => assert!(why.contains("no space left"), "{why}"),
        other => panic!("expected a refusal for lack of room, got {other:?}"),
    }
    assert!(!dest.exists());
    let left: Vec<_> = std::fs::read_dir(tmp.path().join("out"))
        .map(|d| d.map(|e| e.unwrap().file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "no hidden file was made: {left:?}");
}
