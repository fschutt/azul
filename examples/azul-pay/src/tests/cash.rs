//! Cash by post (cash contract v1): the provider that takes cash on paper - no web view, no
//! browser -, its pill, the checkout's slip and the machine's way through it.

use serde_json::{json, Value};

use super::fixtures::run;
use crate::{
    cash::{self, amount_in_words, CashSlip, ACTIVATION_PREFIX},
    machine::{step, Effect, Event, Notice, State},
    offer::{Offer, OfferContext, Settles},
    pills::{self, Choice, Pill, PillContext},
    registry::{self, Method, ProviderKind, SurfaceKind, Webview},
};

/// What AzDrive can show once it takes cash: the popover's fields, a page, the browser, paper.
const APP: &[SurfaceKind] = &[
    SurfaceKind::PopoverFields,
    SurfaceKind::WebviewPage,
    SurfaceKind::SystemBrowser,
    SurfaceKind::Paper,
];

/// The token server's test vector (SRV17): checkout `ck_aaaqeayeaudaocajbifqydiob4` for EUR 11.88
/// (the app checks its shape only).
const CODE: &str = "AZC1-AAAQ-EAYE-AUDA-OCAJ-BIFQ-YDIO-B4AA-ABFE-IVKV-F3QG-5NDF-5KZK-SMTY-I";
const CHECKOUT: &str = "ck_aaaqeayeaudaocajbifqydiob4";

fn offer(value: Value) -> Offer {
    Offer::parse(
        &value.to_string(),
        &OfferContext::for_token_url("https://token.azlin.io"),
    )
    .unwrap()
}

fn cash_offer() -> Offer {
    offer(json!({"offers": [
        {"provider": "stripe", "methods": [{"method": "card"}]},
        {"provider": "cash", "methods": [{"method": "cash", "surfaces": ["paper"]}]}]}))
}

fn ctx(months: u32, recurring: bool, surfaces: &'static [SurfaceKind]) -> PillContext<'static> {
    PillContext {
        country: "DE",
        currency: "EUR",
        months,
        recurring,
        surfaces,
    }
}

fn cash_choice() -> Choice {
    let offer = cash_offer();
    let shown = pills::pills(&offer, &ctx(12, false, APP));
    let pill: &Pill = shown
        .iter()
        .find(|p| p.method == Method::Cash)
        .expect("a cash pill");
    Choice::of(&offer, pill, &ctx(12, false, APP)).expect("cash can be shown")
}

fn answer() -> Value {
    json!({"checkout_id": CHECKOUT, "status": "awaiting_cash", "provider": "cash",
           "method": "cash", "amount_cents": 1188, "currency": "EUR", "activation_code": CODE,
           "mail_to": {"name": "Azlin Test Operator", "lines": ["Postfach 10 20 30",
                                                                "12345 Teststadt", "Germany"]},
           "expires_at": "2026-12-09T10:00:00Z"})
}

fn slip() -> CashSlip {
    CashSlip::parse(&answer(), &cash_choice()).expect("a good cash answer")
}

// ==== The provider and its pill ====

#[test]
fn the_cash_provider_takes_cash_on_paper_only_with_no_web_view_and_no_browser() {
    let spec = registry::provider("cash").expect("the registry knows cash");
    assert_eq!(spec.kind, ProviderKind::Processor, "Azlin sells; the post carries");
    assert!(!spec.fake);
    assert!(spec.fields.is_none(), "no fields page");
    assert!(spec.origins.is_empty(), "no provider pages at all");
    let cash = spec.method(Method::Cash).expect("its method");
    assert_eq!(cash.chain, &[SurfaceKind::Paper]);
    assert_eq!(cash.embed.webview, Webview::Forbidden);
    assert_eq!(cash.settles, Settles::Post);
    assert!(!SurfaceKind::Paper.in_webview());
    assert_eq!(Method::parse("cash"), Some(Method::Cash));
    assert_eq!(SurfaceKind::parse("paper"), Some(SurfaceKind::Paper));
    assert_eq!(Settles::Post.as_str(), "post");
}

#[test]
fn cash_by_post_is_a_pill_for_one_prepaid_payment_and_never_for_a_recurring_plan() {
    let offer = cash_offer();
    let methods = |ctx: &PillContext| -> Vec<Method> {
        pills::pills(&offer, ctx).iter().map(|p| p.method).collect()
    };
    assert!(methods(&ctx(1, false, APP)).contains(&Method::Cash), "a month paid at once");
    assert!(methods(&ctx(12, false, APP)).contains(&Method::Cash), "a year paid at once");
    assert!(!methods(&ctx(1, true, APP)).contains(&Method::Cash), "never a subscription");
    const NO_PAPER: &[SurfaceKind] = &[SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser];
    assert!(
        !methods(&ctx(12, false, NO_PAPER)).contains(&Method::Cash),
        "an app that prints nothing shows no cash pill"
    );
    let shown = pills::pills(&offer, &ctx(12, false, APP));
    assert_eq!(
        shown[pills::default_pill(&shown)].method,
        Method::Card,
        "cash is never the default"
    );
    assert_eq!(Method::Cash.label(), "Cash by post");
}

#[test]
fn the_server_cannot_widen_cash_to_a_web_view_or_the_browser() {
    let wide = offer(json!({"offers": [{"provider": "cash", "methods": [
        {"method": "cash", "surfaces": ["page", "browser", "paper"]}]}]}));
    let cash = wide.provider("cash").unwrap().method(Method::Cash).unwrap();
    assert_eq!(cash.surfaces, vec![SurfaceKind::Paper]);
    assert_eq!(cash.settles, Settles::Post);
    let browser_only = offer(json!({"offers": [{"provider": "cash", "methods": [
        {"method": "cash", "surfaces": ["browser"]}]}]}));
    assert!(browser_only.provider("cash").is_none(), "nothing of it is left to show");
    let card_by_post = offer(json!({"offers": [{"provider": "cash", "methods": [
        {"method": "card"}]}]}));
    assert!(card_by_post.provider("cash").is_none(), "the post takes no card");
}

// ==== The slip ====

#[test]
fn a_cash_checkout_answer_reads_into_its_slip() {
    let slip = slip();
    assert_eq!(slip.checkout_id, CHECKOUT);
    assert_eq!(slip.amount_cents, 1188);
    assert_eq!(slip.currency, "EUR");
    assert_eq!(slip.activation_code, CODE);
    assert!(slip.activation_code.starts_with(ACTIVATION_PREFIX));
    assert_eq!(slip.mail_to.name, "Azlin Test Operator");
    assert_eq!(slip.mail_to.lines.len(), 3);
    assert_eq!(slip.expires_at.as_deref(), Some("2026-12-09T10:00:00Z"));
    assert_eq!(slip.amount_text(), "EUR 11.88");
    assert_eq!(slip.amount_words(), "eleven euros and eighty-eight cents");
}

#[test]
fn a_cash_answer_without_its_code_its_address_or_its_amount_is_refused() {
    let choice = cash_choice();
    for key in ["checkout_id", "activation_code", "mail_to", "amount_cents", "currency"] {
        let mut broken = answer();
        broken.as_object_mut().unwrap().remove(key);
        assert!(CashSlip::parse(&broken, &choice).is_err(), "without {key}");
    }
    let mut no_lines = answer();
    no_lines["mail_to"] = json!({"name": "Azlin", "lines": []});
    assert!(CashSlip::parse(&no_lines, &choice).is_err(), "an address without lines");
    let mut card = answer();
    card["method"] = json!("card");
    assert!(CashSlip::parse(&card, &choice).is_err(), "an answer for another method");
    let mut other = answer();
    other["status"] = json!("approved");
    assert!(CashSlip::parse(&other, &choice).is_err(), "a new checkout awaits the cash");
}

#[test]
fn an_activation_code_of_another_shape_is_refused() {
    let choice = cash_choice();
    for code in [
        "",
        "AZC2-MNVV-6YLB-MFQW-CYLB-MFQW-CYLB-MFQW",
        "azc1-mnvv-6ylb-mfqw-cylb-mfqw-cylb-mfqw-cylb",
        "AZC1-MNVV6YLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB",
        "AZC1-MNV1-6YLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB",
        "AZC1-MNVV-6YLB",
        "AZC1-MNVV-6YLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-",
        // The id's ASCII instead of its 16 bytes: no token server writes it.
        "AZC1-MNVV-6YLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYLB-MFQW-CYIA-AAB5-4RKV-KI74-IMPG-\
         BG5O-LTW7-WE",
    ] {
        let mut broken = answer();
        broken["activation_code"] = json!(code);
        assert!(CashSlip::parse(&broken, &choice).is_err(), "{code:?}");
    }
}

#[test]
fn the_amount_is_written_in_words_for_the_slip() {
    assert_eq!(amount_in_words(990, "EUR"), "nine euros and ninety cents");
    assert_eq!(amount_in_words(4990, "EUR"), "forty-nine euros and ninety cents");
    assert_eq!(amount_in_words(100, "EUR"), "one euro");
    assert_eq!(amount_in_words(101, "EUR"), "one euro and one cent");
    assert_eq!(amount_in_words(2, "EUR"), "two cents");
    assert_eq!(
        amount_in_words(123_456, "EUR"),
        "one thousand two hundred thirty-four euros and fifty-six cents"
    );
    assert_eq!(amount_in_words(1_999_000, "EUR"), "nineteen thousand nine hundred ninety euros");
    assert_eq!(amount_in_words(2_500, "CHF"), "twenty-five Swiss francs");
    assert_eq!(amount_in_words(1_050, "SEK"), "ten SEK and fifty hundredths");
}

// ==== The machine ====

#[test]
fn buying_with_cash_makes_a_paper_checkout_and_opens_no_page() {
    let choice = cash_choice();
    let (state, effects) = step(
        State::Choosing,
        Event::Pay {
            choice: choice.clone(),
            consent: true,
        },
    );
    assert_eq!(state.name(), "preparing");
    assert_eq!(
        effects,
        vec![Effect::CreateCheckout {
            provider: "cash",
            method: Method::Cash,
            surface: SurfaceKind::Paper,
        }]
    );
    let (state, effects) = step(state, Event::Posted(Box::new(slip())));
    assert_eq!(state.name(), "posted");
    assert_eq!(state.checkout_id(), Some(CHECKOUT));
    assert!(!state.busy(), "the dialog waits for no letter");
    assert!(effects.contains(&Effect::ShowPaper(Box::new(slip()))));
    assert!(effects.contains(&Effect::Notice(Notice::WaitingForLetter)));
    assert!(
        effects.contains(&Effect::StopPoll),
        "the background claims look once a day, the dialog does not poll"
    );
    for effect in &effects {
        assert!(
            !matches!(
                effect,
                Effect::ShowSurface(_)
                    | Effect::OpenBrowser(_)
                    | Effect::StartPoll { .. }
                    | Effect::Abandon { .. }
            ),
            "{effect:?}"
        );
    }
}

#[test]
fn closing_the_dialog_after_the_slip_keeps_the_cash_checkout() {
    let (state, effects) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: cash_choice(),
                consent: true,
            },
            Event::Posted(Box::new(slip())),
            Event::Close,
        ],
    );
    assert_eq!(state, State::Choosing);
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::Abandon { .. })),
        "the letter may be on its way: {effects:?}"
    );
}

#[test]
fn a_posted_checkout_becomes_the_drive_when_the_money_arrived_or_says_why_not() {
    let posted = || {
        run(
            State::Choosing,
            vec![
                Event::Pay {
                    choice: cash_choice(),
                    consent: true,
                },
                Event::Posted(Box::new(slip())),
            ],
        )
        .0
    };
    let (done, _) = step(posted(), Event::Approved);
    assert_eq!(done.name(), "done");
    let why = String::from("the envelope held EUR 5.00, not EUR 9.90");
    let (declined, effects) = step(posted(), Event::Declined(why.clone()));
    assert_eq!(declined.name(), "declined");
    assert!(effects.contains(&Effect::Notice(Notice::Declined(why))));
    let (again, _) = step(
        posted(),
        Event::Navigation {
            url: String::from("https://example.com/"),
            redirect: false,
        },
    );
    assert_eq!(again.name(), "posted", "no page belongs to a paper checkout");
}

#[test]
fn a_slip_for_a_dialog_that_moved_on_is_kept_not_abandoned() {
    let (state, effects) = step(State::Choosing, Event::Posted(Box::new(slip())));
    assert_eq!(state, State::Choosing);
    assert!(effects.is_empty(), "{effects:?}");
}

#[test]
fn the_waiting_notice_says_postal_cash_takes_a_while_and_azdrive_looks_daily() {
    let text = Notice::WaitingForLetter.text();
    assert!(text.starts_with("Waiting for your letter"), "{text}");
    assert!(text.contains("postal cash takes a while"), "{text}");
    assert!(text.contains("once a day"), "{text}");
}

#[test]
fn a_token_server_whose_tiers_list_cash_offers_it_without_payment_options() {
    // The token server says it takes cash by post in GET /v1/tiers (its methods): the app needs
    // no GET /v1/checkout/options for it - the registry's provider makes the choice.
    let choice = cash::choice(&ctx(12, false, APP)).expect("cash can be shown");
    assert_eq!(choice.method.method, Method::Cash);
    assert_eq!(choice.method.surfaces, vec![SurfaceKind::Paper]);
    assert_eq!(choice.provider.spec.id, "cash");
    assert_eq!(choice.method.settles, Settles::Post);
    assert!(cash::choice(&ctx(1, true, APP)).is_none(), "never a subscription");
    const NO_PAPER: &[SurfaceKind] = &[SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser];
    assert!(cash::choice(&ctx(12, false, NO_PAPER)).is_none(), "an app that prints nothing");
}
