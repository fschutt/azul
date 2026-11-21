//! T21-T25: the release record and the log that holds it.

use std::cell::Cell;
use std::rc::Rc;

use azlin_merkle::{empty_root, leaf_hash, MemStore, Tree};
use azlin_tlog::{
    Arch, Digest, Hash, LeafIndex, LeafStore, Log, LogError, MemRecords, Name, Origin, RecordError,
    RecordStore, ReleaseRecord, SignerId, StoreError, SubjectKind, TreeError, TreeSize, Version,
    MAX_RECORD_BYTES,
};

use crate::common::{hex32, hex_string, record, sha};

/// T21's record, made outside the crate: the digest is SHA-256 of "signed manifest bytes", the signer the
/// first 8 bytes of SHA-256 of 32 bytes of 0x42.
const GOLDEN_RECORD: &str = "azlin-release-v1\nkind=node-image\nname=azlin-node\nversion=1.4.2\nchannel=release\narch=x86_64\ndigest=sha256:81799ee282e5f220301920533d83001eadf89d1ebfe38f676373a13d0c1f7cc0\nsigner=425ed4e4a36b30ea\nsecurity_fix=1\n";
const GOLDEN_LEAF: &str = "1b19675d126d056f89337d5fdc1aa92ebefc8d7c4978073244ad5a19876d157c";

fn golden() -> ReleaseRecord {
    ReleaseRecord {
        kind: SubjectKind::NodeImage,
        name: Name::new("azlin-node").unwrap(),
        version: Version::new("1.4.2").unwrap(),
        arch: Arch::X86_64,
        digest: Digest::of(b"signed manifest bytes"),
        signer: SignerId::of_public_key(&[0x42; 32]),
        security_fix: true,
    }
}

#[test]
fn t21_the_record_text_is_golden_and_canonical() {
    let r = golden();
    assert_eq!(r.to_bytes(), GOLDEN_RECORD.as_bytes());
    assert_eq!(ReleaseRecord::parse(GOLDEN_RECORD.as_bytes()).unwrap(), r);
    assert_eq!(
        r.leaf_hash(),
        Hash(hex32(GOLDEN_LEAF)),
        "the leaf hash hashlib gave"
    );
    assert_eq!(r.leaf_hash(), leaf_hash(GOLDEN_RECORD.as_bytes()));
    // the helpers that make the digest and the signer id
    assert_eq!(
        hex_string(r.digest.as_bytes()),
        "81799ee282e5f220301920533d83001eadf89d1ebfe38f676373a13d0c1f7cc0"
    );
    assert_eq!(r.digest.as_bytes(), &sha(b"signed manifest bytes"));
    assert_eq!(r.signer.to_string(), "425ed4e4a36b30ea");
    assert_eq!(
        r.digest.to_string(),
        "sha256:81799ee282e5f220301920533d83001eadf89d1ebfe38f676373a13d0c1f7cc0"
    );
    assert_eq!(Digest::from_text(&r.digest.to_string()).unwrap(), r.digest);
    assert_eq!(SignerId::from_text("425ed4e4a36b30ea").unwrap(), r.signer);
    // canonical: every record writes one form, and that form parses back to the same bytes
    let kinds = [
        SubjectKind::NodeImage,
        SubjectKind::Worker,
        SubjectKind::BrowserBundle,
        SubjectKind::DesktopApp,
        SubjectKind::PhoneApp,
    ];
    for kind in kinds {
        assert_eq!(SubjectKind::from_text(kind.as_str()), Some(kind));
    }
    for arch in [Arch::X86_64, Arch::Arm64, Arch::Any] {
        assert_eq!(Arch::from_text(arch.as_str()), Some(arch));
    }
    for i in 0..200 {
        let r = record(i);
        let bytes = r.to_bytes();
        let back = ReleaseRecord::parse(&bytes).unwrap();
        assert_eq!(back, r);
        assert_eq!(back.to_bytes(), bytes, "canonical");
        assert!(bytes.len() < 400 && bytes.ends_with(b"\n"));
    }
    // the words of the five kinds and three architectures, as the plan names the releases
    assert_eq!(
        kinds.map(SubjectKind::as_str),
        [
            "node-image",
            "worker",
            "browser-bundle",
            "desktop-app",
            "phone-app"
        ]
    );
}

#[test]
fn t22_the_record_is_strict() {
    let g = GOLDEN_RECORD;
    let digest_hex = "81799ee282e5f220301920533d83001eadf89d1ebfe38f676373a13d0c1f7cc0";
    let swap = |a: &str, b: &str| -> String {
        let mut lines: Vec<&str> = g.split_inclusive('\n').collect();
        let i = lines.iter().position(|l| l.starts_with(a)).unwrap();
        let j = lines.iter().position(|l| l.starts_with(b)).unwrap();
        lines.swap(i, j);
        lines.concat()
    };
    let name_65 = "n".repeat(65);
    let cases: Vec<(&str, String, RecordError)> = vec![
        (
            "an unknown key",
            g.replacen("name=", "nom=", 1),
            RecordError::Line(azlin_tlog::Field::Name),
        ),
        (
            "keys out of order",
            swap("name=", "version="),
            RecordError::Line(azlin_tlog::Field::Name),
        ),
        (
            "the channel before the version",
            swap("version=", "channel="),
            RecordError::Line(azlin_tlog::Field::Version),
        ),
        (
            "a duplicate key",
            g.replacen("version=1.4.2\n", "name=azlin-node\n", 1),
            RecordError::Line(azlin_tlog::Field::Version),
        ),
        (
            "a missing line",
            g.replacen("signer=425ed4e4a36b30ea\n", "", 1),
            RecordError::Line(azlin_tlog::Field::Signer),
        ),
        (
            "the last line missing",
            g.replacen("security_fix=1\n", "", 1),
            RecordError::Line(azlin_tlog::Field::SecurityFix),
        ),
        (
            "a space before the equals sign",
            g.replacen("name=", "name =", 1),
            RecordError::Line(azlin_tlog::Field::Name),
        ),
        (
            "a digest of 63 digits",
            g.replacen(digest_hex, &digest_hex[1..], 1),
            RecordError::Value(azlin_tlog::Field::Digest),
        ),
        (
            "a digest of 65 digits",
            g.replacen(digest_hex, &format!("{digest_hex}0"), 1),
            RecordError::Value(azlin_tlog::Field::Digest),
        ),
        (
            "an upper-case digest",
            g.replacen(digest_hex, &digest_hex.to_uppercase(), 1),
            RecordError::Value(azlin_tlog::Field::Digest),
        ),
        (
            "a digest without its prefix",
            g.replacen("sha256:", "", 1),
            RecordError::Value(azlin_tlog::Field::Digest),
        ),
        (
            "another hash name",
            g.replacen("sha256:", "sha512:", 1),
            RecordError::Value(azlin_tlog::Field::Digest),
        ),
        (
            "a digest with a non-hex digit",
            g.replacen(digest_hex, &format!("g{}", &digest_hex[1..]), 1),
            RecordError::Value(azlin_tlog::Field::Digest),
        ),
        (
            "an unknown kind",
            g.replacen("node-image", "firmware", 1),
            RecordError::Value(azlin_tlog::Field::Kind),
        ),
        (
            "an upper-case kind",
            g.replacen("node-image", "Node-Image", 1),
            RecordError::Value(azlin_tlog::Field::Kind),
        ),
        (
            "an unknown arch",
            g.replacen("x86_64", "riscv64", 1),
            RecordError::Value(azlin_tlog::Field::Arch),
        ),
        (
            "an upper-case name",
            g.replacen("azlin-node", "Azlin-Node", 1),
            RecordError::Value(azlin_tlog::Field::Name),
        ),
        (
            "an empty name",
            g.replacen("name=azlin-node", "name=", 1),
            RecordError::Value(azlin_tlog::Field::Name),
        ),
        (
            "a name of 65 bytes",
            g.replacen("azlin-node", &name_65, 1),
            RecordError::Value(azlin_tlog::Field::Name),
        ),
        (
            "a name with a space",
            g.replacen("azlin-node", "azlin node", 1),
            RecordError::Value(azlin_tlog::Field::Name),
        ),
        (
            "a name with a plus",
            g.replacen("azlin-node", "azlin+node", 1),
            RecordError::Value(azlin_tlog::Field::Name),
        ),
        (
            "a space after the equals sign",
            g.replacen("name=azlin", "name= azlin", 1),
            RecordError::Value(azlin_tlog::Field::Name),
        ),
        (
            "a version with a space",
            g.replacen("1.4.2", "1.4 2", 1),
            RecordError::Value(azlin_tlog::Field::Version),
        ),
        (
            "an empty version",
            g.replacen("version=1.4.2", "version=", 1),
            RecordError::Value(azlin_tlog::Field::Version),
        ),
        (
            "a version of 65 bytes",
            g.replacen("1.4.2", &"1".repeat(65), 1),
            RecordError::Value(azlin_tlog::Field::Version),
        ),
        (
            "channel=dev",
            g.replacen("channel=release", "channel=dev", 1),
            RecordError::NotRelease,
        ),
        (
            "an empty channel",
            g.replacen("channel=release", "channel=", 1),
            RecordError::NotRelease,
        ),
        (
            "channel=Release",
            g.replacen("channel=release", "channel=Release", 1),
            RecordError::NotRelease,
        ),
        (
            "security_fix=2",
            g.replacen("security_fix=1", "security_fix=2", 1),
            RecordError::Value(azlin_tlog::Field::SecurityFix),
        ),
        (
            "security_fix=true",
            g.replacen("security_fix=1", "security_fix=true", 1),
            RecordError::Value(azlin_tlog::Field::SecurityFix),
        ),
        (
            "an empty security_fix",
            g.replacen("security_fix=1", "security_fix=", 1),
            RecordError::Value(azlin_tlog::Field::SecurityFix),
        ),
        (
            "a signer of 15 digits",
            g.replacen("425ed4e4a36b30ea", "425ed4e4a36b30e", 1),
            RecordError::Value(azlin_tlog::Field::Signer),
        ),
        (
            "an upper-case signer",
            g.replacen("425ed4e4a36b30ea", "425ED4E4A36B30EA", 1),
            RecordError::Value(azlin_tlog::Field::Signer),
        ),
        (
            "a trailing line",
            format!("{g}extra=1\n"),
            RecordError::Trailing,
        ),
        (
            "a trailing empty line",
            format!("{g}\n"),
            RecordError::Trailing,
        ),
        (
            "CRLF line ends",
            g.replace('\n', "\r\n"),
            RecordError::Header,
        ),
        (
            "no final newline",
            g.trim_end().to_string(),
            RecordError::NoFinalNewline,
        ),
        ("nothing", String::new(), RecordError::NoFinalNewline),
        (
            "another version of the header",
            g.replacen("azlin-release-v1", "azlin-release-v2", 1),
            RecordError::Header,
        ),
        (
            "a leading empty line",
            format!("\n{g}"),
            RecordError::Header,
        ),
    ];
    assert!(cases.len() >= 30);
    for (what, text, want) in cases {
        assert_eq!(ReleaseRecord::parse(text.as_bytes()), Err(want), "{what}");
    }
    // not UTF-8, and the size limit: 4096 bytes get past the limit (and fail on their form), 4097 do not
    let mut bad = g.as_bytes().to_vec();
    bad[20] = 0xFF;
    assert_eq!(ReleaseRecord::parse(&bad), Err(RecordError::NotUtf8));
    assert_eq!(
        ReleaseRecord::parse(&vec![b'a'; MAX_RECORD_BYTES]),
        Err(RecordError::NoFinalNewline)
    );
    assert_eq!(
        ReleaseRecord::parse(&vec![b'a'; MAX_RECORD_BYTES + 1]),
        Err(RecordError::TooLarge)
    );
    // the edges that are fine: names and versions of 64 bytes and the characters of their grammars
    let edge = g.replacen("azlin-node", &"a".repeat(64), 1).replacen(
        "1.4.2",
        &"1.0+meta_data-RC.1".repeat(3),
        1,
    );
    let parsed = ReleaseRecord::parse(edge.as_bytes()).unwrap();
    assert_eq!(parsed.to_bytes(), edge.as_bytes());
    assert!(Name::new("a.b_c-9").is_ok() && Name::new("A").is_err() && Name::new("a+b").is_err());
    assert!(Version::new("1.2.3-rc.1+build_5").is_ok() && Version::new("1 2").is_err());
    assert!(Digest::from_text("sha256:00").is_err());
}

#[test]
fn t23_the_leaf_hash_binds_every_field() {
    let base = record(3);
    let mut variants: Vec<(&str, ReleaseRecord)> = Vec::new();
    let mut v = base.clone();
    v.kind = SubjectKind::PhoneApp;
    variants.push(("kind", v));
    let mut v = base.clone();
    v.name = Name::new("app-4").unwrap();
    variants.push(("name", v));
    let mut v = base.clone();
    v.version = Version::new("1.3.1").unwrap();
    variants.push(("version", v));
    let mut v = base.clone();
    v.arch = Arch::Any;
    variants.push(("arch", v));
    let mut v = base.clone();
    v.digest = Digest::of(b"another statement");
    variants.push(("digest", v));
    let mut v = base.clone();
    v.signer = SignerId::of_public_key(&[0x43; 32]);
    variants.push(("signer", v));
    let mut v = base.clone();
    v.security_fix = !base.security_fix;
    variants.push(("security_fix", v));
    assert_eq!(
        base.leaf_hash(),
        leaf_hash(&base.to_bytes()),
        "the leaf hash is the Merkle leaf hash of the bytes"
    );
    let mut seen = vec![base.leaf_hash()];
    for (field, changed) in &variants {
        assert_ne!(
            changed.leaf_hash(),
            base.leaf_hash(),
            "{field} is not bound by the leaf hash"
        );
        assert!(!seen.contains(&changed.leaf_hash()), "{field}");
        seen.push(changed.leaf_hash());
    }
    // and the same fields give the same hash, every time
    assert_eq!(record(3).leaf_hash(), base.leaf_hash());
    // distinct releases, distinct leaves: 500 of them
    let mut hashes: Vec<Hash> = (0..500).map(|i| record(i).leaf_hash()).collect();
    hashes.sort();
    hashes.dedup();
    assert_eq!(hashes.len(), 500);
}

fn open() -> Log<MemStore, MemRecords> {
    Log::open(Origin::azlin(), MemStore::new(), MemRecords::new()).unwrap()
}

/// The root of the tree an independent `azlin_merkle::Tree` builds over the bytes of records `0..n`.
fn model_root(n: usize) -> Hash {
    let mut tree = Tree::new(MemStore::new()).unwrap();
    for i in 0..n {
        tree.append(&record(i).to_bytes()).unwrap();
    }
    tree.root().unwrap()
}

#[test]
fn t24_append_gives_stable_indexes() {
    let mut log = open();
    assert_eq!(log.size(), TreeSize(0));
    assert_eq!(log.root().unwrap(), empty_root());
    assert_eq!(log.origin(), &Origin::azlin());
    let ids: Vec<LeafIndex> = (0..3).map(|i| log.append(&record(i)).unwrap()).collect();
    assert_eq!(ids, [LeafIndex(0), LeafIndex(1), LeafIndex(2)]);
    assert_eq!(log.size(), TreeSize(3));
    let root3 = log.root().unwrap();
    assert_eq!(root3, model_root(3));
    // the same record again returns its index and the size stays 3, whichever record and in whatever order
    for i in [1usize, 0, 2, 1] {
        assert_eq!(log.append(&record(i)).unwrap(), LeafIndex(i as u64));
        assert_eq!(log.size(), TreeSize(3));
        assert_eq!(log.root().unwrap(), root3);
    }
    assert_eq!(log.find(&record(2).key()), Some(LeafIndex(2)));
    assert_eq!(log.find(&record(9).key()), None);
    // reopening over the same stores gives the same root and the same book
    let (store, records) = log.into_parts();
    let mut log = Log::open(Origin::azlin(), store, records).unwrap();
    assert_eq!(log.size(), TreeSize(3));
    assert_eq!(log.root().unwrap(), root3);
    assert_eq!(log.find(&record(1).key()), Some(LeafIndex(1)));
    assert_eq!(
        log.append(&record(1)).unwrap(),
        LeafIndex(1),
        "the book is rebuilt from the records"
    );
    assert_eq!(log.size(), TreeSize(3));
    assert_eq!(log.append(&record(3)).unwrap(), LeafIndex(3));
    assert_eq!(log.root().unwrap(), model_root(4));
    // a log of 300 releases: every index is its position
    let mut big = open();
    for i in 0..300 {
        assert_eq!(big.append(&record(i)).unwrap(), LeafIndex(i as u64));
    }
    assert_eq!(big.root().unwrap(), model_root(300));
}

/// A record store that is a plain vector, which the test can damage before it opens a log over it.
struct VecRecords(Vec<Vec<u8>>);

impl RecordStore for VecRecords {
    fn len(&self) -> Result<TreeSize, StoreError> {
        Ok(TreeSize(self.0.len() as u64))
    }

    fn get(&self, index: LeafIndex) -> Result<Vec<u8>, StoreError> {
        self.0
            .get(index.0 as usize)
            .cloned()
            .ok_or(StoreError::IndexOutOfRange)
    }

    fn append(&mut self, bytes: Vec<u8>) -> Result<(), StoreError> {
        self.0.push(bytes);
        Ok(())
    }
}

fn leaves_of(records: &[Vec<u8>]) -> MemStore {
    MemStore::from_leaves(records.iter().map(|r| leaf_hash(r)).collect())
}

#[test]
fn t24_open_checks_the_records_against_the_leaves() {
    let bytes: Vec<Vec<u8>> = (0..4).map(|i| record(i).to_bytes()).collect();
    // the honest stores
    let honest = Log::open(
        Origin::azlin(),
        leaves_of(&bytes),
        VecRecords(bytes.clone()),
    )
    .unwrap();
    assert_eq!(honest.size(), TreeSize(4));
    assert_eq!(honest.root().unwrap(), model_root(4));
    // a record store one record ahead (a crash between the two writes of an append): completed
    let mut ahead = bytes.clone();
    ahead.push(record(4).to_bytes());
    let mut log = Log::open(Origin::azlin(), leaves_of(&bytes), VecRecords(ahead)).unwrap();
    assert_eq!(log.size(), TreeSize(5));
    assert_eq!(log.root().unwrap(), model_root(5));
    assert_eq!(log.find(&record(4).key()), Some(LeafIndex(4)));
    assert_eq!(log.append(&record(4)).unwrap(), LeafIndex(4));
    assert_eq!(log.size(), TreeSize(5));
    // two records ahead, a leaf without its record, a record store that is empty: refused
    let mut two = bytes.clone();
    two.push(record(4).to_bytes());
    two.push(record(5).to_bytes());
    assert_eq!(
        Log::open(Origin::azlin(), leaves_of(&bytes), VecRecords(two)).err(),
        Some(LogError::RecordsMismatch {
            leaves: TreeSize(4),
            records: TreeSize(6)
        })
    );
    assert_eq!(
        Log::open(
            Origin::azlin(),
            leaves_of(&bytes),
            VecRecords(bytes[..3].to_vec())
        )
        .err(),
        Some(LogError::RecordsMismatch {
            leaves: TreeSize(4),
            records: TreeSize(3)
        })
    );
    assert_eq!(
        Log::open(Origin::azlin(), leaves_of(&bytes), VecRecords(vec![])).err(),
        Some(LogError::RecordsMismatch {
            leaves: TreeSize(4),
            records: TreeSize(0)
        })
    );
    // records that are not the ones the leaves hash: swapped, or one changed to another valid record
    let mut swapped = bytes.clone();
    swapped.swap(0, 1);
    assert_eq!(
        Log::open(Origin::azlin(), leaves_of(&bytes), VecRecords(swapped)).err(),
        Some(LogError::RecordMismatch {
            index: LeafIndex(0)
        })
    );
    let mut changed = bytes.clone();
    changed[2] = record(40).to_bytes();
    assert_eq!(
        Log::open(Origin::azlin(), leaves_of(&bytes), VecRecords(changed)).err(),
        Some(LogError::RecordMismatch {
            index: LeafIndex(2)
        })
    );
    // a stored record that is no record at all, even when its leaf hash is right
    let mut garbage = bytes.clone();
    garbage[1] = b"not a release record\n".to_vec();
    assert_eq!(
        Log::open(
            Origin::azlin(),
            leaves_of(&garbage),
            VecRecords(garbage.clone())
        )
        .err(),
        Some(LogError::BadRecord {
            index: LeafIndex(1),
            error: RecordError::Header
        })
    );
    // the same release twice in the stores (an honest log never writes that)
    let mut twin = record(1);
    twin.digest = Digest::of(b"twin");
    let doubled = vec![record(1).to_bytes(), record(2).to_bytes(), twin.to_bytes()];
    assert_eq!(
        Log::open(
            Origin::azlin(),
            leaves_of(&doubled),
            VecRecords(doubled.clone())
        )
        .err(),
        Some(LogError::DuplicateRelease {
            first: LeafIndex(0),
            second: LeafIndex(2)
        })
    );
    // the orphan record may not be a duplicate of a logged release either
    let mut orphan = bytes.clone();
    orphan.push(twin_of(1));
    assert_eq!(
        Log::open(Origin::azlin(), leaves_of(&bytes), VecRecords(orphan)).err(),
        Some(LogError::DuplicateRelease {
            first: LeafIndex(1),
            second: LeafIndex(4)
        })
    );
}

fn twin_of(i: usize) -> Vec<u8> {
    let mut twin = record(i);
    twin.security_fix = !twin.security_fix;
    twin.to_bytes()
}

/// A record store that fails to append while `fail` is set (the flag is shared with the test).
struct FlakyRecords {
    inner: MemRecords,
    fail: Rc<Cell<bool>>,
}

impl RecordStore for FlakyRecords {
    fn len(&self) -> Result<TreeSize, StoreError> {
        self.inner.len()
    }

    fn get(&self, index: LeafIndex) -> Result<Vec<u8>, StoreError> {
        self.inner.get(index)
    }

    fn append(&mut self, bytes: Vec<u8>) -> Result<(), StoreError> {
        if self.fail.get() {
            return Err(StoreError::Unavailable);
        }
        self.inner.append(bytes)
    }
}

/// A leaf store that fails to append while `fail` is set.
struct FlakyLeaves {
    inner: MemStore,
    fail: Rc<Cell<bool>>,
}

impl LeafStore for FlakyLeaves {
    fn size(&self) -> Result<TreeSize, StoreError> {
        self.inner.size()
    }

    fn leaf(&self, index: LeafIndex) -> Result<Hash, StoreError> {
        self.inner.leaf(index)
    }

    fn append(&mut self, hash: Hash) -> Result<(), StoreError> {
        if self.fail.get() {
            return Err(StoreError::Unavailable);
        }
        self.inner.append(hash)
    }
}

#[test]
fn t24_a_failing_store_never_leaves_a_half_written_log() {
    // the record store fails: nothing was appended anywhere, and the log goes on when it recovers
    let fail = Rc::new(Cell::new(false));
    let records = FlakyRecords {
        inner: MemRecords::new(),
        fail: fail.clone(),
    };
    let mut log = Log::open(Origin::azlin(), MemStore::new(), records).unwrap();
    log.append(&record(0)).unwrap();
    let root = log.root().unwrap();
    fail.set(true);
    assert_eq!(
        log.append(&record(1)),
        Err(LogError::Store(StoreError::Unavailable))
    );
    assert_eq!(log.size(), TreeSize(1));
    assert_eq!(log.root().unwrap(), root);
    assert_eq!(log.find(&record(1).key()), None);
    fail.set(false);
    assert_eq!(log.append(&record(1)).unwrap(), LeafIndex(1));
    assert_eq!(log.root().unwrap(), model_root(2));

    // the leaf store fails after the record was written: the log refuses to go on (the stores are one
    // record apart), and opening it over the same stores completes that record
    let fail = Rc::new(Cell::new(false));
    let leaves = FlakyLeaves {
        inner: MemStore::new(),
        fail: fail.clone(),
    };
    let mut log = Log::open(Origin::azlin(), leaves, MemRecords::new()).unwrap();
    log.append(&record(0)).unwrap();
    log.append(&record(1)).unwrap();
    fail.set(true);
    assert!(matches!(
        log.append(&record(2)),
        Err(LogError::Tree(TreeError::Store(StoreError::Unavailable)))
    ));
    fail.set(false);
    assert_eq!(log.size(), TreeSize(2), "the tree did not move");
    assert_eq!(
        log.append(&record(3)),
        Err(LogError::RecordsMismatch {
            leaves: TreeSize(2),
            records: TreeSize(3)
        })
    );
    let (leaves, records) = log.into_parts();
    let mut log = Log::open(Origin::azlin(), leaves, records).unwrap();
    assert_eq!(
        log.size(),
        TreeSize(3),
        "the half-written record is completed"
    );
    assert_eq!(log.root().unwrap(), model_root(3));
    assert_eq!(
        log.append(&record(2)).unwrap(),
        LeafIndex(2),
        "and it is the release that was being written"
    );
    assert_eq!(log.append(&record(3)).unwrap(), LeafIndex(3));
}

#[test]
fn t25_the_same_release_with_other_bytes_is_an_equivocation() {
    let mut log = open();
    log.append(&record(0)).unwrap();
    log.append(&record(1)).unwrap();
    let (size, root) = (log.size(), log.root().unwrap());
    // the same (kind, name, version, arch) with another digest: refused, the tree is unchanged
    let mut tailored = record(1);
    tailored.digest = Digest::of(b"a build made for one company alone");
    assert_eq!(
        log.append(&tailored),
        Err(LogError::Equivocation {
            first: LeafIndex(1)
        })
    );
    assert_eq!((log.size(), log.root().unwrap()), (size, root));
    // any other byte of the record is the same: the signer, the security fix mark
    let mut other_signer = record(1);
    other_signer.signer = SignerId::of_public_key(&[0x99; 32]);
    assert_eq!(
        log.append(&other_signer),
        Err(LogError::Equivocation {
            first: LeafIndex(1)
        })
    );
    let mut other_mark = record(1);
    other_mark.security_fix = !other_mark.security_fix;
    assert_eq!(
        log.append(&other_mark),
        Err(LogError::Equivocation {
            first: LeafIndex(1)
        })
    );
    assert_eq!((log.size(), log.root().unwrap()), (size, root));
    // the exact record again is fine; a change in a field of the release's identity is another release
    assert_eq!(log.append(&record(1)).unwrap(), LeafIndex(1));
    let mut v = record(1);
    v.version = Version::new("9.9.9").unwrap();
    assert_eq!(log.append(&v).unwrap(), LeafIndex(2));
    let mut a = record(1);
    a.arch = if a.arch == Arch::Arm64 {
        Arch::X86_64
    } else {
        Arch::Arm64
    };
    assert_eq!(log.append(&a).unwrap(), LeafIndex(3));
    let mut k = record(1);
    k.kind = SubjectKind::PhoneApp;
    assert_eq!(log.append(&k).unwrap(), LeafIndex(4));
    let mut n = record(1);
    n.name = Name::new("other-name").unwrap();
    assert_eq!(log.append(&n).unwrap(), LeafIndex(5));
    // the refusal survives a reopen: the book is rebuilt from the records
    let (store, records) = log.into_parts();
    let mut log = Log::open(Origin::azlin(), store, records).unwrap();
    assert_eq!(
        log.append(&tailored),
        Err(LogError::Equivocation {
            first: LeafIndex(1)
        })
    );
    assert_eq!(log.size(), TreeSize(6));
}
