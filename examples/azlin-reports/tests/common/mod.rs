//! Helpers shared by the integration tests: a seeded generator (the property tests print their seed), and
//! constructors of ids and bodies. Every test file declares `mod common;`.
#![allow(dead_code)]

use azlin_cbor::{Map, Value};
use azlin_reports::{
    AccountRef, Day, DeviceRef, Extras, LinkRef, Reason, ReportBody, ReportId, SignedRecord,
    Subject, Text,
};

/// A seeded generator (splitmix64): the same seed gives the same stream on every machine.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n` (`n` > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// A number in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi - lo + 1)
    }

    /// True once in `n` calls, on average.
    pub fn one_in(&mut self, n: u64) -> bool {
        self.below(n) == 0
    }

    pub fn bytes<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        for b in out.iter_mut() {
            *b = (self.next_u64() >> 24) as u8;
        }
        out
    }

    /// `lo..=hi` random bytes.
    pub fn vec_range(&mut self, lo: u64, hi: u64) -> Vec<u8> {
        let n = self.range(lo, hi) as usize;
        self.vec(n)
    }

    /// A text of `lo..=hi` characters.
    pub fn text_range(&mut self, lo: u64, hi: u64) -> String {
        let n = self.range(lo, hi) as usize;
        self.text(n)
    }

    pub fn vec(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| (self.next_u64() >> 24) as u8).collect()
    }

    pub fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u64) as usize]
    }

    /// Fisher-Yates.
    pub fn shuffle<T>(&mut self, xs: &mut [T]) {
        for i in (1..xs.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            xs.swap(i, j);
        }
    }

    /// A text of `len` characters: printable ASCII, a line break and some letters outside ASCII (they are
    /// longer than one byte, so the byte limits are exercised as well).
    pub fn text(&mut self, len: usize) -> String {
        const EXTRA: [char; 6] = ['\n', '\t', 'ä', 'ß', '漢', '😀'];
        (0..len)
            .map(|_| {
                if self.one_in(12) {
                    self.pick(&EXTRA)
                } else {
                    (b' ' + self.below(95) as u8) as char
                }
            })
            .collect()
    }
}

pub fn day(n: u32) -> Day {
    Day::new(n)
}

pub fn dev(n: u8) -> DeviceRef {
    DeviceRef::from_bytes([n; 16])
}

pub fn link(n: u8) -> LinkRef {
    LinkRef::from_bytes([n; 16])
}

pub fn acct(n: u8) -> AccountRef {
    AccountRef::from_bytes([n; 16])
}

pub fn rid(n: u8) -> ReportId {
    ReportId::from_bytes([n; 16])
}

pub fn text(s: &str) -> Text {
    Text::new(s).expect("a valid text")
}

/// A small valid body about a link.
pub fn link_body(reason: Reason) -> ReportBody {
    ReportBody {
        id: rid(1),
        reason,
        text: None,
        contact: None,
        subject: Subject::Link {
            url: text("https://drive.azlin.io/l/abc123#sharekey"),
            extras: Extras::default(),
        },
        reply: None,
        extras: Extras::default(),
    }
}

pub fn chat_record(n: u8) -> SignedRecord {
    SignedRecord {
        record: vec![n, n.wrapping_add(1), n.wrapping_add(2)],
        author: [n; 32],
        sig: [n.wrapping_add(7); 64],
        extras: Extras::default(),
    }
}

pub fn chat_body(reason: Reason, records: Vec<SignedRecord>) -> ReportBody {
    ReportBody {
        id: rid(2),
        reason,
        text: None,
        contact: None,
        subject: Subject::ChatRecords {
            records,
            extras: Extras::default(),
        },
        reply: None,
        extras: Extras::default(),
    }
}

pub fn member_body(reason: Reason) -> ReportBody {
    ReportBody {
        id: rid(3),
        reason,
        text: None,
        contact: None,
        subject: Subject::Member {
            member: [9; 16],
            account: acct(8),
            extras: Extras::default(),
        },
        reply: None,
        extras: Extras::default(),
    }
}

pub fn file_body(reason: Reason, with_link: bool, note: &str) -> ReportBody {
    ReportBody {
        id: rid(4),
        reason,
        text: None,
        contact: None,
        subject: Subject::File {
            link: if with_link { Some(link(6)) } else { None },
            note: text(note),
            extras: Extras::default(),
        },
        reply: None,
        extras: Extras::default(),
    }
}

/// A random value of the CBOR profile, small (for the unknown fields of a newer writer).
pub fn random_value(rng: &mut Rng, depth: u32) -> Value {
    match rng.below(if depth == 0 { 6 } else { 8 }) {
        0 => Value::Uint(rng.next_u64() >> rng.below(60)),
        1 => Value::Nint(rng.next_u64() >> rng.below(60)),
        2 => Value::Bytes(rng.vec_range(0, 39)),
        3 => Value::Text(rng.text_range(0, 19)),
        4 => Value::Bool(rng.one_in(2)),
        5 => Value::Null,
        6 => Value::Array(
            (0..rng.below(4))
                .map(|_| random_value(rng, depth - 1))
                .collect(),
        ),
        _ => {
            let entries = (0..rng.below(4))
                .map(|i| (Value::Uint(i + 1), random_value(rng, depth - 1)))
                .collect();
            Value::Map(Map::from_entries(entries).expect("distinct keys"))
        }
    }
}

/// Extras with `crit` a random subset of `known` and a few unknown keys from `unknown_from..` ascending.
pub fn random_extras(rng: &mut Rng, known: &[u64], unknown_from: u64) -> Extras {
    let crit: Vec<u64> = known.iter().copied().filter(|_| rng.one_in(6)).collect();
    let mut fields = Vec::new();
    let mut key = unknown_from;
    for _ in 0..rng.below(3) {
        key += rng.range(1, 5);
        fields.push((key, random_value(rng, 2)));
    }
    Extras::new(crit, fields)
}

/// A random VALID body (every subject, optional fields present or absent, extras at every level).
pub fn random_body(rng: &mut Rng) -> ReportBody {
    let reason = rng.pick(&Reason::ALL);
    let text_len = match rng.below(6) {
        0 => 0,
        1 => 2_000,
        _ => rng.range(1, 60) as usize,
    };
    // `text` counts bytes: a text of 2,000 characters may be longer: build it ASCII at the limit.
    let text = match text_len {
        0 => None,
        2_000 => Some(text(&"x".repeat(2_000))),
        n => Some(text(&rng.text(n).chars().take(n).collect::<String>())),
    };
    let contact = if rng.one_in(3) {
        Some(text_of(rng, 1, 40))
    } else {
        None
    };
    let subject = match rng.below(4) {
        0 => Subject::Link {
            url: text_url(rng),
            extras: random_extras(rng, &[1, 2], 3),
        },
        1 => {
            let n = rng.range(1, 20) as usize;
            let records = (0..n)
                .map(|_| SignedRecord {
                    record: rng.vec_range(1, 300),
                    author: rng.bytes::<32>(),
                    sig: rng.bytes::<64>(),
                    extras: random_extras(rng, &[1, 2, 3], 4),
                })
                .collect();
            Subject::ChatRecords {
                records,
                extras: random_extras(rng, &[1, 2], 3),
            }
        }
        2 => Subject::Member {
            member: rng.bytes::<16>(),
            account: AccountRef::from_bytes(rng.bytes::<16>()),
            extras: random_extras(rng, &[1, 2, 3], 4),
        },
        _ => {
            let with_link = rng.one_in(2);
            let note = if with_link && rng.one_in(2) {
                String::new()
            } else {
                rng.text(20).replace('\n', " ")
            };
            let note = if !with_link && note.is_empty() {
                "a note".to_owned()
            } else {
                note
            };
            Subject::File {
                link: if with_link {
                    Some(LinkRef::from_bytes(rng.bytes::<16>()))
                } else {
                    None
                },
                note: Text::new(&note).expect("valid note"),
                extras: random_extras(rng, &[1, 2, 3], 4),
            }
        }
    };
    ReportBody {
        id: ReportId::from_bytes(rng.bytes::<16>()),
        reason,
        text,
        contact,
        subject,
        reply: if rng.one_in(3) {
            Some(rng.vec_range(1, 200))
        } else {
            None
        },
        extras: random_extras(rng, &[1, 2, 3, 4, 5, 6, 7], 8),
    }
}

fn text_of(rng: &mut Rng, lo: usize, hi: usize) -> Text {
    let n = rng.range(lo as u64, hi as u64) as usize;
    let s: String = rng.text(n).chars().take(n).collect();
    // `take(n)` keeps at most `n` characters: at most 4n bytes, far below the contact limit for hi = 40.
    Text::new(&s).expect("a valid text")
}

fn text_url(rng: &mut Rng) -> Text {
    let id: String = (0..rng.range(4, 12))
        .map(|_| (b'a' + rng.below(26) as u8) as char)
        .collect();
    let key: String = (0..rng.range(8, 40))
        .map(|_| (b'A' + rng.below(26) as u8) as char)
        .collect();
    Text::for_field(
        &format!("https://drive.azlin.io/l/{id}#{key}"),
        azlin_reports::Field::Url,
    )
    .expect("a valid url")
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "odd hex length");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("hex digit"))
        .collect()
}

/// The pairs of a map with unsigned-integer keys, to be tampered with.
pub type Pairs = Vec<(u64, Value)>;

/// A CBOR map whose keys are unsigned integers, key 0 (`crit`) allowed: the raw form tests tamper with.
pub fn raw_map(entries: Vec<(u64, Value)>) -> Value {
    Value::Map(
        Map::from_entries(
            entries
                .into_iter()
                .map(|(k, v)| (Value::Uint(k), v))
                .collect(),
        )
        .expect("distinct keys"),
    )
}

/// The encoding of a value under the object limits.
pub fn cbor(v: &Value) -> Vec<u8> {
    azlin_cbor::encode(v, &azlin_cbor::Limits::OBJECT).expect("encodes")
}
