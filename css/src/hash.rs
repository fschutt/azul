//! Deterministic content hashing.
//!
//! [`GetHash`] used to live in `codegen::format`, which forced every runtime
//! user (`core::dom`, `core::resources`, the layout solver) to compile the code
//! generator. It is plain runtime infrastructure, so it lives here, always
//! compiled; `codegen::format::GetHash` re-exports it for old import paths.

/// Returns a deterministic 64-bit hash for content-based deduplication.
pub trait GetHash {
    /// Hashes `self` with [`SipHasher13`] (`SipHash` 1-3, fixed keys).
    fn get_hash(&self) -> u64;
}

impl<T: core::hash::Hash> GetHash for T {
    fn get_hash(&self) -> u64 {
        use core::hash::Hasher;
        let mut hasher = SipHasher13::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

/// `SipHash` 1-3 with both keys zero, the hash std's `DefaultHasher::new()`
/// computes.
///
/// It is written out here so hashing needs no `std`, and so its values stay
/// the same when std changes its algorithm: the code generator puts them into
/// the names of the items it emits.
#[derive(Debug, Clone, Copy)]
pub struct SipHasher13 {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    /// The bytes of an unfinished 8-byte block, little-endian.
    tail: u64,
    /// How many bytes `tail` holds (0 to 7).
    ntail: usize,
    /// How many bytes were written; its low byte goes into the last block.
    length: usize,
}

impl SipHasher13 {
    /// A hasher that has seen no bytes yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            v0: 0x736f_6d65_7073_6575,
            v1: 0x646f_7261_6e64_6f6d,
            v2: 0x6c79_6765_6e65_7261,
            v3: 0x7465_6462_7974_6573,
            tail: 0,
            ntail: 0,
            length: 0,
        }
    }

    /// One `SipRound`.
    const fn round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v1 = self.v1.rotate_left(13);
        self.v1 ^= self.v0;
        self.v3 = self.v3.rotate_left(16);
        self.v3 ^= self.v2;
        self.v0 = self.v0.rotate_left(32);

        self.v2 = self.v2.wrapping_add(self.v1);
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v1 = self.v1.rotate_left(17);
        self.v1 ^= self.v2;
        self.v3 = self.v3.rotate_left(21);
        self.v3 ^= self.v0;
        self.v2 = self.v2.rotate_left(32);
    }

    /// Mixes one full 8-byte block into the state (one compression round).
    const fn compress(&mut self, block: u64) {
        self.v3 ^= block;
        self.round();
        self.v0 ^= block;
    }
}

impl Default for SipHasher13 {
    fn default() -> Self {
        Self::new()
    }
}

/// Up to 8 bytes as a little-endian number.
fn le_u64(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .fold(0, |out, (i, byte)| out | (u64::from(*byte) << (8 * i)))
}

// Only `write` and `finish`, like std's: the integer methods and `write_str`
// keep their default (the bytes, and a 0xFF after a string), which is what
// std's `DefaultHasher` hashes too.
impl core::hash::Hasher for SipHasher13 {
    fn write(&mut self, msg: &[u8]) {
        self.length = self.length.wrapping_add(msg.len());
        let mut rest = msg;

        // First complete the block an earlier write left unfinished.
        if self.ntail != 0 {
            let needed = 8 - self.ntail;
            let (head, after) = rest.split_at(needed.min(rest.len()));
            self.tail |= le_u64(head) << (8 * self.ntail);
            if head.len() < needed {
                self.ntail += head.len();
                return;
            }
            self.compress(self.tail);
            rest = after;
        }

        let mut blocks = rest.chunks_exact(8);
        for block in &mut blocks {
            self.compress(le_u64(block));
        }
        let left = blocks.remainder();
        self.tail = le_u64(left);
        self.ntail = left.len();
    }

    fn finish(&self) -> u64 {
        let mut state = *self;
        let last = ((state.length as u64 & 0xff) << 56) | state.tail;
        state.compress(last);
        state.v2 ^= 0xff;
        state.round();
        state.round();
        state.round();
        state.v0 ^ state.v1 ^ state.v2 ^ state.v3
    }
}

#[cfg(test)]
mod tests {
    use core::hash::{Hash, Hasher};
    use std::hash::DefaultHasher;

    use super::{GetHash, SipHasher13};

    /// Writes `bytes` into both hashers in the same pieces and compares.
    fn same_as_std(bytes: &[u8], piece: usize) {
        let mut ours = SipHasher13::new();
        let mut theirs = DefaultHasher::new();
        for chunk in bytes.chunks(piece.max(1)) {
            ours.write(chunk);
            theirs.write(chunk);
        }
        assert_eq!(
            ours.finish(),
            theirs.finish(),
            "{} bytes written {piece} at a time",
            bytes.len()
        );
    }

    #[test]
    fn it_hashes_byte_streams_like_std_whatever_the_pieces() {
        let bytes: Vec<u8> = (0..=255u8).cycle().take(300).collect();
        for len in 0..=40 {
            for piece in 1..=17 {
                same_as_std(&bytes[..len], piece);
            }
        }
        same_as_std(&bytes, 300);
        same_as_std(&bytes, 7);
    }

    #[test]
    fn get_hash_equals_std_default_hasher_for_typical_values() {
        fn std_hash<T: Hash>(value: &T) -> u64 {
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            hasher.finish()
        }
        assert_eq!(0u8.get_hash(), std_hash(&0u8));
        assert_eq!(u64::MAX.get_hash(), std_hash(&u64::MAX));
        assert_eq!((-7i32).get_hash(), std_hash(&-7i32));
        assert_eq!("".get_hash(), std_hash(&""));
        assert_eq!("system:accent".get_hash(), std_hash(&"system:accent"));
        let mixed = (3usize, "a long enough string to cross blocks", [1u16, 2, 3], Some('x'));
        assert_eq!(mixed.get_hash(), std_hash(&mixed));
        let list = vec![String::from("x"), String::from("yz")];
        assert_eq!(list.get_hash(), std_hash(&list));
    }
}
