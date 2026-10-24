//! The shipped dictionaries: every one ever shipped is present, unchanged, under its id, and
//! small text files use their kind's dictionary when it wins.

mod common;

use azlin_compress::{
    decode, dict, encode, encode_member,
    segment::{Choice, Codec},
    Budget, Kind, Options,
};
use sha2::{Digest, Sha256};

/// Every dictionary ever shipped: id, kind, file, SHA-256 (`shasum -a 256 dicts/*.zdict`). A
/// line here is never changed or removed (segments written with it must stay decodable).
const SHIPPED: &[(u16, Kind, &str, &str)] = &[
    (1, Kind::Json, "json-1.zdict", "20c0245f0dcaf270d89fb5afe9081ab79fdbea03ba1c48f5768141088b221270"),
    (2, Kind::Markup, "markup-1.zdict", "f0accf832f447a8e0f1e31346ccac214a28f499fb17fe400508735fdf8adbec7"),
    (3, Kind::Text, "text-1.zdict", "44cb28964b8df4822a16b07272d1c2c561518f5b4a1ff917ace9661a331b6ad9"),
    (4, Kind::Code, "code-1.zdict", "e0490360a9956bdc2186690e10756fcc2c372256e0d5ff0e267cfb4a005d21ec"),
];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn every_dictionary_ever_shipped_is_there_unchanged() {
    for (id, kind, name, sha256) in SHIPPED {
        let d = dict::by_id(*id).unwrap_or_else(|| panic!("dictionary {id} is missing"));
        assert_eq!((d.kind, d.name), (*kind, *name), "dictionary {id}");
        assert_eq!(hex(&Sha256::digest(d.bytes)), *sha256, "dictionary {id} changed");
    }
    // Ids are unique; nothing ships that is not pinned above.
    let mut ids: Vec<u16> = dict::all().iter().map(|d| d.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), dict::all().len());
    assert_eq!(dict::all().len(), SHIPPED.len(), "a new dictionary needs its line in SHIPPED");
}

#[test]
fn csv_and_binary_have_no_dictionary_and_text_kinds_have_one() {
    assert_eq!(dict::for_kind(Kind::Json), Some(1));
    assert_eq!(dict::for_kind(Kind::Markup), Some(2));
    assert_eq!(dict::for_kind(Kind::Text), Some(3));
    assert_eq!(dict::for_kind(Kind::Code), Some(4));
    assert_eq!(dict::for_kind(Kind::Csv), None);
    assert_eq!(dict::for_kind(Kind::Binary), None);
}

fn small_json() -> Vec<u8> {
    // The shape of a typical small JSON file: a config with a schema reference.
    br#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Settings",
  "type": "object",
  "properties": {
    "name": { "type": "string", "description": "The name of the project" },
    "version": { "type": "string" },
    "private": { "type": "boolean", "default": false },
    "dependencies": { "type": "object", "additionalProperties": { "type": "string" } }
  },
  "required": ["name", "version"]
}
"#
    .to_vec()
}

#[test]
fn a_small_json_file_is_compressed_with_the_json_dictionary() {
    let input = small_json();
    let options = Options {
        limits: azlin_compress::Limits {
            codec_time: Budget::UNLIMITED,
            ..Default::default()
        },
        ..Options::upload()
    };
    let with = encode(&input, &options).unwrap();
    assert_eq!(with.report.kind, Kind::Json);
    assert!(matches!(with.report.codec.first_choice, Some(Choice::ZstdDict(1, _))), "{:?}", with.report);
    assert_eq!(&decode(&with.bytes, &options).unwrap()[..], &input[..]);

    let without = encode(
        &input,
        &Options {
            dictionaries: false,
            ..options.clone()
        },
    )
    .unwrap();
    assert!(with.bytes.len() < without.bytes.len(), "{} vs {}", with.bytes.len(), without.bytes.len());

    let member = encode_member(&input, &options).unwrap();
    assert_eq!(member.codec, Codec::ZstdDict.byte());
    assert_eq!(&member.data[..2], &1u16.to_le_bytes());
}

#[test]
fn large_files_do_not_try_the_dictionary() {
    let mut rng = common::Rng::new(30);
    let input = common::json(&mut rng, 20_000);
    assert!(input.len() as u64 > azlin_compress::segment::DICTIONARY_MAX_LEN);
    let report = encode(&input, &Options::upload()).unwrap().report;
    assert!(!matches!(report.codec.first_choice, Some(Choice::ZstdDict(..))), "{report:?}");
}

/// A record of a few dozen bytes: plain zstd's frame costs more than it saves there, so the
/// trial fails, yet a dictionary (the kind's shipped one or brotli's built-in one) still makes
/// it smaller. The trial must not stop a small text file before the dictionaries are tried.
#[test]
fn a_tiny_json_record_plain_zstd_cannot_shrink_still_gets_a_dictionary() {
    let input = br#"{"id": 7, "name": "user7", "active": true, "score": 412, "tags": ["a", "b"]}"#;
    let plain = zstd::bulk::compress(input, 1).unwrap();
    assert!(plain.len() * 100 > input.len() * 95, "plain zstd saves 5 % here: {} of {}", plain.len(), input.len());
    let options = Options {
        limits: azlin_compress::Limits {
            codec_time: Budget::UNLIMITED,
            ..Default::default()
        },
        ..Options::upload()
    };
    let member = encode_member(input, &options).unwrap();
    assert!(
        member.codec == Codec::ZstdDict.byte() || member.codec == Codec::Brotli.byte(),
        "codec {}",
        member.codec
    );
    assert!(member.data.len() < input.len(), "{} of {}", member.data.len(), input.len());
    let report = encode(input, &options).unwrap().report;
    assert!(
        matches!(report.codec.first_choice, Some(Choice::ZstdDict(1, _) | Choice::Brotli(_))),
        "{report:?}"
    );
}
