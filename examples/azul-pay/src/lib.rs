//! azul-pay: the client half of an Azlin checkout, as a plugin layer - what an app (AzDrive's
//! Buy storage, the installer, AzMail) needs to take a payment through a provider's own pages
//! without ever holding a card number or a provider secret.
//!
//! - **The registry** (compiled in): the providers this app knows - id, kind (a processor, or a
//!   merchant of record that sells in its own name), the names the popover's chrome shows, the
//!   origins their pages may navigate to, and per payment method the chain of surfaces to try
//!   (hosted fields in a popover, the provider's hosted page in an embedded web view, the system
//!   browser) with what the provider says about web views. A provider the registry does not
//!   know is never shown: adding one is an app update.
//! - **The offer** (`GET /v1/checkout/options`): the token server's provider descriptors for a
//!   country, currency, tier and period. The server picks, orders and NARROWS - a provider, a
//!   method, a surface, an origin or a page the registry does not allow is dropped, never
//!   widened: whoever controls the token server cannot frame a page of their choosing inside
//!   the app's trusted chrome.
//! - **The pills**: the payment methods to show for a country, a currency and a period, each
//!   with its provider (and the alternatives where two providers offer one method), and the
//!   default one.
//! - **The surface** (`POST /v1/checkout`): what a checkout opens - the hosted-fields page with
//!   its inputs in the URL fragment, a hosted page, or a page for the system browser - checked
//!   against the provider's origins before anything shows it.
//! - **The navigation bridge**: the hosted-fields page and the app talk through main-frame
//!   navigations only. The page navigates to `<pages>/_bridge/<message>?v=...` and the app reads
//!   and cancels that navigation; the app navigates the page to a new fragment (`#...&cmd=confirm`).
//!   No script bridge, no injected script: an injected script switches Apple Pay on the web off
//!   for the page, and a script bridge would be one more door into the app.
//! - **The state machine**: one pure `step(state, event) -> (state, effects)` - choose, prepare,
//!   present (with the fallback chain: fields, the hosted page, the system browser), confirm,
//!   wait, stopped, done or declined. The app runs the effects (an HTTP call on a thread, the
//!   web view's src, `prevent_default`, `Url::open`, the poll) and feeds the results back.
//!
//! The claim of the paid drive (the claim key, the sealed sign-up, the checkouts kept in the
//! keyring) is azcloud-kit's and unchanged: this crate ends where the poll begins.
//!
//! Nothing here prints, logs or `Debug`s a client secret, a publishable key, a URL's query or
//! fragment, or a cardholder's name. No azul types, no network.

#[cfg(test)]
mod tests;
