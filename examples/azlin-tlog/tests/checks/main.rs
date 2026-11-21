//! The acceptance checks T1-T35 of the lane brief (W1-S45c), entering through the public API of `azlin-tlog`
//! only. Every test name starts with its check number. The goldens (the note of T1, the cosignature of T8,
//! the record of T21, the bundle of T27) were made OUTSIDE the crate: SHA-256 by Python's `hashlib`, Ed25519
//! by OpenSSL 3.5 (`pkeyutl -sign -rawin`), the Merkle tree by a transcription of RFC 9162 in Python
//! (scratch/W1-S45c/gold/make_gold*.py), so a bug shared by the code and its tests cannot hide in them.
//!
//! - `notes`: T1-T7. `cosign`: T8-T10. `policy`: T11-T15. `witness`: T16-T20. `records`: T21-T25.
//! - `bundles`: T26-T29. `monitor`: T30-T32. `props`: T33. `limits`: T34, T35. `hygiene`: the crate's rules.

#![allow(
    clippy::needless_range_loop,
    clippy::type_complexity,
    clippy::too_many_arguments
)]

mod bundles;
mod common;
mod cosign;
mod gold;
mod hygiene;
mod limits;
mod monitor;
mod notes;
mod policy;
mod props;
mod records;
mod witness;
