//! The dLocal-shaped test provider: the payments API an app's backend calls for dLocal's hosted
//! checkout (`payment_method_flow: REDIRECT`, every request signed: `X-Login`, `X-Trans-Key`,
//! `X-Date`, `X-Version` and `Authorization: V2-HMAC-SHA256, Signature: ..`), payment reads and
//! cancels, refunds, the payer's checkout page and fake phone, and notifications signed the same
//! way with the secret key.
//!
//! A payment wants the payer's name, email and document, as dLocal's does; they are checked to be
//! there and dropped. Notifications and reads name a fixed test payer instead.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use azul_pay_methods::{Outcome, Registry};

use crate::engine::{bound, due_steps, Cx, FakeProvider, Request, Response};
use crate::hosted::{self, Pay, Refund, Shape, State};
use crate::{util, Config, Error, Kind};

const SHAPE: Shape = Shape {
    provider: "dlocal",
    prefix: "/dlocal",
    name: "dLocal-shaped test provider",
};

/// dLocal's error codes this shape answers with.
const INVALID_CREDENTIALS: i64 = 3001;
const INVALID_PARAM: i64 = 5001;
const NOT_FOUND: i64 = 4000;

fn error(status: u16, code: i64, message: &str) -> Response {
    Response::json(status, json!({ "code": code, "message": message }))
}

/// `2026-10-17T12:00:00.000+0000`, as dLocal writes its dates.
fn date(unix: i64) -> String {
    let t = util::rfc3339(unix);
    match t.strip_suffix('Z') {
        Some(rest) => format!("{rest}+0000"),
        None => t,
    }
}

enum Step {
    /// A refund is done: its notification goes out.
    Refunded { pay: String, refund: String },
}

#[derive(Default)]
pub(crate) struct FakeDLocal {
    registry: Option<Registry>,
    pays: BTreeMap<String, Pay>,
    /// Each refund's payment.
    refunds: BTreeMap<String, String>,
    steps: Vec<(i64, Step)>,
}

impl FakeDLocal {
    pub(crate) fn new() -> Result<FakeDLocal, Error> {
        let registry = Registry::builtin().map_err(|e| Error::Registry(e.to_string()))?;
        Ok(FakeDLocal {
            registry: Some(registry),
            ..FakeDLocal::default()
        })
    }

    fn registry(&self) -> Option<&Registry> {
        self.registry.as_ref()
    }

    /// Whether `req` carries dLocal's headers, signed with this shape's keys over its body.
    fn signed(cfg: &Config, req: &Request<'_>) -> bool {
        let k = &cfg.dlocal;
        let (Some(login), Some(trans), Some(date), Some(auth)) = (
            req.header("x-login"),
            req.header("x-trans-key"),
            req.header("x-date"),
            req.header("authorization"),
        ) else {
            return false;
        };
        let Some(given) = auth.split_once("Signature:").map(|(_, s)| s.trim().to_ascii_lowercase()) else {
            return false;
        };
        login == k.login && trans == k.trans_key && given == util::dlocal_signature(&k.secret_key, login, date, req.body)
    }

    fn payment_json(&self, p: &Pay, base: &str) -> Value {
        let (status, code, detail) = match p.state {
            State::Open => ("PENDING", "100", "The payment is pending."),
            State::Paid => ("PAID", "200", "The payment was paid."),
            State::Rejected => ("REJECTED", "300", "The payment was rejected."),
            State::Cancelled => ("CANCELLED", "400", "The payment was cancelled."),
        };
        let kind = match self.registry().and_then(|r| SHAPE.local(r, &p.provider_type)).map(|l| l.method.flow) {
            Some(azul_pay_methods::Flow::Voucher) => "TICKET",
            _ => "BANK_TRANSFER",
        };
        let mut v = json!({
            "id": p.id, "amount": hosted::amount_number(p.amount, &p.currency), "currency": p.currency,
            "payment_method_id": p.provider_type, "payment_method_type": kind, "payment_method_flow": "REDIRECT",
            "country": p.country, "created_date": date(p.created), "status": status, "status_detail": detail,
            "status_code": code, "order_id": p.order, "payer": hosted::placeholder_payer(),
        });
        if p.state == State::Open {
            v["redirect_url"] = json!(format!("{base}{}/pay/{}", SHAPE.prefix, p.id));
        }
        v
    }

    fn refund_json(p: &Pay, r: &Refund) -> Value {
        let (status, code) = if r.done { ("SUCCESS", 200) } else { ("PENDING", 100) };
        json!({"id": r.id, "payment_id": p.id, "status": status, "status_code": code,
               "amount": hosted::amount_number(r.amount, &p.currency),
               "amount_refunded": hosted::amount_number(r.amount, &p.currency), "currency": p.currency})
    }

    fn create(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let b = req.json();
        let text = |k: &str| b.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
        if text("payment_method_flow") != Some("REDIRECT") {
            return error(400, INVALID_PARAM, "Invalid param: payment_method_flow (this test shape takes REDIRECT)");
        }
        // the payer is required as dLocal requires it; checked to be there, then dropped
        let payer = b.get("payer");
        let has = |k: &str| payer.and_then(|p| p.get(k)).and_then(Value::as_str).is_some_and(|v| !v.trim().is_empty());
        if !(has("name") && has("email") && has("document")) {
            return error(400, INVALID_PARAM, "Invalid param: payer (name, email and document)");
        }
        let Some(method) = text("payment_method_id").map(str::to_string) else {
            return error(400, INVALID_PARAM, "Invalid param: payment_method_id");
        };
        let currency = text("currency").unwrap_or("").to_ascii_uppercase();
        let takes = self
            .registry()
            .and_then(|r| SHAPE.local(r, &method))
            .map(|l| l.route.currencies.contains(&currency));
        match takes {
            None => return error(400, INVALID_PARAM, "Invalid param: payment_method_id (no documented test data for it here)"),
            Some(false) => return error(400, INVALID_PARAM, "Invalid param: currency"),
            Some(true) => {}
        }
        let Some(amount) = hosted::minor_of(b.get("amount"), &currency) else {
            return error(400, INVALID_PARAM, "Invalid param: amount");
        };
        let Some(order) = text("order_id").filter(|o| o.len() <= 200).map(str::to_string) else {
            return error(400, INVALID_PARAM, "Invalid param: order_id");
        };
        let p = Pay {
            id: util::id("D-4-"),
            provider_type: method,
            amount,
            currency,
            country: text("country").unwrap_or("").to_ascii_uppercase(),
            order,
            back: text("callback_url").unwrap_or(cx.base).to_string(),
            description: hosted::description(text("description")),
            state: State::Open,
            created: cx.now,
            refunds: Vec::new(),
        };
        let answer = self.payment_json(&p, cx.base);
        self.pays.insert(p.id.clone(), p);
        bound(&mut self.pays);
        Response::json(200, answer)
    }

    fn cancel(&mut self, cx: &mut Cx<'_>, id: &str) -> Response {
        let Some(p) = self.pays.get_mut(id) else {
            return error(404, NOT_FOUND, "Payment not found");
        };
        if p.state != State::Open {
            return error(400, INVALID_PARAM, "The payment cannot be cancelled");
        }
        p.state = State::Cancelled;
        let p = p.clone();
        let v = self.payment_json(&p, cx.base);
        cx.emit(v.clone());
        Response::json(200, v)
    }

    fn refund(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let b = req.json();
        let Some(id) = b.get("payment_id").and_then(Value::as_str).map(str::to_string) else {
            return error(400, INVALID_PARAM, "Invalid param: payment_id");
        };
        let Some(p) = self.pays.get_mut(&id) else {
            return error(404, NOT_FOUND, "Payment not found");
        };
        if p.state != State::Paid {
            return error(400, INVALID_PARAM, "Only a paid payment is refunded");
        }
        let left = p.amount - p.refunded();
        let amount = match b.get("amount") {
            None => left,
            some => match hosted::minor_of(some, &p.currency) {
                Some(a) if a <= left => a,
                _ => return error(400, INVALID_PARAM, "Invalid param: amount"),
            },
        };
        let r = Refund {
            id: util::upper_id("REF"),
            amount,
            done: false,
        };
        p.refunds.push(r.clone());
        let answer = Self::refund_json(p, &r);
        self.refunds.insert(r.id.clone(), id.clone());
        bound(&mut self.refunds);
        self.steps.push((cx.now + cx.cfg.step_secs, Step::Refunded { pay: id, refund: r.id }));
        Response::json(200, answer)
    }

    fn read_refund(&self, id: &str) -> Response {
        let found = self.refunds.get(id).and_then(|pay| self.pays.get(pay)).and_then(|p| Some((p, p.refunds.iter().find(|r| r.id == id)?)));
        match found {
            Some((p, r)) => Response::json(200, Self::refund_json(p, r)),
            None => error(404, NOT_FOUND, "Refund not found"),
        }
    }

    fn api(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, segs: &[&str]) -> Response {
        if !Self::signed(cx.cfg, req) {
            return error(401, INVALID_CREDENTIALS, "Invalid credentials");
        }
        match (req.method, segs) {
            ("POST", ["payments"]) => self.create(cx, req),
            ("GET", ["payments", id]) => match self.pays.get(*id) {
                Some(p) => Response::json(200, self.payment_json(p, cx.base)),
                None => error(404, NOT_FOUND, "Payment not found"),
            },
            ("POST", ["payments", id, "cancel"]) => self.cancel(cx, id),
            ("POST", ["refunds"]) => self.refund(cx, req),
            ("GET", ["refunds", id]) => self.read_refund(id),
            _ => Response::not_found(),
        }
    }

    /// An answer on the fake phone: paid or rejected by the documented test input it names.
    fn phone_submit(&mut self, cx: &mut Cx<'_>, id: &str, req: &Request<'_>) -> Response {
        let form = req.form();
        let decided = match (self.registry(), self.pays.get(id)) {
            (Some(r), Some(p)) if p.state == State::Open => SHAPE.decide(r, p, &form),
            _ => None,
        };
        if let Some(t) = decided {
            let state = if t.outcome == Outcome::Succeeded { State::Paid } else { State::Rejected };
            if let Some(p) = self.pays.get_mut(id) {
                p.state = state;
            }
            if let Some(p) = self.pays.get(id) {
                let v = self.payment_json(p, cx.base);
                cx.emit(v);
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
            ("GET", Some("pay")) => SHAPE.page(registry, cx.base, p),
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

impl FakeProvider for FakeDLocal {
    fn kind(&self) -> Kind {
        Kind::DLocal
    }

    fn handle(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let segs: Vec<&str> = req.path.trim_start_matches('/').split('/').collect();
        match segs.first().copied() {
            Some("pay" | "status" | "phone") => self.pages(cx, req, &segs),
            _ => self.api(cx, req, &segs),
        }
    }

    fn steps(&mut self, cx: &mut Cx<'_>) {
        for step in due_steps(&mut self.steps, cx.now) {
            let Step::Refunded { pay, refund } = step;
            let Some(p) = self.pays.get_mut(&pay) else {
                continue;
            };
            let Some(r) = p.refunds.iter_mut().find(|r| r.id == refund) else {
                continue;
            };
            r.done = true;
            let r = r.clone();
            let v = Self::refund_json(p, &r);
            cx.emit(v);
        }
    }

    fn advance(&mut self, _cx: &mut Cx<'_>, _months: i64) -> usize {
        // a hosted checkout charges once: nothing recurs
        0
    }

    fn sign(&self, cfg: &Config, body: &[u8], now: i64) -> (&'static str, String) {
        let k = &cfg.dlocal;
        let sig = util::dlocal_signature(&k.secret_key, &k.login, &util::rfc3339(now), body);
        ("Authorization", format!("V2-HMAC-SHA256, Signature: {sig}"))
    }

    fn signed_headers(&self, cfg: &Config, body: &[u8], now: i64) -> Vec<(String, String)> {
        let (name, value) = self.sign(cfg, body, now);
        vec![("X-Date".into(), util::rfc3339(now)), (name.into(), value)]
    }

    fn keys(&self, cfg: &Config, base: &str) -> Value {
        // a provider by contract offers only the methods named: the ones with documented test data
        let methods: Vec<&str> = self
            .registry()
            .map(|r| r.routes.iter().filter(|x| x.provider == SHAPE.provider && x.faked()).map(|x| x.method.as_str()).collect())
            .unwrap_or_default();
        json!({"api": format!("{base}{}", Kind::DLocal.prefix()), "methods": methods, "secret_key": cfg.dlocal.secret_key,
               "webhook_secret": cfg.dlocal.webhook_secret, "login": cfg.dlocal.login,
               "trans_key": cfg.dlocal.trans_key, "api_version": "2.1"})
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
