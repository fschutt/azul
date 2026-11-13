//! What the tests of the surface, the machine and the redaction share: Stripe's offer of a card,
//! PayPal and SEPA, and the checkouts it makes.

use serde_json::{json, Value};

use crate::{
    identity::Payer,
    machine::{step, Effect, Event, State},
    offer::{Offer, OfferContext},
    pills::{Choice, Pill, PillContext},
    registry::{Method, SurfaceKind},
    surface::{Created, Look},
};

/// What AzDrive can show: the popover's fields, a hosted page, the browser.
pub const APP: &[SurfaceKind] = &[
    SurfaceKind::PopoverFields,
    SurfaceKind::WebviewPage,
    SurfaceKind::SystemBrowser,
];

pub fn offer() -> Offer {
    let text = json!({"offers": [{"provider": "stripe", "methods": [
        {"method": "card"}, {"method": "paypal"}, {"method": "sepa_debit", "settles": "days"}]}]})
    .to_string();
    Offer::parse(&text, &OfferContext::for_token_url("https://token.azlin.io")).unwrap()
}

/// Stripe's `method`, with the surfaces `surfaces` of them the app can show.
pub fn choice_with(method: Method, surfaces: &[SurfaceKind]) -> Choice {
    let pill = Pill {
        method,
        providers: vec![0],
        chosen: 0,
    };
    let ctx = PillContext {
        country: "DE",
        currency: "EUR",
        months: 12,
        recurring: false,
        surfaces,
    };
    Choice::of(&offer(), &pill, &ctx).expect("Stripe offers it")
}

pub fn choice(method: Method) -> Choice {
    choice_with(method, APP)
}

pub fn look() -> Look {
    Look {
        locale: "en".to_string(),
        look: "flora-light".to_string(),
    }
}

/// The token server's answer of a checkout `id` on `surface`.
pub fn answer(id: &str, method: Method, surface: Value) -> Value {
    json!({"checkout_id": id, "pay_url": format!("https://pay.azlin.io/legacy/{id}"),
           "provider": "stripe", "method": method.as_str(), "surface": surface})
}

pub fn fields_surface() -> Value {
    json!({"kind": "fields", "page": "https://pay.azlin.io/fields/stripe/v1",
           "publishable_key": "pk_test_SECRETPK", "client_secret": "pi_1_secret_SECRETCS"})
}

pub fn page_surface() -> Value {
    json!({"kind": "page", "url": "https://checkout.stripe.com/c/pay/cs_test_SECRETSESSION"})
}

pub fn browser_surface() -> Value {
    json!({"kind": "browser", "url": "https://checkout.stripe.com/c/pay/cs_test_SECRETSESSION"})
}

/// The `Created` event of `answer` for `choice`.
pub fn created(choice: &Choice, answer: &Value) -> Event {
    Event::Created(Box::new(Created::parse(answer, choice, &look()).expect("a good answer")))
}

/// Runs `events` from `state`: the last state and every effect, in order.
pub fn run(state: State, events: Vec<Event>) -> (State, Vec<Effect>) {
    let mut state = state;
    let mut all = Vec::new();
    for event in events {
        let (next, effects) = step(state, event);
        state = next;
        all.extend(effects);
    }
    (state, all)
}

/// A card checkout `ck_card` showing the popover with Stripe's fields.
pub fn presenting_card() -> State {
    let choice = choice(Method::Card);
    let (state, _) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: choice.clone(),
                consent: true,
                payer: Payer::default(),
            },
            created(&choice, &answer("ck_card", Method::Card, fields_surface())),
        ],
    );
    assert_eq!(state.name(), "presenting");
    state
}

/// A navigation of the web view to `url` (not a server redirect).
pub fn nav(url: &str) -> Event {
    Event::Navigation {
        url: url.to_string(),
        redirect: false,
    }
}

/// `presenting_card` with the fields ready and complete (the brand known).
pub fn complete_card() -> State {
    let (state, _) = run(
        presenting_card(),
        vec![
            nav("https://pay.azlin.io/fields/stripe/v1#pk=pk_test_SECRETPK&cs=pi_1_secret_SECRETCS"),
            nav("https://pay.azlin.io/_bridge/ready"),
            nav("https://pay.azlin.io/_bridge/brand?v=visa"),
            nav("https://pay.azlin.io/_bridge/complete?v=1"),
        ],
    );
    state
}

/// The token server's options for a payer in Poland, a 1 TB month: Stripe's BLIK, Przelewy24
/// and card in Poland's order of use, priced in zloty as the signed ladder froze it; the card
/// tells the card network the cardholder's name, typed on the provider's own fields.
pub fn options_pl() -> Value {
    json!({
        "country": "PL", "currency": "PLN",
        "offers": [{
            "provider": "stripe", "kind": "processor", "origins": ["checkout.stripe.com"],
            "methods": [
                {"method": "card", "provider_type": "card", "surfaces": ["fields", "page", "browser"],
                 "settles": "instant", "recurring": false, "flow": "fields", "handoff": "none",
                 "rank": 2, "expires_minutes": 0, "fee_level": "medium", "preferred": true,
                 "amount_cents": 449, "currency": "PLN",
                 "needs_identity": ["name"]},
                {"method": "blik", "provider_type": "blik", "surfaces": ["page", "browser"],
                 "settles": "instant", "recurring": false, "flow": "code", "handoff": "bank_app",
                 "rank": 0, "expires_minutes": 2, "fee_level": "low", "preferred": true,
                 "amount_cents": 449, "currency": "PLN"},
                {"method": "p24", "provider_type": "p24", "surfaces": ["browser"],
                 "settles": "days", "recurring": false, "flow": "redirect", "handoff": "bank_login",
                 "rank": 1, "expires_minutes": 0, "fee_level": "low", "preferred": true,
                 "amount_cents": 449, "currency": "PLN"}
            ]}],
        "price": {"amount_cents": 449, "currency": "PLN", "vat_rate_permille": 230,
                  "vat_cents": 84, "vat_included": true},
        "legal": {"withdrawal_consent": "I ask Azlin to start the service now."}
    })
}

/// The options for a payer in Germany: GoCardless's direct debit (the cheaper route of it) listed
/// before Stripe, whose PayPal, direct debit, card and Wero follow Germany's order of use - so
/// the pills' order is the ranks', not the providers'.
pub fn options_de() -> Value {
    json!({
        "country": "DE", "currency": "EUR",
        "offers": [
            {"provider": "gocardless", "kind": "processor", "default": true,
             "methods": [
                {"method": "sepa_debit", "surfaces": ["page", "browser"], "settles": "days",
                 "rank": 1, "preferred": true, "amount_cents": 499, "currency": "EUR",
                 "needs_identity": ["name", "email"]}]},
            {"provider": "stripe", "kind": "processor",
             "methods": [
                {"method": "paypal", "surfaces": ["browser"], "rank": 0, "preferred": true,
                 "amount_cents": 499, "currency": "EUR"},
                {"method": "sepa_debit", "surfaces": ["iban", "fields", "page", "browser"],
                 "settles": "days", "rank": 1, "preferred": false, "amount_cents": 499,
                 "currency": "EUR",
                 "needs_identity": ["name", "email"]},
                {"method": "card", "rank": 2, "preferred": true, "amount_cents": 499,
                 "currency": "EUR", "needs_identity": ["name"]},
                {"method": "wero", "surfaces": ["browser"], "rank": 4, "preferred": true,
                 "amount_cents": 499, "currency": "EUR"}]}
        ],
        "price": {"amount_cents": 499, "currency": "EUR", "vat_rate_permille": 190,
                  "vat_cents": 80, "vat_included": true}
    })
}

/// A live token server's offer of `value`.
pub fn live_offer(value: &Value) -> Offer {
    Offer::parse(&value.to_string(), &OfferContext::for_token_url("https://token.azlin.io"))
        .expect("an offer")
}
