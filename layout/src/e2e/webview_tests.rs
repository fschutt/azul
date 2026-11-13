//! A `<webview>` sign-in flow, headless: no browser, the scenario plays the
//! browser's part (`simulate_webview_*`) and lists the views
//! (`list_webviews`), and the app's own callbacks run on the real dispatcher.
//!
//! The flow is the one a cloud drive's sign-in runs: the app shows the
//! provider's authorize page, the user signs in, the provider redirects to
//! the app's loopback redirect URI with a `code` - and the app catches that
//! redirect in its `WebViewNavigationRequested` callback, reads the code and
//! cancels the navigation, so the view never loads a page nobody serves.
//!
//! A child of `runner`, for `run_e2e_test_keeping_runner`.

use std::sync::{Arc, Mutex};

use azul_core::{
    callbacks::Update,
    dom::Dom,
    events::{ComponentEventFilter, EventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
    webview::{OptionWebViewEvent, WebViewEvent},
};
use azul_css::AzString;
use azul_layout::callbacks::{CallbackInfo, CallbackType};

use crate::e2e::E2eTest;

const AUTHORIZE: &str = "https://www.dropbox.com/oauth2/authorize?client_id=app&response_type=code\
                         &redirect_uri=http://127.0.0.1:53682/callback";
const REDIRECT_URI: &str = "http://127.0.0.1:53682/callback";

/// The app's sign-in state: the code its callback caught.
struct SignIn {
    code: Arc<Mutex<Option<String>>>,
}

/// The app's `WebViewNavigationRequested` callback: a navigation to its
/// redirect URI carries the code - read it and cancel the navigation;
/// every other page loads.
extern "C" fn on_navigation_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let OptionWebViewEvent::Some(WebViewEvent::NavigationRequested(nav)) = info.get_webview_event()
    else {
        return Update::DoNothing;
    };
    if !nav.url.as_str().starts_with(REDIRECT_URI) {
        return Update::DoNothing;
    }
    info.prevent_default();
    let code = nav
        .get_query_param(AzString::from("code"))
        .into_option()
        .map(|c| c.as_str().to_string());
    if let Some(state) = data.downcast_ref::<SignIn>() {
        *state.code.lock().expect("the code slot") = code;
    }
    Update::DoNothing
}

/// `body(0) > webview(1)` showing the authorize page, with the callback.
fn sign_in_page(code: &Arc<Mutex<Option<String>>>) -> StyledDom {
    let callback: CallbackType = on_navigation_requested;
    StyledDom::create_from_dom(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_webview(AzString::from(AUTHORIZE))
                .with_css("display: block; width: 400px; height: 300px;")
                .with_callback(
                    EventFilter::Component(ComponentEventFilter::WebViewNavigationRequested),
                    RefAny::new(SignIn {
                        code: Arc::clone(code),
                    }),
                    callback as usize,
                ),
        ),
    )
}

fn scenario(name: &str, steps: serde_json::Value) -> E2eTest {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("scenario json")
}

#[test]
fn a_sign_in_redirect_to_the_loopback_is_caught_and_its_code_read() {
    let code = Arc::new(Mutex::new(None));
    let test = scenario(
        "webview_sign_in",
        serde_json::json!([
            { "op": "wait_frame" },
            // The headless backend asked about the authorize page when it
            // created the view, and the app let it load.
            { "op": "list_webviews" },
            { "op": "assert_response", "type": "json", "contains": "\"loading\":true" },
            { "op": "simulate_webview_load_finished" },
            { "op": "simulate_webview_title", "title": "Sign in - Dropbox" },
            // The user signed in: the provider redirects to the loopback.
            { "op": "simulate_webview_navigation",
              "url": "http://127.0.0.1:53682/callback?code=4%2F0AbCd&state=xyz",
              "redirect": true },
            { "op": "wait_frame" },
            { "op": "list_webviews" },
            { "op": "assert_response", "type": "json", "contains": "\"allowed\":false" },
            { "op": "assert_response", "type": "json", "contains": "\"title\":\"Sign in - Dropbox\"" }
        ]),
    );
    let (result, runner) = super::run_e2e_test_keeping_runner(&test, Some(sign_in_page(&code)));
    assert_eq!(result.status, "pass", "{:#?}", result.steps);

    assert_eq!(
        code.lock().expect("the code slot").as_deref(),
        Some("4/0AbCd"),
        "the callback read the code off the redirect"
    );
    let views = runner.layout_window.webviews.views();
    assert_eq!(views.len(), 1);
    assert_eq!(
        views[0].url.as_str(),
        AUTHORIZE,
        "the cancelled redirect never loaded: the view is still on the authorize page"
    );
    let log: Vec<(String, bool, bool)> = views[0]
        .navigations
        .iter()
        .map(|n| (n.url.as_str().to_string(), n.is_redirect, n.allowed))
        .collect();
    assert_eq!(
        log,
        vec![
            (AUTHORIZE.to_string(), false, true),
            (
                "http://127.0.0.1:53682/callback?code=4%2F0AbCd&state=xyz".to_string(),
                true,
                false
            ),
        ]
    );
}

/// The ops refuse what they cannot do by name: no view to simulate on, a
/// view id nobody has.
#[test]
fn simulating_a_web_view_event_needs_a_web_view() {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "webview_ops_without_a_view",
        "config": { "continue_on_failure": true },
        "steps": [
            { "op": "simulate_webview_navigation", "url": "https://example.com/" },
            { "op": "list_webviews" },
            { "op": "assert_response", "type": "json", "contains": "\"webviews\":[]" }
        ]
    }))
    .expect("scenario json");
    let (result, _runner) = super::run_e2e_test_keeping_runner(&test, None);
    let errors: Vec<String> = result.steps.iter().filter_map(|s| s.error.clone()).collect();
    assert_eq!(errors.len(), 1, "only the simulation fails: {errors:#?}");
    assert!(
        errors[0].contains("webview"),
        "it says there is no web view: {errors:#?}"
    );
}
