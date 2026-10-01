//! The per-user data layout (build ledger F7).
//!
//! Durable data are FILES, laid out as the user's S3 bucket will be: the
//! data root holds one folder per app, `calculator/history.jsonl`,
//! `contacts/<uuid>.vcf`, `notes/<notebook>/<note-uuid>.md`. Today the root
//! is a folder on this computer (an azul-storage `LocalDrive`); later an
//! `S3Drive` over the user's bucket replaces it with no other change, so
//! every path here is a drive KEY: `/`-separated, relative, no `..`.
//!
//! The root: `--data-dir`, else `$AZLIN_DATA`, else `Azlin` in the user's
//! data folder (`~/Library/Application Support/Azlin`, `%APPDATA%\Azlin`,
//! `~/.local/share/Azlin`).

use std::path::{Path, PathBuf};

/// The variable naming the data root for every Azlin app.
pub const DATA_VAR: &str = "AZLIN_DATA";

/// The folder in the user's data folder when nothing else names the root.
pub const ROOT_DIR: &str = "Azlin";

/// The data root: the `--data-dir` switch, else the variable's value (blank
/// counts as unset), else [`ROOT_DIR`] in the OS data folder, else
/// [`ROOT_DIR`] in the working folder (a system without a data folder).
#[must_use]
pub fn data_root(flag: Option<&Path>, var: Option<&str>, os_data_dir: Option<PathBuf>) -> PathBuf {
    if let Some(flag) = flag.filter(|p| !p.as_os_str().is_empty()) {
        return flag.to_path_buf();
    }
    if let Some(var) = var.map(str::trim).filter(|v| !v.is_empty()) {
        return PathBuf::from(var);
    }
    match os_data_dir.filter(|p| !p.as_os_str().is_empty()) {
        Some(dir) => dir.join(ROOT_DIR),
        None => PathBuf::from(ROOT_DIR),
    }
}

/// The drive key of `name` in an app's folder: `calculator` + `history.jsonl`
/// = `calculator/history.jsonl`. Slashes at the joint are not doubled.
#[must_use]
pub fn app_key(app_folder: &str, name: &str) -> String {
    let folder = app_folder.trim_matches('/');
    let name = name.trim_start_matches('/');
    if folder.is_empty() {
        name.to_string()
    } else if name.is_empty() {
        format!("{folder}/")
    } else {
        format!("{folder}/{name}")
    }
}

/// The folder an app's keys live in, as a listing prefix: `contacts/`.
#[must_use]
pub fn app_prefix(app_folder: &str) -> String {
    app_key(app_folder, "")
}

/// The file on disk of a key under a local root (for messages to the user;
/// reading and writing go through the drive).
#[must_use]
pub fn local_path(root: &Path, key: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for segment in key.split('/').filter(|s| !s.is_empty()) {
        path.push(segment);
    }
    path
}

/// A new random identifier for a record file (`contacts/<id>.vcf`): a version
/// 4 UUID in its usual 8-4-4-4-12 lowercase form. Random enough that two
/// devices never pick the same one; drawn from the system's hasher keys and
/// the clock, so no new crate is needed.
#[must_use]
pub fn new_uuid() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut words = [0u64; 2];
    for (i, word) in words.iter_mut().enumerate() {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(nanos);
        h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        h.write_usize(i);
        h.write_u32(std::process::id());
        *word = h.finish();
    }
    uuid_from_words(words[0], words[1])
}

/// The UUID text of 128 random bits, with the version (4) and variant (10xx) bits set.
#[must_use]
pub fn uuid_from_words(hi: u64, lo: u64) -> String {
    let hi = (hi & 0xffff_ffff_ffff_0fff) | 0x0000_0000_0000_4000;
    let lo = (lo & 0x3fff_ffff_ffff_ffff) | 0x8000_0000_0000_0000;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        hi >> 32,
        (hi >> 16) & 0xffff,
        hi & 0xffff,
        lo >> 48,
        lo & 0xffff_ffff_ffff
    )
}

/// Whether `text` is a UUID in the 8-4-4-4-12 hex form (any case).
#[must_use]
pub fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_wins_over_the_variable_and_the_variable_over_the_os_folder() {
        let os = Some(PathBuf::from("/Users/a/Library/Application Support"));
        assert_eq!(
            data_root(Some(Path::new("/tmp/x")), Some("/srv/y"), os.clone()),
            PathBuf::from("/tmp/x")
        );
        assert_eq!(
            data_root(None, Some(" /srv/y "), os.clone()),
            PathBuf::from("/srv/y")
        );
        assert_eq!(
            data_root(None, Some("   "), os.clone()),
            PathBuf::from("/Users/a/Library/Application Support/Azlin"),
            "a blank variable counts as unset"
        );
        assert_eq!(data_root(None, None, None), PathBuf::from("Azlin"));
    }

    #[test]
    fn an_apps_keys_are_its_folder_and_the_name_joined_by_one_slash() {
        assert_eq!(
            app_key("calculator", "history.jsonl"),
            "calculator/history.jsonl"
        );
        assert_eq!(app_key("contacts/", "/a.vcf"), "contacts/a.vcf");
        assert_eq!(app_prefix("contacts"), "contacts/");
        assert_eq!(app_key("", "settings.json"), "settings.json");
    }

    #[test]
    fn a_key_maps_to_the_file_under_the_local_root() {
        assert_eq!(
            local_path(Path::new("/data"), "contacts/abc.vcf"),
            PathBuf::from("/data/contacts/abc.vcf")
        );
    }

    #[test]
    fn a_new_uuid_is_version_4_and_never_repeats() {
        let a = new_uuid();
        let b = new_uuid();
        assert!(is_uuid(&a), "{a}");
        assert_ne!(a, b);
        assert_eq!(a.as_bytes()[14], b'4', "the version digit: {a}");
        assert!(
            matches!(a.as_bytes()[19], b'8' | b'9' | b'a' | b'b'),
            "the variant: {a}"
        );
        assert_eq!(a, a.to_lowercase());
    }

    #[test]
    fn the_uuid_text_of_fixed_bits_is_stable() {
        assert_eq!(
            uuid_from_words(0, 0),
            "00000000-0000-4000-8000-000000000000"
        );
        assert_eq!(
            uuid_from_words(u64::MAX, u64::MAX),
            "ffffffff-ffff-4fff-bfff-ffffffffffff"
        );
        assert!(!is_uuid("not-a-uuid"));
        assert!(!is_uuid("00000000-0000-4000-8000-00000000000"));
    }
}
