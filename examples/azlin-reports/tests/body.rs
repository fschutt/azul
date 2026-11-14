//! Checks R11, R12, R13, R14 and R22 of lane W1-S46a: the report body (planning/architecture.md section 3.9).
//! Every test enters through the public API of the crate.

mod common;

use std::collections::HashSet;

use azlin_cbor::{DecodeError, ShapeError, Value};
use azlin_reports::{
    BodyError, Extras, ExtrasError, Field, Reason, RecordVerifier, ReportBody, ReportId,
    ReportSealer, SealError, SignedRecord, Subject, MAX_BODY, MAX_RECORD,
};

use common::*;

// ---------------------------------------------------------------- R11

#[test]
fn r11_decode_of_encode_is_the_body_and_the_bytes_are_canonical() {
    for seed in 0..2_000u64 {
        let mut rng = Rng::new(seed);
        let body = random_body(&mut rng);
        let bytes = body
            .encode()
            .unwrap_or_else(|e| panic!("seed {seed}: encode refused a valid body: {e}"));
        let back = ReportBody::decode(&bytes)
            .unwrap_or_else(|e| panic!("seed {seed}: decode refused what encode wrote: {e}"));
        assert_eq!(back, body, "seed {seed}: decode(encode(x)) != x");
        assert_eq!(
            back.encode().unwrap(),
            bytes,
            "seed {seed}: re-encoding a decoded body changed the bytes"
        );
        assert_eq!(
            body.encode().unwrap(),
            bytes,
            "seed {seed}: encode is not deterministic"
        );
    }
}

/// The pairs of a small valid body (a link, phishing), to be tampered with.
fn base() -> Vec<(u64, Value)> {
    vec![
        (1, Value::Uint(1)),
        (2, Value::Bytes(vec![1; 16])),
        (3, Value::Uint(5)),
        (6, link_subject("https://drive.azlin.io/l/a#k")),
    ]
}

fn link_subject(url: &str) -> Value {
    raw_map(vec![(1, Value::Uint(1)), (2, Value::Text(url.to_owned()))])
}

fn set(pairs: &mut Vec<(u64, Value)>, key: u64, value: Value) {
    pairs.retain(|(k, _)| *k != key);
    pairs.push((key, value));
}

fn del(pairs: &mut Vec<(u64, Value)>, key: u64) {
    pairs.retain(|(k, _)| *k != key);
}

fn bytes_of(pairs: &[(u64, Value)]) -> Vec<u8> {
    cbor(&raw_map(pairs.to_vec()))
}

fn record_value(record: Vec<u8>) -> Value {
    raw_map(vec![
        (1, Value::Bytes(record)),
        (2, Value::Bytes(vec![3; 32])),
        (3, Value::Bytes(vec![4; 64])),
    ])
}

fn chat_subject(records: Vec<Value>) -> Value {
    raw_map(vec![(1, Value::Uint(2)), (2, Value::Array(records))])
}

#[test]
fn r11_at_least_sixteen_malformed_bodies_each_fail_with_their_own_error() {
    // The control: the base decodes.
    assert!(ReportBody::decode(&bytes_of(&base())).is_ok());

    let mut cases: Vec<(&str, Vec<u8>, BodyError)> = Vec::new();
    let mut tamper = |name: &'static str, expected: BodyError, f: &dyn Fn(&mut Pairs)| {
        let mut pairs = base();
        f(&mut pairs);
        cases.push((name, bytes_of(&pairs), expected));
    };

    tamper("a wrong version", BodyError::Version(2), &|p| {
        set(p, 1, Value::Uint(2))
    });
    tamper(
        "the version has the wrong type",
        BodyError::Shape(ShapeError::WrongType(1)),
        &|p| set(p, 1, Value::Text("1".into())),
    );
    tamper(
        "a missing version",
        BodyError::Shape(ShapeError::Missing(1)),
        &|p| del(p, 1),
    );
    tamper(
        "an unknown reason code",
        BodyError::UnknownReason(9),
        &|p| set(p, 3, Value::Uint(9)),
    );
    tamper("reason code 0", BodyError::UnknownReason(0), &|p| {
        set(p, 3, Value::Uint(0))
    });
    tamper(
        "text over 2,000 bytes",
        BodyError::TooLong(Field::Text),
        &|p| set(p, 4, Value::Text("a".repeat(2_001))),
    );
    tamper(
        "a missing id",
        BodyError::Shape(ShapeError::Missing(2)),
        &|p| del(p, 2),
    );
    tamper(
        "an id of 15 bytes",
        BodyError::Shape(ShapeError::WrongLength(2)),
        &|p| set(p, 2, Value::Bytes(vec![1; 15])),
    );
    tamper(
        "text of the wrong type",
        BodyError::Shape(ShapeError::WrongType(4)),
        &|p| set(p, 4, Value::Uint(5)),
    );
    tamper(
        "a control character in the text",
        BodyError::ControlChar(Field::Text),
        &|p| set(p, 4, Value::Text("a\u{7}b".into())),
    );
    tamper("an empty contact", BodyError::Empty(Field::Contact), &|p| {
        set(p, 5, Value::Text(String::new()))
    });
    tamper(
        "a contact over 200 bytes",
        BodyError::TooLong(Field::Contact),
        &|p| set(p, 5, Value::Text("c".repeat(201))),
    );
    tamper("an empty reply", BodyError::Empty(Field::Reply), &|p| {
        set(p, 7, Value::Bytes(vec![]))
    });
    tamper(
        "a reply over 4,096 bytes",
        BodyError::TooLong(Field::Reply),
        &|p| set(p, 7, Value::Bytes(vec![1; 4_097])),
    );
    tamper(
        "an unknown subject kind",
        BodyError::UnknownSubject(9),
        &|p| set(p, 6, raw_map(vec![(1, Value::Uint(9))])),
    );
    tamper(
        "a link url without a fragment",
        BodyError::UrlNoFragment,
        &|p| set(p, 6, link_subject("https://drive.azlin.io/l/abc")),
    );
    tamper(
        "a link url with a space",
        BodyError::ControlChar(Field::Url),
        &|p| set(p, 6, link_subject("https://drive.azlin.io/l/abc#k ey")),
    );
    tamper(
        "a link url over 2,048 bytes",
        BodyError::TooLong(Field::Url),
        &|p| {
            set(
                p,
                6,
                link_subject(&format!("https://x/#{}", "k".repeat(2_048))),
            )
        },
    );
    tamper("21 chat records", BodyError::TooManyRecords, &|p| {
        set(
            p,
            6,
            chat_subject((0..21).map(|_| record_value(vec![1, 2])).collect()),
        )
    });
    tamper(
        "a chat report without a record",
        BodyError::NoRecords,
        &|p| set(p, 6, chat_subject(vec![])),
    );
    tamper("an empty chat record", BodyError::EmptyRecord(0), &|p| {
        set(p, 6, chat_subject(vec![record_value(vec![])]))
    });
    tamper(
        "a chat record over 64 KiB",
        BodyError::RecordTooLarge(1),
        &|p| {
            set(
                p,
                6,
                chat_subject(vec![
                    record_value(vec![1]),
                    record_value(vec![7; MAX_RECORD + 1]),
                ]),
            )
        },
    );
    tamper(
        "a chat record without its signature",
        BodyError::RecordShape {
            index: 0,
            why: ShapeError::Missing(3),
        },
        &|p| {
            let record = raw_map(vec![
                (1, Value::Bytes(vec![1])),
                (2, Value::Bytes(vec![3; 32])),
            ]);
            set(p, 6, chat_subject(vec![record]))
        },
    );
    tamper(
        "a chat record with a short author key",
        BodyError::RecordShape {
            index: 0,
            why: ShapeError::WrongLength(2),
        },
        &|p| {
            let record = raw_map(vec![
                (1, Value::Bytes(vec![1])),
                (2, Value::Bytes(vec![3; 31])),
                (3, Value::Bytes(vec![4; 64])),
            ]);
            set(p, 6, chat_subject(vec![record]))
        },
    );
    tamper(
        "a file with no link and an empty note",
        BodyError::Empty(Field::Note),
        &|p| {
            set(
                p,
                6,
                raw_map(vec![(1, Value::Uint(4)), (3, Value::Text(String::new()))]),
            )
        },
    );
    tamper(
        "an unknown key in crit",
        BodyError::Shape(ShapeError::CritUnknown(99)),
        &|p| {
            set(p, 99, Value::Null);
            set(p, 0, Value::Array(vec![Value::Uint(99)]));
        },
    );
    tamper(
        "an empty crit list",
        BodyError::Shape(ShapeError::BadCrit),
        &|p| set(p, 0, Value::Array(vec![])),
    );

    // Bytes that are not even the strict profile.
    let ok = bytes_of(&base());
    let mut trailing = ok.clone();
    trailing.push(0);
    let raw = [
        (
            "an unsorted map",
            vec![0xa2, 0x02, 0x40, 0x01, 0x01],
            BodyError::Cbor(DecodeError::UnsortedMapKeys),
        ),
        (
            "a duplicate key",
            vec![0xa2, 0x01, 0x01, 0x01, 0x02],
            BodyError::Cbor(DecodeError::DuplicateMapKey),
        ),
        (
            "a float",
            vec![0xa1, 0x01, 0xf9, 0x3c, 0x00],
            BodyError::Cbor(DecodeError::DisallowedType),
        ),
        (
            "an indefinite length",
            vec![0xbf, 0xff],
            BodyError::Cbor(DecodeError::IndefiniteLength),
        ),
        (
            "a non-shortest integer",
            vec![0xa1, 0x01, 0x18, 0x01],
            BodyError::Cbor(DecodeError::NotShortest),
        ),
        (
            "invalid UTF-8",
            vec![0xa1, 0x04, 0x61, 0xff],
            BodyError::Cbor(DecodeError::InvalidUtf8),
        ),
        (
            "trailing bytes",
            trailing,
            BodyError::Cbor(DecodeError::TrailingBytes),
        ),
        (
            "empty input",
            vec![],
            BodyError::Cbor(DecodeError::Truncated),
        ),
        (
            "not a map",
            vec![0x05],
            BodyError::Shape(ShapeError::NotAnIntMap),
        ),
        (
            "a body over 128 KiB",
            vec![0; MAX_BODY + 1],
            BodyError::TooLarge,
        ),
    ];
    cases.extend(raw);

    assert!(
        cases.len() >= 16,
        "the check needs at least 16 malformed bodies"
    );
    let mut seen: HashSet<BodyError> = HashSet::new();
    for (name, bytes, expected) in &cases {
        assert_eq!(
            ReportBody::decode(bytes).unwrap_err(),
            *expected,
            "the malformed body '{name}' failed with another error"
        );
        assert!(
            seen.insert(*expected),
            "'{name}' shares its error {expected:?} with another case"
        );
    }
}

#[test]
fn r11_a_text_refuses_controls_and_bidi_overrides_but_not_line_breaks() {
    use azlin_reports::Text;
    assert_eq!(
        Text::new("a\u{202E}b"),
        Err(BodyError::ControlChar(Field::Text))
    );
    assert_eq!(
        Text::new("a\u{2067}b"),
        Err(BodyError::ControlChar(Field::Text))
    );
    assert_eq!(
        Text::new("a\u{1b}[31mred"),
        Err(BodyError::ControlChar(Field::Text))
    );
    assert_eq!(
        Text::new("a\u{85}b"),
        Err(BodyError::ControlChar(Field::Text))
    );
    assert!(Text::new("line one\nline two\ttab").is_ok());
    assert_eq!(
        Text::for_field("a\nb", Field::Url),
        Err(BodyError::ControlChar(Field::Url))
    );
    assert_eq!(
        Text::for_field("a b", Field::Url),
        Err(BodyError::ControlChar(Field::Url))
    );
    assert_eq!(
        Text::new(&"x".repeat(2_001)),
        Err(BodyError::TooLong(Field::Text))
    );
    assert!(Text::new(&"x".repeat(2_000)).is_ok());
    assert_eq!(
        Text::for_field(&"x".repeat(201), Field::Contact),
        Err(BodyError::TooLong(Field::Contact))
    );
    assert!(Text::for_field(&"x".repeat(200), Field::Contact).is_ok());
}

#[test]
fn r11_unknown_keys_are_ignored_and_kept_at_every_level() {
    // A body from a newer writer: key 99 at the top, key 9 in the subject, key 9 in the record.
    let record = raw_map(vec![
        (1, Value::Bytes(vec![1, 2, 3])),
        (2, Value::Bytes(vec![3; 32])),
        (3, Value::Bytes(vec![4; 64])),
        (9, Value::Text("future record".into())),
    ]);
    let subject = raw_map(vec![
        (1, Value::Uint(2)),
        (2, Value::Array(vec![record])),
        (9, Value::Array(vec![Value::Uint(1), Value::Null])),
    ]);
    let top = raw_map(vec![
        (1, Value::Uint(1)),
        (2, Value::Bytes(vec![5; 16])),
        (3, Value::Uint(1)),
        (6, subject),
        (99, Value::Bytes(vec![7; 5])),
    ]);
    let bytes = cbor(&top);
    let body = ReportBody::decode(&bytes).expect("unknown non-crit keys are ignored");
    assert_eq!(body.extras.fields().len(), 1);
    assert_eq!(body.extras.fields()[0].0, 99);
    assert!(body.extras.crit().is_empty());
    let Subject::ChatRecords { records, extras } = &body.subject else {
        panic!("a chat subject")
    };
    assert_eq!(extras.fields().len(), 1);
    assert_eq!(extras.fields()[0].0, 9);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].extras.fields().len(), 1);
    assert_eq!(records[0].extras.fields()[0].0, 9);
    // The known fields are read as always.
    assert_eq!(body.reason, Reason::IllegalContent);
    assert_eq!(records[0].record, vec![1, 2, 3]);
    // Kept: the copy is the original bytes (the signature rule: whoever copies a structure keeps its fields).
    assert_eq!(body.encode().unwrap(), bytes);
}

#[test]
fn r11_a_copy_keeps_crit_and_dropping_it_changes_the_bytes() {
    // `crit` lists key 3, the reason, which this reader understands: the body is accepted and keeps the list.
    let mut pairs = base();
    set(&mut pairs, 0, Value::Array(vec![Value::Uint(3)]));
    let bytes = bytes_of(&pairs);
    let body = ReportBody::decode(&bytes).expect("a crit key this reader understands is fine");
    assert_eq!(body.extras.crit(), &[3]);
    assert_eq!(
        body.encode().unwrap(),
        bytes,
        "a copy of a structure with crit equals the original bytes"
    );
    // A writer that forgot to copy `crit` writes other bytes (this is what the rule protects).
    let mut stripped = body.clone();
    stripped.extras = Extras::default();
    assert_ne!(stripped.encode().unwrap(), bytes);
}

#[test]
fn r11_hand_built_extras_that_cannot_be_read_back_are_refused_at_encode() {
    let mut body = link_body(Reason::Malware);
    body.extras = Extras::new(vec![99], vec![]);
    assert_eq!(
        body.encode(),
        Err(BodyError::Extras(ExtrasError::CritUnknown(99)))
    );
    body.extras = Extras::new(vec![3, 3], vec![]);
    assert_eq!(body.encode(), Err(BodyError::Extras(ExtrasError::BadCrit)));
    body.extras = Extras::new(vec![0], vec![]);
    assert_eq!(body.encode(), Err(BodyError::Extras(ExtrasError::BadCrit)));
    // A field under a key the body knows would be written twice.
    body.extras = Extras::new(vec![], vec![(4, Value::Null)]);
    assert_eq!(body.encode(), Err(BodyError::Extras(ExtrasError::Key(4))));
    // Out of order.
    body.extras = Extras::new(vec![], vec![(12, Value::Null), (10, Value::Null)]);
    assert_eq!(body.encode(), Err(BodyError::Extras(ExtrasError::Key(10))));
    body.extras = Extras::new(vec![], vec![(10, Value::Null), (12, Value::Null)]);
    assert!(body.encode().is_ok());
}

#[test]
fn r11_every_limit_is_exact_at_its_edge() {
    use azlin_reports::{Text, MAX_CONTACT, MAX_RECORDS, MAX_REPLY, MAX_TEXT, MAX_URL};
    // The free text: 2,000 bytes are fine, 2,001 are not (a text made for the url field can be longer).
    let mut body = link_body(Reason::Other);
    body.text = Some(Text::for_field(&"t".repeat(MAX_TEXT), Field::Text).unwrap());
    assert_eq!(body.validate(), Ok(()));
    body.text = Some(Text::for_field(&"t".repeat(MAX_TEXT + 1), Field::Url).unwrap());
    assert_eq!(body.validate(), Err(BodyError::TooLong(Field::Text)));
    // The contact: 200 / 201.
    let mut body = link_body(Reason::Other);
    body.contact = Some(Text::for_field(&"c".repeat(MAX_CONTACT), Field::Text).unwrap());
    assert_eq!(body.validate(), Ok(()));
    body.contact = Some(Text::for_field(&"c".repeat(MAX_CONTACT + 1), Field::Text).unwrap());
    assert_eq!(body.validate(), Err(BodyError::TooLong(Field::Contact)));
    // The url: 2,048 / 2,049, with its key.
    let url = |len: usize| format!("https://x/#{}", "k".repeat(len - "https://x/#".len()));
    let mut body = link_body(Reason::Other);
    body.subject = Subject::Link {
        url: Text::for_field(&url(MAX_URL), Field::Url).unwrap(),
        extras: Extras::default(),
    };
    assert_eq!(body.validate(), Ok(()));
    assert_eq!(
        Text::for_field(&url(MAX_URL + 1), Field::Url),
        Err(BodyError::TooLong(Field::Url))
    );
    // The reply handle: 4,096 / 4,097.
    let mut body = link_body(Reason::Other);
    body.reply = Some(vec![1; MAX_REPLY]);
    assert_eq!(body.validate(), Ok(()));
    body.reply = Some(vec![1; MAX_REPLY + 1]);
    assert_eq!(body.validate(), Err(BodyError::TooLong(Field::Reply)));
    // The chat records: 20 / 21 of them, and 64 KiB / 64 KiB + 1 bytes each.
    let records = |n: usize| (0..n).map(|i| chat_record(i as u8)).collect::<Vec<_>>();
    assert_eq!(
        chat_body(Reason::Other, records(MAX_RECORDS)).validate(),
        Ok(())
    );
    assert_eq!(
        chat_body(Reason::Other, records(MAX_RECORDS + 1)).validate(),
        Err(BodyError::TooManyRecords)
    );
    let big = |n: usize| SignedRecord {
        record: vec![7; n],
        ..chat_record(1)
    };
    assert_eq!(
        chat_body(Reason::Other, vec![big(MAX_RECORD)]).validate(),
        Ok(())
    );
    assert_eq!(
        chat_body(Reason::Other, vec![big(MAX_RECORD + 1)]).validate(),
        Err(BodyError::RecordTooLarge(0))
    );
    // The whole body: exactly 128 KiB encodes and decodes, one byte more is refused. Two records, the first of
    // the maximum size, the second sized so that the encoding is exactly MAX_BODY bytes.
    let two = |second: usize| {
        chat_body(
            Reason::Other,
            vec![
                big(MAX_RECORD),
                SignedRecord {
                    record: vec![9; second],
                    ..chat_record(2)
                },
            ],
        )
    };
    let probe = two(300).encode().unwrap().len();
    let fit = 300 + (MAX_BODY - probe);
    assert!((256..=MAX_RECORD).contains(&fit), "the probe is off: {fit}");
    let exact = two(fit).encode().unwrap();
    assert_eq!(
        exact.len(),
        MAX_BODY,
        "the construction of a body of exactly 128 KiB is off"
    );
    assert_eq!(ReportBody::decode(&exact).unwrap(), two(fit));
    assert_eq!(
        two(fit + 1).validate(),
        Ok(()),
        "the size of the whole is the encoder's check"
    );
    assert_eq!(two(fit + 1).encode(), Err(BodyError::TooLarge));
    let mut over = exact.clone();
    over.push(0);
    assert_eq!(ReportBody::decode(&over), Err(BodyError::TooLarge));
}

// ---------------------------------------------------------------- R12

#[test]
fn r12_csam_needs_no_contact_and_others_may_have_one() {
    let bodies: [fn(Reason) -> ReportBody; 4] = [
        link_body,
        member_body,
        |r| chat_body(r, vec![chat_record(1)]),
        |r| file_body(r, true, "a file"),
    ];
    for make in bodies {
        for reason in Reason::ALL {
            let mut body = make(reason);
            assert!(body.contact.is_none());
            // No contact validates for every reason, Csam included (Art. 16(2) DSA).
            body.validate()
                .unwrap_or_else(|e| panic!("{reason:?} without contact: {e}"));
            let bytes = body.encode().unwrap();
            assert_eq!(ReportBody::decode(&bytes).unwrap(), body);
            // A contact validates for every reason.
            body.contact = Some(text("reporter@example.org"));
            body.validate()
                .unwrap_or_else(|e| panic!("{reason:?} with contact: {e}"));
            let back = ReportBody::decode(&body.encode().unwrap()).unwrap();
            assert_eq!(back.contact, body.contact);
        }
    }
    // The wire form of a Csam body has no key 5: the contact is not written when it is not there.
    let csam = link_body(Reason::Csam).encode().unwrap();
    let value = azlin_cbor::decode(&csam, &azlin_cbor::Limits::OBJECT).unwrap();
    assert!(value.as_map().unwrap().get(&Value::Uint(5)).is_none());
}

// ---------------------------------------------------------------- R13

/// A test double for the signature check: a record verifies when its signature's first byte is its author's
/// first byte plus seven (what `chat_record` makes).
struct Toy;

impl RecordVerifier for Toy {
    fn verifies(&self, record: &SignedRecord) -> bool {
        record.sig[0] == record.author[0].wrapping_add(7)
    }
}

/// A verifier that never verifies anything.
struct Never;

impl RecordVerifier for Never {
    fn verifies(&self, _: &SignedRecord) -> bool {
        false
    }
}

#[test]
fn r13_a_chat_report_verifies_its_authors() {
    let records: Vec<SignedRecord> = (1..=5).map(chat_record).collect();
    let body = chat_body(Reason::IllegalContent, records.clone());
    assert_eq!(body.verify_authors(&Toy), Ok(()));
    // One bad signature fails the whole body, with its index.
    for (bad, _) in records.iter().enumerate() {
        let mut tampered = records.clone();
        tampered[bad].sig[0] ^= 1;
        let body = chat_body(Reason::IllegalContent, tampered);
        assert_eq!(body.verify_authors(&Toy), Err(BodyError::BadSignature(bad)));
    }
    // Two bad ones: the first is named.
    let mut two = records;
    two[1].sig[0] ^= 1;
    two[3].sig[0] ^= 1;
    assert_eq!(
        chat_body(Reason::Other, two).verify_authors(&Toy),
        Err(BodyError::BadSignature(1))
    );
    // A subject that carries no record has nothing to verify.
    assert_eq!(link_body(Reason::Phishing).verify_authors(&Never), Ok(()));
    assert_eq!(member_body(Reason::Phishing).verify_authors(&Never), Ok(()));
    // A chat body is refused by the verifier that refuses everything, at the first record.
    assert_eq!(
        chat_body(Reason::Phishing, vec![chat_record(1)]).verify_authors(&Never),
        Err(BodyError::BadSignature(0))
    );
}

// ---------------------------------------------------------------- R14

#[test]
fn r14_the_link_url_carries_its_key_and_only_the_operator_opens_it() {
    let mut body = link_body(Reason::Phishing);
    body.validate().expect("a link with its key validates");
    for url in [
        "https://drive.azlin.io/l/abc123",
        "https://drive.azlin.io/l/abc123#",
        "",
        "abc",
    ] {
        body.subject = Subject::Link {
            url: text(url),
            extras: Extras::default(),
        };
        let err = body.validate().unwrap_err();
        assert!(
            matches!(err, BodyError::UrlNoFragment | BodyError::Empty(Field::Url)),
            "'{url}' was not refused for its fragment: {err:?}"
        );
        assert_eq!(
            body.encode().unwrap_err(),
            err,
            "encode refuses what validate refuses"
        );
    }
    // `Debug` prints no url (it can hold a key), no contact, no text, no record.
    let mut body = link_body(Reason::Phishing);
    body.subject = Subject::Link {
        url: text("https://drive.azlin.io/l/abc123#SUPERSECRETKEY"),
        extras: Extras::default(),
    };
    body.contact = Some(text("alice@example.org"));
    body.text = Some(text("the reporter's own words"));
    let printed = format!("{body:?}");
    for leak in [
        "SUPERSECRETKEY",
        "abc123",
        "drive.azlin.io",
        "alice",
        "example.org",
        "own words",
    ] {
        assert!(
            !printed.contains(leak),
            "Debug of a body printed '{leak}': {printed}"
        );
    }
    assert!(
        printed.contains("Phishing"),
        "the reason may be printed: {printed}"
    );
    let chat = chat_body(Reason::IllegalContent, vec![chat_record(0x5A)]);
    let printed = format!("{chat:?} {:?}", chat_record(0x5A));
    assert!(
        !printed.contains("5a5a"),
        "Debug printed a record or a key: {printed}"
    );
    assert!(
        !printed.contains("[90, 91"),
        "Debug printed the record bytes: {printed}"
    );
    assert!(!format!("{:?}", text("SECRET")).contains("SECRET"));
}

/// A sealer that remembers what it was given and answers with its input reversed.
struct Recording(std::cell::RefCell<Vec<Vec<u8>>>);

impl ReportSealer for Recording {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        self.0.borrow_mut().push(plaintext.to_vec());
        Ok(plaintext.iter().rev().copied().collect())
    }
}

struct Refusing(SealError);

impl ReportSealer for Refusing {
    fn seal(&self, _: &[u8]) -> Result<Vec<u8>, SealError> {
        Err(self.0)
    }
}

#[test]
fn r14_a_body_is_validated_before_it_is_sealed_and_the_sealer_gets_its_encoding() {
    let body = link_body(Reason::Csam);
    let sealer = Recording(Default::default());
    let sealed = body.seal(&sealer).expect("sealed");
    let given = sealer.0.borrow();
    assert_eq!(given.len(), 1);
    assert_eq!(
        given[0],
        body.encode().unwrap(),
        "the sealer is given the canonical encoding"
    );
    assert_eq!(sealed, given[0].iter().rev().copied().collect::<Vec<u8>>());
    drop(given);

    // An invalid body never reaches the sealer.
    let mut bad = link_body(Reason::Csam);
    bad.subject = Subject::Link {
        url: text("https://drive.azlin.io/l/x"),
        extras: Extras::default(),
    };
    let sealer = Recording(Default::default());
    assert_eq!(bad.seal(&sealer), Err(BodyError::UrlNoFragment));
    assert!(
        sealer.0.borrow().is_empty(),
        "the sealer was called with an invalid body"
    );

    // The sealer's refusal comes back typed.
    for why in [
        SealError::NoOperatorKey,
        SealError::TooLarge,
        SealError::Failed,
    ] {
        assert_eq!(body.seal(&Refusing(why)), Err(BodyError::Seal(why)));
    }
}

// ---------------------------------------------------------------- R22

const GOLDEN_LINK: &str = "a5010102501111111111111111111111111111111103050468626164206c696e6b06a2010102782068747470733a2f2f64726976652e617a6c696e2e696f2f6c2f616263236b6579";
const GOLDEN_CHAT: &str = "a60101025022222222222222222222222222222222030105656140622e6306a201020282a30143010203025820333333333333333333333333333333333333333333333333333333333333333303584044444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444444a3014268690258205555555555555555555555555555555555555555555555555555555555555555035840666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666660743090807";
const GOLDEN_MEMBER: &str = "a5010102507777777777777777777777777777777703070465616275736506a30103025088888888888888888888888888888888035099999999999999999999999999999999";
const GOLDEN_FILE: &str = "a401010250aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa030206a301040250bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb03687365652066696c65";

fn golden_bodies() -> Vec<(&'static str, ReportBody, &'static str)> {
    let link = ReportBody {
        id: ReportId::from_bytes([0x11; 16]),
        reason: Reason::Phishing,
        text: Some(text("bad link")),
        contact: None,
        subject: Subject::Link {
            url: text("https://drive.azlin.io/l/abc#key"),
            extras: Extras::default(),
        },
        reply: None,
        extras: Extras::default(),
    };
    let record = |record: Vec<u8>, author: u8, sig: u8| SignedRecord {
        record,
        author: [author; 32],
        sig: [sig; 64],
        extras: Extras::default(),
    };
    let chat = ReportBody {
        id: ReportId::from_bytes([0x22; 16]),
        reason: Reason::IllegalContent,
        text: None,
        contact: Some(text("a@b.c")),
        subject: Subject::ChatRecords {
            records: vec![
                record(vec![1, 2, 3], 0x33, 0x44),
                record(vec![0x68, 0x69], 0x55, 0x66),
            ],
            extras: Extras::default(),
        },
        reply: Some(vec![9, 8, 7]),
        extras: Extras::default(),
    };
    let member = ReportBody {
        id: ReportId::from_bytes([0x77; 16]),
        reason: Reason::Other,
        text: Some(text("abuse")),
        contact: None,
        subject: Subject::Member {
            member: [0x88; 16],
            account: acct(0x99),
            extras: Extras::default(),
        },
        reply: None,
        extras: Extras::default(),
    };
    let file = ReportBody {
        id: ReportId::from_bytes([0xAA; 16]),
        reason: Reason::Csam,
        text: None,
        contact: None,
        subject: Subject::File {
            link: Some(link_ref(0xBB)),
            note: text("see file"),
            extras: Extras::default(),
        },
        reply: None,
        extras: Extras::default(),
    };
    vec![
        ("link", link, GOLDEN_LINK),
        ("chat records", chat, GOLDEN_CHAT),
        ("member", member, GOLDEN_MEMBER),
        ("file", file, GOLDEN_FILE),
    ]
}

fn link_ref(n: u8) -> azlin_reports::LinkRef {
    link(n)
}

#[test]
fn r22_golden_bodies() {
    // The hex was made by an independent encoder (scratch golden.py, written from RFC 8949 section 4.2.1), and
    // the member body reads by eye: a5 (map of 5) 01 01 (version 1) 02 50 77.. (id) 03 07 (Other) 04 65 "abuse"
    // 06 a3 (subject: 01 03 member, 02 50 88.., 03 50 99..).
    for (name, body, golden) in golden_bodies() {
        assert_eq!(
            hex(&body.encode().unwrap()),
            golden,
            "the golden {name} body changed"
        );
        assert_eq!(
            ReportBody::decode(&unhex(golden)).unwrap(),
            body,
            "the golden {name} body no longer decodes"
        );
    }
    // One body per subject kind.
    let kinds: HashSet<&str> = golden_bodies()
        .iter()
        .map(|(_, b, _)| match b.subject {
            Subject::Link { .. } => "link",
            Subject::ChatRecords { .. } => "chat",
            Subject::Member { .. } => "member",
            Subject::File { .. } => "file",
        })
        .collect();
    assert_eq!(kinds.len(), 4);
}
