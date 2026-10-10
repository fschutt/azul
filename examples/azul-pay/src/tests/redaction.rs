//! Nothing a checkout passes through is `Debug`ged with a secret: no client secret, no
//! publishable key, no session id in a path, no fragment, no cardholder name (CHECKOUT-PLAN
//! §3.4, the house rule azcloud-kit follows).

use super::fixtures::{
    answer, browser_surface, choice, complete_card, created, fields_surface, nav, run,
};
use crate::{
    machine::{step, Event, State},
    registry::Method,
};

const SECRETS: &[&str] = &["SECRETPK", "SECRETCS", "SECRETSESSION", "Felix", "flora-light"];

fn assert_clean(what: &str, text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{what} shows {secret}: {text}");
    }
}

#[test]
fn no_state_event_or_effect_of_a_card_payment_debugs_a_secret() {
    let card = choice(Method::Card);
    let events = vec![
        Event::Pay {
            choice: card.clone(),
            consent: true,
        },
        created(&card, &answer("ck_card", Method::Card, fields_surface())),
        nav("https://pay.azlin.io/fields/stripe/v1#pk=pk_test_SECRETPK&cs=pi_1_secret_SECRETCS"),
        nav("https://pay.azlin.io/_bridge/ready"),
        nav("https://pay.azlin.io/_bridge/complete?v=1"),
        Event::Confirm {
            name: "Felix Example".to_string(),
        },
        nav("https://pay.azlin.io/fields/stripe/v1#pk=pk_test_SECRETPK&cmd=confirm&name=Felix+Example"),
        nav("https://checkout.stripe.com/c/pay/cs_test_SECRETSESSION"),
        nav("https://pay.azlin.io/_bridge/result?v=succeeded"),
        Event::StopWaiting,
    ];
    let mut state = State::Choosing;
    for event in events {
        assert_clean("an event", &format!("{event:?}"));
        let (next, effects) = step(state, event);
        assert_clean("a state", &format!("{next:?}"));
        assert_clean("the effects", &format!("{effects:?}"));
        state = next;
    }
}

#[test]
fn no_state_or_effect_of_a_browser_payment_debugs_its_page() {
    let paypal = choice(Method::PayPal);
    let (state, effects) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: paypal.clone(),
                consent: true,
            },
            created(&paypal, &answer("ck_paypal", Method::PayPal, browser_surface())),
        ],
    );
    assert_clean("the state", &format!("{state:?}"));
    assert_clean("the effects", &format!("{effects:?}"));
    let (state, effects) = step(state, Event::OpenInBrowser);
    assert_clean("the state", &format!("{state:?}"));
    assert_clean("the effects", &format!("{effects:?}"));
    let completed = complete_card();
    assert_clean("a complete card", &format!("{completed:?}"));
}
