//! Base64 (RFC 4648, the standard alphabet, padded) and the `data:` address of a PNG: the pictures of a board that is
//! exported as one JSON file travel inside it as `data:image/png;base64,...`, the way a web page holds a picture.
//! Std only, strict (a text that is not exactly what [`encode`] writes is refused: no whitespace, no other alphabet,
//! padding only at the end and only where it is needed, the unused bits zero) and bounded (a size the caller does not
//! allow is refused before any byte is made).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The start of the address of a PNG.
const PNG_PREFIX: &str = "data:image/png;base64,";

/// Why a text is not what was wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base64Error {
    /// It would be longer than the caller allows.
    TooLarge,
    /// A character of another alphabet, a length that is not a multiple of four, padding in the wrong place, or
    /// bits that must be zero and are not.
    Invalid,
    /// An address that is not `data:image/png;base64,` followed by something.
    NotAPngAddress,
}

/// `bytes` as base64 text.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8) | u32::from(chunk.get(2).copied().unwrap_or(0));
        out.push(char::from(ALPHABET[(n >> 18) as usize & 63]));
        out.push(char::from(ALPHABET[(n >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 { char::from(ALPHABET[(n >> 6) as usize & 63]) } else { '=' });
        out.push(if chunk.len() > 2 { char::from(ALPHABET[n as usize & 63]) } else { '=' });
    }
    out
}

/// The value of a character of the alphabet.
const fn sextet(b: u8) -> Option<u32> {
    match b {
        b'A'..=b'Z' => Some((b - b'A') as u32),
        b'a'..=b'z' => Some((b - b'a') as u32 + 26),
        b'0'..=b'9' => Some((b - b'0') as u32 + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// The bytes `text` holds, at most `max` of them.
///
/// # Errors
/// [`Base64Error::TooLarge`] for more than `max` bytes (decided from the length alone when it can be, before any
/// byte is read), [`Base64Error::Invalid`] for anything that is not exactly what [`encode`] writes.
pub fn decode(text: &str, max: usize) -> Result<Vec<u8>, Base64Error> {
    let t = text.as_bytes();
    if t.len() % 4 != 0 {
        return Err(Base64Error::Invalid);
    }
    // Two bytes of padding at most are in the last quad, so the length alone says when it cannot fit.
    if t.len() / 4 * 3 > max.saturating_add(2) {
        return Err(Base64Error::TooLarge);
    }
    let padding = t.iter().rev().take(2).take_while(|b| **b == b'=').count();
    let body = &t[..t.len() - padding];
    let size = t.len() / 4 * 3 - padding;
    if size > max {
        return Err(Base64Error::TooLarge);
    }
    let mut out = Vec::with_capacity(size);
    for quad in body.chunks(4) {
        let mut n = 0_u32;
        for b in quad {
            n = (n << 6) | sextet(*b).ok_or(Base64Error::Invalid)?;
        }
        match quad.len() {
            4 => out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]),
            3 => {
                // 18 bits: two bytes, the last two bits unused and zero.
                if n & 0b11 != 0 {
                    return Err(Base64Error::Invalid);
                }
                out.extend_from_slice(&[(n >> 10) as u8, (n >> 2) as u8]);
            }
            2 => {
                // 12 bits: one byte, the last four bits unused and zero.
                if n & 0b1111 != 0 {
                    return Err(Base64Error::Invalid);
                }
                out.push((n >> 4) as u8);
            }
            _ => return Err(Base64Error::Invalid),
        }
    }
    Ok(out)
}

/// The address of a PNG: `data:image/png;base64,` and its bytes.
#[must_use]
pub fn data_uri_png(png: &[u8]) -> String {
    let mut out = String::with_capacity(PNG_PREFIX.len() + png.len().div_ceil(3) * 4);
    out.push_str(PNG_PREFIX);
    out.push_str(&encode(png));
    out
}

/// The bytes of the PNG a `data:image/png;base64,...` address holds, at most `max` of them. What the bytes are is not
/// looked at: the picture decoder says whether they are a PNG.
///
/// # Errors
/// [`Base64Error::NotAPngAddress`] for any other address (or one without bytes), the errors of [`decode`] for the bytes.
pub fn png_from_data_uri(uri: &str, max: usize) -> Result<Vec<u8>, Base64Error> {
    let head = uri.get(..PNG_PREFIX.len()).ok_or(Base64Error::NotAPngAddress)?;
    if !head.eq_ignore_ascii_case(PNG_PREFIX) {
        return Err(Base64Error::NotAPngAddress);
    }
    let bytes = decode(&uri[PNG_PREFIX.len()..], max)?;
    if bytes.is_empty() {
        return Err(Base64Error::NotAPngAddress);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "whiteboard_base64_test.rs"]
mod whiteboard_base64_test;
