//! A `<webview>` on the headless backend (WEBVIEW17): the shell's web view
//! pump (`common::webview`) creates and places the view through the
//! backend, dispatches what the backend (and a test's simulation) reports
//! to the view's callbacks, and answers the backend's navigation request
//! with what they decided.
//!
//! The flow is a cloud drive's sign-in: the app shows the provider's
//! authorize page and catches the redirect to its loopback redirect URI in
//! its `WebViewNavigationRequested` callback - reading the `code`, cancelling
//! the navigation - so the view never loads a page nobody serves.

use azul_core::{
    events::{ComponentEventFilter, EventFilter},
    webview::{OptionWebViewEvent, WebViewEvent, WebViewNavigation},
};
use azul_css::AzString;
use azul_layout::managers::webview::WebViewReport;

use super::*;

const AUTHORIZE: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?\
                         client_id=app&redirect_uri=http://127.0.0.1:53682/callback";
const REDIRECT_URI: &str = "http://127.0.0.1:53682/callback";

/// The app's state: whether it shows the sign-in page, and the code its
/// callback caught.
struct SignIn {
    show_view: bool,
    code: Option<String>,
}

fn signing_in() -> Arc<RefCell<RefAny>> {
    Arc::new(RefCell::new(RefAny::new(SignIn {
        show_view: true,
        code: None,
    })))
}

extern "C" fn on_navigation_requested(
    mut data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    let OptionWebViewEvent::Some(WebViewEvent::NavigationRequested(nav)) = info.get_webview_event()
    else {
        return azul_core::callbacks::Update::DoNothing;
    };
    if nav.url.as_str().starts_with(REDIRECT_URI) {
        info.prevent_default();
        if let Some(mut state) = data.downcast_mut::<SignIn>() {
            state.code = nav
                .get_query_param(AzString::from("code"))
                .into_option()
                .map(|c| c.as_str().to_string());
        }
    }
    azul_core::callbacks::Update::DoNothing
}

/// `body(0) > webview(1)`: the authorize page in a 300x200 view - while
/// the app shows it.
extern "C" fn sign_in_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let show = data
        .downcast_ref::<SignIn>()
        .is_none_or(|state| state.show_view);
    if !show {
        return Dom::create_body();
    }
    let callback: azul_layout::callbacks::CallbackType = on_navigation_requested;
    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_webview(AzString::from(AUTHORIZE))
            .with_css("display: block; width: 300px; height: 200px;")
            .with_callback(
                EventFilter::Component(ComponentEventFilter::WebViewNavigationRequested),
                data,
                callback as usize,
            ),
    )
}

#[test]
fn the_headless_backend_shows_a_webview_and_the_app_catches_its_sign_in_redirect() {
    let state = signing_in();
    let mut window = make_window_with(&state, sign_in_layout);
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();
    let _ = window.pump_webviews();

    // The backend was told to create the view and where it is, and asked
    // the app about the authorize page, which the app let load.
    let id = {
        let recorded = window.webview_recorder().views();
        assert_eq!(recorded.len(), 1, "one native view");
        let view = &recorded[0];
        assert!(view.placement.visible, "{:?}", view.placement);
        assert_eq!(
            view.placement.rect,
            azul_core::geom::LogicalRect::new(
                azul_core::geom::LogicalPosition::new(0.0, 0.0),
                azul_core::geom::LogicalSize::new(300.0, 200.0),
            )
        );
        assert_eq!(
            view.history,
            vec![AzString::from(AUTHORIZE)],
            "the app let the authorize page load"
        );
        view.id
    };

    // The user signs in; the provider redirects to the loopback.
    let redirect = "http://127.0.0.1:53682/callback?code=M.C507_BAY.2.U.abc&state=s1";
    window
        .common
        .layout_window
        .as_mut()
        .expect("layout window")
        .webviews
        .push_simulated(WebViewReport {
            id,
            request: 0,
            event: WebViewEvent::NavigationRequested(WebViewNavigation {
                url: AzString::from(redirect),
                is_redirect: true,
            }),
        });
    let _ = window.pump_webviews();

    let code = state
        .borrow_mut()
        .downcast_ref::<SignIn>()
        .and_then(|s| s.code.clone());
    assert_eq!(
        code.as_deref(),
        Some("M.C507_BAY.2.U.abc"),
        "the app's callback read the code off the redirect"
    );
    let lw = window.common.layout_window.as_ref().expect("layout window");
    let view = lw.webviews.get(id).expect("the view");
    assert_eq!(
        view.url.as_str(),
        AUTHORIZE,
        "the cancelled redirect never loaded"
    );
    assert_eq!(
        view.navigations.last().map(|n| (n.is_redirect, n.allowed)),
        Some((true, false))
    );
    assert_eq!(
        window.webview_recorder().get(id).map(|v| v.history.len()),
        Some(1),
        "the backend never went to the redirect"
    );
}

/// The app stops showing the page: the node unmounts, the native view goes.
#[test]
fn an_unmounted_webview_is_destroyed_in_the_backend() {
    let state = signing_in();
    let mut window = make_window_with(&state, sign_in_layout);
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();
    let _ = window.pump_webviews();
    assert_eq!(window.webview_recorder().views().len(), 1);

    if let Some(mut s) = state.borrow_mut().downcast_mut::<SignIn>() {
        s.show_view = false;
    }
    window.regenerate_layout().expect("a second layout pass");
    let _ = window.common.take_regeneration();
    let _ = window.pump_webviews();
    assert!(
        window.webview_recorder().views().is_empty(),
        "the native view went with its node"
    );
}
