//! Decoded secrets: a decoded tree whose byte and text buffers are wiped when it drops, and when the
//! decoder refuses its input half way.
//!
//! Implements planning/architecture.md section 1.4 (the quality rule of the launch rules: "buffers
//! that held plaintext or key material are `Zeroizing`") for the formats whose decoded bytes ARE or
//! DERIVE key material: the chunk hashes `full` of a pointer (section 5.7), escrow shares and
//! recovery material (section 5.5), any plaintext secret. Decision D-W1S14A (DECISIONS-TAKEN.md),
//! rule R17 of the W1 common rules, milestone `M-CBOR-SECRET`. ADDITIVE: `decode` and `Value` do not
//! change, and `zeroize` is this crate's one dependency.
//!
//! One decoder serves both entry points (`dec::decode_with`): `decode_secret` accepts and refuses
//! exactly what `decode` does, with the same error, and differs in one thing: what it has decoded
//! when it refuses an input is wiped before the error returns.
//!
//! What is wiped: every byte string and text string buffer the decoder allocated, the whole of its
//! capacity, once (nested arrays, maps, map keys and map values included). What is not: the input
//! slice (the caller's buffer: `Zeroizing`) and any copy the caller makes of a leaf
//! (`IntMap::req_bytes_n` returns a copy on purpose: the caller puts it into a `Secret<N>`).

use core::fmt;

use zeroize::Zeroize;

use crate::{dec, error::DecodeError, limits::Limits, value::Value};

/// A decoded value that holds key material: when it drops, EVERY byte string and text string buffer
/// of the whole tree (nested arrays, maps, map keys and map values, so the values behind the keys of
/// an `IntMap` too) is overwritten with zeros first. It is not `Clone` (a copy is a second tree to
/// wipe), has no `PartialEq` (an equality of secrets is made in constant time on `Secret` types, not
/// on a tree) and no serialization, and its `Debug` shows no content. The readers take
/// [`as_value`](SecretValue::as_value).
///
/// Seam: lanes S14d (pointer v2), S15b (envelope v2), S15d (recovery) and S17f (escrow) decode their
/// key-bearing formats with `decode_secret` (rule R17 of the W1 common rules).
pub struct SecretValue(Value);

impl SecretValue {
    /// Wraps a tree that was not made by `decode_secret` (one built from key material before it is
    /// encoded), so that its buffers are wiped when the wrapper drops.
    ///
    /// Seam: lanes S14d, S15b, S15d and S17f, for the trees they build from key material.
    pub fn new(value: Value) -> SecretValue {
        SecretValue(value)
    }

    /// The tree, for the readers (`IntMap::new(secret.as_value())`) and for `encode`. Reading it
    /// wipes nothing: the wipe is the drop of the `SecretValue`.
    ///
    /// Seam: lanes S14d, S15b, S15d and S17f.
    pub fn as_value(&self) -> &Value {
        &self.0
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        wipe_tree(&mut self.0);
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue(<redacted>)")
    }
}

/// The value of `input`, which must be exactly one item in the deterministic encoding, exactly as
/// [`decode`](crate::decode) reads it (the same code: the same strictness, the same limits, the same
/// error), as a [`SecretValue`]. When it refuses the input after it has decoded some byte or text
/// strings (a later item is malformed, a map key is out of order, bytes follow the item), the
/// partial tree is wiped before the error returns.
///
/// ```
/// use azlin_cbor::{decode_secret, IntMap, IntMapBuilder, Limits, Value};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // A structure that carries 32 bytes of key material under key 1.
/// let wire = azlin_cbor::encode(
///     &IntMapBuilder::new().put(1, Value::Bytes(vec![7; 32])).build()?,
///     &Limits::OBJECT,
/// )?;
/// let secret = decode_secret(&wire, &Limits::OBJECT)?;
/// let mut m = IntMap::new(secret.as_value())?;
/// let key: [u8; 32] = m.req_bytes_n(1)?; // the caller puts this copy into its `Secret<32>`
/// m.finish()?;
/// assert_eq!(key, [7; 32]);
/// drop(secret); // the 32 bytes inside the tree are zeroed here
/// # Ok(())
/// # }
/// ```
///
/// Seam: lanes S14d (pointer v2), S15b (envelope v2), S15d (recovery) and S17f (escrow), for every
/// format whose decoded bytes are or derive key material (rule R17 of the W1 common rules).
pub fn decode_secret(input: &[u8], limits: &Limits) -> Result<SecretValue, DecodeError> {
    dec::decode_with::<true>(input, limits).map(SecretValue::new)
}

/// What the decoder's guards and the drop of a `SecretValue` know how to wipe.
pub(crate) trait Wipe: Sized {
    /// Wipes every byte and text buffer in `self`.
    fn wipe(&mut self);
    /// Takes the content out and leaves an empty `Self` behind: the guard that held it then has
    /// nothing left to wipe.
    fn vacate(&mut self) -> Self;
}

impl Wipe for Value {
    fn wipe(&mut self) {
        wipe_tree(self);
    }

    fn vacate(&mut self) -> Value {
        core::mem::replace(self, Value::Null)
    }
}

impl Wipe for Vec<Value> {
    fn wipe(&mut self) {
        let mut work = core::mem::take(self);
        drain(&mut work);
    }

    fn vacate(&mut self) -> Vec<Value> {
        core::mem::take(self)
    }
}

impl Wipe for Vec<(Value, Value)> {
    fn wipe(&mut self) {
        let mut work: Vec<Value> = Vec::new();
        for (key, value) in core::mem::take(self) {
            work.push(key);
            work.push(value);
        }
        drain(&mut work);
    }

    fn vacate(&mut self) -> Vec<(Value, Value)> {
        core::mem::take(self)
    }
}

/// What the decoder holds while it is not a part of a finished `Value`: one decoded map key, or the
/// items or pairs an array or a map has decoded so far. The decoder moves the content out with
/// `release` when the item is complete; if the decode stops early instead (an `Err` leaves through
/// a `?`), the guard drops with the content in it, and while `SECRET` is set that wipes every leaf
/// of it. Without `SECRET` the drop does nothing, and the plain `decode` is what it was.
pub(crate) struct Guard<T: Wipe, const SECRET: bool> {
    pub(crate) held: T,
}

impl<T: Wipe, const SECRET: bool> Guard<T, SECRET> {
    pub(crate) fn new(held: T) -> Self {
        Guard { held }
    }

    /// The finished content. What is left in the guard is empty, so its drop wipes nothing.
    pub(crate) fn release(mut self) -> T {
        self.held.vacate()
    }
}

impl<T: Wipe, const SECRET: bool> Drop for Guard<T, SECRET> {
    fn drop(&mut self) {
        if SECRET {
            self.held.wipe();
        }
    }
}

/// Wipes every byte and text buffer of the tree under `root`: nested arrays, maps, map keys and map
/// values. No recursion: a tree of any depth (one built by hand, not by the decoder, which stops at
/// `Limits::max_depth`) is walked on a work list.
pub(crate) fn wipe_tree(root: &mut Value) {
    let mut work: Vec<Value> = Vec::new();
    wipe_node(root, &mut work);
    drain(&mut work);
}

/// Wipes the nodes of `work`, and what is below them, until none is left.
fn drain(work: &mut Vec<Value>) {
    while let Some(mut node) = work.pop() {
        wipe_node(&mut node, work);
    }
}

/// THE wipe, the one place a buffer of a secret tree is overwritten (and, under test, counted): a
/// byte string or a text string is zeroized; the children of an array or a map are moved onto
/// `work`, so that each is visited once.
fn wipe_node(node: &mut Value, work: &mut Vec<Value>) {
    match node {
        Value::Bytes(buffer) => {
            #[cfg(test)]
            counter::note(buffer.len());
            buffer.zeroize();
        }
        Value::Text(text) => {
            #[cfg(test)]
            counter::note(text.len());
            text.zeroize();
        }
        Value::Array(items) => work.append(items),
        Value::Map(map) => {
            for (key, value) in core::mem::take(map).into_entries() {
                work.push(key);
                work.push(value);
            }
        }
        Value::Uint(_) | Value::Nint(_) | Value::Bool(_) | Value::Null => {}
    }
}

/// The count of the wipe, for the tests of this crate (`secret_tests.rs`): the bytes and the
/// buffers that `wipe_node` has zeroized on this thread since the last `reset`.
#[cfg(test)]
pub(crate) mod counter {
    use std::cell::Cell;

    thread_local! {
        static WIPED: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
    }

    pub(crate) fn note(bytes: usize) {
        WIPED.with(|c| {
            let (b, n) = c.get();
            c.set((b + bytes, n + 1));
        });
    }

    pub(crate) fn reset() {
        WIPED.with(|c| c.set((0, 0)));
    }

    /// (bytes, buffers) wiped since the last `reset`.
    pub(crate) fn get() -> (usize, usize) {
        WIPED.with(|c| c.get())
    }
}
