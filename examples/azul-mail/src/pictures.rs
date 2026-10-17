//! A mail's pictures: what "download pictures" fetches from the web (the shown web pictures,
//! http / https only, at most [`MAX_PICTURES`]) and how much ([`Budget`]: per picture and per
//! mail), and which of the mail's own `cid:` pictures are decoded under which image-cache key.
//! The fetching and decoding run on azul Threads (`ui_main.rs`); this module is the policy.

use crate::{html::Sanitized, message::InlinePicture};

/// At most this many web pictures are fetched for one mail (in the order it shows them).
pub const MAX_PICTURES: usize = 40;
/// A web picture larger than this is not downloaded (the HTTP client stops there).
pub const MAX_PICTURE_BYTES: usize = 5 * 1024 * 1024;
/// One mail's pictures together download at most this much.
pub const MAX_TOTAL_BYTES: usize = 25 * 1024 * 1024;
/// A web picture's download may take this long, in seconds.
pub const PICTURE_TIMEOUT_SECS: u64 = 20;

/// What "download pictures" fetches for `sanitized` (sanitized with its web pictures on):
/// the web pictures it shows - http and https only, no tracking pixel, each once -, in the
/// order it shows them, at most [`MAX_PICTURES`].
pub fn fetch_list(sanitized: &Sanitized) -> Vec<String> {
    sanitized
        .remote_images
        .iter()
        .filter(|url| {
            let lower = url.to_ascii_lowercase();
            lower.starts_with("https://") || lower.starts_with("http://")
        })
        .take(MAX_PICTURES)
        .cloned()
        .collect()
}

/// How much one mail's pictures have downloaded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budget {
    used: usize,
}

impl Budget {
    /// Counts a picture of `len` bytes, or refuses it (costing nothing): larger than
    /// [`MAX_PICTURE_BYTES`], or past the mail's [`MAX_TOTAL_BYTES`].
    pub fn take(&mut self, len: usize) -> bool {
        if len > MAX_PICTURE_BYTES || self.used.saturating_add(len) > MAX_TOTAL_BYTES {
            return false;
        }
        self.used += len;
        true
    }

    /// The bytes counted so far.
    pub fn used(&self) -> usize {
        self.used
    }

    /// Whether a further picture of `len` bytes would pass the mail's total: the downloads
    /// stop.
    pub fn spent_for(&self, len: usize) -> bool {
        self.used.saturating_add(len) > MAX_TOTAL_BYTES
    }
}

/// The Content-IDs of the mail's own pictures, for `html::PictureOptions::inline`.
pub fn content_ids(parts: &[InlinePicture]) -> Vec<String> {
    parts.iter().map(|part| part.content_id.clone()).collect()
}

/// The mail's own pictures `sanitized` shows, each with its image-cache key
/// (`Sanitized::inline_key`) and its bytes to decode; the parts it does not show are not
/// decoded at all.
pub fn inline_decodes(sanitized: &Sanitized, parts: &[InlinePicture]) -> Vec<(String, Vec<u8>)> {
    sanitized
        .inline_images
        .iter()
        .filter_map(|cid| {
            let part = parts.iter().find(|part| part.content_id == *cid)?;
            Some((sanitized.inline_key(cid), part.bytes.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        html::{self, PictureOptions},
        message::InlinePicture,
    };

    #[test]
    fn a_mail_fetches_its_shown_web_pictures_in_order_up_to_the_count_cap() {
        let mut mail = String::new();
        for i in 0..MAX_PICTURES + 5 {
            mail.push_str(&format!(
                "<img src=\"https://cdn.example/{i}.png\" alt=\"{i}\">"
            ));
        }
        let list = fetch_list(&html::sanitize_with(&mail, true));
        assert_eq!(list.len(), MAX_PICTURES);
        assert_eq!(list[0], "https://cdn.example/0.png");
        assert_eq!(list[1], "https://cdn.example/1.png");
        // Before the reader asks: nothing.
        assert!(fetch_list(&html::sanitize(&mail)).is_empty());
    }

    #[test]
    fn the_budget_refuses_a_picture_too_big_and_a_mail_past_its_total() {
        let mut budget = Budget::default();
        assert!(budget.take(1000));
        assert_eq!(budget.used(), 1000);
        assert!(!budget.take(MAX_PICTURE_BYTES + 1), "one picture too big");
        assert_eq!(budget.used(), 1000, "a refused picture costs nothing");
        let mut taken = 0;
        while budget.take(MAX_PICTURE_BYTES) {
            taken += 1;
        }
        assert!(taken > 0);
        assert!(budget.used() <= MAX_TOTAL_BYTES);
        assert!(budget.used() + MAX_PICTURE_BYTES > MAX_TOTAL_BYTES);
        assert!(budget.spent_for(MAX_PICTURE_BYTES));
        assert!(!Budget::default().spent_for(MAX_PICTURE_BYTES));
    }

    #[test]
    fn the_shown_cid_pictures_are_decoded_under_their_keys_and_the_others_not_at_all() {
        let parts = vec![
            InlinePicture {
                content_id: String::from("logo@example"),
                mime_type: String::from("image/png"),
                bytes: vec![1, 2, 3],
            },
            InlinePicture {
                content_id: String::from("unused@example"),
                mime_type: String::from("image/gif"),
                bytes: vec![4, 5],
            },
        ];
        assert_eq!(
            content_ids(&parts),
            vec![String::from("logo@example"), String::from("unused@example")]
        );
        let shown = html::sanitize_mail(
            "<p><img src=\"cid:logo@example\" alt=\"Logo\"></p>",
            &PictureOptions {
                web: false,
                inline: content_ids(&parts),
            },
        );
        assert_eq!(
            inline_decodes(&shown, &parts),
            vec![(shown.inline_key("logo@example"), vec![1, 2, 3])]
        );
        assert!(inline_decodes(&html::sanitize("<p>none</p>"), &parts).is_empty());
    }
}
