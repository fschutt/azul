//! What this computer paid for: one [`PaidRecord`] per paid checkout, in a file of its own
//! (`paid/paid.json` beside the keyring's locks, readable by this user only) - the customer's
//! receipt on a durable medium (ENT27-B06). The token server books the sale and answers it with
//! the checkout's approval (`receipt`); before this file nothing read that field, and the buyer
//! was left with a toast.
//!
//! The record is made when the poll opens the drive's (or the months') sign-up, BEFORE the
//! checkout leaves the keyring's list: a record that cannot be kept holds the claim back for the
//! next poll, so a paid checkout never ends without it. The receipt AzDrive renders is made
//! from the record - the amount, the VAT rate, the day and the service the token server booked
//! - never from today's tier list. The legal wording of a receipt is not decided here.

use std::path::{Path, PathBuf};

use azlin_contract::{Cents, ReceiptWire};
use serde::{Deserialize, Serialize};

use crate::{
    error::{CloudError, CloudResult, Context},
    lock::LockDir,
    state::{create_private_dir, read_json, write_json},
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

    fn read_file(&self) -> CloudResult<Vec<PaidRecord>> {
        let path = self.path();
        match read_json::<PaidFile>(&path)? {
            None => Ok(Vec::new()),
            Some(file) if file.format == PAID_FORMAT => Ok(file.records),
            Some(file) => Err(CloudError::failed(format!(
                "{} is no list of paid checkouts (format {:?})",
                path.display(),
                file.format
            ))),
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
    ///
    /// # Errors
    ///
    /// The file cannot be read or written - the record is not kept then.
    pub fn keep(&self, record: &PaidRecord) -> CloudResult<()> {
        create_private_dir(&self.dir).with_context(|| self.dir.display().to_string())?;
        let _lock = LockDir::new(self.dir.join("locks")).lock("paid", PAID_WAIT)?;
        let mut records = self.read_file()?;
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
