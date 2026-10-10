//! One bridge per user: `serve` holds a lock in the state folder for as long as it runs
//! (azcloud-kit's [`LockDir`]: the OS's own file lock, dropped with its process, so a crash
//! leaves nothing to clear) and writes who it is and on which ports into `running.json`; a
//! second `serve` of the same state folder - a login item and a hand-started one, two terminals
//! - is refused and names the process that serves.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use azcloud_kit::{lock::HeldLock, LockDir};
use serde::{Deserialize, Serialize};

/// The folder of the bridge's locks in its state folder.
pub const LOCKS_DIR: &str = "locks";
/// The lock `serve` holds.
pub const LOCK_NAME: &str = "azul-bridge/serve";
/// Who serves, in the state folder.
pub const RUNNING_FILE: &str = "running.json";

/// The bridge that serves: its process and ports, since when (seconds since 1970).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Running {
    pub pid: u32,
    pub imap: u16,
    pub smtp: u16,
    pub dav: u16,
    pub pim: u16,
    pub started: u64,
}

impl Running {
    /// The sentence a second `serve` says.
    #[must_use]
    pub fn sentence(&self) -> String {
        format!(
            "azul-bridge already serves this user: process {} (IMAP {}, SMTP {}, WebDAV {}, \
             CalDAV / CardDAV {}); stop it first, or use another --state-dir",
            self.pid, self.imap, self.smtp, self.dav, self.pim
        )
    }
}

/// The lock of a `serve`: released when dropped (or when the process ends).
#[derive(Debug)]
pub struct Instance {
    _lock: HeldLock,
    file: PathBuf,
}

impl Instance {
    /// Writes who serves on which ports, for a second `serve` and `status`.
    pub fn announce(&self, running: &Running) {
        let _ = azcloud_kit::state::write_json(&self.file, running, false);
    }
}

/// Takes the bridge's lock of the state folder `state`.
///
/// # Errors
///
/// Another `serve` holds it: the sentence naming it (its process and ports when it said them).
pub fn claim(state: &Path) -> Result<Instance, String> {
    match LockDir::new(state.join(LOCKS_DIR)).lock(LOCK_NAME, Duration::ZERO) {
        Ok(lock) => Ok(Instance {
            _lock: lock,
            file: state.join(RUNNING_FILE),
        }),
        Err(_) => Err(match read_running(state) {
            Some(running) => running.sentence(),
            None => String::from(
                "azul-bridge already serves this user (another process holds its lock); stop it \
                 first, or use another --state-dir",
            ),
        }),
    }
}

fn read_running(state: &Path) -> Option<Running> {
    azcloud_kit::state::read_json(&state.join(RUNNING_FILE)).ok().flatten()
}

/// The bridge that serves the state folder `state` now, if one does (and said who it is).
#[must_use]
pub fn running(state: &Path) -> Option<Running> {
    match claim(state) {
        Ok(_free) => None,
        Err(_) => read_running(state),
    }
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    /// A second `serve` of the same state folder is refused and names the process that serves
    /// and its ports; once that one ends, the lock is free (the OS drops it with its process, so
    /// a crash leaves nothing to clear).
    #[test]
    fn a_second_serve_is_refused_and_names_the_one_running() {
        let dir = TempDir::new("bridge-single");
        let first = claim(&dir.0).expect("the first serve");
        first.announce(&Running {
            pid: 4242,
            imap: 1143,
            smtp: 1025,
            dav: 1180,
            pim: 1181,
            started: 1_790_843_400,
        });
        let second = claim(&dir.0).err().expect("the second is refused");
        assert!(second.contains("4242") && second.contains("1143"), "{second}");
        assert_eq!(running(&dir.0).map(|r| r.pid), Some(4242));
        drop(first);
        assert_eq!(running(&dir.0), None, "nobody serves");
        assert!(claim(&dir.0).is_ok(), "free once the first ends");
    }
}
