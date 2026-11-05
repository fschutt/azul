//! Numbers somebody else wrote (ARCH28 RF50 / AR5b-5): `Declared` places itself in the data the
//! reader holds and sizes nothing, `ByteReader` takes bytes by those counts and cannot run past its
//! end, and the named limits are what the formats read their sizes against.

use super::mem_bucket::MemBucket;
use crate::{
    crypto::keys::{
        load_key_chain, load_member_wrap, load_recovery_wrap, MemberSecret, CHAIN_KEY_FILE,
        RECOVERY_KEY_FILE,
    },
    limits::{named, ByteReader, Declared, Limit, ShortRead, PREALLOC},
    rotation::{pending, JOURNAL_FILE},
    DriveError,
};

#[test]
fn x_ar5b_5_a_declared_number_is_compared_and_placed_and_sizes_nothing() {
    let limit = Limit::new("a thing", 10);
    assert_eq!(Declared::new(10).within(limit).unwrap(), 10);
    let over = Declared::new(11).within(limit).unwrap_err();
    assert!(
        over.to_string().contains("a thing"),
        "the error names the limit: {over}"
    );

    // A reservation up front is never more than PREALLOC, whatever the header said.
    assert_eq!(Declared::new(5).prealloc(), 5);
    assert_eq!(Declared::new(PREALLOC as u64 + 1).prealloc(), PREALLOC);
    assert_eq!(Declared::new(u64::MAX).prealloc(), PREALLOC);

    // A position, and a span whose end is added with a check.
    assert_eq!(Declared::new(3).index_in(4).unwrap(), 3);
    assert!(Declared::new(4).index_in(4).is_err());
    assert!(Declared::new(u64::MAX).index_in(usize::MAX).is_err());
    assert_eq!(Declared::new(2).span_in(Declared::new(3), 5).unwrap(), 2..5);
    assert!(Declared::new(2).span_in(Declared::new(4), 5).is_err());
    assert!(
        Declared::new(u64::MAX)
            .span_in(Declared::new(2), usize::MAX)
            .is_err(),
        "an offset and a length that wrap are no span"
    );
    assert!(Declared::new(2)
        .span_in(Declared::new(u64::MAX), usize::MAX)
        .is_err());
}

#[test]
fn x_ar5b_5_a_byte_reader_takes_by_declared_counts_and_never_runs_past_its_end() {
    let mut reader = ByteReader::new(b"head\r\nbody-bytes\r\n");
    assert_eq!(reader.line().unwrap(), b"head");
    assert_eq!(reader.take(Declared::new(4)).unwrap(), b"body");
    // More than is left, or a number no slice has: a short read, and nothing is taken.
    let short = reader.take(Declared::new(u64::MAX)).unwrap_err();
    assert_eq!(short.had, 8, "{short}");
    assert_eq!(
        reader.take(Declared::new(9)),
        Err(ShortRead { wanted: 9, had: 8 })
    );
    assert_eq!(reader.rest(), b"-bytes\r\n");
    assert_eq!(reader.line().unwrap(), b"-bytes");
    assert!(reader.is_empty());
    // A line without its CRLF is no line.
    assert!(ByteReader::new(b"no end").line().is_err());
    assert!(ByteReader::new(b"").line().is_err());
}

#[test]
fn x_ar5b_5_the_named_limits_are_what_this_software_writes_with_room() {
    assert_eq!(named::KEY_FILE.max, 64 << 10);
    // The chain holds up to CHAIN_MAX_LINKS links of well under 1 KiB each, in 256 KiB.
    assert_eq!(named::KEY_CHAIN.max, 256 << 10);
    assert!(named::JOURNAL.max >= named::KEY_FILE.max);
    assert!(named::INDEX_OBJECT.max >= 256 << 20);
    // The names are what an error says.
    let err = named::KEY_FILE.within(named::KEY_FILE.max + 1).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("a key file may be at most 65536 bytes"),
        "{err}"
    );
}

/// X-AR5b-5: the key files, the key chain and the rotation journal are read through
/// `get_limited`: a bucket that answers a bigger file than any of them is (it can stream until the
/// transport's timeout) is refused with the limit named, before the bytes are parsed.
#[test]
fn x_ar5b_5_a_key_file_over_its_limit_is_refused_naming_the_limit() {
    let bucket = MemBucket::new();
    let is_refused_by = |result: Result<(), DriveError>, name: &str| match result {
        Err(DriveError::Protocol(why)) => {
            assert!(why.contains(name), "{why}");
        }
        other => panic!("{name}: {other:?}"),
    };

    bucket.set(
        RECOVERY_KEY_FILE,
        vec![b'x'; (named::KEY_FILE.max + 1) as usize],
    );
    is_refused_by(
        load_recovery_wrap(&bucket).map(drop),
        "a key file may be at most",
    );

    let member = MemberSecret::generate().unwrap().public().id();
    bucket.set(
        &crate::crypto::keys::member_key_file(&member).unwrap(),
        vec![b'x'; (named::KEY_FILE.max + 1) as usize],
    );
    is_refused_by(
        load_member_wrap(&bucket, &member).map(drop),
        "a key file may be at most",
    );

    bucket.set(
        CHAIN_KEY_FILE,
        vec![b'x'; (named::KEY_CHAIN.max + 1) as usize],
    );
    is_refused_by(
        load_key_chain(&bucket).map(drop),
        "a key chain file may be at most",
    );

    bucket.set(JOURNAL_FILE, vec![b'x'; (named::JOURNAL.max + 1) as usize]);
    is_refused_by(
        pending(&bucket).map(drop),
        "a rotation journal may be at most",
    );

    // A file within the limit that is none of them is the format's complaint, not the limit's.
    bucket.set(JOURNAL_FILE, b"{}".to_vec());
    assert!(matches!(pending(&bucket), Err(DriveError::Corrupt { .. })));
}
