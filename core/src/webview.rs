//! POD types for `<webview>`: a native web view embedded as a node.
//!
//! The sign-in page of a cloud drive (`Dropbox`, `OneDrive`, ...) has to run
//! in a browser, and its redirect back to the app has to be CAUGHT by the
//! app: the provider sends the browser to the app's redirect URI (a loopback
//! `http://127.0.0.1:<port>/callback?code=..` or a custom scheme), and what
//! the app wants is the `code` in it, not a page load. `<webview src=..>`
//! shows such a page inside the app's window and asks the app about every
//! top-level navigation BEFORE it happens, so the app can cancel the one that
//! carries its code.
//!
//! What a web view may do is deliberately little:
//!
//! - no `JavaScript` bridge: the page cannot call into the app and the app
//!   evaluates no script in the page; the four events below are the only
//!   channel between them;
//! - cookies and storage live in a per-app store that is gone when the app
//!   quits, unless the node asks for a persistent one
//!   (`storage="persistent"`); never in the system browser's;
//! - `file://` pages are refused before the app is asked.
//!
//! The node type (`NodeType::WebView`) carries [`WebViewConfig`] inline; the
//! page is the node's `src` attribute, as on an iframe, so a changed `src` on
//! the same node navigates the view it already has. The engine half (which
//! views are mounted, where they are on screen, what the backend has to do)
//! is `azul_layout::managers::webview`; the native views live in `azul-dll`.
//!
//! # Events, and allowing or cancelling a navigation
//!
//! Each event is fired at the web view's node only (the `WebView*` variants
//! of `ComponentEventFilter`); its payload is read with
//! `CallbackInfo::get_webview_event`. [`WebViewEvent::NavigationRequested`]
//! is cancelable: a callback that calls `CallbackInfo::prevent_default`
//! cancels the navigation - the idiom of the web's `navigate` event and of
//! Electron's `will-navigate`. Every main-frame navigation is asked about,
//! the initial `src` and an app's own `webview_navigate` included; sub-frames
//! are not.

use alloc::{string::String, vec::Vec};

use azul_css::{AzString, OptionString};

use crate::{
    dom::DomNodeId,
    events::{EventData, EventPhase, EventSource, EventType, SyntheticEvent},
    task::Instant,
};

/// Where a web view keeps its cookies, local storage and cache.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum WebViewStorage {
    /// In memory, in one store all of this app's ephemeral web views share,
    /// gone when the app quits, and never the system browser's. The default:
    /// a sign-in that only has to hand the app a code leaves nothing behind.
    #[default]
    Ephemeral,
    /// The app's own persistent store (`storage="persistent"`): a sign-in
    /// survives a restart. Still the app's alone, never the system browser's.
    Persistent,
}

impl WebViewStorage {
    /// `persistent` (any case, white space trimmed) is `Persistent`; anything
    /// else is `Ephemeral` - a typo must not make a web view keep cookies.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        if value.trim().eq_ignore_ascii_case("persistent") {
            Self::Persistent
        } else {
            Self::Ephemeral
        }
    }

    /// The attribute value, as markup writes it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Persistent => "persistent",
        }
    }
}

/// The inline configuration of a `NodeType::WebView`.
///
/// `Copy` and small on purpose: it rides inside `NodeType` the way
/// `TransientWindowConfig` does, so a web view node costs no allocation and
/// `NodeType` does not grow. The page is NOT here - it is the node's `src`
/// attribute.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WebViewConfig {
    /// Where its cookies and storage live.
    pub storage: WebViewStorage,
}

impl WebViewConfig {
    /// A web view with an ephemeral store (the default).
    #[must_use]
    pub const fn ephemeral() -> Self {
        Self {
            storage: WebViewStorage::Ephemeral,
        }
    }

    /// A web view with the app's persistent store.
    #[must_use]
    pub const fn persistent() -> Self {
        Self {
            storage: WebViewStorage::Persistent,
        }
    }

    /// Apply one markup attribute of a `<webview>` element. Returns whether
    /// the key was the config's (`storage`); every other attribute (`src`,
    /// `width`, `id`, ...) is an ordinary attribute of the node.
    pub fn apply_attr(&mut self, key: &str, value: &str) -> bool {
        match key {
            "storage" => {
                self.storage = WebViewStorage::parse(value);
                true
            }
            _ => false,
        }
    }
}

/// A top-level navigation a web view is about to make: the payload of
/// [`WebViewEvent::NavigationRequested`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WebViewNavigation {
    /// Where it goes.
    pub url: AzString,
    /// A server redirect (an HTTP 3xx answer) of a navigation already under
    /// way - the hop an `OAuth` provider makes to the app's redirect URI.
    pub is_redirect: bool,
}

impl WebViewNavigation {
    /// The query parameter `name` of [`Self::url`], decoded
    /// ([`url_query_param`]); `None` when the query has none of that name.
    /// What a sign-in callback reads the `code` (or the `error`) with.
    #[must_use]
    pub fn get_query_param(&self, name: AzString) -> OptionString {
        url_query_param(self.url.as_str(), name.as_str())
            .map(AzString::from)
            .into()
    }
}

/// Why a page did not load: the payload of [`WebViewEvent::LoadFailed`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct WebViewLoadError {
    /// The page that failed.
    pub url: AzString,
    /// What the platform said, or why there is no web view at all (a
    /// missing system library, a platform without a backend yet).
    pub reason: AzString,
}

/// What a web view reports to the app. Each variant is its own
/// [`EventType`] (see [`Self::event_type`]) and its own
/// `ComponentEventFilter`, fired at the web view's node.
///
/// Data-carrying variants are TUPLE variants: the codegen refuses struct
/// variants with more than one field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum WebViewEvent {
    /// A top-level navigation is about to happen; `prevent_default` cancels it.
    NavigationRequested(WebViewNavigation),
    /// A page finished loading; the URL it ended on.
    LoadFinished(AzString),
    /// The page's title changed.
    TitleChanged(AzString),
    /// A page could not be loaded, or there is no web view to load it in.
    LoadFailed(WebViewLoadError),
}

impl_option!(
    WebViewEvent,
    OptionWebViewEvent,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl WebViewEvent {
    /// The event type this event is dispatched as.
    #[must_use]
    pub const fn event_type(&self) -> EventType {
        match self {
            Self::NavigationRequested(_) => EventType::WebViewNavigationRequested,
            Self::LoadFinished(_) => EventType::WebViewLoadFinished,
            Self::TitleChanged(_) => EventType::WebViewTitleChanged,
            Self::LoadFailed(_) => EventType::WebViewLoadFailed,
        }
    }

    /// The URL the event is about, if it is about one.
    #[must_use]
    pub const fn url(&self) -> Option<&AzString> {
        match self {
            Self::NavigationRequested(n) => Some(&n.url),
            Self::LoadFinished(url) => Some(url),
            Self::LoadFailed(e) => Some(&e.url),
            Self::TitleChanged(_) => None,
        }
    }
}

/// What an app asks a web view to do (`CallbackInfo::webview_navigate`,
/// `webview_reload`, `webview_go_back`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum WebViewCommand {
    /// Load this page.
    Navigate(AzString),
    /// Load the current page again.
    Reload,
    /// Go one entry back in the view's history (nothing at its start).
    GoBack,
}

/// The event `event` is dispatched as, aimed at the web view `node`.
///
/// Built with the shape of the other component events (`EventSource::Lifecycle`,
/// `EventPhase::Target`): the dispatcher fires the node's own
/// `ComponentEventFilter` callbacks and nothing else - no capture, no bubble.
/// The payload is not in the event: callbacks read it with
/// `CallbackInfo::get_webview_event`.
#[must_use]
pub fn create_webview_event(
    event: &WebViewEvent,
    node: DomNodeId,
    timestamp: &Instant,
) -> SyntheticEvent {
    SyntheticEvent {
        event_type: event.event_type(),
        source: EventSource::Lifecycle,
        phase: EventPhase::Target,
        target: node,
        current_target: node,
        timestamp: timestamp.clone(),
        data: EventData::None,
        stopped: false,
        stopped_immediate: false,
        prevented_default: false,
        at_target_only: false,
    }
}

/// The query parameter `name` of `url`, decoded as a form
/// (`application/x-www-form-urlencoded`: `%XX` escapes, `+` as a space), or
/// `None` when the query has no parameter of that name.
///
/// Reads the QUERY only - after the first `?`, before the `#` - so a path
/// segment or a fragment that happens to say `code=` is never taken for a
/// parameter. The first of a repeated parameter wins, a bare key (`?flag`)
/// is the empty value, and an escape that is not one (`%zz`) stays as written.
#[must_use]
pub fn url_query_param(url: &str, name: &str) -> Option<String> {
    let without_fragment = url.split_once('#').map_or(url, |(before, _)| before);
    let (_, query) = without_fragment.split_once('?')?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (form_decode(key) == name).then(|| form_decode(value))
    })
}

/// One `application/x-www-form-urlencoded` component, decoded. Bytes that do
/// not form UTF-8 after decoding are replaced, never dropped.
fn form_decode(component: &str) -> String {
    let bytes = component.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let escaped = match (bytes.get(i + 1), bytes.get(i + 2)) {
                    (Some(&hi), Some(&lo)) => hex_value(hi).zip(hex_value(lo)),
                    _ => None,
                };
                if let Some((hi, lo)) = escaped {
                    out.push((hi << 4) | lo);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The value of one hexadecimal digit.
const fn hex_value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
#[path = "webview_test.rs"]
mod webview_test;
