//! Trusted contacts: a recovery code split 2-of-3, its shares printed or sealed to a contact's
//! key, a recovery request answered by sealing a share to the request's key.

use super::mem_bucket::MemBucket;
use crate::crypto::{
    contacts::{
        contact_from_text, contact_text, open_reply, open_share, read_share, request_from_text,
        request_text, safety_number, seal_reply, seal_share, set_of, share_recipient, CodeShare,
        REPLY_PREFIX, SHARES, SHARE_PREFIX,
    },
    device::{
        contact_key_entry, enroll, forget_request_key, load_contact_key, new_contact_key,
        other_devices, request_key, request_key_entry, seal_invite, setup_new_drive,
    },
    keys::{MemberSecret, RecoveryCode},
    CryptoError,
};
use crate::{
    crypto::keys::RecoveryKdf,
    keyring::{KeyringStore, MemoryKeyring},
};

fn code() -> RecoveryCode {
    RecoveryCode::from_bytes([0x5A; 16])
}

#[test]
fn two_of_the_three_shares_give_the_recovery_code_back() {
    let shares = CodeShare::split(&code()).unwrap();
    assert_eq!(shares.len(), usize::from(SHARES));
    assert_eq!(
        shares.iter().map(CodeShare::index).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    for pair in [[0, 1], [0, 2], [1, 2], [2, 0]] {
        let picked = [shares[pair[0]].clone(), shares[pair[1]].clone()];
        let back = CodeShare::combine(&picked).unwrap();
        assert_eq!(back.as_bytes(), code().as_bytes(), "shares {pair:?}");
    }
    assert_eq!(
        CodeShare::combine(&shares).unwrap().as_bytes(),
        code().as_bytes()
    );
    assert!(matches!(
        CodeShare::combine(&shares[..1]),
        Err(CryptoError::Unsupported(_))
    ));
    assert!(matches!(
        CodeShare::combine(&[shares[0].clone(), shares[0].clone()]),
        Err(CryptoError::Damaged(_))
    ));
    // Every share names its split; two splits of the same code do not mix.
    let set = shares[0].set_hex();
    assert_eq!(set.len(), 8);
    assert!(shares.iter().all(|share| share.set_hex() == set));
    let hex_of_set: String = set_of(&code()).iter().map(|b| format!("{b:02X}")).collect();
    assert_eq!(set, hex_of_set);
    let other = CodeShare::split(&code()).unwrap();
    assert!(matches!(
        CodeShare::combine(&[shares[0].clone(), other[1].clone()]),
        Err(CryptoError::Damaged(_))
    ));
    // Another code's shares are of another split.
    let another = CodeShare::split(&RecoveryCode::from_bytes([0x11; 16])).unwrap();
    assert_ne!(another[0].set_hex(), set);
    assert!(matches!(
        CodeShare::combine(&[shares[0].clone(), another[1].clone()]),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn a_printed_share_reads_back_as_people_type_it() {
    let shares = CodeShare::split(&code()).unwrap();
    let text = shares[1].to_text();
    assert!(text.starts_with("S2-"), "{}", text.as_str());
    let parts: Vec<&str> = text.split('-').collect();
    assert_eq!(
        parts.len(),
        7,
        "S2, the split, five groups: {}",
        text.as_str()
    );
    assert_eq!(parts[1], shares[1].set_hex());
    assert_eq!(CodeShare::parse(&text).unwrap(), shares[1]);
    let typed = text.to_lowercase().replace('-', " ").replace('0', "o");
    assert_eq!(CodeShare::parse(&typed).unwrap(), shares[1], "{typed}");
    let printed: Vec<CodeShare> = [0, 2]
        .iter()
        .map(|&i| CodeShare::parse(&shares[i].to_text()).unwrap())
        .collect();
    assert_eq!(
        CodeShare::combine(&printed).unwrap().as_bytes(),
        code().as_bytes()
    );
    // A typo in the share's groups still parses (any 26 symbols are a share), but the two
    // shares no longer give a code of their split.
    let mut chars: Vec<char> = shares[0].to_text().chars().collect();
    let last = chars.len() - 3;
    chars[last] = if chars[last] == '7' { '8' } else { '7' };
    let mistyped: String = chars.into_iter().collect();
    if let Some(wrong) = CodeShare::parse(&mistyped) {
        assert!(matches!(
            CodeShare::combine(&[wrong, shares[1].clone()]),
            Err(CryptoError::Damaged(_))
        ));
    }
    assert!(CodeShare::parse("S4-00000000-00000-00000-00000-00000-000000").is_none());
    assert!(CodeShare::parse("S1-0000-00000-00000-00000-00000-000000").is_none());
    assert!(CodeShare::parse("hello").is_none());
}

#[test]
fn a_share_sealed_to_a_contact_opens_only_with_that_contacts_key() {
    let shares = CodeShare::split(&code()).unwrap();
    let ada = MemberSecret::generate().unwrap();
    let grace = MemberSecret::generate().unwrap();
    let sealed = seal_share(&shares[0], "Felix, drive Photos", &ada.public()).unwrap();
    assert!(sealed.starts_with(SHARE_PREFIX));
    assert!(
        !sealed.contains(&shares[0].set_hex().to_lowercase()),
        "the split's id is inside"
    );
    assert_eq!(share_recipient(&sealed), Some(ada.public()));
    let (opened, label) = open_share(&sealed, &ada).unwrap();
    assert_eq!(opened, shares[0]);
    assert_eq!(label, "Felix, drive Photos");
    assert_eq!(
        open_share(&sealed, &grace).unwrap_err(),
        CryptoError::WrongKey
    );
    // A changed byte of the ciphertext does not open.
    let mut changed = sealed.clone().into_bytes();
    let at = changed.len() - 10;
    changed[at] = if changed[at] == b'0' { b'1' } else { b'0' };
    let changed = String::from_utf8(changed).unwrap();
    assert!(open_share(&changed, &ada).is_err());
    // Two seals of one share differ (a fresh ephemeral key and nonce).
    assert_ne!(seal_share(&shares[0], "x", &ada.public()).unwrap(), sealed);
    // A reply is no share for a contact.
    let reply = seal_reply(&shares[0], &ada.public()).unwrap();
    let as_share = format!("{SHARE_PREFIX}{}", &reply[REPLY_PREFIX.len()..]);
    assert!(matches!(
        open_share(&as_share, &ada),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn a_contact_answers_a_recovery_request_that_only_the_request_opens() {
    let shares = CodeShare::split(&code()).unwrap();
    let request = MemberSecret::generate().unwrap();
    let text = request_text(&request.public());
    let asked = request_from_text(&format!("  {text}\n")).unwrap();
    assert_eq!(asked, request.public());
    let reply = seal_reply(&shares[2], &asked).unwrap();
    assert!(reply.starts_with(REPLY_PREFIX));
    assert_eq!(open_reply(&reply, &request).unwrap(), shares[2]);
    let stranger = MemberSecret::generate().unwrap();
    assert_eq!(
        open_reply(&reply, &stranger).unwrap_err(),
        CryptoError::WrongKey
    );
    // The recovering computer takes a reply and a printed share alike.
    let from_reply = read_share(&reply, &request).unwrap();
    let from_paper = read_share(&shares[0].to_text(), &request).unwrap();
    let back = CodeShare::combine(&[from_reply, from_paper]).unwrap();
    assert_eq!(back.as_bytes(), code().as_bytes());
    assert!(read_share("neither", &request).is_err());
    // The safety number both sides compare: 12 digits, the same for the same request.
    let number = safety_number(&asked);
    assert_eq!(number.len(), 15, "{number}");
    assert!(number
        .split(' ')
        .all(|group| group.len() == 3 && group.chars().all(|c| c.is_ascii_digit())));
    assert_eq!(number, safety_number(&request.public()));
    assert_ne!(number, safety_number(&stranger.public()));
}

#[test]
fn contact_keys_and_requests_travel_as_text_and_shares_never_print() {
    let ada = MemberSecret::generate().unwrap();
    let text = contact_text(&ada.public());
    assert!(
        text.starts_with("azlin-contact:") && text.len() == 14 + 64,
        "{text}"
    );
    assert_eq!(contact_from_text(&text), Some(ada.public()));
    assert_eq!(
        contact_from_text(&request_text(&ada.public())),
        None,
        "a request"
    );
    assert_eq!(request_from_text(&text), None, "a contact key");
    assert_eq!(contact_from_text("azlin-contact:00"), None);
    let shares = CodeShare::split(&code()).unwrap();
    let shown = format!("{:?}", shares[0]);
    assert!(shown.starts_with("CodeShare(1 of 3"), "{shown}");
    assert!(!shown.contains(&shares[0].to_text()[12..]), "{shown}");
}

#[test]
fn contact_keys_and_a_drives_request_key_live_in_the_keyring() {
    let keyring = MemoryKeyring::new();
    let ada = new_contact_key(&keyring).unwrap();
    let grace = new_contact_key(&keyring).unwrap();
    assert_ne!(ada, grace, "a key for each owner");
    let entry = contact_key_entry(&ada.id());
    assert!(keyring.get(&entry).unwrap().is_some(), "{entry}");
    let secret = load_contact_key(&keyring, &ada).unwrap().unwrap();
    assert_eq!(secret.public(), ada);
    let stranger = MemberSecret::generate().unwrap().public();
    assert!(load_contact_key(&keyring, &stranger).unwrap().is_none());
    // A share sealed to the key opens with what the keyring keeps.
    let shares = CodeShare::split(&code()).unwrap();
    let sealed = seal_share(&shares[0], "Felix", &ada).unwrap();
    let held = share_recipient(&sealed).unwrap();
    let key = load_contact_key(&keyring, &held).unwrap().unwrap();
    assert_eq!(open_share(&sealed, &key).unwrap().0, shares[0]);

    // The drive's request key is made once and kept until the recovery is done.
    let first = request_key(&keyring, "d_1").unwrap();
    let again = request_key(&keyring, "d_1").unwrap();
    assert_eq!(first.public(), again.public());
    assert_ne!(
        request_key(&keyring, "d_2").unwrap().public(),
        first.public()
    );
    assert!(keyring.get(&request_key_entry("d_1")).unwrap().is_some());
    forget_request_key(&keyring, "d_1").unwrap();
    assert!(keyring.get(&request_key_entry("d_1")).unwrap().is_none());
    assert_ne!(
        request_key(&keyring, "d_1").unwrap().public(),
        first.public()
    );
}

#[test]
fn another_device_is_a_member_wrap_beside_this_ones_and_invites_do_not_count() {
    let bucket = MemBucket::new();
    let first = MemoryKeyring::new();
    let (drive_key, _code) = setup_new_drive(
        &bucket,
        &first,
        "d_1",
        RecoveryKdf::with_cost(64, 1, 1).unwrap(),
    )
    .unwrap();
    assert_eq!(
        other_devices(&bucket, &first, "d_1").unwrap(),
        0,
        "only this device"
    );
    let second = MemoryKeyring::new();
    enroll(&bucket, &second, "d_1", &drive_key).unwrap();
    assert_eq!(other_devices(&bucket, &first, "d_1").unwrap(), 1);
    assert_eq!(other_devices(&bucket, &second, "d_1").unwrap(), 1);
    let _invite = seal_invite(&bucket, "d_1", &drive_key).unwrap();
    assert_eq!(
        other_devices(&bucket, &first, "d_1").unwrap(),
        1,
        "an invite is no device"
    );
    let stranger = MemoryKeyring::new();
    assert_eq!(
        other_devices(&bucket, &stranger, "d_1").unwrap(),
        2,
        "a computer without a wrap"
    );
}
