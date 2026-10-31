//! The KOMOJU-shaped test provider: the sessions API an app's backend calls for KOMOJU's hosted
//! page (`POST /api/v1/sessions`, Basic authentication with the secret key, `X-KOMOJU-API-VERSION`,
//! POSTs idempotent by `X-KOMOJU-IDEMPOTENCY`), session and payment reads, session cancels,
//! refunds, the payer's page and fake phone, and webhooks signed as KOMOJU signs them
//! (`X-Komoju-Signature`, the hex HMAC-SHA256 of the body under the secret token).
//!
//! It fakes the registry's KOMOJU routes that carry KOMOJU's documented test-mode behaviour (the
//! konbini voucher captured, the smartphone payments' deposit simulation); KOMOJU's page asks the
//! payer for what a method needs, so the request carries no payer at all. Reads and webhooks name
//! a fixed test customer that the backend must drop.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use azul_pay_methods::{Outcome, Registry};

use crate::engine::{bound, due_steps, Cx, FakeProvider, Request, Response};
use crate::hosted::{self, Pay, Refund, Shape, State};
use crate::{util, Config, Error, Kind};

const SHAPE: Shape = Shape {
    provider: "komoju",
    prefix: "/komoju",
    name: "KOMOJU-shaped test provider",
};

/// The API version this shape speaks.
const VERSION: &str = "2025-01-28";

fn error(status: u16, code: &str, message: &str) -> Response {
    Response::json(status, json!({"error": {"code": code, "message": message}}))
}

enum Step {
    /// A refund is done: its webhook goes out.
    Refunded { pay: String },
}

#[derive(Default)]
pub(crate) struct FakeKomoju {
    registry: Option<Registry>,
    /// Session id -> its payment (the payment's id is the session's own here: one each).
    pays: BTreeMap<String, Pay>,
    /// Idempotency key -> (the request body, the answer).
    idem: BTreeMap<String, (Vec<u8>, Response)>,
    steps: Vec<(i64, Step)>,
}

impl FakeKomoju {
    pub(crate) fn new() -> Result<FakeKomoju, Error> {
        let registry = Registry::builtin().map_err(|e| Error::Registry(e.to_string()))?;
        Ok(FakeKomoju {
            registry: Some(registry),
            ..FakeKomoju::default()
        })
    }

    fn registry(&self) -> Option<&Registry> {
        self.registry.as_ref()
    }

    /// Whether `req` carries this shape's secret key as Basic authentication.
    fn authorized(cfg: &Config, req: &Request<'_>) -> bool {
        let want = format!("Basic {}", util::b64(format!("{}:", cfg.komoju.secret_key).as_bytes()));
        req.header("authorization") == Some(want.as_str())
    }

    fn payment_json(p: &Pay) -> Value {
        let status = match p.state {
            State::Open => "pending",
            State::Paid if p.refunded() >= p.amount => "refunded",
            State::Paid => "captured",
            State::Rejected | State::Cancelled => "cancelled",
        };
        let refunds: Vec<Value> = p.refunds.iter().map(|r| json!({"id": r.id, "amount": r.amount, "currency": p.currency})).collect();
        json!({
            "id": p.id, "resource": "payment", "status": status, "amount": p.amount, "currency": p.currency,
            "external_order_num": p.order, "session": p.id, "metadata": {"provider_ref": p.order},
            "amount_refunded": p.refunded(), "refunds": refunds,
            "customer": hosted::placeholder_payer(),
            "payment_details": {"type": p.provider_type, "email": "payer@example.com"},
        })
    }

    fn session_json(p: &Pay, base: &str) -> Value {
        let (status, payment) = match p.state {
            State::Open => ("pending", Value::Null),
            State::Cancelled => ("cancelled", Value::Null),
            _ => ("completed", Self::payment_json(p)),
        };
        json!({"id": p.id, "resource": "session", "status": status, "mode": "payment",
               "amount": p.amount, "currency": p.currency, "external_order_num": p.order,
               "session_url": format!("{base}{}/sessions/{}", SHAPE.prefix, p.id),
               "return_url": p.back, "expires_at": util::rfc3339(p.created + 86_400), "payment": payment})
    }

    /// A webhook of `kind` about `p`.
    fn event(kind: &str, p: &Pay, now: i64) -> Value {
        json!({"id": util::id("evt_"), "type": kind, "resource": "event", "data": Self::payment_json(p),
               "created_at": util::rfc3339(now)})
    }

    fn create(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let b = req.json();
        let text = |k: &str| b.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
        let types: Vec<String> = b
            .get("payment_types")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let [method] = types.as_slice() else {
            return error(400, "bad_request", "payment_types: one payment type (this test shape)");
        };
        let currency = text("currency").unwrap_or("").to_ascii_uppercase();
        let takes = self
            .registry()
            .and_then(|r| SHAPE.local(r, method))
            .map(|l| l.route.currencies.contains(&currency));
        match takes {
            None => return error(400, "bad_request", "payment_types: no documented test-mode behaviour for it here"),
            Some(false) => return error(400, "bad_request", "currency"),
            Some(true) => {}
        }
        let Some(amount) = b.get("amount").and_then(Value::as_i64).filter(|a| *a > 0) else {
            return error(400, "bad_request", "amount: an integer in the currency's smallest unit");
        };
        let Some(order) = text("external_order_num").map(str::to_string) else {
            return error(400, "bad_request", "external_order_num");
        };
        let p = Pay {
            id: util::id(""),
            provider_type: method.clone(),
            amount,
            currency,
            country: "JP".into(),
            order,
            back: text("return_url").unwrap_or(cx.base).to_string(),
            description: hosted::description(None),
            state: State::Open,
            created: cx.now,
            refunds: Vec::new(),
        };
        let answer = Self::session_json(&p, cx.base);
        self.pays.insert(p.id.clone(), p);
        bound(&mut self.pays);
        Response::json(200, answer)
    }

    fn cancel(&mut self, cx: &mut Cx<'_>, id: &str) -> Response {
        let Some(p) = self.pays.get_mut(id) else {
            return error(404, "not_found", "Session not found");
        };
        if p.state != State::Open {
            return error(422, "invalid_state", "The session is completed");
        }
        p.state = State::Cancelled;
        Response::json(200, Self::session_json(p, cx.base))
    }

    fn refund(&mut self, cx: &mut Cx<'_>, id: &str, req: &Request<'_>) -> Response {
        let b = req.json();
        let Some(p) = self.pays.get_mut(id) else {
            return error(404, "not_found", "Payment not found");
        };
        if p.state != State::Paid {
            return error(422, "invalid_state", "Only a captured payment is refunded");
        }
        let left = p.amount - p.refunded();
        let amount = match b.get("amount") {
            None => left,
            Some(v) => match v.as_i64() {
                Some(a) if a > 0 && a <= left => a,
                _ => return error(400, "bad_request", "amount"),
            },
        };
        p.refunds.push(Refund {
            id: util::id("ref_"),
            amount,
            done: false,
        });
        let answer = Self::payment_json(p);
        self.steps.push((cx.now + cx.cfg.step_secs, Step::Refunded { pay: id.to_string() }));
        Response::json(200, answer)
    }

    fn api(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, segs: &[&str]) -> Response {
        if !Self::authorized(cx.cfg, req) {
            return error(401, "unauthorized", "Invalid secret key");
        }
        // a POST with a key used before gets the first answer again (the same body), or a 409
        let key = (req.method == "POST").then(|| req.header("x-komoju-idempotency").map(String::from)).flatten();
        if let Some((body, answer)) = key.as_ref().and_then(|k| self.idem.get(k)) {
            return if body.as_slice() == req.body { answer.clone() } else { error(409, "idempotency_conflict", "another request used this key") };
        }
        let answer = match (req.method, segs) {
            ("POST", ["sessions"]) => self.create(cx, req),
            ("GET", ["sessions", id]) => match self.pays.get(*id) {
                Some(p) => Response::json(200, Self::session_json(p, cx.base)),
                None => error(404, "not_found", "Session not found"),
            },
            ("POST", ["sessions", id, "cancel"]) => self.cancel(cx, id),
            ("GET", ["payments", id]) => match self.pays.get(*id).filter(|p| p.state != State::Open) {
                Some(p) => Response::json(200, Self::payment_json(p)),
                None => error(404, "not_found", "Payment not found"),
            },
            ("POST", ["payments", id, "refund"]) => self.refund(cx, id, req),
            _ => Response::not_found(),
        };
        if let Some(k) = key.filter(|_| answer.status < 400) {
            self.idem.insert(k, (req.body.to_vec(), answer.clone()));
            bound(&mut self.idem);
        }
        answer
    }

    /// An answer on the fake phone: captured or cancelled by the documented test behaviour it names.
    fn phone_submit(&mut self, cx: &mut Cx<'_>, id: &str, req: &Request<'_>) -> Response {
        let form = req.form();
        let decided = match (self.registry(), self.pays.get(id)) {
            (Some(r), Some(p)) if p.state == State::Open => SHAPE.decide(r, p, &form),
            _ => None,
        };
        if let Some(t) = decided {
            let (state, kind) = if t.outcome == Outcome::Succeeded { (State::Paid, "payment.captured") } else { (State::Cancelled, "payment.cancelled") };
            if let Some(p) = self.pays.get_mut(id) {
                p.state = state;
            }
            if let Some(p) = self.pays.get(id) {
                cx.emit(Self::event(kind, p, cx.now));
            }
        }
        match (self.registry(), self.pays.get(id)) {
            (Some(r), Some(p)) => SHAPE.phone(r, p),
            _ => SHAPE.closed(404),
        }
    }

    fn pages(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, segs: &[&str]) -> Response {
        let (Some(registry), Some(id)) = (self.registry(), segs.get(1)) else {
            return Response::not_found();
        };
        let Some(p) = self.pays.get(*id) else {
            return SHAPE.closed(404);
        };
        match (req.method, segs.first().copied()) {
            ("GET", Some("sessions")) => SHAPE.page(registry, cx.base, p),
            ("GET", Some("status")) => hosted::status(p),
            ("GET", Some("phone")) => SHAPE.phone(registry, p),
            ("POST", Some("phone")) => {
                let id = id.to_string();
                self.phone_submit(cx, &id, req)
            }
            _ => Response::not_found(),
        }
    }
}

impl FakeProvider for FakeKomoju {
    fn kind(&self) -> Kind {
        Kind::Komoju
    }

    fn handle(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let segs: Vec<&str> = req.path.trim_start_matches('/').split('/').collect();
        match segs.as_slice() {
            ["api", "v1", rest @ ..] => self.api(cx, req, rest),
            ["sessions" | "status" | "phone", ..] => self.pages(cx, req, &segs),
            _ => Response::not_found(),
        }
    }

    fn steps(&mut self, cx: &mut Cx<'_>) {
        for step in due_steps(&mut self.steps, cx.now) {
            let Step::Refunded { pay } = step;
            let Some(p) = self.pays.get_mut(&pay) else {
                continue;
            };
            for r in p.refunds.iter_mut().filter(|r| !r.done) {
                r.done = true;
            }
            let p = p.clone();
            cx.emit(Self::event("payment.refund.created", &p, cx.now));
        }
    }

    fn advance(&mut self, _cx: &mut Cx<'_>, _months: i64) -> usize {
        // a hosted page charges once: nothing recurs
        0
    }

    fn sign(&self, cfg: &Config, body: &[u8], _now: i64) -> (&'static str, String) {
        ("X-Komoju-Signature", util::hmac_hex(cfg.komoju.webhook_secret.as_bytes(), body))
    }

    fn keys(&self, cfg: &Config, base: &str) -> Value {
        // a merchant takes the methods its application was granted: here the ones with KOMOJU's
        // documented test-mode behaviour
        let methods: Vec<&str> = self
            .registry()
            .map(|r| r.routes.iter().filter(|x| x.provider == SHAPE.provider && x.faked()).map(|x| x.method.as_str()).collect())
            .unwrap_or_default();
        json!({"api": format!("{base}{}/api/v1", Kind::Komoju.prefix()), "methods": methods,
               "secret_key": cfg.komoju.secret_key, "webhook_secret": cfg.komoju.webhook_secret, "api_version": VERSION})
    }

    fn summary(&self) -> Value {
        let payments: Vec<Value> = self
            .pays
            .values()
            .map(|p| json!({"id": p.id, "method": p.provider_type, "state": format!("{:?}", p.state), "refunds": p.refunds.len()}))
            .collect();
        json!({ "payments": payments })
    }
}
