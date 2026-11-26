//! Base64 (RFC 4648, the standard alphabet with padding) and the `data:` address of a PNG: the pictures of a board that is
//! exported as one JSON file travel inside it. No window, no azul.

use super::*;

#[test]
fn the_vectors_of_rfc_4648_are_encoded_and_decoded() {
    let vectors = [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")];
    for (plain, coded) in vectors {
        assert_eq!(encode(plain.as_bytes()), coded, "{plain:?}");
        assert_eq!(decode(coded, 1 << 20).as_deref(), Ok(plain.as_bytes()), "{coded:?}");
    }
}

#[test]
fn every_byte_value_goes_round_and_the_alphabet_is_the_standard_one() {
    let all: Vec<u8> = (0..=255).collect();
    let coded = encode(&all);
    assert_eq!(decode(&coded, 1 << 20), Ok(all));
    assert!(coded.starts_with("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0+P0BB"));
    assert_eq!(encode(&[0xfb, 0xff]), "+/8=", "plus and slash, not minus and underscore");
    // Every length, every padding.
    for n in 0..64 {
        let bytes: Vec<u8> = (0..n).map(|i| (i * 7 + 3) as u8).collect();
        assert_eq!(decode(&encode(&bytes), 1 << 20), Ok(bytes), "{n} bytes");
    }
}

#[test]
fn what_is_not_base64_is_refused_and_never_a_panic() {
    for bad in ["Zg", "Zg=", "Z", "Zm9v!", "Zm 9v", "Zm9v\\n", "=Zm9", "Zg==Zg==", "Zh==", "Zm9=", "ZmZ=", "-_-_", "Zm9vYg==x", "é"] {
        assert!(decode(bad, 1 << 20).is_err(), "{bad:?}");
    }
    // Padding that is not needed and bits that are not zero where they must be.
    assert!(decode("Zm9v====", 1 << 20).is_err());
    assert!(decode("Zh==", 1 << 20).is_err(), "the last bits of 'Zg==' must be zero");
}

#[test]
fn a_size_that_would_be_too_large_is_refused_before_anything_is_made() {
    let coded = encode(&vec![7_u8; 3_000]);
    assert_eq!(decode(&coded, 3_000).map(|b| b.len()), Ok(3_000));
    assert_eq!(decode(&coded, 2_999), Err(Base64Error::TooLarge));
    // A huge text is refused by its length alone.
    let huge = "A".repeat(100_000_000 / 4 * 4);
    assert_eq!(decode(&huge, 1_000), Err(Base64Error::TooLarge));
}

#[test]
fn a_png_goes_into_a_data_address_and_comes_out_and_only_a_png_does() {
    let png = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1, 2, 3];
    let uri = data_uri_png(&png);
    assert!(uri.starts_with("data:image/png;base64,"), "{uri}");
    assert_eq!(png_from_data_uri(&uri, 1 << 20), Ok(png.to_vec()));
    for bad in ["", "data:", "data:image/png,abc", "data:image/jpeg;base64,AAAA", "image/png;base64,AAAA", "data:image/png;base64,", "http://example.org/a.png"] {
        assert!(png_from_data_uri(bad, 1 << 20).is_err(), "{bad:?}");
    }
    // What is in it is not looked at: a PNG is a PNG when the decoder says so.
    let not_png = data_uri_png(b"hello");
    assert_eq!(png_from_data_uri(&not_png, 1 << 20), Ok(b"hello".to_vec()));
}
