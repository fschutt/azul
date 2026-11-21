//! The GoCardless-shaped test provider: the API an app's backend calls (Billing Requests with a
//! mandate request - `sepa_core`, or another scheme the payment methods registry has GoCardless's
//! documented sandbox account for - and a subscription request, Billing Request Flows, payments,
//! refunds, subscription cancellation, the sandbox's scenario simulators; `GoCardless-Version:
//! 2015-07-06`), the payer's flow pages (the account holder and IBAN, the mandate to confirm, the
//! bank's confirmation), and batched webhooks signed with `Webhook-Signature`.
//!
//! A debit runs the SEPA timeline compressed into steps of `step_secs`: created, submitted,
//! confirmed, paid out, or failed / charged back. The account holder's name is shown back once
//! on the mandate page and never kept; of the IBAN, or another scheme's test account, only its
//! outcome is kept.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use azul_pay_methods::{Outcome, Registry};

use crate::engine::{bound, due_steps, take, Cx, FakeProvider, Request, Response};
use crate::local::{self, Answer};
use crate::testdata::{debit_of, iban_problem, Debit};
use crate::{kit, util, Config, Error, Kind};

const VERSION: &str = "2015-07-06";
const SHAPE: &str = "GoCardless-shaped test provider";
const FLOW_SECS: i64 = 7 * 86_400;
/// The scheme of a SEPA Direct Debit mandate, the one this fake has always taken.
const SEPA: &str = "sepa_core";

#[derive(Debug, Clone)]
struct SubRequest {
    amount: i64,
    currency: String,
    count: Option<i64>,
    start_date: Option<String>,
    name: String,
    metadata_ref: Option<String>,
}

#[derive(Debug, Clone)]
struct BillingRequest {
    id: String,
    status: String,
    /// The mandate request's scheme (`sepa_core`, `bacs`, ...).
    scheme: String,
    currency: String,
    metadata_ref: Option<String>,
    mandate_ref: Option<String>,
    sub: SubRequest,
    mandate: Option<String>,
    subscription: Option<String>,
    created: i64,
}

#[derive(Debug, Clone)]
struct Flow {
    id: String,
    br: String,
    redirect_uri: String,
    exit_uri: String,
    expires_at: i64,
    /// What the payer's test IBAN will do, once step one took it (the IBAN itself is not kept).
    debit: Option<Debit>,
}

#[derive(Debug, Clone)]
struct Subscription {
    id: String,
    /// Its mandate's scheme, which its payments' events name.
    scheme: String,
    mandate: String,
    status: String,
    amount: i64,
    currency: String,
    count: Option<i64>,
    made: i64,
    name: String,
    metadata_ref: Option<String>,
    debit: Debit,
}

#[derive(Debug, Clone)]
struct Payment {
    id: String,
    /// Its mandate's scheme.
    scheme: String,
    subscription: String,
    mandate: String,
    status: String,
    amount: i64,
    currency: String,
    refunded: i64,
    created: i64,
    /// Whether it was paid out: from then on its exchange into euros is known.
    paid_out: bool,
}

#[derive(Debug, Clone)]
struct Step {
    pm: String,
    to: &'static str,
}

#[derive(Default)]
pub(crate) struct FakeGoCardless {
    /// Which schemes besides SEPA it takes, and their documented test accounts.
    registry: Registry,
    requests: BTreeMap<String, BillingRequest>,
    flows: BTreeMap<String, Flow>,
    subscriptions: BTreeMap<String, Subscription>,
    payments: BTreeMap<String, Payment>,
    refunds: BTreeMap<String, Value>,
    /// Idempotency key -> the resource it made: a repeat gets GoCardless's 409 conflict naming it.
    keys: BTreeMap<String, String>,
    steps: Vec<(i64, Step)>,
}

fn gc_error(status: u16, kind: &str, reason: &str, message: &str) -> Response {
    Response::json(
        status,
        json!({"error": {"code": status, "type": kind, "message": message,
                         "errors": [{"reason": reason, "message": message}]}}),
    )
}

/// GoCardless's answer to a repeated idempotency key: 409, naming what the first request made.
fn conflict(made: &str) -> Response {
    let message = "A resource has already been created with this idempotency key";
    Response::json(
        409,
        json!({"error": {"code": 409, "type": "invalid_state", "message": message,
                         "errors": [{"reason": "idempotent_creation_conflict", "message": message,
                                     "links": {"conflicting_resource_id": made}}]}}),
    )
}


fn event(resource: &str, action: &str, links: Value, details: Value, resource_meta: Value, now: i64) -> Value {
    json!({
        "id": util::upper_id("EV"),
        "created_at": util::rfc3339(now),
        "resource_type": resource,
        "action": action,
        "links": links,
        "details": details,
        "metadata": {},
        "resource_metadata": resource_meta,
    })
}

fn details(origin: &str, cause: &str, description: &str, scheme: &str) -> Value {
    json!({"origin": origin, "cause": cause, "description": description, "scheme": scheme})
}

/// The bank's reason code of a returned debit (SEPA's codes, as GoCardless passes them on).
fn reason_code(decline: &str) -> &'static str {
    match decline {
        "insufficient_funds" => "AM04",
        "debit_not_authorized" => "MD01",
        _ => "MS03",
    }
}

impl FakeGoCardless {
    pub(crate) fn new() -> Result<FakeGoCardless, Error> {
        let registry = Registry::builtin().map_err(|e| Error::Registry(e.to_string()))?;
        Ok(FakeGoCardless {
            registry,
            ..FakeGoCardless::default()
        })
    }

    /// The registry's GoCardless route of a scheme other than SEPA, if it has documented test data.
    fn scheme_route(&self, scheme: &str) -> Option<local::Local<'_>> {
        (scheme != SEPA).then(|| local::find(&self.registry, "gocardless", scheme)).flatten()
    }

    fn br_json(b: &BillingRequest) -> Value {
        let mut links = json!({});
        if let Some(m) = &b.mandate {
            links["mandate_request_mandate"] = json!(m);
        }
        if let Some(s) = &b.subscription {
            links["subscription_request_subscription"] = json!(s);
        }
        let mut sub = json!({"amount": b.sub.amount, "currency": b.sub.currency, "interval_unit": "monthly",
                             "interval": 1, "name": b.sub.name, "metadata": util::ref_meta(&b.sub.metadata_ref)});
        if let Some(c) = b.sub.count {
            sub["count"] = json!(c);
        }
        if let Some(d) = &b.sub.start_date {
            sub["start_date"] = json!(d);
        }
        json!({"billing_requests": {
            "id": b.id, "status": b.status, "created_at": util::rfc3339(b.created),
            "mandate_request": {"scheme": b.scheme, "currency": b.currency, "metadata": util::ref_meta(&b.mandate_ref)},
            "subscription_request": sub, "metadata": util::ref_meta(&b.metadata_ref), "links": links,
        }})
    }

    fn payment_json(p: &Payment) -> Value {
        json!({"payments": {"id": p.id, "amount": p.amount, "amount_refunded": p.refunded, "currency": p.currency,
                            "status": p.status, "created_at": util::rfc3339(p.created), "metadata": {},
                            "links": {"mandate": p.mandate, "subscription": p.subscription},
                            "fx": Self::fx_json(p)}})
    }

    /// A payment in another currency than the payout's (euros): an estimated rate until it is
    /// paid out, its euro amount and rate after, at the sandbox's marked test rates.
    fn fx_json(p: &Payment) -> Value {
        if p.currency.eq_ignore_ascii_case("EUR") {
            return json!({"fx_currency": null, "fx_amount": null, "exchange_rate": null, "estimated_exchange_rate": null});
        }
        let rate = local::euros_per_unit(&p.currency);
        if p.paid_out {
            json!({"fx_currency": "EUR", "fx_amount": local::convert(p.amount, &p.currency, "EUR"),
                   "exchange_rate": rate, "estimated_exchange_rate": null})
        } else {
            json!({"fx_currency": "EUR", "fx_amount": null, "exchange_rate": null, "estimated_exchange_rate": rate})
        }
    }

    fn create_billing_request(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let key = req.header("idempotency-key").map(String::from);
        if let Some(made) = key.as_ref().and_then(|k| self.keys.get(k)) {
            return conflict(made);
        }
        let body = req.json();
        let b = &body["billing_requests"];
        let mandate = &b["mandate_request"];
        let scheme = mandate["scheme"].as_str().unwrap_or("").to_string();
        let currency = mandate["currency"].as_str().unwrap_or("EUR").to_ascii_uppercase();
        // the scheme's currency, as GoCardless checks it: SEPA in euros, Bacs in pounds, ...
        let takes = if scheme == SEPA {
            Some(currency == "EUR")
        } else {
            self.scheme_route(&scheme).map(|l| l.route.takes_currency(&currency))
        };
        match takes {
            None => return gc_error(422, "validation_failed", "invalid_scheme", "This sandbox takes sepa_core and the schemes it has documented test accounts for"),
            Some(false) => return gc_error(422, "validation_failed", "invalid_currency", "The currency is not the scheme's"),
            Some(true) => {}
        }
        let s = &b["subscription_request"];
        let Some(amount) = s["amount"].as_i64().filter(|a| *a > 0) else {
            return gc_error(422, "validation_failed", "invalid_amount", "subscription_request.amount is required");
        };
        if s["interval_unit"].as_str() != Some("monthly") {
            return gc_error(422, "validation_failed", "invalid_interval_unit", "This sandbox takes monthly subscriptions");
        }
        let id = util::upper_id("BRQ");
        let r = BillingRequest {
            id: id.clone(),
            status: "pending".into(),
            scheme: scheme.clone(),
            currency: currency.clone(),
            metadata_ref: b["metadata"]["provider_ref"].as_str().map(String::from),
            mandate_ref: mandate["metadata"]["provider_ref"].as_str().map(String::from),
            sub: SubRequest {
                amount,
                currency: s["currency"].as_str().unwrap_or(&currency).to_ascii_uppercase(),
                count: s["count"].as_i64().filter(|c| *c > 0),
                start_date: s["start_date"].as_str().map(String::from),
                name: s["name"].as_str().unwrap_or("Subscription").chars().take(100).collect(),
                metadata_ref: s["metadata"]["provider_ref"].as_str().map(String::from),
            },
            mandate: None,
            subscription: None,
            created: cx.now,
        };
        let j = Self::br_json(&r);
        cx.emit(event("billing_requests", "created", json!({"billing_request": id}), details("api", "billing_request_created", "Billing request created", &scheme), util::ref_meta(&r.metadata_ref), cx.now));
        if let Some(k) = key {
            self.keys.insert(k, id.clone());
            bound(&mut self.keys);
        }
        self.requests.insert(id, r);
        bound(&mut self.requests);
        Response::json(201, j)
    }

    fn create_flow(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let key = req.header("idempotency-key").map(String::from);
        if let Some(made) = key.as_ref().and_then(|k| self.keys.get(k)) {
            return conflict(made);
        }
        let body = req.json();
        let f = &body["billing_request_flows"];
        let br = f["links"]["billing_request"].as_str().unwrap_or("").to_string();
        let Some(scheme) = self.requests.get(&br).filter(|b| b.status == "pending").map(|b| b.scheme.clone()) else {
            return gc_error(422, "invalid_state", "billing_request_not_pending", "The billing request is not pending");
        };
        let (Some(redirect), Some(exit)) = (f["redirect_uri"].as_str(), f["exit_uri"].as_str()) else {
            return gc_error(422, "validation_failed", "missing_uri", "redirect_uri and exit_uri are required");
        };
        let id = util::upper_id("BRF");
        let flow = Flow {
            id: id.clone(),
            br: br.clone(),
            redirect_uri: redirect.to_string(),
            exit_uri: exit.to_string(),
            expires_at: cx.now + FLOW_SECS,
            debit: None,
        };
        let answer = json!({"billing_request_flows": {
            "id": id, "authorisation_url": format!("{}/gocardless/flow/{id}", cx.base),
            "expires_at": util::rfc3339(flow.expires_at), "redirect_uri": redirect, "exit_uri": exit,
            "links": {"billing_request": br}}});
        cx.emit(event("billing_requests", "flow_created", json!({"billing_request": br}), details("api", "billing_request_flow_created", "Flow created", &scheme), json!({}), cx.now));
        if let Some(k) = key {
            self.keys.insert(k, id.clone());
            bound(&mut self.keys);
        }
        self.flows.insert(id, flow);
        bound(&mut self.flows);
        Response::json(201, answer)
    }

    /// The payer confirmed the mandate at the bank: the mandate, the subscription, the first
    /// debit when it is due now.
    fn fulfil(&mut self, cx: &mut Cx<'_>, flow: &Flow, debit: Debit) {
        let Some(b) = self.requests.get_mut(&flow.br) else {
            return;
        };
        if b.status != "pending" {
            return;
        }
        let (md, sb, cu) = (util::upper_id("MD"), util::upper_id("SB"), util::upper_id("CU"));
        b.status = "fulfilled".into();
        b.mandate = Some(md.clone());
        b.subscription = Some(sb.clone());
        let b = b.clone();
        let links = json!({"billing_request": b.id, "mandate_request_mandate": md,
                           "subscription_request_subscription": sb, "customer": cu});
        cx.emit(event("billing_requests", "fulfilled", links, details("payer", "billing_request_fulfilled", "The payer confirmed the mandate", &b.scheme), util::ref_meta(&b.metadata_ref), cx.now));
        cx.emit(event("mandates", "created", json!({"mandate": md}), details("api", "mandate_created", "Mandate created", &b.scheme), util::ref_meta(&b.mandate_ref), cx.now));
        cx.emit(event("subscriptions", "created", json!({"subscription": sb, "mandate": md}), details("api", "subscription_created", "Subscription created", &b.scheme), util::ref_meta(&b.sub.metadata_ref), cx.now));
        self.subscriptions.insert(
            sb.clone(),
            Subscription {
                id: sb.clone(),
                scheme: b.scheme.clone(),
                mandate: md,
                status: "active".into(),
                amount: b.sub.amount,
                currency: b.sub.currency.clone(),
                count: b.sub.count,
                made: 0,
                name: b.sub.name.clone(),
                metadata_ref: b.sub.metadata_ref.clone(),
                debit,
            },
        );
        bound(&mut self.subscriptions);
        let today = util::ymd(cx.now);
        if b.sub.start_date.as_ref().is_none_or(|d| *d <= today) {
            self.charge(cx, &sb);
        }
    }

    /// The subscription's next debit and its timeline; `false` when its count is used up.
    fn charge(&mut self, cx: &mut Cx<'_>, sb: &str) -> bool {
        let Some(s) = self.subscriptions.get_mut(sb) else {
            return false;
        };
        if s.status != "active" || s.count.is_some_and(|c| s.made >= c) {
            return false;
        }
        s.made += 1;
        let pm = util::upper_id("PM");
        let payment = Payment {
            id: pm.clone(),
            scheme: s.scheme.clone(),
            subscription: s.id.clone(),
            mandate: s.mandate.clone(),
            status: "pending_submission".into(),
            amount: s.amount,
            currency: s.currency.clone(),
            refunded: 0,
            created: cx.now,
            paid_out: false,
        };
        let finished = s.count.is_some_and(|c| s.made >= c);
        let debit = if take(&mut cx.switches.return_next_debit) { Debit::Fail("generic_decline") } else { s.debit };
        let sub_meta = util::ref_meta(&s.metadata_ref);
        let scheme = s.scheme.clone();
        if finished {
            s.status = "finished".into();
        }
        cx.emit(event("payments", "created", json!({"payment": pm, "subscription": sb}), details("gocardless", "payment_created", "Payment created by a subscription", &scheme), json!({}), cx.now));
        if finished {
            cx.emit(event("subscriptions", "finished", json!({"subscription": sb}), details("gocardless", "plan_finished", "The subscription took its last payment", &scheme), sub_meta, cx.now));
        }
        self.payments.insert(pm.clone(), payment);
        bound(&mut self.payments);
        let (now, step) = (cx.now, cx.cfg.step_secs);
        let at = |n: i64| now + n * step;
        let mut plan: Vec<(i64, &'static str)> = vec![(at(1), "submitted")];
        match debit {
            Debit::Ok => plan.extend([(at(2), "confirmed"), (at(3), "paid_out")]),
            Debit::Slow => plan.extend([(at(4), "confirmed"), (at(5), "paid_out")]),
            Debit::Fail(code) => plan.push((at(2), reason_code(code))),
            Debit::Dispute => plan.extend([(at(2), "confirmed"), (at(3), "paid_out"), (at(4), "charged_back")]),
        }
        if take(&mut cx.switches.chargeback_next) && !matches!(debit, Debit::Fail(_) | Debit::Dispute) {
            plan.push((at(4), "charged_back"));
        }
        for (t, to) in plan {
            self.steps.push((t, Step { pm: pm.clone(), to }));
        }
        true
    }

    /// One step of a payment's timeline (or a scenario simulator's): its status and its event.
    /// A failure's step names the bank's reason code.
    fn apply(&mut self, cx: &mut Cx<'_>, pm: &str, to: &str) -> bool {
        let Some(p) = self.payments.get_mut(pm) else {
            return false;
        };
        let scheme = p.scheme.clone();
        let (action, detail) = match to {
            "submitted" => ("submitted", details("gocardless", "payment_submitted", "Payment submitted to the banks", &scheme)),
            "confirmed" => ("confirmed", details("gocardless", "payment_confirmed", "Payment collected", &scheme)),
            "paid_out" => ("paid_out", details("gocardless", "payment_paid_out", "Payment paid out", &scheme)),
            "charged_back" => {
                let mut d = details("bank", "payment_charged_back", "The payer's bank charged the payment back", &scheme);
                d["reason_code"] = json!("MD06");
                ("charged_back", d)
            }
            code @ ("AM04" | "MD01" | "MS03") => {
                let mut d = details("bank", "payment_failed", "The payer's bank returned the debit", &scheme);
                d["reason_code"] = json!(code);
                ("failed", d)
            }
            _ => return false,
        };
        p.status = action.into();
        if action == "paid_out" {
            p.paid_out = true;
        }
        cx.emit(event("payments", action, json!({"payment": pm}), detail, json!({}), cx.now));
        true
    }

    fn refund(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let body = req.json();
        let r = &body["refunds"];
        let pm = r["links"]["payment"].as_str().unwrap_or("").to_string();
        let Some(p) = self.payments.get_mut(&pm) else {
            return gc_error(404, "invalid_api_usage", "resource_not_found", "No such payment");
        };
        if p.status != "confirmed" && p.status != "paid_out" {
            return gc_error(422, "invalid_state", "refund_payment_invalid_state", "Only a collected payment can be refunded");
        }
        let amount = r["amount"].as_i64().unwrap_or(0);
        if amount <= 0 || amount > p.amount - p.refunded {
            return gc_error(422, "validation_failed", "refund_amount_invalid", "The refund is more than is left to refund");
        }
        if r["total_amount_confirmation"].as_i64() != Some(p.refunded + amount) {
            return gc_error(422, "validation_failed", "total_amount_confirmation_invalid", "total_amount_confirmation must be the total refunded, this refund included");
        }
        p.refunded += amount;
        let id = util::upper_id("RF");
        let refund = json!({"id": id, "amount": amount, "currency": p.currency, "created_at": util::rfc3339(cx.now),
                            "links": {"payment": pm}, "metadata": r["metadata"].clone()});
        cx.emit(event("refunds", "created", json!({"refund": id, "payment": pm}), details("api", "payment_refunded", "Refund created", &p.scheme), json!({}), cx.now));
        self.refunds.insert(id, refund.clone());
        bound(&mut self.refunds);
        Response::json(201, json!({ "refunds": refund }))
    }

    fn api(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, segs: &[&str]) -> Response {
        if req.bearer() != Some(cx.cfg.gocardless.secret_key.as_str()) {
            return gc_error(401, "invalid_api_usage", "access_token_not_found", "Invalid access token");
        }
        if req.header("gocardless-version") != Some(VERSION) {
            return gc_error(400, "invalid_api_usage", "version_required", "GoCardless-Version: 2015-07-06 is required");
        }
        let missing = || gc_error(404, "invalid_api_usage", "resource_not_found", "Resource not found");
        match (req.method, segs) {
            ("POST", ["billing_requests"]) => self.create_billing_request(cx, req),
            ("GET", ["billing_requests", id]) => self.requests.get(*id).map_or_else(missing, |b| Response::json(200, Self::br_json(b))),
            ("POST", ["billing_requests", id, "actions", "cancel"]) => {
                let Some(b) = self.requests.get_mut(*id) else {
                    return missing();
                };
                if b.status != "pending" {
                    return gc_error(422, "invalid_state", "billing_request_not_cancellable", "Only a pending billing request can be cancelled");
                }
                b.status = "cancelled".into();
                let b = b.clone();
                cx.emit(event("billing_requests", "cancelled", json!({"billing_request": b.id}), details("api", "billing_request_cancelled", "Cancelled", &b.scheme), util::ref_meta(&b.metadata_ref), cx.now));
                Response::json(200, Self::br_json(&b))
            }
            ("POST", ["billing_request_flows"]) => self.create_flow(cx, req),
            ("GET", ["payments", id]) => self.payments.get(*id).map_or_else(missing, |p| Response::json(200, Self::payment_json(p))),
            ("POST", ["refunds"]) => self.refund(cx, req),
            ("GET", ["refunds", id]) => self.refunds.get(*id).map_or_else(missing, |r| Response::json(200, json!({ "refunds": r }))),
            ("GET", ["subscriptions", id]) => self.subscriptions.get(*id).map_or_else(missing, |s| {
                Response::json(200, json!({"subscriptions": {"id": s.id, "status": s.status, "amount": s.amount,
                                                             "currency": s.currency, "name": s.name, "count": s.count,
                                                             "metadata": util::ref_meta(&s.metadata_ref), "links": {"mandate": s.mandate}}}))
            }),
            ("POST", ["subscriptions", id, "actions", "cancel"]) => {
                let Some(s) = self.subscriptions.get_mut(*id) else {
                    return missing();
                };
                if s.status != "active" {
                    return gc_error(422, "invalid_state", "subscription_not_active", "The subscription is not active");
                }
                s.status = "cancelled".into();
                let m = util::ref_meta(&s.metadata_ref);
                let scheme = s.scheme.clone();
                cx.emit(event("subscriptions", "cancelled", json!({"subscription": id}), details("api", "subscription_cancelled", "Cancelled", &scheme), m, cx.now));
                Response::json(200, json!({"subscriptions": {"id": id, "status": "cancelled"}}))
            }
            ("POST", ["scenario_simulators", name, "actions", "run"]) => {
                let target = req.json()["data"]["links"]["resource"].as_str().unwrap_or("").to_string();
                let to = match *name {
                    "payment_confirmed" => "confirmed",
                    "payment_paid_out" => "paid_out",
                    "payment_failed" => "AM04",
                    "payment_charged_back" => "charged_back",
                    _ => return missing(),
                };
                if !self.apply(cx, &target, to) {
                    return missing();
                }
                Response::json(200, json!({"scenario_simulators": {"id": name}}))
            }
            _ => missing(),
        }
    }

    // ---- the payer's flow pages ----

    fn flow_open(&self, id: &str, now: i64) -> Option<(Flow, BillingRequest)> {
        let f = self.flows.get(id)?;
        let b = self.requests.get(&f.br)?;
        (b.status == "pending" && now < f.expires_at).then(|| (f.clone(), b.clone()))
    }

    fn closed() -> Response {
        let body = "<h2>This test flow is closed</h2><p>It was completed, cancelled or expired. Go back to the app.</p>";
        Response::html(409, kit::single("Flow closed", SHAPE, body, ""))
    }

    /// The debit's name for the payer: SEPA Direct Debit, or the scheme's (Bacs Direct Debit, ...).
    fn debit_name(&self, b: &BillingRequest) -> String {
        self.scheme_route(&b.scheme)
            .map(|l| l.method.name.clone())
            .unwrap_or_else(|| "SEPA Direct Debit".to_string())
    }

    fn order(&self, b: &BillingRequest) -> String {
        let amount = local::money(b.sub.amount, &b.sub.currency);
        let by = self.debit_name(b);
        let note = match b.sub.count {
            Some(1) => format!("One debit by {by}"),
            _ => format!("{amount} every month by {by}"),
        };
        kit::order(&b.sub.name, &amount, &note)
    }

    /// Step one: the account holder and the IBAN, or another scheme's test account.
    fn details_page(&self, f: &Flow, b: &BillingRequest, message: &str) -> Response {
        let (class, inputs) = match self.scheme_route(&b.scheme) {
            Some(l) => ("account", local::account_inputs(&l)),
            None => (
                "iban",
                "<label>Account holder<input name=\"holder\" autocomplete=\"off\"></label>\
                 <label>IBAN<input name=\"iban\" placeholder=\"DE89 3704 0044 0532 0130 00\" autocomplete=\"off\"></label>"
                    .to_string(),
            ),
        };
        let right = format!(
            "<h2>Set up a Direct Debit (test)</h2><form class=\"{class}\" method=\"post\" action=\"/gocardless/flow/{}\" autocomplete=\"off\">\
             {inputs}<div class=\"error\">{}</div><button>Continue</button></form>\
             <p class=\"muted\"><a href=\"{}\">Cancel and go back</a></p>",
            kit::esc(&f.id),
            kit::esc(message),
            kit::esc(&f.exit_uri)
        );
        Response::html(200, kit::split("Direct Debit", SHAPE, &self.order(b), &right, kit::IBAN_SCRIPT))
    }

    /// Step two: the mandate, with the holder and the masked account shown back once.
    fn mandate_page(&self, f: &Flow, b: &BillingRequest, holder: &str, account: &str) -> Response {
        let left = format!(
            "{}<h3>The mandate (test)</h3><p class=\"muted\">By confirming you would authorise the test merchant to send \
             instructions to your bank to debit your account, and your bank to debit it accordingly. This is a test: \
             no bank is asked and nothing is debited.</p>",
            self.order(b)
        );
        let right = format!(
            "<h2>Confirm your details</h2><p>Account holder: <b>{}</b></p><p>Account: <b>{}</b></p>\
             <form method=\"post\" action=\"/gocardless/flow/{}/mandate\"><button>Set up the mandate</button></form>",
            kit::esc(holder),
            kit::esc(account),
            kit::esc(&f.id)
        );
        Response::html(200, kit::split("Confirm the mandate", SHAPE, &left, &right, ""))
    }

    /// Step three: the payer's bank asks to confirm (the redirect a real flow makes).
    fn bank_page(f: &Flow) -> Response {
        let body = format!(
            "<h2>Test bank: confirm the Direct Debit mandate</h2><p>A test merchant asks to debit your account.</p>\
             <form method=\"post\" action=\"/gocardless/flow/{}/bank\"><button name=\"decision\" value=\"confirm\">Confirm</button>\
             <button class=\"plain\" name=\"decision\" value=\"cancel\">Cancel</button></form>",
            kit::esc(&f.id)
        );
        Response::html(200, kit::single("Confirm at your bank", SHAPE, &body, ""))
    }

    fn flow_pages(&mut self, cx: &mut Cx<'_>, req: &Request<'_>, rest: &[&str]) -> Response {
        let Some(id) = rest.first() else {
            return Response::not_found();
        };
        let Some((flow, b)) = self.flow_open(id, cx.now) else {
            return if self.flows.contains_key(*id) { Self::closed() } else { Response::not_found() };
        };
        let form = req.form();
        match (req.method, rest.get(1).copied()) {
            ("GET", None) => self.details_page(&flow, &b, ""),
            ("POST", None) if b.scheme != SEPA => {
                // another scheme: only its documented test account, whose outcome is all that is kept
                let Some(l) = self.scheme_route(&b.scheme) else {
                    return Self::closed();
                };
                let debit = match l.answer(&form) {
                    Answer::Matched(t) if t.outcome == Outcome::Succeeded => Debit::Ok,
                    Answer::Matched(_) => Debit::Fail("generic_decline"),
                    Answer::Refused(message) => return self.details_page(&flow, &b, &message),
                };
                if let Some(f) = self.flows.get_mut(*id) {
                    f.debit = Some(debit);
                }
                self.mandate_page(&flow, &b, "", "the documented test account")
            }
            ("POST", None) => {
                let iban = util::compact_iban(form.get("iban").map(String::as_str).unwrap_or(""));
                if let Some(problem) = iban_problem(&iban) {
                    return self.details_page(&flow, &b, problem);
                }
                let Some(debit) = debit_of(&iban) else {
                    return self.details_page(&flow, &b, crate::testdata::TEST_IBANS_ONLY);
                };
                if let Some(f) = self.flows.get_mut(*id) {
                    f.debit = Some(debit);
                }
                let holder = form.get("holder").map(String::as_str).unwrap_or("").trim().to_string();
                let holder: String = holder.chars().take(70).collect();
                self.mandate_page(&flow, &b, &holder, &util::masked(&iban))
            }
            ("POST", Some("mandate")) if flow.debit.is_some() => Self::bank_page(&flow),
            ("POST", Some("bank")) => {
                let Some(debit) = flow.debit else {
                    return self.details_page(&flow, &b, "Enter the account first.");
                };
                if form.get("decision").map(String::as_str) != Some("confirm") {
                    return Response::redirect(&flow.exit_uri);
                }
                self.fulfil(cx, &flow, debit);
                Response::redirect(&flow.redirect_uri)
            }
            (_, Some("mandate")) => self.details_page(&flow, &b, "Enter the account first."),
            _ => Response::not_found(),
        }
    }
}

impl FakeProvider for FakeGoCardless {
    fn kind(&self) -> Kind {
        Kind::GoCardless
    }

    fn handle(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response {
        let segs: Vec<&str> = req.path.trim_start_matches('/').split('/').collect();
        match segs.as_slice() {
            ["flow", rest @ ..] => self.flow_pages(cx, req, rest),
            api => self.api(cx, req, api),
        }
    }

    fn steps(&mut self, cx: &mut Cx<'_>) {
        let due = due_steps(&mut self.steps, cx.now);
        for s in due {
            self.apply(cx, &s.pm, s.to);
        }
    }

    fn advance(&mut self, cx: &mut Cx<'_>, months: i64) -> usize {
        let subs: Vec<String> = self.subscriptions.values().filter(|s| s.status == "active").map(|s| s.id.clone()).collect();
        let mut charged = 0;
        for sb in subs {
            for _ in 0..months {
                if self.charge(cx, &sb) {
                    charged += 1;
                }
            }
        }
        charged
    }

    fn sign(&self, cfg: &Config, body: &[u8], _now: i64) -> (&'static str, String) {
        ("Webhook-Signature", util::gocardless_signature(&cfg.gocardless.webhook_secret, body))
    }

    fn batches(&self) -> bool {
        true
    }

    fn keys(&self, cfg: &Config, base: &str) -> Value {
        json!({"api": format!("{base}{}", Kind::GoCardless.prefix()), "secret_key": cfg.gocardless.secret_key,
               "webhook_secret": cfg.gocardless.webhook_secret, "api_version": VERSION})
    }

    fn summary(&self) -> Value {
        let requests: Vec<Value> = self.requests.values().map(|b| json!({"id": b.id, "status": b.status})).collect();
        let payments: Vec<Value> = self
            .payments
            .values()
            .map(|p| json!({"id": p.id, "status": p.status, "subscription": p.subscription, "refunded": p.refunded}))
            .collect();
        let subs: Vec<Value> = self
            .subscriptions
            .values()
            .map(|s| json!({"id": s.id, "status": s.status, "payments": s.made}))
            .collect();
        json!({"billing_requests": requests, "payments": payments, "subscriptions": subs, "refunds": self.refunds.len()})
    }
}
