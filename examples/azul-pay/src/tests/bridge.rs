//! The navigation bridge (CHECKOUT-PLAN §3.5): what a fields page tells the app by navigating
//! to `<pages>/_bridge/<message>?...`, and what the app tells the page with a new fragment.

use crate::{
    bridge::{command_url, parse_bridge, BridgeError, BridgeMessage, CardBrand, Outcome, PageCommand, MAX_QUERY},
    surface::SecretUrl,
    url::WebUrl,
};

fn bridge(path_and_query: &str) -> Result<BridgeMessage, BridgeError> {
    parse_bridge(&WebUrl::parse(&format!("https://pay.azlin.io{path_and_query}")).unwrap())
}

#[test]
fn every_bridge_message_reads_back() {
    assert_eq!(bridge("/_bridge/ready"), Ok(BridgeMessage::Ready));
    assert_eq!(bridge("/_bridge/height?v=420"), Ok(BridgeMessage::Height(420)));
    assert_eq!(bridge("/_bridge/brand?v=visa"), Ok(BridgeMessage::Brand(CardBrand::Visa)));
    assert_eq!(
        bridge("/_bridge/brand?v=mastercard"),
        Ok(BridgeMessage::Brand(CardBrand::Mastercard))
    );
    assert_eq!(bridge("/_bridge/brand?v=unknown"), Ok(BridgeMessage::Brand(CardBrand::Unknown)));
    assert_eq!(bridge("/_bridge/complete?v=1"), Ok(BridgeMessage::Complete(true)));
    assert_eq!(bridge("/_bridge/complete?v=0"), Ok(BridgeMessage::Complete(false)));
    assert_eq!(bridge("/_bridge/last4?v=4242"), Ok(BridgeMessage::Last4("4242".to_string())));
    assert_eq!(
        bridge("/_bridge/error?code=card_declined&message=Your+card+was+declined."),
        Ok(BridgeMessage::Error {
            code: "card_declined".to_string(),
            message: "Your card was declined.".to_string(),
        })
    );
    assert_eq!(
        bridge("/_bridge/result?v=succeeded"),
        Ok(BridgeMessage::Result {
            outcome: Outcome::Succeeded,
            code: String::new(),
        })
    );
    assert_eq!(
        bridge("/_bridge/result?v=failed&code=card_declined"),
        Ok(BridgeMessage::Result {
            outcome: Outcome::Failed,
            code: "card_declined".to_string(),
        })
    );
    for (v, outcome) in [
        ("processing", Outcome::Processing),
        ("requires_action", Outcome::RequiresAction),
    ] {
        assert_eq!(
            bridge(&format!("/_bridge/result?v={v}")),
            Ok(BridgeMessage::Result {
                outcome,
                code: String::new(),
            })
        );
    }
    // The fragment is never read: only the page's own navigation carries a message.
    assert_eq!(bridge("/_bridge/ready#v=1"), Ok(BridgeMessage::Ready));
}

#[test]
fn a_path_that_is_no_bridge_message_is_refused() {
    assert_eq!(bridge("/fields/stripe/v1"), Err(BridgeError::NotBridge));
    assert_eq!(bridge("/_bridgeready"), Err(BridgeError::NotBridge));
    assert_eq!(bridge("/_bridge/"), Err(BridgeError::Unknown(String::new())));
    assert_eq!(bridge("/_bridge/launch"), Err(BridgeError::Unknown("launch".to_string())));
    assert_eq!(bridge("/_bridge/ready/again"), Err(BridgeError::Unknown("ready/again".to_string())));
}

#[test]
fn junk_arguments_are_refused() {
    for path in [
        "/_bridge/ready?v=1",
        "/_bridge/height?v=abc",
        "/_bridge/height?v=0",
        "/_bridge/height?v=99999",
        "/_bridge/height",
        "/_bridge/brand?v=visa%3Cscript%3E",
        "/_bridge/brand?v=Visa",
        "/_bridge/complete?v=yes",
        "/_bridge/last4?v=42a2",
        "/_bridge/last4?v=12345",
        "/_bridge/error?code=Card+Declined",
        "/_bridge/error?code=",
        "/_bridge/error?code=x&message=line%0Abreak",
        "/_bridge/result?v=maybe",
        "/_bridge/result?v=failed&code=NO%20WAY",
        "/_bridge/complete?v=1&extra=2",
    ] {
        assert!(
            matches!(bridge(path), Err(BridgeError::Junk(_) | BridgeError::Missing(_))),
            "{path}: {:?}",
            bridge(path)
        );
    }
}

#[test]
fn a_repeated_argument_is_refused() {
    assert_eq!(
        bridge("/_bridge/complete?v=1&v=0"),
        Err(BridgeError::Repeated("v".to_string()))
    );
    assert_eq!(
        bridge("/_bridge/result?v=succeeded&code=a&code=b"),
        Err(BridgeError::Repeated("code".to_string()))
    );
}

#[test]
fn an_oversized_query_is_refused() {
    let long = "a".repeat(MAX_QUERY);
    assert_eq!(
        bridge(&format!("/_bridge/error?code=x&message={long}")),
        Err(BridgeError::Oversized)
    );
    let message = "b".repeat(300);
    assert!(
        matches!(bridge(&format!("/_bridge/error?code=x&message={message}")), Err(BridgeError::Junk(_))),
        "a provider message is 200 characters at most"
    );
}

fn fields_page() -> SecretUrl {
    SecretUrl::new(
        WebUrl::parse("https://pay.azlin.io/fields/stripe/v1#pk=pk_test_1&cs=pi_1_secret_2&locale=en")
            .unwrap(),
    )
}

#[test]
fn the_confirm_command_keeps_the_pages_inputs_and_adds_the_name_in_the_fragment() {
    let page = fields_page();
    let first = command_url(
        &page,
        &PageCommand::Confirm {
            name: "Felix Ex\u{e4}mple & Co".to_string(),
        },
        1,
    );
    assert!(first.url().same_document(page.url()), "a fragment navigation: nothing reloads");
    assert_eq!(
        first.url().fragment(),
        "pk=pk_test_1&cs=pi_1_secret_2&locale=en&cmd=confirm&name=Felix+Ex%C3%A4mple+%26+Co&n=1"
    );
    let second = command_url(
        &page,
        &PageCommand::Confirm {
            name: "Felix Example".to_string(),
        },
        2,
    );
    assert_ne!(first, second, "two confirms are two fragments: each fires hashchange");
    assert!(second.url().fragment().ends_with("&n=2"));
    assert!(command_url(&page, &PageCommand::Reset, 3)
        .url()
        .fragment()
        .ends_with("&cmd=reset&n=3"));
    assert!(command_url(&page, &PageCommand::Look("flora-dark".to_string()), 4)
        .url()
        .fragment()
        .ends_with("&cmd=look&look=flora-dark&n=4"));
}

#[test]
fn a_cardholder_name_loses_its_control_characters_and_its_excess() {
    let page = fields_page();
    let cleaned = command_url(
        &page,
        &PageCommand::Confirm {
            name: " Fe\nlix\u{7}  Example ".to_string(),
        },
        1,
    );
    assert!(cleaned.url().fragment().contains("&name=Felix+Example&"), "{:?}", cleaned.url().fragment());
    let long = command_url(
        &page,
        &PageCommand::Confirm {
            name: "x".repeat(500),
        },
        1,
    );
    assert!(long.url().fragment().contains(&format!("&name={}&", "x".repeat(100))));
    assert!(!long.url().fragment().contains(&"x".repeat(101)));
}
