//! The strict decoder: the input is ONE item in the one deterministic encoding of the profile, or
//! it is refused with the reason. Everything `encode` would write differently is refused, so for
//! every accepted `b`, `encode(decode(b)) == b` (the signature rule of the brief).
//!
//! Implements planning/architecture.md section 5.1 principle 6 and section 1.4 rules 3 and 4
//! (a pure codec, every limit stated). The decoder recurses at most `Limits::max_depth` (at most
//! 255) levels, and reserves memory only for items that the input still left can hold.
//!
//! Every initial byte is dealt with in the order: break, tag, simple value, indefinite length,
//! reserved additional information, then the argument (whose shortest form is checked) and the
//! content.
//!
//! One decoder serves `decode` and `decode_secret` (milestone `M-CBOR-SECRET`, decision D-W1S14A):
//! `Reader<SECRET>` differs in one thing, the guards around what an array, a map or a map key has
//! decoded so far wipe it when the decode stops early. With `SECRET` unset the guards do nothing,
//! and `decode` is the decoder it was.

use crate::{
    error::DecodeError,
    limits::Limits,
    secret::{wipe_tree, Guard},
    value::{Map, Value},
};

/// The value of `bytes`, which must be exactly one item in the deterministic encoding: shortest
/// integers and lengths, definite lengths, map keys strictly ascending in the bytewise order of
/// their encodings, valid UTF-8 text, nothing outside the profile, nothing after the item.
///
/// Caller: every format crate that reads a format (azlin-keys, azlin-content, azlin-creds, ...),
/// INT-WIRE (azlin-proto contract4).
pub fn decode(bytes: &[u8], l: &Limits) -> Result<Value, DecodeError> {
    decode_with::<false>(bytes, l)
}

/// The decoder both entry points share. With `SECRET` set, whatever it has decoded when it refuses
/// the input is wiped before the error returns; the accepted input and every error are the same.
pub(crate) fn decode_with<const SECRET: bool>(
    bytes: &[u8],
    l: &Limits,
) -> Result<Value, DecodeError> {
    match u64::try_from(bytes.len()) {
        Ok(n) if n <= u64::from(l.max_input) => {}
        _ => return Err(DecodeError::InputTooLarge),
    }
    let mut reader = Reader::<SECRET> {
        rest: bytes,
        l,
        total: 0,
    };
    let mut value = reader.item(0)?;
    if reader.rest.is_empty() {
        Ok(value)
    } else {
        // The whole tree is decoded and then refused: for a secret it is wiped too.
        if SECRET {
            wipe_tree(&mut value);
        }
        Err(DecodeError::TrailingBytes)
    }
}

struct Reader<'a, 'l, const SECRET: bool> {
    /// The input that is not read yet.
    rest: &'a [u8],
    l: &'l Limits,
    /// Data items started so far (against `max_total_items`).
    total: u32,
}

impl<'a, const SECRET: bool> Reader<'a, '_, SECRET> {
    /// `n` bytes, or `Truncated`: nothing is copied or reserved before the input is known to hold
    /// them.
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let (head, tail) = self
            .rest
            .split_at_checked(n)
            .ok_or(DecodeError::Truncated)?;
        self.rest = tail;
        Ok(head)
    }

    fn take_byte(&mut self) -> Result<u8, DecodeError> {
        match self.take(1)? {
            [b] => Ok(*b),
            _ => Err(DecodeError::Truncated),
        }
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        <[u8; N]>::try_from(self.take(N)?).map_err(|_| DecodeError::Truncated)
    }

    /// Counts one data item against `max_total_items`.
    fn count_item(&mut self) -> Result<(), DecodeError> {
        match self.total.checked_add(1) {
            Some(t) if t <= self.l.max_total_items => {
                self.total = t;
                Ok(())
            }
            _ => Err(DecodeError::TooManyItems),
        }
    }

    /// The data items the input may still hold under `max_total_items`.
    fn items_left(&self) -> u64 {
        u64::from(self.l.max_total_items.saturating_sub(self.total))
    }

    /// The depth inside a container that sits at `depth`.
    fn enter(&self, depth: u8) -> Result<u8, DecodeError> {
        match depth.checked_add(1) {
            Some(inner) if inner <= self.l.max_depth => Ok(inner),
            _ => Err(DecodeError::DepthExceeded),
        }
    }

    /// The argument of a head whose additional information is `ai` (0 to 27), in the shortest
    /// form only.
    fn argument(&mut self, ai: u8) -> Result<u64, DecodeError> {
        match ai {
            0..=23 => Ok(u64::from(ai)),
            24 => {
                let n = self.take_byte()?;
                if n < 24 {
                    return Err(DecodeError::NotShortest);
                }
                Ok(u64::from(n))
            }
            25 => {
                let n = u16::from_be_bytes(self.take_array::<2>()?);
                if n <= 0xff {
                    return Err(DecodeError::NotShortest);
                }
                Ok(u64::from(n))
            }
            26 => {
                let n = u32::from_be_bytes(self.take_array::<4>()?);
                if n <= 0xffff {
                    return Err(DecodeError::NotShortest);
                }
                Ok(u64::from(n))
            }
            27 => {
                let n = u64::from_be_bytes(self.take_array::<8>()?);
                if n <= 0xffff_ffff {
                    return Err(DecodeError::NotShortest);
                }
                Ok(n)
            }
            // 28 to 30 are reserved by the RFC; the caller has dealt with 31.
            _ => Err(DecodeError::DisallowedType),
        }
    }

    /// One data item that sits inside `depth` arrays and maps.
    fn item(&mut self, depth: u8) -> Result<Value, DecodeError> {
        self.count_item()?;
        let initial = self.take_byte()?;
        let major = initial >> 5;
        let ai = initial & 0x1f;
        match major {
            7 => return simple(ai),
            // Tags (and their reserved or indefinite heads) are not in the profile.
            6 => return Err(DecodeError::DisallowedType),
            _ => {}
        }
        if ai == 31 {
            // Indefinite length: strings, arrays and maps. On an integer it is no item at all.
            return Err(if major >= 2 {
                DecodeError::IndefiniteLength
            } else {
                DecodeError::DisallowedType
            });
        }
        if ai >= 28 {
            return Err(DecodeError::DisallowedType);
        }
        let arg = self.argument(ai)?;
        match major {
            0 => Ok(Value::Uint(arg)),
            1 => Ok(Value::Nint(arg)),
            2 => Ok(Value::Bytes(self.string(arg)?.to_vec())),
            3 => {
                let bytes = self.string(arg)?;
                let text = core::str::from_utf8(bytes).map_err(|_| DecodeError::InvalidUtf8)?;
                Ok(Value::Text(text.to_owned()))
            }
            4 => self.array(arg, depth),
            _ => self.map(arg, depth),
        }
    }

    /// The content of a byte or text string of `n` bytes.
    fn string(&mut self, n: u64) -> Result<&'a [u8], DecodeError> {
        // DRIFT X-DELTA-W1S14A-5: the brief has no variant for a string over `max_text`.
        if n > u64::from(self.l.max_text) {
            return Err(DecodeError::InputTooLarge);
        }
        // `n` fits a `u32`; where `usize` is smaller it cannot be in the input either.
        let n = usize::try_from(n).map_err(|_| DecodeError::Truncated)?;
        self.take(n)
    }

    /// An array of `n` items. The capacity reserved is bounded by the limits and by the input that
    /// is left (every item takes at least one byte), never by the claim `n` alone.
    fn array(&mut self, n: u64, depth: u8) -> Result<Value, DecodeError> {
        let inner = self.enter(depth)?;
        if n > u64::from(self.l.max_items) || n > self.items_left() {
            return Err(DecodeError::TooManyItems);
        }
        let mut items =
            Guard::<Vec<Value>, SECRET>::new(Vec::with_capacity(reserve(n, self.rest.len())));
        for _ in 0..n {
            let item = self.item(inner)?;
            items.held.push(item);
        }
        Ok(Value::Array(items.release()))
    }

    /// A map of `n` pairs: the keys must be strictly ascending in the bytewise order of their
    /// encodings, which is compared on the input bytes themselves (the shortest forms were
    /// checked, so they ARE the encodings).
    fn map(&mut self, n: u64, depth: u8) -> Result<Value, DecodeError> {
        let inner = self.enter(depth)?;
        let nodes = n.checked_mul(2).ok_or(DecodeError::TooManyItems)?;
        if n > u64::from(self.l.max_items) || nodes > self.items_left() {
            return Err(DecodeError::TooManyItems);
        }
        // Every pair takes at least two bytes.
        let mut entries = Guard::<Vec<(Value, Value)>, SECRET>::new(Vec::with_capacity(reserve(
            n,
            self.rest.len() / 2,
        )));
        let mut previous: Option<&'a [u8]> = None;
        for _ in 0..n {
            let before = self.rest;
            // The key is held by a guard until its pair is complete: the order check and the value
            // can both fail after it is decoded.
            let key = Guard::<Value, SECRET>::new(self.item(inner)?);
            let used = before
                .len()
                .checked_sub(self.rest.len())
                .ok_or(DecodeError::Truncated)?;
            let encoded = before.get(..used).ok_or(DecodeError::Truncated)?;
            if let Some(prev) = previous {
                match prev.cmp(encoded) {
                    core::cmp::Ordering::Less => {}
                    core::cmp::Ordering::Equal => return Err(DecodeError::DuplicateMapKey),
                    core::cmp::Ordering::Greater => return Err(DecodeError::UnsortedMapKeys),
                }
            }
            previous = Some(encoded);
            let value = self.item(inner)?;
            entries.held.push((key.release(), value));
        }
        Ok(Value::Map(Map::from_sorted(entries.release())))
    }
}

/// How many items to make room for: the claim `n`, but never more than `room`, the most items the
/// input that is left can still hold (every item takes at least one byte, every map pair two). A
/// claim of 2^32 items in a 10-byte input makes room for at most 10: no allocation follows a
/// number the input has not paid for (planning/architecture.md section 1.4 rule 4).
fn reserve(claim: u64, room: usize) -> usize {
    usize::try_from(claim).map_or(room, |n| n.min(room))
}

/// An item of major type 7 with additional information `ai`: `false`, `true` and `null` are in the
/// profile; the break is reserved; floats, `undefined` and the other simple values are not.
fn simple(ai: u8) -> Result<Value, DecodeError> {
    match ai {
        20 => Ok(Value::Bool(false)),
        21 => Ok(Value::Bool(true)),
        22 => Ok(Value::Null),
        31 => Err(DecodeError::ReservedBreak),
        _ => Err(DecodeError::DisallowedType),
    }
}

#[cfg(test)]
mod tests {
    use super::reserve;

    #[test]
    fn c8_reservation_never_exceeds_the_input_that_is_left() {
        // The claim of a hostile head, whatever it is, makes room for what the input can hold.
        assert_eq!(reserve(u64::MAX, 10), 10);
        assert_eq!(reserve(u64::from(u32::MAX), 9), 9);
        assert_eq!(reserve(1 << 40, 0), 0);
        // An honest claim gets exactly its room.
        assert_eq!(reserve(3, 10), 3);
        assert_eq!(reserve(10, 10), 10);
        assert_eq!(reserve(0, 10), 0);
    }
}
