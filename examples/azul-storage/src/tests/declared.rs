//! Numbers somebody else wrote (ARCH28 RF50 / AR5b-5): `Declared` places itself in the data the
//! reader holds and sizes nothing, `ByteReader` takes bytes by those counts and cannot run past its
//! end, and the named limits are what the formats read their sizes against.

use crate::limits::{named, ByteReader, Declared, Limit, ShortRead, PREALLOC};

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
    const _: () = assert!(named::JOURNAL.max >= named::KEY_FILE.max);
    const _: () = assert!(named::INDEX_OBJECT.max >= 256 << 20);
    // The names are what an error says.
    let err = named::KEY_FILE.within(named::KEY_FILE.max + 1).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("a key file may be at most 65536 bytes"),
        "{err}"
    );
}

/// The key files of an encrypted drive (feature `encryption`).
#[cfg(feature = "encryption")]
mod key_files {
    use crate::tests::mem_bucket::MemBucket;
    use crate::{
        crypto::keys::{
            load_key_chain, load_member_wrap, load_recovery_wrap, MemberSecret, CHAIN_KEY_FILE,
            RECOVERY_KEY_FILE,
        },
        limits::{named, Limit},
        rotation::{pending, JOURNAL_FILE},
        DriveError,
    };

    /// `result` is the refusal of `file`, found over `limit`: `Corrupt`, naming the file, with the
    /// limit's words and the size that was one byte too many.
    fn is_refused_by(result: Result<(), DriveError>, file: &str, limit: Limit) {
        match result {
            Err(DriveError::Corrupt { key, reason }) => {
                assert_eq!(key, file, "the refusal names the file");
                assert_eq!(
                    reason,
                    format!(
                        "{} may be at most {} bytes, this one is {}",
                        limit.name,
                        limit.max,
                        limit.max + 1
                    ),
                    "the limit and the size are said"
                );
            }
            other => panic!("{file}: {other:?}"),
        }
    }

    /// X-AR5b-5: the key files, the key chain and the rotation journal are read through
    /// `get_limited`: a bucket that answers a bigger file than any of them is (it can stream until
    /// the transport's timeout) is refused with the limit named, before the bytes are parsed. The
    /// refusal is `Corrupt` - a file nobody who holds the key wrote, the same kind as one that does
    /// not parse (`Limit::read_from`: a drive that checks after the read says `Protocol`, a
    /// transport that refuses the answer on its way says its own error, and the HEAD that follows
    /// says the size as a number) - and it names the FILE and says the limit and the size.
    #[test]
    fn x_ar5b_5_a_key_file_over_its_limit_is_refused_naming_the_limit() {
        let bucket = MemBucket::new();
        bucket.set(
            RECOVERY_KEY_FILE,
            vec![b'x'; (named::KEY_FILE.max + 1) as usize],
        );
        is_refused_by(
            load_recovery_wrap(&bucket).map(drop),
            RECOVERY_KEY_FILE,
            named::KEY_FILE,
        );

        let member = MemberSecret::generate().unwrap().public().id();
        let member_file = crate::crypto::keys::member_key_file(&member).unwrap();
        bucket.set(&member_file, vec![b'x'; (named::KEY_FILE.max + 1) as usize]);
        is_refused_by(
            load_member_wrap(&bucket, &member).map(drop),
            &member_file,
            named::KEY_FILE,
        );

        bucket.set(
            CHAIN_KEY_FILE,
            vec![b'x'; (named::KEY_CHAIN.max + 1) as usize],
        );
        is_refused_by(
            load_key_chain(&bucket).map(drop),
            CHAIN_KEY_FILE,
            named::KEY_CHAIN,
        );

        bucket.set(JOURNAL_FILE, vec![b'x'; (named::JOURNAL.max + 1) as usize]);
        is_refused_by(pending(&bucket).map(drop), JOURNAL_FILE, named::JOURNAL);

        // A file within the limit that is none of them is the format's complaint, not the limit's:
        // the same kind, and no word of a limit.
        bucket.set(JOURNAL_FILE, b"{}".to_vec());
        match pending(&bucket) {
            Err(DriveError::Corrupt { key, reason }) => {
                assert_eq!(key, JOURNAL_FILE);
                assert!(!reason.contains("may be at most"), "{reason}");
            }
            other => panic!("a journal that is none: {other:?}"),
        }
    }
}
