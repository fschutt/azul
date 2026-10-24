//! Format version 1 must decode forever: containers and pack member records that this version
//! wrote are committed under `tests/golden/`, and every later build must turn each back into its
//! original, byte for byte. A failure here means a change broke files that users already store.
//!
//! The vectors cover every codec byte (0, 1, 2, 4), every transform (0, 1, 2), every transform
//! record (literal, Lepton, deflate, PNG IDAT, a Lepton record inside a deflate record) and both
//! member-only codecs. Never edit or delete a vector; a new format version adds new ones.
//!
//! `AZC_WRITE_GOLDEN=1 cargo test -p azlin-compress --test golden -- --ignored` writes the
//! vectors that are missing (it never overwrites one) and prints the BLAKE3 lines for the table.

mod common;

use std::path::PathBuf;

use azlin_compress::{
    container::read_header, decode, decode_member, encode, encode_member, segment::Choice, Budget,
    Kind, Limits, Options, TransformId, MEMBER_CONTAINER,
};
use common::Rng;

/// One vector: its name, the BLAKE3 of its original, the transform its container names, and the
/// codec bytes its segments use (for a member: the record's codec byte).
struct Vector {
    name: &'static str,
    original_blake3: &'static str,
    transform: TransformId,
    codecs: &'static [u8],
}

const fn container(name: &'static str, original_blake3: &'static str, transform: TransformId, codecs: &'static [u8]) -> Vector {
    Vector {
        name,
        original_blake3,
        transform,
        codecs,
    }
}

/// Format version 1. A line here is never changed or removed.
const V1: &[Vector] = &[
    container("v1-stored", "955163ebb448815d7d9b6b6d0a5708f9a469ed6721c4310c89acc23e156e4d38", TransformId::None, &[0]),
    container("v1-zstd", "8a2fa2344e4e5eeb6d43f10ae9b50795638e157fe431addce0948ebb82fea277", TransformId::None, &[1]),
    container("v1-brotli", "4c9d3fd2c0414c37bc3874e50e3b06d87c1b69dec1389f122ca0ee6dd868fd0f", TransformId::None, &[2]),
    container("v1-dictionary", "995ae898dec25d1d74dd3585ac596b04eb8eeb9fb7f3b390c2af382cdc3f294d", TransformId::None, &[4]),
    container("v1-jpeg", "d1ed31e6936feb0fb690a9a07412aea67d78ba71eadef760b044865c0022170b", TransformId::Jpeg, &[0]),
    container("v1-gzip", "037f9b46fdfc853ee884a8a07788a3849852954e13e7b3e0075d0301a855ccc4", TransformId::Containers, &[1]),
    container("v1-png", "f73830055a337a1e134ae03a1d360f56e8b094da77861531f08bc7f99420ddb2", TransformId::Containers, &[1]),
    container("v1-docx", "c6eb703a28820171b9026ee718b1f07d50c5adb286073f53c39106833f2ebfb9", TransformId::Containers, &[1]),
    container("v1-pdf", "d37c144ad953b60f45381b5cdb24202e62a21db52890ed0521beff49d87e8fb6", TransformId::Containers, &[1]),
];

/// Pack member records of format version 1: `<name>.member` is the codec byte, then the data.
const V1_MEMBERS: &[Vector] = &[
    container("v1-member-dictionary", "0fb7bf501d5d6d2b9fee88600f04bf4a8a422ee9fc95cc217ba64893a4bd8ccf", TransformId::None, &[4]),
    container("v1-member-jpeg", "fc5bc8ef913710e81309abd29b8457207a02a6c09610449a44812e4bb3618471", TransformId::Jpeg, &[MEMBER_CONTAINER]),
];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("golden")
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(dir().join(name)).unwrap_or_else(|e| panic!("golden vector {name}: {e}"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The codec bytes of a container's segments, in order (a minimal walk of the frames).
fn segment_codecs(container: &[u8]) -> Vec<u8> {
    let params = usize::from(u16::from_le_bytes([container[12], container[13]]));
    let mut at = 14 + params;
    let mut codecs = Vec::new();
    while container[at] != 0xFF {
        codecs.push(container[at]);
        let stored = u32::from_le_bytes(container[at + 5..at + 9].try_into().unwrap()) as usize;
        at += 9 + stored;
    }
    codecs
}

#[test]
fn every_format_1_container_still_decodes_to_its_original() {
    for v in V1 {
        let original = read(&format!("{}.orig", v.name));
        let bytes = read(&format!("{}.azc1", v.name));
        assert_eq!(hex(blake3::hash(&original).as_bytes()), v.original_blake3, "{}: the original changed", v.name);
        let header = read_header(&mut &bytes[..]).unwrap();
        assert_eq!(header.transform, v.transform, "{}", v.name);
        let mut codecs = segment_codecs(&bytes);
        codecs.sort_unstable();
        codecs.dedup();
        assert_eq!(codecs, v.codecs, "{}", v.name);
        // The default limits are the format's: every container decodes under them.
        let decoded = decode(&bytes, &Options::default()).unwrap_or_else(|e| panic!("{}: {e}", v.name));
        assert!(decoded[..] == original[..], "{}: decodes to other bytes", v.name);
        // One thread or several: Lepton's output does not depend on it.
        let one = Options {
            threads: 1,
            ..Options::default()
        };
        assert!(decode(&bytes, &one).unwrap()[..] == original[..], "{}", v.name);
    }
}

#[test]
fn every_format_1_member_record_still_decodes_to_its_original() {
    for v in V1_MEMBERS {
        let original = read(&format!("{}.orig", v.name));
        let record = read(&format!("{}.member", v.name));
        assert_eq!(hex(blake3::hash(&original).as_bytes()), v.original_blake3, "{}: the original changed", v.name);
        assert_eq!(&record[..1], v.codecs, "{}", v.name);
        let decoded = decode_member(record[0], &record[1..], original.len(), &Options::default())
            .unwrap_or_else(|e| panic!("{}: {e}", v.name));
        assert!(decoded[..] == original[..], "{}: decodes to other bytes", v.name);
    }
}

#[test]
fn every_file_in_the_golden_folder_is_a_listed_vector() {
    let mut listed: Vec<String> = V1
        .iter()
        .flat_map(|v| [format!("{}.orig", v.name), format!("{}.azc1", v.name)])
        .chain(V1_MEMBERS.iter().flat_map(|v| [format!("{}.orig", v.name), format!("{}.member", v.name)]))
        .collect();
    listed.sort();
    let mut found: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    found.sort();
    assert_eq!(found, listed);
}

/// The originals, made from fixed seeds and the committed fixtures.
fn originals() -> Vec<(&'static str, Vec<u8>)> {
    let mut rng = Rng::new(900);
    let mut zstd_input = Vec::new();
    for i in 0..3000u32 {
        zstd_input.extend_from_slice(&(i % 97 * 13).to_le_bytes());
        zstd_input.push(rng.below(4) as u8);
    }
    let docx_body = String::from_utf8(common::far_repeats(&mut rng, 34 * 1024, 2)).unwrap();
    let docx_document = format!("<?xml version=\"1.0\"?><w:document><w:body>{docx_body}</w:body></w:document>");
    let pdf_content = common::zlib(&common::far_repeats(&mut rng, 34 * 1024, 2), 6);
    let embedded: &[u8] = include_bytes!("fixtures/embedded.jpg");
    vec![
        ("v1-stored", rng.bytes(3000)),
        ("v1-zstd", zstd_input),
        ("v1-brotli", common::text(&mut rng, 6000)),
        ("v1-dictionary", br#"{"name": "settings", "version": "1.0.0", "private": true, "dependencies": {"left-pad": "^1.3.0"}, "scripts": {"test": "node test.js"}}"#.to_vec()),
        ("v1-jpeg", include_bytes!("fixtures/baseline.jpg").to_vec()),
        ("v1-gzip", common::gzip(&common::far_repeats(&mut rng, 34 * 1024, 2), 6)),
        ("v1-png", common::png(&mut rng, 128, 100, 90, 8192)),
        (
            "v1-docx",
            common::deflated_zip(
                &[
                    ("[Content_Types].xml", b"<?xml version=\"1.0\"?><Types/>"),
                    ("word/document.xml", docx_document.as_bytes()),
                    ("word/media/image1.jpeg", embedded),
                ],
                6,
            ),
        ),
        (
            "v1-pdf",
            common::pdf(
                &[
                    ("/Filter /FlateDecode", &pdf_content),
                    ("/Type /XObject /Subtype /Image /Width 560 /Height 420 /Filter /DCTDecode", embedded),
                ],
                40,
            ),
        ),
        ("v1-member-dictionary", br#"{"id": 7, "name": "user7", "active": true, "score": 412, "tags": ["a", "b"]}"#.to_vec()),
        ("v1-member-jpeg", include_bytes!("fixtures/progressive.jpg").to_vec()),
    ]
}

/// How each vector is encoded: one thread, no time limit, small segments for the codec ones so
/// that a short last segment occurs.
fn writer_options(name: &str) -> Options {
    let mut options = Options {
        threads: 1,
        limits: Limits {
            codec_time: Budget::UNLIMITED,
            transform_time: Budget::UNLIMITED,
            ..Limits::default()
        },
        ..Options::idle()
    };
    match name {
        "v1-zstd" => {
            options.segment_size = 4096;
            options.max_brotli_quality = 0;
        }
        "v1-brotli" => {
            options.segment_size = 4096;
            options.dictionaries = false;
        }
        _ => {}
    }
    options
}

#[test]
#[ignore = "writes the golden vectors that are missing; run once per new format version"]
fn write_missing_golden_vectors() {
    if std::env::var_os("AZC_WRITE_GOLDEN").is_none() {
        return;
    }
    std::fs::create_dir_all(dir()).unwrap();
    for (name, original) in originals() {
        let member = name.contains("member");
        let out = dir().join(format!("{name}.{}", if member { "member" } else { "azc1" }));
        if !out.exists() {
            let options = writer_options(name);
            let bytes = if member {
                let m = encode_member(&original, &options).unwrap();
                [&[m.codec][..], &m.data[..]].concat()
            } else {
                let encoded = encode(&original, &options).unwrap();
                if name == "v1-brotli" {
                    assert!(matches!(encoded.report.codec.first_choice, Some(Choice::Brotli(_))), "{:?}", encoded.report);
                }
                if name == "v1-dictionary" {
                    assert_eq!(encoded.report.kind, Kind::Json);
                }
                encoded.bytes.to_vec()
            };
            std::fs::write(&out, bytes).unwrap();
            std::fs::write(dir().join(format!("{name}.orig")), &original).unwrap();
        }
        println!("{name} {}", hex(blake3::hash(&original).as_bytes()));
    }
}
