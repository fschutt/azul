//! Helpers shared by the checks: keys from fixed seeds, records, a world of a log and reference witnesses,
//! a seeded generator.

#![allow(dead_code)]

use azlin_tlog::{
    Arch, Checkpoint, ConsistencyProof, Digest, Expect, KeyName, Log, LogKey, MemRecords, MemStore,
    MemoryKey, Name, Note, NoteSig, Origin, Policy, ReleaseRecord, SigKind, SignerId, Span,
    SubjectKind, TreeSize, UnixTime, Verifier, Version, Witness,
};
use sha2::{Digest as _, Sha256};

pub fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_decode(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0, "odd hex length");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

pub fn hex32(s: &str) -> [u8; 32] {
    hex_decode(s).try_into().expect("64 hex digits")
}

pub fn kname(s: &str) -> KeyName {
    KeyName::new(s).unwrap()
}

pub fn key(name: &str, seed: u8) -> MemoryKey {
    MemoryKey::from_seed(kname(name), [seed; 32])
}

pub fn t(secs: u64) -> UnixTime {
    UnixTime::from_secs(secs)
}

/// The name of the log key of the goldens (the origin is `azlin.io/release-log/v1`).
pub const LOG_KEY_NAME: &str = "azlin.io/release-log";

/// The log key of every check: name `azlin.io/release-log`, seed `[7; 32]`.
pub fn log_key() -> MemoryKey {
    key(LOG_KEY_NAME, 7)
}

/// Witness `i` (0-based): name `witness.example/w<i+1>`, seed `[9 + 2i; 32]` (so `w1` is seed 9 and `w2` seed 11,
/// as the goldens have them).
pub fn witness_key(i: usize) -> MemoryKey {
    key(&format!("witness.example/w{}", i + 1), 9 + 2 * i as u8)
}

/// The `i`-th release of a family of distinct releases.
pub fn record(i: usize) -> ReleaseRecord {
    let kinds = [
        SubjectKind::NodeImage,
        SubjectKind::Worker,
        SubjectKind::BrowserBundle,
        SubjectKind::DesktopApp,
        SubjectKind::PhoneApp,
    ];
    let arches = [Arch::X86_64, Arch::Arm64, Arch::Any];
    ReleaseRecord {
        kind: kinds[i % 5],
        name: Name::new(&format!("app-{i}")).unwrap(),
        version: Version::new(&format!("1.{i}.0")).unwrap(),
        arch: arches[i % 3],
        digest: Digest::of(format!("release {i}").as_bytes()),
        signer: SignerId::of_public_key(&[0x42; 32]),
        security_fix: i.is_multiple_of(7),
    }
}

/// A log of Azlin's origin signed by [`log_key`], and `witnesses` reference witnesses that know that log.
pub struct World {
    pub origin: Origin,
    pub log_key: MemoryKey,
    pub log: Log<MemStore, MemRecords>,
    pub witnesses: Vec<Witness>,
    pub verifiers: Vec<Verifier>,
    pub log_vkey: LogKey,
}

impl World {
    pub fn new(witnesses: usize) -> World {
        let origin = Origin::azlin();
        let log_key = log_key();
        let log_vkey = LogKey {
            origin: origin.clone(),
            verifier: log_key.verifier(SigKind::Ed25519),
        };
        let mut ws = Vec::new();
        let mut vs = Vec::new();
        for i in 0..witnesses {
            let k = witness_key(i);
            vs.push(k.verifier(SigKind::Cosignature));
            ws.push(Witness::new(Box::new(k), vec![log_vkey.clone()]).unwrap());
        }
        World {
            origin: origin.clone(),
            log_key,
            log: Log::open(origin, MemStore::new(), MemRecords::new()).unwrap(),
            witnesses: ws,
            verifiers: vs,
            log_vkey,
        }
    }

    /// Appends `record(size)`, `record(size + 1)`, ... until the log has `size` leaves.
    pub fn grow_to(&mut self, size: usize) {
        while (self.log.size().0 as usize) < size {
            let i = self.log.size().0 as usize;
            self.log.append(&record(i)).unwrap();
        }
    }

    /// The checkpoint of the log as it is now, with the log's signature only.
    pub fn note(&self) -> Note {
        self.log.checkpoint(&self.log_key, vec![]).unwrap()
    }

    /// Witness `w` cosigns `note` at `time`, asked with a consistency proof from the checkpoint it holds.
    pub fn cosign(&mut self, w: usize, note: &Note, time: u64) -> NoteSig {
        let old = self.witnesses[w].head(&self.origin).unwrap().0;
        let request = self.log.add_request(old, note).unwrap();
        self.witnesses[w].add_checkpoint(&request, t(time)).unwrap()
    }

    /// `note` with the cosignatures of the witnesses `at` (witness index, time).
    pub fn cosigned(&mut self, note: &Note, at: &[(usize, u64)]) -> Note {
        let mut out = note.clone();
        for &(w, time) in at {
            let sig = self.cosign(w, note, time);
            out = out.with_sig(sig).unwrap();
        }
        out
    }

    /// The policy of this world's log and witnesses with `quorum` and 600 seconds of slack.
    pub fn policy(&self, quorum: u8) -> Policy {
        Policy::new(
            self.log_vkey.clone(),
            self.verifiers.clone(),
            quorum,
            Span::from_secs(600),
        )
        .unwrap()
    }
}

/// SplitMix64: a seeded generator for the property tests (the seed is in every failure message).
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// A number below `n` as an index (`n > 0`).
    pub fn pick(&mut self, n: usize) -> usize {
        self.below(n as u64) as usize
    }

    /// True with probability `num / den`.
    pub fn chance(&mut self, num: u64, den: u64) -> bool {
        self.below(den) < num
    }
}

/// The size of the tree the checkpoint in `note` is for.
pub fn size_of(note: &Note) -> TreeSize {
    Checkpoint::parse_text(note.text()).unwrap().size()
}

/// The consistency proof the log of `world` makes from `old` to the checkpoint in `note`.
pub fn consistency(world: &World, old: u64, note: &Note) -> ConsistencyProof {
    let request = world.log.add_request(TreeSize(old), note).unwrap();
    ConsistencyProof {
        old: TreeSize(old),
        new: size_of(note),
        path: request.proof().to_vec(),
    }
}

/// What a client that wants release `record(i)` expects of a bundle.
pub fn expect_of(i: usize) -> Expect {
    let r = record(i);
    Expect {
        kind: r.kind,
        name: r.name,
        version: r.version,
        arch: r.arch,
        digest: Some(r.digest),
    }
}
