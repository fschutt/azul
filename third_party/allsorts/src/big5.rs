//! Big5 encoding.

// Note that we need to recreate Encoder/Decoders in these functions due to this note in the
// encoding_rs documentation:
//
// > Once the stream has ended, the Decoder object must not be used anymore. That is, you need to
// > create another one to process another stream.

use encoding_rs::{DecoderResult, EncoderResult, BIG5};

pub fn unicode_to_big5(u: char) -> Option<u16> {
    let mut encoder = BIG5.new_encoder();
    let src: &mut [u8] = &mut [0, 0, 0, 0];
    let mut dst = [0, 0];
    let (res, _read, written) =
        encoder.encode_from_utf8_without_replacement(u.encode_utf8(src), &mut dst, true);
    match res {
        EncoderResult::InputEmpty => {
            match written {
                1 => Some(u16::from(dst[0])),
                2 => Some(u16::from_be_bytes(dst)),
                _ => None, // should not happen
            }
        }
        EncoderResult::OutputFull => None, // should not happen
        EncoderResult::Unmappable(_) => None,
    }
}

/// Decodes a Big5 character into a Unicode char
pub fn big5_to_unicode(ch: u16) -> Option<char> {
    // Strictly speaking, the Big5 encoding contains only double-byte character set characters.
    // However, in practice, the Big5 codes are always used together with an unspecified,
    // system-dependent single-byte character set (ASCII, or an 8-bit character set such as code
    // page 437), so that you will find a mix of DBCS characters and single-byte characters in
    // Big5-encoded text. Bytes in the range 0x00 to 0x7f that are not part of a double-byte
    // character are assumed to be single-byte characters.
    //
    // — https://en.wikipedia.org/wiki/Big5
    //
    // encoding_rs::Decoder for Big5 returns 0 for single-byte codes, so we handle ASCII manually.
    if ch < 128 {
        return Some(ch as u8 as char);
    }

    let mut decoder = BIG5.new_decoder_without_bom_handling();
    let src = ch.to_be_bytes();
    let mut dst = [0, 0, 0, 0];
    let (res, _read, written) = decoder.decode_to_utf8_without_replacement(&src, &mut dst, true);
    match res {
        DecoderResult::InputEmpty if written > 0 => {
            // Safety: It is assumed that decode_to_utf8_without_replacement will yield valid utf-8
            // output in dst. Additionally because written is > 1 we assume chars will yield a char
            // thus it should always be valid utf-8.
            let s = unsafe { std::str::from_utf8_unchecked(&dst[..written]) }
                .chars()
                .next()
                .unwrap();
            Some(s)
        }
        DecoderResult::InputEmpty |
        DecoderResult::OutputFull | // should not happen
        DecoderResult::Malformed(_, _) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_to_big5() {
        for i in 0..128 {
            let u = char::from(i);
            assert_eq!(unicode_to_big5(u), Some(u16::from(i)));
        }
    }

    #[test]
    fn chinese_to_big5() {
        assert_eq!(unicode_to_big5('好'), Some(0xA66E));
    }

    #[test]
    fn greek_to_big5() {
        assert_eq!(unicode_to_big5('ε'), Some(0xA360));
    }

    #[test]
    fn hindi_to_big5() {
        assert_eq!(unicode_to_big5('म'), None);
    }

    #[test]
    fn big5_to_ascii() {
        for i in 0..128 {
            assert_eq!(big5_to_unicode(u16::from(i)), Some(char::from(i)));
        }
    }

    #[test]
    fn chinese_big5_to_unicode() {
        assert_eq!(big5_to_unicode(0xA66E), Some('好'));
    }

    #[test]
    fn greek_big5_to_unicode() {
        assert_eq!(big5_to_unicode(0xA360), Some('ε'));
    }

    #[test]
    fn graphical_big5_to_unicode() {
        assert_eq!(big5_to_unicode(0xA143), Some('。'));
    }
}
