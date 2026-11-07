//! Unit tests of the storage crate. No network and no libazul: the S3 tests
//! send through a fake transport, the local tests work in a temporary folder.

/// Reads and writes that never go through a link out of the root.
mod beneath;
/// What a probe could not answer: the free space of a disk is a `Cap`, never a number for unknown.
mod cap;
mod catalog;
mod config;
/// A contained drive's folders made and removed without a link; a link as the file itself.
mod contained_links;
/// A transfer told to stop (cancelled, nothing made visible) and a read that names its limit.
mod ctl;
/// Numbers somebody else wrote: `Declared` places itself and sizes nothing, `ByteReader` cannot
/// run past its end, the named limits.
mod declared;
/// A folder read in batches, a listing page that stats only its own entries.
mod dir_read;
/// What a drive leaves on a device: its keyring entries' names, the removal that takes the drives file
/// entry last, the evidence a key needs, the plain file that takes no key.
mod footprint;
/// The entry points of the fuzz targets, on bytes no fuzzer is needed for (feature
/// `encryption`: the formats are the encrypted drive's).
#[cfg(feature = "encryption")]
mod fuzz_entry;
/// Documents with an owner: the drives file changed by several writers, a newer build's fields.
mod docs;
/// Which `Drive` methods the folder on disk overrides and which it leaves to the defaults.
mod drive_coverage;
/// The one durable write path: the order of its steps, what a cut at each leaves, what the name
/// must hold, the bits and the time, a stop, the old file kept in a slot.
mod durable;
#[cfg(feature = "sql")]
mod database;
mod key;
mod keyring;
mod local;
mod manifest;
mod meta;
/// The name policy: the protected names in every spelling, the key that passed, a download's
/// file name.
mod names;
mod oauth;
mod ops;
/// The "downloaded from elsewhere" mark on what a transfer brings from a drive to this disk.
mod quarantine;
/// The restore protocol: the truth table of `judge`, the baseline of a new pairing, the pure
/// rebase of a base after a restore.
mod restore;
mod s3;
mod scoped;
/// The one private place for what a program keeps for a while: a spool above its memory limit, the
/// folders, the names that say who made a file, the sweep.
mod scratch;
mod sigv4;
/// A new data object between its upload and the index change that names it.
#[cfg(feature = "encryption")]
mod staged;
mod tables;
mod time;
mod transfer;
/// What Windows tells that std does not: a file's identity and change time, what a reparse point
/// is, the cap of FAT, the errors that say a file is busy (the model on every host).
mod win;
/// Names a Windows disk cannot hold, refused before a key becomes a path there.
mod windows_names;
/// The conservation matrix of a move: whichever call fails, whenever it is stopped, every file is at
/// the source or at the target.
mod xfer_conserve;
/// The conditional vocabulary on a folder on disk, moves that are renames, a folder removed only
/// when empty, the default moves of a drive with tags.
mod xfer_local;
/// The plan of a transfer: the names a target holds and the names the plan reserves.
mod xfer_plan;
/// The run of a plan: a file lands before its source is touched, a stop inside a file, a name
/// made since the plan not replaced, a move deleting what it copied.
mod xfer_run;
/// Where drives are and whether two places overlap: two drives over one tree, a bucket twice.
mod xfer_site;
mod xml;

/// An S3 service in memory with multipart uploads (the multipart and transfer tests).
mod fake_bucket;
/// Parts of 16 MiB four at once, resumable uploads, conditional uploads, racing writers.
mod multipart;
/// Uploads that must hold the bytes a scan named, resumed after a dropped connection.
mod checked_upload;
/// A checked upload opens its file once: no link, no FIFO, only the file the scan saw.
#[cfg(unix)]
mod checked_open;
/// A request refused because this computer's clock is off, with or without a body.
mod clock_skew;
/// An S3 drive's requests through a router (the endpoint failover's seam).
mod router;
/// Whether a request may be sent twice, and a completion whose answer was lost.
mod idempotency;
/// A request and its answer as HTTP/1.1 bytes (S3 over iroh).
mod http1;
/// The UI thread never waits for a network answer (debug assertions).
mod ui_thread;
/// An upload's time grows with its body, a document's answer is capped, parts follow the link.
mod limits;
/// The roots the Azlin apps pin for the iroh relays.
mod trust;

/// The OpenDAL drive (feature `opendal`).
#[cfg(feature = "opendal")]
mod opendal_drive;

/// The keys of an encrypted drive (feature `encryption`).
#[cfg(feature = "encryption")]
mod crypto;
/// The AZL1 object format (feature `encryption`).
#[cfg(feature = "encryption")]
mod azl1;
/// azlin-compress's own test inputs (deflated ZIPs, PNGs, PDFs, gzip files from seeds), shared
/// so the transformed objects here are made the way the crate's tests make them.
#[cfg(feature = "encryption")]
#[path = "../../../azlin-compress/tests/common/mod.rs"]
pub(crate) mod compress_inputs;
/// AZL1 version 2: transformed files (JPEG, Office, PNG, PDF) sealed, read back every way,
/// refused when changed; compression on by default.
#[cfg(feature = "encryption")]
mod azl1_transform;
/// A bucket in memory that counts and records its calls (the encryption tests).
#[cfg(feature = "encryption")]
mod mem_bucket;
/// The encrypted drive (feature `encryption`).
#[cfg(feature = "encryption")]
mod encrypted;
/// The keys on a device: setup, unlock, invites, recovery (feature `encryption`).
#[cfg(feature = "encryption")]
mod device;
/// A plaintext drive moved into its encrypted namespace (feature `encryption`).
#[cfg(feature = "encryption")]
mod migrate;
/// The recompression pass (feature `encryption`).
#[cfg(feature = "encryption")]
mod recompress;
/// An encrypted file's upload cut off and resumed at its next part (feature `encryption`).
#[cfg(feature = "encryption")]
mod encrypted_resume;
/// The drops of incoming mail, AZD1 (feature `encryption`).
#[cfg(feature = "encryption")]
mod drops;
/// Share manifests and links (feature `encryption`).
#[cfg(feature = "encryption")]
mod sharing;
/// The key rotation and re-encryption (feature `encryption`).
#[cfg(feature = "encryption")]
mod rotation;
/// The key rotation through the drive index (feature `encryption`).
#[cfg(feature = "encryption")]
mod rotation_meta;
/// The key flows over the drive index's policy (feature `encryption`).
#[cfg(feature = "encryption")]
mod policy_keys;
/// A device's pin of its drive key, and the key ids join and claim codes carry (feature
/// `encryption`).
#[cfg(feature = "encryption")]
mod key_pin;
/// Small files packed together on an encrypted drive (feature `encryption`).
#[cfg(feature = "encryption")]
mod packing;

/// A fresh folder under the system's temporary folder, removed when dropped.
pub(crate) use crate::testing::TempDir;
/// Shamir's secret sharing over GF(256) (feature `encryption`).
#[cfg(feature = "encryption")]
mod shamir;
/// Trusted contacts: a recovery code's shares, sealed and printed (feature `encryption`).
#[cfg(feature = "encryption")]
mod contacts;
/// A drive's further recovery codes (feature `encryption`).
#[cfg(feature = "encryption")]
mod recovery_codes;
