//! Check R21 of lane W1-S46a: no report type holds an address, a name or a payer (planning/architecture.md
//! section 3.9: "What can be handed over: pseudonymous account ids, sizes and coarse times ... No IP addresses
//! (never stored, R1)"; decision D37: accounts have no email and no name). Every test enters through the public
//! API of the crate; the source scan reads the crate's own files.

mod common;

use std::collections::BTreeSet;

use azlin_cbor::ShapeError;
use azlin_reports::{
    Action, AutoStep, BodyError, Config, Decision, Facts, Ground, NotifierNotice, Outcome,
    QueueItem, Reason, Redress, ReportBody, ReportEvent, Restriction, SignedRecord,
    StatementOfReasons, Stats, Subject,
};

use common::*;

const FORBIDDEN: [&str; 5] = ["ip", "address", "email", "name", "payer"];

/// Destructures a struct with ALL its fields (no `..`): adding a field breaks the compilation of this test,
/// and the hand-kept list below has to be extended with the new name.
macro_rules! fields {
    ($seen:ident, $ty:ident { $($f:ident),* $(,)? } = $value:expr) => {{
        let $ty { $($f: _),* } = $value;
        $seen.push((stringify!($ty), vec![$(stringify!($f)),*]));
    }};
}

/// The same for a struct-like variant of an enum.
macro_rules! variant {
    ($seen:ident, $name:expr, $ty:path { $($f:ident),* $(,)? } = $value:expr) => {{
        let value = $value;
        let $ty { $($f: _),* } = value else { panic!("{} did not match", $name) };
        $seen.push(($name, vec![$(stringify!($f)),*]));
    }};
}

/// The public types with a private or no named field: ids, units, texts and the engines. Anything that is not
/// in this list and not in `fields!` below is a public struct nobody has looked at.
const OPAQUE: [&str; 10] = [
    "ReportId",
    "LinkRef",
    "AccountRef",
    "DeviceRef",
    "Day",
    "Days",
    "Text",
    "Extras",
    "BlockList",
    "Ledger",
];

#[test]
fn r21_no_report_type_holds_an_address_a_name_or_a_payer() {
    let mut seen: Vec<(&str, Vec<&str>)> = Vec::new();
    let body = link_body(Reason::Csam);
    let record = chat_record(1);
    fields!(
        seen,
        ReportBody {
            id,
            reason,
            text,
            contact,
            subject,
            reply,
            extras
        } = body.clone()
    );
    fields!(
        seen,
        SignedRecord {
            record,
            author,
            sig,
            extras
        } = record
    );
    fields!(
        seen,
        Config {
            link_devices,
            link_window,
            account_devices,
            account_window,
            max_links,
            max_accounts,
            max_reporters_per_key
        } = Config::default()
    );
    fields!(
        seen,
        ReportEvent {
            report,
            link,
            account,
            reporter,
            day
        } = ReportEvent {
            report: rid(1),
            link: None,
            account: acct(1),
            reporter: dev(1),
            day: day(1),
        }
    );
    fields!(
        seen,
        Stats {
            links,
            accounts,
            paused_links,
            disabled_accounts
        } = Stats {
            links: 0,
            accounts: 0,
            paused_links: 0,
            disabled_accounts: 0,
        }
    );
    fields!(
        seen,
        QueueItem {
            id,
            reason,
            received,
            decided
        } = QueueItem {
            id: rid(1),
            reason: Reason::Other,
            received: day(1),
            decided: false,
        }
    );
    let decision = Decision {
        report: rid(1),
        action: Action::NoAction,
        reason: Reason::Other,
        text: None,
        day: day(1),
    };
    fields!(
        seen,
        Decision {
            report,
            action,
            reason,
            text,
            day
        } = decision.clone()
    );
    let statement = StatementOfReasons {
        restriction: Restriction::NoRestriction,
        facts: Facts::Counted { devices: 1 },
        automated: false,
        ground: Ground::Terms,
        redress: Redress::STANDARD,
        day: day(1),
    };
    fields!(
        seen,
        StatementOfReasons {
            restriction,
            facts,
            automated,
            ground,
            redress,
            day
        } = statement
    );
    fields!(
        seen,
        Redress {
            reply_in_app,
            out_of_court,
            courts
        } = Redress::STANDARD
    );
    fields!(
        seen,
        NotifierNotice {
            receipt,
            outcome,
            day
        } = NotifierNotice {
            receipt: rid(1),
            outcome: Outcome::NoAction,
            day: day(1),
        }
    );
    // The struct-like variants.
    variant!(
        seen,
        "Subject::Link",
        Subject::Link { url, extras } = link_body(Reason::Other).subject
    );
    variant!(
        seen,
        "Subject::ChatRecords",
        Subject::ChatRecords { records, extras } =
            chat_body(Reason::Other, vec![chat_record(1)]).subject
    );
    variant!(
        seen,
        "Subject::Member",
        Subject::Member {
            member,
            account,
            extras
        } = member_body(Reason::Other).subject
    );
    variant!(
        seen,
        "Subject::File",
        Subject::File { link, note, extras } = file_body(Reason::Other, true, "x").subject
    );
    variant!(
        seen,
        "Facts::Notice",
        Facts::Notice { reason, text } = Facts::Notice {
            reason: Reason::Other,
            text: None
        }
    );
    variant!(
        seen,
        "Facts::Counted",
        Facts::Counted { devices } = Facts::Counted { devices: 1 }
    );
    variant!(
        seen,
        "AutoStep::PauseLink",
        AutoStep::PauseLink { link, devices } = AutoStep::PauseLink {
            link: link(1),
            devices: 1
        }
    );
    variant!(
        seen,
        "AutoStep::DisableLinks",
        AutoStep::DisableLinks { account, devices } = AutoStep::DisableLinks {
            account: acct(1),
            devices: 1
        }
    );
    variant!(
        seen,
        "BodyError::RecordShape",
        BodyError::RecordShape { index, why } = BodyError::RecordShape {
            index: 0,
            why: ShapeError::BadCrit
        }
    );

    for (ty, names) in &seen {
        assert!(!names.is_empty(), "{ty} has no field in the hand-kept list");
        for name in names {
            assert!(
                !FORBIDDEN.contains(name),
                "{ty} has a field named `{name}`: a report type must not hold an address, a name or a payer"
            );
        }
    }

    // Every `pub struct` of the crate is either in the list above or is one of the opaque types.
    let declared = public_structs();
    let known: BTreeSet<String> = seen
        .iter()
        .filter(|(ty, _)| !ty.contains("::"))
        .map(|(ty, _)| (*ty).to_owned())
        .chain(OPAQUE.iter().map(|s| (*s).to_owned()))
        .collect();
    let missing: Vec<&String> = declared.difference(&known).collect();
    assert!(
        missing.is_empty(),
        "public structs nobody listed in this test: {missing:?}"
    );
    let stale: Vec<&String> = known.difference(&declared).collect();
    assert!(
        stale.is_empty(),
        "this test lists structs the crate does not declare: {stale:?}"
    );
}

/// The names of the `pub struct`s declared in `src/` (comments skipped), the four ids that `opaque_id!` makes
/// included.
fn public_structs() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for source in sources() {
        let lines = code_lines(&source);
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("pub struct ") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                // The macro's own body says `pub struct $name`: not a name.
                if !name.is_empty() {
                    names.insert(name);
                }
            }
            if t == "opaque_id!(" {
                // The invocation is: the doc comment (skipped above), then the name, then `);`.
                let name = lines.get(i + 1).map(|l| l.trim()).unwrap_or_default();
                assert!(
                    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric()),
                    "opaque_id! block: {name}"
                );
                names.insert(name.to_owned());
            }
        }
    }
    names
}

fn sources() -> Vec<String> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("src is readable") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|e| e == "rs") {
            out.push(std::fs::read_to_string(&path).expect("a source file"));
        }
    }
    assert!(
        out.len() >= 8,
        "the scan found only {} source files",
        out.len()
    );
    out
}

fn code_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect()
}

#[test]
fn r21_the_source_declares_no_field_named_ip_address_email_name_or_payer() {
    for source in sources() {
        for line in code_lines(&source) {
            let t = line.trim_start().trim_start_matches("pub ").trim_start();
            for name in FORBIDDEN {
                if let Some(rest) = t.strip_prefix(name) {
                    assert!(
                        !rest.trim_start().starts_with(':'),
                        "a field named `{name}` is declared: {line}"
                    );
                }
            }
        }
    }
}

#[test]
fn r21_the_only_free_personal_text_is_the_contact_and_debug_never_prints_it() {
    let mut body = link_body(Reason::Csam);
    body.contact = Some(text("alice@example.org, +49 170 1234567"));
    body.text = Some(text("free words of the reporter"));
    let printed = format!("{body:?}");
    for leak in ["alice", "example.org", "170", "1234567", "free words"] {
        assert!(!printed.contains(leak), "Debug printed `{leak}`: {printed}");
    }
    // The text types print their length only.
    assert_eq!(
        format!("{:?}", body.contact.as_ref().unwrap()),
        "Text(<34 bytes>)"
    );
    // A decision's and a statement's text are not printed in full by anything but the text type's own Debug,
    // which prints only the length.
    let d = Decision {
        report: rid(1),
        action: Action::Ban,
        reason: Reason::Other,
        text: Some(text("operator note: SECRETWORD")),
        day: day(1),
    };
    assert!(!format!("{d:?}").contains("SECRETWORD"));
}

#[test]
fn r17_the_body_is_decoded_with_decode_secret_and_built_inside_a_secret_value() {
    // The link in a body carries its key: rule R17 of the W1 common rules. The tree a body is decoded into is
    // wiped when it drops, and the tree it is encoded from is wrapped so that it is wiped as well.
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/body.rs")).unwrap();
    let code = code_lines(&source).join("\n");
    assert!(
        code.contains("decode_secret(bytes"),
        "ReportBody::decode must read with decode_secret"
    );
    assert!(
        !code.contains("decode(bytes, "),
        "ReportBody::decode must not read with the plain decode"
    );
    assert!(
        code.contains("SecretValue::new(self.to_value()"),
        "ReportBody::encode must wrap its tree"
    );
    assert!(
        code.contains("SecretValue::new(Value::Bytes(self.encode()"),
        "ReportBody::seal must wrap the plaintext"
    );
}
