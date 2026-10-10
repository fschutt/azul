//! The keys of an encrypted drive: file keys wrapped by the drive key or a share key, the
//! object ids that name the bucket's objects, ids of keys; a segment's compression policy;
//! the drive key sealed to members and to the recovery code; a share.

use std::collections::HashSet;

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        azl1::{decrypt, encrypt, OpenObject, WriteOptions},
        codec::{
            brotli_window_bits, decompress, decompress_brotli, looks_compressed, worth_it, Codec,
            Compression, Encoded, Encoder, Recoding, CODEC_BROTLI, CODEC_JPEG_XL,
        },
        keys::{
            load_member_wrap, load_recovery_wrap, member_key_file, store_member_wrap,
            store_recovery_wrap, MemberPublic, MemberSecret, MemberWrap, RecoveryCode,
            RecoveryKdf, RecoveryWrap, RECOVERY_KEY_FILE,
        },
        CryptoError, DriveKey, FileKey, KeyId, ObjectId, ShareKey, WrappedKey, KEY_LEN,
    },
    DriveError,
};

#[test]
fn a_file_key_wrapped_by_the_drive_key_opens_for_its_object_only() {
    let drive = DriveKey::generate().unwrap();
    let file = FileKey::generate().unwrap();
    let object = ObjectId::generate().unwrap();
    let wrapped = drive.wrap_file_key(&file, &object).unwrap();
    assert_eq!(wrapped.key_id, drive.id());
    assert_eq!(drive.unwrap_file_key(&wrapped, &object).unwrap(), file);

    let another_object = ObjectId::generate().unwrap();
    assert!(matches!(
        drive.unwrap_file_key(&wrapped, &another_object),
        Err(CryptoError::Damaged(_))
    ));
    let another_drive = DriveKey::generate().unwrap();
    assert_eq!(
        another_drive.unwrap_file_key(&wrapped, &object),
        Err(CryptoError::WrongKey)
    );
}

#[test]
fn every_wrap_of_a_key_is_new_and_round_trips_through_its_bytes_and_hex() {
    let drive = DriveKey::generate().unwrap();
    let file = FileKey::generate().unwrap();
    let object = ObjectId::generate().unwrap();
    let a = drive.wrap_file_key(&file, &object).unwrap();
    let b = drive.wrap_file_key(&file, &object).unwrap();
    assert_ne!(a.nonce, b.nonce, "a fresh nonce per wrap");
    assert_ne!(a.sealed, b.sealed);

    let bytes = a.to_bytes();
    assert_eq!(bytes.len(), WrappedKey::LEN);
    assert_eq!(WrappedKey::from_bytes(&bytes), Some(a.clone()));
    assert_eq!(WrappedKey::from_hex(&a.to_hex()), Some(a.clone()));
    assert_eq!(WrappedKey::from_bytes(&bytes[1..]), None);

    let mut flipped = a.clone();
    flipped.sealed[0] ^= 1;
    assert!(matches!(
        drive.unwrap_file_key(&flipped, &object),
        Err(CryptoError::Damaged(_))
    ));
    let mut relabelled = a;
    relabelled.key_id = DriveKey::generate().unwrap().id();
    assert_eq!(
        drive.unwrap_file_key(&relabelled, &object),
        Err(CryptoError::WrongKey)
    );
}

#[test]
fn a_share_key_and_the_drive_key_cannot_open_each_others_wraps() {
    let bytes = [5u8; KEY_LEN];
    let drive = DriveKey::from_bytes(bytes);
    // The same bytes as a share key: another id, another wrapping key.
    let share = ShareKey::from_bytes(bytes);
    let file = FileKey::generate().unwrap();
    let object = ObjectId::generate().unwrap();

    let for_share = share.wrap_file_key(&file, &object).unwrap();
    assert_eq!(share.unwrap_file_key(&for_share, &object).unwrap(), file);
    assert_eq!(
        drive.unwrap_file_key(&for_share, &object),
        Err(CryptoError::WrongKey)
    );

    let for_drive = drive.wrap_file_key(&file, &object).unwrap();
    assert_eq!(
        share.unwrap_file_key(&for_drive, &object),
        Err(CryptoError::WrongKey)
    );
    // Even a wrap relabelled with the share's id does not open: the wrapping keys differ.
    let mut relabelled = for_drive;
    relabelled.key_id = share.id();
    assert!(matches!(
        share.unwrap_file_key(&relabelled, &object),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn an_object_id_names_a_data_object_under_a_two_digit_fan_out() {
    let fixed = ObjectId([0xab; 16]);
    let hex = "ab".repeat(16);
    assert_eq!(fixed.to_hex(), hex);
    assert_eq!(fixed.bucket_key(), format!("data/ab/{hex}"));
    assert_eq!(ObjectId::from_bucket_key(&fixed.bucket_key()), Some(fixed));
    assert_eq!(ObjectId::from_bucket_key(&format!("data/cd/{hex}")), None);
    assert_eq!(
        ObjectId::from_bucket_key(&format!("data/AB/{}", "AB".repeat(16))),
        None,
        "only the canonical lowercase form"
    );
    assert_eq!(ObjectId::from_bucket_key("data/ab/abab"), None);
    assert_eq!(ObjectId::from_bucket_key(&format!("mail/ab/{hex}")), None);

    let ids: HashSet<ObjectId> = (0..1000).map(|_| ObjectId::generate().unwrap()).collect();
    assert_eq!(ids.len(), 1000, "a thousand ids, a thousand values");
    for id in ids.iter().take(10) {
        let key = id.bucket_key();
        assert!(key.starts_with("data/"), "{key}");
        assert_eq!(key.len(), "data/".len() + 3 + 32, "{key}");
        assert_eq!(ObjectId::from_bucket_key(&key), Some(*id));
    }
}

#[test]
fn keys_never_print_and_compare_by_value() {
    let key = DriveKey::from_bytes([7u8; KEY_LEN]);
    assert_eq!(format!("{key:?}"), "DriveKey(***)");
    assert_eq!(
        format!("{:?}", FileKey::from_bytes([1u8; KEY_LEN])),
        "FileKey(***)"
    );
    assert_eq!(
        format!("{:?}", ShareKey::from_bytes([1u8; KEY_LEN])),
        "ShareKey(***)"
    );
    assert_eq!(key, DriveKey::from_bytes([7u8; KEY_LEN]));
    assert_ne!(key, DriveKey::from_bytes([8u8; KEY_LEN]));
    assert!(DriveKey::from_slice(&[7u8; 31]).is_none());
    assert_eq!(DriveKey::from_slice(&[7u8; 32]), Some(key.clone()));
    assert_ne!(
        DriveKey::generate().unwrap(),
        DriveKey::generate().unwrap(),
        "two random keys"
    );
    let wrapped = key
        .wrap_file_key(&FileKey::generate().unwrap(), &ObjectId([0u8; 16]))
        .unwrap();
    let shown = format!("{wrapped:?}");
    assert!(shown.contains(&key.id().to_hex()), "{shown}");
    assert!(!shown.contains("sealed"), "{shown}");
}

#[test]
fn a_key_id_is_stable_and_differs_between_keys_and_kinds_of_key() {
    let bytes = [9u8; KEY_LEN];
    let drive = DriveKey::from_bytes(bytes);
    assert_eq!(drive.id(), DriveKey::from_bytes(bytes).id());
    assert_ne!(drive.id(), DriveKey::from_bytes([10u8; KEY_LEN]).id());
    assert_ne!(drive.id(), ShareKey::from_bytes(bytes).id());
    assert_eq!(KeyId::from_hex(&drive.id().to_hex()), Some(drive.id()));
    assert_eq!(drive.id().to_hex().len(), 32);
    assert_ne!(&drive.id().0[..], &bytes[..16], "the id is not the key");
}

#[test]
fn a_crypto_error_comes_back_as_itself_through_an_io_error() {
    let io: std::io::Error = CryptoError::KeyMismatch.into();
    assert_eq!(io.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(CryptoError::from(io), CryptoError::KeyMismatch);
    let plain = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    assert!(matches!(CryptoError::from(plain), CryptoError::Io(why) if why.contains("gone")));
}

// ==== The compression policy of a segment (crypto::codec) ====

/// Text that compresses well.
fn text(len: usize) -> Vec<u8> {
    b"The drive keeps ciphertext only; names live in the encrypted index. "
        .iter()
        .copied()
        .cycle()
        .take(len)
        .collect()
}

/// Bytes from the OS random source: zstd cannot make them smaller.
fn noise(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    crate::crypto::random_bytes(&mut bytes).unwrap();
    bytes
}

#[test]
fn a_segment_stays_compressed_only_when_that_saves_five_percent() {
    assert!(worth_it(95, 100));
    assert!(!worth_it(96, 100));
    assert!(worth_it(0, 1));
    assert!(!worth_it(0, 0), "nothing to save on nothing");
    assert!(!worth_it(100, 100));
}

#[test]
fn already_compressed_formats_are_known_by_their_magic_number() {
    let mut png = b"\x89PNG\r\n\x1A\n".to_vec();
    png.extend(text(100));
    assert!(looks_compressed(&png));
    assert!(looks_compressed(b"\xFF\xD8\xFF\xE0 a jpeg"));
    assert!(looks_compressed(b"PK\x03\x04 a docx"));
    assert!(looks_compressed(b"%PDF-1.7 ..."));
    assert!(
        looks_compressed(b"\x00\x00\x00\x18ftypheic...."),
        "HEIC / MP4 by ftyp"
    );
    assert!(looks_compressed(b"RIFF\x00\x00\x00\x00WEBPVP8 "));
    assert!(!looks_compressed(b"RIFF\x00\x00\x00\x00WAVEfmt "), "WAV is PCM");
    assert!(!looks_compressed(&text(1000)));
    assert!(!looks_compressed(b""));
}

#[test]
fn the_first_segment_decides_whether_the_rest_of_a_file_is_tried() {
    // Text: the trial saves, so every segment is tried (a segment of noise stays stored).
    let mut encoder = Encoder::new(Compression::Auto);
    let first = encoder.encode(&text(64 * 1024)).unwrap();
    assert_eq!(first.codec(), Codec::Zstd);
    if let Encoded::Zstd(bytes) = &first {
        assert!(bytes.len() < 64 * 1024 / 10, "{}", bytes.len());
    }
    assert_eq!(encoder.encode(&noise(4096)).unwrap().codec(), Codec::Stored);
    assert_eq!(encoder.encode(&text(4096)).unwrap().codec(), Codec::Zstd);

    // Noise first: the file is not tried again, not even for text that would compress.
    let mut encoder = Encoder::new(Compression::Auto);
    assert_eq!(encoder.encode(&noise(4096)).unwrap().codec(), Codec::Stored);
    assert_eq!(encoder.encode(&text(4096)).unwrap().codec(), Codec::Stored);

    // A PNG is never tried, whatever follows its magic number.
    let mut png = b"\x89PNG\r\n\x1A\n".to_vec();
    png.extend(text(4096));
    let mut encoder = Encoder::new(Compression::Auto);
    assert_eq!(encoder.encode(&png).unwrap().codec(), Codec::Stored);
    assert_eq!(encoder.encode(&text(4096)).unwrap().codec(), Codec::Stored);

    let mut never = Encoder::new(Compression::Never);
    assert_eq!(never.encode(&text(4096)).unwrap().codec(), Codec::Stored);
    let mut empty = Encoder::new(Compression::Auto);
    assert_eq!(empty.encode(&[]).unwrap().codec(), Codec::Stored);
}

#[test]
fn a_zstd_segment_decompresses_to_exactly_its_length_and_no_further() {
    let plain = text(10_000);
    let mut encoder = Encoder::new(Compression::Auto);
    let Encoded::Zstd(frame) = encoder.encode(&plain).unwrap() else {
        panic!("text compresses");
    };
    let back = decompress(&frame, plain.len()).unwrap();
    assert_eq!(back.as_slice(), plain.as_slice());
    assert!(matches!(
        decompress(&frame, plain.len() - 1),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        decompress(&frame, plain.len() + 1),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        decompress(b"not zstd at all", 100),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn codec_bytes_of_later_versions_are_named_not_guessed() {
    assert_eq!(Codec::from_byte(0), Ok(Codec::Stored));
    assert_eq!(Codec::from_byte(1), Ok(Codec::Zstd));
    assert_eq!(Codec::Zstd.byte(), 1);
    // The recompression pass writes brotli segments: this version reads them.
    assert_eq!(Codec::from_byte(CODEC_BROTLI), Ok(Codec::Brotli));
    assert_eq!(Codec::Brotli.byte(), CODEC_BROTLI);
    assert!(matches!(
        Codec::from_byte(CODEC_JPEG_XL),
        Err(CryptoError::Unsupported(why)) if why.contains("JPEG XL")
    ));
    assert!(matches!(
        Codec::from_byte(200),
        Err(CryptoError::Unsupported(_))
    ));
}

#[test]
fn the_recompression_pass_tries_every_segment_with_its_codec() {
    // No trial on the first segment: noise first does not stop the text after it.
    let mut brotli = Encoder::new(Compression::Recode(Recoding::Brotli));
    assert_eq!(brotli.encode(&noise(4096)).unwrap().codec(), Codec::Stored);
    let Encoded::Brotli(bytes) = brotli.encode(&text(64 * 1024)).unwrap() else {
        panic!("text recompresses with brotli");
    };
    assert!(bytes.len() < 64 * 1024 / 10, "{}", bytes.len());
    assert_eq!(brotli.encode(&[]).unwrap().codec(), Codec::Stored);

    let mut zstd = Encoder::new(Compression::Recode(Recoding::ZstdMax));
    assert_eq!(zstd.encode(&noise(4096)).unwrap().codec(), Codec::Stored);
    let Encoded::Zstd(frame) = zstd.encode(&text(64 * 1024)).unwrap() else {
        panic!("text recompresses with zstd");
    };
    // A level-19 frame is a zstd frame like any other: the upload pass's reader takes it.
    assert_eq!(decompress(&frame, 64 * 1024).unwrap().as_slice(), text(64 * 1024).as_slice());
}

#[test]
fn a_brotli_segment_decompresses_to_exactly_its_length_and_no_further() {
    let plain = text(10_000);
    let mut encoder = Encoder::new(Compression::Recode(Recoding::Brotli));
    let Encoded::Brotli(stream) = encoder.encode(&plain).unwrap() else {
        panic!("text compresses");
    };
    let back = decompress_brotli(&stream, plain.len()).unwrap();
    assert_eq!(back.as_slice(), plain.as_slice());
    for wrong in [plain.len() - 1, plain.len() + 1] {
        assert!(matches!(
            decompress_brotli(&stream, wrong),
            Err(CryptoError::Damaged(_))
        ));
    }
    assert!(matches!(
        decompress_brotli(b"not brotli at all", 100),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn a_brotli_window_holds_its_segment_and_no_more() {
    assert_eq!(brotli_window_bits(0), 16);
    assert_eq!(brotli_window_bits(1), 16);
    assert_eq!(brotli_window_bits(1 << 16), 16);
    assert_eq!(brotli_window_bits((1 << 16) + 1), 17);
    assert_eq!(brotli_window_bits(1 << 20), 20, "a default 1 MiB segment");
    assert_eq!(brotli_window_bits(1 << 24), 24, "the largest segment");
    assert_eq!(brotli_window_bits(1 << 30), 24);
}

// ==== The drive key sealed to members and to the recovery code (crypto::keys) ====

const DRIVE: &str = "drive-7f3a";

/// A tiny Argon2id cost: the tests run in debug builds too.
fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

#[test]
fn a_member_wrap_opens_with_the_members_secret_key_only() {
    let drive_key = DriveKey::generate().unwrap();
    let alice = MemberSecret::generate().unwrap();
    let bob = MemberSecret::generate().unwrap();
    let alice_id = alice.public().id();
    let wrap = MemberWrap::seal(&drive_key, DRIVE, &alice_id, &alice.public()).unwrap();
    assert_eq!(wrap.drive_key_id, drive_key.id());
    assert_eq!(wrap.open(DRIVE, &alice).unwrap(), drive_key);
    assert_eq!(wrap.open(DRIVE, &bob), Err(CryptoError::WrongKey));

    // The secret as the keyring keeps it.
    let again = MemberSecret::from_bytes(*alice.to_bytes());
    assert_eq!(again.public(), alice.public());
    assert_eq!(wrap.open(DRIVE, &again).unwrap(), drive_key);

    let second = MemberWrap::seal(&drive_key, DRIVE, &alice_id, &alice.public()).unwrap();
    assert_ne!(second.ephemeral, wrap.ephemeral, "a fresh ephemeral key per wrap");
    assert_ne!(second.sealed, wrap.sealed);
    assert_eq!(format!("{alice:?}"), "MemberSecret(***)");
}

#[test]
fn a_member_wrap_is_bound_to_its_drive_and_its_member() {
    let drive_key = DriveKey::generate().unwrap();
    let alice = MemberSecret::generate().unwrap();
    let wrap = MemberWrap::seal(&drive_key, DRIVE, "alice-laptop", &alice.public()).unwrap();
    assert_eq!(
        wrap.open("another-drive", &alice),
        Err(CryptoError::WrongKey)
    );
    let mut renamed = wrap.clone();
    renamed.member = String::from("mallory");
    assert_eq!(renamed.open(DRIVE, &alice), Err(CryptoError::WrongKey));
    let mut relabelled = wrap.clone();
    relabelled.drive_key_id = DriveKey::generate().unwrap().id();
    assert_eq!(relabelled.open(DRIVE, &alice), Err(CryptoError::WrongKey));
    // A low-order point instead of a key, either way round.
    let mut low_order = wrap;
    low_order.ephemeral = [0u8; 32];
    assert!(matches!(
        low_order.open(DRIVE, &alice),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        MemberWrap::seal(&drive_key, DRIVE, "alice", &MemberPublic([0u8; 32])),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn member_ids_are_safe_file_names_in_the_bucket() {
    let alice = MemberSecret::generate().unwrap();
    let id = alice.public().id();
    assert_eq!(id.len(), 32);
    assert_eq!(
        member_key_file(&id).unwrap(),
        format!(".azlin/keys/{id}.key")
    );
    let long = "a".repeat(65);
    for bad in ["", "Alice", "../x", "a/b", "recovery", long.as_str()] {
        assert!(member_key_file(bad).is_err(), "{bad}");
        assert!(
            MemberWrap::seal(&DriveKey::generate().unwrap(), DRIVE, bad, &alice.public()).is_err(),
            "{bad}"
        );
    }
    assert_ne!(id, MemberSecret::generate().unwrap().public().id());
}

#[test]
fn a_member_wrap_round_trips_through_its_key_file_in_the_bucket() {
    let bucket = MemBucket::new();
    let drive_key = DriveKey::generate().unwrap();
    let alice = MemberSecret::generate().unwrap();
    let id = alice.public().id();
    let wrap = MemberWrap::seal(&drive_key, DRIVE, &id, &alice.public()).unwrap();
    store_member_wrap(&bucket, &wrap).unwrap();
    assert_eq!(bucket.keys(), vec![format!(".azlin/keys/{id}.key")]);
    let file = String::from_utf8(bucket.object(&bucket.keys()[0]).unwrap()).unwrap();
    assert!(file.contains("\"format\": \"azlin-drive-key\""), "{file}");
    assert!(!file.contains(&crate::crypto::to_hex(drive_key.as_bytes())));
    let loaded = load_member_wrap(&bucket, &id).unwrap();
    assert_eq!(loaded, wrap);
    assert_eq!(loaded.open(DRIVE, &alice).unwrap(), drive_key);

    bucket.set(
        &member_key_file(&id).unwrap(),
        b"{\"format\": \"something else\"}".to_vec(),
    );
    assert!(matches!(
        load_member_wrap(&bucket, &id),
        Err(DriveError::Corrupt { .. })
    ));
    assert!(matches!(
        load_member_wrap(&bucket, "nobody"),
        Err(DriveError::NotFound { .. })
    ));
}

#[test]
fn the_recovery_code_opens_the_recovery_wrap_and_a_wrong_code_does_not() {
    let drive_key = DriveKey::generate().unwrap();
    let code = RecoveryCode::generate().unwrap();
    let wrap = RecoveryWrap::seal(&drive_key, DRIVE, &code, cheap()).unwrap();
    assert_eq!(wrap.drive_key_id, drive_key.id());
    assert_eq!(wrap.open(DRIVE, &code).unwrap(), drive_key);
    let typed = RecoveryCode::parse(&code.to_text()).unwrap();
    assert_eq!(wrap.open(DRIVE, &typed).unwrap(), drive_key);
    assert_eq!(
        wrap.open(DRIVE, &RecoveryCode::generate().unwrap()),
        Err(CryptoError::WrongKey)
    );
    assert_eq!(
        wrap.open("another-drive", &code),
        Err(CryptoError::WrongKey)
    );
    let mut cheaper = wrap.clone();
    cheaper.kdf.memory_kib = 32;
    assert_eq!(
        cheaper.open(DRIVE, &code),
        Err(CryptoError::WrongKey),
        "the cost is bound into the wrap"
    );
    let mut greedy = wrap;
    greedy.kdf.memory_kib = RecoveryKdf::MAX_MEMORY_KIB + 1;
    assert!(matches!(
        greedy.open(DRIVE, &code),
        Err(CryptoError::Unsupported(_))
    ));
}

#[test]
fn the_recovery_wrap_round_trips_through_its_key_file() {
    let bucket = MemBucket::new();
    let drive_key = DriveKey::generate().unwrap();
    let code = RecoveryCode::generate().unwrap();
    let wrap = RecoveryWrap::seal(&drive_key, DRIVE, &code, cheap()).unwrap();
    store_recovery_wrap(&bucket, &wrap).unwrap();
    assert_eq!(bucket.keys(), vec![RECOVERY_KEY_FILE.to_string()]);
    let file = String::from_utf8(bucket.object(RECOVERY_KEY_FILE).unwrap()).unwrap();
    assert!(file.contains("\"algorithm\": \"argon2id\""), "{file}");
    assert!(!file.contains(&crate::crypto::to_hex(drive_key.as_bytes())));
    let loaded = load_recovery_wrap(&bucket).unwrap();
    assert_eq!(loaded, wrap);
    assert_eq!(loaded.open(DRIVE, &code).unwrap(), drive_key);
}

#[test]
fn a_recovery_code_is_26_base32_characters_and_reads_back_as_people_type_it() {
    let code = RecoveryCode::from_bytes([0xA5; 16]);
    let text = code.to_text();
    assert_eq!(text.len(), 30, "{}", text.as_str());
    let groups: Vec<usize> = text.split('-').map(str::len).collect();
    assert_eq!(groups, vec![5, 5, 5, 5, 6]);
    let alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    assert!(text.chars().all(|c| c == '-' || alphabet.contains(c)));
    assert_eq!(
        RecoveryCode::parse(&text).unwrap().as_bytes(),
        code.as_bytes()
    );

    // As people type it: lower case, spaces, O for 0, L for 1.
    let sloppy = text
        .to_lowercase()
        .replace('-', " ")
        .replace('0', "o")
        .replace('1', "l");
    assert_eq!(
        RecoveryCode::parse(&sloppy).unwrap().as_bytes(),
        code.as_bytes()
    );

    let compact: String = text.chars().filter(|c| *c != '-').collect();
    assert!(RecoveryCode::parse(&compact[..25]).is_none(), "one short");
    assert!(RecoveryCode::parse(&format!("{compact}0")).is_none(), "one long");
    assert!(RecoveryCode::parse(&format!("U{}", &compact[1..])).is_none(), "no U");
    // The last character carries two zero bits; another one is a typo.
    let last = compact.chars().last().unwrap();
    let next = alphabet.as_bytes()[alphabet.find(last).unwrap() + 1] as char;
    let typo = format!("{}{next}", &compact[..25]);
    assert!(RecoveryCode::parse(&typo).is_none());

    for _ in 0..100 {
        let code = RecoveryCode::generate().unwrap();
        let back = RecoveryCode::parse(&code.to_text()).unwrap();
        assert_eq!(back.as_bytes(), code.as_bytes());
    }
}

#[test]
fn the_default_recovery_cost_is_argon2id_with_256_mib_and_3_passes() {
    assert_eq!(
        (
            RecoveryKdf::DEFAULT_MEMORY_KIB,
            RecoveryKdf::DEFAULT_ITERATIONS
        ),
        (262_144, 3)
    );
    let kdf = RecoveryKdf::fresh().unwrap();
    assert_eq!(kdf.memory_kib, 256 * 1024);
    assert_ne!(kdf.salt, RecoveryKdf::fresh().unwrap().salt, "a fresh salt");
    assert!(
        RecoveryKdf::with_cost(4, 1, 1).is_err(),
        "Argon2 needs 8 KiB per lane"
    );
    assert_eq!(
        format!("{:?}", RecoveryCode::generate().unwrap()),
        "RecoveryCode(***)"
    );
}

// ==== A share ====

#[test]
fn a_share_key_opens_the_file_it_covers_and_not_another() {
    let drive = DriveKey::generate().unwrap();
    let (a_id, b_id) = (ObjectId::generate().unwrap(), ObjectId::generate().unwrap());
    let (a, a_summary) =
        encrypt(b"shared report", a_id, &drive, &WriteOptions::default()).unwrap();
    let (b, b_summary) =
        encrypt(b"private diary", b_id, &drive, &WriteOptions::default()).unwrap();

    // The owner shares A: its file key, wrapped for the share; never the drive key.
    let share = ShareKey::generate().unwrap();
    let a_key = drive.unwrap_file_key(&a_summary.wrapped_key, &a_id).unwrap();
    let grant = share.wrap_file_key(&a_key, &a_id).unwrap();

    // The recipient holds the share key and the grant.
    let opened = share.unwrap_file_key(&grant, &a_id).unwrap();
    assert_eq!(decrypt(&a, &a_id, &opened).unwrap(), b"shared report");

    // B: the grant names A, B's own wrap belongs to the drive key, A's key is not B's.
    assert!(matches!(
        share.unwrap_file_key(&grant, &b_id),
        Err(CryptoError::Damaged(_))
    ));
    assert_eq!(
        share.unwrap_file_key(&b_summary.wrapped_key, &b_id),
        Err(CryptoError::WrongKey)
    );
    assert_eq!(decrypt(&b, &b_id, &opened), Err(CryptoError::KeyMismatch));
    let share_as_drive_key = DriveKey::from_bytes(*share.as_bytes());
    assert_eq!(
        OpenObject::open_with_drive_key(&b[..], &b_id, &share_as_drive_key).unwrap_err(),
        CryptoError::WrongKey
    );
}
