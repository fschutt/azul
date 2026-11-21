//! Shared helpers of the integration tests: where the crate and the plan are, the rows of a table of
//! the plan, ids, and the seeded generators of random policies and patches (the property tests).
//!
//! Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays: the generators
//! draw from `azlin_seams::sim::Rng`) and the plan path rule R7 of COMMON-W1R.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use azlin_policy::{
    AccountKind, AdminQuorum, AudienceClass, AudienceSet, AuditLevel, DeviceApproval,
    ExternalSharing, LinkPolicy, LinkPolicyPatch, ManagedDefault, PaddingPolicy, Policy,
    PolicyPatch, RequestPolicy, RequestPolicyPatch, UpdatePolicy,
};
use azlin_seams::sim::Rng;
use azlin_seams::{ByteCount, MemberId, Span};

/// The folder of this crate.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The four kinds, in the order of the columns of the table of planning/architecture.md section
/// 5.10.
pub const KINDS: [AccountKind; 4] = [
    AccountKind::Personal,
    AccountKind::Family,
    AccountKind::Team,
    AccountKind::Business,
];

/// The plan, by the rule R7: `AZLIN_PLAN_MD`; else (an azul crate)
/// `CARGO_MANIFEST_DIR/../../../azul-apps/planning/architecture.md`. `None` (and a printed
/// `SKIPPED: plan not present`) when the file is absent and `AZLIN_PLAN_MD` is unset; a failure when
/// it is set and the file cannot be read.
pub fn plan_text() -> Option<String> {
    let from_env = std::env::var_os("AZLIN_PLAN_MD");
    let path = match &from_env {
        Some(path) => PathBuf::from(path),
        None => root().join("../../../azul-apps/planning/architecture.md"),
    };
    match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) => {
            assert!(
                from_env.is_none(),
                "AZLIN_PLAN_MD is set but {} cannot be read: {error}",
                path.display()
            );
            eprintln!("SKIPPED: plan not present");
            None
        }
    }
}

/// The cells of one row of a markdown table: the text between the pipes, trimmed.
pub fn split_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

/// The data rows of the table whose header row starts with `header_prefix` (the separator row
/// `|---|` is skipped): the table ends at the first line that does not start with a pipe. An error
/// when the header is not there.
pub fn table_rows(plan: &str, header_prefix: &str) -> Result<Vec<Vec<String>>, String> {
    let mut lines = plan.lines();
    let mut found = false;
    for line in lines.by_ref() {
        if line.starts_with(header_prefix) {
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!("no table header starting with {header_prefix:?}"));
    }
    let mut rows = Vec::new();
    for line in lines {
        if !line.starts_with('|') {
            break;
        }
        let cells = split_row(line);
        let separator = cells
            .iter()
            .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'));
        if separator {
            continue;
        }
        rows.push(cells);
    }
    Ok(rows)
}

/// The names between backticks in `cell`, in order.
pub fn backticked(cell: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = cell;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        match after.find('`') {
            Some(end) => {
                names.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    names
}

/// The member with the id of 16 bytes of `n`.
pub fn member(n: u8) -> MemberId {
    MemberId::from_bytes([n; 16])
}

/// A random element of a pool.
fn pick<T: Copy>(rng: &mut Rng, pool: &[T]) -> T {
    *rng.pick(pool).expect("a pool is never empty")
}

/// The durations the generators draw from: few values, so that equal values are common.
const SPANS: [Span; 6] = [
    Span::minutes(15),
    Span::hours(1),
    Span::hours(12),
    Span::hours(48),
    Span::hours(72),
    Span::days(7),
];

fn span(rng: &mut Rng) -> Span {
    pick(rng, &SPANS)
}

fn limit(rng: &mut Rng) -> Option<Span> {
    if rng.chance(1, 3) {
        None
    } else {
        Some(span(rng))
    }
}

fn flag(rng: &mut Rng) -> bool {
    rng.chance(1, 2)
}

fn audiences(rng: &mut Rng) -> AudienceSet {
    let mut set = AudienceSet::NONE;
    for class in AudienceClass::ALL {
        if rng.chance(2, 3) {
            set = set.with(*class);
        }
    }
    set
}

fn files(rng: &mut Rng) -> u32 {
    pick(rng, &[1, 10, 1_000, 100_000, u32::MAX])
}

fn bytes(rng: &mut Rng) -> ByteCount {
    pick(
        rng,
        &[
            ByteCount::kib(1),
            ByteCount::mib(1),
            ByteCount::gib(1),
            ByteCount::gib(100),
            ByteCount::MAX,
        ],
    )
}

fn quorum(rng: &mut Rng) -> AdminQuorum {
    AdminQuorum::new(pick(rng, &[1, 2, 3, 5])).expect("1 to 5 are quorums")
}

fn approval(rng: &mut Rng) -> DeviceApproval {
    pick(rng, DeviceApproval::ALL)
}

fn audit(rng: &mut Rng) -> AuditLevel {
    pick(rng, AuditLevel::ALL)
}

fn sharing(rng: &mut Rng) -> ExternalSharing {
    pick(rng, ExternalSharing::ALL)
}

fn managed(rng: &mut Rng) -> ManagedDefault {
    pick(rng, ManagedDefault::ALL)
}

fn updates(rng: &mut Rng) -> UpdatePolicy {
    pick(rng, UpdatePolicy::ALL)
}

fn padding(rng: &mut Rng) -> PaddingPolicy {
    pick(rng, PaddingPolicy::ALL)
}

fn devices(rng: &mut Rng) -> u8 {
    pick(rng, &[1, 3, 5, 10, 20])
}

/// A random policy: every field drawn from a small pool.
pub fn random_policy(rng: &mut Rng) -> Policy {
    Policy {
        credential_ttl: span(rng),
        device_approval: approval(rng),
        max_devices_per_member: devices(rng),
        managed_default: managed(rng),
        sso_required: flag(rng),
        sso_max_age: limit(rng),
        enroll_hold: span(rng),
        passkey_recovery: flag(rng),
        escrow_open_wait: span(rng),
        audit_level: audit(rng),
        admin_quorum: quorum(rng),
        link_policy: LinkPolicy {
            audiences: audiences(rng),
            max_expiry: limit(rng),
            password_for_anyone: flag(rng),
            force_preview_only: flag(rng),
            force_watermark: flag(rng),
            force_app_only: flag(rng),
            uploads: flag(rng),
            edit: flag(rng),
            comments: flag(rng),
            open_logs: flag(rng),
            slugs: flag(rng),
        },
        request_policy: RequestPolicy {
            allowed: flag(rng),
            max_expiry: limit(rng),
            max_files: files(rng),
            max_bytes: bytes(rng),
        },
        browser_devices: flag(rng),
        external_sharing: sharing(rng),
        updates: updates(rng),
        padding: padding(rng),
    }
}

/// With probability 1/3, `Some(draw())`; else `None`.
fn maybe<T>(rng: &mut Rng, draw: impl FnOnce(&mut Rng) -> T) -> Option<T> {
    if rng.chance(1, 3) {
        Some(draw(rng))
    } else {
        None
    }
}

/// A random patch: each field is set with probability 1/3, to a value of the same pools.
pub fn random_patch(rng: &mut Rng) -> PolicyPatch {
    PolicyPatch {
        credential_ttl: maybe(rng, span),
        device_approval: maybe(rng, approval),
        max_devices_per_member: maybe(rng, devices),
        managed_default: maybe(rng, managed),
        sso_required: maybe(rng, flag),
        sso_max_age: maybe(rng, limit),
        enroll_hold: maybe(rng, span),
        passkey_recovery: maybe(rng, flag),
        escrow_open_wait: maybe(rng, span),
        audit_level: maybe(rng, audit),
        admin_quorum: maybe(rng, quorum),
        link_policy: LinkPolicyPatch {
            audiences: maybe(rng, audiences),
            max_expiry: maybe(rng, limit),
            password_for_anyone: maybe(rng, flag),
            force_preview_only: maybe(rng, flag),
            force_watermark: maybe(rng, flag),
            force_app_only: maybe(rng, flag),
            uploads: maybe(rng, flag),
            edit: maybe(rng, flag),
            comments: maybe(rng, flag),
            open_logs: maybe(rng, flag),
            slugs: maybe(rng, flag),
        },
        request_policy: RequestPolicyPatch {
            allowed: maybe(rng, flag),
            max_expiry: maybe(rng, limit),
            max_files: maybe(rng, files),
            max_bytes: maybe(rng, bytes),
        },
        browser_devices: maybe(rng, flag),
        external_sharing: maybe(rng, sharing),
        updates: maybe(rng, updates),
        padding: maybe(rng, padding),
    }
}
