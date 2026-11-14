//! Contract 4's tables (planning/architecture.md 2.5, 4.1, 5.12): the constants of `azlin_contract::{native,
//! dbtokens, links, signed}` against themselves and against the golden tables in `fixtures/c4/*.json`, and
//! the digest of the fixtures.
//!
//! The fixtures are written from the constants (`AZ_BLESS=1 cargo test -p azlin-contract c4`), so a
//! constant that changes without its fixture fails here, and the fixture changes the digest the services
//! pin (`iso/AZUL_CONTRACT`). That the constants are what the plan's own tables say is azlin-proto's
//! `contract4` doc-table tests' job: the plan is a file of the private repository.

use std::collections::BTreeSet;
use std::path::PathBuf;

use azlin_contract::dbtokens::{
    access, Access, Database, DbDescriptor, DescriptorDocument, DescriptorType, NodeToken,
    Principal, Roots, TokenScope, Writer, ACCESS, DB_TABLE,
};
use azlin_contract::links::{
    LinkAudience, LinkGroup, LinkKind, LinkNotify, LinkRights, Strength, LINK_OPTIONS,
};
use azlin_contract::native::{
    LimitId, LimitValue, Status, StatusClass, Verb, VerbRange, FEATURES_KNOWN, FLAGS_KNOWN,
    FLAG_IDEMPOTENT, FLAG_ZERO_RTT, KEY_CRIT, REQUEST_LAYOUT, REQUEST_PREFIX_BYTES, RESERVED_RANGE,
    RESPONSE_LAYOUT, RESPONSE_PREFIX_BYTES, RETRY_ATTEMPTS,
};
use azlin_contract::signed::{DomainSource, SIGNED_DOMAINS};
use azlin_contract::{digest_of, fixture, fixtures_digest, CONTRACT_VERSION, FIXTURES};
use serde_json::Value;

// ---- the writer of the fixtures --------------------------------------------------------------------

enum J {
    Str(String),
    Num(u64),
    Arr(Vec<J>),
    Obj(Vec<(&'static str, J)>),
}

fn s(text: &str) -> J {
    J::Str(text.to_string())
}

fn render(j: &J, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth + 1);
    let end = "  ".repeat(depth);
    match j {
        J::Str(t) => out.push_str(&serde_json::to_string(t).expect("a string")),
        J::Num(n) => out.push_str(&n.to_string()),
        J::Arr(items) if items.iter().all(|i| matches!(i, J::Str(_) | J::Num(_))) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                render(item, depth + 1, out);
            }
            out.push(']');
        }
        J::Arr(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad);
                render(item, depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&end);
            out.push(']');
        }
        J::Obj(fields) => {
            out.push_str("{\n");
            for (i, (name, value)) in fields.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&serde_json::to_string(name).expect("a name"));
                out.push_str(": ");
                render(value, depth + 1, out);
                out.push_str(if i + 1 < fields.len() { ",\n" } else { "\n" });
            }
            out.push_str(&end);
            out.push('}');
        }
    }
}

fn blessing() -> bool {
    std::env::var("AZ_BLESS").is_ok_and(|v| v == "1")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn table(name: &str, header: &[&str], rows: Vec<Vec<String>>) -> J {
    J::Obj(vec![
        ("name", s(name)),
        ("header", J::Arr(header.iter().map(|h| s(h)).collect())),
        (
            "rows",
            J::Arr(
                rows.into_iter()
                    .map(|r| J::Arr(r.into_iter().map(J::Str).collect()))
                    .collect(),
            ),
        ),
    ])
}

/// A family's fixture: written when blessing, else held byte for byte to the one compiled into the crate.
fn check(family: &str, section: &str, tables: Vec<J>, documents: Vec<J>) {
    let mut fields = vec![
        ("contract", J::Num(4)),
        ("family", s(family)),
        ("section", s(section)),
        ("tables", J::Arr(tables)),
    ];
    if !documents.is_empty() {
        fields.push(("documents", J::Arr(documents)));
    }
    let mut text = String::new();
    render(&J::Obj(fields), 0, &mut text);
    text.push('\n');
    let file = format!("c4/{family}.json");
    if blessing() {
        let path = fixtures_dir().join(&file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("creates c4/");
        std::fs::write(&path, text).expect("writes the fixture");
        return;
    }
    let committed = fixture(&file).unwrap_or_else(|| panic!("{file} is not in FIXTURES"));
    if committed != text {
        let first = committed
            .lines()
            .zip(text.lines())
            .position(|(a, b)| a != b)
            .map_or_else(|| "the lengths".to_string(), |i| format!("line {}", i + 1));
        panic!(
            "{file} is not what the constants say (first difference: {first}): the contract changed; \
             AZ_BLESS=1 cargo test -p azlin-contract c4 writes it, and the pin and the digest move with it"
        );
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|i| (*i).to_string()).collect()
}

// ---- 2.5: the protocol ---------------------------------------------------------------------------

#[test]
fn the_verbs_are_numbered_by_range_and_position_and_found_by_both_number_and_name() {
    assert_eq!(Verb::ALL.len(), 53);
    let mut per_range: Vec<(u8, u16)> = Vec::new();
    for v in Verb::ALL {
        assert_eq!(Verb::from_number(v.number()), Some(*v), "{}", v.name());
        assert_eq!(Verb::from_name(v.name()), Some(*v), "{}", v.name());
        assert_eq!(
            u8::try_from(v.number() >> 8),
            Ok(v.range().number()),
            "{}: the range",
            v.name()
        );
        let position = v.number() & 0xff;
        assert_ne!(
            position,
            0,
            "{}: position 0 is no verb, an all-zero frame is none",
            v.name()
        );
        // a range's row is counted 1, 2, 3 ... in order
        let expected = per_range
            .iter()
            .filter(|(r, _)| *r == v.range().number())
            .count()
            + 1;
        assert_eq!(
            usize::from(position),
            expected,
            "{}: the position is the place in the row",
            v.name()
        );
        per_range.push((v.range().number(), position));
    }
    let numbers: BTreeSet<u16> = Verb::ALL.iter().map(|v| v.number()).collect();
    let names: BTreeSet<&str> = Verb::ALL.iter().map(|v| v.name()).collect();
    assert_eq!(numbers.len(), Verb::ALL.len(), "two verbs share a number");
    assert_eq!(names.len(), Verb::ALL.len(), "two verbs share a name");
    // the ranges: in order, 0x06 reserved, and the rows of the verbs follow the ranges
    let ranges: Vec<u8> = VerbRange::ALL.iter().map(|r| r.number()).collect();
    assert_eq!(ranges, [0, 1, 2, 3, 4, 5, 7]);
    assert!(!ranges.contains(&RESERVED_RANGE));
    assert!(
        Verb::ALL.windows(2).all(|w| w[0].number() < w[1].number()),
        "the verbs are in the plan's order"
    );
    for n in [
        0x0000u16, 0x000d, 0x0109, 0x020b, 0x030d, 0x0405, 0x0505, 0x0600, 0x0601, 0x0704, 0x0801,
        0xffff,
    ] {
        assert_eq!(Verb::from_number(n), None, "{n:#06x}");
    }
    assert_eq!(Verb::from_name("Hello"), None);
    assert_eq!(Verb::from_name(""), None);
    // the sentences the plan makes about verbs
    assert!(Verb::ShardPut.allows_zero_rtt() && !Verb::HeadCas.allows_zero_rtt());
    assert!(Verb::PackPut.repeats_without_reading() && !Verb::LogAppend.repeats_without_reading());
}

#[test]
fn the_statuses_are_numbered_by_position_and_every_one_has_a_class_but_ok() {
    assert_eq!(Status::ALL.len(), 16);
    for (i, st) in Status::ALL.iter().enumerate() {
        assert_eq!(
            usize::from(st.number()),
            i,
            "{}: the number is the position",
            st.name()
        );
        assert_eq!(Status::from_number(st.number()), Some(*st));
        assert_eq!(Status::from_name(st.name()), Some(*st));
        assert_eq!(
            st.class().is_none(),
            *st == Status::Ok,
            "{}: only ok has no class",
            st.name()
        );
    }
    assert_eq!(Status::from_number(16), None);
    assert_eq!(Status::from_name("OK"), None);
    assert_eq!(StatusClass::NAMES, ["fatal", "retry", "elsewhere", "after"]);
    // every limit's refusal is a status the client can act on
    for id in LimitId::ALL {
        if let Some(st) = id.at_limit() {
            assert!(
                st.class().is_some(),
                "{}: refused with {}",
                id.doc_label(),
                st.name()
            );
        }
    }
    assert_eq!(RETRY_ATTEMPTS, 5);
}

#[test]
fn the_limits_have_distinct_names_and_the_defaults_the_plan_lists() {
    assert_eq!(LimitId::ALL.len(), 11);
    let wire: BTreeSet<&str> = LimitId::ALL.iter().map(|l| l.wire_name()).collect();
    let labels: BTreeSet<&str> = LimitId::ALL.iter().map(|l| l.doc_label()).collect();
    assert_eq!(wire.len(), 11);
    assert_eq!(labels.len(), 11);
    assert_eq!(
        LimitId::ConnectionsPerNode.default_value(),
        LimitValue::Count(100_000)
    );
    assert_eq!(
        LimitId::RequestHeader.default_value(),
        LimitValue::Bytes(16 * 1024)
    );
    assert_eq!(
        LimitId::LogAppendRecord.default_value(),
        LimitValue::Bytes(64 * 1024)
    );
    assert_eq!(
        LimitId::UncompactedLogBytesPerAccount.default_value(),
        LimitValue::Bytes(1 << 30)
    );
    assert_eq!(
        LimitId::BytesInFlightPerConnection.default_value(),
        LimitValue::PathBound {
            max_bytes: 64 << 20
        }
    );
    assert_eq!(
        LimitId::QueuedEventsPerWatcher.default_value(),
        LimitValue::OnePerSpaceKind
    );
}

#[test]
fn the_frame_constants_are_the_plans_layouts() {
    assert_eq!(REQUEST_PREFIX_BYTES, 2 + 2 + 4);
    assert_eq!(RESPONSE_PREFIX_BYTES, 2 + 4);
    assert_eq!(FLAGS_KNOWN, FLAG_IDEMPOTENT | FLAG_ZERO_RTT);
    assert_eq!(FLAG_IDEMPOTENT & FLAG_ZERO_RTT, 0);
    assert_eq!(KEY_CRIT, 0);
    assert!(REQUEST_LAYOUT.starts_with("u16 verb | u16 flags | u32 header length"));
    assert!(RESPONSE_LAYOUT.starts_with("u16 status | u32 header length"));
}

fn native_tables() -> Vec<J> {
    let verbs = Verb::ALL
        .iter()
        .map(|v| {
            vec![
                format!("0x{:04x}", v.number()),
                v.name().to_string(),
                format!("0x{:02x} {}", v.range().number(), v.range().name()),
                (v.number() & 0xff).to_string(),
            ]
        })
        .collect();
    let statuses = Status::ALL
        .iter()
        .map(|st| {
            vec![
                st.number().to_string(),
                st.name().to_string(),
                st.class().map_or("-", |c| c.name()).to_string(),
            ]
        })
        .collect();
    let limits = LimitId::ALL
        .iter()
        .map(|id| {
            let default = match id.default_value() {
                LimitValue::Count(n) => format!("count {n}"),
                LimitValue::Bytes(n) => format!("bytes {n}"),
                LimitValue::PathBound { max_bytes } => {
                    format!("path-bound, at most bytes {max_bytes}")
                }
                LimitValue::OnePerSpaceKind => "one per (space, kind)".to_string(),
            };
            vec![
                id.wire_name().to_string(),
                id.doc_label().to_string(),
                default,
                id.at_limit().map_or("-", |st| st.name()).to_string(),
            ]
        })
        .collect();
    vec![
        table("verbs", &["number", "name", "range", "position"], verbs),
        table("statuses", &["number", "name", "class"], statuses),
        table(
            "limits",
            &["name", "label", "default", "at_the_limit"],
            limits,
        ),
        table(
            "flags",
            &["bit", "name"],
            vec![
                vec![format!("0x{FLAG_IDEMPOTENT:04x}"), "idempotent".into()],
                vec![format!("0x{FLAG_ZERO_RTT:04x}"), "zero_rtt".into()],
            ],
        ),
        table(
            "framing",
            &["frame", "layout", "byte order", "prefix bytes"],
            vec![
                vec![
                    "request".into(),
                    REQUEST_LAYOUT.into(),
                    "big-endian".into(),
                    REQUEST_PREFIX_BYTES.to_string(),
                ],
                vec![
                    "response".into(),
                    RESPONSE_LAYOUT.into(),
                    "big-endian".into(),
                    RESPONSE_PREFIX_BYTES.to_string(),
                ],
            ],
        ),
        table(
            "crit",
            &["key", "meaning"],
            vec![vec![
                KEY_CRIT.to_string(),
                "crit: the header keys the receiver must understand, or refuse".into(),
            ]],
        ),
        table(
            "features",
            &["name"],
            FEATURES_KNOWN
                .iter()
                .map(|f| vec![(*f).to_string()])
                .collect(),
        ),
        table(
            "retries",
            &["attempts", "backoff"],
            vec![vec![RETRY_ATTEMPTS.to_string(), "jittered".into()]],
        ),
    ]
}

#[test]
fn the_native_protocol_fixture_is_what_the_constants_say() {
    check("native_protocol", "2.5", native_tables(), Vec::new());
}

// ---- 4.1 and 4.2: the databases and their tokens -----------------------------------------------------

#[test]
fn the_token_matrix_follows_the_enum_order_and_the_rule_of_4_1() {
    assert_eq!(ACCESS.len(), Principal::ALL.len());
    for (i, p) in Principal::ALL.iter().enumerate() {
        assert_eq!(*p as usize, i, "{p:?}");
    }
    for row in ACCESS {
        assert_eq!(row.len(), Database::ALL.len());
    }
    for (i, d) in Database::ALL.iter().enumerate() {
        assert_eq!(*d as usize, i, "{}", d.name());
    }
    // "No node ever holds a token that writes operator or payment data."
    for db in [
        Database::Ctl,
        Database::Rel,
        Database::Ops,
        Database::Accounts,
        Database::Biz,
    ] {
        for p in [Principal::Node, Principal::Watcher, Principal::HealthProber] {
            assert_ne!(
                access(p, db).token(),
                TokenScope::ReadWrite,
                "{p:?} writes {}",
                db.name()
            );
        }
    }
    // "The payments service never writes operator data."
    for db in [
        Database::Ctl,
        Database::Rel,
        Database::Ops,
        Database::Fleet,
        Database::Tele,
        Database::Bulk,
    ] {
        assert_ne!(
            access(Principal::Payments, db).token(),
            TokenScope::ReadWrite,
            "{}",
            db.name()
        );
    }
    // payer and drive stay in separate databases, each the payments service's alone to write
    for db in [Database::Accounts, Database::Biz] {
        assert_eq!(access(Principal::Payments, db), Access::ReadWrite);
        assert_eq!(access(Principal::AzCtl, db), Access::ReadOnly);
    }
    // "The one token a server holds before approval is the `fleet` token"
    for db in Database::ALL {
        let want = if *db == Database::Fleet {
            TokenScope::ReadWrite
        } else {
            TokenScope::None
        };
        assert_eq!(
            access(Principal::NewServer, *db).token(),
            want,
            "{}",
            db.name()
        );
    }
    // one row per database, and a node token that is a write exactly where the nodes are the writers
    for row in DB_TABLE {
        assert_eq!(DB_TABLE.iter().filter(|r| r.db == row.db).count(), 1);
        let node_writes = access(Principal::Node, row.db).token() == TokenScope::ReadWrite;
        assert_eq!(
            node_writes,
            row.writer == Writer::Nodes,
            "{}",
            row.db.name()
        );
        assert_eq!(
            row.node_token == NodeToken::ReadWriteOwnRows,
            node_writes,
            "{}",
            row.db.name()
        );
    }
}

fn document(entry: impl Fn(&str) -> String) -> String {
    let body: Vec<String> = Database::ALL
        .iter()
        .map(|d| format!("\"{}\": {}", d.name(), entry(d.name())))
        .collect();
    format!("{{{}}}", body.join(", "))
}

fn libsql(name: &str) -> String {
    format!(
        "{{\"type\": \"libsql\", \"url\": \"libsql://{name}.example.net\", \"auth\": {{\"bearer\": \"sealed-{name}\"}}, \"roots\": \"turso\", \"fallback\": [\"libsql://{name}-b.example.net\"]}}"
    )
}

fn parse(json: &str) -> Result<std::collections::BTreeMap<Database, DbDescriptor>, &'static str> {
    match serde_json::from_str::<DescriptorDocument>(json) {
        Ok(doc) => doc.validate().map_err(|e| e.kind()),
        Err(_) => Err("json"),
    }
}

const MIXED_CTL: &str = "{\"type\": \"sqld\", \"url\": \"https://db.example.net:8080\", \"auth\": {\"basic\": {\"user\": \"u\", \"password\": \"sealed-p\"}}, \"roots\": {\"pinned_hash\": \"ab\"}}";
const MIXED_REL: &str = "{\"type\": \"node-local\", \"url\": \"file:/var/lib/azlin/rel.db\"}";

fn mixed() -> String {
    document(|name| match name {
        "ctl" => MIXED_CTL.to_string(),
        "rel" => MIXED_REL.to_string(),
        other => libsql(other),
    })
}

#[test]
fn a_document_of_eight_descriptors_reads_and_each_part_is_typed() {
    let read = parse(&document(libsql)).expect("reads");
    assert_eq!(read.len(), 8);
    let ctl = &read[&Database::Ctl];
    assert_eq!(ctl.kind, DescriptorType::Libsql);
    assert_eq!(ctl.url, "libsql://ctl.example.net");
    assert_eq!(ctl.roots, Some(Roots::Turso));
    assert_eq!(ctl.fallback, ["libsql://ctl-b.example.net"]);
    let read = parse(&mixed()).expect("reads");
    assert_eq!(read[&Database::Ctl].kind, DescriptorType::Sqld);
    assert_eq!(
        read[&Database::Ctl].roots,
        Some(Roots::PinnedHash("ab".into()))
    );
    assert_eq!(read[&Database::Rel].kind, DescriptorType::NodeLocal);
    assert!(read[&Database::Rel].auth.is_none() && read[&Database::Rel].fallback.is_empty());
}

#[test]
fn a_document_that_is_not_all_eight_typed_descriptors_is_refused() {
    let one = |db: &str, with: &str| {
        document(|name| {
            if name == db {
                with.to_string()
            } else {
                libsql(name)
            }
        })
    };
    let seven: Vec<String> = Database::ALL[..7]
        .iter()
        .map(|d| format!("\"{}\": {}", d.name(), libsql(d.name())))
        .collect();
    assert_eq!(
        parse(&format!("{{{}}}", seven.join(", "))),
        Err("missing_database")
    );
    assert_eq!(
        parse(&document(libsql).replace("\"bulk\"", "\"zzz\"")),
        Err("unknown_database")
    );
    // a name twice (serde's map would keep the last)
    let twice = document(libsql).replacen(
        "{\"ctl\":",
        "{\"ctl\": {\"type\": \"libsql\", \"url\": \"libsql://x\", \"auth\": {\"bearer\": \"t\"}, \"roots\": \"system\"}, \"ctl\":",
        1,
    );
    assert_eq!(parse(&twice), Err("json"));
    for bad in ["not json", "[]", "{\"ctl\": 1}"] {
        assert_eq!(parse(bad), Err("json"), "{bad}");
    }
    let libsql_with = |url: &str| {
        format!("{{\"type\": \"libsql\", \"url\": \"{url}\", \"auth\": {{\"bearer\": \"t\"}}, \"roots\": \"system\"}}")
    };
    let extra = "{\"type\": \"libsql\", \"url\": \"libsql://a\", \"auth\": {\"bearer\": \"t\"}, \"roots\": \"system\", \"extra\": 1}";
    assert_eq!(parse(&one("ctl", extra)), Err("json"));
    let postgres = "{\"type\": \"postgres\", \"url\": \"libsql://a\", \"auth\": {\"bearer\": \"t\"}, \"roots\": \"system\"}";
    assert_eq!(parse(&one("ctl", postgres)), Err("json"));
    for url in ["http://a", "libsql://", "https://", "ftp://a", "a", ""] {
        assert_eq!(
            parse(&one("ctl", &libsql_with(url))),
            Err("bad_url"),
            "{url}"
        );
    }
    assert_eq!(
        parse(&one(
            "ctl",
            "{\"type\": \"node-local\", \"url\": \"libsql://a\"}"
        )),
        Err("bad_url")
    );
    assert_eq!(
        parse(&one(
            "ctl",
            "{\"type\": \"node-local\", \"url\": \"file:\"}"
        )),
        Err("bad_url")
    );
    assert_eq!(
        parse(&one(
            "ctl",
            "{\"type\": \"libsql\", \"url\": \"libsql://a\", \"roots\": \"system\"}"
        )),
        Err("auth_roots")
    );
    assert_eq!(
        parse(&one(
            "ctl",
            "{\"type\": \"libsql\", \"url\": \"libsql://a\", \"auth\": {\"bearer\": \"t\"}}"
        )),
        Err("auth_roots")
    );
    assert_eq!(
        parse(&one(
            "ctl",
            "{\"type\": \"node-local\", \"url\": \"file:/x\", \"roots\": \"system\"}"
        )),
        Err("auth_roots")
    );
    assert_eq!(
        parse(&one("ctl", "{\"type\": \"libsql\", \"url\": \"libsql://a\", \"auth\": {\"bearer\": \"t\"}, \"roots\": \"system\", \"fallback\": [\"http://b\"]}")),
        Err("bad_fallback")
    );
}

#[test]
fn the_db_tokens_fixture_is_what_the_constants_say() {
    let databases = DB_TABLE
        .iter()
        .map(|r| {
            vec![
                r.db.name().to_string(),
                r.writer.name().to_string(),
                r.node_token.name().to_string(),
            ]
        })
        .collect();
    let mut header = vec!["holder"];
    header.extend(Database::ALL.iter().map(|d| d.name()));
    let tokens = Principal::ALL
        .iter()
        .map(|p| {
            let mut row = vec![p.doc_label().to_string()];
            row.extend(
                Database::ALL
                    .iter()
                    .map(|d| access(*p, *d).label().to_string()),
            );
            row
        })
        .collect();
    let missing: Vec<String> = Database::ALL[..7]
        .iter()
        .map(|d| format!("\"{}\": {}", d.name(), libsql(d.name())))
        .collect();
    let no_auth = document(|name| {
        if name == "ctl" {
            "{\"type\": \"libsql\", \"url\": \"libsql://a\", \"roots\": \"system\"}".to_string()
        } else {
            libsql(name)
        }
    });
    let cases: Vec<(&'static str, String, Option<&'static str>)> = vec![
        ("eight_libsql_connections", document(libsql), None),
        ("mixed_types", mixed(), None),
        (
            "a_database_missing",
            format!("{{{}}}", missing.join(", ")),
            Some("missing_database"),
        ),
        (
            "an_unknown_database",
            document(libsql).replace("\"bulk\"", "\"zzz\""),
            Some("unknown_database"),
        ),
        ("a_connection_without_auth", no_auth, Some("auth_roots")),
    ];
    for (name, json, error) in &cases {
        assert_eq!(parse(json).map(|_| ()).err(), *error, "{name}");
    }
    let documents = cases
        .into_iter()
        .map(|(name, json, error)| {
            let mut fields = vec![("name", s(name)), ("json", J::Str(json))];
            if let Some(e) = error {
                fields.push(("error", s(e)));
            }
            J::Obj(fields)
        })
        .collect();
    check(
        "db_tokens",
        "4.1",
        vec![
            table(
                "databases",
                &["database", "writer", "node_token"],
                databases,
            ),
            table("tokens", &header, tokens),
        ],
        documents,
    );
}

// ---- 5.12: the link options -----------------------------------------------------------------------

#[test]
fn the_link_options_are_distinct_and_the_enumerations_agree_with_the_strengths() {
    let mut names: Vec<&str> = LINK_OPTIONS
        .iter()
        .flat_map(|r| r.options.iter().copied())
        .collect();
    assert_eq!(names.len(), 25, "the plan's rows hold 25 backticked names");
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 25, "an option twice");
    assert_eq!(LINK_OPTIONS.len(), 21);
    for r in LINK_OPTIONS {
        assert!(!r.options.is_empty() && !r.strength.is_empty());
        assert!(!r.choice || r.options.len() == 2, "a choice has two values");
    }
    // groups appear in the plan's order, each group's rows together
    let mut order: Vec<LinkGroup> = Vec::new();
    for r in LINK_OPTIONS {
        if order.last() != Some(&r.group) {
            assert!(
                !order.contains(&r.group),
                "{}: its rows are not together",
                r.group.name()
            );
            order.push(r.group);
        }
    }
    assert_eq!(order, LinkGroup::ALL);
    // audience: five values, two strengths, the plan's split
    assert_eq!(
        LinkAudience::NAMES,
        ["anyone", "signed_in", "members", "people", "approval"]
    );
    assert_eq!(LinkAudience::Anyone.strength(), Strength::Key);
    assert_eq!(LinkAudience::SignedIn.strength(), Strength::Server);
    let audience = LINK_OPTIONS
        .iter()
        .find(|r| r.options == ["audience"])
        .expect("audience");
    for a in LinkAudience::ALL {
        assert!(audience.strength.contains(&a.strength()), "{}", a.name());
    }
    assert_eq!(
        LinkKind::NAMES,
        [
            "file",
            "selection",
            "folder_snapshot",
            "folder_live",
            "version"
        ]
    );
    assert_eq!(LinkRights::NAMES, ["download", "preview_only"]);
    assert_eq!(LinkNotify::NAMES, ["never", "first_open", "every_open"]);
    assert_eq!(Strength::NAMES, ["key", "server", "app"]);
}

#[test]
fn the_link_options_fixture_is_what_the_constants_say() {
    let options = LINK_OPTIONS
        .iter()
        .map(|r| {
            vec![
                r.group.name().to_string(),
                r.options.join(", "),
                if r.choice { "choice" } else { "each" }.to_string(),
                r.strength
                    .iter()
                    .map(|x| x.name())
                    .collect::<Vec<_>>()
                    .join(" + "),
            ]
        })
        .collect();
    let enumerated = |option: &str, names: &[&str]| vec![option.to_string(), names.join(", ")];
    let values = vec![
        enumerated("audience", LinkAudience::NAMES),
        enumerated("kind", LinkKind::NAMES),
        enumerated("rights", LinkRights::NAMES),
        enumerated("notify", LinkNotify::NAMES),
    ];
    let audiences = LinkAudience::ALL
        .iter()
        .map(|a| vec![a.name().to_string(), a.strength().name().to_string()])
        .collect();
    check(
        "link_options",
        "5.12",
        vec![
            table(
                "strengths",
                &["name"],
                Strength::NAMES.iter().map(|n| strings(&[*n])).collect(),
            ),
            table(
                "groups",
                &["name"],
                LinkGroup::NAMES.iter().map(|n| strings(&[*n])).collect(),
            ),
            table(
                "options",
                &["group", "options", "kind", "strength"],
                options,
            ),
            table("values", &["option", "values"], values),
            table("audience_strengths", &["audience", "strength"], audiences),
        ],
        Vec::new(),
    );
}

// ---- 2.5: the signed structures -------------------------------------------------------------------

#[test]
fn the_domain_strings_are_distinct_versioned_and_the_plans_come_first() {
    let domains: BTreeSet<&str> = SIGNED_DOMAINS.iter().map(|d| d.domain).collect();
    assert_eq!(domains.len(), SIGNED_DOMAINS.len(), "a domain twice");
    for d in SIGNED_DOMAINS {
        assert!(d.domain.starts_with("azlin-"), "{}", d.domain);
        let version = d.domain.rsplit('-').next().expect("a suffix");
        assert!(
            version.len() >= 2
                && version.starts_with('v')
                && version[1..].bytes().all(|b| b.is_ascii_digit()),
            "{}: a domain ends in its version",
            d.domain
        );
        assert!(
            d.domain
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{}",
            d.domain
        );
    }
    let first_proposed = SIGNED_DOMAINS
        .iter()
        .position(|d| d.source == DomainSource::Proposed)
        .expect("proposals");
    assert!(SIGNED_DOMAINS[..first_proposed]
        .iter()
        .all(|d| d.source == DomainSource::Plan));
    assert!(SIGNED_DOMAINS[first_proposed..]
        .iter()
        .all(|d| d.source == DomainSource::Proposed));
    assert_eq!(first_proposed, 7, "2.5 lists seven");
}

#[test]
fn the_signed_domains_fixture_is_what_the_constants_say() {
    let rows = SIGNED_DOMAINS
        .iter()
        .map(|d| {
            vec![
                d.domain.to_string(),
                d.structure.to_string(),
                match d.source {
                    DomainSource::Plan => "plan",
                    DomainSource::Proposed => "proposed",
                }
                .to_string(),
            ]
        })
        .collect();
    check(
        "signed_domains",
        "2.5",
        vec![table("domains", &["domain", "structure", "source"], rows)],
        Vec::new(),
    );
}

// ---- the fixtures and their digest ----------------------------------------------------------------

fn c4() -> Vec<(&'static str, Value)> {
    FIXTURES
        .iter()
        .filter(|(name, _)| name.starts_with("c4/"))
        .map(|(name, text)| {
            (
                *name,
                serde_json::from_str(text).expect("a c4 fixture is JSON"),
            )
        })
        .collect()
}

#[test]
fn this_is_contract_four_and_every_c4_fixture_is_listed_and_shaped_alike() {
    assert_eq!(CONTRACT_VERSION, 4);
    let listed: Vec<&str> = c4().iter().map(|(n, _)| *n).collect();
    assert_eq!(
        listed,
        [
            "c4/db_tokens.json",
            "c4/link_options.json",
            "c4/native_protocol.json",
            "c4/signed_domains.json"
        ]
    );
    assert_eq!(
        fixture("c4/native_protocol.json"),
        FIXTURES
            .iter()
            .find(|(n, _)| *n == "c4/native_protocol.json")
            .map(|(_, t)| *t)
    );
    assert_eq!(fixture("c4/none.json"), None);
    for (name, doc) in c4() {
        let family = name.trim_start_matches("c4/").trim_end_matches(".json");
        assert_eq!(doc["family"], family, "{name}");
        assert_eq!(doc["contract"], 4, "{name}");
        assert!(doc["section"].is_string(), "{name}");
        for t in doc["tables"].as_array().expect("tables") {
            let width = t["header"].as_array().expect("header").len();
            assert!(width > 0);
            let rows = t["rows"].as_array().expect("rows");
            assert!(!rows.is_empty(), "{name}: {} has no rows", t["name"]);
            for row in rows {
                assert_eq!(
                    row.as_array().expect("row").len(),
                    width,
                    "{name}: {}",
                    t["name"]
                );
            }
        }
    }
}

/// C5: the digest moves with every byte of every fixture, and with their names.
#[test]
fn the_digest_changes_when_any_golden_byte_changes() {
    let base = fixtures_digest();
    assert_eq!(digest_of(FIXTURES), base);
    for (i, (name, text)) in FIXTURES.iter().enumerate() {
        let (name, text) = (*name, *text);
        if !name.starts_with("c4/") {
            continue;
        }
        // flip one character of the file (the first digit or letter after the first quote)
        let mut bytes = text.as_bytes().to_vec();
        let spot = bytes
            .iter()
            .position(|b| b.is_ascii_alphanumeric())
            .expect("a character");
        bytes[spot] = if bytes[spot] == b'0' { b'1' } else { b'0' };
        let changed = String::from_utf8(bytes).expect("ascii");
        let mut copy: Vec<(&str, &str)> = FIXTURES.to_vec();
        copy[i] = (name, changed.as_str());
        assert_ne!(
            digest_of(&copy),
            base,
            "{name}: a changed byte left the digest alone"
        );
        let mut renamed: Vec<(&str, &str)> = FIXTURES.to_vec();
        renamed[i] = ("c4/other.json", text);
        assert_ne!(digest_of(&renamed), base, "{name}: renamed");
        let mut dropped = FIXTURES.to_vec();
        dropped.remove(i);
        assert_ne!(digest_of(&dropped), base, "{name}: dropped");
    }
    // line endings as git may check them out on Windows do not change it
    let crlf: Vec<(String, String)> = FIXTURES
        .iter()
        .map(|(n, t)| ((*n).to_string(), t.replace('\n', "\r\n")))
        .collect();
    let borrowed: Vec<(&str, &str)> = crlf.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    assert_eq!(digest_of(&borrowed), base);
}
