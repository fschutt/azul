//! The sandbox's rules, each with its test: loopback only, an unmistakably fake look, test data
//! only and nothing of it kept, no embedding elsewhere, never in a release; and the Stripe- and
//! GoCardless-shaped flows an app's backend drives against it, with webhooks signed the way the
//! real providers sign them.

use serde_json::{json, Value};

use crate::{
    dlocal_signature, form_decode, form_encode, gocardless_signature, guard, stripe_signature, Config,
    Core, Delivery, Error, Kind, Request, Response,
};

/// 2026-10-17 12:00 UTC.
const T: i64 = 1_791_892_800;
const BASE: &str = "http://127.0.0.1:8784";
const REF: &str = "pr_sandboxsandboxsandboxsand";

fn core() -> Core {
    Core::new(&Config::default(), BASE).expect("the default configuration")
}

/// A request as a loopback client sends it (`Host: 127.0.0.1:8784`), with `extra` headers.
fn call(c: &mut Core, method: &str, path_and_query: &str, extra: &[(&str, &str)], body: &[u8], now: i64) -> Response {
    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (path_and_query, None),
    };
    let mut headers: Vec<(String, String)> = vec![("Host".into(), "127.0.0.1:8784".into())];
    headers.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    c.handle(
        &Request {
            method,
            path,
            query,
            headers: &headers,
            body,
        },
        now,
    )
}

/// A call to the Stripe-shaped API with its default test key.
fn stripe_api(c: &mut Core, method: &str, path: &str, form: &[(&str, &str)], key: Option<&str>, now: i64) -> (u16, Value) {
    let auth = format!("Bearer {}", Config::default().stripe.secret_key);
    let mut extra = vec![("Authorization", auth.as_str()), ("Content-Type", "application/x-www-form-urlencoded")];
    if let Some(k) = key {
        extra.push(("Idempotency-Key", k));
    }
    let body = form_encode(form);
    let r = call(c, method, &format!("/stripe{path}"), &extra, body.as_bytes(), now);
    (r.status, serde_json::from_slice(&r.body).unwrap_or(Value::Null))
}

/// A new Checkout Session of `ui_mode` for `method`, as an app's backend makes it (each under an
/// idempotency key of its own); its JSON.
fn session(c: &mut Core, method: &str, ui_mode: &str, mode: &str) -> Value {
    let made = c.state()["stripe"]["sessions"].as_array().map_or(0, Vec::len);
    session_keyed(c, method, ui_mode, mode, &format!("{REF}/{made}"))
}

/// [`session`] under the idempotency key `key`.
fn session_keyed(c: &mut Core, method: &str, ui_mode: &str, mode: &str, key: &str) -> Value {
    let expires = (T + 3600).to_string();
    let mut form = vec![
        ("mode", mode),
        ("ui_mode", ui_mode),
        ("client_reference_id", REF),
        ("metadata[provider_ref]", REF),
        ("line_items[0][quantity]", "1"),
        ("line_items[0][price_data][currency]", "eur"),
        ("line_items[0][price_data][unit_amount]", "5999"),
        ("line_items[0][price_data][product_data][name]", "Storage 1TB, 12 months"),
        ("payment_method_types[0]", method),
        ("expires_at", &expires),
    ];
    if mode == "subscription" {
        form.push(("line_items[0][price_data][recurring][interval]", "month"));
        form.push(("subscription_data[metadata][provider_ref]", REF));
        form.push(("subscription_data[trial_period_days]", "30"));
    } else {
        form.push(("payment_intent_data[metadata][provider_ref]", REF));
    }
    if ui_mode == "elements" {
        form.push(("return_url", "http://127.0.0.1:8081/return/ok"));
    } else {
        form.push(("success_url", "http://127.0.0.1:8081/return/ok"));
        form.push(("cancel_url", "http://127.0.0.1:8081/return/cancel"));
    }
    let (status, v) = stripe_api(c, "POST", "/v1/checkout/sessions", &form, Some(key), T);
    assert_eq!(status, 200, "{v}");
    v
}

/// The hosted page's path of a session.
fn page_path(s: &Value) -> String {
    s["url"].as_str().expect("a hosted page").strip_prefix(BASE).expect("on the sandbox").to_string()
}

/// Posts the hosted page's form: `(status, location or body)`.
fn submit(c: &mut Core, path: &str, fields: &[(&str, &str)], now: i64) -> (u16, String) {
    let body = form_encode(fields);
    let r = call(c, "POST", path, &[("Content-Type", "application/x-www-form-urlencoded")], body.as_bytes(), now);
    let text = r.header("location").map(String::from).unwrap_or_else(|| String::from_utf8_lossy(&r.body).into_owned());
    (r.status, text)
}

/// The Stripe events of `deliveries`, each signature checked: their types.
fn stripe_events(deliveries: &[Delivery]) -> Vec<Value> {
    let secret = Config::default().stripe.webhook_secret;
    deliveries
        .iter()
        .filter(|d| d.provider == "fake-stripe")
        .map(|d| {
            assert_eq!(d.url, "http://127.0.0.1:8081/v1/webhook/fake-stripe");
            let header = d.headers.iter().find(|(k, _)| k == "Stripe-Signature").map(|(_, v)| v.clone()).expect("signed");
            let t: i64 = header.split(',').find_map(|p| p.strip_prefix("t=")).and_then(|t| t.parse().ok()).expect("t=");
            assert_eq!(header, stripe_signature(&secret, t, &d.body), "the signature Stripe's scheme gives");
            serde_json::from_slice(&d.body).expect("an event")
        })
        .collect()
}

fn types(events: &[Value]) -> Vec<String> {
    events.iter().map(|e| e["type"].as_str().unwrap_or("").to_string()).collect()
}

/// Every text the sandbox kept or sent: its state, and the bodies of `deliveries`.
fn everything(c: &Core, deliveries: &[Delivery]) -> String {
    let mut all = c.state().to_string();
    for d in deliveries {
        all.push_str(&String::from_utf8_lossy(&d.body));
    }
    all
}

/// Every single value (string or number) the sandbox kept or sent, as text: a short secret
/// such as a CVC is looked for as a whole value, since random ids may contain any three digits.
fn values(c: &Core, deliveries: &[Delivery]) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Number(n) => out.push(n.to_string()),
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            Value::Object(o) => o.values().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&c.state(), &mut out);
    for d in deliveries {
        walk(&serde_json::from_slice(&d.body).unwrap_or(Value::Null), &mut out);
    }
    out
}

// ---- 1. loopback only ----

#[test]
fn the_sandbox_refuses_a_webhook_url_that_leaves_this_computer() {
    for good in [
        "http://127.0.0.1:8081/v1/webhook/{provider}",
        "http://localhost:3000/hooks",
        "http://[::1]:8081/",
    ] {
        assert!(guard::loopback_webhook_url(good).is_ok(), "{good}");
    }
    for bad in [
        "http://203.0.113.5/hooks",
        "https://hooks.example.com/stripe",
        "http://127.0.0.1.nip.io/hooks",
        "http://user@127.0.0.1@evil.example/",
        "http://localhost.evil.example/",
        "ftp://127.0.0.1/",
        "",
    ] {
        assert!(guard::loopback_webhook_url(bad).is_err(), "{bad}");
    }
    let cfg = Config {
        webhook_url: "https://hooks.example.com/{provider}".into(),
        ..Config::default()
    };
    assert!(matches!(Core::new(&cfg, BASE), Err(Error::NotLoopback(_))));
    assert!(matches!(crate::start(cfg), Err(Error::NotLoopback(_))));
}

#[test]
fn the_sandbox_listens_on_loopback_only() {
    let running = crate::start(Config {
        port: 0,
        ..Config::default()
    })
    .expect("starts on a free port");
    assert!(running.base_url().starts_with("http://127.0.0.1:"), "{}", running.base_url());
    assert_eq!(running.api_url(Kind::Stripe), format!("{}/stripe", running.base_url()));
    running.stop();
}

#[test]
fn a_request_naming_another_host_is_refused_so_dns_rebinding_gets_nothing() {
    let mut c = core();
    for host in ["127.0.0.1:8784", "localhost:8784", "[::1]:8784", "localhost", "127.0.0.1"] {
        let headers = vec![("Host".to_string(), host.to_string())];
        let r = c.handle(&Request { method: "GET", path: "/health", query: None, headers: &headers, body: b"" }, T);
        assert_eq!(r.status, 200, "{host}");
    }
    for host in ["evil.example", "127.0.0.1.evil.example", "localhost.evil.example:8784", "10.0.0.2:8784", ""] {
        let headers = vec![("Host".to_string(), host.to_string())];
        let r = c.handle(&Request { method: "GET", path: "/health", query: None, headers: &headers, body: b"" }, T);
        assert_eq!(r.status, 403, "{host:?}");
    }
    let r = c.handle(&Request { method: "GET", path: "/health", query: None, headers: &[], body: b"" }, T);
    assert_eq!(r.status, 403, "no Host header at all");
}

// ---- 2. an unmistakably fake look ----

/// Every page an app or a payer may see.
fn pages(c: &mut Core) -> Vec<(String, String)> {
    let card = session(c, "card", "hosted_page", "payment");
    let mut out = Vec::new();
    for path in ["/".to_string(), "/fields/fake-stripe/v1".to_string(), page_path(&card)] {
        let r = call(c, "GET", &path, &[], b"", T);
        assert_eq!(r.status, 200, "{path}");
        out.push((path, String::from_utf8_lossy(&r.body).into_owned()));
    }
    c.handle(&Request { method: "POST", path: "/_dev/switch", query: None, headers: &[("Host".into(), "localhost".into())], body: br#"{"name": "three_ds_next"}"# }, T);
    let (status, challenge) = submit(c, &page_path(&card), &[("card_number", "4242 4242 4242 4242"), ("exp", "12/34"), ("cvc", "123"), ("country", "DE")], T);
    assert_eq!(status, 303, "{challenge}");
    let challenge = challenge.strip_prefix(BASE).unwrap_or(&challenge).to_string();
    let r = call(c, "GET", &challenge, &[], b"", T);
    out.push((challenge, String::from_utf8_lossy(&r.body).into_owned()));
    let flow = gocardless_flow(c);
    let r = call(c, "GET", &flow, &[], b"", T);
    out.push((flow, String::from_utf8_lossy(&r.body).into_owned()));
    out
}

#[test]
fn every_page_says_test_in_its_title_ribbon_and_watermark_and_shows_no_provider_brand() {
    let mut c = core();
    for (path, html) in pages(&mut c) {
        let title = html.split("<title>").nth(1).and_then(|t| t.split("</title>").next()).unwrap_or("");
        assert!(title.starts_with("[TEST]"), "{path}: {title}");
        assert!(html.contains("TEST - local fake"), "{path}: the ribbon");
        assert!(html.contains("TEST - NOT A REAL PAYMENT PAGE"), "{path}: the watermark");
        assert!(html.contains("hazard"), "{path}: the hazard frame");
        assert!(!html.contains("<img") && !html.contains("<svg"), "{path}: no logo, no artwork");
        for brand in ["stripe.com", "gocardless.com", "js.stripe", "font-face"] {
            assert!(!html.to_ascii_lowercase().contains(brand), "{path}: {brand}");
        }
        assert!(!html.contains("@import") && !html.contains("https://"), "{path}: nothing loaded from elsewhere");
    }
}

// ---- 3. test data only, and nothing of it kept ----

#[test]
fn only_documented_test_cards_are_taken_and_nothing_of_a_card_or_its_holder_is_kept() {
    let mut c = core();
    let s = session(&mut c, "card", "hosted_page", "payment");
    let path = page_path(&s);
    // Luhn-valid numbers that are not Stripe's test cards: a real card is never taken
    for real in ["4111 1111 1111 1111", "5105 1051 0510 5100", "4012 8888 8888 1881"] {
        let (status, page) = submit(&mut c, &path, &[("card_number", real), ("exp", "12/34"), ("cvc", "987"), ("country", "DE")], T);
        assert_eq!(status, 200, "{real}");
        assert!(page.contains("This sandbox only accepts test card numbers"), "{real}: {page}");
        assert!(!page.contains(&real.replace(' ', "")), "the number is not echoed");
    }
    let (status, back) = submit(&mut c, &path, &[("card_number", "4242 4242 4242 4242"), ("exp", "12/34"), ("cvc", "987"), ("country", "DE"), ("name", "Erika Muster")], T);
    assert_eq!((status, back.as_str()), (303, "http://127.0.0.1:8081/return/ok"));
    let sent = c.tick(T);
    let all = everything(&c, &sent);
    for secret in ["4242424242424242", "4242 4242", "4111111111111111", "Erika", "Muster"] {
        assert!(!all.contains(secret), "{secret} was kept or sent: {all}");
    }
    let kept = values(&c, &sent);
    for short in ["987", "12/34", "1234"] {
        assert!(!kept.iter().any(|v| v == short), "{short} was kept or sent: {kept:?}");
    }
    assert!(types(&stripe_events(&sent)).contains(&"checkout.session.completed".to_string()));
}

#[test]
fn only_documented_test_ibans_are_taken() {
    let mut c = core();
    let s = session(&mut c, "sepa_debit", "hosted_page", "payment");
    let path = page_path(&s);
    // a real IBAN with a correct checksum is refused all the same
    let (status, page) = submit(&mut c, &path, &[("iban", "DE02 1203 0000 0000 2020 51"), ("name", "Erika Muster")], T);
    assert_eq!(status, 200);
    assert!(page.contains("This sandbox only accepts test IBANs"), "{page}");
    let (status, page) = submit(&mut c, &path, &[("iban", "DE89 3704 0044 0532 0130 01")], T);
    assert!(status == 200 && page.contains("IBAN"), "a wrong checksum: {page}");
    let (status, _) = submit(&mut c, &path, &[("iban", "DE89 3704 0044 0532 0130 00"), ("name", "Erika Muster")], T);
    assert_eq!(status, 303);
    let sent = c.tick(T + 60);
    let all = everything(&c, &sent);
    for secret in ["DE89370400440532013000", "DE89 3704", "Erika", "DE02120300000000202051"] {
        assert!(!all.contains(secret), "{secret} was kept or sent");
    }
}

// ---- 4. no embedding elsewhere ----

#[test]
fn every_answer_forbids_framing_outside_this_computer_and_sends_no_referrer() {
    let mut c = core();
    let s = session(&mut c, "card", "hosted_page", "payment");
    for path in ["/".to_string(), "/fields/fake-stripe/v1".to_string(), page_path(&s), "/health".to_string(), "/nope".to_string()] {
        let r = call(&mut c, "GET", &path, &[], b"", T);
        let csp = r.header("content-security-policy").unwrap_or("");
        assert!(csp.contains("frame-ancestors 'self' http://127.0.0.1:* http://localhost:* http://[::1]:*"), "{path}: {csp}");
        assert!(!csp.contains("frame-ancestors *"), "{path}");
        assert_eq!(r.header("x-frame-options"), Some("SAMEORIGIN"), "{path}");
        assert_eq!(r.header("referrer-policy"), Some("no-referrer"), "{path}");
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"), "{path}");
    }
}

// ---- 5. never in a release ----

#[test]
fn a_release_build_refuses_to_start() {
    assert_eq!(guard::refuse_in_release(false, false), Err(Error::Release));
    assert_eq!(guard::refuse_in_release(true, false), Ok(()));
    let plain = crate::server::start_as(Config { port: 0, ..Config::default() }, false);
    assert!(matches!(plain, Err(Error::Release)), "no opt-in, no start");
}

#[test]
fn a_release_build_starts_for_the_local_test_stack_and_keeps_it_on_loopback_and_test_data() {
    assert_eq!(guard::refuse_in_release(false, true), Ok(()));
    // The opt-in is the switch or the environment variable set to 1, nothing else.
    assert!(guard::release_opted_in(Some("1")));
    for no in [None, Some(""), Some("0"), Some("yes"), Some("true")] {
        assert!(!guard::release_opted_in(no), "{no:?}");
    }
    assert_eq!(guard::ALLOW_RELEASE_ENV, "AZ_PAY_SANDBOX_ALLOW_RELEASE");

    let stack = || Config { port: 0, local_test_stack: true, ..Config::default() };
    let running = crate::server::start_as(stack(), false).expect("the local test stack's sandbox starts");
    assert!(running.base_url().starts_with("http://127.0.0.1:"), "{}", running.base_url());
    running.stop();
    // Off this computer it is refused all the same.
    let off = Config { webhook_url: "https://hooks.example.com/{provider}".into(), ..stack() };
    assert!(matches!(crate::server::start_as(off, false), Err(Error::NotLoopback(_))));
    // And a real card is refused all the same.
    let mut c = Core::new(&stack(), BASE).expect("the stack's configuration");
    let s = session(&mut c, "card", "hosted_page", "payment");
    let (status, page) = submit(&mut c, &page_path(&s), &[("card_number", "4111 1111 1111 1111"), ("exp", "12/34"), ("cvc", "987"), ("country", "DE")], T);
    assert_eq!(status, 200);
    assert!(page.contains("This sandbox only accepts test card numbers"), "{page}");
}

// ---- 6. the README says it plainly ----

#[test]
fn the_readme_states_the_rules() {
    let readme = include_str!("../README.md");
    for rule in [
        "local test tool",
        "does not reproduce any provider's real UI or branding",
        "refuses real payment data",
        "Loopback only",
        "Unmistakably fake look",
        "Test data only",
        "No embedding elsewhere",
        "Never in release",
    ] {
        assert!(readme.contains(rule), "{rule}");
    }
}

// ---- the Stripe-shaped flows ----

#[test]
fn the_stripe_api_wants_its_test_key_and_keeps_an_idempotency_key_to_its_parameters() {
    let mut c = core();
    let form = [("mode", "payment")];
    let r = call(&mut c, "POST", "/stripe/v1/checkout/sessions", &[("Authorization", "Bearer sk_test_other")], form_encode(&form).as_bytes(), T);
    assert_eq!(r.status, 401);
    let key = format!("{REF}/once");
    let a = session_keyed(&mut c, "card", "hosted_page", "payment", &key);
    let b = session_keyed(&mut c, "card", "hosted_page", "payment", &key);
    assert_eq!(a["id"], b["id"], "a retried create makes no second session");
    let (status, err) = stripe_api(&mut c, "POST", "/v1/checkout/sessions", &[("mode", "payment")], Some(&key), T);
    assert_eq!(status, 400);
    assert_eq!(err["error"]["type"], "idempotency_error", "{err}");
    let (status, err) = stripe_api(&mut c, "POST", "/v1/checkout/sessions", &[("mode", "payment"), ("ui_mode", "elements"), ("cancel_url", "http://127.0.0.1:1/")], None, T);
    assert_eq!(status, 400, "{err}");
}

#[test]
fn a_card_is_declined_then_paid_in_the_same_session_with_the_documented_messages() {
    let mut c = core();
    let s = session(&mut c, "card", "hosted_page", "payment");
    let path = page_path(&s);
    let card = |n: &str| vec![("card_number", n.to_string()), ("exp", "12/34".into()), ("cvc", "123".into()), ("country", "DE".into())];
    for (number, message) in [
        ("4000 0000 0000 0002", "Your card was declined."),
        ("4000 0000 0000 9995", "Your card has insufficient funds."),
        ("4000 0000 0000 0069", "Your card has expired."),
        ("4000 0000 0000 0127", "Your card's security code is incorrect."),
    ] {
        let fields = card(number);
        let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (status, page) = submit(&mut c, &path, &pairs, T);
        assert_eq!(status, 200, "{number}");
        assert!(page.contains(&crate::kit::esc(message)), "{number}: {page}");
    }
    let fields = card("4242 4242 4242 4242");
    let pairs: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let (status, back) = submit(&mut c, &path, &pairs, T);
    assert_eq!((status, back.as_str()), (303, "http://127.0.0.1:8081/return/ok"));
    let events = stripe_events(&c.tick(T));
    let t = types(&events);
    assert_eq!(t.iter().filter(|e| *e == "payment_intent.payment_failed").count(), 4, "{t:?}");
    assert!(t.contains(&"payment_intent.succeeded".to_string()) && t.contains(&"checkout.session.completed".to_string()), "{t:?}");
    let completed = events.iter().find(|e| e["type"] == "checkout.session.completed").unwrap();
    assert_eq!(completed["data"]["object"]["client_reference_id"], REF);
    assert_eq!(completed["data"]["object"]["payment_status"], "paid");
    let (status, _) = submit(&mut c, &path, &pairs, T);
    assert_eq!(status, 409, "a completed session takes no second payment");
}

#[test]
fn three_d_secure_asks_on_its_own_page_and_fails_or_completes_there() {
    let mut c = core();
    let s = session(&mut c, "card", "hosted_page", "payment");
    let path = page_path(&s);
    let (status, challenge) = submit(&mut c, &path, &[("card_number", "4000 0025 0000 3155"), ("exp", "12/34"), ("cvc", "123"), ("country", "DE")], T);
    assert_eq!(status, 303);
    let challenge = challenge.strip_prefix(BASE).expect("the sandbox's challenge page").to_string();
    let page = String::from_utf8_lossy(&call(&mut c, "GET", &challenge, &[], b"", T).body).into_owned();
    assert!(page.contains("Complete") && page.contains("Fail"), "{page}");
    assert!(c.tick(T).is_empty(), "nothing paid before the challenge");
    let (status, back) = submit(&mut c, &challenge, &[("action", "fail")], T);
    assert_eq!(status, 303, "back to the page for another try");
    assert_eq!(back, s["url"].as_str().unwrap());
    let (_, challenge) = submit(&mut c, &path, &[("card_number", "4000 0027 6000 3184"), ("exp", "12/34"), ("cvc", "123"), ("country", "DE")], T);
    let challenge = challenge.strip_prefix(BASE).unwrap().to_string();
    let (status, back) = submit(&mut c, &challenge, &[("action", "complete")], T);
    assert_eq!((status, back.as_str()), (303, "http://127.0.0.1:8081/return/ok"));
    let t = types(&stripe_events(&c.tick(T)));
    assert!(t.contains(&"checkout.session.completed".to_string()), "{t:?}");
}

/// A scripted run in a real window cannot type into the web view: `/_dev/fill` makes the NEXT
/// fields page come with a test card in its inputs, as if the payer had typed it, and the page
/// tells the app the brand and that the card is complete once it is ready. Once: the page after
/// it is empty again. Only digits are taken; `/_dev/reset` drops a fill that was not used.
#[test]
fn a_fill_brings_the_next_fields_page_with_a_test_card_typed_in_once() {
    let mut c = core();
    let page = |c: &mut Core| String::from_utf8_lossy(&call(c, "GET", "/fields/fake-stripe/v1", &[], b"", T).body).into_owned();
    let empty = page(&mut c);
    assert!(!empty.contains("value=\"4242"), "no fill, no card");
    let json_type = [("Content-Type", "application/json")];
    let fill = json!({"card_number": "4242424242424242", "exp": "1234", "cvc": "123"}).to_string();
    let r = call(&mut c, "POST", "/_dev/fill", &json_type, fill.as_bytes(), T);
    assert_eq!(r.status, 200);
    let filled = page(&mut c);
    for part in ["value=\"4242 4242 4242 4242\"", "value=\"12 / 34\"", "value=\"123\"", "data-filled"] {
        assert!(filled.contains(part), "{part} in the filled page");
    }
    assert!(filled.contains("dataset.filled"), "the page says brand and complete once ready");
    let again = page(&mut c);
    assert!(!again.contains("value=\"4242") && !again.contains("data-filled"), "once only");
    let bad = json!({"card_number": "4242<script>", "exp": "1234", "cvc": "123"}).to_string();
    assert_eq!(call(&mut c, "POST", "/_dev/fill", &json_type, bad.as_bytes(), T).status, 400);
    call(&mut c, "POST", "/_dev/fill", &json_type, fill.as_bytes(), T);
    call(&mut c, "POST", "/_dev/reset", &[], b"", T);
    assert!(!page(&mut c).contains("data-filled"), "a reset drops an unused fill");
}

#[test]
fn the_fields_page_answers_through_the_bridge_and_confirms_with_the_client_secret() {
    let mut c = core();
    let s = session(&mut c, "card", "elements", "payment");
    assert!(s["url"].is_null(), "elements has no hosted page");
    let secret = s["client_secret"].as_str().expect("a client secret").to_string();
    let page = String::from_utf8_lossy(&call(&mut c, "GET", "/fields/fake-stripe/v1", &[], b"", T).body).into_owned();
    for part in ["/_bridge/ready", "/_bridge/complete", "/_bridge/result", "hashchange", "cmd", "/stripe/confirm"] {
        assert!(page.contains(part), "{part}");
    }
    let r = call(&mut c, "GET", "/_bridge/ready", &[], b"", T);
    assert_eq!(r.status, 204, "the app cancels it; a browser stays on the page");
    let info = call(&mut c, "GET", &format!("/stripe/elements?cs={secret}"), &[], b"", T);
    let info: Value = serde_json::from_slice(&info.body).unwrap();
    assert_eq!((info["method"].as_str(), info["amount"].as_i64()), (Some("card"), Some(5999)), "{info}");
    let confirm = |c: &mut Core, number: &str| {
        let body = json!({"client_secret": secret, "card_number": number, "exp": "12/34", "cvc": "123", "country": "DE", "name": "Erika Muster"});
        let r = call(c, "POST", "/stripe/confirm", &[("Content-Type", "application/json")], body.to_string().as_bytes(), T);
        (r.status, serde_json::from_slice::<Value>(&r.body).unwrap_or(Value::Null))
    };
    let (_, declined) = confirm(&mut c, "4000000000000002");
    assert_eq!((declined["result"].as_str(), declined["code"].as_str()), (Some("failed"), Some("card_declined")));
    let (_, real) = confirm(&mut c, "4111111111111111");
    assert_eq!(real["code"], "sandbox_test_cards_only", "{real}");
    let (status, paid) = confirm(&mut c, "4242424242424242");
    assert_eq!((status, paid["result"].as_str(), paid["last4"].as_str()), (200, Some("succeeded"), Some("4242")));
    let (status, _) = confirm(&mut c, "4242424242424242");
    assert_eq!(status, 409);
    let state = c.state();
    assert!(state["bridge"].as_array().is_some_and(|b| b.iter().any(|m| m == "ready")), "{state}");
}

#[test]
fn a_sepa_debit_settles_on_the_compressed_timeline_or_is_returned_or_disputed() {
    let mut c = core();
    let pay = |c: &mut Core, iban: &str, now: i64| {
        let s = session(c, "sepa_debit", "hosted_page", "payment");
        let (status, _) = submit(c, &page_path(&s), &[("iban", iban)], now);
        assert_eq!(status, 303, "{iban}");
    };
    pay(&mut c, "DE89370400440532013000", T);
    let first = types(&stripe_events(&c.tick(T)));
    assert!(first.contains(&"payment_intent.processing".to_string()) && first.contains(&"checkout.session.completed".to_string()), "{first:?}");
    assert!(!first.contains(&"payment_intent.succeeded".to_string()));
    let later = types(&stripe_events(&c.tick(T + 30)));
    assert!(later.contains(&"payment_intent.succeeded".to_string()) && later.contains(&"checkout.session.async_payment_succeeded".to_string()), "{later:?}");
    pay(&mut c, "AT861904300235473202", T + 40);
    let failed = types(&stripe_events(&c.tick(T + 80)));
    assert!(failed.contains(&"payment_intent.payment_failed".to_string()) && failed.contains(&"checkout.session.async_payment_failed".to_string()), "{failed:?}");
    c.handle(&Request { method: "POST", path: "/_dev/switch", query: None, headers: &[("Host".into(), "localhost".into())], body: br#"{"name": "return_next_debit"}"# }, T + 90);
    pay(&mut c, "DE89370400440532013000", T + 90);
    let returned = types(&stripe_events(&c.tick(T + 130)));
    assert!(returned.contains(&"payment_intent.payment_failed".to_string()), "the switch returns a good IBAN's debit: {returned:?}");
    pay(&mut c, "AT591904300235473203", T + 140);
    let disputed = types(&stripe_events(&c.tick(T + 200)));
    assert!(disputed.contains(&"charge.dispute.created".to_string()), "{disputed:?}");
}

#[test]
fn a_subscription_starts_with_its_trial_renews_when_advanced_and_ends_when_deleted() {
    let mut c = core();
    let s = session(&mut c, "card", "hosted_page", "subscription");
    submit(&mut c, &page_path(&s), &[("card_number", "4242424242424242"), ("exp", "12/34"), ("cvc", "123"), ("country", "DE")], T);
    let start = stripe_events(&c.tick(T));
    let invoice = start.iter().find(|e| e["type"] == "invoice.paid").expect("the trial's invoice");
    assert_eq!(invoice["data"]["object"]["amount_paid"], 0);
    assert_eq!(invoice["data"]["object"]["parent"]["subscription_details"]["metadata"]["provider_ref"], REF);
    let r = c.handle(&Request { method: "POST", path: "/_dev/advance", query: None, headers: &[("Host".into(), "localhost".into())], body: br#"{"provider": "fake-stripe", "months": 2}"# }, T);
    assert_eq!(r.status, 200);
    let renewed = stripe_events(&c.tick(T));
    let paid: Vec<&Value> = renewed.iter().filter(|e| e["type"] == "invoice.paid").collect();
    assert_eq!(paid.len(), 2);
    assert!(paid.iter().all(|e| e["data"]["object"]["amount_paid"] == 5999 && e["data"]["object"]["billing_reason"] == "subscription_cycle"));
    let sub = invoice["data"]["object"]["parent"]["subscription_details"]["subscription"].as_str().unwrap().to_string();
    let (status, v) = stripe_api(&mut c, "DELETE", &format!("/v1/subscriptions/{sub}"), &[], None, T);
    assert_eq!((status, v["status"].as_str()), (200, Some("canceled")));
    let ended = types(&stripe_events(&c.tick(T)));
    assert!(ended.contains(&"customer.subscription.deleted".to_string()), "{ended:?}");
}

#[test]
fn a_refund_names_its_payment_intent_and_a_retry_refunds_nothing_more() {
    let mut c = core();
    let s = session(&mut c, "card", "hosted_page", "payment");
    submit(&mut c, &page_path(&s), &[("card_number", "4242424242424242"), ("exp", "12/34"), ("cvc", "123"), ("country", "DE")], T);
    c.tick(T);
    let id = s["id"].as_str().unwrap();
    let (_, read) = stripe_api(&mut c, "GET", &format!("/v1/checkout/sessions/{id}"), &[], None, T);
    let pi = read["payment_intent"].as_str().expect("its PaymentIntent").to_string();
    let form = [("payment_intent", pi.as_str()), ("amount", "1000"), ("metadata[provider_ref]", REF)];
    let key = format!("{REF}/refund/{pi}/1000");
    let (status, refund) = stripe_api(&mut c, "POST", "/v1/refunds", &form, Some(&key), T);
    assert_eq!((status, refund["amount"].as_i64()), (200, Some(1000)), "{refund}");
    let t = types(&stripe_events(&c.tick(T)));
    assert!(t.contains(&"refund.created".to_string()) && t.contains(&"charge.refunded".to_string()), "{t:?}");
    let (status, again) = stripe_api(&mut c, "POST", "/v1/refunds", &form, Some(&key), T);
    assert_eq!((status, &again), (200, &refund));
    assert!(c.tick(T).is_empty());
    let (status, _) = stripe_api(&mut c, "POST", "/v1/refunds", &[("payment_intent", pi.as_str()), ("amount", "999999")], None, T);
    assert_eq!(status, 400, "more than was paid");
}

#[test]
fn an_open_session_expires_and_a_paid_one_does_not() {
    let mut c = core();
    let s = session(&mut c, "card", "hosted_page", "payment");
    let id = s["id"].as_str().unwrap().to_string();
    let (status, v) = stripe_api(&mut c, "POST", &format!("/v1/checkout/sessions/{id}/expire"), &[], None, T);
    assert_eq!((status, v["status"].as_str()), (200, Some("expired")));
    assert!(types(&stripe_events(&c.tick(T))).contains(&"checkout.session.expired".to_string()));
    let (status, _) = submit(&mut c, &page_path(&s), &[("card_number", "4242424242424242"), ("exp", "12/34"), ("cvc", "123")], T);
    assert_eq!(status, 409, "an expired session takes nothing");
    let paid = session(&mut c, "paypal", "hosted_page", "payment");
    let (status, _) = submit(&mut c, &page_path(&paid), &[("approve", "1")], T);
    assert_eq!(status, 303, "PayPal through Stripe: approved on its test page");
    let pid = paid["id"].as_str().unwrap().to_string();
    let (status, _) = stripe_api(&mut c, "POST", &format!("/v1/checkout/sessions/{pid}/expire"), &[], None, T);
    assert_eq!(status, 400);
}

// ---- the GoCardless-shaped flow ----

/// A Billing Request with its flow, as an app's backend makes it: the flow page's path.
fn gocardless_flow(c: &mut Core) -> String {
    gocardless_scheme_flow(c, "sepa_core", "EUR").expect("a SEPA billing request")
}

/// A Billing Request with a mandate request of `scheme` in `currency` and its flow: the flow
/// page's path, or the refusal's status and body.
fn gocardless_scheme_flow(c: &mut Core, scheme: &str, currency: &str) -> Result<String, (u16, String)> {
    let token = format!("Bearer {}", Config::default().gocardless.secret_key);
    let headers = [("Authorization", token.as_str()), ("GoCardless-Version", "2015-07-06"), ("Content-Type", "application/json")];
    let br = json!({"billing_requests": {
        "mandate_request": {"scheme": scheme, "currency": currency, "metadata": {"provider_ref": REF}},
        "subscription_request": {"amount": 5999, "currency": currency, "interval_unit": "monthly", "interval": 1,
                                 "count": 1, "name": "Storage 1TB, 12 months", "metadata": {"provider_ref": REF}},
        "metadata": {"provider_ref": REF}}});
    let mut h: Vec<(&str, &str)> = headers.to_vec();
    let key = format!("{REF}/{}", c.state()["gocardless"]["billing_requests"].as_array().map_or(0, Vec::len));
    h.push(("Idempotency-Key", key.as_str()));
    let r = call(c, "POST", "/gocardless/billing_requests", &h, br.to_string().as_bytes(), T);
    if r.status != 201 {
        return Err((r.status, String::from_utf8_lossy(&r.body).into_owned()));
    }
    let id = serde_json::from_slice::<Value>(&r.body).unwrap()["billing_requests"]["id"].as_str().unwrap().to_string();
    let flow = json!({"billing_request_flows": {"redirect_uri": "http://127.0.0.1:8081/return/ok",
                                                "exit_uri": "http://127.0.0.1:8081/return/cancel",
                                                "links": {"billing_request": id}}});
    let r = call(c, "POST", "/gocardless/billing_request_flows", &headers, flow.to_string().as_bytes(), T);
    assert_eq!(r.status, 201);
    let url = serde_json::from_slice::<Value>(&r.body).unwrap()["billing_request_flows"]["authorisation_url"].as_str().unwrap().to_string();
    Ok(url.strip_prefix(BASE).expect("on the sandbox").to_string())
}

fn gocardless_events(deliveries: &[Delivery]) -> Vec<Value> {
    let secret = Config::default().gocardless.webhook_secret;
    let mut out = Vec::new();
    for d in deliveries.iter().filter(|d| d.provider == "fake-gocardless") {
        let header = d.headers.iter().find(|(k, _)| k == "Webhook-Signature").map(|(_, v)| v.clone()).expect("signed");
        assert_eq!(header, gocardless_signature(&secret, &d.body));
        let batch: Value = serde_json::from_slice(&d.body).unwrap();
        out.extend(batch["events"].as_array().cloned().unwrap_or_default());
    }
    out
}

fn actions(events: &[Value]) -> Vec<String> {
    events.iter().map(|e| format!("{}.{}", e["resource_type"].as_str().unwrap_or(""), e["action"].as_str().unwrap_or(""))).collect()
}

#[test]
fn the_gocardless_flow_takes_a_test_iban_then_the_mandate_then_the_bank_confirmation() {
    let mut c = core();
    let flow = gocardless_flow(&mut c);
    let page = String::from_utf8_lossy(&call(&mut c, "GET", &flow, &[], b"", T).body).into_owned();
    assert!(page.contains("name=\"iban\"") && page.contains("name=\"holder\""), "{page}");
    let (status, confirm) = submit(&mut c, &flow, &[("iban", "DE02 1203 0000 0000 2020 51"), ("holder", "Erika Muster")], T);
    assert!(status == 200 && confirm.contains("This sandbox only accepts test IBANs"), "{confirm}");
    let (status, mandate) = submit(&mut c, &flow, &[("iban", "AT61 1904 3002 3457 3201"), ("holder", "Erika Muster")], T);
    assert_eq!(status, 200);
    assert!(mandate.contains("mandate") && mandate.contains("3201") && !mandate.contains("AT611904300234573201"), "the IBAN masked: {mandate}");
    assert!(mandate.contains("Erika Muster"), "the holder is shown back, from the form, never kept");
    let (status, bank) = submit(&mut c, &format!("{flow}/mandate"), &[("iban", "AT611904300234573201"), ("holder", "Erika Muster")], T);
    assert_eq!(status, 200);
    assert!(bank.contains("Confirm"), "the bank's confirmation step: {bank}");
    let (status, back) = submit(&mut c, &format!("{flow}/bank"), &[("iban", "AT611904300234573201"), ("decision", "confirm")], T);
    assert_eq!((status, back.as_str()), (303, "http://127.0.0.1:8081/return/ok"));
    let sent = c.tick(T);
    let first = actions(&gocardless_events(&sent));
    assert!(first.contains(&"billing_requests.fulfilled".to_string()) && first.contains(&"payments.created".to_string()), "{first:?}");
    let all = everything(&c, &sent);
    assert!(!all.contains("Erika") && !all.contains("AT611904300234573201"), "nothing of the payer kept");
    let later = actions(&gocardless_events(&c.tick(T + 30)));
    for step in ["payments.submitted", "payments.confirmed", "payments.paid_out"] {
        assert!(later.contains(&step.to_string()), "{step}: {later:?}");
    }
}

#[test]
fn the_dev_switches_drop_repeat_and_delay_webhooks() {
    let mut c = core();
    let dev = |c: &mut Core, path: &str, body: &str| {
        let r = c.handle(&Request { method: "POST", path, query: None, headers: &[("Host".into(), "localhost".into())], body: body.as_bytes() }, T);
        assert_eq!(r.status, 200, "{path} {body}");
    };
    dev(&mut c, "/_dev/switch", r#"{"name": "drop_next_webhook"}"#);
    let s = session(&mut c, "card", "hosted_page", "payment");
    submit(&mut c, &page_path(&s), &[("card_number", "4242424242424242"), ("exp", "12/34"), ("cvc", "123")], T);
    let sent = c.tick(T);
    assert_eq!(c.state()["webhooks"]["dropped"], 1);
    assert!(!sent.is_empty(), "only the first one is lost");
    dev(&mut c, "/_dev/switch", r#"{"name": "duplicate_next_webhook"}"#);
    dev(&mut c, "/_dev/delay", r#"{"secs": 30}"#);
    let s = session(&mut c, "card", "hosted_page", "payment");
    submit(&mut c, &page_path(&s), &[("card_number", "5555555555554444"), ("exp", "12/34"), ("cvc", "123")], T);
    assert!(c.tick(T).is_empty(), "delayed");
    let late = c.tick(T + 31);
    let ids: Vec<String> = late.iter().filter_map(|d| serde_json::from_slice::<Value>(&d.body).ok()).filter_map(|v| v["id"].as_str().map(String::from)).collect();
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(ids.len(), unique.len() + 1, "one sent twice: {ids:?}");
    // a delivery the backend did not take comes again, signed anew
    let d = late[0].clone();
    c.delivered(d.clone(), Some(500), T + 31);
    let again = c.tick(T + 60);
    assert!(again.iter().any(|x| x.body == d.body && x.attempt == 1), "retried");
}

/// A backend's reconcile has to find a payment none of whose webhooks arrived: `/_dev/drop`
/// with an id loses every webhook that names it, and only those.
#[test]
fn every_webhook_that_names_a_dropped_id_is_lost_and_only_those() {
    let mut c = core();
    let lost = session(&mut c, "card", "hosted_page", "payment");
    let kept = session(&mut c, "card", "hosted_page", "payment");
    let (lost_id, kept_id) = (lost["id"].as_str().expect("an id"), kept["id"].as_str().expect("an id"));
    let r = call(&mut c, "POST", "/_dev/drop", &[], json!({"id": lost_id}).to_string().as_bytes(), T);
    assert_eq!(r.status, 200);
    for (s, card) in [(&lost, "4242424242424242"), (&kept, "5555555555554444")] {
        submit(&mut c, &page_path(s), &[("card_number", card), ("exp", "12/34"), ("cvc", "123")], T);
    }
    let sent = c.tick(T);
    let names = |d: &Delivery, id: &str| String::from_utf8_lossy(&d.body).contains(id);
    assert!(!sent.iter().any(|d| names(d, lost_id)), "a webhook of the dropped id arrived");
    assert!(sent.iter().any(|d| names(d, kept_id)), "the other payment's webhooks arrive");
    assert!(c.state()["webhooks"]["dropped"].as_u64() >= Some(1));
    // an id that is no object's id is refused
    let r = call(&mut c, "POST", "/_dev/drop", &[], br#"{"id": "cs_a\"b"}"#, T);
    assert_eq!(r.status, 400);
}

/// `/_dev/drop` with a count loses that many of the next webhooks, then none.
#[test]
fn a_counted_drop_loses_that_many_of_the_next_webhooks() {
    let mut c = core();
    let r = call(&mut c, "POST", "/_dev/drop", &[], br#"{"count": 2}"#, T);
    assert_eq!(r.status, 200);
    let s = session(&mut c, "card", "hosted_page", "payment");
    submit(&mut c, &page_path(&s), &[("card_number", "4242424242424242"), ("exp", "12/34"), ("cvc", "123")], T);
    let sent = c.tick(T);
    assert_eq!(c.state()["webhooks"]["dropped"], 2);
    assert!(!sent.is_empty(), "the webhooks after the second arrive");
    assert_eq!(c.state()["switches"]["drop_webhooks"], 0);
}

#[test]
fn form_encoding_round_trips() {
    let pairs = [("metadata[provider_ref]", "pr_x"), ("name", "a b&c=ä")];
    let back = form_decode(&form_encode(&pairs));
    assert_eq!(back, vec![("metadata[provider_ref]".to_string(), "pr_x".to_string()), ("name".to_string(), "a b&c=ä".to_string())]);
}

#[test]
fn the_sandbox_tells_a_backend_its_test_keys_and_api_addresses() {
    let mut c = core();
    let r = call(&mut c, "GET", "/_dev/keys", &[], b"", T);
    assert_eq!(r.status, 200);
    let keys: Value = serde_json::from_slice(&r.body).unwrap();
    let stripe = &keys["providers"]["fake-stripe"];
    assert_eq!(stripe["api"], format!("{BASE}/stripe"));
    assert_eq!(stripe["secret_key"], Config::default().stripe.secret_key.as_str());
    assert_eq!(stripe["webhook_secret"], Config::default().stripe.webhook_secret.as_str());
    assert_eq!(stripe["publishable_key"], Config::default().stripe.publishable_key.as_str());
    assert_eq!(stripe["fields_page"], format!("{BASE}/fields/fake-stripe/v1"));
    let gc = &keys["providers"]["fake-gocardless"];
    assert_eq!(gc["api"], format!("{BASE}/gocardless"));
    assert_eq!(gc["secret_key"], Config::default().gocardless.secret_key.as_str());
    assert_eq!(keys["webhook_url"], Config::default().webhook_url.as_str());
    // a sandbox for one provider names that one only
    let only = Config {
        providers: vec![Kind::GoCardless],
        ..Config::default()
    };
    let mut c = Core::new(&only, BASE).unwrap();
    let r = call(&mut c, "GET", "/_dev/keys", &[], b"", T);
    let keys: Value = serde_json::from_slice(&r.body).unwrap();
    assert!(keys["providers"].get("fake-stripe").is_none(), "{keys}");
}

// ---- the local payment methods (the registry's routes with documented test data) ----

/// A hosted-page session for `method` in `currency`, with `extra` parameters; `(status, json)`.
fn local_session(c: &mut Core, method: &str, currency: &str, mode: &str, extra: &[(&str, &str)]) -> (u16, Value) {
    let made = c.state()["stripe"]["sessions"].as_array().map_or(0, Vec::len);
    let key = format!("{REF}/local/{made}");
    let expires = (T + 3600).to_string();
    let mut form = vec![
        ("mode", mode),
        ("ui_mode", "hosted_page"),
        ("client_reference_id", REF),
        ("metadata[provider_ref]", REF),
        ("line_items[0][quantity]", "1"),
        ("line_items[0][price_data][currency]", currency),
        ("line_items[0][price_data][unit_amount]", "5999"),
        ("line_items[0][price_data][product_data][name]", "Storage 1TB, 12 months"),
        ("payment_method_types[0]", method),
        ("expires_at", &expires),
        ("success_url", "http://127.0.0.1:8081/return/ok"),
        ("cancel_url", "http://127.0.0.1:8081/return/cancel"),
    ];
    if mode == "subscription" {
        form.push(("line_items[0][price_data][recurring][interval]", "month"));
    } else {
        form.push(("payment_intent_data[metadata][provider_ref]", REF));
    }
    form.extend_from_slice(extra);
    stripe_api(c, "POST", "/v1/checkout/sessions", &form, Some(&key), T)
}

/// [`local_session`] that must be made: its JSON.
fn local(c: &mut Core, method: &str, currency: &str, extra: &[(&str, &str)]) -> Value {
    let (status, v) = local_session(c, method, currency, "payment", extra);
    assert_eq!(status, 200, "{method} in {currency}: {v}");
    v
}

fn get_page(c: &mut Core, path: &str) -> String {
    let r = call(c, "GET", path, &[], b"", T);
    assert_eq!(r.status, 200, "{path}");
    String::from_utf8_lossy(&r.body).into_owned()
}

/// The fake phone's path a page links to (the QR's loopback URL, or the approval step).
fn phone_path(html: &str) -> String {
    let at = html.find("/stripe/phone/").expect("a link to the fake phone");
    let rest = &html[at..];
    let end = rest.find(['"', '<', ' ']).unwrap_or(rest.len());
    rest[..end].to_string()
}

fn has(events: &[String], kind: &str) -> bool {
    events.iter().any(|e| e == kind)
}

#[test]
fn every_local_method_page_says_test_and_shows_no_brand_and_no_real_bank() {
    let registry = azul_pay_methods::Registry::builtin().expect("the registry");
    let mut c = core();
    let mut seen = 0;
    for route in registry.routes.iter().filter(|r| r.provider == "stripe" && r.faked()) {
        if ["card", "sepa_debit", "paypal"].contains(&route.provider_type.as_str()) {
            continue;
        }
        let currency = route.currencies.first().cloned().unwrap_or_default().to_ascii_lowercase();
        let extra: &[(&str, &str)] = if route.provider_type == "wechat_pay" { &[("payment_method_options[wechat_pay][client]", "web")] } else { &[] };
        let s = local(&mut c, &route.provider_type, &currency, extra);
        let html = get_page(&mut c, &page_path(&s));
        let title = html.split("<title>").nth(1).and_then(|t| t.split("</title>").next()).unwrap_or("");
        assert!(title.starts_with("[TEST]"), "{}: {title}", route.method);
        assert!(html.contains("TEST - local fake") && html.contains("TEST - NOT A REAL PAYMENT PAGE"), "{}", route.method);
        assert!(!html.contains("<img") && !html.contains("<svg") && !html.contains("https://"), "{}: no logo, nothing from elsewhere", route.method);
        for real in ["ING", "Rabobank", "ABN AMRO", "PKO", "mBank", "Erste", "Raiffeisen", "KBC", "Belfius", "stripe.com"] {
            assert!(!html.contains(real), "{}: {real} is a real bank or brand", route.method);
        }
        seen += 1;
    }
    assert!(seen >= 25, "the local methods with documented test data: {seen}");
}

#[test]
fn a_bank_redirect_lists_test_banks_and_is_authorized_or_failed_there() {
    let mut c = core();
    let s = local(&mut c, "ideal", "eur", &[]);
    let path = page_path(&s);
    let html = get_page(&mut c, &path);
    assert!(html.contains("Test Bank A") && html.contains("Test Bank B"), "{html}");
    let (status, page) = submit(&mut c, &path, &[("bank", "a"), ("action", "fail")], T);
    assert_eq!(status, 200, "a failure stays on the page: {page}");
    let (status, back) = submit(&mut c, &path, &[("bank", "a"), ("action", "authorize")], T);
    assert_eq!((status, back.as_str()), (303, "http://127.0.0.1:8081/return/ok"));
    let events = types(&stripe_events(&c.tick(T)));
    assert!(has(&events, "payment_intent.payment_failed") && has(&events, "payment_intent.succeeded"), "{events:?}");
    assert!(has(&events, "checkout.session.completed"), "{events:?}");
}

#[test]
fn a_blik_code_is_approved_on_the_fake_phone_and_signed_webhooks_follow() {
    let mut c = core();
    let s = local(&mut c, "blik", "pln", &[]);
    let path = page_path(&s);
    let (status, phone) = submit(&mut c, &path, &[("code", "123456")], T);
    assert_eq!(status, 303, "{phone}");
    let phone = phone.strip_prefix(BASE).unwrap_or(&phone).to_string();
    assert!(phone.starts_with("/stripe/phone/"), "{phone}");
    let html = get_page(&mut c, &phone);
    assert!(html.contains("[TEST]") && html.contains("Approve"), "{html}");
    let first = types(&stripe_events(&c.tick(T)));
    assert!(has(&first, "payment_intent.processing"), "waiting for the bank app: {first:?}");
    let (status, _) = submit(&mut c, &phone, &[("action", "authorize")], T + 5);
    assert_eq!(status, 200);
    let later = types(&stripe_events(&c.tick(T + 5)));
    assert!(has(&later, "payment_intent.succeeded") && has(&later, "checkout.session.completed"), "{later:?}");
    let r = call(&mut c, "GET", &format!("{path}/status"), &[], b"", T + 5);
    let status: Value = serde_json::from_slice(&r.body).expect("json");
    assert_eq!(status["status"], "complete");
}

#[test]
fn blik_fails_by_its_documented_email_or_times_out_on_the_phone() {
    let mut c = core();
    let s = local(&mut c, "blik", "pln", &[]);
    let (status, page) = submit(&mut c, &page_path(&s), &[("code", "123456"), ("email", "customer_declined@example.com")], T);
    assert_eq!(status, 200, "declined at once: {page}");
    assert!(types(&stripe_events(&c.tick(T))).contains(&"payment_intent.payment_failed".to_string()));
    let s = local(&mut c, "blik", "pln", &[]);
    let (_, phone) = submit(&mut c, &page_path(&s), &[("code", "123456")], T);
    let phone = phone.strip_prefix(BASE).unwrap_or(&phone).to_string();
    submit(&mut c, &phone, &[("action", "timeout")], T + 61);
    let events = stripe_events(&c.tick(T + 61));
    let failed = events.iter().find(|e| e["type"] == "payment_intent.payment_failed").expect("timed out");
    assert_eq!(failed["data"]["object"]["last_payment_error"]["code"], "payment_method_provider_timeout");
}

#[test]
fn input_that_is_not_documented_test_data_is_refused_and_not_kept() {
    let mut c = core();
    let s = local(&mut c, "blik", "pln", &[]);
    let (status, page) = submit(&mut c, &page_path(&s), &[("code", "654321")], T);
    assert_eq!(status, 200);
    assert!(page.contains("only accepts the documented test data"), "{page}");
    assert!(!page.contains("654321"), "not echoed");
    let s = local(&mut c, "mb_way", "eur", &[]);
    let (status, page) = submit(&mut c, &page_path(&s), &[("phone", "+351912345678")], T);
    assert_eq!(status, 200);
    assert!(page.contains("only accepts the documented test data"), "{page}");
    let sent = c.tick(T);
    let all = everything(&c, &sent);
    for secret in ["654321", "+351912345678", "912345678"] {
        assert!(!all.contains(secret), "{secret} was kept or sent");
    }
}

#[test]
fn a_qr_code_is_a_loopback_test_url_and_never_a_real_payment_payload() {
    let mut c = core();
    for (method, currency) in [("twint", "chf"), ("swish", "sek"), ("upi", "inr"), ("wero", "eur")] {
        let s = local(&mut c, method, currency, &[]);
        let html = get_page(&mut c, &page_path(&s));
        assert!(html.contains("http://127.0.0.1:8784/stripe/phone/"), "{method}: {html}");
        for payload in ["upi://", "twint:", "swish:", "000201", "br.gov.bcb.pix", "EPC069"] {
            assert!(!html.contains(payload), "{method}: a real payment payload {payload}");
        }
        let phone = phone_path(&html);
        let (status, _) = submit(&mut c, &phone, &[("action", "authorize")], T);
        assert_eq!(status, 200, "{method}");
        let events = types(&stripe_events(&c.tick(T)));
        assert!(has(&events, "payment_intent.succeeded"), "{method}: {events:?}");
    }
}

#[test]
fn a_pix_code_is_paid_or_expires_on_the_timeline_its_documented_email_names() {
    let mut c = core();
    let s = local(&mut c, "pix", "brl", &[]);
    let (status, html) = submit(&mut c, &page_path(&s), &[("email", "succeed_immediately@example.com")], T);
    assert_eq!(status, 200, "the code is shown: {html}");
    assert!(html.contains("/stripe/phone/"), "{html}");
    let first = types(&stripe_events(&c.tick(T)));
    assert!(has(&first, "payment_intent.requires_action") && has(&first, "checkout.session.completed"), "{first:?}");
    let paid = types(&stripe_events(&c.tick(T + 30)));
    assert!(has(&paid, "payment_intent.succeeded") && has(&paid, "checkout.session.async_payment_succeeded"), "{paid:?}");
    let s = local(&mut c, "pix", "brl", &[]);
    submit(&mut c, &page_path(&s), &[("email", "fill_never@example.com")], T + 40);
    c.tick(T + 40);
    let never = types(&stripe_events(&c.tick(T + 400)));
    assert!(has(&never, "payment_intent.payment_failed") && has(&never, "checkout.session.async_payment_failed"), "{never:?}");
}

#[test]
fn a_multibanco_voucher_shows_a_test_reference_and_is_paid_by_the_timeline() {
    let mut c = core();
    let s = local(&mut c, "multibanco", "eur", &[]);
    let (status, html) = submit(&mut c, &page_path(&s), &[("email", "succeed_immediately@example.com")], T);
    assert_eq!(status, 200);
    assert!(html.contains("[TEST]") && html.contains("Reference"), "{html}");
    let events = types(&stripe_events(&c.tick(T + 30)));
    assert!(has(&events, "payment_intent.requires_action") && has(&events, "checkout.session.async_payment_succeeded"), "{events:?}");
    let s = local(&mut c, "multibanco", "eur", &[]);
    submit(&mut c, &page_path(&s), &[("email", "expire_immediately@example.com")], T + 40);
    let expired = types(&stripe_events(&c.tick(T + 70)));
    assert!(has(&expired, "payment_intent.payment_failed"), "{expired:?}");
}

#[test]
fn an_mb_way_test_number_decides_what_the_app_does() {
    let mut c = core();
    let s = local(&mut c, "mb_way", "eur", &[]);
    let (status, html) = submit(&mut c, &page_path(&s), &[("phone", "+351911111112")], T);
    assert!(status == 200 || status == 303, "{html}");
    assert!(!types(&stripe_events(&c.tick(T))).contains(&"payment_intent.succeeded".to_string()), "about 30 seconds later");
    let later = types(&stripe_events(&c.tick(T + 60)));
    assert!(has(&later, "payment_intent.succeeded"), "{later:?}");
    let s = local(&mut c, "mb_way", "eur", &[]);
    submit(&mut c, &page_path(&s), &[("phone", "+351911111116")], T + 70);
    let events = stripe_events(&c.tick(T + 70));
    let failed = events.iter().find(|e| e["type"] == "payment_intent.payment_failed").expect("declined in the app");
    assert_eq!(failed["data"]["object"]["last_payment_error"]["code"], "payment_method_customer_decline");
}

#[test]
fn a_method_without_documented_test_data_is_refused_when_the_session_is_made() {
    let mut c = core();
    let (status, v) = local_session(&mut c, "customer_balance", "eur", "payment", &[]);
    assert_eq!(status, 400, "{v}");
    assert!(v["error"]["message"].as_str().unwrap_or("").contains("documented test data"), "{v}");
    let (status, v) = local_session(&mut c, "telepathy", "eur", "payment", &[]);
    assert_eq!(status, 400, "{v}");
}

#[test]
fn a_method_paid_in_another_currency_needs_adaptive_pricing() {
    let mut c = core();
    let (status, v) = local_session(&mut c, "blik", "eur", "payment", &[]);
    assert_eq!(status, 400, "BLIK is paid in zloty: {v}");
    let s = local(&mut c, "blik", "eur", &[("adaptive_pricing[enabled]", "true")]);
    assert_eq!(s["presentment_details"]["presentment_currency"], "pln", "{s}");
    assert!(s["presentment_details"]["presentment_amount"].as_i64().unwrap_or(0) > 5999, "{s}");
    let html = get_page(&mut c, &page_path(&s));
    assert!(html.contains("PLN") && html.contains("fake rate"), "the converted price is marked as a test rate: {html}");
}

#[test]
fn a_local_method_is_refused_where_stripe_refuses_it() {
    let mut c = core();
    let (status, v) = local_session(&mut c, "wero", "eur", "subscription", &[]);
    assert_eq!(status, 400, "Wero has no recurring payments: {v}");
    let (status, _) = local_session(&mut c, "twint", "chf", "subscription", &[]);
    assert_eq!(status, 200, "TWINT recurs");
    let (status, v) = local_session(&mut c, "wechat_pay", "eur", "payment", &[]);
    assert_eq!(status, 400, "WeChat Pay needs its client: {v}");
}

/// Pays a session of `route` with its documented success, the way its flow is paid (the test
/// bank's Authorize, the code and the phone's approval, the phone number, the documented email);
/// the types of the signed events it brought.
fn pay_with_its_documented_success(c: &mut Core, route: &azul_pay_methods::Route, flow: azul_pay_methods::Flow) -> Vec<String> {
    use azul_pay_methods::Flow;
    let currency = if route.takes_currency("eur") { "eur".to_string() } else { route.currencies[0].to_ascii_lowercase() };
    let extra: &[(&str, &str)] = if route.provider_type == "wechat_pay" { &[("payment_method_options[wechat_pay][client]", "web")] } else { &[] };
    let s = local(c, &route.provider_type, &currency, extra);
    let path = page_path(&s);
    let success = route
        .test_inputs
        .iter()
        .filter(|t| t.outcome == azul_pay_methods::Outcome::Succeeded)
        .min_by_key(|t| t.after_minutes)
        .expect("a documented success");
    let (field, value) = (success.field.as_str(), success.value.as_str());
    match (field, flow) {
        ("action", Flow::Redirect) => {
            let (status, _) = submit(c, &path, &[("bank", "a"), ("action", value)], T);
            assert_eq!(status, 303, "{}", route.method);
        }
        ("action", _) => {
            let html = get_page(c, &path);
            let (status, _) = submit(c, &phone_path(&html), &[("action", value)], T);
            assert_eq!(status, 200, "{}", route.method);
        }
        ("code", _) => {
            let (status, phone) = submit(c, &path, &[("code", value)], T);
            assert_eq!(status, 303, "{}", route.method);
            let phone = phone.strip_prefix(BASE).unwrap_or(&phone).to_string();
            submit(c, &phone, &[("action", "authorize")], T);
        }
        (f, _) => {
            let (status, page) = submit(c, &path, &[(f, value)], T);
            assert!(status == 200 || status == 303, "{}: {page}", route.method);
        }
    }
    types(&stripe_events(&c.tick(T + 600)))
}

#[test]
fn every_local_method_the_registry_documents_is_paid_by_its_documented_success() {
    let registry = azul_pay_methods::Registry::builtin().expect("the registry");
    let mut c = core();
    for route in registry.routes.iter().filter(|r| r.provider == "stripe" && r.faked()) {
        if ["card", "sepa_debit", "paypal"].contains(&route.provider_type.as_str()) {
            continue;
        }
        let flow = registry.method(&route.method).expect("its method").flow;
        let events = pay_with_its_documented_success(&mut c, route, flow);
        assert!(events.contains(&"payment_intent.succeeded".to_string()), "{} ({flow:?}): {events:?}", route.method);
    }
}

#[test]
fn the_gocardless_flow_takes_each_scheme_s_documented_test_account() {
    let registry = azul_pay_methods::Registry::builtin().expect("the registry");
    let mut c = core();
    let mut seen = 0;
    for route in registry.routes.iter().filter(|r| r.provider == "gocardless" && r.faked() && r.provider_type != "sepa_core") {
        let currency = route.currencies[0].clone();
        let flow = gocardless_scheme_flow(&mut c, &route.provider_type, &currency).unwrap_or_else(|e| panic!("{}: {e:?}", route.provider_type));
        let page = String::from_utf8_lossy(&call(&mut c, "GET", &flow, &[], b"", T).body).into_owned();
        let input = &route.test_inputs[0];
        assert!(page.contains(&format!("name=\"{}\"", input.field)), "{}: {page}", route.provider_type);
        assert!(!page.contains("name=\"iban\""), "{}: no IBAN outside SEPA", route.provider_type);
        let (status, refused) = submit(&mut c, &flow, &[(input.field.as_str(), "000000 00000000")], T);
        assert!(status == 200 && refused.contains("only accepts the documented test data"), "{}: {refused}", route.provider_type);
        let (status, mandate) = submit(&mut c, &flow, &[(input.field.as_str(), input.value.as_str())], T);
        assert_eq!(status, 200, "{}: {mandate}", route.provider_type);
        assert!(mandate.contains("mandate"), "{}: {mandate}", route.provider_type);
        submit(&mut c, &format!("{flow}/mandate"), &[], T);
        let (status, back) = submit(&mut c, &format!("{flow}/bank"), &[("decision", "confirm")], T);
        assert_eq!((status, back.as_str()), (303, "http://127.0.0.1:8081/return/ok"), "{}", route.provider_type);
        let events = gocardless_events(&c.tick(T));
        let fulfilled = events.iter().find(|e| e["resource_type"] == "billing_requests" && e["action"] == "fulfilled").expect("fulfilled");
        assert_eq!(fulfilled["details"]["scheme"], route.provider_type.as_str(), "{fulfilled}");
        seen += 1;
    }
    assert!(seen >= 6, "Bacs, Autogiro, Betalingsservice, BECS, BECS NZ, PAD, ACH: {seen}");
}

#[test]
fn a_scheme_in_another_currency_or_without_test_data_is_refused() {
    let mut c = core();
    let (status, body) = gocardless_scheme_flow(&mut c, "bacs", "EUR").expect_err("Bacs is in pounds");
    assert_eq!(status, 422, "{body}");
    let (status, body) = gocardless_scheme_flow(&mut c, "faster_payments", "GBP").expect_err("no mandate scheme");
    assert_eq!(status, 422, "{body}");
}

// ---- the euros a payment in another currency became ----

#[test]
fn a_stripe_payment_in_another_currency_reads_back_with_its_euros_at_the_test_rate() {
    let mut c = core();
    let s = local(&mut c, "card", "pln", &[]);
    let card = [("card_number", "4242424242424242"), ("exp", "12/34"), ("cvc", "123"), ("country", "PL")];
    submit(&mut c, &page_path(&s), &card, T);
    c.tick(T);
    let id = s["id"].as_str().unwrap();
    let (_, read) = stripe_api(&mut c, "GET", &format!("/v1/checkout/sessions/{id}"), &[], None, T);
    let pi = read["payment_intent"].as_str().expect("its PaymentIntent").to_string();
    let (_, plain) = stripe_api(&mut c, "GET", &format!("/v1/payment_intents/{pi}"), &[], None, T);
    assert!(plain["latest_charge"].is_string(), "the charge is an id unless expanded: {plain}");
    let expand = "expand%5B%5D=latest_charge.balance_transaction";
    let (status, expanded) = stripe_api(&mut c, "GET", &format!("/v1/payment_intents/{pi}?{expand}"), &[], None, T);
    assert_eq!(status, 200, "{expanded}");
    let bt = &expanded["latest_charge"]["balance_transaction"];
    assert_eq!(bt["currency"], "eur", "{expanded}");
    assert_eq!(bt["amount"].as_i64(), crate::local::convert(5999, "PLN", "EUR"), "59.99 zloty at the test rate");
    let form = [("payment_intent", pi.as_str()), ("amount", "1000"), ("metadata[provider_ref]", REF)];
    let (_, refund) = stripe_api(&mut c, "POST", "/v1/refunds", &form, Some("refund-pln"), T);
    let rid = refund["id"].as_str().expect("a refund").to_string();
    let (_, back) = stripe_api(&mut c, "GET", &format!("/v1/refunds/{rid}?expand%5B%5D=balance_transaction"), &[], None, T);
    assert_eq!(back["balance_transaction"]["currency"], "eur", "{back}");
    assert_eq!(back["balance_transaction"]["amount"].as_i64(), crate::local::convert(1000, "PLN", "EUR").map(|e| -e));
}

#[test]
fn a_gocardless_payment_in_pounds_has_its_euros_once_paid_out_and_an_estimate_before() {
    let registry = azul_pay_methods::Registry::builtin().expect("the registry");
    let bacs = registry.routes.iter().find(|r| r.provider == "gocardless" && r.provider_type == "bacs").expect("Bacs");
    let input = &bacs.test_inputs[0];
    let mut c = core();
    let flow = gocardless_scheme_flow(&mut c, "bacs", "GBP").expect("a Bacs request");
    submit(&mut c, &flow, &[(input.field.as_str(), input.value.as_str())], T);
    submit(&mut c, &format!("{flow}/mandate"), &[], T);
    submit(&mut c, &format!("{flow}/bank"), &[("decision", "confirm")], T);
    let events = gocardless_events(&c.tick(T));
    let created = events.iter().find(|e| e["resource_type"] == "payments" && e["action"] == "created").expect("a payment");
    let pm = created["links"]["payment"].as_str().expect("its id").to_string();
    let token = format!("Bearer {}", Config::default().gocardless.secret_key);
    let headers = [("Authorization", token.as_str()), ("GoCardless-Version", "2015-07-06")];
    let read = |c: &mut Core, now: i64| -> Value {
        let r = call(c, "GET", &format!("/gocardless/payments/{pm}"), &headers, b"", now);
        serde_json::from_slice::<Value>(&r.body).unwrap()["payments"].clone()
    };
    let before = read(&mut c, T);
    assert_eq!(before["currency"], "GBP");
    assert_eq!(before["fx"]["fx_currency"], "EUR", "{before}");
    assert!(before["fx"]["estimated_exchange_rate"].is_string(), "an estimate: {before}");
    assert!(before["fx"]["fx_amount"].is_null(), "no euros before the payout: {before}");
    c.tick(T + 30);
    let after = read(&mut c, T + 30);
    assert_eq!(after["status"], "paid_out", "{after}");
    assert_eq!(after["fx"]["fx_amount"].as_i64(), crate::local::convert(5999, "GBP", "EUR"), "{after}");
    assert!(after["fx"]["exchange_rate"].is_string());
}

#[test]
fn a_euro_payment_has_no_exchange() {
    let mut c = core();
    let flow = gocardless_flow(&mut c);
    submit(&mut c, &flow, &[("iban", "AT61 1904 3002 3457 3201"), ("holder", "Erika Muster")], T);
    submit(&mut c, &format!("{flow}/mandate"), &[("iban", "AT611904300234573201"), ("holder", "Erika Muster")], T);
    submit(&mut c, &format!("{flow}/bank"), &[("iban", "AT611904300234573201"), ("decision", "confirm")], T);
    let events = gocardless_events(&c.tick(T));
    let created = events.iter().find(|e| e["resource_type"] == "payments" && e["action"] == "created").expect("a payment");
    let pm = created["links"]["payment"].as_str().expect("its id").to_string();
    let token = format!("Bearer {}", Config::default().gocardless.secret_key);
    let headers = [("Authorization", token.as_str()), ("GoCardless-Version", "2015-07-06")];
    let r = call(&mut c, "GET", &format!("/gocardless/payments/{pm}"), &headers, b"", T);
    assert_eq!(r.status, 200);
    let p: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(p["payments"]["currency"], "EUR", "{p}");
    assert!(p["payments"]["fx"]["fx_currency"].is_null(), "{p}");
}

// ---- the dLocal-shaped and EBANX-shaped hosted pages: the payer's identity once, kept nowhere ----

const PAYER_NAME: &str = "Ana Testpagadora";
const PAYER_EMAIL: &str = "ana.d37@example.test";
const PAYER_DOCUMENT: &str = "53033315550";

/// A call to the dLocal-shaped API signed as dLocal wants it: X-Login, X-Trans-Key, X-Date,
/// X-Version and the V2-HMAC-SHA256 signature of login, date and body.
fn dlocal_api(c: &mut Core, method: &str, path: &str, body: &str, now: i64) -> (u16, Value) {
    let k = Config::default().dlocal;
    let date = crate::util::rfc3339(now);
    let sig = dlocal_signature(&k.secret_key, &k.login, &date, body.as_bytes());
    let auth = format!("V2-HMAC-SHA256, Signature: {sig}");
    let extra = [("X-Login", k.login.as_str()), ("X-Trans-Key", k.trans_key.as_str()), ("X-Date", date.as_str()),
                 ("X-Version", "2.1"), ("Authorization", auth.as_str()), ("Content-Type", "application/json")];
    let r = call(c, method, &format!("/dlocal{path}"), &extra, body.as_bytes(), now);
    (r.status, serde_json::from_slice(&r.body).unwrap_or(Value::Null))
}

fn dlocal_payment(c: &mut Core, method_id: &str, payer: Value) -> (u16, Value) {
    let body = json!({"amount": 342.90, "currency": "BRL", "country": "BR", "payment_method_id": method_id,
                      "payment_method_flow": "REDIRECT", "payer": payer, "order_id": REF,
                      "description": "Storage 1TB, 12 months", "callback_url": "http://127.0.0.1:8081/return/ok"});
    dlocal_api(c, "POST", "/payments", &body.to_string(), T)
}

fn payer() -> Value {
    json!({"name": PAYER_NAME, "email": PAYER_EMAIL, "document": PAYER_DOCUMENT})
}

/// The dLocal notifications of `deliveries`, each signature checked as dLocal signs them.
fn dlocal_events(deliveries: &[Delivery]) -> Vec<Value> {
    let k = Config::default().dlocal;
    deliveries
        .iter()
        .filter(|d| d.provider == "fake-dlocal")
        .map(|d| {
            assert_eq!(d.url, "http://127.0.0.1:8081/v1/webhook/fake-dlocal");
            let header = |name: &str| d.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()).expect(name);
            let date = header("X-Date");
            let want = format!("V2-HMAC-SHA256, Signature: {}", dlocal_signature(&k.secret_key, &k.login, &date, &d.body));
            assert_eq!(header("Authorization"), want, "signed as dLocal signs");
            serde_json::from_slice(&d.body).expect("a payment object")
        })
        .collect()
}

/// The path of a page an answer points the payer to.
fn path_of(url: &str) -> String {
    url.strip_prefix(BASE).expect("on the sandbox").to_string()
}

/// The fake phone's path a hosted page links to, for any of the shapes.
fn phone_of(html: &str, prefix: &str) -> String {
    let needle = format!("{prefix}/phone/");
    let at = html.find(&needle).expect("a link to the fake phone");
    let rest = &html[at..];
    let end = rest.find(['"', '<', ' ']).unwrap_or(rest.len());
    rest[..end].replace("&amp;", "&")
}

#[test]
fn the_dlocal_shape_wants_its_signed_headers_and_the_payer_and_keeps_nothing_of_the_payer() {
    let mut c = core();
    // unsigned, or signed with another key: refused with dLocal's code
    let r = call(&mut c, "POST", "/dlocal/payments", &[("Content-Type", "application/json")], b"{}", T);
    assert_eq!(r.status, 401);
    let (status, err) = dlocal_payment(&mut c, "PQ", Value::Null);
    assert_eq!(status, 400, "the payer is required: {err}");
    assert!(err["code"].is_number());
    let (status, err) = dlocal_payment(&mut c, "SE", payer());
    assert_eq!(status, 400, "a method without documented test data is refused: {err}");
    let (status, p) = dlocal_payment(&mut c, "PQ", payer());
    assert_eq!(status, 200, "{p}");
    assert_eq!((p["status"].as_str(), p["currency"].as_str()), (Some("PENDING"), Some("BRL")));
    let url = p["redirect_url"].as_str().expect("its checkout");
    assert!(url.starts_with(&format!("{BASE}/dlocal/pay/")), "{url}");
    let html = get_page(&mut c, &path_of(url));
    assert!(html.contains("[TEST]") && html.contains("TEST - NOT A REAL PAYMENT PAGE") && html.contains("BRL 342.90"), "{html}");
    assert!(!html.contains("https://") && !html.contains("<img"), "nothing from elsewhere");
    let phone = phone_of(&html, "/dlocal");
    let (status, _) = submit(&mut c, &phone, &[("action", "authorize")], T);
    assert_eq!(status, 200);
    let deliveries = c.tick(T);
    let events = dlocal_events(&deliveries);
    let paid = events.iter().find(|e| e["status"] == "PAID").expect("paid");
    assert_eq!((paid["order_id"].as_str(), paid["id"].as_str()), (Some(REF), p["id"].as_str()));
    let all = everything(&c, &deliveries);
    for secret in [PAYER_NAME, PAYER_EMAIL, PAYER_DOCUMENT] {
        assert!(!all.contains(secret), "{secret} was kept or sent");
    }
    for page in [html, get_page(&mut c, &phone)] {
        for secret in [PAYER_NAME, PAYER_EMAIL, PAYER_DOCUMENT] {
            assert!(!page.contains(secret), "{secret} was shown back");
        }
    }
}

#[test]
fn a_dlocal_payment_is_read_refunded_and_rejected_as_dlocal_does_it() {
    let mut c = core();
    let (_, p) = dlocal_payment(&mut c, "PQ", payer());
    let id = p["id"].as_str().expect("an id").to_string();
    let html = get_page(&mut c, &path_of(p["redirect_url"].as_str().unwrap()));
    submit(&mut c, &phone_of(&html, "/dlocal"), &[("action", "authorize")], T);
    c.tick(T);
    let (status, read) = dlocal_api(&mut c, "GET", &format!("/payments/{id}"), "", T);
    assert_eq!((status, read["status"].as_str()), (200, Some("PAID")), "{read}");
    let body = json!({"payment_id": id, "amount": 100.00, "currency": "BRL"}).to_string();
    let (status, refund) = dlocal_api(&mut c, "POST", "/refunds", &body, T);
    assert_eq!(status, 200, "{refund}");
    assert!(refund["id"].as_str().is_some_and(|r| r.starts_with("REF")), "{refund}");
    assert_eq!(refund["status"], "PENDING");
    let later = dlocal_events(&c.tick(T + 60));
    let done = later.iter().find(|e| e["payment_id"] == id.as_str()).expect("the refund's notification");
    assert_eq!((done["status"].as_str(), done["amount"].as_f64()), (Some("SUCCESS"), Some(100.0)));
    // a second payment declined on the phone
    let (_, p) = dlocal_payment(&mut c, "BL", payer());
    let html = get_page(&mut c, &path_of(p["redirect_url"].as_str().unwrap()));
    submit(&mut c, &phone_of(&html, "/dlocal"), &[("action", "fail")], T);
    let events = dlocal_events(&c.tick(T));
    assert!(events.iter().any(|e| e["status"] == "REJECTED" && e["id"] == p["id"]), "{events:?}");
}

/// A call to the EBANX-shaped API with its integration key (or `key`).
fn ebanx_api(c: &mut Core, op: &str, mut body: Value, key: Option<&str>) -> Value {
    let k = Config::default().ebanx;
    body["integration_key"] = json!(key.unwrap_or(&k.secret_key));
    let r = call(c, "POST", &format!("/ebanx/ws/{op}"), &[("Content-Type", "application/json")], body.to_string().as_bytes(), T);
    assert_eq!(r.status, 200, "EBANX answers its errors with 200 too");
    serde_json::from_slice(&r.body).expect("JSON")
}

fn ebanx_page(c: &mut Core, code: &str, name: Option<&str>) -> Value {
    let mut body = json!({"email": PAYER_EMAIL, "country": "br", "payment_type_code": code,
                          "merchant_payment_code": REF, "currency_code": "BRL", "amount": 342.90,
                          "redirect_url": "http://127.0.0.1:8081/return/ok"});
    if let Some(n) = name {
        body["name"] = json!(n);
    }
    ebanx_api(c, "request", body, None)
}

#[test]
fn the_ebanx_shape_wants_its_key_and_the_payer_s_name_and_email_and_keeps_neither() {
    let mut c = core();
    let wrong = ebanx_api(&mut c, "request", json!({"name": PAYER_NAME, "email": PAYER_EMAIL}), Some("not-the-key"));
    assert_eq!(wrong["status"], "ERROR");
    assert!(wrong["status_code"].is_string());
    let nameless = ebanx_page(&mut c, "pix", None);
    assert_eq!(nameless["status"], "ERROR", "the name is required: {nameless}");
    let unknown = ebanx_page(&mut c, "picpay", Some(PAYER_NAME));
    assert_eq!(unknown["status"], "ERROR", "a type without documented test data: {unknown}");
    let page = ebanx_page(&mut c, "pix", Some(PAYER_NAME));
    assert_eq!(page["status"], "SUCCESS", "{page}");
    let hash = page["payment"]["hash"].as_str().expect("a hash").to_string();
    assert_eq!(page["redirect_url"], format!("{BASE}/ebanx/checkout?hash={hash}"));
    let html = get_page(&mut c, &path_of(page["redirect_url"].as_str().unwrap()));
    assert!(html.contains("[TEST]") && html.contains("TEST - local fake") && html.contains("BRL 342.90"), "{html}");
    let phone = phone_of(&html, "/ebanx");
    let (status, _) = submit(&mut c, &phone, &[("action", "authorize")], T);
    assert_eq!(status, 200);
    let deliveries = c.tick(T);
    let hook = deliveries.iter().find(|d| d.provider == "fake-ebanx").expect("a notification");
    let token = Config::default().ebanx.webhook_secret;
    assert_eq!(hook.url, format!("http://127.0.0.1:8081/v1/webhook/fake-ebanx?token={token}"), "the URL set in the Merchant Area");
    assert!(hook.headers.iter().any(|(k, v)| k == "Content-Type" && v == "application/x-www-form-urlencoded"));
    assert_eq!(String::from_utf8_lossy(&hook.body), format!("operation=payment_status_change&notification_type=update&hash_codes={hash}"));
    let q = ebanx_api(&mut c, "query", json!({"hash": hash}), None);
    assert_eq!((q["status"].as_str(), q["payment"]["status"].as_str()), (Some("SUCCESS"), Some("CO")), "{q}");
    assert_eq!(q["payment"]["merchant_payment_code"], REF);
    let all = format!("{}{q}{html}{}", everything(&c, &deliveries), get_page(&mut c, &phone));
    for secret in [PAYER_NAME, PAYER_EMAIL] {
        assert!(!all.contains(secret), "{secret} was kept, sent or shown");
    }
}

#[test]
fn an_ebanx_payment_is_refunded_or_cancelled_as_ebanx_does_it() {
    let mut c = core();
    let page = ebanx_page(&mut c, "pix", Some(PAYER_NAME));
    let hash = page["payment"]["hash"].as_str().unwrap().to_string();
    let html = get_page(&mut c, &path_of(page["redirect_url"].as_str().unwrap()));
    submit(&mut c, &phone_of(&html, "/ebanx"), &[("action", "authorize")], T);
    c.tick(T);
    let refund = ebanx_api(&mut c, "refund", json!({"operation": "request", "hash": hash, "amount": "100.00", "description": "test"}), None);
    assert_eq!(refund["status"], "SUCCESS", "{refund}");
    assert_eq!(refund["payment"]["refunds"][0]["status"], "RE");
    let later = c.tick(T + 60);
    let hook = later.iter().find(|d| d.provider == "fake-ebanx").expect("the refund's notification");
    assert!(String::from_utf8_lossy(&hook.body).contains("notification_type=refund"));
    let q = ebanx_api(&mut c, "query", json!({"hash": hash}), None);
    assert_eq!(q["payment"]["refunds"][0]["status"], "CO", "{q}");
    // an open page is cancelled; a confirmed payment is not
    let open = ebanx_page(&mut c, "boleto", Some(PAYER_NAME));
    let open_hash = open["payment"]["hash"].as_str().unwrap().to_string();
    let cancelled = ebanx_api(&mut c, "cancel", json!({"payment_hash": open_hash}), None);
    assert_eq!(cancelled["payment"]["status"], "CA", "{cancelled}");
    let refused = ebanx_api(&mut c, "cancel", json!({"payment_hash": hash}), None);
    assert_eq!(refused["status"], "ERROR");
}

#[test]
fn the_new_shapes_tell_a_backend_their_test_keys() {
    let mut c = core();
    let r = call(&mut c, "GET", "/_dev/keys", &[], b"", T);
    let keys: Value = serde_json::from_slice(&r.body).unwrap();
    let d = &keys["providers"]["fake-dlocal"];
    assert_eq!(d["api"], format!("{BASE}/dlocal"));
    assert_eq!(d["login"], Config::default().dlocal.login.as_str());
    assert_eq!(d["trans_key"], Config::default().dlocal.trans_key.as_str());
    let e = &keys["providers"]["fake-ebanx"];
    assert_eq!(e["api"], format!("{BASE}/ebanx"));
    assert_eq!(e["webhook_secret"], Config::default().ebanx.webhook_secret.as_str());
    assert_eq!((d["methods"].clone(), e["methods"].clone()), (json!(["pix", "boleto", "oxxo"]), json!(["pix", "boleto", "oxxo"])));
    assert_eq!(Kind::parse("ebanx"), Some(Kind::Ebanx));
    assert_eq!(Kind::parse("fake-dlocal"), Some(Kind::DLocal));
}

#[test]
fn the_hang_switch_leaves_the_next_api_call_unanswered_once() {
    let mut c = core();
    let r = call(&mut c, "POST", "/_dev/hang", &[], br#"{"prefix": "/stripe/v1/", "secs": 7}"#, T);
    assert_eq!(r.status, 200);
    let page = call(&mut c, "GET", "/health", &[], b"", T);
    assert_eq!(page.hang_secs, 0, "another path answers");
    let hung = call(&mut c, "POST", "/stripe/v1/checkout/sessions", &[], b"", T);
    assert_eq!(hung.hang_secs, 7, "the next API call hangs");
    let next = call(&mut c, "POST", "/stripe/v1/checkout/sessions", &[], b"", T);
    assert_eq!(next.hang_secs, 0, "once");
}

// ---- the KOMOJU shape ----

fn komoju_api(c: &mut Core, method: &str, path: &str, body: &str, key: Option<&str>, now: i64) -> (u16, Value) {
    let k = Config::default().komoju;
    let auth = format!("Basic {}", crate::util::b64(format!("{}:", k.secret_key).as_bytes()));
    let mut extra = vec![("Authorization", auth.as_str()), ("X-KOMOJU-API-VERSION", "2025-01-28"), ("Content-Type", "application/json")];
    if let Some(k) = key {
        extra.push(("X-KOMOJU-IDEMPOTENCY", k));
    }
    let r = call(c, method, &format!("/komoju/api/v1{path}"), &extra, body.as_bytes(), now);
    (r.status, serde_json::from_slice(&r.body).unwrap_or(Value::Null))
}

fn komoju_session(c: &mut Core, method: &str) -> (u16, Value) {
    let body = json!({"amount": 8900, "currency": "JPY", "payment_types": [method], "external_order_num": REF,
                      "metadata": {"provider_ref": REF}, "return_url": "http://127.0.0.1:8081/return/ok"});
    komoju_api(c, "POST", "/sessions", &body.to_string(), Some(&format!("{REF}/{method}")), T)
}

/// The KOMOJU webhooks of `deliveries`, each signature checked as KOMOJU signs them.
fn komoju_events(deliveries: &[Delivery]) -> Vec<Value> {
    let k = Config::default().komoju;
    deliveries
        .iter()
        .filter(|d| d.provider == "fake-komoju")
        .map(|d| {
            let sig = d.headers.iter().find(|(h, _)| h.eq_ignore_ascii_case("X-Komoju-Signature")).map(|(_, v)| v.clone()).expect("signed");
            assert_eq!(sig, crate::komoju_signature(&k.webhook_secret, &d.body), "the hex HMAC of the body");
            serde_json::from_slice(&d.body).expect("an event")
        })
        .collect()
}

#[test]
fn the_komoju_shape_takes_its_basic_key_and_documented_test_payments_only_and_signs_its_webhooks() {
    let mut c = core();
    let r = call(&mut c, "POST", "/komoju/api/v1/sessions", &[("Content-Type", "application/json")], b"{}", T);
    assert_eq!(r.status, 401, "no key");
    let (status, err) = komoju_session(&mut c, "kakaopay");
    assert_eq!(status, 400, "no documented test-mode behaviour: {err}");
    let (status, s) = komoju_session(&mut c, "konbini");
    assert_eq!(status, 200, "{s}");
    let (again, same) = komoju_session(&mut c, "konbini");
    assert_eq!((again, same["id"].clone()), (200, s["id"].clone()), "the same key is the same session");
    let url = s["session_url"].as_str().expect("its page");
    assert!(url.starts_with(&format!("{BASE}/komoju/sessions/")), "{url}");
    let html = get_page(&mut c, &path_of(url));
    assert!(html.contains("[TEST]") && html.contains("JPY 8900"), "{html}");
    submit(&mut c, &phone_of(&html, "/komoju"), &[("action", "authorize")], T);
    let events = komoju_events(&c.tick(T));
    let captured = events.iter().find(|e| e["type"] == "payment.captured").expect("captured");
    assert_eq!((captured["data"]["external_order_num"].as_str(), captured["data"]["amount"].as_i64()), (Some(REF), Some(8900)));
    // read back, refunded, and the refund's webhook
    let id = s["id"].as_str().unwrap();
    let (status, p) = komoju_api(&mut c, "GET", &format!("/payments/{id}"), "", None, T);
    assert_eq!((status, p["status"].as_str()), (200, Some("captured")), "{p}");
    let (status, r) = komoju_api(&mut c, "POST", &format!("/payments/{id}/refund"), r#"{"amount": 8900}"#, Some("refund-1"), T);
    assert_eq!(status, 200, "{r}");
    let later = komoju_events(&c.tick(T + 60));
    let refunded = later.iter().find(|e| e["type"] == "payment.refund.created").expect("the refund's webhook");
    assert_eq!(refunded["data"]["refunds"][0]["amount"], 8900);
}
