//! The name policy of a synced folder lives in azul-storage ([`azul_storage::names`], ARCH28 RF42):
//! one table of what a name may be, the protected names in every spelling, the key that passed
//! the policy. The sync reaches it here under the names it always used.

pub use azul_storage::names::*;
