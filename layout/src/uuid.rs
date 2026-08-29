//! UUID string generation - the intended mint for MARKER strings.
//!
//! Markers (`Dom::with_marker` + `CallbackInfo::get_node_id_by_marker`) are
//! app-chosen strings created at `layout()` time; what makes them work is that
//! they collide with *nothing* - not another widget, not another window, not a
//! hard-coded string in some library. A UUID gives that guarantee without any
//! coordination, so the pattern is:
//!
//! ```rust,ignore
//! // once, at layout() time (store it wherever the driving callback looks):
//! let marker = Uuid::short();
//! let bar = ProgressBar::create(0.0).dom().with_marker(Some(marker.clone()).into());
//! ```
//!
//! [`Uuid::v4`] returns the canonical hyphenated form (36 chars);
//! [`Uuid::short`] returns the same 128 bits as a 22-char flickrBase58 string
//! (the `short-uuid` encoding) - shorter, no hyphens, URL/log friendly.

use azul_css::AzString;

/// Static-method namespace for UUID string generation ([`Uuid::v4`],
/// [`Uuid::short`]). The struct only exists so the FFI layer can hang static
/// methods off it.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
#[allow(clippy::pub_underscore_fields)] // FFI/api.json static-namespace placeholder field
pub struct Uuid {
    pub _reserved: u8,
}

impl Default for Uuid {
    fn default() -> Self {
        Self::new()
    }
}

impl Uuid {
    /// Returns a zero-initialised handle. The struct only exists so the FFI
    /// layer can hang static methods off it.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// A fresh random (version 4) UUID in canonical hyphenated lowercase form,
    /// e.g. `"550e8400-e29b-41d4-a716-446655440000"` (36 characters).
    #[must_use]
    pub fn v4() -> AzString {
        ::uuid::Uuid::new_v4().to_string().into()
    }

    /// A fresh random (version 4) UUID as a 22-character flickrBase58 string,
    /// e.g. `"mhvXdrZT4jP5T8vBxuvm75"` - the `short-uuid` encoding of the same
    /// 128 bits `v4` would print. The compact spelling for markers, log lines
    /// and URLs.
    #[must_use]
    pub fn short() -> AzString {
        short_uuid::ShortUuid::generate().to_string().into()
    }
}

#[cfg(test)]
mod uuid_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn v4_is_canonical_hyphenated_lowercase() {
        for _ in 0..64 {
            let s = Uuid::v4();
            let s = s.as_str();
            assert_eq!(s.len(), 36, "canonical UUID is 36 chars: {s}");
            for (i, c) in s.char_indices() {
                if matches!(i, 8 | 13 | 18 | 23) {
                    assert_eq!(c, '-', "hyphen expected at {i}: {s}");
                } else {
                    assert!(
                        c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
                        "lowercase hex expected at {i}: {s}",
                    );
                }
            }
            assert_eq!(&s[14..15], "4", "version nibble must say v4: {s}");
        }
    }

    #[test]
    fn short_is_22_chars_of_flickr_base58() {
        // flickrBase58: no 0 / O / I / l - the look-alike characters.
        for _ in 0..64 {
            let s = Uuid::short();
            let s = s.as_str();
            assert_eq!(s.len(), 22, "short UUID is 22 chars: {s}");
            for c in s.chars() {
                assert!(c.is_ascii_alphanumeric(), "base58 is alphanumeric: {s}");
                assert!(
                    !matches!(c, '0' | 'O' | 'I' | 'l'),
                    "flickrBase58 excludes look-alikes, got {c:?} in {s}",
                );
            }
        }
    }

    #[test]
    fn minted_ids_do_not_collide() {
        // The whole point of the module: 10k mints, 10k distinct strings.
        let mut seen = HashSet::new();
        for _ in 0..5_000 {
            assert!(seen.insert(Uuid::v4().as_str().to_string()));
            assert!(seen.insert(Uuid::short().as_str().to_string()));
        }
    }

    #[test]
    fn the_namespace_handle_is_inert() {
        assert_eq!(Uuid::new(), Uuid::default());
        assert_eq!(Uuid::new()._reserved, 0);
    }
}
