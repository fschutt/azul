//! What a `<webview>` keeps, what it reports, and what a sign-in callback
//! reads off the URL it is asked about.

use azul_css::{AzString, OptionString};

use super::*;
use crate::{
    dom::{DomId, DomNodeId, NodeId},
    events::{EventPhase, EventSource, EventType},
    styled_dom::NodeHierarchyItemId,
};

fn node(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

#[test]
fn a_web_view_keeps_its_cookies_in_memory_unless_it_asks_for_persistent_storage() {
    assert_eq!(WebViewConfig::default().storage, WebViewStorage::Ephemeral);
    assert_eq!(
        WebViewStorage::parse("persistent"),
        WebViewStorage::Persistent
    );
    assert_eq!(
        WebViewStorage::parse(" Persistent "),
        WebViewStorage::Persistent,
        "attribute values are case-insensitive and trimmed"
    );
    for other in ["", "ephemeral", "session", "true", "persist:drive"] {
        assert_eq!(
            WebViewStorage::parse(other),
            WebViewStorage::Ephemeral,
            "{other:?} asks for nothing that outlives the app"
        );
    }
}

#[test]
fn the_storage_attribute_is_the_only_one_the_config_reads() {
    let mut cfg = WebViewConfig::default();
    assert!(cfg.apply_attr("storage", "persistent"));
    assert_eq!(cfg.storage, WebViewStorage::Persistent);
    // The page is the node's `src` attribute and the size its CSS: neither
    // rides in the config.
    for (key, value) in [("src", "https://example.com/"), ("width", "640"), ("id", "x")] {
        assert!(!cfg.apply_attr(key, value), "{key} is not the config's");
    }
    assert_eq!(cfg.storage, WebViewStorage::Persistent);
}

#[test]
fn a_callback_reads_the_code_of_an_oauth_redirect_to_the_loopback() {
    let redirect = WebViewNavigation {
        url: AzString::from(
            "http://127.0.0.1:53682/callback?code=4%2F0AbCd-Ef_9&state=xyz&scope=files+profile",
        ),
        is_redirect: true,
    };
    let code = redirect.get_query_param(AzString::from("code"));
    assert_eq!(
        code.as_ref().map(AzString::as_str),
        Some("4/0AbCd-Ef_9"),
        "percent-escapes are decoded (a Google code holds a `/`)"
    );
    assert_eq!(
        url_query_param(redirect.url.as_str(), "scope").as_deref(),
        Some("files profile"),
        "a `+` in a query is a space (application/x-www-form-urlencoded)"
    );
    assert_eq!(
        url_query_param(redirect.url.as_str(), "state").as_deref(),
        Some("xyz")
    );
    assert_eq!(
        redirect.get_query_param(AzString::from("error")),
        OptionString::None
    );
}

#[test]
fn a_query_parameter_is_read_from_the_query_only() {
    // The fragment is not part of the query, and a path segment that looks
    // like `code=` is no parameter.
    let url = "app.example:/oauth/code=nope?code=abc#code=frag";
    assert_eq!(url_query_param(url, "code").as_deref(), Some("abc"));
    assert_eq!(
        url_query_param("https://example.com/cb#code=frag", "code"),
        None,
        "an implicit-flow fragment is not a query"
    );
    // The first of a repeated parameter, a bare key as the empty value, and
    // an escape that is not one kept as written.
    let url = "https://example.com/cb?a=1&a=2&flag&bad=%zz%41";
    assert_eq!(url_query_param(url, "a").as_deref(), Some("1"));
    assert_eq!(url_query_param(url, "flag").as_deref(), Some(""));
    assert_eq!(url_query_param(url, "bad").as_deref(), Some("%zzA"));
    assert_eq!(url_query_param(url, "missing"), None);
}

#[test]
fn each_web_view_event_is_its_own_event_type() {
    let pairs = [
        (
            WebViewEvent::NavigationRequested(WebViewNavigation {
                url: AzString::from("https://example.com/"),
                is_redirect: false,
            }),
            EventType::WebViewNavigationRequested,
        ),
        (
            WebViewEvent::LoadFinished(AzString::from("https://example.com/")),
            EventType::WebViewLoadFinished,
        ),
        (
            WebViewEvent::TitleChanged(AzString::from("Sign in")),
            EventType::WebViewTitleChanged,
        ),
        (
            WebViewEvent::LoadFailed(WebViewLoadError {
                url: AzString::from("https://example.com/"),
                reason: AzString::from("offline"),
            }),
            EventType::WebViewLoadFailed,
        ),
    ];
    for (event, ty) in pairs {
        assert_eq!(event.event_type(), ty, "{event:?}");
    }
}

#[test]
fn a_web_view_event_is_aimed_at_its_node_alone() {
    let event = WebViewEvent::NavigationRequested(WebViewNavigation {
        url: AzString::from("http://127.0.0.1:53682/callback?code=x"),
        is_redirect: true,
    });
    let synthetic = create_webview_event(&event, node(3), &crate::task::Instant::Tick(
        crate::task::SystemTick::new(0),
    ));
    assert_eq!(synthetic.event_type, EventType::WebViewNavigationRequested);
    assert_eq!(synthetic.target, node(3));
    assert_eq!(synthetic.current_target, node(3));
    assert_eq!(
        synthetic.phase,
        EventPhase::Target,
        "a component event: delivered at the node, no capture, no bubble"
    );
    assert_eq!(synthetic.source, EventSource::Lifecycle);
    assert!(!synthetic.prevented_default, "a fresh navigation is allowed");
}

#[test]
fn a_command_names_what_the_app_asked_the_web_view_to_do() {
    assert_eq!(
        WebViewCommand::Navigate(AzString::from("https://example.com/")),
        WebViewCommand::Navigate(AzString::from("https://example.com/"))
    );
    assert_ne!(WebViewCommand::Reload, WebViewCommand::GoBack);
}
