//! The drive's policy and its members' key wraps as files of the drive index (feature
//! `encryption`).

use crate::meta::{
    policy::{Access, Grant, Member, Policy, Role, POLICY_PATH},
    tree::walk,
    MemoryBucket, MetaError, MetaRepo, TestSealer,
};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

fn sealer() -> TestSealer {
    TestSealer::new([81; 32])
}

fn policy() -> Policy {
    let mut policy = Policy::default();
    for (id, name, role) in [
        ("d_laptop", "Laptop", Role::Owner),
        ("d_phone", "Phone", Role::Editor),
        ("d_tv", "TV", Role::Viewer),
        ("d_anna", "Anna", Role::Guest),
    ] {
        policy.members.insert(
            id.to_string(),
            Member {
                name: name.to_string(),
                role,
            },
        );
    }
    policy.grants.push(Grant {
        member: "d_anna".to_string(),
        folder: "photos/2026/".to_string(),
        access: Access::Read,
    });
    policy.grants.push(Grant {
        member: "d_tv".to_string(),
        folder: "inbox".to_string(),
        access: Access::Write,
    });
    policy
}

#[test]
fn a_policy_is_toml_and_reads_back_the_same() {
    let text = policy().to_toml().unwrap();
    assert!(text.contains("[members.d_laptop]"), "{text}");
    assert!(text.contains("role = \"guest\""), "{text}");
    assert!(text.contains("[[grants]]"), "{text}");
    assert_eq!(Policy::from_toml(&text).unwrap(), policy());
    assert_eq!(Policy::from_toml("").unwrap(), Policy::default());
    assert!(Policy::from_toml("[members.x]\nrole = \"king\"\n").is_err());
}

#[test]
fn roles_and_grants_decide_who_reads_and_who_writes() {
    let policy = policy();
    assert!(policy.allows("d_laptop", "anything/at/all.txt", true));
    assert!(policy.allows("d_phone", "docs/a.txt", true));
    assert!(policy.allows("d_tv", "docs/a.txt", false));
    assert!(!policy.allows("d_tv", "docs/a.txt", true));
    assert!(policy.allows("d_tv", "inbox/new.txt", true), "a write grant");
    assert!(policy.allows("d_anna", "photos/2026/beach.jpg", false));
    assert!(policy.allows("d_anna", "photos/2026", false));
    assert!(!policy.allows("d_anna", "photos/2026/beach.jpg", true), "a read grant");
    assert!(!policy.allows("d_anna", "photos/2025/old.jpg", false));
    assert!(!policy.allows("d_anna", "photos/2026-other/x.jpg", false));
    assert!(!policy.allows("d_stranger", "docs/a.txt", false));
    assert!(policy.administers("d_laptop"));
    assert!(!policy.administers("d_phone"));
}

#[test]
fn the_policy_is_a_file_of_the_drive_index_another_device_reads() {
    let bucket = MemoryBucket::new();
    let mut laptop = Repo::create(bucket.clone(), sealer(), "d_laptop", "Laptop").unwrap();
    assert_eq!(laptop.policy().unwrap(), None);
    laptop.set_policy(&policy()).unwrap();
    let phone = Repo::open(bucket, sealer(), "d_phone", "Phone").unwrap();
    assert_eq!(phone.policy().unwrap(), Some(policy()));
    let root = phone.root().unwrap().unwrap();
    assert!(crate::meta::tree::entry_at(phone.objects(), &root, POLICY_PATH)
        .unwrap()
        .is_some());
}

#[test]
fn a_member_joining_and_leaving_are_commits_the_history_keeps() {
    let bucket = MemoryBucket::new();
    let mut laptop = Repo::create(bucket.clone(), sealer(), "d_laptop", "Laptop").unwrap();
    laptop.set_member_wrap("d_laptop", Some(b"wrap for the laptop")).unwrap();
    let joined = laptop.set_member_wrap("d_phone", Some(b"wrap for the phone")).unwrap();
    let wraps = laptop.member_wraps().unwrap();
    assert_eq!(wraps.len(), 2);
    assert_eq!(wraps["d_phone"], b"wrap for the phone");

    laptop.set_member_wrap("d_phone", None).unwrap();
    let wraps = laptop.member_wraps().unwrap();
    assert_eq!(wraps.keys().collect::<Vec<_>>(), ["d_laptop"]);
    // The commit in which the phone was a member still has its wrap.
    assert!(laptop
        .file_at(&joined.head, ".azlin/keys/d_phone.key")
        .unwrap()
        .is_some());
    assert_eq!(laptop.history().unwrap().len(), 3);
    assert!(laptop.set_member_wrap("a/b", Some(b"x")).is_err(), "not a member id");
}

// ==== AZ-M15: one effective policy, a revision that only grows ====

/// The paths of the head's `.azlin` folder.
fn azlin_paths(repo: &Repo) -> Vec<String> {
    let root = repo.root().unwrap().unwrap();
    walk(repo.objects(), &root)
        .unwrap()
        .into_keys()
        .filter(|path| path.starts_with(".azlin"))
        .collect()
}

fn member(name: &str, role: Role) -> Member {
    Member {
        name: name.to_string(),
        role,
    }
}

fn grant(member: &str, folder: &str, access: Access) -> Grant {
    Grant {
        member: member.to_string(),
        folder: folder.to_string(),
        access,
    }
}

/// A laptop that wrote the policy, and a phone that read it.
fn laptop_and_phone() -> (Repo, Repo) {
    let bucket = MemoryBucket::new();
    let mut laptop = Repo::create(bucket.clone(), sealer(), "d_laptop", "Laptop").unwrap();
    laptop.set_policy(&policy()).unwrap();
    let phone = Repo::open(bucket, sealer(), "d_phone", "Phone").unwrap();
    (laptop, phone)
}

/// AZ-M15. Two devices rewrite the policy before either has heard of the other's rewrite: the
/// policy that is effective afterwards has both changes. It was the other device's policy
/// alone, with the revoke in a conflict copy nobody reads.
#[test]
fn az_m15_two_rewrites_of_the_policy_at_once_keep_what_each_added_and_what_each_removed() {
    let (mut laptop, mut phone) = laptop_and_phone();
    // The laptop adds a member; the phone revokes the TV.
    let mut with_nas = laptop.policy().unwrap().unwrap();
    with_nas
        .members
        .insert("d_nas".to_string(), member("NAS", Role::Editor));
    laptop.set_policy(&with_nas).unwrap();
    let mut without_tv = phone.policy().unwrap().unwrap();
    without_tv.members.remove("d_tv");
    without_tv.grants.retain(|grant| grant.member != "d_tv");
    phone.set_policy(&without_tv).unwrap();

    let merged = phone.policy().unwrap().unwrap();
    assert!(
        merged.members.contains_key("d_nas"),
        "the laptop's new member"
    );
    assert!(!merged.members.contains_key("d_tv"), "the phone's revoke");
    assert!(
        !merged.grants.iter().any(|grant| grant.member == "d_tv"),
        "and its grant"
    );
    assert_eq!(merged.revision, 2, "greater than both rewrites (1 and 1)");
    laptop.pull().unwrap();
    assert_eq!(
        laptop.policy().unwrap().unwrap(),
        merged,
        "the same on both devices"
    );
    assert_eq!(
        azlin_paths(&phone),
        [".azlin", POLICY_PATH],
        "no conflict copy of the policy"
    );
}

/// AZ-M15. The same race the other way round: the device that wrote first is the one that
/// revokes, and the late one adds - the revoke stands.
#[test]
fn az_m15_a_revoke_that_landed_first_is_not_undone_by_a_late_rewrite_made_without_it() {
    let (mut laptop, mut phone) = laptop_and_phone();
    let mut without_tv = laptop.policy().unwrap().unwrap();
    without_tv.members.remove("d_tv");
    without_tv.grants.retain(|grant| grant.member != "d_tv");
    laptop.set_policy(&without_tv).unwrap();
    // The phone has not heard of it.
    let mut with_nas = phone.policy().unwrap().unwrap();
    with_nas
        .members
        .insert("d_nas".to_string(), member("NAS", Role::Editor));
    with_nas
        .grants
        .push(grant("d_nas", "backup", Access::Write));
    phone.set_policy(&with_nas).unwrap();

    let merged = phone.policy().unwrap().unwrap();
    assert!(!merged.members.contains_key("d_tv"), "the revoke stands");
    assert!(merged.members.contains_key("d_nas"));
    assert!(merged.allows("d_nas", "backup/x", true));
    assert!(!merged.allows("d_tv", "docs/a.txt", false));
    assert_eq!(azlin_paths(&phone), [".azlin", POLICY_PATH]);
}

/// AZ-M15. The merge itself: what either side removed is gone, what either added is there, a
/// member changed two ways gets the lesser role, a grant added two ways the lesser access.
#[test]
fn az_m15_merging_two_rewrites_removes_what_either_removed_and_takes_the_lesser_of_two_changes() {
    let base = policy();
    let mut mine = base.clone();
    mine.members.get_mut("d_phone").unwrap().role = Role::Owner;
    mine.members.remove("d_tv");
    mine.grants.retain(|grant| grant.member != "d_tv");
    mine.grants[0].access = Access::Write; // Anna may write now
    mine.grants.push(grant("d_phone", "docs", Access::Write));
    mine.revision = 4;
    let mut theirs = base.clone();
    theirs.members.get_mut("d_phone").unwrap().role = Role::Viewer;
    theirs.members.get_mut("d_tv").unwrap().name = "Telly".to_string();
    theirs
        .members
        .insert("d_nas".to_string(), member("NAS", Role::Editor));
    theirs.grants.push(grant("d_nas", "backup", Access::Write));
    theirs.grants.push(grant("d_phone", "docs", Access::Read));
    theirs.revision = 7;

    let merged = Policy::merge(&base, &mine, &theirs);
    assert_eq!(
        merged.members.keys().collect::<Vec<_>>(),
        ["d_anna", "d_laptop", "d_nas", "d_phone"],
        "the TV is gone: mine removed it, and theirs only renamed it"
    );
    assert_eq!(
        merged.members["d_phone"].role,
        Role::Viewer,
        "the lesser of Owner and Viewer"
    );
    assert_eq!(merged.members["d_laptop"], base.members["d_laptop"]);
    assert_eq!(
        merged.grants,
        [
            grant("d_anna", "photos/2026/", Access::Write), // only mine changed it
            grant("d_nas", "backup", Access::Write),        // only theirs added it
            grant("d_phone", "docs", Access::Read),         // both added it: the lesser
        ],
        "sorted; the TV's grant went with the TV"
    );
    assert_eq!(merged.revision, 8, "greater than both sides'");
    // The sets do not depend on which side is which (the roles and accesses are the lesser).
    let flipped = Policy::merge(&base, &theirs, &mine);
    assert_eq!(flipped.grants, merged.grants);
    assert_eq!(
        flipped
            .members
            .iter()
            .map(|(id, m)| (id, m.role))
            .collect::<Vec<_>>(),
        merged
            .members
            .iter()
            .map(|(id, m)| (id, m.role))
            .collect::<Vec<_>>()
    );
}

/// AZ-M15. An older policy is never written back over a newer one by accident: a policy read
/// before another rewrite reached this device is refused.
#[test]
fn az_m15_a_policy_read_before_the_latest_rewrite_is_refused_and_writes_nothing() {
    let (mut laptop, _) = laptop_and_phone();
    let first = laptop.policy().unwrap().unwrap();
    let mut without_tv = first.clone();
    without_tv.members.remove("d_tv");
    laptop.set_policy(&without_tv).unwrap();
    let error = laptop.set_policy(&first).unwrap_err();
    assert!(matches!(error, MetaError::Conflict { .. }), "{error:?}");
    let now = laptop.policy().unwrap().unwrap();
    assert!(
        !now.members.contains_key("d_tv"),
        "the newer policy is still the policy"
    );
    assert_eq!(now.revision, 1);
}

/// AZ-M15. The first policy is revision 0 (and is written without the number), every rewrite
/// the next.
#[test]
fn az_m15_every_rewrite_of_the_policy_is_the_next_revision() {
    let (mut laptop, phone) = laptop_and_phone();
    assert_eq!(phone.policy().unwrap().unwrap().revision, 0);
    assert!(!policy().to_toml().unwrap().contains("revision"));
    for expected in 1..=3u64 {
        let current = laptop.policy().unwrap().unwrap();
        laptop.set_policy(&current).unwrap();
        assert_eq!(laptop.policy().unwrap().unwrap().revision, expected);
    }
    let text = laptop.policy().unwrap().unwrap().to_toml().unwrap();
    assert!(text.contains("revision = 3"), "{text}");
    assert_eq!(Policy::from_toml(&text).unwrap().revision, 3);
    assert_eq!(
        Policy::from_toml("[members.d_a]\nname = \"A\"\nrole = \"owner\"\n")
            .unwrap()
            .revision,
        0,
        "a policy written before the count"
    );
}

/// AZ-M15. A member the laptop removed stays removed when the tablet, which has not heard of it,
/// wraps the key for it again: the changed wrap used to win over the removal.
#[test]
fn az_m15_a_removed_key_wrap_stays_removed_when_another_device_wraps_it_at_once() {
    let bucket = MemoryBucket::new();
    let mut laptop = Repo::create(bucket.clone(), sealer(), "d_laptop", "Laptop").unwrap();
    laptop
        .set_member_wrap("d_laptop", Some(b"laptop wrap"))
        .unwrap();
    laptop
        .set_member_wrap("d_phone", Some(b"phone wrap 1"))
        .unwrap();
    let mut tablet = Repo::open(bucket, sealer(), "d_tablet", "Tablet").unwrap();
    laptop.set_member_wrap("d_phone", None).unwrap();
    tablet
        .set_member_wrap("d_phone", Some(b"phone wrap 2"))
        .unwrap();
    assert_eq!(
        tablet.member_wraps().unwrap().keys().collect::<Vec<_>>(),
        ["d_laptop"]
    );
    assert!(
        !azlin_paths(&tablet)
            .iter()
            .any(|path| path.contains("conflict")),
        "{:?}",
        azlin_paths(&tablet)
    );
}

/// AZ-M15. And the other way round: the tablet removes a wrap the laptop has changed at the same
/// time; the removal stands.
#[test]
fn az_m15_a_removal_stands_over_a_wrap_another_device_changed_at_once() {
    let bucket = MemoryBucket::new();
    let mut laptop = Repo::create(bucket.clone(), sealer(), "d_laptop", "Laptop").unwrap();
    laptop
        .set_member_wrap("d_laptop", Some(b"laptop wrap"))
        .unwrap();
    laptop
        .set_member_wrap("d_phone", Some(b"phone wrap 1"))
        .unwrap();
    let mut tablet = Repo::open(bucket, sealer(), "d_tablet", "Tablet").unwrap();
    laptop
        .set_member_wrap("d_phone", Some(b"phone wrap 2"))
        .unwrap();
    tablet.set_member_wrap("d_phone", None).unwrap();
    assert_eq!(
        tablet.member_wraps().unwrap().keys().collect::<Vec<_>>(),
        ["d_laptop"]
    );
    assert!(!azlin_paths(&tablet)
        .iter()
        .any(|path| path.contains("conflict")));
}

/// AZ-M15. Two new wraps for one member: the device that asked last has the effective one, and
/// the other is not left next to it as a file named like a key.
#[test]
fn az_m15_of_two_new_wraps_for_a_member_this_devices_is_the_effective_one() {
    let bucket = MemoryBucket::new();
    let mut laptop = Repo::create(bucket.clone(), sealer(), "d_laptop", "Laptop").unwrap();
    laptop
        .set_member_wrap("d_laptop", Some(b"laptop wrap"))
        .unwrap();
    laptop
        .set_member_wrap("d_phone", Some(b"phone wrap 1"))
        .unwrap();
    let mut tablet = Repo::open(bucket, sealer(), "d_tablet", "Tablet").unwrap();
    laptop
        .set_member_wrap("d_phone", Some(b"phone wrap 2"))
        .unwrap();
    tablet
        .set_member_wrap("d_phone", Some(b"phone wrap 3"))
        .unwrap();
    let wraps = tablet.member_wraps().unwrap();
    assert_eq!(wraps.keys().collect::<Vec<_>>(), ["d_laptop", "d_phone"]);
    assert_eq!(wraps["d_phone"], b"phone wrap 3");
}
