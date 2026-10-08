//! A drive this device holds: a grant kept as the drives file's entry, the Azlin record and two
//! secrets; join codes; the refresh with the rotating drive token; the emergency calls.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use azul_storage::{config::DrivesFile, testing::TempDir, Transport};
use serde_json::{json, Value};

use super::{header, json as reply, Fake, Shared, TOKEN};
use crate::{
    account::{read_grant, store_grant, Account, AccountFile, DriveRecord, JoinCode, JOIN_PREFIX},
    drive::TransportFactory,
    state::StateDir,
};

/// A sign-up answer in the token server's shape: drive `d_k3f9`, the drive token `token`.
fn answer(token: &str) -> Value {
    json!({
        "drive": {
            "id": "d_k3f9",
            "name": "Ann's drive",
            "location": {"kind": "s3", "endpoint": "http://127.0.0.1:9000", "region": "us-east-1",
                         "bucket": "d-k3f9", "path_style": true,
                         "auth": {"type": "azlin", "drive_id": "d_k3f9", "account_url": ""}}
        },
        "credentials": {"access_key_id": "AZTKEY", "secret_access_key": "sesame",
                        "session_token": "st", "expires_at": "2026-10-08T21:00:00Z"},
        "failover": ["http://127.0.0.1:9001", "http://127.0.0.1:9002"],
        "nodes": [{"name": "n1", "url": "http://127.0.0.1:9001", "ready": true}],
        "quota_bytes": 100_000_000_000_i64,
        "read_only": false,
        "drive_token": token,
        "tier": "100GB",
    })
}

fn factory(fake: &Arc<Fake>) -> TransportFactory {
    let fake = fake.clone();
    Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>)
}

/// The transports of a test that must not talk to anyone.
fn no_network() -> TransportFactory {
    factory(&Fake::new(|_, _| Err(String::from("no network in this test"))))
}

/// A token server: a sign-up answers the drive with `dt_f.0.a`; a refresh with `dt_f.0.a`
/// answers `dt_f.1.b` (and that one `dt_f.2.c`); a member family's `dt_m.0.joins` is exchanged
/// for `dt_m.1.x`; a lockdown answers `dt_f.9.locked`; a token spent before, or any other, is
/// refused as reused.
fn token_server() -> Arc<Fake> {
    let spent: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    Fake::new(move |call, _| {
        let url = call.url.as_str();
        let bearer = header(call, "authorization")
            .unwrap_or_default()
            .trim_start_matches("Bearer ")
            .to_string();
        if url.ends_with("/v1/drives") {
            return Ok(reply(201, &answer("dt_f.0.a").to_string()));
        }
        if url.ends_with("/credentials") {
            let first_time = spent.lock().unwrap().insert(bearer.clone());
            let next = match (first_time, bearer.as_str()) {
                (true, "dt_f.0.a") => "dt_f.1.b",
                (true, "dt_f.1.b") => "dt_f.2.c",
                (true, "dt_m.0.joins") => "dt_m.1.x",
                _ => {
                    return Ok(reply(
                        401,
                        r#"{"error": "token_reuse", "message": "an old token was reused"}"#,
                    ))
                }
            };
            return Ok(reply(200, &answer(next).to_string()));
        }
        if url.ends_with("/members") {
            return Ok(reply(
                201,
                r#"{"member": "m_laptop", "drive_token": "dt_m.0.joins"}"#,
            ));
        }
        if url.ends_with("/lockdown") {
            return Ok(reply(200, &answer("dt_f.9.locked").to_string()));
        }
        Ok(reply(404, r#"{"error": "not_found"}"#))
    })
}

#[test]
fn a_grant_is_kept_as_azdrives_drive_entry_the_azlin_record_and_two_secrets() {
    let dir = TempDir::new("azcloud-grant");
    let state = StateDir::open(dir.path()).unwrap();
    let grant = read_grant(&answer("dt_f.0.a"), "http://127.0.0.1:8081", "owner", 100).unwrap();
    assert_eq!(grant.record.id, "d_k3f9");
    assert_eq!(grant.record.endpoint, "http://127.0.0.1:9000");
    assert_eq!(
        grant.record.expires_at,
        crate::parse_rfc3339("2026-10-08T21:00:00Z").unwrap()
    );
    assert_eq!(
        grant.record.node_urls(),
        vec!["http://127.0.0.1:9001", "http://127.0.0.1:9002"]
    );
    assert!(!format!("{grant:?}").contains("sesame"));
    store_grant(&state, &grant).unwrap();

    // AzDrive can read the drives file: the auth is the keyring one it knows.
    let drives = DrivesFile::load(&state.drives_file()).unwrap();
    let entry = drives.get("d_k3f9").expect("the drive entry");
    assert!(entry.needs_keyring());
    assert_eq!(entry.s3_config().unwrap().bucket, "d-k3f9");
    let text = std::fs::read_to_string(state.drives_file()).unwrap();
    assert!(!text.contains("sesame"), "no secret in drives.json");

    let account = Account::open(&state, "http://127.0.0.1:8081", no_network(), None).unwrap();
    assert_eq!(account.drive_token().unwrap(), "dt_f.0.a");
    let creds = account.credentials().unwrap();
    assert_eq!(creds.access_key_id, "AZTKEY");
    assert_eq!(creds.session_token.as_deref(), Some("st"));
    assert_eq!(account.token_url_moved(), None);
    let moved = Account::open(&state, "http://127.0.0.1:18081", no_network(), None).unwrap();
    assert_eq!(
        moved.token_url_moved(),
        Some(("http://127.0.0.1:8081", "http://127.0.0.1:18081"))
    );

    // The next grant replaces the token and keeps one entry.
    let next = read_grant(&answer("dt_f.1.b"), "http://127.0.0.1:8081", "owner", 200).unwrap();
    store_grant(&state, &next).unwrap();
    let account =
        Account::open(&state, "http://127.0.0.1:8081", no_network(), Some("d_k3f9")).unwrap();
    assert_eq!(account.drive_token().unwrap(), "dt_f.1.b");
    assert_eq!(
        AccountFile::load(&state.account_file())
            .unwrap()
            .drives
            .len(),
        1
    );
}

#[test]
fn an_answer_without_credentials_or_token_is_no_grant_and_the_error_quotes_nothing() {
    let mut v = answer("");
    assert!(read_grant(&v, "u", "owner", 0)
        .unwrap_err()
        .to_string()
        .contains("drive token"));
    v["drive_token"] = json!("dt_x");
    v["credentials"]["secret_access_key"] = json!("");
    assert!(read_grant(&v, "u", "owner", 0).is_err());
    v["drive"] = json!("sesame");
    let err = read_grant(&v, "u", "owner", 0).unwrap_err().to_string();
    assert!(!err.contains("sesame"), "{err}");
    v = answer("dt_x");
    v["drive"]["id"] = json!("../../keys");
    assert!(read_grant(&v, "u", "owner", 0).is_err());
}

#[test]
fn credentials_are_renewed_six_hours_before_they_expire_and_a_long_lived_key_never() {
    let dir = TempDir::new("azcloud-refresh");
    let state = StateDir::open(dir.path()).unwrap();
    let grant = read_grant(&answer("dt_f.0.a"), "http://t", "owner", 0).unwrap();
    let expires = grant.record.expires_at;
    store_grant(&state, &grant).unwrap();
    let account = Account::open(&state, "http://t", no_network(), None).unwrap();
    assert!(!account.needs_refresh(expires - 7 * 3600));
    assert!(account.needs_refresh(expires - 5 * 3600));
    assert!(account.needs_refresh(expires + 1));
    let mut long = answer("dt_f.0.a");
    long["credentials"] = json!({"access_key_id": "AZK1", "secret_access_key": "s"});
    let grant = read_grant(&long, "http://t", "owner", 0).unwrap();
    assert_eq!(grant.record.expires_at, 0);
}

#[test]
fn a_join_code_round_trips_hides_its_token_and_refuses_what_is_not_one() {
    let code = JoinCode {
        drive_id: String::from("d_k3f9"),
        drive_token: String::from("dt_m.0.sesame"),
        member: String::from("m_laptop"),
        name: String::from("Ann's drive"),
        token_url: String::from("http://127.0.0.1:8081"),
    };
    let text = code.encode();
    assert!(text.starts_with(JOIN_PREFIX));
    assert!(
        text[JOIN_PREFIX.len()..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "URL-safe base64 without padding: {text}"
    );
    assert_eq!(JoinCode::decode(&format!("  {text}\n")).unwrap(), code);
    assert_eq!(
        JoinCode::decode(&format!("{text}==")).unwrap(),
        code,
        "padding is taken"
    );
    assert!(!format!("{code:?}").contains("sesame"));
    assert!(JoinCode::decode("dt_m.0.sesame").is_err());
    assert!(JoinCode::decode("azlin-join:%%%").is_err());
    assert!(JoinCode::decode("azlin-join:e").is_err(), "one character holds no byte");
    let mut bad = code.clone();
    bad.drive_id = String::from("../x");
    assert!(JoinCode::decode(&bad.encode()).is_err());
}

#[test]
fn the_current_drive_is_the_one_named_else_the_last_added_else_the_only_one() {
    let rec = |id: &str| DriveRecord {
        id: id.to_string(),
        name: String::new(),
        bucket: String::new(),
        endpoint: String::new(),
        region: String::new(),
        path_style: true,
        token_url: String::new(),
        member: String::new(),
        expires_at: 0,
        refreshed_at: 0,
        nodes: vec![],
        failover: vec![],
        quota_bytes: None,
        read_only: false,
        tier: None,
    };
    let mut file = AccountFile::default();
    assert!(file.get(None).is_none());
    file.drives.push(rec("d_a"));
    assert_eq!(file.get(None).map(|d| d.id.as_str()), Some("d_a"));
    file.put(rec("d_b"));
    assert_eq!(file.get(None).map(|d| d.id.as_str()), Some("d_b"));
    assert_eq!(file.get(Some("d_a")).map(|d| d.id.as_str()), Some("d_a"));
    assert!(file.get(Some("d_c")).is_none());
}

#[test]
fn a_signed_up_device_refreshes_with_the_rotated_token_and_invites_another_that_joins() {
    let server = token_server();
    let a_dir = TempDir::new("azcloud-device-a");
    let a_state = StateDir::open(a_dir.path()).unwrap();
    let mut a = Account::signup(
        &a_state,
        &format!("{TOKEN}/"),
        factory(&server),
        "100GB",
        "Ann's drive",
    )
    .unwrap();
    assert_eq!(a.record().id, "d_k3f9");
    assert_eq!(a.record().member, "owner");
    assert_eq!(a.record().token_url, TOKEN, "kept without its trailing slash");
    assert_eq!(a.drive_token().unwrap(), "dt_f.0.a");
    let signup = &server.calls()[0];
    let body: Value = serde_json::from_slice(&signup.body).unwrap();
    assert_eq!(body["tier"], "100GB");
    assert_eq!(body["name"], "Ann's drive");

    a.refresh().unwrap();
    assert_eq!(
        a.drive_token().unwrap(),
        "dt_f.1.b",
        "the rotated token is kept at once"
    );
    let calls = server.calls();
    let refresh = calls
        .iter()
        .find(|c| c.url.ends_with("/credentials"))
        .unwrap();
    assert_eq!(header(refresh, "authorization"), Some("Bearer dt_f.0.a"));
    let body: Value = serde_json::from_slice(&refresh.body).unwrap();
    assert_eq!(body["name"], "Ann's drive");

    let code = a.invite(Some("laptop")).unwrap();
    assert_eq!(code.drive_token, "dt_m.0.joins");
    assert_eq!(code.member, "m_laptop");
    assert_eq!(code.token_url, TOKEN);
    let b_dir = TempDir::new("azcloud-device-b");
    let b_state = StateDir::open(b_dir.path()).unwrap();
    let decoded = JoinCode::decode(&code.encode()).unwrap();
    let b = Account::join(&b_state, TOKEN, factory(&server), &decoded).unwrap();
    assert_eq!(b.record().id, "d_k3f9");
    assert_eq!(b.record().member, "m_laptop");
    assert_eq!(
        b.drive_token().unwrap(),
        "dt_m.1.x",
        "each device has a token family of its own"
    );
    assert_eq!(a.drive_token().unwrap(), "dt_f.1.b");

    // A spent code joins nobody, and the answer says to sign in again.
    let again = Account::join(&b_state, TOKEN, factory(&server), &code).unwrap_err();
    assert!(again.is_sign_in(), "{again}");
}

#[test]
fn a_lockdown_keeps_this_devices_new_grant_and_a_restore_wants_an_rfc_3339_time() {
    let server = token_server();
    let dir = TempDir::new("azcloud-lockdown");
    let state = StateDir::open(dir.path()).unwrap();
    let mut a =
        Account::signup(&state, TOKEN, factory(&server), "100GB", "Ann's drive").unwrap();
    let locked = a.lockdown().unwrap();
    assert_eq!(locked["locked_down"], true);
    assert_eq!(locked["drive"], "d_k3f9");
    assert_eq!(locked["credentials_expire"], "2026-10-08T21:00:00Z");
    assert!(
        !locked.to_string().contains("dt_f.9"),
        "what is returned holds no secret"
    );
    assert_eq!(a.drive_token().unwrap(), "dt_f.9.locked");
    let before = server.calls().len();
    assert!(a.restore("docs/", "yesterday").is_err());
    assert_eq!(
        server.calls().len(),
        before,
        "a time that is none is refused before anything is sent"
    );
}
