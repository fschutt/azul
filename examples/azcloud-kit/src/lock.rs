//! Locks every process of this user shares - the storage crate's ([`azul_storage::lock`]): a
//! folder (an app's is beside its drives file, `<config dir>/azul-storage/locks`) with one file
//! per lock, held with the OS's own file lock, which the OS drops when its process ends. What
//! they guard: an Azlin drive's session while its drive token is spent and the next one stored
//! (two windows, two apps never spend one token twice: the token server would revoke the
//! device), and the list of unfinished checkouts while it is changed. A failed lock is a
//! [`CloudError`](crate::error::CloudError) through `?`.

pub use azul_storage::lock::{HeldLock, LockDir, LockError};
