//! A mail's pictures: what "download pictures" fetches from the web (the shown web pictures,
//! http / https only, at most [`MAX_PICTURES`]) and how much ([`Budget`]: per picture and per
//! mail), and which of the mail's own `cid:` pictures are decoded under which image-cache key.
//! The fetching and decoding run on azul Threads (`ui_main.rs`); this module is the policy.

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
