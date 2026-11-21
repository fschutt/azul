//! What this computer paid for: one [`PaidRecord`] per paid checkout, in a file of its own
//! (`paid/paid.json` beside the keyring's locks, readable by this user only) - the customer's
//! receipt on a durable medium (ENT27-B06). The token server books the sale and answers it with
//! the checkout's approval (`receipt`); before this file nothing read that field, and the buyer
//! was left with a toast.
//!
//! The record is made when the poll opens the drive's (or the months') sign-up and has put the
//! drive (the months) where the app takes it from - the delivery comes first, the record second:
//! a record that cannot be kept never holds a paid drive back (it would sit on the list, claim
//! secret and all, until the token server purges the sealed sign-up after 30 days), the app is
//! told and says so ([`crate::pending::Claimed::record`]). A list that does not read - cut, no
//! JSON - is set aside as `paid.json.bad` (its bytes stay) and a new one begun; a list of
//! another format is a newer build's and is left as it is. The receipt AzDrive renders is made
//! from the record - the amount, the VAT rate, the day and the service the token server booked
//! - never from today's tier list. The legal wording of a receipt is not decided here.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use azlin_contract::{Cents, ReceiptWire};
use serde::{Deserialize, Serialize};

use crate::{
    error::{CloudError, CloudResult, Context},
    lock::LockDir,
    state::{create_private_dir, write_atomic, write_json},
};

/// The `format` of the file.
const PAID_FORMAT: &str = "azcloud.paid";
/// How long a record waits for another window's change of the file.
const PAID_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
/// The most records the file keeps (the oldest go first): a paid list is a drive's lifetime of
/// months, not a ledger.
const MAX_RECORDS: usize = 2000;

/// One paid checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaidRecord {
    pub checkout: String,
    /// The drive the months (or the drive itself) were paid for.
    pub drive: String,
    /// The tier bought (`100GB`; empty when the answer named none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tier: String,
    /// The months it pays for (0: not said).
    #[serde(default)]
    pub months: u32,
    /// When this computer saw it paid, in seconds since 1970 (this computer's clock).
    pub at: i64,
    /// The sale the token server booked, as it answered it; `None` from a token server that
    /// books none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptWire>,
}

impl PaidRecord {
    /// What was charged: the booked sale's gross amount and currency; `None` without a receipt
    /// (or for an amount below zero).
    #[must_use]
    pub fn charge(&self) -> Option<Cents> {
        let receipt = self.receipt.as_ref()?;
        Some(Cents {
            amount: u64::try_from(receipt.gross_cents).ok()?,
            currency: receipt.currency.clone(),
        })
    }
}

/// The file's text.
#[derive(Serialize, Deserialize)]
struct PaidFile {
    format: String,
    #[serde(default)]
    records: Vec<PaidRecord>,
}

/// How many lists that did not read are kept beside the file.
const MAX_ASIDE: u32 = 9;

/// What the file holds.
enum Loaded {
    /// The records of a list of this build's format (none: no file yet).
    Records(Vec<PaidRecord>),
    /// JSON of a list of another format: a newer build's, left as it is.
    OtherFormat(String),
    /// Not a paid list at all (cut off, torn, another document): why, and its bytes.
    Damaged(String, Vec<u8>),
}

/// The paid list in one folder.
#[derive(Debug, Clone)]
pub struct PaidList {
    dir: PathBuf,
}

impl PaidList {
    /// The list in `dir` (made with the first record).
    #[must_use]
    pub fn in_dir(dir: &Path) -> PaidList {
        PaidList {
            dir: dir.to_path_buf(),
        }
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.dir.join("paid.json")
    }

    /// What the file holds: its records, nothing (no file), or why it is no list of this build's.
    fn load(&self) -> CloudResult<Loaded> {
        let path = self.path();
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Loaded::Records(Vec::new())),
            Err(e) => return Err(e).with_context(|| path.display().to_string()),
        };
        Ok(match serde_json::from_slice::<PaidFile>(&bytes) {
            Ok(file) if file.format == PAID_FORMAT => Loaded::Records(file.records),
            Ok(file) => Loaded::OtherFormat(format!(
                "{} is no list of paid checkouts (format {:?})",
                path.display(),
                file.format
            )),
            Err(e) => Loaded::Damaged(format!("{} cannot be read: {e}", path.display()), bytes),
        })
    }

    fn read_file(&self) -> CloudResult<Vec<PaidRecord>> {
        match self.load()? {
            Loaded::Records(records) => Ok(records),
            Loaded::OtherFormat(why) | Loaded::Damaged(why, _) => Err(CloudError::failed(why)),
        }
    }

    /// Every record, oldest first.
    ///
    /// # Errors
    ///
    /// The file cannot be read or is no paid list: it is never read as an empty one.
    pub fn records(&self) -> CloudResult<Vec<PaidRecord>> {
        self.read_file()
    }

    /// The file that does not read, `bytes`, kept under a name beside it: `paid.json.bad`, else
    /// `paid.json.bad.2`, ... - a list set aside earlier is never replaced.
    fn set_aside(&self, bytes: &[u8]) -> CloudResult<PathBuf> {
        for n in 1..=MAX_ASIDE {
            let mut name = self.path().into_os_string();
            name.push(if n == 1 {
                String::from(".bad")
            } else {
                format!(".bad.{n}")
            });
            let aside = PathBuf::from(name);
            if aside.exists() {
                continue;
            }
            write_atomic(&aside, bytes, true).with_context(|| aside.display().to_string())?;
            return Ok(aside);
        }
        Err(CloudError::failed(format!(
            "{} does not read, and {MAX_ASIDE} lists that did not are set aside beside it \
             already: remove them",
            self.path().display()
        )))
    }

    /// The record of the checkout `checkout`, if this computer has one.
    ///
    /// # Errors
    ///
    /// As [`PaidList::records`].
    pub fn of_checkout(&self, checkout: &str) -> CloudResult<Option<PaidRecord>> {
        Ok(self
            .read_file()?
            .into_iter()
            .find(|r| r.checkout == checkout))
    }

    /// Keeps `record`: a record of the same checkout is updated (its first `at` stays), else it
    /// is appended; changed under a lock of its own, so two windows lose none of each other's.
    /// A list that is no JSON (cut, torn) is set aside - its bytes stay as `paid.json.bad` - and
    /// a new one begun with the record: one damaged file never keeps the next receipt from
    /// being kept.
    ///
    /// # Errors
    ///
    /// The file cannot be read, set aside or written - the record is not kept then - or it is a
    /// list of another format (a newer build's), which is never written over.
    pub fn keep(&self, record: &PaidRecord) -> CloudResult<()> {
        create_private_dir(&self.dir).with_context(|| self.dir.display().to_string())?;
        let _lock = LockDir::new(self.dir.join("locks")).lock("paid", PAID_WAIT)?;
        let mut records = match self.load()? {
            Loaded::Records(records) => records,
            Loaded::Damaged(_, bytes) => {
                self.set_aside(&bytes)?;
                Vec::new()
            }
            Loaded::OtherFormat(why) => return Err(CloudError::failed(why)),
        };
        match records.iter_mut().find(|r| r.checkout == record.checkout) {
            Some(kept) => {
                let first = kept.at.min(record.at);
                // The answer that names a receipt is the better one.
                if record.receipt.is_some() || kept.receipt.is_none() {
                    *kept = record.clone();
                }
                kept.at = first;
            }
            None => records.push(record.clone()),
        }
        if records.len() > MAX_RECORDS {
            let surplus = records.len() - MAX_RECORDS;
            records.drain(..surplus);
        }
        let file = PaidFile {
            format: PAID_FORMAT.to_string(),
            records,
        };
        write_json(&self.path(), &file, true)
    }
}
