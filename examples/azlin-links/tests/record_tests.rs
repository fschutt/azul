//! The row of a link (`reg_link`): how a record is made, read back and kept; the wraps, the grants,
//! the OPAQUE record, the device tokens (hashed, never an address), the pauses.
//!
//! Implements planning/architecture.md section 5.12 (`reg_link`, "Nobody at Azlin can peek").

mod common;

use std::collections::BTreeSet;

use azlin_links::{
    apply_policy, validate, Audience, ByteRange, Counters, DeviceToken, Grant, GrantError, Grants,
    LinkKind, LinkOptions, LinkRecord, LinkRow, NewLink, ObjectRef, OpaqueRecord,
    OpaqueRecordError, PausedBy, Pauses, RecordError, SeenDevice, Wrap, MAX_GRANTS,
    OPAQUE_RECORD_MAX_LEN, WRAP_MAX_LEN,
};
use azlin_policy::LinkPolicy;
use azlin_seams::{ByteCount, Span};
use common::{
    build, facts_of, grants, key, link_id, member, object, opaque, opts, parts_for, people, sealed,
    space, wrap, DeadEntropy, TestEntropy, NOW,
};

/// Makes a link from options and parts that may differ from what `parts_for` derives.
fn create(options: &LinkOptions, parts: NewLink) -> Result<LinkRecord, RecordError> {
    let validated = validate(options, &facts_of(&parts)).expect("valid options");
    let bounded = apply_policy(validated, &LinkPolicy::allow_all(), NOW).expect("no bounds");
    LinkRecord::create(bounded, parts, link_id(1), NOW)
}

#[test]
fn a_new_link_holds_what_it_was_made_of_and_nothing_else() {
    let mut o = opts(people(&[10, 11]));
    o.password = true;
    o.comment = true;
    o.allow_upload = true;
    let link = build(o.clone());
    assert_eq!(link.id(), &link_id(1));
    assert_eq!(link.space(), &space(2));
    assert_eq!(link.created_by(), &member(1));
    assert_eq!(link.options(), &o);
    assert_eq!(link.kind(), LinkKind::File);
    assert_eq!(link.slug(), None);
    assert_eq!(link.manifest_object(), &object(9));
    assert_eq!(link.grants(), &grants(1));
    assert_eq!(link.opaque_record(), Some(&opaque(5)));
    assert_eq!(link.wraps().count(), 2);
    assert!(link.wrap_for(&member(10)).is_some() && link.wrap_for(&member(11)).is_some());
    assert!(link.wrap_for(&member(12)).is_none());
    assert_eq!(link.request_id(), Some(&common::request(3)));
    assert_eq!(link.comment_log(), Some(&common::log(4)));
    // Nobody has opened it, nothing is paused or revoked, and the times are the creation.
    assert_eq!(link.counters(), &Counters::zero());
    assert_eq!(link.pauses(), Pauses::NONE);
    assert!(!link.is_paused() && !link.is_revoked());
    assert_eq!(link.revoked_at(), None);
    assert_eq!(link.created_at(), NOW);
    assert_eq!(link.keyed_at(), NOW);
    assert!(link.answers_to(&link_id(1)) && !link.answers_to(&link_id(2)));
    assert_eq!(link.check(), Ok(()));
    // The counters start at zero.
    let counters = link.counters();
    assert_eq!(counters.downloads, 0);
    assert_eq!(counters.bytes_served, ByteCount::ZERO);
    assert!(counters.devices_seen.is_empty());
    assert_eq!(
        (counters.first_open_at, counters.last_open_at),
        (None, None)
    );
}

#[test]
fn a_slug_is_the_options_and_the_link_answers_to_it() {
    let slug = azlin_links::Slug::parse("quarterly-report").expect("a slug");
    let mut o = opts(people(&[10]));
    o.slug = Some(slug.clone());
    let link = build(o);
    assert_eq!(link.slug(), Some(&slug));
    assert!(link.answers_to_slug(&slug));
    let other = azlin_links::Slug::parse("another-name").expect("a slug");
    assert!(!link.answers_to_slug(&other));
    assert!(!build(opts(Audience::SignedIn)).answers_to_slug(&slug));
}

#[test]
fn create_refuses_parts_that_are_not_the_ones_the_options_were_checked_against() {
    let o = opts(Audience::SignedIn);
    let ok = parts_for(&o);
    assert!(create(&o, ok.clone()).is_ok());

    // The facts were: no paired request, no log, no record, no wraps, one grant.
    let mut with_request = ok.clone();
    with_request.request_id = Some(common::request(1));
    let validated = validate(&o, &facts_of(&ok)).expect("valid");
    let bounded = apply_policy(validated, &LinkPolicy::allow_all(), NOW).expect("no bounds");
    assert_eq!(
        LinkRecord::create(bounded, with_request, link_id(1), NOW),
        Err(RecordError::FactsChanged)
    );

    for change in 0..5 {
        let mut parts = ok.clone();
        match change {
            0 => parts.comment_log = Some(common::log(1)),
            1 => parts.opaque_record = Some(opaque(1)),
            2 => parts.wraps = vec![wrap(10)],
            3 => parts.grants = grants(2),
            _ => parts.grants = grants(0),
        }
        let validated = validate(&o, &facts_of(&ok)).expect("valid");
        let bounded = apply_policy(validated, &LinkPolicy::allow_all(), NOW).expect("no bounds");
        assert_eq!(
            LinkRecord::create(bounded, parts, link_id(1), NOW),
            Err(RecordError::FactsChanged),
            "change {change}"
        );
    }
}

#[test]
fn the_wraps_of_a_people_link_are_the_named_people() {
    let o = opts(people(&[10, 11]));
    // One wrap for each person: through.
    let ok = parts_for(&o);
    assert_eq!(ok.wraps.len(), 2);
    assert!(create(&o, ok.clone()).is_ok());

    // Two wraps, but for 10 and a stranger: the count is right and the people are not.
    let mut wrong = ok.clone();
    wrong.wraps = vec![wrap(10), wrap(12)];
    assert_eq!(create(&o, wrong), Err(RecordError::WrapsAreNotThePeople));

    // Two wraps for the same member, one person unwrapped: refused as a duplicate.
    let mut twice = ok.clone();
    twice.wraps = vec![wrap(10), wrap(10)];
    assert_eq!(
        create(&o, twice),
        Err(RecordError::DuplicateWrap(member(10)))
    );

    // An approval link holds the wraps of whoever was approved.
    let mut approval = opts(Audience::Approval);
    approval.comment = false;
    let mut parts = parts_for(&approval);
    parts.wraps = vec![wrap(20), wrap(21), wrap(22)];
    let link = create(&approval, parts).expect("an approval link with three approvals");
    assert_eq!(link.wraps().count(), 3);
    assert!(link.wrap_for(&member(21)).is_some());
}

#[test]
fn from_row_reads_a_row_back_and_checks_it_again() {
    let mut o = opts(people(&[10, 11]));
    o.password = true;
    let link = build(o);
    assert_eq!(LinkRecord::from_row(link.row().clone()), Ok(link.clone()));
    assert_eq!(link.clone().into_row(), link.row().clone());

    // A row that breaks an invariant is no record.
    let row = |edit: &dyn Fn(&mut LinkRow)| {
        let mut row = link.row().clone();
        edit(&mut row);
        LinkRecord::from_row(row)
    };
    // The wraps are not the people.
    assert_eq!(
        row(&|r| {
            r.wraps.remove(&member(11));
        }),
        Err(RecordError::Options(vec![
            azlin_links::OptionError::PeopleNeedOneWrapEach {
                people: 2,
                wraps: 1
            }
        ]))
    );
    // A wrap kept under another member's name.
    assert_eq!(
        row(&|r| {
            r.wraps.insert(member(11), wrap(10));
        }),
        Err(RecordError::WrapKeyMismatch(member(11)))
    );
    // A password without its record.
    assert!(matches!(
        row(&|r| r.opaque_record = None),
        Err(RecordError::Options(_))
    ));
    // Keyed before it was created.
    assert_eq!(
        row(&|r| r.keyed_at = NOW.checked_sub(Span::secs(1)).expect("after the epoch")),
        Err(RecordError::KeyedBeforeCreated)
    );
    // A link that is long expired is still a record: the rules between options do not read the
    // clock when a row is read back.
    let mut o = opts(Audience::SignedIn);
    o.expires_at = Some(NOW.checked_add(Span::secs(10)).expect("no overflow"));
    let link = build(o);
    let mut row = link.row().clone();
    row.options.expires_at = Some(NOW.checked_sub(Span::days(30)).expect("after the epoch"));
    assert!(LinkRecord::from_row(row).is_ok());
}

#[test]
fn a_record_prints_no_bytes_it_holds() {
    let mut o = opts(people(&[10]));
    o.title = Some(sealed(0xab, 40));
    o.message = Some(sealed(0xcd, 40));
    o.password = true;
    let link = build(o);
    let text = format!("{link:?}");
    // 0xab is 171, 0xcd is 205: no list of those bytes anywhere, only lengths.
    assert!(!text.contains("171, 171"), "{text}");
    assert!(!text.contains("205, 205"), "{text}");
    assert!(text.contains("Sealed(40 bytes)"), "{text}");
    assert!(text.contains("OpaqueRecord(200 bytes)"), "{text}");
    // A device token prints as nothing.
    assert_eq!(
        format!("{:?}", DeviceToken::from_bytes([0xee; 16])),
        "DeviceToken(..)"
    );
}

#[test]
fn a_seen_device_is_a_hash_of_the_token_under_the_link() {
    let token = DeviceToken::from_bytes([7; 16]);
    let other_token = DeviceToken::from_bytes([8; 16]);
    let first = link_id(1);
    let second = link_id(2);
    // Deterministic, and four different inputs give four different values.
    assert_eq!(token.seen_by(&first), token.seen_by(&first));
    let seen: BTreeSet<SeenDevice> = [
        token.seen_by(&first),
        token.seen_by(&second),
        other_token.seen_by(&first),
        other_token.seen_by(&second),
    ]
    .into_iter()
    .collect();
    assert_eq!(seen.len(), 4);
    // The token itself is not in the value, and the value has the width of the token.
    assert_ne!(token.seen_by(&first).as_bytes(), token.as_bytes());
    assert_eq!(token.seen_by(&first).as_bytes().len(), 16);

    // A device makes its token from the entropy seam: on a failure there is no token.
    let mut entropy = TestEntropy::new(3);
    let a = DeviceToken::generate(&mut entropy).expect("a token");
    let b = DeviceToken::generate(&mut entropy).expect("a token");
    assert_ne!(a, b);
    assert!(DeviceToken::generate(&mut DeadEntropy).is_err());
}

#[test]
fn pauses_are_independent_sources() {
    let none = Pauses::NONE;
    assert!(!none.is_paused());
    assert_eq!(none.sources(), Vec::<PausedBy>::new());
    assert_eq!(none.frozen_by(), None);
    for by in PausedBy::ALL {
        assert!(!none.has(*by));
        let one = none.with(*by);
        assert!(one.is_paused() && one.has(*by));
        assert_eq!(one.sources(), vec![*by]);
        assert_eq!(one.without(*by), none);
        // Setting twice is setting once.
        assert_eq!(one.with(*by), one);
    }
    let all = none
        .with(PausedBy::Operator)
        .with(PausedBy::Owner)
        .with(PausedBy::Reports);
    assert_eq!(
        all.sources(),
        vec![PausedBy::Owner, PausedBy::Reports, PausedBy::Operator]
    );
    // Lifting one source keeps the others.
    assert_eq!(
        all.without(PausedBy::Operator).sources(),
        vec![PausedBy::Owner, PausedBy::Reports]
    );
    assert_eq!(
        all.without(PausedBy::Owner).sources(),
        vec![PausedBy::Reports, PausedBy::Operator]
    );
    // The operator and the reports freeze the link; the owner's own pause does not.
    assert_eq!(none.with(PausedBy::Owner).frozen_by(), None);
    assert_eq!(
        none.with(PausedBy::Reports).frozen_by(),
        Some(PausedBy::Reports)
    );
    assert_eq!(
        none.with(PausedBy::Operator).frozen_by(),
        Some(PausedBy::Operator)
    );
    assert_eq!(all.frozen_by(), Some(PausedBy::Operator));
}

#[test]
fn grants_and_byte_ranges_have_limits() {
    // A range is at least one byte and ends within a size.
    assert_eq!(
        ByteRange::new(ByteCount::new(10), ByteCount::ZERO),
        Err(GrantError::EmptyRange)
    );
    assert_eq!(
        ByteRange::new(ByteCount::MAX, ByteCount::new(1)),
        Err(GrantError::RangeOverflow)
    );
    let range = ByteRange::new(ByteCount::new(10), ByteCount::new(5)).expect("a range");
    assert_eq!(range.start(), ByteCount::new(10));
    assert_eq!(range.size(), ByteCount::new(5));
    assert_eq!(range.end(), ByteCount::new(15));
    assert!(ByteRange::new(ByteCount::new(u64::MAX - 1), ByteCount::new(1)).is_ok());

    // At most MAX_GRANTS.
    let one = Grant {
        object: object(1),
        range: Some(range),
    };
    assert!(Grants::new(vec![one; MAX_GRANTS]).is_ok());
    assert_eq!(
        Grants::new(vec![one; MAX_GRANTS + 1]),
        Err(GrantError::TooMany {
            count: MAX_GRANTS + 1
        })
    );
    let list = Grants::new(vec![one, one]).expect("grants");
    assert_eq!((list.len(), list.is_empty()), (2, false));
    assert_eq!(list.as_slice(), [one, one]);
    assert!(Grants::new(Vec::new()).expect("no grants").is_empty());
    assert_eq!(ObjectRef::from_hash(*object(5).as_hash()), object(5));
}

#[test]
fn an_opaque_record_and_a_wrap_have_limits() {
    assert_eq!(OpaqueRecord::new(Vec::new()), Err(OpaqueRecordError::Empty));
    assert!(OpaqueRecord::new(vec![1; OPAQUE_RECORD_MAX_LEN]).is_ok());
    assert_eq!(
        OpaqueRecord::new(vec![1; OPAQUE_RECORD_MAX_LEN + 1]),
        Err(OpaqueRecordError::TooLong {
            len: OPAQUE_RECORD_MAX_LEN + 1
        })
    );
    assert_eq!(opaque(1).as_bytes().len(), 200);

    // A wrap: the sealed bytes at most WRAP_MAX_LEN.
    let at_limit = Wrap::new(member(1), key(1), sealed(1, WRAP_MAX_LEN)).expect("a wrap");
    assert_eq!(at_limit.member(), member(1));
    assert_eq!(at_limit.key(), key(1));
    assert_eq!(at_limit.sealed().len(), WRAP_MAX_LEN);
    assert!(Wrap::new(member(1), key(1), sealed(1, WRAP_MAX_LEN + 1)).is_err());
}

#[test]
fn a_used_row_is_read_back_with_its_counters() {
    // The counters of a used link can be put into a row and read back: the in-crate functions of
    // the resolver (S16c2) change them, a stored row carries them.
    let link = build(opts(Audience::SignedIn));
    let mut row = link.row().clone();
    row.counters.downloads = 5;
    row.counters.bytes_served = ByteCount::mib(3);
    row.counters
        .devices_seen
        .insert(DeviceToken::from_bytes([1; 16]).seen_by(&link_id(1)));
    row.counters.first_open_at = Some(NOW);
    row.counters.last_open_at = Some(NOW);
    let used = LinkRecord::from_row(row).expect("a used link");
    assert_eq!(used.counters().downloads, 5);
    assert_eq!(used.counters().devices_seen.len(), 1);
    assert_eq!(used.counters().bytes_served, ByteCount::mib(3));
    assert_eq!(used.counters().first_open_at, Some(NOW));
    assert_eq!(used.check(), Ok(()));
}

#[test]
fn a_record_has_no_address_and_no_clock_in_its_types() {
    // `LinkRow` names every column of `reg_link`; none holds an address type (the source scan of
    // `hygiene_tests` repeats this for the whole crate).
    let row = build(opts(Audience::SignedIn)).into_row();
    let text = format!("{row:?}");
    for banned in ["IpAddr", "SocketAddr", "Ipv4", "Ipv6"] {
        assert!(!text.contains(banned), "{banned}");
    }
}
