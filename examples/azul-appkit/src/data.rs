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
/// reading and writing go through the drive). A key that is no path of plain names - a `..`, an
/// absolute path, a backslash, a Windows drive or device name - is no place under the root: the
/// answer is the root itself ([`azul_storage::names::OnDisk`], AZ-CN17). The key is one to LOOK AT
/// ([`azul_storage::names::OnDisk`] makes nothing): the path is for a message, and it may name a
/// file that is not there - nothing is opened or made by it (ARCH29 C6c; X-DELTA-STRUCT-DRIVE-2: the
/// type claimed an entry this disk read, and this caller says a drive's key).
#[must_use]
pub fn local_path(root: &Path, key: &str) -> PathBuf {
    match azul_storage::names::OnDisk::of_entry(key.trim_end_matches('/')) {
        Ok(key) => key.under(root),
        Err(_) => root.to_path_buf(),
    }
}

// The record-file ids (`contacts/<id>.vcf`): the storage crate's mint
// (`azul_storage::ids`, the one seed source of the repo). The names stay
// here for the apps that call them through the kit.
pub use azul_storage::ids::{is_uuid, new_uuid, uuid_from_words};

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
    fn az_cn17_a_key_that_is_no_path_of_plain_names_is_no_place_under_the_root() {
        let root = Path::new("/data");
        assert_eq!(
            local_path(root, "contacts/"),
            PathBuf::from("/data/contacts")
        );
        assert_eq!(local_path(root, ""), PathBuf::from("/data"));
        for bad in [
            "../../etc/passwd",
            "a/../../b",
            "/etc/passwd",
            "a\\b",
            "C:/Windows",
            "a//b",
            "a/./b",
            "x\0y",
        ] {
            assert_eq!(local_path(root, bad), PathBuf::from("/data"), "{bad:?}");
        }
    }

    /// ARCH29 C6c. The path of a key for a message is an `OnDisk` key's: a name to look at, said as it is
    /// (a protected spelling included), and saying it opens and makes nothing (a name to be MADE is an
    /// `Admitted` key, which no message needs).
    #[test]
    fn c6c_the_path_for_a_message_names_a_file_that_is_there_as_it_is_and_makes_nothing() {
        let root = Path::new("/c6c-appkit-data-root-that-is-not-there");
        for key in [".GIT/hooks/x", "GIT~1/config", "proj/.git/config"] {
            assert_eq!(local_path(root, key), root.join(key), "{key:?}");
        }
        assert!(!root.exists(), "saying a path made the root");
    }

    #[test]
    fn the_kits_uuid_is_the_storage_crates_mint() {
        let id = new_uuid();
        assert!(is_uuid(&id), "{id}");
        assert_ne!(id, new_uuid());
        assert_eq!(
            uuid_from_words(0, 0),
            azul_storage::ids::uuid_from_words(0, 0)
        );
    }
}
