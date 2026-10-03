//! `data:` URIs and the base64 in them: a contact's photo is one (`data:image/jpeg;base64,...`,
//! vCard 4.0's PHOTO; vCard 3.0 carries the same base64 with `ENCODING=b`), and so is an
//! inline image of a mail. AzContacts encoded them alone (its `ui::base64`); showing the photo
//! needs them decoded too. Standard base64 (RFC 4648 section 4), padding written; read with or
//! without padding, blanks and line breaks (a folded vCard line) skipped.

/// `bytes` in standard base64, padded.
#[must_use]
pub fn encode_base64(bytes: &[u8]) -> String {
    let _ = bytes;
    todo!()
}

/// The bytes of standard base64 `text` (padding optional, whitespace skipped); `None` for a
/// character base64 does not have, or a length no base64 has.
#[must_use]
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let _ = text;
    todo!()
}

/// `data:<mime>;base64,<bytes>`.
#[must_use]
pub fn data_uri(mime: &str, bytes: &[u8]) -> String {
    let _ = (mime, bytes);
    todo!()
}

/// The type and the bytes of a `data:` URI (`data:image/png;base64,...`, or percent-free
/// plain text `data:text/plain,hello`); `None` for anything else.
#[must_use]
pub fn parse_data_uri(uri: &str) -> Option<(String, Vec<u8>)> {
    let _ = uri;
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        // RFC 4648 section 10.
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(encode_base64(plain.as_bytes()), coded);
            assert_eq!(decode_base64(coded).as_deref(), Some(plain.as_bytes()));
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(decode_base64(&encode_base64(&all)), Some(all));
        assert_eq!(encode_base64(&[0xfb, 0xff]), "+/8=");
    }

    #[test]
    fn base64_is_read_without_padding_and_across_line_breaks() {
        assert_eq!(decode_base64("Zm9vYg").as_deref(), Some(&b"foob"[..]));
        assert_eq!(decode_base64("Zm9v\r\n YmFy").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(decode_base64("Zm9v!"), None, "not a base64 character");
        assert_eq!(decode_base64("Z"), None, "no base64 is one character long");
        assert_eq!(decode_base64("Zm=9v"), None, "padding inside");
    }

    #[test]
    fn a_data_uri_names_its_type_and_carries_its_bytes() {
        let uri = data_uri("image/png", b"\x89PNG");
        assert_eq!(uri, "data:image/png;base64,iVBORw==");
        assert_eq!(
            parse_data_uri(&uri),
            Some(("image/png".to_string(), b"\x89PNG".to_vec()))
        );
        assert_eq!(
            parse_data_uri("data:text/plain,hello"),
            Some(("text/plain".to_string(), b"hello".to_vec()))
        );
        assert_eq!(
            parse_data_uri("DATA:image/jpeg;BASE64,Zm9v"),
            Some(("image/jpeg".to_string(), b"foo".to_vec()))
        );
        assert_eq!(parse_data_uri("https://example.org/a.png"), None);
        assert_eq!(parse_data_uri("data:image/png;base64,!!"), None);
        assert_eq!(parse_data_uri("data:image/png;base64"), None, "no comma");
    }
}
