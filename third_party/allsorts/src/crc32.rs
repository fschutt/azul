//! Minimal CRC-32 (IEEE 802.3 / gzip / PNG polynomial).
//!
//! Used by the variable-font PostScript-name generator to produce the
//! "last resort" hashed suffix described in the OpenType specification's
//! *Generating PostScript names for OpenType Font Variations* section,
//! which calls for the IEEE 802.3 CRC-32. This is not on a hot path —
//! it only runs when a generated PostScript name exceeds 63 characters —
//! so a simple bitwise implementation is preferred over pulling in a
//! dedicated CRC-32 crate.

const POLY: u32 = 0xEDB88320;

pub fn hash(bytes: &[u8]) -> u32 {
    let mut crc: u32 = !0;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (POLY & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::hash;

    // Reference values from the standard IEEE 802.3 CRC-32 (gzip / PNG).
    #[test]
    fn empty() {
        assert_eq!(hash(b""), 0);
    }

    #[test]
    fn ascii() {
        // "123456789" has the well-known check value 0xCBF43926.
        assert_eq!(hash(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn short() {
        assert_eq!(hash(b"a"), 0xE8B7BE43);
    }
}
