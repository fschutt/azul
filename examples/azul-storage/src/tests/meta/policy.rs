//! The drive's policy and its members' key wraps as files of the drive index (feature
//! `encryption`).

use crate::meta::{
    policy::{Access, Grant, Member, Policy, Role, POLICY_PATH},
    MemoryBucket, MetaRepo, TestSealer,
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
