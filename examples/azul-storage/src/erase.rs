//! A reason at every verb that destroys (ARCH29 section 9.4 A1; ARCH30-1, step (1): additive).
//!
//! `Drive::delete`, `delete_many` and `delete_folder` take no reason: 127 call sites remove an object
//! on the strength of a look the caller made - a listing, a read that failed, a write whose answer was
//! lost - and only four of them ask the one gate (`meta::reclaim::ask_retired_delete`). The defects of
//! the sync rounds are almost all deleters of that kind. The rule that already holds where it was used
//! (`Permit`, `Absent`, `DeleteLicence`, `Landed`, `Admitted`) is: **a dangerous verb takes, as a
//! parameter, a witness that only the owner of the invariant can make, and a collapsed read, a listing
//! or a name cannot make one.** [`Drive::delete_for`](crate::Drive::delete_for),
//! [`Drive::delete_many_for`](crate::Drive::delete_many_for) and
//! [`Drive::delete_folder_for`](crate::Drive::delete_folder_for) take a [`Why`]: the reason to
//! delete, made of one of five witnesses.
//!
//! | reason | the witness (made by) | bound to |
//! |---|---|---|
//! | `Why::Retired` | the licence of the one gate (`meta::reclaim::DeleteLicence`, from `ask_retired_delete`) and the proof that the history no longer names the object (`meta::reclaim::Reclaimable`, the reclaimer's analysis) | the object's bucket key, from the proof |
//! | `Why::Confirmed` | the user's confirmed purge (`meta::reclaim::PurgeConfirmation`, from the answer a re-encryption pass recorded) and the same proof | the object's bucket key, from the proof |
//! | [`Why::Superseded`] | the receipt of a publish ([`crate::durable::Landed`], made by `durable`) | the name the new version landed at |
//! | `Why::Refused` | the proof that the change which was to name a new object was refused for good or never made (`meta::reclaim::Refusal`, made from the error and the object by `meta::reclaim` and nowhere else) | the new object it names |
//! | [`Why::UserChose`] | the job a user started, which names what it is about ([`Chose`], `Chose::job`: the one witness the apps make) | the keys or the folder the job names |
//!
//! (`Retired`, `Confirmed` and `Refused` belong to the encrypted drive's history and exist with the
//! cargo feature `encryption`.)
//!
//! Every witness is bound to what it is for, by its owner: [`Why::covers`] answers for ONE key (and
//! [`Why::covers_folder`] for a folder that is removed with everything under it), so a witness that was
//! made for the object A is refused for the object B at the first call that runs it, whatever the site
//! had lying around. The check is [`Why::check`], the first line of every `*_for` method.
//!
//! # The migration, and where this stands
//!
//! 1. **This step: additive.** The three methods are PROVIDED beside `delete`, `delete_many` and
//!    `delete_folder`, which stay; no caller changes. A `*_for` method checks the reason against the key
//!    the caller names and then calls the drive's own `delete` (`delete_many`, `delete_folder`): a
//!    wrapper keeps every piece of its own delete path - the retry, the cache that forgets the object,
//!    the renewed credentials, the stop flag, the grant and the key rewrite - and the witness is spent
//!    at the first drive it meets, against the key the CALLER names. That is why the three are `compose`
//!    entries of `drive_methods!` (never forwarded to the drive below: it would skip the wrapper's own
//!    delete; never `send`: a witness is spent by the call and cannot be sent twice).
//! 2. One crate per commit (ARCH30-4): the 127 sites move to the `*_for` methods, `#[deprecated]` on the
//!    old three and `#![deny(deprecated)]` in the crate being migrated, so the compiler prints the list.
//! 3. The last commit removes `delete`, `delete_many`, `delete_folder` and `retired_delete_gate`, makes
//!    the `*_for` methods the required ones and hands the witness down through every wrapper.
//!
//! What the witness cannot say: that the evidence behind it is enough (a stale look, a clock), which
//! outcomes count as refused, that the user really chose. Those are the owners' and the reviewer's:
//! `rg 'Why::UserChose'` is the whole list of "the user said so", and a ten-line gate
//! (`azul-srcgate/tests/chose_gate.rs`) lists the modules that may call `Chose::job(`.
//!
//! # What cannot be built
//!
//! `Why` is no witness itself, only the choice of one: it has no `Default`, no `Clone` (a reason is
//! spent by the call it is given to; a second call needs a second witness in hand), no `Deserialize` and
//! no `From<&str>`, and its variants take a reference to a witness, which only the owner's module can
//! make:
//!
//! ```compile_fail,E0599
//! // No reason from nothing.
//! use azul_storage::erase::Why;
//! let _why = Why::default();
//! ```
//!
//! ```compile_fail,E0599
//! // No second copy of a reason: a delete spends the one it is given.
//! use azul_storage::erase::{Chose, Target, Why};
//! fn twice(chose: &Chose) {
//!     let why = Why::UserChose(chose);
//!     let again = why.clone();
//!     let _ = (why, again);
//! }
//! ```
//!
//! ```compile_fail,E0277
//! // No reason read from a file or from the wire.
//! use azul_storage::erase::Why;
//! fn from_file(text: &str) -> Option<Why<'static>> {
//!     serde_json::from_str::<Why>(text).ok()
//! }
//! ```
//!
//! ```compile_fail,E0277
//! // No reason from a string.
//! use azul_storage::erase::Why;
//! fn from_text(text: &'static str) -> Why<'static> {
//!     text.into()
//! }
//! ```
//!
//! What compiles - the control of the four above: a reason made of a witness in hand, given to a
//! delete.
//!
//! ```no_run
//! use azul_storage::{erase::{Chose, Target, Why}, Drive, DriveError};
//! fn delete(drive: &dyn Drive, chose: &Chose) -> Result<(), DriveError> {
//!     drive.delete_for("a/b.txt", Why::UserChose(chose))
//! }
//! fn chose() -> Chose {
//!     Chose::job("delete the selected file", Target::Key(String::from("a/b.txt")))
//! }
//! # let _ = (delete, chose);
//! ```

use std::{collections::BTreeSet, fmt};

#[cfg(feature = "encryption")]
use crate::meta::reclaim::{DeleteLicence, PurgeConfirmation, Reclaimable, Refusal};
use crate::{durable::Landed, key, BatchDelete, DriveError};

/// The reason to delete: one of five witnesses, each bound to what it is for ([`Why::covers`]). It is
/// not `Clone`, `Default` or `Deserialize`, and not made from text; a delete spends the one it is
/// given. See the module's documentation for what each witness proves and who makes it.
#[derive(Debug)]
#[must_use]
pub enum Why<'a> {
    /// The history no longer names the object and its retention is over: the licence of the one gate
    /// and the proof for that object (feature `encryption`).
    #[cfg(feature = "encryption")]
    Retired(&'a DeleteLicence, &'a Reclaimable),
    /// The user confirmed that the old versions of every file are erased, and the proof says the
    /// history released this object before they said so (feature `encryption`).
    #[cfg(feature = "encryption")]
    Confirmed(&'a PurgeConfirmation, &'a Reclaimable),
    /// A new version landed at the name: what it replaced may go.
    Superseded(&'a Landed),
    /// The change that was to name a new object was refused for good or never made: the object it
    /// uploaded goes back out (feature `encryption`).
    #[cfg(feature = "encryption")]
    Refused(&'a Refusal),
    /// The job a user started names what it is about.
    UserChose(&'a Chose),
}

impl Why<'_> {
    /// Whether this reason is for removing the object `key`. Every witness answers for its own key
    /// and no other; a key that is no object's name (`..`, absolute, empty, a folder) is covered by
    /// none.
    #[must_use]
    pub fn covers(&self, key: &str) -> bool {
        match self {
            #[cfg(feature = "encryption")]
            Why::Retired(_, proof) | Why::Confirmed(_, proof) => proof.covers(key),
            Why::Superseded(landed) => landed.covers(key),
            #[cfg(feature = "encryption")]
            Why::Refused(refusal) => refusal.covers(key),
            Why::UserChose(chose) => chose.covers(key),
        }
    }

    /// Whether this reason is for removing the folder `prefix` with everything under it. Only a job
    /// that named a folder ([`Target::Folder`]) is: a witness for one object, or for several, is no
    /// reason to remove what happens to be under a name.
    #[must_use]
    pub fn covers_folder(&self, prefix: &str) -> bool {
        match self {
            #[cfg(feature = "encryption")]
            Why::Retired(..) | Why::Confirmed(..) | Why::Refused(_) => false,
            Why::Superseded(_) => false,
            Why::UserChose(chose) => chose.covers_folder(prefix),
        }
    }

    /// What the reason is, in words (the job's own name for [`Why::UserChose`]).
    #[must_use]
    pub fn reason(&self) -> &'static str {
        match self {
            #[cfg(feature = "encryption")]
            Why::Retired(..) => "an object the history no longer names",
            #[cfg(feature = "encryption")]
            Why::Confirmed(..) => "the erasure of the old versions the user confirmed",
            Why::Superseded(_) => "a version that landed",
            #[cfg(feature = "encryption")]
            Why::Refused(_) => "a change that was refused",
            Why::UserChose(chose) => chose.named(),
        }
    }

    /// The first line of [`Drive::delete_for`](crate::Drive::delete_for): the reason is for `key`.
    ///
    /// # Errors
    ///
    /// [`Unlicensed`], and nothing was deleted: the reason is for something else.
    pub fn check(&self, key: &str) -> Result<(), Unlicensed> {
        if self.covers(key) {
            Ok(())
        } else {
            Err(self.unlicensed(key, false))
        }
    }

    /// [`Why::check`] of the folder `prefix`, removed with everything under it.
    ///
    /// # Errors
    ///
    /// [`Unlicensed`], and nothing was deleted.
    pub fn check_folder(&self, prefix: &str) -> Result<(), Unlicensed> {
        if self.covers_folder(prefix) {
            Ok(())
        } else {
            Err(self.unlicensed(prefix, true))
        }
    }

    /// [`Why::check`] of every item of a batch: one key the reason is not for refuses the whole
    /// batch before anything is deleted (a batch that is deleted but for one item is a partial
    /// effect of a call that was wrong).
    ///
    /// # Errors
    ///
    /// [`Unlicensed`] for the first item the reason is not for.
    pub fn check_all(&self, items: &[BatchDelete<'_>]) -> Result<(), Unlicensed> {
        items.iter().try_for_each(|item| self.check(item.key))
    }

    fn unlicensed(&self, target: &str, folder: bool) -> Unlicensed {
        Unlicensed {
            target: target.to_string(),
            offered: self.reason(),
            folder,
        }
    }
}

/// The refusal of a `*_for` method: the reason it was given is not for what it was asked to delete.
/// Nothing was deleted. It reaches the caller as [`DriveError::Denied`] ([`Certainty::Refused`]:
/// the call was refused, no change was made).
///
/// [`Certainty::Refused`]: crate::meta::certainty::Certainty::Refused
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlicensed {
    /// What was to be deleted: the key, or the folder's prefix.
    pub target: String,
    /// What the reason offered is, in words ([`Why::reason`]).
    pub offered: &'static str,
    /// Whether a folder was to be removed.
    pub folder: bool,
}

impl fmt::Display for Unlicensed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = if self.folder {
            "the folder"
        } else {
            "the object"
        };
        write!(
            f,
            "the reason offered ({}) is not for deleting {what} \"{}\": nothing was deleted",
            self.offered, self.target
        )
    }
}

impl std::error::Error for Unlicensed {}

impl From<Unlicensed> for DriveError {
    fn from(refused: Unlicensed) -> Self {
        DriveError::Denied {
            message: refused.to_string(),
        }
    }
}

/// What a job a user started is about: the files or the folder the user chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// One file: its key.
    Key(String),
    /// The files the user picked: their keys.
    Keys(Vec<String>),
    /// A folder and everything under it: its prefix, ending in `/`. The root is no folder to choose.
    Folder(String),
}

/// The job a user started, which names what it is about: the one reason the apps make themselves
/// (the other four are made by the owners of the evidence in this crate). The user chose to delete
/// THESE files or THIS folder - in a dialog, with a click or a command - and the job that carries it
/// out says so in the call: `drive.delete_for(key, Why::UserChose(&chose))`.
///
/// What it does not say is that the user really chose; that is the job's, and the reason this is the
/// one witness with a public constructor, [`Chose::job`], is that the apps have to make it. So the
/// calls of `Chose::job(` are a short list in shipped code - the jobs, the handlers of a user's own
/// request - and a gate (`azul-srcgate/tests/chose_gate.rs`) lists the modules that may make one: a
/// listing, a scan, a retry or a mirror of another side's delete is no job a user started and does
/// not make a `Chose`. A reason that is bound to what the user named is checked against every key
/// the delete is given ([`Why::covers`]): a listing that yields a key outside the folder the user
/// chose is refused.
///
/// It has private fields, no `Default`, no `Clone` and no `Deserialize`, and nothing makes one but
/// `Chose::job`:
///
/// ```compile_fail,E0451
/// // Not made by hand.
/// use azul_storage::erase::Chose;
/// let _chose = Chose { job: "delete", scope: todo!() };
/// ```
///
/// ```compile_fail,E0599
/// // Not from nothing.
/// use azul_storage::erase::Chose;
/// let _chose = Chose::default();
/// ```
///
/// ```compile_fail,E0599
/// // Not copied: a job that deletes several things names them all.
/// use azul_storage::erase::{Chose, Target};
/// let chose = Chose::job("delete", Target::Key(String::from("a.txt")));
/// let _again = chose.clone();
/// ```
///
/// What compiles - the control of the three above:
///
/// ```no_run
/// use azul_storage::erase::{Chose, Target};
/// let chose = Chose::job("delete the folder", Target::Folder(String::from("old/")));
/// assert_eq!(chose.named(), "delete the folder");
/// ```
#[derive(Debug)]
#[must_use]
pub struct Chose {
    job: &'static str,
    scope: Scope,
}

/// What the job named, ready to be asked about a key.
#[derive(Debug)]
enum Scope {
    Key(String),
    Keys(BTreeSet<String>),
    Folder(String),
}

impl Chose {
    /// The job `job` - what the user asked for, in the words the record and the refusal keep
    /// ("delete the selected files") - about `target`. A target no key can be under (an empty
    /// folder, the root) makes a reason that covers nothing.
    pub fn job(job: &'static str, target: Target) -> Chose {
        let scope = match target {
            Target::Key(key) => Scope::Key(key),
            Target::Keys(keys) => Scope::Keys(keys.into_iter().collect()),
            Target::Folder(prefix) => Scope::Folder(prefix),
        };
        Chose { job, scope }
    }

    /// The job's name.
    #[must_use]
    pub fn named(&self) -> &'static str {
        self.job
    }

    /// Whether the user chose to remove the object `key`: it is one of the files the job names, or
    /// under the folder it names (the folder's own marker included), and it is a name an object can
    /// have.
    fn covers(&self, key: &str) -> bool {
        match &self.scope {
            Scope::Key(own) => own == key && is_object(key),
            Scope::Keys(own) => own.contains(key) && is_object(key),
            Scope::Folder(own) => {
                is_folder(own) && (key == own || (key.starts_with(own.as_str()) && is_object(key)))
            }
        }
    }

    /// Whether the user chose to remove the folder `prefix`: it is the folder the job names, or one
    /// under it.
    fn covers_folder(&self, prefix: &str) -> bool {
        match &self.scope {
            Scope::Folder(own) => {
                is_folder(own) && is_folder(prefix) && prefix.starts_with(own.as_str())
            }
            Scope::Key(_) | Scope::Keys(_) => false,
        }
    }
}

/// Whether `key` is a name an object can have: its shape only ([`key::check_key`]).
fn is_object(key: &str) -> bool {
    key::check_key(key).is_ok()
}

/// Whether `prefix` is a folder below the root: `a/`, `a/b/`.
fn is_folder(prefix: &str) -> bool {
    prefix.ends_with('/') && key::check_prefix(prefix).is_ok()
}
