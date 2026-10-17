//! The token endpoint and the refreshing transport, against the S3 tests' scripted fake
//! (`super::s3::Fake`: it records every request and answers from a script).

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use super::s3::Fake;
use crate::{
    config::SecretOptions,
    oauth::{
        self, exchange_code, is_token_endpoint, refresh, signed_in, OAuthClient, OAuthError,
        RefreshingTransport, Tokens, CLIENT_ID, PLACEHOLDER_ACCESS_TOKEN, REFRESH_TOKEN, TOKEN_URL,
    },
    HttpCall, Method, Transport,
};

const TOKEN_URL_HTTPS: &str = "https://oauth2.example/token";
const JSON: &[(&str, &str)] = &[("Content-Type", "application/json")];

/// The form fields of a token request's body, decoded.
fn form(call: &HttpCall) -> BTreeMap<String, String> {
    String::from_utf8(call.body.clone())
        .unwrap()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap();
            (
                crate::sigv4::uri_decode(k).unwrap(),
                crate::sigv4::uri_decode(v).unwrap(),
            )
        })
        .collect()
}

fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
    call.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn api_call(token: &str) -> HttpCall {
    HttpCall {
        method: Method::Get,
        url: String::from("https://www.googleapis.com/drive/v3/files?q=x"),
        headers: vec![
            (String::from("authorization"), format!("Bearer {token}")),
            (String::from("accept"), String::from("application/json")),
        ],
        body: Vec::new(),
        content_type: String::new(),
    }
}

fn token_answer(access: &str, refresh: Option<&str>) -> String {
    match refresh {
        Some(r) => format!(
            r#"{{"access_token":"{access}","refresh_token":"{r}","expires_in":3599,"token_type":"Bearer","scope":"drive"}}"#
        ),
        None => format!(r#"{{"access_token":"{access}","expires_in":3599,"token_type":"Bearer"}}"#),
    }
}

// ==== The token endpoint ====

#[test]
fn exchanging_a_code_posts_the_code_the_verifier_the_redirect_uri_and_the_client_as_a_form() {
    let fake = Fake::default();
    fake.answer(200, JSON, &token_answer("ya29.A", Some("1//R")));
    let tokens = exchange_code(
        &fake,
        TOKEN_URL_HTTPS,
        &OAuthClient::public("app.apps.example"),
        "4/0Ab code",
        "verifier-43-characters-long-xxxxxxxxxxxxxxx",
        "http://127.0.0.1:53682/callback",
    )
    .unwrap();
    let call = fake.last();
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, TOKEN_URL_HTTPS);
    assert_eq!(call.content_type, "application/x-www-form-urlencoded");
    assert_eq!(header(&call, "accept"), Some("application/json"));
    let form = form(&call);
    assert_eq!(form["grant_type"], "authorization_code");
    assert_eq!(form["code"], "4/0Ab code");
    assert_eq!(
        form["code_verifier"],
        "verifier-43-characters-long-xxxxxxxxxxxxxxx"
    );
    assert_eq!(form["redirect_uri"], "http://127.0.0.1:53682/callback");
    assert_eq!(form["client_id"], "app.apps.example");
    assert!(
        !form.contains_key("client_secret"),
        "a public client has none"
    );
    assert_eq!(tokens.access_token, "ya29.A");
    assert_eq!(tokens.refresh_token.as_deref(), Some("1//R"));
    assert_eq!(tokens.expires_in, Some(3599));
    assert_eq!(tokens.scope.as_deref(), Some("drive"));
}

#[test]
fn a_client_secret_is_sent_only_when_the_client_has_one() {
    let fake = Fake::default();
    fake.answer(200, JSON, &token_answer("a", Some("r")));
    let client = OAuthClient::public("id").with_secret(Some("GOCSPX-not-secret"));
    exchange_code(
        &fake,
        TOKEN_URL_HTTPS,
        &client,
        "c",
        "v",
        "http://127.0.0.1/cb",
    )
    .unwrap();
    assert_eq!(form(&fake.last())["client_secret"], "GOCSPX-not-secret");
    assert_eq!(
        OAuthClient::public("id")
            .with_secret(Some("  "))
            .client_secret,
        None
    );
}

#[test]
fn an_oauth_error_answer_reads_as_its_error_and_its_description() {
    let fake = Fake::default();
    fake.answer(
        400,
        JSON,
        r#"{"error":"invalid_grant","error_description":"Bad Request"}"#,
    );
    let err = exchange_code(
        &fake,
        TOKEN_URL_HTTPS,
        &OAuthClient::public("id"),
        "used-code",
        "v",
        "http://127.0.0.1/cb",
    )
    .unwrap_err();
    assert_eq!(
        err,
        OAuthError::Rejected {
            status: 400,
            error: String::from("invalid_grant"),
            description: String::from("Bad Request"),
        }
    );
    assert!(err.needs_sign_in());
    assert!(err.to_string().contains("invalid_grant"), "{err}");
}

#[test]
fn a_refusal_without_a_json_body_says_its_status() {
    let fake = Fake::default();
    fake.answer(503, &[], "Service Unavailable");
    let err = refresh(&fake, TOKEN_URL_HTTPS, &OAuthClient::public("id"), "r").unwrap_err();
    assert!(
        matches!(err, OAuthError::Rejected { status: 503, .. }),
        "{err:?}"
    );
    assert!(!err.needs_sign_in());
    assert!(err.to_string().contains("503"), "{err}");
}

#[test]
fn an_answer_without_an_access_token_or_with_another_token_type_is_a_protocol_error() {
    let fake = Fake::default();
    fake.answer(200, JSON, r#"{"refresh_token":"r"}"#);
    fake.answer(200, JSON, r#"{"access_token":"a","token_type":"mac"}"#);
    fake.answer(200, &[], "<html>not json</html>");
    let client = OAuthClient::public("id");
    for _ in 0..3 {
        let err = refresh(&fake, TOKEN_URL_HTTPS, &client, "r").unwrap_err();
        assert!(matches!(err, OAuthError::Protocol(_)), "{err:?}");
    }
}

#[test]
fn a_code_or_a_token_never_goes_to_plain_http_off_this_computer() {
    assert!(is_token_endpoint("https://oauth2.googleapis.com/token"));
    assert!(is_token_endpoint("http://127.0.0.1:8081/oauth/token"));
    assert!(is_token_endpoint("http://localhost:9000/token"));
    assert!(is_token_endpoint("http://[::1]:9000/token"));
    assert!(!is_token_endpoint("http://oauth2.example/token"));
    assert!(!is_token_endpoint("http://127.0.0.1.evil.example/token"));
    assert!(!is_token_endpoint("ftp://oauth2.example/token"));
    assert!(!is_token_endpoint(""));
    let fake = Fake::default();
    let err = exchange_code(
        &fake,
        "http://oauth2.example/token",
        &OAuthClient::public("id"),
        "c",
        "v",
        "http://127.0.0.1/cb",
    )
    .unwrap_err();
    assert!(matches!(err, OAuthError::Protocol(_)), "{err:?}");
    assert!(fake.calls().is_empty(), "nothing was sent");
}

#[test]
fn a_request_without_an_answer_is_a_transport_error() {
    let fake = Fake::default();
    fake.fail("connection refused");
    let err = refresh(&fake, TOKEN_URL_HTTPS, &OAuthClient::public("id"), "r").unwrap_err();
    assert_eq!(
        err,
        OAuthError::Transport(String::from("connection refused"))
    );
}

#[test]
fn refreshing_posts_the_refresh_token_and_the_client() {
    let fake = Fake::default();
    fake.answer(200, JSON, &token_answer("new-access", None));
    let tokens = refresh(
        &fake,
        TOKEN_URL_HTTPS,
        &OAuthClient::public("id").with_secret(Some("s")),
        "1//refresh",
    )
    .unwrap();
    let form = form(&fake.last());
    assert_eq!(form["grant_type"], "refresh_token");
    assert_eq!(form["refresh_token"], "1//refresh");
    assert_eq!(form["client_id"], "id");
    assert_eq!(form["client_secret"], "s");
    assert_eq!(tokens.access_token, "new-access");
    assert_eq!(tokens.refresh_token, None);
}

#[test]
fn the_debug_output_of_tokens_a_client_and_the_transport_hides_the_secrets() {
    let tokens = Tokens {
        access_token: String::from("ya29.SECRET-A"),
        refresh_token: Some(String::from("1//SECRET-R")),
        expires_in: Some(1),
        scope: None,
    };
    let client = OAuthClient::public("id").with_secret(Some("SECRET-C"));
    let transport = RefreshingTransport::new(
        Box::new(Fake::default()),
        "Google Drive",
        TOKEN_URL_HTTPS,
        client.clone(),
        "1//SECRET-R",
    )
    .with_access_token("ya29.SECRET-A");
    for shown in [
        format!("{tokens:?}"),
        format!("{client:?}"),
        format!("{transport:?}"),
    ] {
        assert!(!shown.contains("SECRET"), "{shown}");
    }
}

// ==== The refreshing transport ====

fn refreshing(fake: &Fake) -> RefreshingTransport {
    RefreshingTransport::new(
        Box::new(fake.clone()),
        "Google Drive",
        TOKEN_URL_HTTPS,
        OAuthClient::public("id"),
        "1//R",
    )
}

#[test]
fn the_first_request_gets_an_access_token_first_and_carries_it_instead_of_the_placeholder() {
    let fake = Fake::default();
    fake.answer(200, JSON, &token_answer("A1", None));
    fake.answer(200, JSON, r#"{"files":[]}"#);
    let transport = refreshing(&fake);
    let reply = transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    assert_eq!(reply.status, 200);
    let calls = fake.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].url, TOKEN_URL_HTTPS);
    assert_eq!(form(&calls[0])["refresh_token"], "1//R");
    assert_eq!(
        calls[1].url,
        "https://www.googleapis.com/drive/v3/files?q=x"
    );
    assert_eq!(header(&calls[1], "authorization"), Some("Bearer A1"));
    assert_eq!(header(&calls[1], "accept"), Some("application/json"));
    // The next request uses the same token: no second refresh.
    fake.answer(200, JSON, "{}");
    transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(header(&calls[2], "authorization"), Some("Bearer A1"));
}

#[test]
fn a_401_refreshes_once_and_sends_the_request_again_with_the_new_token() {
    let fake = Fake::default();
    fake.answer(401, JSON, r#"{"error":{"code":401}}"#);
    fake.answer(200, JSON, &token_answer("A2", None));
    fake.answer(200, JSON, r#"{"files":[]}"#);
    let transport = refreshing(&fake).with_access_token("A1-expired");
    let reply = transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    assert_eq!(reply.status, 200);
    let calls = fake.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(
        header(&calls[0], "authorization"),
        Some("Bearer A1-expired")
    );
    assert_eq!(calls[1].url, TOKEN_URL_HTTPS);
    assert_eq!(header(&calls[2], "authorization"), Some("Bearer A2"));
    assert_eq!(calls[2].url, calls[0].url);
}

#[test]
fn a_second_401_is_returned_as_it_is() {
    let fake = Fake::default();
    fake.answer(401, JSON, "{}");
    fake.answer(200, JSON, &token_answer("A2", None));
    fake.answer(401, JSON, r#"{"error":"still no"}"#);
    let transport = refreshing(&fake).with_access_token("A1");
    let reply = transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    assert_eq!(reply.status, 401);
    assert_eq!(fake.calls().len(), 3, "one refresh, one retry, no loop");
}

#[test]
fn a_request_without_a_bearer_token_passes_untouched() {
    let fake = Fake::default();
    fake.answer(201, &[], "");
    let transport = refreshing(&fake);
    let upload = HttpCall {
        method: Method::Put,
        url: String::from("https://upload.example/session/abc"),
        headers: vec![(String::from("content-range"), String::from("bytes 0-1/2"))],
        body: vec![1, 2],
        content_type: String::from("application/octet-stream"),
    };
    let reply = transport.send(&upload).unwrap();
    assert_eq!(reply.status, 201);
    assert_eq!(fake.calls(), vec![upload], "no refresh, no header added");
}

#[test]
fn a_rotated_refresh_token_is_used_next_and_handed_on() {
    let fake = Fake::default();
    let kept: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&kept);
    let transport = refreshing(&fake)
        .on_new_refresh_token(move |token| sink.lock().unwrap().push(token.to_string()));
    fake.answer(200, JSON, &token_answer("A1", Some("R2")));
    fake.answer(401, JSON, "{}");
    fake.answer(200, JSON, &token_answer("A2", Some("R3")));
    fake.answer(200, JSON, "{}");
    let reply = transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    assert_eq!(reply.status, 200);
    let calls = fake.calls();
    assert_eq!(calls.len(), 4);
    assert_eq!(form(&calls[0])["refresh_token"], "1//R");
    assert_eq!(form(&calls[2])["refresh_token"], "R2", "the rotated one");
    assert_eq!(
        *kept.lock().unwrap(),
        vec![String::from("R2"), String::from("R3")]
    );
}

#[test]
fn a_refused_refresh_is_an_error_that_asks_to_sign_in_again_and_sends_nothing_else() {
    let fake = Fake::default();
    fake.answer(
        400,
        JSON,
        r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#,
    );
    let transport = refreshing(&fake);
    let why = transport
        .send(&api_call(PLACEHOLDER_ACCESS_TOKEN))
        .unwrap_err();
    assert!(why.contains("Google Drive"), "{why}");
    assert!(why.contains("sign in again"), "{why}");
    assert_eq!(fake.calls().len(), 1, "only the refresh");
}

// ==== Opening a signed-in drive ====

fn options(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn secrets(pairs: &[(&str, &str)]) -> SecretOptions {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_signed_in_drive_opens_with_the_placeholder_token_and_without_its_oauth_settings() {
    let fake = Fake::default();
    let (opts, secs, transport) = signed_in(
        "gdrive",
        &options(&[(CLIENT_ID, "id"), ("root", "/Work")]),
        &secrets(&[(REFRESH_TOKEN, "1//R"), ("client_secret", "s")]),
        Box::new(fake.clone()),
        None,
    );
    assert_eq!(opts, options(&[("root", "/Work")]));
    assert_eq!(secs, secrets(&[("access_token", PLACEHOLDER_ACCESS_TOKEN)]));
    // Its requests are refreshed with the drive's client and refresh token.
    fake.answer(200, JSON, &token_answer("A1", None));
    fake.answer(200, JSON, "{}");
    transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    let calls = fake.calls();
    assert_eq!(calls[0].url, oauth::GOOGLE.token_url);
    let refresh_form = form(&calls[0]);
    assert_eq!(refresh_form["client_id"], "id");
    assert_eq!(refresh_form["client_secret"], "s");
    assert_eq!(refresh_form["refresh_token"], "1//R");
    assert_eq!(header(&calls[1], "authorization"), Some("Bearer A1"));
}

#[test]
fn a_drive_signed_in_at_another_token_endpoint_refreshes_there() {
    let fake = Fake::default();
    let (opts, _, transport) = signed_in(
        "dropbox",
        &options(&[
            (CLIENT_ID, "id"),
            (TOKEN_URL, "http://127.0.0.1:8081/oauth/dropbox/token"),
        ]),
        &secrets(&[(REFRESH_TOKEN, "R")]),
        Box::new(fake.clone()),
        None,
    );
    assert!(!opts.contains_key(TOKEN_URL));
    fake.answer(200, JSON, &token_answer("A1", None));
    fake.answer(200, JSON, "{}");
    transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    assert_eq!(
        fake.calls()[0].url,
        "http://127.0.0.1:8081/oauth/dropbox/token"
    );
}

#[test]
fn a_drive_without_a_refresh_token_or_a_client_id_or_of_another_service_opens_as_it_is() {
    let cases = [
        (
            "gdrive",
            options(&[(CLIENT_ID, "id")]),
            secrets(&[("access_token", "pasted")]),
        ),
        ("onedrive", options(&[]), secrets(&[(REFRESH_TOKEN, "R")])),
        (
            "webdav",
            options(&[(CLIENT_ID, "id")]),
            secrets(&[(REFRESH_TOKEN, "R")]),
        ),
    ];
    for (scheme, opts, secs) in cases {
        let fake = Fake::default();
        let (o, s, transport) = signed_in(scheme, &opts, &secs, Box::new(fake.clone()), None);
        assert_eq!((&o, &s), (&opts, &secs), "{scheme}");
        fake.answer(200, &[], "");
        transport.send(&api_call("pasted")).unwrap();
        assert_eq!(fake.calls().len(), 1, "{scheme}: no refresh");
    }
}

#[test]
fn a_rotated_refresh_token_reaches_the_sink_as_the_drives_whole_keyring_text() {
    let fake = Fake::default();
    let kept: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&kept);
    let (_, _, transport) = signed_in(
        "onedrive",
        &options(&[(CLIENT_ID, "id")]),
        &secrets(&[(REFRESH_TOKEN, "R1"), ("client_secret", "s")]),
        Box::new(fake.clone()),
        Some(Box::new(move |text| sink.lock().unwrap().push(text))),
    );
    fake.answer(200, JSON, &token_answer("A1", Some("R2")));
    fake.answer(200, JSON, "{}");
    transport.send(&api_call(PLACEHOLDER_ACCESS_TOKEN)).unwrap();
    let kept = kept.lock().unwrap();
    assert_eq!(kept.len(), 1);
    let text = SecretOptions::from_keyring_secret(&kept[0]).unwrap();
    assert_eq!(
        text,
        secrets(&[(REFRESH_TOKEN, "R2"), ("client_secret", "s")])
    );
}

#[test]
fn the_providers_are_google_drive_dropbox_and_onedrive_at_https_endpoints() {
    let schemes: Vec<&str> = oauth::PROVIDERS.iter().map(|p| p.scheme).collect();
    assert_eq!(schemes, ["gdrive", "dropbox", "onedrive"]);
    for p in &oauth::PROVIDERS {
        assert!(p.authorize_url.starts_with("https://"), "{}", p.name);
        assert!(p.token_url.starts_with("https://"), "{}", p.name);
        assert_eq!(oauth::provider(p.scheme), Some(p));
    }
    assert_eq!(oauth::provider("webdav"), None);
    assert!(oauth::ONEDRIVE.scope.contains("offline_access"));
    assert!(oauth::GOOGLE
        .authorize_extras
        .contains(&("access_type", "offline")));
    assert!(oauth::DROPBOX
        .authorize_extras
        .contains(&("token_access_type", "offline")));
}
