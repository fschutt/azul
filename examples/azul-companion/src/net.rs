//! The phone's network: where the app keeps its pairing, and the client over azul's iroh endpoint that the screens talk to.
//!
//! WHAT IS KEPT, and nothing else: the phone's own key (so that the Mac still knows it) and the Mac it is paired with (its key, its name,
//! the addresses it was reached at), in one small text file in the app's own data folder ([`azul_review_link::store`]). The folder is
//! `$AZ_COMPANION_DATA` when that is set (a test, a script), else `AzCompanion` in the system's data folder, else `.azcompanion` in the
//! home folder. A phone that has none of them keeps nothing between two runs: the app says so, and it has to be paired again at the next
//! start. (Whether the system hands an Android app a data folder this way has not been tried on a phone.)
//!
//! WHAT IT NEVER DOES: bind to a relay (the companion is for a desk, not the internet), keep a connection while the app is suspended
//! (the system does not allow it), or make a connection to anything but the Mac the phone was paired with or the offer it was given.

use std::path::{Path, PathBuf};

use azul::file::FilePath;
use azul_review_link::client::{ClientConfig, PhoneClient};
use azul_review_link::iroh_link::{IrohLink, IrohOptions};
use azul_review_link::link::Link;
use azul_review_link::pairing::OsEntropy;
use azul_review_link::phone::PhoneApi;
use azul_review_link::store::{load_identity, FileStore, MemoryStore, PhoneStore};

use crate::offline::OfflinePhone;

/// The variable that names the data folder (a test, a script).
pub const DATA_VAR: &str = "AZ_COMPANION_DATA";
/// The folder in the system's data folder.
pub const APP_DIR: &str = "AzCompanion";
/// The folder in the home folder when the system has no data folder.
pub const HOME_DIR: &str = ".azcompanion";
/// The file the pairing is kept in.
pub const STORE_FILE: &str = "companion.txt";

/// What the app starts with: the connection the screens talk to, and what a person debugging it may want to read.
pub struct Net {
    pub api: Box<dyn PhoneApi>,
    /// Lines for the log: where the pairing is kept, or why it cannot be, why the network did not start.
    pub notes: Vec<String>,
}

/// The folder the pairing is kept in: the variable, else the system's data folder, else the home folder; `None` when there is none.
#[must_use]
pub fn data_dir_from(var: Option<&str>, os_data: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(var) = var.map(str::trim).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(var));
    }
    if let Some(dir) = os_data.filter(|d| !d.as_os_str().is_empty()) {
        return Some(dir.join(APP_DIR));
    }
    home.filter(|d| !d.as_os_str().is_empty()).map(|dir| dir.join(HOME_DIR))
}

/// The folder for this machine.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    let os_data = FilePath::get_data_dir().into_option().map(|dir| PathBuf::from(dir.inner.as_str()));
    let home = FilePath::get_home_dir().into_option().map(|dir| PathBuf::from(dir.inner.as_str()));
    data_dir_from(std::env::var(DATA_VAR).ok().as_deref(), os_data, home)
}

/// What the Mac's screen calls this phone: the kind of phone and the first characters of its key, so that two phones differ.
#[must_use]
pub fn phone_name(key: &str) -> String {
    let kind = if cfg!(target_os = "ios") {
        "iPhone"
    } else if cfg!(target_os = "android") {
        "Android phone"
    } else {
        "Phone"
    };
    let short: String = key.chars().take(4).collect();
    if short.is_empty() {
        kind.to_string()
    } else {
        format!("{kind} {short}")
    }
}

/// The store in `dir` (or one in memory when there is no folder), and what to say about it.
fn store_in(dir: Option<&Path>, notes: &mut Vec<String>) -> Box<dyn PhoneStore> {
    match dir {
        Some(dir) => {
            let store = FileStore::new(dir.join(STORE_FILE));
            notes.push(format!("the pairing is kept in {}", store.path().display()));
            Box::new(store)
        }
        None => {
            notes.push("the system gave no folder to keep the pairing in: the phone has to be paired again at every start".to_string());
            Box::new(MemoryStore::new())
        }
    }
}

/// The connection of the phone's app: the client over an iroh endpoint with the key and the Mac that were kept in `dir`. When the endpoint
/// cannot be bound (a library without the engine, no socket) the app still runs, on a connection that says it finds no Mac.
#[must_use]
pub fn connect_in(dir: Option<&Path>, app_version: &str) -> Net {
    let mut notes = Vec::new();
    let mut store = store_in(dir, &mut notes);
    let (key, said) = load_identity(store.as_mut(), &mut OsEntropy);
    notes.extend(said);
    match IrohLink::bind(&IrohOptions { secret_key: Some(key), ..IrohOptions::default() }) {
        Ok(link) => {
            let id = link.local_id();
            let config = ClientConfig { phone_name: phone_name(&id), app_version: app_version.to_string() };
            // the phone announces nothing (an iPhone only looks); it learns from the local network when its Mac appears
            let discovery = link.discovery();
            notes.push(format!("the local network: {}", link.discovery_status()));
            Net { api: Box::new(PhoneClient::new(link, config, store).with_discovery(Box::new(discovery))), notes }
        }
        Err(why) => {
            notes.push(format!("the network did not start: {why}"));
            Net { api: Box::new(OfflinePhone::new()), notes }
        }
    }
}

/// The connection of the phone's app, with the folder and the version of this build.
#[must_use]
pub fn connect() -> Net {
    connect_in(data_dir().as_deref(), env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
#[path = "net_test.rs"]
mod net_test;
