//! What the phone keeps between two runs, and nothing else: its own key (so that the Mac still knows it) and the Mac it is paired with
//! (the Mac's key, its name and the addresses it was last reached at). Three things about it, said once:
//!
//! - it is a seam ([`PhoneStore`]): the app gives it a directory it owns ([`FileStore`]), a test or the simulator a [`MemoryStore`];
//! - reading is total: a missing file is an empty store, a damaged one an error text and an empty store, nothing in it can make
//!   the phone crash or take more than [`MAX_STORE_BYTES`];
//! - writing is atomic (a temporary file, then a rename) and private to the user where the system has such a thing (mode 600).
//!
//! The text of the file is for a person to read when something goes wrong: `azreview-companion 1`, then `key`, `mac`, `name` and
//! up to four `ticket` lines. Lines it does not know are ignored, so a newer companion's file is still read by an older one.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::discovery::MAX_SAVED_TICKETS;
use crate::limits::{cut_bytes, MAX_NAME_BYTES, MAX_TICKET_BYTES};
use crate::pairing::{is_valid_ticket, Entropy};

/// The first line of the file.
pub const STORE_HEADER: &str = "azreview-companion 1";
/// The most the file may hold; a larger one is not read.
pub const MAX_STORE_BYTES: u64 = 16 * 1024;
/// The longest key of a Mac the file takes (the transport's keys are 64 characters).
const MAX_MAC_ID_CHARS: usize = 128;

/// The Mac a phone is paired with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedMac {
    /// The Mac's key as the transport authenticated it when the phone was paired (lowercase hex).
    pub mac_id: String,
    pub name: String,
    /// How to dial it, the newest first (at most [`MAX_SAVED_TICKETS`]).
    pub tickets: Vec<String>,
}

/// Everything kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stored {
    /// The phone's own key (32 bytes), made at the first start: the Mac knows the phone by it.
    pub secret_key: Option<[u8; 32]>,
    pub mac: Option<SavedMac>,
}

/// Where the phone keeps it.
pub trait PhoneStore {
    /// What was kept. A store that was never written is empty, which is no error.
    ///
    /// # Errors
    /// The store exists and cannot be read (or is damaged): the text says why, and the phone starts empty.
    fn load(&mut self) -> Result<Stored, String>;

    /// Keeps `stored`, replacing what was there.
    ///
    /// # Errors
    /// It could not be written: the text says why.
    fn save(&mut self, stored: &Stored) -> Result<(), String>;
}

/// A store in memory: for the simulator and the tests. Clones share what is kept, so a test can look at what the client wrote.
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    cell: Arc<Mutex<Stored>>,
    saves: Arc<Mutex<u32>>,
}

impl MemoryStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> MemoryStore {
        MemoryStore::default()
    }

    /// A store that holds `stored` already.
    #[must_use]
    pub fn with(stored: Stored) -> MemoryStore {
        MemoryStore { cell: Arc::new(Mutex::new(stored)), saves: Arc::new(Mutex::new(0)) }
    }

    /// What is kept now.
    #[must_use]
    pub fn peek(&self) -> Stored {
        self.cell.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// How often `save` was called.
    #[must_use]
    pub fn saves(&self) -> u32 {
        *self.saves.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl PhoneStore for MemoryStore {
    fn load(&mut self) -> Result<Stored, String> {
        Ok(self.peek())
    }

    fn save(&mut self, stored: &Stored) -> Result<(), String> {
        *self.cell.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = stored.clone();
        *self.saves.lock().unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
        Ok(())
    }
}

/// A store in one file the app owns.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    /// A store in `path` (the folders above it are made when something is saved).
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> FileStore {
        FileStore { path: path.into() }
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl PhoneStore for FileStore {
    fn load(&mut self) -> Result<Stored, String> {
        use std::io::Read;
        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Stored::default()),
            Err(e) => return Err(format!("{} cannot be opened: {e}", self.path.display())),
        };
        let mut bytes = Vec::new();
        file.take(MAX_STORE_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("{} cannot be read: {e}", self.path.display()))?;
        if bytes.len() as u64 > MAX_STORE_BYTES {
            return Err(format!("{} is larger than {MAX_STORE_BYTES} bytes: not read", self.path.display()));
        }
        let text = String::from_utf8(bytes).map_err(|_| format!("{} is not text: not read", self.path.display()))?;
        parse(&text).map_err(|why| format!("{}: {why}", self.path.display()))
    }

    fn save(&mut self, stored: &Stored) -> Result<(), String> {
        use std::io::Write;
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| format!("{} cannot be made: {e}", dir.display()))?;
        }
        let mut temporary = self.path.clone().into_os_string();
        temporary.push(".new");
        let temporary = PathBuf::from(temporary);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // the file holds the phone's key: the user only
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(|e| format!("{} cannot be written: {e}", temporary.display()))?;
        file.write_all(render(stored).as_bytes()).map_err(|e| format!("{} cannot be written: {e}", temporary.display()))?;
        file.sync_all().map_err(|e| format!("{} cannot be written: {e}", temporary.display()))?;
        drop(file);
        std::fs::rename(&temporary, &self.path).map_err(|e| format!("{} cannot be replaced: {e}", self.path.display()))
    }
}

/// The phone's key: the one the store has kept, or a new one (made, kept, and returned) at the first start. The text says what went
/// wrong with the store, if anything did (the phone then goes on with a key that is not kept: it will have to be paired again next
/// time, which the person is told by the pairing screen, not by a crash).
pub fn load_identity(store: &mut dyn PhoneStore, entropy: &mut dyn Entropy) -> ([u8; 32], Vec<String>) {
    let mut said = Vec::new();
    let mut stored = match store.load() {
        Ok(stored) => stored,
        Err(why) => {
            said.push(format!("what the phone kept could not be read: {why}"));
            Stored::default()
        }
    };
    if let Some(key) = stored.secret_key {
        return (key, said);
    }
    let mut key = [0u8; 32];
    entropy.fill(&mut key);
    stored.secret_key = Some(key);
    if let Err(why) = store.save(&stored) {
        said.push(format!("the phone's key could not be kept: {why}"));
    }
    (key, said)
}

/// The text of a store.
#[must_use]
pub fn render(stored: &Stored) -> String {
    let mut out = format!("{STORE_HEADER}\n");
    if let Some(key) = &stored.secret_key {
        out.push_str(&format!("key {}\n", hex(key)));
    }
    if let Some(mac) = &stored.mac {
        out.push_str(&format!("mac {}\n", one_line(&mac.mac_id)));
        out.push_str(&format!("name {}\n", one_line(&mac.name)));
        for ticket in mac.tickets.iter().filter(|t| is_valid_ticket(t)).take(MAX_SAVED_TICKETS) {
            out.push_str(&format!("ticket {ticket}\n"));
        }
    }
    out
}

/// A store from its text.
///
/// # Errors
/// It is not a companion's file, or it is of a version this build does not know.
pub fn parse(text: &str) -> Result<Stored, String> {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim_end();
    if first != STORE_HEADER {
        return Err(if first.starts_with("azreview-companion ") { "it is of another version of the companion".to_string() } else { "it is not a companion's file".to_string() });
    }
    let (mut key, mut mac_id, mut name, mut tickets) = (None, None, String::new(), Vec::new());
    for line in lines {
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        match word {
            "key" => key = from_hex_32(rest.trim()),
            "mac" => {
                let id = rest.trim();
                mac_id = (!id.is_empty() && id.len() <= MAX_MAC_ID_CHARS && id.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))).then(|| id.to_string());
            }
            "name" => name = cut_bytes(&one_line(rest), MAX_NAME_BYTES, "name").0,
            "ticket" => {
                let ticket = rest.trim();
                if is_valid_ticket(ticket) && ticket.len() <= MAX_TICKET_BYTES && tickets.len() < MAX_SAVED_TICKETS && !tickets.iter().any(|t| t == ticket) {
                    tickets.push(ticket.to_string());
                }
            }
            _ => {}
        }
    }
    Ok(Stored { secret_key: key, mac: mac_id.map(|mac_id| SavedMac { mac_id, name, tickets }) })
}

/// A text on one line, without control characters.
fn one_line(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex_32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
#[path = "store_test.rs"]
mod store_test;
