//! Where this device's identity seed lives (CRYPTO.md section 3): the OS keyring under
//! [`KEYRING_KEY`] (azul's `CallbackInfo::keyring_get` / `keyring_store`, the way AzMail keeps its
//! passwords; the answer arrives as the window's `KeyringResult` event), or - with
//! `--identity-file <path>` (`AZMEET_IDENTITY_FILE`), for tests and unattended machines - a file
//! made with mode 0600, or nowhere: a seed for this run only, when the keyring cannot keep one.
//! The file side is plain `std::fs`; no azul types.

use std::path::Path;

use crate::crypto::Identity;

/// The keyring entry of AzMeet's seed (every azul app shares one keyring service, so the name
/// says the app).
pub const KEYRING_KEY: &str = "AzMeet/identity";

/// Where the seed of this run came from (the settings and stdout say it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The OS keyring.
    Keyring,
    /// An identity file (`--identity-file`).
    File(String),
    /// Nowhere: this run only (why: the keyring's answer).
    Session(String),
}

impl Source {
    /// One word for stdout: `keyring`, `file`, `session`.
    #[must_use]
    pub fn word(&self) -> &'static str {
        match self {
            Source::Keyring => "keyring",
            Source::File(_) => "file",
            Source::Session(_) => "session",
        }
    }

    /// What the settings say about it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Source::Keyring => String::from("kept in the system keyring"),
            Source::File(path) => format!("kept in {path} (--identity-file)"),
            Source::Session(why) => format!("for this run only: the system keyring {why}"),
        }
    }
}

/// The identity in the file at `path`, made (with a new seed) when there is none. The file is
/// written with mode 0600 through a temporary file and a rename.
pub fn load_or_create(path: &Path) -> Result<(Identity, bool), String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Identity::from_secret_json(&text)
            .map(|identity| (identity, false))
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let identity = Identity::generate().map_err(|e| e.to_string())?;
            write_private(path, identity.to_secret_json().as_bytes())
                .map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((identity, true))
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Writes `bytes` to `path` readable by its owner only, creating its folder.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identity_file_is_made_once_and_read_back_as_the_same_device() {
        let dir = std::env::temp_dir().join(format!("azmeet-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("identity.json");
        let (made, created) = load_or_create(&path).unwrap();
        assert!(created);
        let (again, created) = load_or_create(&path).unwrap();
        assert!(!created);
        assert_eq!(again.device(), made.device());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "readable by its owner only");
        }
        std::fs::write(&path, "not an identity").unwrap();
        assert!(load_or_create(&path).unwrap_err().contains("identity.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_source_says_where_the_seed_lives() {
        assert_eq!(Source::Keyring.word(), "keyring");
        assert_eq!(Source::File(String::from("/tmp/x")).word(), "file");
        assert!(Source::Session(String::from("is unavailable"))
            .describe()
            .contains("this run only"));
    }
}
