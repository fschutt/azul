//! A contact's photo as the window shows it: the picture of a `data:` URI (vCard 4.0's PHOTO,
//! or 3.0's inline base64 the reader turned into one), decoded once and kept by its key; a
//! link or a picture azul cannot decode shows the initials.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

/// The bytes of a photo azul can decode: a `data:image/...` URI (not SVG, which azul's image
/// decoder does not read); `None` for a link, another type, or broken base64.
#[must_use]
pub fn image_bytes(photo: &str) -> Option<Vec<u8>> {
    let (mime, bytes) = azul_pim::data_uri::parse_data_uri(photo)?;
    (mime.starts_with("image/") && mime != "image/svg+xml" && !bytes.is_empty()).then_some(bytes)
}

/// The key a decoded photo is kept by (the same photo text, the same key).
#[must_use]
pub fn key(photo: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    photo.hash(&mut hasher);
    hasher.finish()
}

/// The decoded photos the window keeps at most (a list scrolled through many cards does not
/// keep every picture).
pub const KEPT: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_decodable_picture_in_the_card_is_shown() {
        assert_eq!(
            image_bytes("data:image/png;base64,iVBORw=="),
            Some(b"\x89PNG".to_vec())
        );
        assert_eq!(image_bytes("data:image/jpeg;base64,Zm9v"), Some(b"foo".to_vec()));
        // The sample's SVG, a link, text, broken base64, nothing.
        assert_eq!(image_bytes("data:image/svg+xml;base64,PHN2Zy8+"), None);
        assert_eq!(image_bytes("https://example.org/ana.jpg"), None);
        assert_eq!(image_bytes("data:text/plain,hello"), None);
        assert_eq!(image_bytes("data:image/png;base64,!!"), None);
        assert_eq!(image_bytes(""), None);
    }

    #[test]
    fn the_same_photo_has_the_same_key() {
        let a = "data:image/png;base64,iVBORw==";
        assert_eq!(key(a), key(&a.to_string()));
        assert_ne!(key(a), key("data:image/png;base64,iVBORx=="));
    }
}
