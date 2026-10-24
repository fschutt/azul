//! The sandbox without its network: [`Core::handle`] answers one request at the time it is given
//! and [`Core::tick`] runs what is scheduled (a SEPA debit settles, a dispute arrives) and hands
//! back the webhooks that are due, signed at that time. The server ([`crate::start`]) feeds it
//! from a loopback socket on its own clock; tests and in-process users drive it directly.
//!
//! Each provider is one module implementing [`FakeProvider`]: its API and pages under its path
//! prefix, its timeline, its webhook signature. Adding a provider is a module, a [`crate::Kind`]
//! and its keys in [`Config`].

use std::collections::{BTreeMap, VecDeque};

use serde_json::{json, Value};

use crate::{guard, kit, util, Config, Error, Kind};

/// The most objects of one kind a provider keeps (sessions, payments, ...): a long run fills no
/// memory.
pub const MAX_OBJECTS: usize = 5_000;
/// The most webhooks waiting to be sent.
pub const MAX_QUEUE: usize = 2_000;
/// The most entries of the delivery log and of the bridge log.
pub const MAX_LOG: usize = 200;
/// How often a webhook the backend did not take is sent again, and the first wait.
pub const MAX_ATTEMPTS: u32 = 5;
pub const RETRY_SECS: i64 = 2;
/// The longest a switch may delay webhooks.
pub const MAX_DELAY_SECS: i64 = 3_600;
/// The most months one advance charges.
pub const MAX_ADVANCE_MONTHS: i64 = 36;
/// The most events one batched webhook carries.
const MAX_BATCH: usize = 100;

/// One request: `path` without its query.
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
}

impl Request<'_> {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn bearer(&self) -> Option<&str> {
        self.header("authorization")
            .and_then(|v| v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")))
    }

    /// The body as a form.
    pub fn form(&self) -> BTreeMap<String, String> {
        util::form_decode(&String::from_utf8_lossy(self.body)).into_iter().collect()
    }

    /// The body as JSON (`Null` when it is not).
    pub fn json(&self) -> Value {
        serde_json::from_slice(self.body).unwrap_or(Value::Null)
    }

    pub fn query_param(&self, name: &str) -> Option<String> {
        util::form_decode(self.query?)
            .into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
}

/// One answer.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The server says nothing for this long and then closes (`/_dev/hang`: a provider that
    /// takes the connection and never answers); zero for every real answer.
    pub hang_secs: u64,
}

impl Response {
    pub fn json(status: u16, v: Value) -> Response {
        Response {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: serde_json::to_vec(&v).unwrap_or_default(),
            hang_secs: 0,
        }
    }

    pub fn html(status: u16, html: String) -> Response {
        Response {
            status,
            headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
            body: html.into_bytes(),
            hang_secs: 0,
        }
    }

    pub fn redirect(location: &str) -> Response {
        Response {
            status: 303,
            headers: vec![("Location".into(), location.to_string())],
            body: Vec::new(),
            hang_secs: 0,
        }
    }

    pub fn empty(status: u16) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            hang_secs: 0,
        }
    }

    pub fn not_found() -> Response {
        Response::json(404, json!({"error": "not_found", "message": "no such page or API"}))
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// What a provider may use while it answers: the time, the configuration, its own address, the
/// switches, and the outbox its events go into.
pub struct Cx<'a> {
    pub now: i64,
    pub cfg: &'a Config,
    /// This sandbox's address (`http://127.0.0.1:8784`): its pages' URLs name it.
    pub base: &'a str,
    pub switches: &'a mut Switches,
    outbox: &'a mut Vec<(i64, Value)>,
}

impl Cx<'_> {
    /// Queues `event` for this provider's webhook, after the switches' delay.
    pub fn emit(&mut self, event: Value) {
        let due = self.now + self.switches.delay_secs;
        if self.outbox.len() < MAX_QUEUE {
            self.outbox.push((due, event));
        }
    }
}

/// One fake provider.
pub trait FakeProvider: Send {
    fn kind(&self) -> Kind;
    /// Pages it serves outside its prefix (the fields page).
    fn owns(&self, _path: &str) -> bool {
        false
    }
    /// `req.path` is below the provider's prefix, or the whole path of a page it owns.
    fn handle(&mut self, cx: &mut Cx<'_>, req: &Request<'_>) -> Response;
    /// The scheduled steps due at `cx.now`.
    fn steps(&mut self, cx: &mut Cx<'_>);
    /// Charges the next `months` of every recurring plan; how many charges were made.
    fn advance(&mut self, cx: &mut Cx<'_>, months: i64) -> usize;
    /// The webhook's signature header for `body` sent at `now`.
    fn sign(&self, cfg: &Config, body: &[u8], now: i64) -> (&'static str, String);
    /// Every header that proves the webhook (dLocal's date and signature): by default the one
    /// [`FakeProvider::sign`] gives.
    fn signed_headers(&self, cfg: &Config, body: &[u8], now: i64) -> Vec<(String, String)> {
        let (name, value) = self.sign(cfg, body, now);
        vec![(name.to_string(), value)]
    }
    /// The webhook body's type.
    fn content_type(&self) -> &'static str {
        "application/json"
    }
    /// The webhook body of an emitted event: by default its JSON; a string as it is (EBANX's form).
    fn body_of(&self, event: &Value) -> Vec<u8> {
        match event {
            Value::String(s) => s.clone().into_bytes(),
            other => serde_json::to_vec(other).unwrap_or_default(),
        }
    }
    /// What the webhook URL carries after `?` (EBANX's notification URL with the merchant's token).
    fn webhook_query(&self, _cfg: &Config) -> Option<String> {
        None
    }
    /// Whether due events go out together as `{"events": [...]}`.
    fn batches(&self) -> bool {
        false
    }
    /// What `/_dev/state` shows of it: outcomes only, never payment data.
    fn summary(&self) -> Value;
    /// What a backend is configured with to use it (`/_dev/keys`): its API address and its
    /// test keys.
    fn keys(&self, cfg: &Config, base: &str) -> Value;
}

/// The dev switches; each `*_next` one acts once.
#[derive(Debug, Clone, Default)]
pub struct Switches {
    pub decline_next: bool,
    pub three_ds_next: bool,
    pub return_next_debit: bool,
    pub chargeback_next: bool,
    pub drop_next_webhook: bool,
    pub duplicate_next_webhook: bool,
    /// Every webhook waits this long before it is sent.
    pub delay_secs: i64,
    /// The next request whose path starts with this gets no answer for `hang_secs` (a provider
    /// that hangs; the token server's own timeout must give it up).
    pub hang_next: Option<String>,
    pub hang_secs: u64,
    /// A test card the next fields page comes with, as if typed (`/_dev/fill`): a scripted run
    /// in a real window, where no script reaches into the web view. Taken by that page.
    pub fill_next: Option<Fill>,
}

/// A test card for the next fields page: digits only, as the payer would type them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fill {
    pub card_number: String,
    /// `MMYY`.
    pub exp: String,
    pub cvc: String,
}

/// The longest a hanging answer waits.
pub const MAX_HANG_SECS: u64 = 300;

/// Takes a `*_next` switch: whether it was on (it is off afterwards).
pub fn take(flag: &mut bool) -> bool {
    std::mem::replace(flag, false)
}

/// A signed webhook, ready to be posted.
#[derive(Debug, Clone)]
pub struct Delivery {
    /// The provider's id (`fake-stripe`).
    pub provider: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// How often it was sent before (a retry is signed again).
    pub attempt: u32,
}

enum Payload {
    Event(Value),
    Body(Vec<u8>),
}

struct Queued {
    due: i64,
    /// The index of its provider.
    provider: usize,
    payload: Payload,
    attempt: u32,
}

pub struct Core {
    cfg: Config,
    base: String,
    switches: Switches,
    providers: Vec<Box<dyn FakeProvider>>,
    queue: Vec<Queued>,
    sent: u64,
    dropped: u64,
    log: VecDeque<Value>,
    /// The bridge messages a fields page sent (their names only, never their values).
    bridge: VecDeque<String>,
}

impl Core {
    /// The sandbox of `cfg`, serving at `base` (`http://127.0.0.1:<port>`). A webhook URL off
    /// this computer is refused.
    pub fn new(cfg: &Config, base: &str) -> Result<Core, Error> {
        guard::loopback_webhook_url(&cfg.webhook_url)?;
        let mut providers: Vec<Box<dyn FakeProvider>> = Vec::new();
        for kind in &cfg.providers {
            if providers.iter().any(|p| p.kind() == *kind) {
                continue;
            }
            let fake: Box<dyn FakeProvider> = match kind {
                Kind::Stripe => Box::new(crate::stripe::FakeStripe::new()?),
                Kind::GoCardless => Box::new(crate::gocardless::FakeGoCardless::new()?),
                Kind::DLocal => Box::new(crate::dlocal::FakeDLocal::new()?),
                Kind::Ebanx => Box::new(crate::ebanx::FakeEbanx::new()?),
                Kind::Komoju => Box::new(crate::komoju::FakeKomoju::new()?),
            };
            providers.push(fake);
        }
        Ok(Core {
            cfg: cfg.clone(),
            base: base.trim_end_matches('/').to_string(),
            switches: Switches::default(),
            providers,
            queue: Vec::new(),
            sent: 0,
            dropped: 0,
            log: VecDeque::new(),
            bridge: VecDeque::new(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// Runs `f` on provider `i` at `now` and queues what it emitted.
    fn with<T>(&mut self, i: usize, now: i64, f: impl FnOnce(&mut (dyn FakeProvider + 'static), &mut Cx<'_>) -> T) -> Option<T> {
        let mut outbox = Vec::new();
        let p = self.providers.get_mut(i)?;
        let mut cx = Cx {
            now,
            cfg: &self.cfg,
            base: &self.base,
            switches: &mut self.switches,
            outbox: &mut outbox,
        };
        let out = f(p.as_mut(), &mut cx);
        for (due, event) in outbox {
            if self.queue.len() >= MAX_QUEUE {
                self.queue.remove(0);
                self.dropped += 1;
            }
            self.queue.push(Queued {
                due,
                provider: i,
                payload: Payload::Event(event),
                attempt: 0,
            });
        }
        Some(out)
    }

    /// One request; every answer carries the security headers.
    pub fn handle(&mut self, req: &Request<'_>, now: i64) -> Response {
        let mut r = self.route(req, now);
        r.headers.extend(kit::security_headers());
        r
    }

    fn route(&mut self, req: &Request<'_>, now: i64) -> Response {
        // a name that is not this computer's gets nothing (DNS rebinding)
        if !req.header("host").is_some_and(guard::loopback_host) {
            return Response::json(403, json!({"error": "not_loopback", "message": "this test sandbox answers loopback names only"}));
        }
        if self.switches.hang_next.as_deref().is_some_and(|p| req.path.starts_with(p)) {
            self.switches.hang_next = None;
            let mut r = Response::empty(504);
            r.hang_secs = self.switches.hang_secs.clamp(1, MAX_HANG_SECS);
            return r;
        }
        let method = req.method.to_ascii_uppercase();
        let owner = self.providers.iter().position(|p| {
            let prefix = p.kind().prefix();
            p.owns(req.path) || req.path == prefix || req.path.starts_with(&format!("{prefix}/"))
        });
        if let Some(i) = owner {
            let path = match self.providers.get(i) {
                Some(p) if !p.owns(req.path) => req.path.get(p.kind().prefix().len()..).unwrap_or(""),
                _ => req.path,
            };
            let sub = Request {
                method: &method,
                path,
                query: req.query,
                headers: req.headers,
                body: req.body,
            };
            return self.with(i, now, |p, cx| p.handle(cx, &sub)).unwrap_or_else(Response::not_found);
        }
        if let Some(message) = req.path.strip_prefix("/_bridge/") {
            // the app cancels these navigations; a browser stays on the page with a 204
            if self.bridge.len() >= MAX_LOG {
                self.bridge.pop_front();
            }
            let name: String = message.chars().take(32).filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            self.bridge.push_back(name);
            return Response::empty(204);
        }
        match (method.as_str(), req.path) {
            ("GET", "/health") => Response::json(200, json!({"ok": true, "test_only": true})),
            ("GET", "/") => {
                let list: Vec<(&str, &str)> = self.providers.iter().map(|p| (p.kind().id(), p.kind().prefix())).collect();
                Response::html(200, kit::index(&list))
            }
            ("GET", "/_dev/state") => Response::json(200, self.state()),
            ("GET", "/_dev/keys") => Response::json(200, self.keys()),
            ("POST", "/_dev/switch") => self.switch(req.body),
            ("POST", "/_dev/delay") => self.delay(req.body),
            ("POST", "/_dev/hang") => self.hang(req.body),
            ("POST", "/_dev/fill") => self.fill(req.body),
            ("POST", "/_dev/advance") => self.advance(req.body, now),
            ("POST", "/_dev/reset") => {
                self.switches = Switches::default();
                Response::json(200, json!({"switches": self.switches_json()}))
            }
            _ => Response::not_found(),
        }
    }

    fn switches_json(&self) -> Value {
        let s = &self.switches;
        json!({"decline_next": s.decline_next, "three_ds_next": s.three_ds_next,
               "return_next_debit": s.return_next_debit, "chargeback_next": s.chargeback_next,
               "drop_next_webhook": s.drop_next_webhook, "duplicate_next_webhook": s.duplicate_next_webhook,
               "delay_secs": s.delay_secs, "hang_next": s.hang_next, "hang_secs": s.hang_secs,
               "fill_next": s.fill_next.is_some()})
    }

    /// `{"card_number", "exp": "MMYY", "cvc"}`: the next fields page comes with this test card in
    /// its inputs. Digits only (they go into the page's HTML).
    fn fill(&mut self, body: &[u8]) -> Response {
        let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let digits = |k: &str, max: usize| {
            v.get(k)
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty() && t.len() <= max && t.bytes().all(|b| b.is_ascii_digit()))
                .map(str::to_string)
        };
        match (digits("card_number", 19), digits("exp", 4), digits("cvc", 4)) {
            (Some(card_number), Some(exp), Some(cvc)) if exp.len() == 4 => {
                self.switches.fill_next = Some(Fill { card_number, exp, cvc });
                Response::json(200, json!({"switches": self.switches_json()}))
            }
            _ => Response::json(400, json!({"error": "bad_fill", "message": "card_number, exp (MMYY) and cvc: digits only"})),
        }
    }

    fn switch(&mut self, body: &[u8]) -> Response {
        let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let on = v.get("on").and_then(Value::as_bool).unwrap_or(true);
        let s = &mut self.switches;
        let flag = match v.get("name").and_then(Value::as_str).unwrap_or("") {
            "decline_next" => &mut s.decline_next,
            "three_ds_next" => &mut s.three_ds_next,
            "return_next_debit" => &mut s.return_next_debit,
            "chargeback_next" => &mut s.chargeback_next,
            "drop_next_webhook" => &mut s.drop_next_webhook,
            "duplicate_next_webhook" => &mut s.duplicate_next_webhook,
            other => {
                let why = format!("no switch {other:?}: decline_next, three_ds_next, return_next_debit, chargeback_next, drop_next_webhook, duplicate_next_webhook");
                return Response::json(400, json!({"error": "bad_switch", "message": why}));
            }
        };
        *flag = on;
        Response::json(200, json!({"switches": self.switches_json()}))
    }

    /// `{"prefix": "/stripe/v1/", "secs": 60}`: the next request under `prefix` is not answered
    /// for `secs` (60 when not given).
    fn hang(&mut self, body: &[u8]) -> Response {
        let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let Some(prefix) = v.get("prefix").and_then(Value::as_str).filter(|p| p.starts_with('/')) else {
            return Response::json(400, json!({"error": "bad_hang", "message": "prefix: the path the next hanging request starts with"}));
        };
        self.switches.hang_next = Some(prefix.to_string());
        self.switches.hang_secs = v.get("secs").and_then(Value::as_u64).unwrap_or(60).clamp(1, MAX_HANG_SECS);
        Response::json(200, json!({"switches": self.switches_json()}))
    }

    fn delay(&mut self, body: &[u8]) -> Response {
        let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let secs = v.get("secs").and_then(Value::as_i64).unwrap_or(0);
        if !(0..=MAX_DELAY_SECS).contains(&secs) {
            let why = format!("a delay is 0 to {MAX_DELAY_SECS} seconds");
            return Response::json(400, json!({"error": "bad_delay", "message": why}));
        }
        self.switches.delay_secs = secs;
        Response::json(200, json!({"switches": self.switches_json()}))
    }

    /// Recurring plans charge their next `months` now.
    fn advance(&mut self, body: &[u8], now: i64) -> Response {
        let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let months = v.get("months").and_then(Value::as_i64).unwrap_or(1);
        if !(1..=MAX_ADVANCE_MONTHS).contains(&months) {
            let why = format!("advance 1 to {MAX_ADVANCE_MONTHS} months");
            return Response::json(400, json!({"error": "bad_months", "message": why}));
        }
        let which = v.get("provider").and_then(Value::as_str).and_then(Kind::parse);
        let mut charged = 0;
        for i in 0..self.providers.len() {
            let kind = self.providers.get(i).map(|p| p.kind());
            if which.is_none() || kind == which {
                charged += self.with(i, now, |p, cx| p.advance(cx, months)).unwrap_or(0);
            }
        }
        Response::json(200, json!({"charged": charged}))
    }

    /// The scheduled steps that are due, then the webhooks that are due, signed now. A provider
    /// that batches sends its due events as one body. The switches act on what leaves.
    pub fn tick(&mut self, now: i64) -> Vec<Delivery> {
        for i in 0..self.providers.len() {
            self.with(i, now, |p, cx| p.steps(cx));
        }
        let queue = std::mem::take(&mut self.queue);
        let (due, later): (Vec<Queued>, Vec<Queued>) = queue.into_iter().partition(|q| q.due <= now);
        self.queue = later;
        let mut out = Vec::new();
        let mut batches: BTreeMap<usize, Vec<Value>> = BTreeMap::new();
        for q in due {
            let batched = self.providers.get(q.provider).is_some_and(|p| p.batches());
            match q.payload {
                Payload::Event(e) if batched => batches.entry(q.provider).or_default().push(e),
                Payload::Event(e) => {
                    let body = self.providers.get(q.provider).map(|p| p.body_of(&e)).unwrap_or_default();
                    out.extend(self.sign(q.provider, body, q.attempt, now));
                }
                Payload::Body(body) => out.extend(self.sign(q.provider, body, q.attempt, now)),
            }
        }
        for (i, events) in batches {
            for chunk in events.chunks(MAX_BATCH) {
                let body = serde_json::to_vec(&json!({ "events": chunk })).unwrap_or_default();
                out.extend(self.sign(i, body, 0, now));
            }
        }
        let mut sent = Vec::new();
        for d in out {
            if take(&mut self.switches.drop_next_webhook) {
                self.dropped += 1;
                self.note(&d, "dropped");
                continue;
            }
            if take(&mut self.switches.duplicate_next_webhook) {
                sent.push(d.clone());
            }
            sent.push(d);
        }
        self.sent += sent.len() as u64;
        sent
    }

    fn sign(&self, i: usize, body: Vec<u8>, attempt: u32, now: i64) -> Option<Delivery> {
        let p = self.providers.get(i)?;
        let mut headers = vec![("Content-Type".to_string(), p.content_type().to_string())];
        headers.extend(p.signed_headers(&self.cfg, &body, now));
        let mut url = self.cfg.webhook_url.replace("{provider}", p.kind().id());
        if let Some(q) = p.webhook_query(&self.cfg) {
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str(&q);
        }
        Some(Delivery {
            provider: p.kind().id(),
            url,
            headers,
            body,
            attempt,
        })
    }

    /// What the backend answered a delivery (`None`: no answer). One it did not take is sent
    /// again later, at most [`MAX_ATTEMPTS`] times, as the providers retry.
    pub fn delivered(&mut self, d: Delivery, status: Option<u16>, now: i64) {
        let ok = status.is_some_and(|s| (200..300).contains(&s));
        let what = status.map_or_else(|| "no answer".to_string(), |s| s.to_string());
        self.note(&d, &what);
        if ok || d.attempt + 1 >= MAX_ATTEMPTS {
            return;
        }
        let Some(provider) = self.providers.iter().position(|p| p.kind().id() == d.provider) else {
            return;
        };
        if self.queue.len() < MAX_QUEUE {
            self.queue.push(Queued {
                due: now + (RETRY_SECS << d.attempt.min(10)),
                provider,
                payload: Payload::Body(d.body),
                attempt: d.attempt + 1,
            });
        }
    }

    fn note(&mut self, d: &Delivery, outcome: &str) {
        let body: Value = serde_json::from_slice(&d.body).unwrap_or(Value::Null);
        let events: Vec<Value> = match body.get("events").and_then(Value::as_array) {
            Some(list) => list
                .iter()
                .map(|e| json!(format!("{}.{}", e["resource_type"].as_str().unwrap_or(""), e["action"].as_str().unwrap_or(""))))
                .collect(),
            // dLocal's notification is the object with its status; EBANX's a form naming hashes
            None if body.get("status").is_some() => vec![body["status"].clone()],
            None if body.is_null() => vec![json!("notification")],
            None => vec![body["type"].clone()],
        };
        if self.log.len() >= MAX_LOG {
            self.log.pop_front();
        }
        self.log.push_back(json!({"provider": d.provider, "events": events, "attempt": d.attempt, "outcome": outcome}));
    }

    /// What a backend is configured with (`/_dev/keys`): each provider's API address and test
    /// keys, and where the webhooks go.
    pub fn keys(&self) -> Value {
        let mut providers = serde_json::Map::new();
        for p in &self.providers {
            providers.insert(p.kind().id().to_string(), p.keys(&self.cfg, &self.base));
        }
        json!({"test_only": true, "providers": providers, "webhook_url": self.cfg.webhook_url})
    }

    /// What `/_dev/state` shows: each provider's objects and outcomes (never payment data), the
    /// webhooks, the switches, the bridge messages.
    pub fn state(&self) -> Value {
        let mut out = json!({
            "test_only": true,
            "webhooks": {"sent": self.sent, "dropped": self.dropped, "queued": self.queue.len(),
                         "log": self.log.iter().cloned().collect::<Vec<Value>>()},
            "switches": self.switches_json(),
            "bridge": self.bridge.iter().cloned().collect::<Vec<String>>(),
        });
        for p in &self.providers {
            out[p.kind().short()] = p.summary();
        }
        out
    }
}

/// Takes the steps of a provider's timeline that are due at `now`, oldest first.
pub fn due_steps<S>(steps: &mut Vec<(i64, S)>, now: i64) -> Vec<S> {
    let (mut due, later): (Vec<_>, Vec<_>) = std::mem::take(steps).into_iter().partition(|(t, _)| *t <= now);
    *steps = later;
    due.sort_by_key(|(t, _)| *t);
    due.into_iter().map(|(_, s)| s).collect()
}

/// Keeps `map` at most [`MAX_OBJECTS`] long (any old entry goes; a local run never gets near).
pub fn bound<V>(map: &mut BTreeMap<String, V>) {
    while map.len() > MAX_OBJECTS {
        let Some(k) = map.keys().next().cloned() else {
            break;
        };
        map.remove(&k);
    }
}
