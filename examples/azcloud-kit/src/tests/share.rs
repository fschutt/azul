//! Public links: a presigned GET cut to the credentials' life, a synced file shared by its blob.

use std::sync::Arc;

use azul_storage::{testing::TempDir, Transport};
use serde_json::json;

use super::{bundle, fake_s3::S3Bucket, Fake, Shared, S3, TOKEN};
use crate::{
    account::{read_grant, store_grant, Account},
    drive::TransportFactory,
    share::{public_link, synced_link},
    state::StateDir,
    sync::{sync_folder, LocalRoot, SyncOptions},
};

fn no_network() -> TransportFactory {
    let fake = Fake::new(|_, _| Err(String::from("no network in this test")));
    Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>)
}

fn account(dir: &TempDir, expires_at: &str) -> Account {
    let state = StateDir::open(dir.path()).unwrap();
    let answer = json!({
        "drive": {"id": "d_1", "name": "Ann's drive",
                  "location": {"kind": "s3", "endpoint": "http://127.0.0.1:9000", "bucket": "d-1",
                               "region": "us-east-1", "path_style": true}},
        "credentials": {"access_key_id": "AZTK", "secret_access_key": "s", "session_token": "t",
                        "expires_at": expires_at},
        "drive_token": "dt_f.0.x",
    });
    store_grant(&state, &read_grant(&answer, "http://t", "owner", 0).unwrap()).unwrap();
    Account::open(&state, "http://t", no_network(), None).unwrap()
}

#[test]
fn a_link_is_a_presigned_get_cut_to_the_credentials_life_with_the_reason() {
    let dir = TempDir::new("azcloud-share");
    let a = account(&dir, "2026-10-08T12:00:00Z");
    let expires = crate::parse_rfc3339("2026-10-08T12:00:00Z").unwrap();
    let now = expires - 3600;
    let link = public_link(&a, "http://127.0.0.1:9000", "docs/a.pdf", 600, now).unwrap();
    assert!(
        link.url
            .starts_with("http://127.0.0.1:9000/d-1/docs/a.pdf?"),
        "{}",
        link.url
    );
    assert!(link.url.contains("X-Amz-Signature="), "{}", link.url);
    assert!(link.url.contains("X-Amz-Expires=600"), "{}", link.url);
    assert!(
        link.url.contains("X-Amz-Security-Token=t"),
        "temporary credentials sign with their session token: {}",
        link.url
    );
    assert_eq!(link.expires_at, now + 600);
    assert!(link.note.is_none());
    let long = public_link(&a, "http://127.0.0.1:9000", "docs/a.pdf", 86_400, now).unwrap();
    assert_eq!(long.expires_at, expires);
    assert!(long.note.unwrap().contains("public_links"));
    assert!(public_link(&a, "http://127.0.0.1:9000", "a", 60, expires + 1).is_err());
}

#[test]
fn a_synced_file_is_shared_by_the_blob_its_index_names() {
    let store = S3Bucket::new();
    let folder = TempDir::new("azcloud-share-folder");
    let state_dir = TempDir::new("azcloud-share-state");
    std::fs::create_dir_all(folder.path().join("notes")).unwrap();
    std::fs::write(folder.path().join("notes").join("a.md"), b"alpha").unwrap();
    let opts = SyncOptions::new("e2e/share", "d-1", "dev-a").unwrap();
    sync_folder(
        &store,
        &LocalRoot::folder(folder.path()),
        &state_dir.path().join("index.json"),
        &opts,
    )
    .unwrap();

    let state = StateDir::open(&state_dir.path().join("state")).unwrap();
    let answer: serde_json::Value =
        serde_json::from_str(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    store_grant(&state, &read_grant(&answer, TOKEN, "owner", 0).unwrap()).unwrap();
    let account = Account::open(&state, TOKEN, no_network(), None).unwrap();
    let now = crate::parse_rfc3339("2026-10-08T20:15:00Z").unwrap();
    let link = synced_link(&store, &account, S3, "e2e/share", "notes/a.md", 600, now).unwrap();
    let hash = crate::sync::local::hash_bytes(b"alpha");
    assert!(
        link.url.starts_with(&format!(
            "{S3}/d-1/e2e/share/.azlin/blobs/{}/{hash}?",
            &hash[..2]
        )),
        "{}",
        link.url
    );
    assert!(link.note.unwrap().contains("hash"));
    assert!(synced_link(&store, &account, S3, "e2e/share", "notes/b.md", 600, now).is_err());
    assert!(synced_link(&store, &account, S3, "e2e/other", "notes/a.md", 600, now).is_err());
}
