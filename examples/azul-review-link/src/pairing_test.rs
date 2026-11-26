//! The pairing's arithmetic: the text of an offer, the proof of the secret, the six-digit code, the randomness.
//! The known answers were computed with another implementation (Python's hmac / hashlib), so the format is the
//! same on every platform that follows it.

use super::*;

const SECRET: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

fn host_key() -> String {
    "aa".repeat(32)
}

fn phone_key() -> String {
    "bb".repeat(32)
}

fn offer() -> PairingOffer {
    PairingOffer { ticket: "endpointabcdefghij234567".into(), secret: SECRET }
}

// ---- the text of an offer

#[test]
fn an_offer_is_written_as_prefix_secret_check_ticket() {
    assert_eq!(offer().to_text(), "azrv1:000102030405060708090a0b0c0d0e0f:3c32660a:endpointabcdefghij234567");
}

#[test]
fn an_offer_reads_back_what_was_written_and_ignores_the_white_space_of_a_paste() {
    assert_eq!(PairingOffer::parse(&offer().to_text()), Ok(offer()));
    assert_eq!(PairingOffer::parse(&format!("  {}\r\n", offer().to_text())), Ok(offer()));
    assert_eq!(PairingOffer::parse(&format!("\n\t{}\t", offer().to_text())), Ok(offer()));
}

#[test]
fn a_text_that_is_no_offer_says_which_way() {
    let good = offer().to_text();
    assert_eq!(PairingOffer::parse(""), Err(OfferError::NotAnOffer));
    assert_eq!(PairingOffer::parse("hello"), Err(OfferError::NotAnOffer));
    assert_eq!(PairingOffer::parse("https://example.com/azrv1:x"), Err(OfferError::NotAnOffer));
    assert_eq!(PairingOffer::parse(&good.replacen("azrv1", "azrv2", 1)), Err(OfferError::WrongVersion));
    assert_eq!(PairingOffer::parse(&good.replacen("000102", "0001", 1)), Err(OfferError::BadSecret));
    assert_eq!(PairingOffer::parse(&good.replacen("000102", "00010x", 1)), Err(OfferError::BadSecret));
    assert_eq!(PairingOffer::parse(&good.replacen("3c32660a", "3c32660b", 1)), Err(OfferError::BadCheck));
    assert_eq!(PairingOffer::parse(&good.replacen("endpointabc", "endpointabd", 1)), Err(OfferError::BadCheck));
    assert_eq!(PairingOffer::parse("azrv1:000102030405060708090a0b0c0d0e0f:3c32660a:"), Err(OfferError::BadTicket));
    assert_eq!(PairingOffer::parse("azrv1:000102030405060708090a0b0c0d0e0f:3c32660a"), Err(OfferError::NotAnOffer));
}

#[test]
fn a_ticket_with_characters_a_ticket_does_not_have_is_refused() {
    for bad in ["has space", "tab\there", "new\nline", "nul\0byte", "ünï", "semi;colon", "slash/slash"] {
        let text = PairingOffer { ticket: bad.into(), secret: SECRET }.to_text();
        assert_eq!(PairingOffer::parse(&text), Err(OfferError::BadTicket), "{bad:?}");
    }
}

#[test]
fn an_offer_longer_than_any_offer_is_refused_before_it_is_read() {
    let long = PairingOffer { ticket: "a".repeat(MAX_TICKET_BYTES + 1), secret: SECRET }.to_text();
    assert_eq!(PairingOffer::parse(&long), Err(OfferError::TooLong));
    let biggest = PairingOffer { ticket: "a".repeat(MAX_TICKET_BYTES), secret: SECRET };
    assert_eq!(PairingOffer::parse(&biggest.to_text()), Ok(biggest));
    assert_eq!(PairingOffer::parse(&"z".repeat(1_000_000)), Err(OfferError::TooLong));
}

#[test]
fn garbage_never_panics() {
    let mut rng = SeededEntropy(0x1234_5678_9ABC_DEF0);
    for len in 0..400 {
        let mut bytes = vec![0u8; len];
        rng.fill(&mut bytes);
        let text = String::from_utf8_lossy(&bytes).to_string();
        let _ = PairingOffer::parse(&text);
        let _ = PairingOffer::parse(&format!("azrv1:{text}"));
        let _ = PairingOffer::parse(&format!("azrv1:000102030405060708090a0b0c0d0e0f:{text}"));
    }
}

#[test]
fn every_error_says_in_words_what_to_do() {
    for error in [
        OfferError::NotAnOffer,
        OfferError::WrongVersion,
        OfferError::BadSecret,
        OfferError::BadCheck,
        OfferError::BadTicket,
        OfferError::TooLong,
    ] {
        assert!(error.to_string().len() > 20, "{error:?}");
    }
    assert!(OfferError::WrongVersion.to_string().contains("update"));
}

// ---- the proof

#[test]
fn the_proof_is_the_hmac_of_the_domain_and_both_keys() {
    let hex: String = proof(&SECRET, &host_key(), &phone_key()).iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, "8e13e51fbb3ecb01b861485a88920aa3852fbddcf9e294b0be20965fbc086fc2");
}

#[test]
fn the_proof_is_good_for_this_secret_this_mac_and_this_phone_only() {
    let good = proof(&SECRET, &host_key(), &phone_key());
    assert!(verify_proof(&SECRET, &host_key(), &phone_key(), &good));
    let mut other_secret = SECRET;
    other_secret[15] ^= 1;
    assert!(!verify_proof(&other_secret, &host_key(), &phone_key(), &good));
    assert!(!verify_proof(&SECRET, &"ab".repeat(32), &phone_key(), &good), "another Mac");
    assert!(!verify_proof(&SECRET, &host_key(), &"bc".repeat(32), &good), "another phone replaying it");
    assert!(!verify_proof(&SECRET, &phone_key(), &host_key(), &good), "the keys swapped");
}

#[test]
fn a_proof_with_any_one_byte_changed_is_refused() {
    let good = proof(&SECRET, &host_key(), &phone_key());
    for at in 0..32 {
        for bit in 0..8 {
            let mut hurt = good;
            hurt[at] ^= 1 << bit;
            assert!(!verify_proof(&SECRET, &host_key(), &phone_key(), &hurt), "byte {at} bit {bit}");
        }
    }
    assert!(!verify_proof(&SECRET, &host_key(), &phone_key(), &[0u8; 32]));
}

#[test]
fn a_key_that_runs_into_the_next_does_not_make_another_proof_valid() {
    // the keys are separated, so moving a character from one to the other changes the proof
    let a = proof(&SECRET, "ab", "cd");
    let b = proof(&SECRET, "abc", "d");
    let c = proof(&SECRET, "a", "bcd");
    assert!(a != b && b != c && a != c);
}

// ---- the code

#[test]
fn the_code_is_the_first_four_bytes_of_a_hash_of_the_secret_and_both_keys_in_six_digits() {
    assert_eq!(confirmation_code(&SECRET, &host_key(), &phone_key()), 656_503);
}

#[test]
fn both_sides_work_out_the_same_code_and_another_secret_or_key_makes_another() {
    let code = confirmation_code(&SECRET, &host_key(), &phone_key());
    assert_eq!(code, confirmation_code(&SECRET, &host_key(), &phone_key()));
    let mut other = SECRET;
    other[0] ^= 0x80;
    assert_ne!(code, confirmation_code(&other, &host_key(), &phone_key()));
    assert_ne!(code, confirmation_code(&SECRET, &"ab".repeat(32), &phone_key()));
    assert_ne!(code, confirmation_code(&SECRET, &host_key(), &"bc".repeat(32)));
    assert_ne!(code, confirmation_code(&SECRET, &phone_key(), &host_key()));
}

#[test]
fn codes_are_six_digits_and_spread_over_the_whole_range() {
    let mut rng = SeededEntropy(7);
    let mut seen = std::collections::BTreeSet::new();
    let mut highest = 0;
    for _ in 0..2_000 {
        let secret = new_secret(&mut rng);
        let code = confirmation_code(&secret, &host_key(), &phone_key());
        assert!(code <= 999_999, "{code}");
        highest = highest.max(code);
        seen.insert(code);
    }
    assert!(seen.len() >= 1_990, "{} different codes of 2000", seen.len());
    assert!(highest > 900_000, "{highest}");
}

#[test]
fn a_code_is_shown_in_two_groups_of_three_with_its_leading_zeros() {
    assert_eq!(format_code(482_913), "482 913");
    assert_eq!(format_code(7), "000 007");
    assert_eq!(format_code(0), "000 000");
    assert_eq!(format_code(999_999), "999 999");
    assert_eq!(format_code(1_234_567), "234 567", "a number over six digits cannot make a longer code");
}

// ---- the randomness

#[test]
fn the_systems_randomness_gives_different_bytes_each_time() {
    let mut os = OsEntropy;
    let mut first = [0u8; 32];
    let mut second = [0u8; 32];
    os.fill(&mut first);
    os.fill(&mut second);
    assert_ne!(first, [0u8; 32]);
    assert_ne!(first, second);
    assert_ne!(new_secret(&mut os), new_secret(&mut os));
}

#[test]
fn the_seeded_randomness_repeats_for_a_seed_and_differs_between_seeds() {
    let mut a = SeededEntropy(1);
    let mut b = SeededEntropy(1);
    let mut c = SeededEntropy(2);
    let (mut x, mut y, mut z) = ([0u8; 24], [0u8; 24], [0u8; 24]);
    a.fill(&mut x);
    b.fill(&mut y);
    c.fill(&mut z);
    assert_eq!(x, y);
    assert_ne!(x, z);
    assert_ne!(&x[..8], &x[8..16], "it does not repeat within a call");
    let mut zero = SeededEntropy(0);
    let mut w = [0u8; 8];
    zero.fill(&mut w);
    assert_ne!(w, [0u8; 8], "a zero seed still gives bytes");
}

#[test]
fn a_secret_is_sixteen_bytes() {
    assert_eq!(new_secret(&mut SeededEntropy(3)).len(), PAIRING_SECRET_BYTES);
    assert_ne!(new_secret(&mut SeededEntropy(3)), new_secret(&mut SeededEntropy(4)));
}

#[test]
fn a_ticket_is_letters_digits_dashes_and_underscores_and_not_longer_than_any_ticket() {
    assert!(is_valid_ticket("mt-ab12-1"));
    assert!(is_valid_ticket("Ticket_with-Mixed_Case09"));
    assert!(!is_valid_ticket(""), "an empty ticket is no ticket");
    assert!(!is_valid_ticket("has space"));
    assert!(!is_valid_ticket("new\nline"));
    assert!(!is_valid_ticket("colon:inside"), "the colon is the offer's separator");
    assert!(!is_valid_ticket("ünïcode"));
    assert!(is_valid_ticket(&"a".repeat(MAX_TICKET_BYTES)));
    assert!(!is_valid_ticket(&"a".repeat(MAX_TICKET_BYTES + 1)));
}
