//! Check 1: `Policy::defaults(kind)` against the plan's own table (planning/architecture.md section
//! 5.10). The test reads the document, finds the table, reads every row (the four-value rows, the
//! single-value rows, and the rows with a special cell) and compares each cell with the default of
//! its kind. A copy of the document with one cell edited, a row added or a row removed must fail:
//! `defaults_test_fails_when_a_table_cell_is_edited`.
//!
//! The plan is found by the rule R7 (`AZLIN_PLAN_MD`; else the sibling checkout of azul-apps; if it
//! is absent the test prints `SKIPPED: plan not present` and passes only while `AZLIN_PLAN_MD` is
//! unset).
//!
//! Implements planning/architecture.md section 5.10 (the account policy, the table) and section 5.15
//! (M4).

mod common;

use std::collections::BTreeMap;
use std::str::FromStr;

use azlin_policy::{
    required_people, AccountKind, AdminQuorum, AuditLevel, DeviceApproval, ExternalSharing,
    ManagedDefault, PaddingPolicy, Policy, QuorumClass, UpdatePolicy,
};
use azlin_seams::Span;
use common::{backticked, plan_text, table_rows, KINDS};

/// The rows of the table, by their first cell (the backticked names, joined by a comma), as the
/// text of the value cell.
fn policy_rows(plan: &str) -> Result<BTreeMap<String, String>, String> {
    let rows = table_rows(
        plan,
        "| Field | personal / family / team / business default |",
    )?;
    let mut map = BTreeMap::new();
    for row in rows {
        let [field, value] = row.as_slice() else {
            return Err(format!(
                "a row of the table has {} cells: {row:?}",
                row.len()
            ));
        };
        let name = backticked(field).join(",");
        if map.insert(name.clone(), value.clone()).is_some() {
            return Err(format!("the row {name} is in the table twice"));
        }
    }
    Ok(map)
}

/// The text of `cell` without its parenthesised parts and without backticks, trimmed.
fn plain(cell: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in cell.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '`' => {}
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// The four cells of a four-value row: the value text up to the first `;`, split on ` / `, each
/// cell without its parenthesised part.
fn four(row: &str, value: &str) -> Result<Vec<String>, String> {
    let head = value.split(';').next().unwrap_or(value);
    let cells: Vec<String> = head.split(" / ").map(plain).collect();
    if cells.len() != 4 {
        return Err(format!(
            "{row}: {} cells where the table has four: {value:?}",
            cells.len()
        ));
    }
    Ok(cells)
}

/// `N h` (an hour count, then anything): the number of hours.
fn hours(row: &str, cell: &str) -> Result<u64, String> {
    let mut words = cell.split_whitespace();
    let number = words
        .next()
        .ok_or_else(|| format!("{row}: no number in {cell:?}"))?;
    let unit = words
        .next()
        .ok_or_else(|| format!("{row}: no unit in {cell:?}"))?;
    if unit != "h" {
        return Err(format!("{row}: the unit is {unit:?}, not h: {cell:?}"));
    }
    number
        .parse::<u64>()
        .map_err(|_| format!("{row}: {number:?} is no number: {cell:?}"))
}

/// The rows of the table that this crate knows, by name; a row that is in the document and not
/// here, or here and not in the document, is a problem.
const KNOWN_ROWS: [&str; 16] = [
    "credential_ttl_s",
    "device_approval",
    "max_devices_per_member",
    "managed_default",
    "sso_required,sso_max_age_s",
    "enroll_hold_s",
    "passkey_recovery",
    "escrow_open_wait_s",
    "audit_level",
    "admin_quorum",
    "link_policy",
    "request_policy",
    "browser_devices",
    "external_sharing",
    "allowed_networks",
    "updates",
];

/// Compares every cell of the table of `plan` with `Policy::defaults`; every mismatch is a line of
/// the error.
fn check_defaults(plan: &str) -> Result<(), Vec<String>> {
    let mut problems: Vec<String> = Vec::new();
    let rows = match policy_rows(plan) {
        Ok(rows) => rows,
        Err(error) => return Err(vec![error]),
    };
    for name in rows.keys() {
        if !KNOWN_ROWS.contains(&name.as_str()) {
            problems.push(format!(
                "unknown row: the table has a row {name} that the crate does not know"
            ));
        }
    }
    for name in KNOWN_ROWS {
        if !rows.contains_key(name) {
            problems.push(format!("missing row: the table has no row {name}"));
        }
    }
    let defaults: Vec<Policy> = KINDS.iter().map(|kind| Policy::defaults(*kind)).collect();

    // Each row is checked by a closure that returns its own problems.
    let mut check = |name: &str, body: &dyn Fn(&str) -> Result<Vec<String>, String>| {
        if let Some(value) = rows.get(name) {
            match body(value) {
                Ok(found) => problems.extend(found),
                Err(error) => problems.push(error),
            }
        }
    };

    // A four-value row: each cell is turned into the expected value of its kind by `expect`, and
    // compared with `got` of the default of that kind.
    fn per_kind<T: PartialEq + std::fmt::Debug>(
        row: &str,
        value: &str,
        defaults: &[Policy],
        expect: impl Fn(&str) -> Result<T, String>,
        got: impl Fn(&Policy) -> T,
    ) -> Result<Vec<String>, String> {
        let cells = four(row, value)?;
        let mut found = Vec::new();
        for ((kind, cell), policy) in KINDS.iter().zip(&cells).zip(defaults) {
            let want = expect(cell).map_err(|e| format!("{row} ({kind}): {e}"))?;
            let have = got(policy);
            if want != have {
                found.push(format!(
                    "{row} ({kind}): the table says {cell:?} = {want:?}, the default is {have:?}"
                ));
            }
        }
        Ok(found)
    }

    // A single-value row: one value for every kind.
    fn every_kind<T: PartialEq + std::fmt::Debug>(
        row: &str,
        want: T,
        defaults: &[Policy],
        got: impl Fn(&Policy) -> T,
    ) -> Vec<String> {
        let mut found = Vec::new();
        for (kind, policy) in KINDS.iter().zip(defaults) {
            let have = got(policy);
            if want != have {
                found.push(format!(
                    "{row} ({kind}): the table says {want:?}, the default is {have:?}"
                ));
            }
        }
        found
    }

    check("credential_ttl_s", &|value| {
        per_kind(
            "credential_ttl_s",
            value,
            &defaults,
            |cell| hours("credential_ttl_s", cell).map(|h| Span::hours(h as u32)),
            |p| p.credential_ttl,
        )
    });
    check("device_approval", &|value| {
        per_kind(
            "device_approval",
            value,
            &defaults,
            |cell| match cell {
                "self" => Ok(DeviceApproval::SelfOnly),
                "self or quorum" => Ok(DeviceApproval::SelfOrQuorum),
                other => Err(format!("an unknown cell {other:?}")),
            },
            |p| p.device_approval,
        )
    });
    check("max_devices_per_member", &|value| {
        let want: u8 = plain(value)
            .parse()
            .map_err(|_| format!("max_devices_per_member: {value:?} is no number"))?;
        Ok(every_kind("max_devices_per_member", want, &defaults, |p| {
            p.max_devices_per_member
        }))
    });
    check("managed_default", &|value| {
        per_kind(
            "managed_default",
            value,
            &defaults,
            |cell| match cell {
                "no" => Ok(ManagedDefault::No),
                "yes" => Ok(ManagedDefault::Yes),
                other => Err(format!("an unknown cell {other:?}")),
            },
            |p| p.managed_default,
        )
    });
    check("sso_required,sso_max_age_s", &|value| {
        // "no / no / optional / optional": "optional" is a choice of an admin, so the default is off
        // and no maximum age in every kind; a cell that says "yes" would make it a default.
        let mut found = per_kind(
            "sso_required",
            value,
            &defaults,
            |cell| match cell {
                "no" | "optional" => Ok(false),
                "yes" => Ok(true),
                other => Err(format!("an unknown cell {other:?}")),
            },
            |p| p.sso_required,
        )?;
        found.extend(every_kind("sso_max_age_s", None, &defaults, |p| {
            p.sso_max_age
        }));
        Ok(found)
    });
    check("enroll_hold_s", &|value| {
        let want = Span::hours(hours("enroll_hold_s", value)? as u32);
        Ok(every_kind("enroll_hold_s", want, &defaults, |p| {
            p.enroll_hold
        }))
    });
    check("passkey_recovery", &|value| {
        per_kind(
            "passkey_recovery",
            value,
            &defaults,
            |cell| match cell {
                "on" => Ok(true),
                "off" => Ok(false),
                other => Err(format!("an unknown cell {other:?}")),
            },
            |p| p.passkey_recovery,
        )
    });
    check("escrow_open_wait_s", &|value| {
        let want = Span::hours(hours("escrow_open_wait_s", value)? as u32);
        Ok(every_kind("escrow_open_wait_s", want, &defaults, |p| {
            p.escrow_open_wait
        }))
    });
    check("audit_level", &|value| {
        // The words of the level are the list in the parentheses: none, admin, access_summary,
        // access_full.
        let listed: Vec<String> = value
            .split_once('(')
            .and_then(|(_, rest)| rest.split_once(')'))
            .map(|(list, _)| list.split(',').map(|w| w.trim().to_string()).collect())
            .unwrap_or_default();
        let words: Vec<String> = AuditLevel::ALL
            .iter()
            .map(|l| l.as_str().to_string())
            .collect();
        let mut found = Vec::new();
        if listed != words {
            found.push(format!(
                "audit_level: the table lists the levels {listed:?}, the crate has {words:?}"
            ));
        }
        found.extend(per_kind(
            "audit_level",
            value,
            &defaults,
            |cell| {
                AuditLevel::from_str(&cell.replace(' ', "_")).map_err(|e| format!("{e}: {cell:?}"))
            },
            |p| p.audit_level,
        )?);
        Ok(found)
    });
    check("admin_quorum", &|value| {
        // "1 for personal and family; business: 2 for admin changes, policy, escrow, escrow_open,
        // quorum enrollments"; a team has no value (drift register: 1).
        let (first, second) = value
            .split_once(';')
            .ok_or_else(|| format!("admin_quorum: no `;` in {value:?}"))?;
        let one: u8 = first
            .split_whitespace()
            .next()
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| format!("admin_quorum: no number in {first:?}"))?;
        if !first.contains("personal and family") {
            return Err(format!(
                "admin_quorum: {first:?} does not say `personal and family`"
            ));
        }
        let business = second
            .trim()
            .strip_prefix("business:")
            .ok_or_else(|| format!("admin_quorum: {second:?} does not start with `business:`"))?;
        let (number, classes) = business
            .trim()
            .split_once(" for ")
            .ok_or_else(|| format!("admin_quorum: no ` for ` in {business:?}"))?;
        let two: u8 = number
            .trim()
            .parse()
            .map_err(|_| format!("admin_quorum: {number:?} is no number"))?;
        let listed: Vec<&str> = classes.split(',').map(str::trim).collect();
        let classes_of_the_crate = [
            ("admin changes", QuorumClass::AdminChange),
            ("policy", QuorumClass::PolicyChange),
            ("escrow", QuorumClass::EscrowChange),
            ("escrow_open", QuorumClass::EscrowOpen),
            ("quorum enrollments", QuorumClass::QuorumEnrollment),
        ];
        let mut found = Vec::new();
        let names: Vec<&str> = classes_of_the_crate.iter().map(|(n, _)| *n).collect();
        if listed != names {
            found.push(format!(
                "admin_quorum: the table lists the classes {listed:?}, the crate has {names:?}"
            ));
        }
        for (kind, policy) in KINDS.iter().zip(&defaults) {
            let want = match kind {
                AccountKind::Personal | AccountKind::Family => one,
                AccountKind::Team => 1, // the table gives none: the signer alone
                AccountKind::Business => two,
            };
            if policy.admin_quorum != AdminQuorum::new(want).map_err(|e| e.to_string())? {
                found.push(format!(
                    "admin_quorum ({kind}): the table says {want}, the default is {}",
                    policy.admin_quorum.get()
                ));
            }
            for (name, class) in classes_of_the_crate {
                if required_people(policy, class) != want {
                    found.push(format!(
                        "admin_quorum ({kind}) {name}: need {} not {want}",
                        required_people(policy, class)
                    ));
                }
            }
            if required_people(policy, QuorumClass::Other) != 1 {
                found.push(format!("admin_quorum ({kind}) other: one person"));
            }
        }
        Ok(found)
    });
    check("browser_devices", &|value| {
        per_kind(
            "browser_devices",
            value,
            &defaults,
            |cell| match cell {
                "allowed" => Ok(true),
                "forbidden" => Ok(false),
                other => Err(format!("an unknown cell {other:?}")),
            },
            |p| p.browser_devices,
        )
    });
    check("external_sharing", &|value| {
        per_kind(
            "external_sharing",
            value,
            &defaults,
            |cell| {
                ExternalSharing::from_str(&cell.replace(' ', "_"))
                    .map_err(|e| format!("{e}: {cell:?}"))
            },
            |p| p.external_sharing,
        )
    });
    check("updates", &|value| {
        // "auto / ... / `managed_by_mdm`": the first cell is the personal one, the last the
        // business one; the elided middle is a drift row (the crate: auto).
        let cells: Vec<String> = value.split(" / ").map(plain).collect();
        let (Some(first), Some(last)) = (cells.first(), cells.last()) else {
            return Err(format!("updates: no cells in {value:?}"));
        };
        let mut found = Vec::new();
        for (cell, kind) in [
            (first, AccountKind::Personal),
            (last, AccountKind::Business),
        ] {
            let want =
                UpdatePolicy::from_str(cell).map_err(|e| format!("updates: {e}: {cell:?}"))?;
            let have = Policy::defaults(kind).updates;
            if want != have {
                found.push(format!(
                    "updates ({kind}): the table says {want:?}, the default is {have:?}"
                ));
            }
        }
        Ok(found)
    });
    check("allowed_networks", &|value| {
        // "later: ...": the field is reserved, and the crate has none. When the plan makes it
        // real, this test says so.
        if value.starts_with("later") {
            Ok(Vec::new())
        } else {
            Ok(vec![format!(
                "allowed_networks: the plan no longer says `later`: {value:?}"
            )])
        }
    });
    // The rows of link_policy and request_policy are read by tests/link_policy.rs.
    // `padding` is not a row of the table (planning/architecture.md section 5.15, M4).
    for kind in KINDS {
        let policy = Policy::defaults(kind);
        if policy.padding != PaddingPolicy::Buckets {
            problems.push(format!(
                "padding ({kind}): section 5.15 says buckets is the default"
            ));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

#[test]
fn defaults_match_architecture_5_10_table() {
    let Some(plan) = plan_text() else {
        return;
    };
    if let Err(problems) = check_defaults(&plan) {
        panic!(
            "Policy::defaults does not match the table of 5.10:\n{}",
            problems.join("\n")
        );
    }
}

/// `plan` with `old` replaced by `new`; `old` must be in the document exactly once.
fn edited(plan: &str, old: &str, new: &str) -> String {
    assert_eq!(
        plan.matches(old).count(),
        1,
        "the text to edit is not in the plan exactly once: {old:?}"
    );
    plan.replacen(old, new, 1)
}

#[test]
fn defaults_test_fails_when_a_table_cell_is_edited() {
    let Some(plan) = plan_text() else {
        return;
    };
    // The unedited document passes (so that every failure below is the edit's).
    if let Err(problems) = check_defaults(&plan) {
        panic!("the unedited plan fails: {problems:?}");
    }
    // (old text, new text, a word the problem must name)
    let edits: [(&str, &str, &str); 20] = [
        (
            "| `credential_ttl_s` | 12 h / 12 h / 12 h / 1 h |",
            "| `credential_ttl_s` | 12 h / 12 h / 12 h / 2 h |",
            "credential_ttl_s",
        ),
        (
            "| `credential_ttl_s` | 12 h / 12 h / 12 h / 1 h |",
            "| `credential_ttl_s` | 6 h / 12 h / 12 h / 1 h |",
            "credential_ttl_s",
        ),
        (
            "| `device_approval` | self / self / self or quorum / self or quorum |",
            "| `device_approval` | self / self / self / self or quorum |",
            "device_approval",
        ),
        (
            "| `device_approval` | self / self / self or quorum / self or quorum |",
            "| `device_approval` | self or quorum / self / self or quorum / self or quorum |",
            "device_approval",
        ),
        (
            "| `max_devices_per_member` | 10 |",
            "| `max_devices_per_member` | 11 |",
            "max_devices_per_member",
        ),
        (
            "| no / no (private spaces never) / no / yes |",
            "| no / no (private spaces never) / no / no |",
            "managed_default",
        ),
        (
            "| no / no / optional / optional;",
            "| yes / no / optional / optional;",
            "sso_required",
        ),
        (
            "| `enroll_hold_s` | 48 h for any enrollment",
            "| `enroll_hold_s` | 24 h for any enrollment",
            "enroll_hold_s",
        ),
        (
            "| on / on / off / off;",
            "| on / on / on / off;",
            "passkey_recovery",
        ),
        (
            "| `escrow_open_wait_s` | 72 h for an active member |",
            "| `escrow_open_wait_s` | 48 h for an active member |",
            "escrow_open_wait_s",
        ),
        (
            "| none / none / admin / access summary (",
            "| none / none / none / access summary (",
            "audit_level",
        ),
        (
            "(none, admin, access_summary, access_full)",
            "(none, admin, access_full)",
            "audit_level",
        ),
        (
            "business: 2 for admin changes",
            "business: 3 for admin changes",
            "admin_quorum",
        ),
        (
            "1 for personal and family; business",
            "2 for personal and family; business",
            "admin_quorum",
        ),
        (
            "policy, escrow, escrow_open, quorum enrollments",
            "policy, escrow, quorum enrollments",
            "admin_quorum",
        ),
        (
            "| allowed / allowed / allowed / allowed;",
            "| allowed / allowed / allowed / forbidden;",
            "browser_devices",
        ),
        (
            "| allowed / allowed / allowed / same workspace only |",
            "| allowed / allowed / allowed / allowed |",
            "external_sharing",
        ),
        (
            "| auto / ... / `managed_by_mdm` |",
            "| auto / ... / auto |",
            "updates",
        ),
        (
            "| later: an allow-list checked in RAM, never stored |",
            "| an allow-list checked in RAM, never stored |",
            "allowed_networks",
        ),
        // A row added: a field of the table that the crate does not know.
        (
            "| `updates` | auto / ... / `managed_by_mdm` |",
            "| `updates` | auto / ... / `managed_by_mdm` |\n| `new_field` | on |",
            "unknown row",
        ),
    ];
    for (old, new, names) in edits {
        let copy = edited(&plan, old, new);
        match check_defaults(&copy) {
            Ok(()) => panic!("the edit {old:?} -> {new:?} was not noticed"),
            Err(problems) => assert!(
                problems.iter().any(|p| p.contains(names)),
                "the edit {old:?} -> {new:?} gave problems that do not name {names:?}: {problems:?}"
            ),
        }
    }
    // A row removed.
    let line = plan
        .lines()
        .find(|l| l.starts_with("| `escrow_open_wait_s` |"))
        .expect("the row is in the plan");
    let copy = edited(&plan, &format!("{line}\n"), "");
    match check_defaults(&copy) {
        Ok(()) => panic!("a removed row was not noticed"),
        Err(problems) => assert!(
            problems
                .iter()
                .any(|p| p.contains("missing row") && p.contains("escrow_open_wait_s")),
            "{problems:?}"
        ),
    }
}
