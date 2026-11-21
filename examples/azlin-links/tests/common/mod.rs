//! Shared helpers of the integration tests: where the crate and the plan are, the rows of a table of
//! the plan, ids and parts of a link, the seeded entropy, and a builder that makes a link from its
//! options the way the node does (`validate`, `apply_policy`, `LinkRecord::create`).
//!
//! Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays: the entropy draws
//! from `azlin_seams::sim::Rng`) and the plan path rule R7 of COMMON-W1R.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use azlin_links::{
    apply_policy, validate, Actor, Audience, Facts, Grant, Grants, LinkKind, LinkOptions,
    LinkRecord, NewLink, ObjectRef, OpaqueRecord, PublicLinkId, RequestId, Sealed, Wrap,
};
use azlin_links::{LogId, PolicyViolation};
use azlin_policy::LinkPolicy;
use azlin_seams::sim::Rng;
use azlin_seams::{Entropy, EntropyError, GroupId, Hash32, KeyId, MemberId, SpaceId, Span, Stamp};

/// The leader's stamp of every test: 2027-01-15.
pub const NOW: Stamp = Stamp::from_secs(1_800_000_000);

/// One day.
pub const DAY: Span = Span::days(1);

/// The folder of this crate.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

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

/// A seeded entropy source: the same seed gives the same bytes on every platform (SplitMix64).
pub struct TestEntropy(Rng);

impl TestEntropy {
    /// The source of this seed.
    pub fn new(seed: u64) -> TestEntropy {
        TestEntropy(Rng::new(seed))
    }
}

impl Entropy for TestEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        self.0.fill_bytes(out);
        Ok(())
    }
}

/// A source that always fails.
pub struct DeadEntropy;

impl Entropy for DeadEntropy {
    fn fill(&mut self, _out: &mut [u8]) -> Result<(), EntropyError> {
        Err(EntropyError::Exhausted)
    }
}

/// A broken source that gives the same bytes every time.
pub struct RepeatingEntropy(pub u8);

impl Entropy for RepeatingEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        out.fill(self.0);
        Ok(())
    }
}

/// A broken source that gives these 8 bytes at every draw (the first ones of a shorter buffer).
pub struct FixedEntropy(pub [u8; 8]);

impl Entropy for FixedEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        for (dst, src) in out.iter_mut().zip(self.0) {
            *dst = src;
        }
        Ok(())
    }
}

/// The member with the id of 16 bytes of `n`.
pub fn member(n: u8) -> MemberId {
    MemberId::from_bytes([n; 16])
}

/// The group with the id of 16 bytes of `n`.
pub fn group(n: u8) -> GroupId {
    GroupId::from_bytes([n; 16])
}

/// The space with the id of 16 bytes of `n`.
pub fn space(n: u8) -> SpaceId {
    SpaceId::from_bytes([n; 16])
}

/// The key id of 32 bytes of `n`.
pub fn key(n: u8) -> KeyId {
    KeyId::from_bytes([n; 32])
}

/// The object named by the hash of 32 bytes of `n`.
pub fn object(n: u8) -> ObjectRef {
    ObjectRef::from_hash(Hash32::from_bytes([n; 32]))
}

/// A sealed value of `len` bytes of `n`.
pub fn sealed(n: u8, len: usize) -> Sealed {
    Sealed::new(vec![n; len]).expect("a sealed value")
}

/// The wrap of the share key to the member `n` (and the key `n`).
pub fn wrap(n: u8) -> Wrap {
    Wrap::new(member(n), key(n), sealed(n, 64)).expect("a wrap")
}

/// `n` grants of whole objects.
pub fn grants(n: usize) -> Grants {
    let list: Vec<Grant> = (0..n)
        .map(|i| Grant {
            object: object(i as u8),
            range: None,
        })
        .collect();
    Grants::new(list).expect("grants")
}

/// An OPAQUE record of the bytes `n`.
pub fn opaque(n: u8) -> OpaqueRecord {
    OpaqueRecord::new(vec![n; 200]).expect("an OPAQUE record")
}

/// The public id with the number `n` (below 2^60).
pub fn link_id(n: u64) -> PublicLinkId {
    PublicLinkId::from_bytes(n.to_be_bytes()).expect("an id below 2^60")
}

/// The paired request `n`.
pub fn request(n: u8) -> RequestId {
    RequestId::from_bytes([n; 16])
}

/// The comment log `n`.
pub fn log(n: u8) -> LogId {
    LogId::from_bytes([n; 16])
}

/// A `people` audience of the members `ns`.
pub fn people(ns: &[u8]) -> Audience {
    Audience::People {
        people: ns.iter().map(|n| member(*n)).collect(),
    }
}

/// A `members` audience of the groups `ns`.
pub fn groups(ns: &[u8]) -> Audience {
    Audience::Members {
        groups: ns.iter().map(|n| group(*n)).collect(),
    }
}

/// The five audiences, one of each class (the named ones with two names).
pub fn every_audience() -> Vec<Audience> {
    vec![
        Audience::Anyone,
        Audience::SignedIn,
        groups(&[1, 2]),
        people(&[10, 11]),
        Audience::Approval,
    ]
}

/// The options of a link over one file for `audience`, everything else off.
pub fn opts(audience: Audience) -> LinkOptions {
    LinkOptions::new(audience, LinkKind::File)
}

/// The parts of a link that fit `options`: a wrap for each named person, the OPAQUE record of a
/// password, the paired request an upload or an approval needs, the comment log comments need, one
/// grant. The creator is the member 1, the space 2.
pub fn parts_for(options: &LinkOptions) -> NewLink {
    let wraps = match &options.audience {
        Audience::People { people } => people
            .iter()
            .map(|person| {
                Wrap::new(*person, KeyId::from_bytes([7; 32]), sealed(7, 64)).expect("a wrap")
            })
            .collect(),
        _ => Vec::new(),
    };
    let needs_request = options.allow_upload || matches!(options.audience, Audience::Approval);
    NewLink {
        space: space(2),
        created_by: member(1),
        manifest_object: object(9),
        grants: grants(1),
        opaque_record: options.password.then(|| opaque(5)),
        wraps,
        request_id: needs_request.then(|| request(3)),
        comment_log: options.comment.then(|| log(4)),
    }
}

/// The facts of `parts`, at `NOW`.
pub fn facts_of(parts: &NewLink) -> Facts {
    Facts {
        paired_request: parts.request_id.is_some(),
        comment_log: parts.comment_log.is_some(),
        opaque_record: parts.opaque_record.is_some(),
        wraps: parts.wraps.len(),
        grants: parts.grants.len(),
        now: NOW,
    }
}

/// The facts that fit `options` (see `parts_for`).
pub fn facts_for(options: &LinkOptions) -> Facts {
    facts_of(&parts_for(options))
}

/// Plain facts: nothing paired, no wraps, one grant, at `NOW`.
pub fn plain_facts() -> Facts {
    Facts {
        paired_request: false,
        comment_log: false,
        opaque_record: false,
        wraps: 0,
        grants: 1,
        now: NOW,
    }
}

/// Makes a link as the node does: `validate`, `apply_policy` under `policy`, `LinkRecord::create`
/// with the parts that fit the options. Panics when a step refuses (the tests that expect a refusal
/// call the steps themselves).
pub fn build_under(options: LinkOptions, policy: &LinkPolicy) -> LinkRecord {
    let parts = parts_for(&options);
    let facts = facts_of(&parts);
    let validated = validate(&options, &facts).expect("valid options");
    let bounded = apply_policy(validated, policy, NOW).expect("the policy allows these options");
    LinkRecord::create(bounded, parts, link_id(1), NOW).expect("a link")
}

/// Makes a link under a policy that bounds nothing.
pub fn build(options: LinkOptions) -> LinkRecord {
    build_under(options, &LinkPolicy::allow_all())
}

/// The refusals of `apply_policy`, or none.
pub fn refusals_under(options: &LinkOptions, policy: &LinkPolicy) -> Vec<PolicyViolation> {
    let validated = validate(options, &facts_for(options)).expect("valid options");
    match apply_policy(validated, policy, NOW) {
        Ok(_) => Vec::new(),
        Err(violations) => violations,
    }
}

/// The creator of the links `build` makes.
pub fn creator() -> Actor {
    Actor::Member {
        member: member(1),
        write: true,
        manage: false,
    }
}

/// A manager of the space (not the creator).
pub fn manager() -> Actor {
    Actor::Member {
        member: member(50),
        write: true,
        manage: true,
    }
}

/// A writer of the space who is neither the creator nor a manager.
pub fn writer() -> Actor {
    Actor::Member {
        member: member(51),
        write: true,
        manage: false,
    }
}

/// A member with no right on the space.
pub fn stranger() -> Actor {
    Actor::Member {
        member: member(52),
        write: false,
        manage: false,
    }
}

/// The creator without the right to write the space (it left the folder's writers).
pub fn creator_without_write() -> Actor {
    Actor::Member {
        member: member(1),
        write: false,
        manage: false,
    }
}
