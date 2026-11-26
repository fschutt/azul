//! The hashes of a published board: SHA-256 (the `sha2` crate), written as `sha256:` and 64 lower-case hex digits.
//! One is the hash of the lines a code reference covers - what a viewer, or a later snapshot, compares with the lines at
//! the same place of the file at the reference's revision to tell whether the code is what was circled; another is a
//! snapshot's own hash, which makes a published history checkable (it is the hash of the snapshot's text without the
//! hash itself, [`crate::whiteboard_azboard`]).

use sha2::{Digest, Sha256};

/// The prefix of a hash as it is written.
pub const PREFIX: &str = "sha256:";

/// The 64 hex digits of the SHA-256 of `bytes`.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(char::from(b"0123456789abcdef"[usize::from(b >> 4)]));
        out.push(char::from(b"0123456789abcdef"[usize::from(b & 15)]));
    }
    out
}

/// `sha256:` and the hash of `bytes`.
#[must_use]
pub fn tagged(bytes: &[u8]) -> String {
    format!("{PREFIX}{}", hex(bytes))
}

/// The hash of the lines of code a reference covers: each line, with a line feed after it (a carriage return at its end
/// is not part of it, so a file kept with Windows line ends gives the same hash), as UTF-8 - the text a viewer gets by
/// cutting the lines out of the file and writing a line feed after each. Tagged ([`tagged`]).
#[must_use]
pub fn lines_tagged(lines: &[String]) -> String {
    let mut text = String::with_capacity(lines.iter().map(|l| l.len() + 1).sum());
    for line in lines {
        text.push_str(line.strip_suffix('\r').unwrap_or(line));
        text.push('\n');
    }
    tagged(text.as_bytes())
}

/// Whether `text` is a hash as [`tagged`] writes it.
#[must_use]
pub fn is_tagged(text: &str) -> bool {
    text.strip_prefix(PREFIX).is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

#[cfg(test)]
#[path = "whiteboard_hash_test.rs"]
mod whiteboard_hash_test;
