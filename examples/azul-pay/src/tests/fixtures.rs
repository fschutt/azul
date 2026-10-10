//! What the tests of the surface, the machine and the redaction share: Stripe's offer of a card,
//! PayPal and SEPA, and the checkouts it makes.

use serde_json::{json, Value};

use crate::{
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
