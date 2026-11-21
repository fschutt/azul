//! The deterministic encoder: the core deterministic encoding of RFC 8949 section 4.2.1 on the
//! profile of `Value` (shortest integers and lengths, definite lengths, map keys in the bytewise
//! order of their encodings: the order `Map` already holds).
//!
//! Implements planning/architecture.md section 5.1 principle 6. The encoder works in two passes:
//! the first checks every limit and measures the exact output, the second writes into a buffer of
//! exactly that size, so nothing is ever copied by a growing `Vec` (a value that carries key
//! material leaves no stray copy of it in freed memory) and the second pass cannot fail.

use crate::{error::EncodeError, limits::Limits, value::Value};

/// The one deterministic encoding of `v`, or the limit it breaks. For every `Ok(bytes)`,
/// `decode(&bytes, l)` returns `v` again; for every input `decode` accepts, `encode` of the result
/// returns that input (the signature rule: a signature covers canonical bytes).
///
/// Caller: every format crate that writes a format (azlin-keys, azlin-content, azlin-creds, ...),
/// INT-WIRE (azlin-proto contract4).
pub fn encode(v: &Value, l: &Limits) -> Result<Vec<u8>, EncodeError> {
    let mut measure = Measure { l, total: 0 };
    let len = measure.value(v, 0)?;
    if len > u64::from(l.max_input) {
        return Err(EncodeError::InputTooLarge);
    }
    let cap = usize::try_from(len).map_err(|_| EncodeError::InputTooLarge)?;
    let mut out = Vec::with_capacity(cap);
    write(&mut out, v);
    Ok(out)
}

/// The first pass: every limit of `Limits`, and the length of the encoding.
struct Measure<'l> {
    l: &'l Limits,
    /// Data items counted so far (against `max_total_items`).
    total: u32,
}

impl Measure<'_> {
    /// The encoded length of `v`, which sits inside `depth` arrays and maps.
    fn value(&mut self, v: &Value, depth: u8) -> Result<u64, EncodeError> {
        self.total = match self.total.checked_add(1) {
            Some(t) if t <= self.l.max_total_items => t,
            _ => return Err(EncodeError::TooManyItems),
        };
        match v {
            Value::Uint(n) | Value::Nint(n) => Ok(head_len(*n)),
            Value::Bytes(b) => self.string(b.len()),
            Value::Text(s) => self.string(s.len()),
            Value::Bool(_) | Value::Null => Ok(1),
            Value::Array(items) => {
                let inner = self.enter(depth)?;
                let n = self.count(items.len())?;
                let mut len = head_len(n);
                for item in items {
                    len = add(len, self.value(item, inner)?)?;
                }
                Ok(len)
            }
            Value::Map(m) => {
                let inner = self.enter(depth)?;
                let n = self.count(m.len())?;
                let mut len = head_len(n);
                for (key, value) in m.iter() {
                    len = add(len, self.value(key, inner)?)?;
                    len = add(len, self.value(value, inner)?)?;
                }
                Ok(len)
            }
        }
    }

    /// The depth inside a container that sits at `depth`.
    fn enter(&self, depth: u8) -> Result<u8, EncodeError> {
        match depth.checked_add(1) {
            Some(inner) if inner <= self.l.max_depth => Ok(inner),
            _ => Err(EncodeError::DepthExceeded),
        }
    }

    /// The item count of a container, as the number its head carries.
    fn count(&self, n: usize) -> Result<u64, EncodeError> {
        let n = u64::try_from(n).map_err(|_| EncodeError::TooManyItems)?;
        if n > u64::from(self.l.max_items) {
            return Err(EncodeError::TooManyItems);
        }
        Ok(n)
    }

    /// The encoded length of a byte or text string of `n` bytes.
    fn string(&self, n: usize) -> Result<u64, EncodeError> {
        let n = u64::try_from(n).map_err(|_| EncodeError::InputTooLarge)?;
        if n > u64::from(self.l.max_text) {
            return Err(EncodeError::InputTooLarge);
        }
        add(head_len(n), n)
    }
}

fn add(a: u64, b: u64) -> Result<u64, EncodeError> {
    a.checked_add(b).ok_or(EncodeError::InputTooLarge)
}

/// The bytes of a head (initial byte and argument) in the shortest form.
fn head_len(arg: u64) -> u64 {
    if arg < 24 {
        1
    } else if arg <= 0xff {
        2
    } else if arg <= 0xffff {
        3
    } else if arg <= 0xffff_ffff {
        5
    } else {
        9
    }
}

/// `n` as the argument of a head. The input was measured, so it fits a `u64` on every target.
fn len64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// The head of major type `major` with argument `arg`, in the shortest form (the same thresholds
/// as `head_len`).
fn put_head(out: &mut Vec<u8>, major: u8, arg: u64) {
    let m = major << 5;
    if let Ok(b) = u8::try_from(arg) {
        if b < 24 {
            out.push(m | b);
        } else {
            out.push(m | 24);
            out.push(b);
        }
    } else if let Ok(w) = u16::try_from(arg) {
        out.push(m | 25);
        out.extend_from_slice(&w.to_be_bytes());
    } else if let Ok(w) = u32::try_from(arg) {
        out.push(m | 26);
        out.extend_from_slice(&w.to_be_bytes());
    } else {
        out.push(m | 27);
        out.extend_from_slice(&arg.to_be_bytes());
    }
}

/// The second pass: `v` was measured against the limits, so this cannot fail and recurses no
/// deeper than `Limits::max_depth`.
fn write(out: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Uint(n) => put_head(out, 0, *n),
        Value::Nint(n) => put_head(out, 1, *n),
        Value::Bytes(b) => {
            put_head(out, 2, len64(b.len()));
            out.extend_from_slice(b);
        }
        Value::Text(s) => {
            put_head(out, 3, len64(s.len()));
            out.extend_from_slice(s.as_bytes());
        }
        Value::Array(items) => {
            put_head(out, 4, len64(items.len()));
            for item in items {
                write(out, item);
            }
        }
        Value::Map(m) => {
            put_head(out, 5, len64(m.len()));
            for (key, value) in m.iter() {
                write(out, key);
                write(out, value);
            }
        }
        Value::Bool(false) => out.push(0xf4),
        Value::Bool(true) => out.push(0xf5),
        Value::Null => out.push(0xf6),
    }
}
