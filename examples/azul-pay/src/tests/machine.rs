//! The checkout's state machine (CHECKOUT-PLAN §3.4, §3.6, §3.7, §2.4): every row of the
//! state table, every fallback, and the cases between them.

use super::fixtures::{
    answer, browser_surface, choice, choice_with, complete_card, created, look, nav,
    page_surface, presenting_card, run, APP,
};
use crate::{
    bridge::CardBrand,
    machine::{step, ChipPage, Effect, Event, Notice, State, WaitReason},
    registry::{Method, SurfaceKind},
    surface::Surface,
};

fn cancelled(effects: &[Effect]) -> bool {
    effects.contains(&Effect::CancelNavigation)
}

fn allowed(effects: &[Effect]) -> bool {
    effects.contains(&Effect::AllowNavigation)
}

fn notices(effects: &[Effect]) -> Vec<&Notice> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Notice(n) => Some(n),
            _ => None,
        })
        .collect()
}

fn opened(effects: &[Effect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::OpenBrowser(url) => Some(url.reveal()),
            _ => None,
        })
        .collect()
}

fn switch_to(state: State, kind: SurfaceKind) -> (State, Vec<Effect>) {
    let surface = match kind {
        SurfaceKind::WebviewPage => page_surface(),
        _ => browser_surface(),
    };
    let surface = Surface::parse(&surface, &choice(Method::Card), &look()).unwrap();
    step(state, Event::Switched(surface))
}

// ==== Choosing -> Preparing ====

#[test]
fn pay_without_the_consent_stays_choosing_and_says_why() {
    let (state, effects) = step(
        State::Choosing,
        Event::Pay {
            choice: choice(Method::Card),
            consent: false,
        },
    );
    assert_eq!(state, State::Choosing);
    assert_eq!(effects, vec![Effect::Notice(Notice::ConsentRequired)]);
}

#[test]
fn pay_with_the_consent_prepares_the_first_surface_the_app_can_show() {
    let (state, effects) = step(
        State::Choosing,
        Event::Pay {
            choice: choice(Method::Card),
            consent: true,
        },
    );
    assert_eq!(state.name(), "preparing");
    assert_eq!(
        effects,
        vec![Effect::CreateCheckout {
            provider: "stripe",
            method: Method::Card,
            surface: SurfaceKind::PopoverFields,
        }]
    );
    let page_only = choice_with(Method::Card, &[SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser]);
    let (_, effects) = step(
        State::Choosing,
        Event::Pay {
            choice: page_only,
            consent: true,
        },
    );
    assert_eq!(
        effects,
        vec![Effect::CreateCheckout {
            provider: "stripe",
            method: Method::Card,
            surface: SurfaceKind::WebviewPage,
        }]
    );
}

#[test]
fn a_second_order_while_one_is_open_does_nothing() {
    let pay = || Event::Pay {
        choice: choice(Method::Card),
        consent: true,
    };
    let (preparing, _) = step(State::Choosing, pay());
    let (again, effects) = step(preparing.clone(), pay());
    assert_eq!(again, preparing);
    assert!(effects.is_empty(), "{effects:?}");
    let (still, effects) = step(presenting_card(), pay());
    assert_eq!(still.name(), "presenting");
    assert!(effects.is_empty(), "one checkout per popover");
}

#[test]
fn a_failed_create_goes_back_to_the_pills_with_the_reason() {
    let (state, effects) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: choice(Method::Card),
                consent: true,
            },
            Event::CreateFailed("provider_unavailable".to_string()),
        ],
    );
    assert_eq!(state, State::Choosing);
    assert!(notices(&effects)
        .contains(&&Notice::CreateFailed("provider_unavailable".to_string())));
}

// ==== Preparing -> Presenting / Waiting ====

#[test]
fn a_created_fields_checkout_shows_the_popover() {
    let choice = choice(Method::Card);
    let (state, effects) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: choice.clone(),
                consent: true,
            },
            created(&choice, &answer("ck_card", Method::Card, super::fixtures::fields_surface())),
        ],
    );
    assert_eq!(state.name(), "presenting");
    assert_eq!(state.checkout_id(), Some("ck_card"));
    let shown: Vec<SurfaceKind> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::ShowSurface(s) => Some(s.kind),
            _ => None,
        })
        .collect();
    assert_eq!(shown, vec![SurfaceKind::PopoverFields]);
    let chip = state.chip().unwrap();
    assert_eq!(chip.host, "pay.azlin.io");
    assert_eq!(chip.what, "card fields by Stripe");
    // What the words say, for an app that says them in its own language.
    assert_eq!(chip.page, ChipPage::CardFields);
    assert_eq!(chip.provider, "Stripe");
    assert_eq!(chip.legal_name, "Stripe Payments Europe, Limited");
    assert!(state.busy());
}

#[test]
fn a_created_browser_checkout_opens_the_browser_and_waits() {
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
    assert!(matches!(
        &state,
        State::Waiting {
            reason: WaitReason::Browser,
            ..
        }
    ));
    assert_eq!(
        opened(&effects),
        vec!["https://checkout.stripe.com/c/pay/cs_test_SECRETSESSION".to_string()]
    );
    assert!(effects.contains(&Effect::StartPoll {
        checkout_id: "ck_paypal".to_string()
    }));
    assert!(notices(&effects).contains(&&Notice::BrowserOpened {
        host: "checkout.stripe.com".to_string()
    }));
}

// ==== Presenting: the page, the bridge, the policy ====

#[test]
fn the_fields_page_and_its_own_command_navigations_are_allowed() {
    let (state, effects) = step(
        presenting_card(),
        nav("https://pay.azlin.io/fields/stripe/v1#pk=pk_test_SECRETPK&cs=pi_1_secret_SECRETCS"),
    );
    assert!(allowed(&effects) && !cancelled(&effects));
    assert_eq!(state.chip().unwrap().host, "pay.azlin.io");
}

#[test]
fn bridge_messages_are_cancelled_and_followed() {
    let mut state = presenting_card();
    for url in [
        "https://pay.azlin.io/_bridge/ready",
        "https://pay.azlin.io/_bridge/height?v=420",
        "https://pay.azlin.io/_bridge/brand?v=visa",
        "https://pay.azlin.io/_bridge/complete?v=1",
        "https://pay.azlin.io/_bridge/last4?v=4242",
        "https://pay.azlin.io/_bridge/nonsense?v=1",
    ] {
        let (next, effects) = step(state, nav(url));
        assert!(cancelled(&effects) && !allowed(&effects), "{url}");
        state = next;
    }
    let (_, page) = state.presenting().unwrap();
    assert!(page.ready && page.complete);
    assert_eq!(page.brand, Some(CardBrand::Visa));
    assert_eq!(page.last4.as_deref(), Some("4242"));
    assert_eq!(page.height, Some(420));
    assert_eq!(state.chip().unwrap().host, "pay.azlin.io", "a bridge message is no page");
}

#[test]
fn pay_in_the_popover_needs_complete_fields_then_sends_the_confirm_with_the_name() {
    let (state, effects) = step(
        presenting_card(),
        Event::Confirm {
            name: "Felix Example".to_string(),
        },
    );
    assert_eq!(state.name(), "presenting");
    assert_eq!(effects, vec![Effect::Notice(Notice::FieldsIncomplete)]);
    let (state, effects) = step(
        complete_card(),
        Event::Confirm {
            name: "Felix Example".to_string(),
        },
    );
    assert_eq!(state.name(), "confirming");
    let sent: Vec<String> = effects
        .iter()
        .filter_map(|e| match e {
            Effect::WebviewNavigate(url) => Some(url.reveal()),
            _ => None,
        })
        .collect();
    assert_eq!(sent.len(), 1, "{effects:?}");
    assert!(sent[0].starts_with("https://pay.azlin.io/fields/stripe/v1#pk=pk_test_SECRETPK&"));
    assert!(sent[0].ends_with("&cmd=confirm&name=Felix+Example&n=1"), "{}", sent[0]);
    // The app's own command is asked about like any navigation: it is the fields page.
    let (_, effects) = step(state, nav(&sent[0]));
    assert!(allowed(&effects));
}

#[test]
fn a_result_after_the_confirm_waits_for_the_drive() {
    for outcome in ["succeeded", "processing"] {
        let (confirming, _) = step(
            complete_card(),
            Event::Confirm {
                name: "Felix Example".to_string(),
            },
        );
        let (state, effects) = step(
            confirming,
            nav(&format!("https://pay.azlin.io/_bridge/result?v={outcome}")),
        );
        assert!(
            matches!(
                &state,
                State::Waiting {
                    reason: WaitReason::Confirmed,
                    ..
                }
            ),
            "{outcome}: {state:?}"
        );
        assert!(cancelled(&effects));
        assert!(effects.contains(&Effect::StartPoll {
            checkout_id: "ck_card".to_string()
        }));
    }
    let (confirming, _) = step(
        complete_card(),
        Event::Confirm {
            name: "F".to_string(),
        },
    );
    let (state, _) = step(confirming, nav("https://pay.azlin.io/_bridge/result?v=requires_action"));
    assert_eq!(state.name(), "confirming", "3-D Secure shows inside the fields");
}

#[test]
fn a_bridge_result_without_a_prior_confirm_changes_nothing() {
    let before = complete_card();
    let (state, effects) = step(before.clone(), nav("https://pay.azlin.io/_bridge/result?v=succeeded"));
    assert_eq!(state, before);
    assert_eq!(effects, vec![Effect::CancelNavigation]);
}

#[test]
fn a_declined_card_stays_in_the_popover_and_pay_works_again() {
    let confirm = || Event::Confirm {
        name: "Felix Example".to_string(),
    };
    let (state, effects) = run(
        complete_card(),
        vec![
            confirm(),
            nav("https://pay.azlin.io/_bridge/error?code=card_declined&message=Your+card+was+declined."),
            nav("https://pay.azlin.io/_bridge/result?v=failed&code=card_declined"),
        ],
    );
    assert_eq!(state.name(), "presenting");
    assert!(notices(&effects)
        .iter()
        .any(|n| matches!(n, Notice::ProviderError(text) if text == "Your card was declined.")));
    assert!(!effects.iter().any(|e| matches!(e, Effect::CreateCheckout { .. } | Effect::Abandon { .. })));
    let (state, effects) = step(state, confirm());
    assert_eq!(state.name(), "confirming");
    assert!(effects.iter().any(
        |e| matches!(e, Effect::WebviewNavigate(url) if url.reveal().ends_with("&n=2"))
    ));
}

#[test]
fn three_declines_suggest_another_method() {
    let mut state = complete_card();
    let mut all = Vec::new();
    for _ in 0..3 {
        let (next, effects) = run(
            state,
            vec![
                Event::Confirm {
                    name: "F".to_string(),
                },
                nav("https://pay.azlin.io/_bridge/result?v=failed&code=card_declined"),
            ],
        );
        state = next;
        all.push(notices(&effects).contains(&&Notice::TryAnotherMethod));
    }
    assert_eq!(all, vec![false, false, true]);
}

#[test]
fn a_navigation_off_the_providers_origins_is_blocked_and_the_chip_stays() {
    let (state, effects) = step(complete_card(), nav("https://evil.example/login"));
    assert!(cancelled(&effects));
    assert_eq!(
        notices(&effects),
        vec![&Notice::Blocked {
            host: "evil.example".to_string()
        }]
    );
    assert_eq!(state.chip().unwrap().host, "pay.azlin.io");
    assert_eq!(state.name(), "presenting");
}

#[test]
fn the_classics_of_phishing_are_blocked() {
    for url in [
        "https://checkout.stripe.com@evil.example/",
        "https://checkout.stripe.com.evil.example/",
        "https://evilstripe.com/",
        "http://checkout.stripe.com/c/pay/cs_1",
        "https://checkout.stripe.com:8443/",
        "https://CHECKOUT.STRIPE.COM./",
        "https://checkout%2Estripe.com/",
        "javascript:alert(1)",
        "data:text/html,<h1>Pay</h1>",
        "file:///etc/passwd",
        "blob:https://pay.azlin.io/1",
        "https://pay.azlin.io/somewhere-else",
        "http://127.0.0.1:8081/fields/stripe/v1",
    ] {
        let (_, effects) = step(complete_card(), nav(url));
        assert!(cancelled(&effects) && !allowed(&effects), "{url}");
        assert!(
            notices(&effects)
                .iter()
                .any(|n| matches!(n, Notice::Blocked { .. })),
            "{url}"
        );
    }
    let (state, effects) = step(complete_card(), nav("https://checkout.stripe.com/c/pay/cs_1"));
    assert!(allowed(&effects), "the provider's own page");
    assert_eq!(state.chip().unwrap().host, "checkout.stripe.com", "the chip follows");
}

#[test]
fn a_provider_login_goes_to_the_system_browser() {
    let (state, effects) = step(
        complete_card(),
        Event::Navigation {
            url: "https://www.paypal.com/checkoutnow?token=EC-1".to_string(),
            redirect: true,
        },
    );
    assert!(cancelled(&effects));
    assert_eq!(opened(&effects), vec!["https://www.paypal.com/checkoutnow?token=EC-1".to_string()]);
    assert!(matches!(
        &state,
        State::Waiting {
            reason: WaitReason::Browser,
            ..
        }
    ));
    assert!(effects.contains(&Effect::StartPoll {
        checkout_id: "ck_card".to_string()
    }));
}

// ==== Return pages ====

#[test]
fn a_success_return_waits_and_a_second_one_is_only_cancelled() {
    let (state, effects) = step(complete_card(), nav("https://pay.azlin.io/return/ok"));
    assert!(matches!(
        &state,
        State::Waiting {
            reason: WaitReason::Returned,
            ..
        }
    ));
    assert!(cancelled(&effects));
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, Effect::StartPoll { .. }))
            .count(),
        1
    );
    let (again, effects) = step(state.clone(), nav("https://pay.azlin.io/return/ok"));
    assert_eq!(again, state);
    assert_eq!(effects, vec![Effect::CancelNavigation]);
    let (_, pending) = step(complete_card(), nav("https://pay.azlin.io/return/pending?x=1"));
    assert!(pending.iter().any(|e| matches!(e, Effect::StartPoll { .. })));
}

#[test]
fn a_cancel_return_abandons_the_checkout_and_goes_back_to_the_pills() {
    let (state, effects) = step(complete_card(), nav("https://pay.azlin.io/return/cancel"));
    assert_eq!(state, State::Choosing);
    assert!(cancelled(&effects));
    assert!(effects.contains(&Effect::Abandon {
        checkout_id: "ck_card".to_string()
    }));
    assert!(notices(&effects).contains(&&Notice::Cancelled));
}

#[test]
fn a_cancel_after_a_success_abandons_nothing() {
    let (waiting, _) = step(complete_card(), nav("https://pay.azlin.io/return/ok"));
    let (state, effects) = step(waiting, nav("https://pay.azlin.io/return/cancel"));
    assert_eq!(state.name(), "waiting");
    assert_eq!(effects, vec![Effect::CancelNavigation]);
}

// ==== The fallback chain ====

#[test]
fn a_load_failure_falls_back_to_the_hosted_page_and_then_the_browser() {
    let (state, effects) = step(
        presenting_card(),
        Event::LoadFailed {
            reason: "this platform has no web view backend yet".to_string(),
        },
    );
    assert!(effects.contains(&Effect::SwitchSurface {
        checkout_id: "ck_card".to_string(),
        kind: SurfaceKind::WebviewPage,
    }));
    let (state, effects) = switch_to(state, SurfaceKind::WebviewPage);
    assert_eq!(state.name(), "presenting");
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::ShowSurface(s) if s.kind == SurfaceKind::WebviewPage)));
    assert_eq!(state.chip().unwrap().what, "payment page of Stripe");
    assert_eq!(state.chip().unwrap().page, ChipPage::Page);
    let (state, effects) = step(
        state,
        Event::LoadFailed {
            reason: "offline".to_string(),
        },
    );
    assert!(effects.contains(&Effect::SwitchSurface {
        checkout_id: "ck_card".to_string(),
        kind: SurfaceKind::SystemBrowser,
    }));
    let (state, effects) = switch_to(state, SurfaceKind::SystemBrowser);
    assert!(matches!(
        &state,
        State::Waiting {
            reason: WaitReason::Browser,
            ..
        }
    ));
    assert_eq!(opened(&effects).len(), 1);
    assert_eq!(state.checkout_id(), Some("ck_card"), "the same checkout all along");
}

#[test]
fn a_second_load_failure_while_switching_asks_once() {
    let (state, _) = step(
        presenting_card(),
        Event::LoadFailed {
            reason: "x".to_string(),
        },
    );
    let (_, effects) = step(
        state,
        Event::LoadFailed {
            reason: "x".to_string(),
        },
    );
    assert!(!effects.iter().any(|e| matches!(e, Effect::SwitchSurface { .. })));
}

#[test]
fn a_load_failure_on_the_last_surface_gives_up_and_offers_another_method() {
    let fields_only = choice_with(Method::Card, &[SurfaceKind::PopoverFields]);
    let (state, _) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: fields_only.clone(),
                consent: true,
            },
            created(&fields_only, &answer("ck_1", Method::Card, super::fixtures::fields_surface())),
        ],
    );
    let (state, effects) = step(
        state,
        Event::LoadFailed {
            reason: "x".to_string(),
        },
    );
    assert_eq!(state, State::Choosing);
    assert!(effects.contains(&Effect::Abandon {
        checkout_id: "ck_1".to_string()
    }));
    assert!(notices(&effects).contains(&&Notice::ProviderUnavailable {
        provider: "Stripe".to_string()
    }));
}

#[test]
fn a_failed_switch_tries_the_next_surface_then_gives_up() {
    let (state, _) = step(
        presenting_card(),
        Event::LoadFailed {
            reason: "x".to_string(),
        },
    );
    let (state, effects) = step(state, Event::SwitchFailed("surface_unavailable".to_string()));
    assert!(effects.contains(&Effect::SwitchSurface {
        checkout_id: "ck_card".to_string(),
        kind: SurfaceKind::SystemBrowser,
    }));
    let (state, effects) = step(state, Event::SwitchFailed("surface_unavailable".to_string()));
    assert_eq!(state, State::Choosing);
    assert!(effects.contains(&Effect::Abandon {
        checkout_id: "ck_card".to_string()
    }));
}

#[test]
fn open_in_browser_switches_the_same_checkout_to_the_browser() {
    let (state, effects) = step(complete_card(), Event::OpenInBrowser);
    assert_eq!(
        effects,
        vec![Effect::SwitchSurface {
            checkout_id: "ck_card".to_string(),
            kind: SurfaceKind::SystemBrowser,
        }]
    );
    let (state, effects) = switch_to(state, SurfaceKind::SystemBrowser);
    assert!(matches!(&state, State::Waiting { .. }));
    assert_eq!(opened(&effects).len(), 1);
    assert!(!effects.iter().any(|e| matches!(e, Effect::CreateCheckout { .. })));
    // "Open the page again" while waiting for the browser.
    let (_, effects) = step(state, Event::OpenInBrowser);
    assert_eq!(opened(&effects).len(), 1);
}

#[test]
fn a_refused_browser_switch_keeps_the_popover() {
    let (state, _) = step(complete_card(), Event::OpenInBrowser);
    let (state, effects) = step(state, Event::SwitchFailed("no".to_string()));
    assert_eq!(state.name(), "presenting");
    assert!(notices(&effects)
        .iter()
        .any(|n| matches!(n, Notice::SurfaceRefused(_))));
    assert!(!effects.iter().any(|e| matches!(e, Effect::Abandon { .. })));
}

// ==== Closing, waiting, stopping ====

#[test]
fn closing_the_popover_before_paying_abandons_the_checkout() {
    let (state, effects) = step(complete_card(), Event::Close);
    assert_eq!(state, State::Choosing);
    assert_eq!(
        effects,
        vec![Effect::Abandon {
            checkout_id: "ck_card".to_string()
        }]
    );
}

#[test]
fn closing_while_confirming_keeps_the_claim() {
    let (confirming, _) = step(
        complete_card(),
        Event::Confirm {
            name: "F".to_string(),
        },
    );
    let (state, effects) = step(confirming, Event::Close);
    assert_eq!(
        state,
        State::Stopped {
            checkout_id: "ck_card".to_string()
        }
    );
    assert!(effects.contains(&Effect::StopPoll));
    assert!(!effects.iter().any(|e| matches!(e, Effect::Abandon { .. })));
}

#[test]
fn stop_waiting_keeps_the_claim_and_check_again_waits_again() {
    let (waiting, _) = step(complete_card(), nav("https://pay.azlin.io/return/ok"));
    let (stopped, effects) = step(waiting, Event::StopWaiting);
    assert_eq!(
        stopped,
        State::Stopped {
            checkout_id: "ck_card".to_string()
        }
    );
    assert!(effects.contains(&Effect::StopPoll));
    assert!(notices(&effects).contains(&&Notice::StoppedWaiting));
    assert!(!stopped.busy());
    let (again, effects) = step(stopped, Event::CheckAgain);
    assert_eq!(again.name(), "waiting");
    assert_eq!(
        effects,
        vec![Effect::StartPoll {
            checkout_id: "ck_card".to_string()
        }]
    );
}

#[test]
fn approved_is_done_and_declined_says_why() {
    let (waiting, _) = step(complete_card(), nav("https://pay.azlin.io/return/ok"));
    let (done, _) = step(waiting.clone(), Event::Approved);
    assert_eq!(
        done,
        State::Done {
            checkout_id: "ck_card".to_string()
        }
    );
    let (declined, effects) = step(waiting, Event::Declined("card_declined".to_string()));
    assert_eq!(declined.name(), "declined");
    assert!(notices(&effects).contains(&&Notice::Declined("card_declined".to_string())));
    let (stopped, _) = step(
        State::Stopped {
            checkout_id: "ck_x".to_string(),
        },
        Event::Approved,
    );
    assert_eq!(stopped.name(), "done");
    // A new order after a declined one.
    let (again, effects) = step(
        declined,
        Event::Pay {
            choice: choice(Method::Card),
            consent: true,
        },
    );
    assert_eq!(again.name(), "preparing");
    assert_eq!(effects.len(), 1);
}

#[test]
fn a_checkout_that_arrives_after_the_dialog_closed_is_abandoned() {
    let card = choice(Method::Card);
    let (state, effects) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: card.clone(),
                consent: true,
            },
            Event::Close,
            created(&card, &answer("ck_late", Method::Card, super::fixtures::fields_surface())),
        ],
    );
    assert_eq!(state, State::Choosing);
    assert_eq!(
        effects.last(),
        Some(&Effect::Abandon {
            checkout_id: "ck_late".to_string()
        })
    );
}

#[test]
fn a_method_that_settles_in_days_says_so_while_waiting() {
    let sepa = choice(Method::SepaDebit);
    let (state, effects) = run(
        State::Choosing,
        vec![
            Event::Pay {
                choice: sepa.clone(),
                consent: true,
            },
            created(&sepa, &answer("ck_sepa", Method::SepaDebit, browser_surface())),
        ],
    );
    assert_eq!(state.name(), "waiting");
    assert!(notices(&effects).contains(&&Notice::SettlesInDays));
    let (stopped, effects) = step(state, Event::PendingForDays);
    assert_eq!(stopped.name(), "stopped");
    assert!(effects.contains(&Effect::StopPoll));
}

#[test]
fn a_navigation_where_no_page_should_be_is_cancelled() {
    for state in [
        State::Choosing,
        State::Stopped {
            checkout_id: "ck".to_string(),
        },
    ] {
        let (_, effects) = step(state, nav("https://pay.azlin.io/fields/stripe/v1"));
        assert_eq!(effects, vec![Effect::CancelNavigation]);
    }
    let _ = APP;
}
