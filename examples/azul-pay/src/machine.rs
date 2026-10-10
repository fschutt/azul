//! The checkout's state machine (CHECKOUT-PLAN §3.4): one pure [`step`] from a state and an
//! event to the next state and the effects the app runs. No window, no network, no clock.
//!
//! ```text
//! Choosing --Pay (consent)--> Preparing --Created--> Presenting (fields / page)
//!                                       \--Created (browser)--> Waiting
//! Preparing --CreateFailed--> Choosing
//! Presenting --Confirm (fields complete)--> Confirming --result succeeded / processing--> Waiting
//! Confirming --result failed--> Presenting (the provider's message; Pay works again)
//! Presenting --success / pending return page, a provider login (to the browser)--> Waiting
//! Presenting --load failed / Open in browser--> the next surface of the SAME checkout
//! Presenting --cancel return page / Close--> Choosing (the checkout abandoned)
//! Confirming --Close--> Stopped (the claim kept: the payment may be in flight)
//! Waiting --Approved--> Done        Waiting --Declined--> Declined
//! Waiting --Stop waiting / Close--> Stopped --Check again--> Waiting
//! Preparing (paper) --Posted--> Posted: the slip shown, the background claims look daily
//! Posted --Approved--> Done   Posted --Declined--> Declined   Posted --Close--> Choosing (kept)
//! ```
//!
//! The app runs the effects: [`Effect::CreateCheckout`] (`POST /v1/checkout` with the claim key,
//! azcloud-kit's), [`Effect::ShowSurface`] (the popover with the web view's `src`),
//! [`Effect::AllowNavigation`] / [`Effect::CancelNavigation`] (`prevent_default` in the web
//! view's `WebViewNavigationRequested` callback - exactly one of the two for every
//! [`Event::Navigation`] of a page), [`Effect::WebviewNavigate`] (`webview_navigate`),
//! [`Effect::OpenBrowser`] (`Url::open`), [`Effect::SwitchSurface`]
//! (`POST /v1/checkout/{id}/surface`), [`Effect::Abandon`] (`POST /v1/checkout/{id}/abandon`),
//! [`Effect::StartPoll`] / [`Effect::StopPoll`] (the dialog's wait for the drive; after
//! `StopPoll` the background claims and the next start take over), [`Effect::Notice`].
//!
//! The navigation policy of a page (§3.7), in order: a URL that does not parse strictly is
//! blocked; on the provider's pages origin a `/_bridge/` message is read and cancelled, a return
//! page is acted on and cancelled, the fields page is allowed and anything else is blocked; a
//! "leave" origin of the method (a PayPal or Link login) is cancelled and opened in the system
//! browser; the provider's own origins are allowed; everything else is blocked. The chip's host
//! follows the allowed navigations only.

use std::fmt;

use crate::{
    bridge::{command_url, parse_bridge, BridgeMessage, CardBrand, Outcome, PageCommand},
    offer::{ReturnKind, Returns, Settles},
    pills::Choice,
    registry::{Method, SurfaceKind, BRIDGE_PREFIX, FIELDS_PREFIX},
    cash::CashSlip,
    surface::{Created, SecretUrl, Surface},
    url::{shown_host, WebUrl},
};

/// How many declines suggest another method.
pub const DECLINES_BEFORE_ANOTHER_METHOD: u32 = 3;

/// What the dialog says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The order needs the consent ticked.
    ConsentRequired,
    /// Nothing of the method can be shown here.
    NoSurface,
    CreateFailed(String),
    /// A navigation off the provider's pages was blocked: where it went.
    Blocked { host: String },
    /// A provider login (or a bank) opens in the browser.
    LeftForBrowser { host: String },
    /// The payment goes on in the browser.
    BrowserOpened { host: String },
    /// The provider's message (a declined card).
    ProviderError(String),
    TryAnotherMethod,
    FieldsIncomplete,
    /// The surface did not load; the next one is asked for.
    LoadFailed { host: String },
    ProviderUnavailable { provider: String },
    /// Cancelled on the provider's page.
    Cancelled,
    /// The method settles in days: the drive comes when the bank confirms.
    SettlesInDays,
    StoppedWaiting,
    Declined(String),
    NoBrowserSurface,
    /// The server could not make the asked surface.
    SurfaceRefused(String),
    /// A cash checkout's slip is printed: the drive comes when the letter arrived.
    WaitingForLetter,
}

impl Notice {
    /// The sentence the dialog shows.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Notice::ConsentRequired => String::from(
                "Tick the box above to order: the service starts as soon as the payment is \
                 confirmed.",
            ),
            Notice::NoSurface => String::from(
                "This payment method cannot be shown on this computer. Choose another one.",
            ),
            Notice::CreateFailed(why) => format!("The payment could not be prepared: {why}"),
            Notice::Blocked { host } => format!(
                "The payment page tried to open {host}; it was blocked. Use \"Open in browser \
                 instead\" to go on in your browser."
            ),
            Notice::LeftForBrowser { host } => {
                format!("{host} opens in your browser: finish the payment there.")
            }
            Notice::BrowserOpened { host } => format!(
                "Finish the payment in your browser ({host}). The drive appears here as soon as \
                 the payment is confirmed - also after a restart."
            ),
            Notice::ProviderError(text) => text.clone(),
            Notice::TryAnotherMethod => String::from(
                "The payment was not confirmed three times. Try another payment method.",
            ),
            Notice::FieldsIncomplete => String::from("Fill in the payment details first."),
            Notice::LoadFailed { host } => {
                format!("The payment page of {host} did not load; trying another way.")
            }
            Notice::ProviderUnavailable { provider } => {
                format!("{provider} is not reachable. Try another payment method.")
            }
            Notice::Cancelled => String::from("The payment was cancelled; nothing was charged."),
            Notice::SettlesInDays => String::from(
                "We'll add the drive when your bank confirms. You can close AzDrive.",
            ),
            Notice::StoppedWaiting => String::from(
                "Stopped waiting. A payment made now still brings the drive: it is asked for in \
                 the background, and again at the next start.",
            ),
            Notice::Declined(why) => format!("The payment did not go through: {why}"),
            Notice::NoBrowserSurface => {
                String::from("This payment cannot be opened in the browser.")
            }
            Notice::SurfaceRefused(why) => {
                format!("The payment could not be moved to the browser: {why}")
            }
            Notice::WaitingForLetter => String::from(
                "Waiting for your letter: postal cash takes a while, AzDrive checks once a day. \
                 Nothing else tells you - look here again in a few days.",
            ),
        }
    }
}

/// An open checkout: what was chosen, what it shows now, its return pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub checkout_id: String,
    pub choice: Choice,
    pub surface: Surface,
    pub returns: Returns,
}

impl Checkout {
    /// The surface after `kind` in the choice's chain.
    #[must_use]
    pub fn after(&self, kind: SurfaceKind) -> Option<SurfaceKind> {
        let surfaces = &self.choice.method.surfaces;
        let at = surfaces.iter().position(|s| *s == kind)?;
        surfaces.get(at + 1).copied()
    }

    fn settles(&self) -> Settles {
        self.choice.method.settles
    }
}

/// What the page in the popover reported.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Page {
    /// The host of the top-level document, from the allowed navigations (empty: the surface's).
    pub host: String,
    pub ready: bool,
    pub complete: bool,
    pub brand: Option<CardBrand>,
    pub last4: Option<String>,
    /// The height the fields asked for.
    pub height: Option<u32>,
    /// The provider's last message.
    pub error: Option<String>,
    /// Confirm commands sent (their sequence numbers).
    pub confirms: u32,
    pub declines: u32,
    /// The surface asked of the server.
    pub switching: Option<SurfaceKind>,
    /// The current surface failed to load.
    pub failed: bool,
}

/// Why the dialog waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitReason {
    /// The payment goes on in the system browser.
    Browser,
    /// The provider's page returned (success or pending).
    Returned,
    /// The fields page confirmed.
    Confirmed,
    /// "Check again" after Stop waiting.
    Again,
}

/// Where a checkout stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// The pills, the consent, the order button.
    Choosing,
    /// The checkout is being made.
    Preparing {
        choice: Choice,
        surface: SurfaceKind,
    },
    /// The popover shows the surface.
    Presenting {
        checkout: Box<Checkout>,
        page: Page,
    },
    /// The fields page was told to confirm.
    Confirming {
        checkout: Box<Checkout>,
        page: Page,
    },
    /// The dialog polls for the drive.
    Waiting {
        checkout_id: String,
        reason: WaitReason,
        /// The browser's page, to open again.
        browser: Option<SecretUrl>,
        settles: Settles,
    },
    /// The dialog stopped waiting; the claim stays (the background claims, the next start).
    Stopped { checkout_id: String },
    Done { checkout_id: String },
    Declined { checkout_id: String, reason: String },
    /// A cash checkout: its slip is printed, the letter on its way; the background claims
    /// look once a day (the dialog waits for nothing).
    Posted { slip: Box<CashSlip> },
}

/// The popover's chrome: the verified host, what the page is, who runs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub host: String,
    /// `card fields by Stripe`, `payment page of GoCardless`.
    pub what: String,
    pub legal_name: &'static str,
    /// The seller, for a merchant of record.
    pub seller: Option<&'static str>,
}

impl State {
    /// For the scripts' lines: `choosing`, `preparing`, `presenting`, `confirming`, `waiting`,
    /// `stopped`, `done`, `declined`, `posted`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            State::Choosing => "choosing",
            State::Preparing { .. } => "preparing",
            State::Presenting { .. } => "presenting",
            State::Confirming { .. } => "confirming",
            State::Waiting { .. } => "waiting",
            State::Stopped { .. } => "stopped",
            State::Done { .. } => "done",
            State::Declined { .. } => "declined",
            State::Posted { .. } => "posted",
        }
    }

    /// The checkout, once there is one.
    #[must_use]
    pub fn checkout_id(&self) -> Option<&str> {
        match self {
            State::Choosing | State::Preparing { .. } => None,
            State::Presenting { checkout, .. } | State::Confirming { checkout, .. } => {
                Some(&checkout.checkout_id)
            }
            State::Waiting { checkout_id, .. }
            | State::Stopped { checkout_id }
            | State::Done { checkout_id }
            | State::Declined { checkout_id, .. } => Some(checkout_id),
            State::Posted { slip } => Some(&slip.checkout_id),
        }
    }

    /// The checkout and its page, while the popover shows it.
    #[must_use]
    pub fn presenting(&self) -> Option<(&Checkout, &Page)> {
        match self {
            State::Presenting { checkout, page } | State::Confirming { checkout, page } => {
                Some((checkout, page))
            }
            _ => None,
        }
    }

    /// A checkout runs that the dialog waits for (no second order).
    #[must_use]
    pub fn busy(&self) -> bool {
        matches!(
            self,
            State::Preparing { .. }
                | State::Presenting { .. }
                | State::Confirming { .. }
                | State::Waiting { .. }
        )
    }

    /// The popover's chrome, while it shows a page.
    #[must_use]
    pub fn chip(&self) -> Option<Chip> {
        let (checkout, page) = self.presenting()?;
        let spec = checkout.choice.provider.spec;
        let host = if page.host.is_empty() {
            checkout.surface.url.host()
        } else {
            page.host.clone()
        };
        let what = match checkout.surface.kind {
            SurfaceKind::PopoverFields => {
                let fields = match checkout.choice.method.method {
                    Method::Card => "card fields",
                    Method::SepaDebit => "direct debit fields",
                    _ => "payment fields",
                };
                format!("{fields} by {}", spec.name)
            }
            _ => format!("payment page of {}", spec.name),
        };
        Some(Chip {
            host,
            what,
            legal_name: spec.legal_name,
            seller: spec.kind.seller(),
        })
    }
}

/// What happened.
#[derive(Clone, PartialEq, Eq)]
pub enum Event {
    /// The order button, with the pill's choice and whether the consent is ticked.
    Pay { choice: Choice, consent: bool },
    /// `POST /v1/checkout` answered, checked.
    Created(Box<Created>),
    CreateFailed(String),
    /// The web view asks about a main-frame navigation (`WebViewNavigationRequested`).
    Navigation { url: String, redirect: bool },
    /// The page finished loading, on `url`.
    LoadFinished { url: String },
    /// The page did not load (or there is no web view here).
    LoadFailed { reason: String },
    /// The popover's Pay, with the cardholder name typed natively.
    Confirm { name: String },
    /// "Open in browser instead" (in the popover), "Open the page again" (while waiting).
    OpenInBrowser,
    /// `POST /v1/checkout/{id}/surface` answered, checked.
    Switched(Surface),
    SwitchFailed(String),
    /// The popover (or the dialog) closed.
    Close,
    StopWaiting,
    CheckAgain,
    /// The poll says the payment settles in days.
    PendingForDays,
    /// The poll opened the sealed sign-up: the drive is the app's.
    Approved,
    /// The poll says the payment was declined (or the checkout is gone).
    Declined(String),
    /// `POST /v1/checkout` of a cash checkout answered, checked: its slip.
    Posted(Box<CashSlip>),
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Pay { choice, consent } => f
                .debug_struct("Pay")
                .field("provider", &choice.provider.spec.id)
                .field("method", &choice.method.method)
                .field("consent", consent)
                .finish(),
            Event::Created(created) => f.debug_tuple("Created").field(created).finish(),
            Event::CreateFailed(why) => f.debug_tuple("CreateFailed").field(why).finish(),
            Event::Navigation { url, redirect } => f
                .debug_struct("Navigation")
                .field("host", &shown_host(url))
                .field("redirect", redirect)
                .finish(),
            Event::LoadFinished { url } => f
                .debug_struct("LoadFinished")
                .field("host", &shown_host(url))
                .finish(),
            Event::LoadFailed { reason } => {
                f.debug_struct("LoadFailed").field("reason", reason).finish()
            }
            Event::Confirm { .. } => f.write_str("Confirm { name: <hidden> }"),
            Event::OpenInBrowser => f.write_str("OpenInBrowser"),
            Event::Switched(surface) => f.debug_tuple("Switched").field(surface).finish(),
            Event::SwitchFailed(why) => f.debug_tuple("SwitchFailed").field(why).finish(),
            Event::Close => f.write_str("Close"),
            Event::StopWaiting => f.write_str("StopWaiting"),
            Event::CheckAgain => f.write_str("CheckAgain"),
            Event::PendingForDays => f.write_str("PendingForDays"),
            Event::Approved => f.write_str("Approved"),
            Event::Declined(why) => f.debug_tuple("Declined").field(why).finish(),
            Event::Posted(slip) => f.debug_tuple("Posted").field(&slip.checkout_id).finish(),
        }
    }
}

/// What the app does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// `POST /v1/checkout` for this provider, method and first surface.
    CreateCheckout {
        provider: &'static str,
        method: Method,
        surface: SurfaceKind,
    },
    /// The popover shows `Surface` (the web view's `src`).
    ShowSurface(Surface),
    /// The web view goes to this URL (a command to the fields page).
    WebviewNavigate(SecretUrl),
    /// The navigation asked about goes ahead.
    AllowNavigation,
    /// The navigation asked about is cancelled (`prevent_default`).
    CancelNavigation,
    /// `Url::open`.
    OpenBrowser(SecretUrl),
    /// `POST /v1/checkout/{id}/surface {"kind"}`.
    SwitchSurface {
        checkout_id: String,
        kind: SurfaceKind,
    },
    /// `POST /v1/checkout/{id}/abandon`: the provider session expires (and the checkout leaves
    /// the keyring's list - it can never be paid).
    Abandon { checkout_id: String },
    /// The dialog waits for the drive.
    StartPoll { checkout_id: String },
    /// The dialog stops waiting; the background claims take over.
    StopPoll,
    Notice(Notice),
    /// The cash checkout's two pages are offered: the buyer's copy and the slip to post.
    ShowPaper(Box<CashSlip>),
}

/// One step: `state` after `event`, and what the app does.
#[must_use]
pub fn step(state: State, event: Event) -> (State, Vec<Effect>) {
    match state {
        State::Choosing => choosing(event),
        State::Preparing { choice, surface } => preparing(choice, surface, event),
        State::Presenting { checkout, page } => presenting(checkout, page, event, false),
        State::Confirming { checkout, page } => presenting(checkout, page, event, true),
        State::Waiting {
            checkout_id,
            reason,
            browser,
            settles,
        } => waiting(checkout_id, reason, browser, settles, event),
        State::Stopped { checkout_id } => stopped(checkout_id, event),
        State::Posted { slip } => posted(slip, event),
        State::Done { checkout_id } => match event {
            Event::Navigation { .. } => (State::Done { checkout_id }, vec![Effect::CancelNavigation]),
            Event::Close => (State::Choosing, Vec::new()),
            Event::Created(created) => (
                State::Done { checkout_id },
                vec![Effect::Abandon {
                    checkout_id: created.checkout_id,
                }],
            ),
            _ => (State::Done { checkout_id }, Vec::new()),
        },
        State::Declined {
            checkout_id,
            reason,
        } => match event {
            Event::Pay { .. } => choosing(event),
            Event::Close => (State::Choosing, Vec::new()),
            Event::Navigation { .. } => (
                State::Declined {
                    checkout_id,
                    reason,
                },
                vec![Effect::CancelNavigation],
            ),
            Event::Created(created) => (
                State::Declined {
                    checkout_id,
                    reason,
                },
                vec![Effect::Abandon {
                    checkout_id: created.checkout_id,
                }],
            ),
            _ => (
                State::Declined {
                    checkout_id,
                    reason,
                },
                Vec::new(),
            ),
        },
    }
}

fn choosing(event: Event) -> (State, Vec<Effect>) {
    match event {
        Event::Pay { choice, consent } => {
            if !consent {
                return (State::Choosing, vec![Effect::Notice(Notice::ConsentRequired)]);
            }
            let Some(&surface) = choice.method.surfaces.first() else {
                return (State::Choosing, vec![Effect::Notice(Notice::NoSurface)]);
            };
            let effect = Effect::CreateCheckout {
                provider: choice.provider.spec.id,
                method: choice.method.method,
                surface,
            };
            (State::Preparing { choice, surface }, vec![effect])
        }
        // A checkout made for a dialog that closed: nobody will pay it.
        Event::Created(created) => (
            State::Choosing,
            vec![Effect::Abandon {
                checkout_id: created.checkout_id,
            }],
        ),
        Event::Navigation { .. } => (State::Choosing, vec![Effect::CancelNavigation]),
        _ => (State::Choosing, Vec::new()),
    }
}

fn preparing(choice: Choice, surface: SurfaceKind, event: Event) -> (State, Vec<Effect>) {
    match event {
        Event::Created(created) => {
            let Created {
                checkout_id,
                surface,
                returns,
            } = *created;
            enter(
                Checkout {
                    checkout_id,
                    choice,
                    surface,
                    returns,
                },
                Vec::new(),
            )
        }
        Event::CreateFailed(why) => (
            State::Choosing,
            vec![Effect::Notice(Notice::CreateFailed(why))],
        ),
        Event::Posted(slip) if surface == SurfaceKind::Paper => (
            State::Posted { slip: slip.clone() },
            vec![
                Effect::ShowPaper(slip),
                Effect::Notice(Notice::WaitingForLetter),
                Effect::StopPoll,
            ],
        ),
        Event::Posted(_) => (
            State::Choosing,
            vec![Effect::Notice(Notice::CreateFailed(String::from(
                "the token server answered with a cash slip for another payment",
            )))],
        ),
        Event::Close => (State::Choosing, Vec::new()),
        Event::Navigation { .. } => (
            State::Preparing { choice, surface },
            vec![Effect::CancelNavigation],
        ),
        _ => (State::Preparing { choice, surface }, Vec::new()),
    }
}

/// The checkout shows its surface: the popover (a fresh page), or the system browser and the
/// wait. `effects` come first.
fn enter(checkout: Checkout, mut effects: Vec<Effect>) -> (State, Vec<Effect>) {
    if checkout.surface.kind == SurfaceKind::SystemBrowser {
        let host = checkout.surface.url.host();
        effects.push(Effect::OpenBrowser(checkout.surface.url.clone()));
        effects.push(Effect::Notice(Notice::BrowserOpened { host }));
        return wait(
            checkout.checkout_id,
            WaitReason::Browser,
            Some(checkout.surface.url),
            checkout.choice.method.settles,
            effects,
        );
    }
    effects.push(Effect::ShowSurface(checkout.surface.clone()));
    (
        State::Presenting {
            checkout: Box::new(checkout),
            page: Page::default(),
        },
        effects,
    )
}

/// The dialog waits for the drive of `checkout_id`.
fn wait(
    checkout_id: String,
    reason: WaitReason,
    browser: Option<SecretUrl>,
    settles: Settles,
    mut effects: Vec<Effect>,
) -> (State, Vec<Effect>) {
    effects.push(Effect::StartPoll {
        checkout_id: checkout_id.clone(),
    });
    if settles == Settles::Days && reason != WaitReason::Again {
        effects.push(Effect::Notice(Notice::SettlesInDays));
    }
    (
        State::Waiting {
            checkout_id,
            reason,
            browser,
            settles,
        },
        effects,
    )
}

/// Where a navigation of the page goes.
enum Nav {
    /// Blocked: the host to name.
    Blocked(String),
    Bridge(Option<BridgeMessage>),
    Return(ReturnKind),
    /// A login or a bank: to the system browser.
    Leave(WebUrl),
    /// On the provider's pages: the host the chip shows.
    Allowed(String),
}

fn classify(checkout: &Checkout, text: &str) -> Nav {
    let Ok(url) = WebUrl::parse(text) else {
        return Nav::Blocked(shown_host(text));
    };
    let provider = &checkout.choice.provider;
    if provider.spec.on_pages(&url) {
        if url.path().starts_with(BRIDGE_PREFIX) {
            return Nav::Bridge(parse_bridge(&url).ok());
        }
        if let Some(kind) = checkout.returns.kind_of(&url, provider) {
            return Nav::Return(kind);
        }
        let fields = format!("{FIELDS_PREFIX}{}/", provider.spec.id);
        if url.path().starts_with(&fields) {
            return Nav::Allowed(url.shown());
        }
    }
    if checkout.choice.method.spec.leave.iter().any(|o| o.matches(&url)) {
        return Nav::Leave(url);
    }
    if provider.allows(&url) {
        return Nav::Allowed(url.shown());
    }
    Nav::Blocked(url.shown())
}

fn presenting(
    checkout: Box<Checkout>,
    mut page: Page,
    event: Event,
    confirming: bool,
) -> (State, Vec<Effect>) {
    let keep = |checkout: Box<Checkout>, page: Page| {
        if confirming {
            State::Confirming { checkout, page }
        } else {
            State::Presenting { checkout, page }
        }
    };
    match event {
        Event::Navigation { url, .. } => match classify(&checkout, &url) {
            Nav::Blocked(host) => (
                keep(checkout, page),
                vec![
                    Effect::CancelNavigation,
                    Effect::Notice(Notice::Blocked { host }),
                ],
            ),
            Nav::Allowed(host) => {
                page.host = host;
                (keep(checkout, page), vec![Effect::AllowNavigation])
            }
            Nav::Bridge(None) => (keep(checkout, page), vec![Effect::CancelNavigation]),
            Nav::Bridge(Some(message)) => bridge(checkout, page, message, confirming),
            Nav::Return(ReturnKind::Success | ReturnKind::Pending) => {
                let settles = checkout.settles();
                wait(
                    checkout.checkout_id,
                    WaitReason::Returned,
                    None,
                    settles,
                    vec![Effect::CancelNavigation],
                )
            }
            Nav::Return(ReturnKind::Cancel) => (
                State::Choosing,
                vec![
                    Effect::CancelNavigation,
                    Effect::Abandon {
                        checkout_id: checkout.checkout_id,
                    },
                    Effect::Notice(Notice::Cancelled),
                ],
            ),
            Nav::Leave(url) => {
                let settles = checkout.settles();
                let url = SecretUrl::new(url);
                let host = url.host();
                wait(
                    checkout.checkout_id,
                    WaitReason::Browser,
                    Some(url.clone()),
                    settles,
                    vec![
                        Effect::CancelNavigation,
                        Effect::OpenBrowser(url),
                        Effect::Notice(Notice::LeftForBrowser { host }),
                    ],
                )
            }
        },
        Event::LoadFinished { url } => {
            if let Nav::Allowed(host) = classify(&checkout, &url) {
                page.host = host;
            }
            (keep(checkout, page), Vec::new())
        }
        Event::LoadFailed { .. } => {
            if confirming {
                // The payment may be on its way: the poll tells.
                let settles = checkout.settles();
                return wait(
                    checkout.checkout_id,
                    WaitReason::Confirmed,
                    None,
                    settles,
                    Vec::new(),
                );
            }
            if page.switching.is_some() {
                return (keep(checkout, page), Vec::new());
            }
            page.failed = true;
            let host = checkout.surface.url.host();
            match checkout.after(checkout.surface.kind) {
                Some(kind) => {
                    page.switching = Some(kind);
                    let effects = vec![
                        Effect::Notice(Notice::LoadFailed { host }),
                        Effect::SwitchSurface {
                            checkout_id: checkout.checkout_id.clone(),
                            kind,
                        },
                    ];
                    (keep(checkout, page), effects)
                }
                None => give_up(*checkout),
            }
        }
        Event::Confirm { name } => {
            if confirming || checkout.surface.kind != SurfaceKind::PopoverFields {
                return (keep(checkout, page), Vec::new());
            }
            if !(page.ready && page.complete) {
                return (
                    keep(checkout, page),
                    vec![Effect::Notice(Notice::FieldsIncomplete)],
                );
            }
            page.confirms += 1;
            page.error = None;
            let url = command_url(
                &checkout.surface.url,
                &PageCommand::Confirm { name },
                page.confirms,
            );
            (
                State::Confirming { checkout, page },
                vec![Effect::WebviewNavigate(url)],
            )
        }
        Event::OpenInBrowser => {
            if confirming || page.switching.is_some() {
                return (keep(checkout, page), Vec::new());
            }
            if !checkout
                .choice
                .method
                .surfaces
                .contains(&SurfaceKind::SystemBrowser)
            {
                return (
                    keep(checkout, page),
                    vec![Effect::Notice(Notice::NoBrowserSurface)],
                );
            }
            page.switching = Some(SurfaceKind::SystemBrowser);
            let effect = Effect::SwitchSurface {
                checkout_id: checkout.checkout_id.clone(),
                kind: SurfaceKind::SystemBrowser,
            };
            (keep(checkout, page), vec![effect])
        }
        Event::Switched(surface) => {
            if confirming || page.switching.is_none() {
                return (keep(checkout, page), Vec::new());
            }
            let mut checkout = *checkout;
            checkout.surface = surface;
            enter(checkout, Vec::new())
        }
        Event::SwitchFailed(why) => {
            let Some(asked) = page.switching.take() else {
                return (keep(checkout, page), Vec::new());
            };
            if !page.failed {
                // The surface shown still works: stay on it.
                return (
                    keep(checkout, page),
                    vec![Effect::Notice(Notice::SurfaceRefused(why))],
                );
            }
            match checkout.after(asked) {
                Some(kind) => {
                    page.switching = Some(kind);
                    let effect = Effect::SwitchSurface {
                        checkout_id: checkout.checkout_id.clone(),
                        kind,
                    };
                    (keep(checkout, page), vec![effect])
                }
                None => give_up(*checkout),
            }
        }
        Event::Close => {
            if confirming {
                return (
                    State::Stopped {
                        checkout_id: checkout.checkout_id,
                    },
                    vec![Effect::StopPoll, Effect::Notice(Notice::StoppedWaiting)],
                );
            }
            (
                State::Choosing,
                vec![Effect::Abandon {
                    checkout_id: checkout.checkout_id,
                }],
            )
        }
        Event::Created(created) => (
            keep(checkout, page),
            vec![Effect::Abandon {
                checkout_id: created.checkout_id,
            }],
        ),
        Event::Pay { .. }
        | Event::CreateFailed(_)
        | Event::StopWaiting
        | Event::CheckAgain
        | Event::PendingForDays
        | Event::Approved
        | Event::Declined(_)
        | Event::Posted(_) => (keep(checkout, page), Vec::new()),
    }
}

/// No surface of the checkout is left: back to the pills, the checkout abandoned.
fn give_up(checkout: Checkout) -> (State, Vec<Effect>) {
    let provider = checkout.choice.provider.spec.name.to_string();
    (
        State::Choosing,
        vec![
            Effect::Abandon {
                checkout_id: checkout.checkout_id,
            },
            Effect::Notice(Notice::ProviderUnavailable { provider }),
        ],
    )
}

/// A bridge message of the page (its navigation is cancelled).
fn bridge(
    checkout: Box<Checkout>,
    mut page: Page,
    message: BridgeMessage,
    confirming: bool,
) -> (State, Vec<Effect>) {
    let mut effects = vec![Effect::CancelNavigation];
    match message {
        BridgeMessage::Ready => page.ready = true,
        BridgeMessage::Height(px) => page.height = Some(px),
        BridgeMessage::Brand(brand) => page.brand = Some(brand),
        BridgeMessage::Complete(complete) => page.complete = complete,
        BridgeMessage::Last4(digits) => page.last4 = Some(digits),
        BridgeMessage::Error { code, message } => {
            let text = if message.is_empty() { code } else { message };
            effects.push(Effect::Notice(Notice::ProviderError(text.clone())));
            page.error = Some(text);
        }
        BridgeMessage::Result { outcome, code } => {
            if !confirming {
                // No confirm was sent: a result is no answer to anything.
                return (State::Presenting { checkout, page }, effects);
            }
            match outcome {
                Outcome::Succeeded | Outcome::Processing => {
                    let settles = checkout.settles();
                    return wait(
                        checkout.checkout_id,
                        WaitReason::Confirmed,
                        None,
                        settles,
                        effects,
                    );
                }
                Outcome::RequiresAction => {
                    return (State::Confirming { checkout, page }, effects);
                }
                Outcome::Failed => {
                    page.declines += 1;
                    if page.error.is_none() {
                        let text = if code.is_empty() {
                            String::from("the payment was declined")
                        } else {
                            code
                        };
                        effects.push(Effect::Notice(Notice::ProviderError(text.clone())));
                        page.error = Some(text);
                    }
                    if page.declines >= DECLINES_BEFORE_ANOTHER_METHOD {
                        effects.push(Effect::Notice(Notice::TryAnotherMethod));
                    }
                    return (State::Presenting { checkout, page }, effects);
                }
            }
        }
    }
    let state = if confirming {
        State::Confirming { checkout, page }
    } else {
        State::Presenting { checkout, page }
    };
    (state, effects)
}

fn waiting(
    checkout_id: String,
    reason: WaitReason,
    browser: Option<SecretUrl>,
    settles: Settles,
    event: Event,
) -> (State, Vec<Effect>) {
    let same = |checkout_id: String, browser: Option<SecretUrl>| State::Waiting {
        checkout_id,
        reason,
        browser,
        settles,
    };
    match event {
        Event::Navigation { .. } => (same(checkout_id, browser), vec![Effect::CancelNavigation]),
        Event::StopWaiting | Event::Close => (
            State::Stopped { checkout_id },
            vec![Effect::StopPoll, Effect::Notice(Notice::StoppedWaiting)],
        ),
        Event::PendingForDays => (
            State::Stopped { checkout_id },
            vec![Effect::StopPoll, Effect::Notice(Notice::SettlesInDays)],
        ),
        Event::OpenInBrowser => {
            let effects: Vec<Effect> = browser
                .iter()
                .map(|url| Effect::OpenBrowser(url.clone()))
                .collect();
            (same(checkout_id, browser), effects)
        }
        Event::Approved => (State::Done { checkout_id }, Vec::new()),
        Event::Declined(why) => (
            State::Declined {
                checkout_id,
                reason: why.clone(),
            },
            vec![Effect::Notice(Notice::Declined(why))],
        ),
        Event::Created(created) => (
            same(checkout_id, browser),
            vec![Effect::Abandon {
                checkout_id: created.checkout_id,
            }],
        ),
        _ => (same(checkout_id, browser), Vec::new()),
    }
}

fn stopped(checkout_id: String, event: Event) -> (State, Vec<Effect>) {
    match event {
        Event::CheckAgain => wait(
            checkout_id,
            WaitReason::Again,
            None,
            Settles::Instant,
            Vec::new(),
        ),
        Event::Approved => (State::Done { checkout_id }, Vec::new()),
        Event::Declined(why) => (
            State::Declined {
                checkout_id,
                reason: why.clone(),
            },
            vec![Effect::Notice(Notice::Declined(why))],
        ),
        Event::Close => (State::Choosing, Vec::new()),
        Event::Pay { .. } => choosing(event),
        Event::Navigation { .. } => (State::Stopped { checkout_id }, vec![Effect::CancelNavigation]),
        Event::Created(created) => (
            State::Stopped { checkout_id },
            vec![Effect::Abandon {
                checkout_id: created.checkout_id,
            }],
        ),
        _ => (State::Stopped { checkout_id }, Vec::new()),
    }
}

/// A cash checkout whose slip is printed: the letter, then the operator, then the claim. Closing
/// the dialog keeps it (nobody abandons a letter in the post); a new order may follow.
fn posted(slip: Box<CashSlip>, event: Event) -> (State, Vec<Effect>) {
    match event {
        Event::Close => (State::Choosing, Vec::new()),
        Event::Pay { .. } => choosing(event),
        Event::Approved => (
            State::Done {
                checkout_id: slip.checkout_id,
            },
            Vec::new(),
        ),
        Event::Declined(why) => (
            State::Declined {
                checkout_id: slip.checkout_id,
                reason: why.clone(),
            },
            vec![Effect::Notice(Notice::Declined(why))],
        ),
        Event::Navigation { .. } => (State::Posted { slip }, vec![Effect::CancelNavigation]),
        Event::Created(created) => (
            State::Posted { slip },
            vec![Effect::Abandon {
                checkout_id: created.checkout_id,
            }],
        ),
        _ => (State::Posted { slip }, Vec::new()),
    }
}
