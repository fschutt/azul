//! Deterministic content hashing.
//!
//! [`GetHash`] used to live in `codegen::format`, which forced every runtime
//! user (`core::dom`, `core::resources`, the layout solver) to compile the code
//! generator. It is plain runtime infrastructure, so it lives here, always
//! compiled; `codegen::format::GetHash` re-exports it for old import paths.

/// Returns a deterministic 64-bit hash for content-based deduplication.
pub trait GetHash {
    /// Hashes `self` with the std `DefaultHasher` (SipHash, fixed keys).
    fn get_hash(&self) -> u64;
}

impl<T: core::hash::Hash> GetHash for T {
    fn get_hash(&self) -> u64 {
        use core::hash::Hasher;
        let mut hasher = std::hash::DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}
