//! The EBANX-shaped test provider: the Payment Page request (`/ws/request`), reads
//! (`/ws/query`), cancels (`/ws/cancel`) and refunds (`/ws/refund`), each with the integration key
//! in its body and answered with HTTP 200 and EBANX's own `status`; the payer's page and fake
//! phone; and notifications as EBANX sends them: a form naming the payment's hash only, posted to
//! the notification URL with the merchant's token.
//!
//! A page wants the payer's name and email, as EBANX's does; they are checked to be there and
//! dropped. Reads name a fixed test customer instead. EBANX signs its notifications with RSA;
//! this shape's are unsigned and say so, which is why a backend reads every hash back.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use azul_pay_methods::{Outcome, Registry};

use crate::engine::{bound, due_steps, Cx, FakeProvider, Request, Response};
use crate::hosted::{self, Pay, Refund, Shape, State};
use crate::{util, Config, Error, Kind};

const SHAPE: Shape = Shape {
    provider: "ebanx",
    prefix: "/ebanx",
    name: "EBANX-shaped test provider",
};

/// EBANX's answer to a call it refuses: HTTP 200 with its status and code.
fn refused(code: &str, message: &str) -> Response {
    Response::json(200, json!({"status": "ERROR", "status_code": code, "status_message": message}))
}

fn success(payment: Value) -> Response {
    Response::json(200, json!({"payment": payment, "status": "SUCCESS"}))
}

/// `2026-10-17 12:00:00`, as EBANX writes its dates.
fn date(unix: i64) -> String {
    util::rfc3339(unix).replace('T', " ").chars().take(19).collect()
}

/// The notification EBANX posts: what changed and the hash, as a form.
fn notification(kind: &str, hash: &str) -> Value {
    Value::String(format!("operation=payment_status_change&notification_type={kind}&hash_codes={hash}"))
}

enum Step {
    /// A refund is confirmed: its notification goes out.
    Refunded { hash: String, refund: String },
}

#[derive(Default)]
pub(crate) struct FakeEbanx {
    registry: Option<Registry>,
    pays: BTreeMap<String, Pay>,
    steps: Vec<(i64, Step)>,
    refunds_made: u64,
}

impl FakeEbanx {
    pub(crate) fn new() -> Result<FakeEbanx, Error> {
        let registry = Registry::builtin().map_err(|e| Error::Registry(e.to_string()))?;
        Ok(FakeEbanx {
            registry: Some(registry),
            ..FakeEbanx::default()
        })
    }

    fn registry(&self) -> Option<&Registry> {
        self.registry.as_ref()
    }

    fn payment_json(p: &Pay) -> Value {
        let status = match p.state {
            State::Open => "OP",
            State::Paid => "CO",
            State::Rejected | State::Cancelled => "CA",
        };
        let refunds: Vec<Value> = p
            .refunds
            .iter()
            .map(|r| json!({"id": r.id, "status": if r.done { "CO" } else { "RE" },
                            "amount_ext": hosted::amount_text(r.amount, &p.currency)}))
            .collect();
        let amount = hosted::amount_text(p.amount, &p.currency);
        json!({
            "hash": p.id, "country": p.country.to_ascii_lowercase(), "merchant_payment_code": p.order,
            "order_number": null, "status": status, "open_date": date(p.created),
            "amount_br": amount, "amount_ext": amount, "amount_ext_requested": amount, "currency_rate": "1",
            "currency_ext": p.currency, "payment_type_code": p.provider_type, "refunds": refunds,
            "customer": hosted::placeholder_payer(),
        })
    }

    fn request(&mut self, cx: &mut Cx<'_>, b: &Value) -> Response {
        let text = |k: &str| b.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
        // the payer's name and email are required as EBANX requires them; checked, then dropped
        if text("name").is_none() {
            return refused("BP-R-4", "Parameter name not informed");
        }
        if !text("email").is_some_and(|e| e.contains('@')) {
            return refused("BP-R-5", "Parameter email not informed or invalid");
        }
        let Some(code) = text("payment_type_code").map(str::to_string) else {
            return refused("BP-R-6", "Parameter payment_type_code not informed");
        };
        let currency = text("currency_code").unwrap_or("").to_ascii_uppercase();
        let takes = self
            .registry()
            .and_then(|r| SHAPE.local(r, &code))
            .map(|l| l.route.currencies.contains(&currency));
        match takes {
            None => return refused("BP-R-7", "This test shape has no documented test data for the payment type"),
            Some(false) => return refused("BP-R-8", "Currency not accepted for the payment type"),
            Some(true) => {}
        }
        let Some(amount) = hosted::minor_of(b.get("amount"), &currency) else {
            return refused("BP-R-9", "Parameter amount is invalid");
        };
        let Some(order) = text("merchant_payment_code").filter(|o| o.len() <= 40).map(str::to_string) else {
            return refused("BP-R-10", "Parameter merchant_payment_code not informed");
        };
        let p = Pay {
            id: format!("{}{}", util::id(""), util::id("")),
            provider_type: code,
            amount,
            currency,
            country: text("country").unwrap_or("").to_ascii_uppercase(),
            order,
            back: text("redirect_url").unwrap_or(cx.base).to_string(),
            description: hosted::description(Some("Your purchase")),
            state: State::Open,
            created: cx.now,
            refunds: Vec::new(),
        };
        let url = format!("{}{}/checkout?hash={}", cx.base, SHAPE.prefix, p.id);
        let payment = Self::payment_json(&p);
        self.pays.insert(p.id.clone(), p);
        bound(&mut self.pays);
        Response::json(200, json!({"payment": payment, "redirect_url": url, "status": "SUCCESS"}))
    }

    fn cancel(&mut self, cx: &mut Cx<'_>, b: &Value) -> Response {
        let hash = b.get("payment_hash").and_then(Value::as_str).unwrap_or("");
        let Some(p) = self.pays.get_mut(hash) else {
            return refused("BP-CAN-1", "Payment not found");
        };
        if p.state != State::Open {
            return refused("BP-CAN-2", "Only an open or pending payment can be cancelled");
        }
        p.state = State::Cancelled;
        cx.emit(notification("update", hash));
        success(Self::payment_json(p))
    }

    fn refund(&mut self, cx: &mut Cx<'_>, b: &Value) -> Response {
        if b.get("operation").and_then(Value::as_str) != Some("request") {
            return refused("BP-REF-1", "This test shape takes operation request");
        }
        let hash = b.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
        let Some(p) = self.pays.get_mut(&hash) else {
            return refused("BP-REF-2", "Payment not found");
        };
        if p.state != State::Paid {
            return refused("BP-REF-3", "Only a confirmed payment can be refunded");
        }
        let left = p.amount - p.refunded();
        let amount = match hosted::minor_of(b.get("amount"), &p.currency) {
            Some(a) if a <= left => a,
            _ => return refused("BP-REF-4", "Parameter amount is invalid"),
        };
        self.refunds_made += 1;
        let id = (1000 + self.refunds_made).to_string();
        p.refunds.push(Refund {
            id: id.clone(),
            amount,
            done: false,
        });
        let answer = Self::payment_json(p);
        self.steps.push((cx.now + cx.cfg.step_secs, Step::Refunded { hash, refund: id }));
        success(answer)
    }

    /// Whether `jws` (`BASE64URL(header)..BASE64URL(signature)`) signs `body` RS256 under the
    /// registered key, its header naming that key's id, `b64: false` and `crit: ["b64"]`.
    fn jws_ok(jws: &str, body: &[u8]) -> bool {
        let Some((h, sig)) = jws.trim().split_once("..") else {
            return false;
        };
        let header: Value = util::b64url_decode(h).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
        let shape = header["alg"] == "RS256" && header["kid"] == util::EBANX_JWS_KID && header["b64"] == false && header["crit"] == json!(["b64"]);
        let mut input = format!("{h}.").into_bytes();
        input.extend_from_slice(body);
        shape && util::b64url_decode(sig).is_some_and(|s| util::rsa_sha256_ok(util::EBANX_JWS_PUB, &input, &s))
    }

    fn api(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, op: &str) -> Response {
        if req.method != "POST" {
            return Response::not_found();
        }
        let b = req.json();
        // a merchant authenticates with JWS (RS256 over BASE64URL(header).body, its registered
        // key) or with the integration key EBANX is retiring
        match req.header("x-jws-signature") {
            Some(jws) => {
                if !Self::jws_ok(jws, req.body) {
                    return refused("DA-6", "Invalid JWS signature");
                }
            }
            None if b.get("integration_key").and_then(Value::as_str) != Some(cx.cfg.ebanx.secret_key.as_str()) => {
                return refused("DA-1", "Invalid integration key");
            }
            None => {}
        }
        match op {
            "request" => self.request(cx, &b),
            "query" => {
                let hash = b.get("hash").and_then(Value::as_str).unwrap_or("");
                match self.pays.get(hash) {
                    Some(p) => success(Self::payment_json(p)),
                    None => refused("BP-Q-2", "Payment not found"),
                }
            }
            "cancel" => self.cancel(cx, &b),
            "refund" => self.refund(cx, &b),
            _ => Response::not_found(),
        }
    }

    /// An answer on the fake phone: confirmed or cancelled by the documented test input it names.
    fn phone_submit(&mut self, cx: &mut Cx<'_>, hash: &str, req: &Request<'_>) -> Response {
        let form = req.form();
        let decided = match (self.registry(), self.pays.get(hash)) {
            (Some(r), Some(p)) if p.state == State::Open => SHAPE.decide(r, p, &form),
            _ => None,
        };
        if let Some(t) = decided {
            let state = if t.outcome == Outcome::Succeeded { State::Paid } else { State::Rejected };
            if let Some(p) = self.pays.get_mut(hash) {
                p.state = state;
            }
            cx.emit(notification("update", hash));
        }
        match (self.registry(), self.pays.get(hash)) {
            (Some(r), Some(p)) => SHAPE.phone(r, p),
            _ => SHAPE.closed(404),
        }
    }
}

impl FakeProvider for FakeEbanx {
    fn kind(&self) -> Kind {
        Kind::Ebanx
    }

    fn handle(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let segs: Vec<&str> = req.path.trim_start_matches('/').split('/').collect();
        match (req.method, segs.as_slice()) {
            (_, ["ws", op]) => self.api(cx, req, op),
            ("GET", ["checkout"]) => {
                let hash = req.query_param("hash").unwrap_or_default();
                match (self.registry(), self.pays.get(&hash)) {
                    (Some(r), Some(p)) => SHAPE.page(r, cx.base, p),
                    _ => SHAPE.closed(404),
                }
            }
            ("GET", ["status", hash]) => match self.pays.get(*hash) {
                Some(p) => hosted::status(p),
                None => SHAPE.closed(404),
            },
            ("GET", ["phone", hash]) => match (self.registry(), self.pays.get(*hash)) {
                (Some(r), Some(p)) => SHAPE.phone(r, p),
                _ => SHAPE.closed(404),
            },
            ("POST", ["phone", hash]) => {
                let hash = hash.to_string();
                self.phone_submit(cx, &hash, req)
            }
            _ => Response::not_found(),
        }
    }

    fn steps(&mut self, cx: &mut Cx<'_>) {
        for step in due_steps(&mut self.steps, cx.now) {
            let Step::Refunded { hash, refund } = step;
            let Some(r) = self.pays.get_mut(&hash).and_then(|p| p.refunds.iter_mut().find(|r| r.id == refund)) else {
                continue;
            };
            r.done = true;
            cx.emit(notification("refund", &hash));
        }
    }

    fn advance(&mut self, _cx: &mut Cx<'_>, _months: i64) -> usize {
        // a Payment Page charges once: nothing recurs
        0
    }

    /// EBANX's RSA-SHA1 signature of the notification's exact body.
    fn sign(&self, _cfg: &Config, body: &[u8], _now: i64) -> (&'static str, String) {
        ("X-Signature-Content", util::rsa_sha1_sign(util::EBANX_NOTIFY_KEY, body).unwrap_or_default())
    }

    fn signed_headers(&self, cfg: &Config, body: &[u8], now: i64) -> Vec<(String, String)> {
        let (name, value) = self.sign(cfg, body, now);
        vec![
            ("X-Signature-Type".into(), "rsa,sha1".into()),
            ("X-Signature-Fingerprint".into(), util::EBANX_NOTIFY_FINGERPRINT.into()),
            (name.into(), value),
        ]
    }

    fn content_type(&self) -> &'static str {
        "application/x-www-form-urlencoded"
    }

    fn webhook_query(&self, cfg: &Config) -> Option<String> {
        Some(util::form_encode(&[("token", cfg.ebanx.webhook_secret.as_str())]))
    }

    fn keys(&self, cfg: &Config, base: &str) -> Value {
        // a provider by contract offers only the methods named: the ones with documented test data
        let methods: Vec<&str> = self
            .registry()
            .map(|r| r.routes.iter().filter(|x| x.provider == SHAPE.provider && x.faked()).map(|x| x.method.as_str()).collect())
            .unwrap_or_default();
        // the test keys a backend is configured with: the notifications' public key and our JWS
        // key with its id (a real merchant makes its key pair and registers the public half)
        json!({"api": format!("{base}{}", Kind::Ebanx.prefix()), "methods": methods, "secret_key": cfg.ebanx.secret_key,
               "webhook_secret": cfg.ebanx.webhook_secret,
               "notification_url": "the webhook URL with ?token=<webhook_secret>",
               "notify_key": cfg.ebanx.notify_key, "notify_fingerprint": cfg.ebanx.notify_fingerprint,
               "jws_key": cfg.ebanx.jws_key, "jws_kid": cfg.ebanx.jws_kid})
    }

    fn summary(&self) -> Value {
        let payments: Vec<Value> = self
            .pays
            .values()
            .map(|p| json!({"hash": p.id, "type": p.provider_type, "state": format!("{:?}", p.state), "refunds": p.refunds.len()}))
            .collect();
        json!({ "payments": payments })
    }
}
