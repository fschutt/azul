//! Check 11 and check 12: replicas that apply the same calls agree (the state hash), no function of
//! the resolver reads a clock or an address, and the mutation guards of the second lane (the four of
//! the brief and the rest of the resolver's) are the ones listed, each with a killer test that exists.
//! Also the hygiene of the six files of lane S16c2: no panicking call, no lenient default, every public
//! item names the plan section it implements and its caller or the later unit it is a seam of.
//!
//! Check 13 (the golden `reg_link` row against the fixture of `azlin-contract`) is written here and
//! ignored until INT-WIRE lands the row's wire form and the fixture (D-W1S16C1, drift row
//! X-DELTA-W1S16C1-17).
//!
//! Implements planning/architecture.md section 1.4 (rules 1, 3 and 4), section 5.12 and section 12
//! (E5).

#![allow(dead_code)]

mod common;

use std::collections::BTreeSet;
use std::fs;

use azlin_links::counters::{count_bytes_served, count_download_complete, count_open};
use azlin_links::resolve::{resolve, Asked, Caller, ResolveCtx, MUTANT_IDS, RESOLVER_GUARDS};
use azlin_links::{
    pause, revoke, unpause, update, Actor, Audience, Change, DeviceToken, LinkKind, LinkRecord,
    LinkRow, Notify, PausedBy, Pauses, PublicLinkId, Rights, Slug, Wrap, MUTANT_GUARDS,
};
use azlin_policy::LinkPolicy;
use azlin_seams::sim::Rng;
use azlin_seams::{AccountId, ByteCount, Hash32, Span, Stamp};
use common::{
    build, creator, groups, key, link_id, log, member, object, opaque, opts, people, plan_text,
    request, root, sealed, space, NOW,
};

/// Seconds in a day.
const DAY_SECS: u64 = 86_400;

/// No link is blocked.
static NO_BLOCKS: BTreeSet<PublicLinkId> = BTreeSet::new();

/// The six files of lane S16c2.
const C2_FILES: [&str; 6] = [
    "resolve",
    "counters",
    "credential",
    "misses",
    "open_log",
    "notify",
];

/// The stamp `secs` seconds after `NOW`.
fn later(secs: u64) -> Stamp {
    NOW.checked_add(Span::secs(secs)).expect("no overflow")
}

/// The row of `link` changed by `change`, read back as a record.
fn with_row(link: &LinkRecord, change: impl FnOnce(&mut LinkRow)) -> LinkRecord {
    let mut row = link.row().clone();
    change(&mut row);
    LinkRecord::from_row(row).expect("a valid row")
}

/// The caller of the replica tests: 0 is nobody, 1 to 3 are members of another account.
fn caller(who: u8) -> Caller {
    if who == 0 {
        Caller::Anonymous
    } else {
        Caller::SignedIn {
            account: AccountId::from_bytes([200; 16]),
            member: member(who),
        }
    }
}

/// The context of a request.
fn ctx(link: &LinkRecord, who: u8, device: u8, secs: u64, human_step: bool) -> ResolveCtx<'static> {
    ResolveCtx {
        asked: Asked::Id(*link.id()),
        now: later(secs),
        caller: caller(who),
        device: Some(DeviceToken::from_bytes([device; 16])),
        blocks: &NO_BLOCKS,
        links_disabled: false,
        served_today: ByteCount::ZERO,
        daily_cap: ByteCount::gib(100),
        password_verified: false,
        human_step,
    }
}

/// One call on a link: a read, or one of the calls that change it.
#[derive(Clone, Debug)]
enum Op {
    Resolve {
        who: u8,
        device: u8,
        secs: u64,
    },
    Open {
        who: u8,
        device: u8,
        ack: bool,
        human: bool,
        secs: u64,
    },
    Bytes(u64),
    Complete {
        bytes: u64,
        secs: u64,
    },
    Pause(u8),
    Unpause(u8),
    Revoke(u64),
    Change(u8, u64),
}

/// A number below `n`.
fn below(rng: &mut Rng, n: u64) -> u64 {
    rng.below(n).expect("a range")
}

/// The call number `step` of a history: reads and writes, drawn from the generator.
fn random_op(rng: &mut Rng) -> Op {
    match below(rng, 9) {
        0 | 1 => Op::Resolve {
            who: below(rng, 4) as u8,
            device: below(rng, 5) as u8,
            secs: below(rng, 6 * DAY_SECS),
        },
        2..=4 => Op::Open {
            who: below(rng, 4) as u8,
            device: below(rng, 5) as u8,
            ack: rng.chance(4, 5),
            human: rng.chance(9, 10),
            secs: below(rng, 6 * DAY_SECS),
        },
        5 => Op::Bytes(below(rng, 300_000)),
        6 => Op::Complete {
            bytes: below(rng, 300_000),
            secs: below(rng, 6 * DAY_SECS),
        },
        7 => match below(rng, 3) {
            0 => Op::Pause(below(rng, 3) as u8),
            1 => Op::Unpause(below(rng, 3) as u8),
            _ => Op::Revoke(below(rng, 6 * DAY_SECS)),
        },
        _ => Op::Change(below(rng, 5) as u8, below(rng, 1_000)),
    }
}

/// The actor with the number `n`: the creator, the reports or the operator.
fn actor(n: u8) -> Actor {
    match n % 3 {
        0 => creator(),
        1 => Actor::Reports,
        _ => Actor::Operator,
    }
}

/// Applies `op` to `link` and describes what came back.
fn apply(link: &mut LinkRecord, op: &Op) -> String {
    match op {
        Op::Resolve { who, device, secs } => {
            let c = ctx(link, *who, *device, *secs, false);
            format!("{:?}", resolve(link, &c))
        }
        Op::Open {
            who,
            device,
            ack,
            human,
            secs,
        } => {
            let c = ctx(link, *who, *device, *secs, *human);
            format!("{:?}", count_open(link, &c, *ack))
        }
        Op::Bytes(n) => {
            count_bytes_served(link, ByteCount::new(*n));
            String::from("bytes")
        }
        Op::Complete { bytes, secs } => {
            format!(
                "{:?}",
                count_download_complete(link, ByteCount::new(*bytes), later(*secs))
            )
        }
        Op::Pause(who) => format!("{:?}", pause(link, &actor(*who))),
        Op::Unpause(who) => format!("{:?}", unpause(link, &actor(*who))),
        Op::Revoke(secs) => format!("{:?}", revoke(link, &creator(), later(*secs))),
        Op::Change(kind, n) => {
            let change = match kind % 5 {
                0 => Change::AllowReshare(n % 2 == 1),
                1 => Change::Watermark(n % 2 == 1),
                2 => Change::MaxDownloads(Some(1 + (n % 6) as u32)),
                3 => Change::BytesCap(Some(ByteCount::kib(1 + (n % 2_000) as u32))),
                _ => Change::Notify(match n % 3 {
                    0 => Notify::Never,
                    1 => Notify::FirstOpen,
                    _ => Notify::EveryOpen,
                }),
            };
            format!(
                "{:?}",
                update(link, change, &creator(), &LinkPolicy::allow_all(), later(0))
            )
        }
    }
}

/// The link of the replica tests: signed-in callers, a log, devices, caps and expiries.
fn replica_link() -> LinkRecord {
    let mut options = opts(Audience::SignedIn);
    options.notify = Notify::EveryOpen;
    options.open_log = true;
    options.max_devices = Some(3);
    options.max_downloads = Some(4);
    options.bytes_cap = Some(ByteCount::mib(1));
    options.expires_after_open = Some(Span::days(3));
    options.idle_expiry = Some(Span::days(2));
    build(options)
}

#[test]
fn two_replicas_applying_the_same_calls_agree() {
    for seed in 0..300u64 {
        let mut rng = Rng::new(seed);
        let history: Vec<Op> = (0..40).map(|_| random_op(&mut rng)).collect();

        let mut first = replica_link();
        let mut second = replica_link();
        for (step, op) in history.iter().enumerate() {
            let a = apply(&mut first, op);
            let b = apply(&mut second, op);
            assert_eq!(a, b, "seed {seed} step {step}: {op:?} answered differently");
            assert_eq!(first, second, "seed {seed} step {step}: the rows differ");
            assert_eq!(
                first.state_hash(),
                second.state_hash(),
                "seed {seed} step {step}: the state hashes differ"
            );
        }
    }
}

#[test]
fn reads_between_the_calls_change_nothing() {
    for seed in 0..200u64 {
        let mut rng = Rng::new(seed ^ 0x5eed);
        let history: Vec<Op> = (0..40).map(|_| random_op(&mut rng)).collect();
        let reads: Vec<Op> = (0..200)
            .map(|_| Op::Resolve {
                who: below(&mut rng, 4) as u8,
                device: below(&mut rng, 5) as u8,
                secs: below(&mut rng, 6 * DAY_SECS),
            })
            .collect();

        let mut quiet = replica_link();
        let mut noisy = replica_link();
        for (step, op) in history.iter().enumerate() {
            apply(&mut quiet, op);
            apply(&mut noisy, op);
            // A scanner resolves the link, again and again, between two of the calls.
            for read in reads.iter().skip(step * 5).take(5) {
                apply(&mut noisy, read);
            }
            assert_eq!(
                quiet.state_hash(),
                noisy.state_hash(),
                "seed {seed} step {step}: a resolve changed the state"
            );
        }
        assert_eq!(quiet, noisy);
    }
}

#[test]
fn the_same_history_run_twice_gives_the_same_hash_and_another_history_another() {
    let mut rng = Rng::new(99);
    let history: Vec<Op> = (0..60).map(|_| random_op(&mut rng)).collect();
    let run = |ops: &[Op]| {
        let mut link = replica_link();
        let answers: Vec<String> = ops.iter().map(|op| apply(&mut link, op)).collect();
        (link.state_hash(), answers)
    };
    assert_eq!(run(&history), run(&history));

    // One more human open of a link that is still open is one more thing the hash has to show.
    let mut link = replica_link();
    let before = link.state_hash();
    let c = ctx(&link, 1, 1, 60, true);
    count_open(&mut link, &c, true).expect("counted");
    assert_ne!(link.state_hash(), before);
}

/// The base of the coverage test: every option set, every column filled.
fn covered_base() -> LinkRecord {
    let mut options = opts(Audience::SignedIn);
    options.kind = LinkKind::FolderSnapshot;
    options.subfolders = true;
    options.password = true;
    options.app_only = true;
    options.not_before = Some(later(10));
    options.expires_at = Some(later(100_000));
    options.expires_after_open = Some(Span::hours(2));
    options.idle_expiry = Some(Span::days(3));
    options.one_time = true;
    options.max_downloads = Some(5);
    options.bytes_cap = Some(ByteCount::mib(1));
    options.max_devices = Some(2);
    options.rights = Rights::PreviewOnly;
    options.allow_upload = true;
    options.comment = true;
    options.allow_reshare = true;
    options.watermark = true;
    options.notify = Notify::FirstOpen;
    options.open_log = true;
    options.title = Some(sealed(1, 40));
    options.message = Some(sealed(2, 60));
    options.slug = Some(Slug::parse("quarterly-report").expect("a slug"));
    let link = build(options);
    with_row(&link, |row| {
        row.keyed_at = later(5);
        row.counters.downloads = 2;
        row.counters.bytes_served = ByteCount::new(1_000);
        row.counters
            .devices_seen
            .insert(DeviceToken::from_bytes([1; 16]).seen_by(&row.id));
        row.counters.first_open_at = Some(later(20));
        row.counters.last_open_at = Some(later(30));
        row.pauses = Pauses::NONE.with(PausedBy::Owner);
    })
}

/// A change of one column of a row.
type Mutation = (&'static str, Box<dyn Fn(&mut LinkRow)>);

fn m(name: &'static str, change: impl Fn(&mut LinkRow) + 'static) -> Mutation {
    (name, Box::new(change))
}

/// The hash of `base` and of each of its mutations: all different.
fn assert_every_mutation_changes_the_hash(base: &LinkRecord, mutations: Vec<Mutation>) {
    let mut seen: BTreeSet<Hash32> = BTreeSet::new();
    seen.insert(base.state_hash());
    let count = mutations.len();
    for (name, change) in mutations {
        let mutated = with_row(base, change);
        let hash = mutated.state_hash();
        assert_ne!(hash, base.state_hash(), "`{name}` is not in the state hash");
        assert!(
            seen.insert(hash),
            "`{name}` gives the hash of another mutation"
        );
    }
    assert_eq!(seen.len(), count + 1);
}

#[test]
fn state_hash_covers_every_column() {
    let base = covered_base();
    let mutations: Vec<Mutation> = vec![
        m("id", |row| row.id = link_id(2)),
        m("space", |row| row.space = space(9)),
        m("created_by", |row| row.created_by = member(9)),
        m("manifest_object", |row| row.manifest_object = object(8)),
        m("grants", |row| row.grants = common::grants(2)),
        m("opaque_record", |row| row.opaque_record = Some(opaque(6))),
        m("request_id", |row| row.request_id = Some(request(9))),
        m("comment_log", |row| row.comment_log = Some(log(9))),
        m("audience", |row| row.options.audience = groups(&[1])),
        m("password", |row| {
            row.options.password = false;
            row.opaque_record = None;
            row.options.slug = None;
        }),
        m("app_only", |row| row.options.app_only = false),
        m("kind", |row| row.options.kind = LinkKind::FolderLive),
        m("subfolders", |row| row.options.subfolders = false),
        m("not_before", |row| row.options.not_before = Some(later(11))),
        m("expires_at", |row| {
            row.options.expires_at = Some(later(100_001))
        }),
        m("expires_after_open", |row| {
            row.options.expires_after_open = Some(Span::hours(3))
        }),
        m("idle_expiry", |row| {
            row.options.idle_expiry = Some(Span::days(4))
        }),
        m("one_time", |row| row.options.one_time = false),
        m("max_downloads", |row| row.options.max_downloads = Some(6)),
        m("bytes_cap", |row| {
            row.options.bytes_cap = Some(ByteCount::mib(2))
        }),
        m("max_devices", |row| row.options.max_devices = Some(3)),
        m("rights", |row| row.options.rights = Rights::Download),
        m("allow_upload", |row| row.options.allow_upload = false),
        m("comment", |row| row.options.comment = false),
        m("allow_reshare", |row| row.options.allow_reshare = false),
        m("watermark", |row| row.options.watermark = false),
        m("notify", |row| row.options.notify = Notify::EveryOpen),
        m("open_log", |row| row.options.open_log = false),
        m("title", |row| row.options.title = Some(sealed(9, 40))),
        m("message", |row| row.options.message = None),
        m("slug", |row| {
            row.options.slug = Some(Slug::parse("other-name").expect("a slug"))
        }),
        m("downloads", |row| row.counters.downloads += 1),
        m("bytes_served", |row| {
            row.counters.bytes_served = ByteCount::new(1_001)
        }),
        m("devices_seen", |row| {
            row.counters
                .devices_seen
                .insert(DeviceToken::from_bytes([2; 16]).seen_by(&row.id));
        }),
        m("first_open_at", |row| {
            row.counters.first_open_at = Some(later(21))
        }),
        m("last_open_at", |row| row.counters.last_open_at = None),
        m("pauses", |row| {
            row.pauses = Pauses::NONE.with(PausedBy::Owner).with(PausedBy::Reports)
        }),
        m("revoked_at", |row| row.revoked_at = Some(later(50))),
        m("created_at", |row| row.created_at = later(1)),
        m("keyed_at", |row| row.keyed_at = later(6)),
    ];
    assert_every_mutation_changes_the_hash(&base, mutations);

    // The wraps and the named people.
    let named = build(opts(people(&[10, 11])));
    let wrap_of = |n: u8, key_n: u8, seal_n: u8| {
        Wrap::new(member(n), key(key_n), sealed(seal_n, 64)).expect("a wrap")
    };
    let added = wrap_of(12, 7, 7);
    let again = wrap_of(10, 7, 8);
    let other = wrap_of(10, 8, 7);
    let mutations: Vec<Mutation> = vec![
        m("a person and a wrap are added", move |row: &mut LinkRow| {
            row.options.audience = people(&[10, 11, 12]);
            row.wraps.insert(member(12), added.clone());
        }),
        m("a wrap is sealed again", move |row: &mut LinkRow| {
            row.wraps.insert(member(10), again.clone());
        }),
        m("a wrap is for another key", move |row: &mut LinkRow| {
            row.wraps.insert(member(10), other.clone());
        }),
        m("a person and a wrap are removed", |row| {
            row.options.audience = people(&[10]);
            row.wraps.remove(&member(11));
        }),
    ];
    assert_every_mutation_changes_the_hash(&named, mutations);

    // The chosen groups.
    let members = build(opts(groups(&[1, 2])));
    let mutations: Vec<Mutation> = vec![
        m("another group", |row| {
            row.options.audience = groups(&[1, 3])
        }),
        m("a group more", |row| {
            row.options.audience = groups(&[1, 2, 3])
        }),
        m("the whole account", |row| {
            row.options.audience = groups(&[])
        }),
    ];
    assert_every_mutation_changes_the_hash(&members, mutations);
    assert_eq!(named.wraps().count(), 2);
}

#[test]
fn state_hash_is_a_function_of_the_row_only() {
    let base = covered_base();
    assert_eq!(base.state_hash(), base.clone().state_hash());
    assert_eq!(base.state_hash(), covered_base().state_hash());

    // Sets have one canonical order: the order the devices were seen in does not show.
    let tokens: Vec<DeviceToken> = (1..=5u8)
        .map(|n| DeviceToken::from_bytes([n; 16]))
        .collect();
    let in_order = with_row(&base, |row| {
        row.counters.devices_seen.clear();
        for token in &tokens {
            row.counters.devices_seen.insert(token.seen_by(&row.id));
        }
    });
    let reversed = with_row(&base, |row| {
        row.counters.devices_seen.clear();
        for token in tokens.iter().rev() {
            row.counters.devices_seen.insert(token.seen_by(&row.id));
        }
    });
    assert_eq!(in_order.state_hash(), reversed.state_hash());

    // The state is a hash of 32 bytes under its own domain: not zero, and a different link is another.
    assert!(!base.state_hash().is_zero());
    assert_ne!(
        base.state_hash(),
        with_row(&base, |row| row.id = link_id(5)).state_hash()
    );
}

/// The text of a source file of lane S16c2.
fn c2_source(name: &str) -> String {
    fs::read_to_string(root().join(format!("src/{name}.rs"))).expect("a source file")
}

/// The code lines of a source: the lines that are not comments.
fn code_lines(text: &str) -> Vec<(usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(n, line)| (n + 1, line.trim_start()))
        .filter(|(_, line)| !line.starts_with("//"))
        .collect()
}

/// The tokens that read a clock, the environment or an address (the same list as the crate's
/// hygiene test, which scans every file; this scan is the lane's own, over its six files).
const CLOCK_ENVIRONMENT_AND_ADDRESS: &[&str] = &[
    "std::time",
    "SystemTime",
    "Instant",
    "now()",
    "elapsed(",
    "std::env",
    "env::var",
    "env!(",
    "option_env!",
    "var_os",
    "current_dir",
    "args()",
    "std::net",
    "IpAddr",
    "SocketAddr",
    "Ipv4Addr",
    "Ipv6Addr",
    "ToSocketAddrs",
    "TcpStream",
    "UdpSocket",
];

/// The tokens that do not belong in library code: panics, lenient defaults, I/O, randomness.
const BANNED: &[&str] = &[
    "unsafe {",
    "unsafe fn",
    "std::fs",
    "std::io",
    "std::thread",
    "std::process",
    "rand::",
    "getrandom",
    "serde",
    ".unwrap()",
    ".expect(",
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
    "unwrap_or_default",
    "unwrap_unchecked",
    "get_unchecked",
    "mem::transmute",
    "println!",
    "eprintln!",
    "dbg!",
    ".ok()",
    "let _ =",
    "map_or(0",
    "unwrap_or(false)",
];

#[test]
fn no_function_reads_a_clock_or_an_address() {
    let mut scanned = 0usize;
    for name in C2_FILES {
        let text = c2_source(name);
        assert!(
            !text.contains("#[cfg(test)]"),
            "{name}: tests live in tests/"
        );
        for (n, line) in code_lines(&text) {
            scanned += 1;
            for banned in CLOCK_ENVIRONMENT_AND_ADDRESS {
                assert!(
                    !line.contains(banned),
                    "src/{name}.rs line {n}: `{banned}` in library code: {line}"
                );
            }
            for banned in BANNED {
                assert!(
                    !line.contains(banned),
                    "src/{name}.rs line {n}: `{banned}` in library code: {line}"
                );
            }
        }
    }
    assert!(scanned > 500, "the scan saw only {scanned} lines");

    // The crate has no function that takes an address: every function of the six files that takes
    // a time takes it as a `Stamp`.
    let all: String = C2_FILES.iter().map(|name| c2_source(name)).collect();
    assert!(all.contains("azlin_seams::mutant!"));
    assert!(!all.contains("fn now"), "no function reads the time itself");
}

/// The ids of the guards in the six files: the string of every `azlin_seams::mutant!("...")`.
fn guards_in_the_six_files() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for name in C2_FILES {
        let text = c2_source(name);
        let code: String = code_lines(&text)
            .into_iter()
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n");
        let mut rest = code.as_str();
        while let Some(at) = rest.find("azlin_seams::mutant!(") {
            let after = rest[at + "azlin_seams::mutant!(".len()..].trim_start();
            let Some(after) = after.strip_prefix('"') else {
                rest = &rest[at + 1..];
                continue;
            };
            match after.find('"') {
                Some(end) => {
                    ids.insert(after[..end].to_string());
                    rest = &after[end..];
                }
                None => break,
            }
        }
    }
    ids
}

/// The text of every test file (`tests/*.rs`).
fn test_files() -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root().join("tests")).expect("tests folder") {
        let entry = entry.expect("a directory entry");
        if entry.path().extension().is_some_and(|e| e == "rs") {
            out.push(fs::read_to_string(entry.path()).expect("test file"));
        }
    }
    out
}

#[test]
fn every_guard_of_the_six_files_is_listed_and_every_listed_id_is_a_guard() {
    let in_source = guards_in_the_six_files();
    let listed: BTreeSet<String> = RESOLVER_GUARDS
        .iter()
        .map(|(id, _)| id.to_string())
        .collect();
    assert_eq!(
        in_source, listed,
        "the guards of the six files are the ids of RESOLVER_GUARDS"
    );
    assert_eq!(
        listed.len(),
        RESOLVER_GUARDS.len(),
        "the ids are unique (one row each)"
    );
    for id in &listed {
        assert!(id.starts_with("links_"), "{id}");
        assert!(
            id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}"
        );
    }
    // The guards of the first lane are not ours, and ours are not theirs.
    for (id, _) in MUTANT_GUARDS {
        assert!(!listed.contains(id), "{id} is lane S16c1's");
    }
    // Every guard has a killer test that exists.
    let tests = test_files();
    for (id, killer) in RESOLVER_GUARDS {
        assert!(
            tests.iter().any(|t| t.contains(&format!("fn {killer}()"))),
            "the killer of {id}: no test {killer}"
        );
    }
}

#[test]
fn the_four_guards_of_the_brief_are_listed_with_their_killers() {
    assert_eq!(
        MUTANT_IDS,
        [
            "links_resolve_counts_open",
            "links_one_time_used_at_resolve",
            "links_max_devices_unbounded",
            "links_credential_ttl_unbounded",
        ]
    );
    let guards: BTreeSet<&str> = RESOLVER_GUARDS.iter().map(|(id, _)| *id).collect();
    for id in MUTANT_IDS {
        assert!(guards.contains(id), "{id} is a guard of the resolver");
    }
}

/// The doc block (the `///` lines) above line `i`, attributes skipped, joined by spaces.
fn doc_above(lines: &[&str], i: usize) -> String {
    let mut doc = Vec::new();
    let mut j = i;
    while j > 0 {
        j -= 1;
        let t = lines[j].trim();
        if let Some(rest) = t.strip_prefix("///") {
            doc.push(rest.trim().to_string());
        } else if t.starts_with("#[") || t.starts_with("#![") {
            continue;
        } else {
            break;
        }
    }
    doc.reverse();
    doc.join(" ")
}

#[test]
fn public_items_of_the_second_lane_cite_a_plan_section_and_a_caller() {
    let mut checked = 0usize;
    for name in C2_FILES {
        let text = c2_source(name);
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let public = line.starts_with("pub ")
                && !line.starts_with("pub use ")
                && !line.starts_with("pub mod ");
            if !public {
                continue;
            }
            let doc = doc_above(&lines, i);
            assert!(
                doc.contains("planning/architecture.md section "),
                "src/{name}.rs line {}: `{}` does not cite a plan section",
                i + 1,
                line.trim()
            );
            assert!(
                doc.contains("Caller:") || doc.contains("Seam:"),
                "src/{name}.rs line {}: `{}` has no `Caller:` or `Seam:` doc line",
                i + 1,
                line.trim()
            );
            checked += 1;
        }
    }
    assert!(checked >= 30, "the scan found only {checked} public items");
}

/// The numbers that follow the word `section`/`sections` in the comments of the six files.
fn cited_sections() -> BTreeSet<String> {
    let mut cited = BTreeSet::new();
    for name in C2_FILES {
        let text = c2_source(name);
        let comments: String = text
            .lines()
            .filter_map(|l| {
                let t = l.trim_start();
                t.strip_prefix("//!")
                    .or_else(|| t.strip_prefix("///"))
                    .or_else(|| t.strip_prefix("//"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let tokens: Vec<&str> = comments.split_whitespace().collect();
        for (i, token) in tokens.iter().enumerate() {
            let word = token.trim_matches(|c: char| !c.is_ascii_alphabetic());
            if word != "section" && word != "sections" {
                continue;
            }
            let mut k = i + 1;
            while let Some(next) = tokens.get(k) {
                let from_digit = next.trim_start_matches(|c: char| !c.is_ascii_digit());
                let run: String = from_digit
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                let number = run.trim_end_matches('.');
                if number.is_empty() {
                    break;
                }
                cited.insert(number.to_string());
                let more = next.ends_with(',') || tokens.get(k + 1) == Some(&"and");
                if !more {
                    break;
                }
                k += if tokens.get(k + 1) == Some(&"and") {
                    2
                } else {
                    1
                };
            }
        }
    }
    cited
}

#[test]
fn every_plan_section_the_six_files_cite_exists_in_the_plan() {
    let cited = cited_sections();
    for expected in ["1.4", "3.9", "5.11", "5.12", "5.13", "5.15", "12"] {
        assert!(
            cited.contains(expected),
            "the six files do not cite section {expected}: {cited:?}"
        );
    }
    let Some(plan) = plan_text() else {
        return;
    };
    let headings: Vec<&str> = plan
        .lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim_start())
        .collect();
    for number in &cited {
        let found = headings.iter().any(|h| {
            h.strip_prefix(number.as_str())
                .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with(". "))
        });
        assert!(
            found,
            "the six files cite section {number}, which is no heading of the plan"
        );
    }
}

#[test]
#[ignore = "S5 fixture not landed: INT-WIRE writes the wire form of reg_link and fixtures/c4/reg_link.json (D-W1S16C1, X-DELTA-W1S16C1-17)"]
fn reg_link_row_matches_azlin_contract_fixture() {
    let path = root().join("../azlin-contract/fixtures/c4/reg_link.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} cannot be read: {error}", path.display()));
    // The columns of the row, as the plan names them (planning/architecture.md section 5.12).
    for column in [
        "link_id",
        "slug",
        "space_id",
        "created_by",
        "kind",
        "manifest_object",
        "grants",
        "follow_latest",
        "opaque_record",
        "wraps",
        "request_id",
        "comment_log",
        "downloads",
        "bytes_served",
        "devices_seen",
        "first_open_at",
        "paused",
        "revoked_at",
        "created_at",
    ] {
        assert!(text.contains(column), "the fixture has no column {column}");
    }
}
