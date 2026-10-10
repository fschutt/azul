//! The compiled-in registry: the providers this app knows (CHECKOUT-PLAN §1.3, §3.3).
//!
//! It is the reviewed table of what each provider says about embedding its pages - which
//! payment method may show the provider's hosted fields in the app's popover, which the hosted
//! page in an embedded web view, and which only the system browser - and of where its pages may
//! navigate. The token server's offer can pick from it, order it and narrow it ([`crate::offer`]),
//! never widen it; a change here is an app update.
//!
//! A method's chain is the §1.3 table read left to right minus its "no" cells: the surfaces to
//! try, best first, ending in the system browser. A surface is skipped when the app cannot show
//! it (no web view backend on this platform, no Apple Pay sheet), when the provider forbids it
//! ([`Webview::Forbidden`]: PayPal, every provider login - RFC 8252 and the providers' own
//! rules), or after it failed.
//!
//! The fake providers of the local stacks (feature `fake-providers`) live on this computer: their
//! pages are `http://127.0.0.1:<port>/...`, the pages of a fake provider login
//! `http://localhost:<port>/...` (a different host, so a navigation to it is told apart).

use std::borrow::Cow;

use crate::url::{Origin, WebUrl};

/// Where Azlin's own payment pages live: one hosted-fields page per provider, the return pages
/// and the bridge (static pages, no cookies, no secrets).
pub const PAY_HOST: &str = "pay.azlin.io";
/// The return page after a payment the provider took ("go and ask": never the outcome).
pub const RETURN_SUCCESS: &str = "/return/ok";
/// The return page of a payment the payer cancelled on the provider's page.
pub const RETURN_CANCEL: &str = "/return/cancel";
/// The return page of a payment that settles later (a bank transfer, a debit "processing").
pub const RETURN_PENDING: &str = "/return/pending";
/// The bridge: a fields page navigates to `<pages>/_bridge/<message>?v=...` to tell the app.
pub const BRIDGE_PREFIX: &str = "/_bridge/";
/// The hosted-fields pages: `<pages>/fields/<provider id>/<version>`.
pub const FIELDS_PREFIX: &str = "/fields/";

const PAY_PAGES: Origin = Origin::Exact(Cow::Borrowed(PAY_HOST));

/// How a payer pays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// SEPA Direct Debit.
    SepaDebit,
    Card,
    ApplePay,
    PayPal,
    /// Wero (EPI): a push payment confirmed in the banking app.
    Wero,
    /// A SEPA credit transfer to Azlin's own account.
    BankTransfer,
    /// A voucher code (ours).
    Voucher,
}

impl Method {
    /// Every method, in the order the app lists them when nothing else orders them.
    pub const ALL: [Method; 7] = [
        Method::SepaDebit,
        Method::Card,
        Method::ApplePay,
        Method::PayPal,
        Method::Wero,
        Method::BankTransfer,
        Method::Voucher,
    ];

    /// The wire name (`sepa_debit`, `card`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::SepaDebit => "sepa_debit",
            Method::Card => "card",
            Method::ApplePay => "apple_pay",
            Method::PayPal => "paypal",
            Method::Wero => "wero",
            Method::BankTransfer => "bank_transfer",
            Method::Voucher => "voucher",
        }
    }

    /// A wire name, in any case (`sepa`, the claim contract v1's name, is SEPA Direct Debit).
    #[must_use]
    pub fn parse(text: &str) -> Option<Method> {
        let text = text.trim().to_ascii_lowercase();
        if text == "sepa" {
            return Some(Method::SepaDebit);
        }
        Method::ALL.into_iter().find(|m| m.as_str() == text)
    }

    /// The pill's words.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Method::SepaDebit => "Direct debit",
            Method::Card => "Card",
            Method::ApplePay => "Apple Pay",
            Method::PayPal => "PayPal",
            Method::Wero => "Wero",
            Method::BankTransfer => "Bank transfer",
            Method::Voucher => "Voucher",
        }
    }

    /// The pill's icon: a Material icon name the app ships (never loaded from the network).
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Method::SepaDebit => "account_balance",
            Method::Card => "credit_card",
            Method::ApplePay => "phone_iphone",
            Method::PayPal => "account_balance_wallet",
            Method::Wero => "smartphone",
            Method::BankTransfer => "receipt_long",
            Method::Voucher => "redeem",
        }
    }
}

/// Where a payment is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    /// The system's Apple Pay sheet (macOS, PassKit).
    NativeSheet,
    /// The app's own IBAN field and mandate text, sent straight to the provider's client API.
    NativeIban,
    /// The app's own voucher field.
    NativeVoucher,
    /// The app's popover (card artwork, name, Pay) around the provider's hosted fields in a web
    /// view.
    PopoverFields,
    /// The provider's hosted page in an embedded web view, under the app's verified-origin chip.
    WebviewPage,
    /// The system browser (`Url::open`): its own address bar takes over.
    SystemBrowser,
}

impl SurfaceKind {
    /// Every surface.
    pub const ALL: [SurfaceKind; 6] = [
        SurfaceKind::NativeSheet,
        SurfaceKind::NativeIban,
        SurfaceKind::NativeVoucher,
        SurfaceKind::PopoverFields,
        SurfaceKind::WebviewPage,
        SurfaceKind::SystemBrowser,
    ];

    /// The wire name (`fields`, `page`, `browser`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            SurfaceKind::NativeSheet => "sheet",
            SurfaceKind::NativeIban => "iban",
            SurfaceKind::NativeVoucher => "voucher",
            SurfaceKind::PopoverFields => "fields",
            SurfaceKind::WebviewPage => "page",
            SurfaceKind::SystemBrowser => "browser",
        }
    }

    /// A wire name.
    #[must_use]
    pub fn parse(text: &str) -> Option<SurfaceKind> {
        let text = text.trim();
        SurfaceKind::ALL
            .into_iter()
            .find(|s| s.as_str().eq_ignore_ascii_case(text))
    }

    /// Shown in an embedded web view.
    #[must_use]
    pub const fn in_webview(self) -> bool {
        matches!(self, SurfaceKind::PopoverFields | SurfaceKind::WebviewPage)
    }
}

/// What a provider says about its pages in an embedded web view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Webview {
    Allowed,
    /// Advised against, not forbidden: the chain lists only what was tested to work.
    Discouraged,
    /// Never: the chain has no web-view surface.
    Forbidden,
}

/// What a `window.open` of the provider's page becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popups {
    None,
    /// The same web view, within the allowlist.
    SameWebview,
    SystemBrowser,
}

/// What a web view breaks for a method (why its chain skips the web view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breaks {
    /// Apple Pay on the web (no injected scripts; iOS web views only).
    ApplePayWeb,
    /// Google Pay (pop-ups, PaymentRequest).
    GooglePay,
    /// A login at the provider (PayPal, Link): never in an embedded web view.
    ProviderLogin,
    /// A switch to a banking app (refused from in-app browser views).
    BankAppSwitch,
    Passkeys,
}

/// A provider's stance on a method in a web view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedPolicy {
    pub webview: Webview,
    pub popups: Popups,
    pub breaks: &'static [Breaks],
}

/// Who sells (CHECKOUT-PLAN §3.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// The provider processes the payment; Azlin sells (its receipts, its withdrawal and cancel
    /// flows, its taxes).
    Processor,
    /// The provider is the seller (merchant of record): it owns the contract, the receipts and
    /// the taxes; the app says who sells before the order.
    MerchantOfRecord {
        /// "Sold by <seller>".
        seller: &'static str,
    },
}

impl ProviderKind {
    /// The wire name: `processor`, `merchant_of_record`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Processor => "processor",
            ProviderKind::MerchantOfRecord { .. } => "merchant_of_record",
        }
    }

    /// Who sells, when it is not Azlin.
    #[must_use]
    pub const fn seller(self) -> Option<&'static str> {
        match self {
            ProviderKind::Processor => None,
            ProviderKind::MerchantOfRecord { seller } => Some(seller),
        }
    }
}

/// When a method's money arrives, as the app should expect it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settles {
    /// The drive comes within seconds or minutes (a card, PayPal, a mandate accepted).
    Instant,
    /// The drive may come days later (a bank transfer, a debit "processing"): the app stops
    /// waiting in the dialog and checks at every start.
    Days,
}

impl Settles {
    /// The wire name: `instant`, `days`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Settles::Instant => "instant",
            Settles::Days => "days",
        }
    }
}

/// One method of one provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodSpec {
    pub method: Method,
    /// The surfaces to try, best first; the last is the system browser.
    pub chain: &'static [SurfaceKind],
    pub embed: EmbedPolicy,
    /// Where the provider's pages may leave to the system browser (a login, a bank): a
    /// navigation there is cancelled in the web view and opened in the browser.
    pub leave: &'static [Origin],
    /// When its money usually arrives (the offer may say otherwise).
    pub settles: Settles,
}

/// One provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSpec {
    /// The id the token server names it by (`stripe`, `fake-stripe`).
    pub id: &'static str,
    pub kind: ProviderKind,
    /// The pill's "via ..." line.
    pub name: &'static str,
    /// The contracting entity: the popover chip's second line.
    pub legal_name: &'static str,
    /// Where the provider's own pages may be (its hosted page, 3-D Secure).
    pub origins: &'static [Origin],
    /// Where Azlin's pages for it live: its fields page, the return pages, the bridge.
    pub pages: Origin,
    /// The path of its hosted-fields page on `pages` (`None`: no popover).
    pub fields: Option<&'static str>,
    pub methods: &'static [MethodSpec],
    /// The provider's icon: a Material icon name the app ships.
    pub icon: &'static str,
    /// A fake provider of the local stacks.
    pub fake: bool,
}

impl ProviderSpec {
    /// Its spec of `method`, if it offers it.
    #[must_use]
    pub fn method(&self, method: Method) -> Option<&'static MethodSpec> {
        self.methods.iter().find(|m| m.method == method)
    }

    /// Whether `url` is one of Azlin's pages for this provider (its fields page, a return page,
    /// the bridge).
    #[must_use]
    pub fn on_pages(&self, url: &WebUrl) -> bool {
        self.pages.matches(url)
    }

    /// Whether `url` is one of the provider's own pages.
    #[must_use]
    pub fn owns(&self, url: &WebUrl) -> bool {
        self.origins.iter().any(|o| o.matches(url))
    }
}

/// Every provider this build knows (the fakes with the feature `fake-providers`).
pub fn providers() -> impl Iterator<Item = &'static ProviderSpec> {
    PROVIDERS.iter().chain(FAKE_PROVIDERS.iter())
}

/// The provider `id`.
#[must_use]
pub fn provider(id: &str) -> Option<&'static ProviderSpec> {
    let id = id.trim();
    providers().find(|p| p.id == id)
}

// ==== The table ====

const fn exact(host: &'static str) -> Origin {
    Origin::Exact(Cow::Borrowed(host))
}

const fn suffix(suffix: &'static str) -> Origin {
    Origin::Suffix(Cow::Borrowed(suffix))
}

/// Embedded, within the allowlist (`window.open` stays in the web view).
const EMBED_OK: EmbedPolicy = EmbedPolicy {
    webview: Webview::Allowed,
    popups: Popups::SameWebview,
    breaks: &[],
};

/// A provider login: never in a web view.
const EMBED_LOGIN: EmbedPolicy = EmbedPolicy {
    webview: Webview::Forbidden,
    popups: Popups::SystemBrowser,
    breaks: &[Breaks::ProviderLogin],
};

const FIELDS_PAGE_BROWSER: &[SurfaceKind] = &[
    SurfaceKind::PopoverFields,
    SurfaceKind::WebviewPage,
    SurfaceKind::SystemBrowser,
];
const PAGE_BROWSER: &[SurfaceKind] = &[SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser];
const BROWSER: &[SurfaceKind] = &[SurfaceKind::SystemBrowser];

/// Where Stripe's pages leave for the system browser: a PayPal or a Link login.
const STRIPE_LEAVE: &[Origin] = &[
    exact("paypal.com"),
    suffix(".paypal.com"),
    exact("link.com"),
    suffix(".link.com"),
];

static PROVIDERS: &[ProviderSpec] = &[
    // Cards in the Payment Element (no statement against web views; Link off - it is a login),
    // SEPA in the Payment Element (or natively with the publishable key), PayPal / Wero / Apple
    // Pay outside any web view (Stripe's in-app web view table: "Not supported").
    ProviderSpec {
        id: "stripe",
        kind: ProviderKind::Processor,
        name: "Stripe",
        legal_name: "Stripe Payments Europe, Limited",
        origins: &[
            exact("checkout.stripe.com"),
            exact("hooks.stripe.com"),
            exact("js.stripe.com"),
        ],
        pages: PAY_PAGES,
        fields: Some("/fields/stripe/v1"),
        methods: &[
            MethodSpec {
                method: Method::Card,
                chain: FIELDS_PAGE_BROWSER,
                embed: EMBED_OK,
                leave: STRIPE_LEAVE,
                settles: Settles::Instant,
            },
            MethodSpec {
                method: Method::SepaDebit,
                chain: &[
                    SurfaceKind::NativeIban,
                    SurfaceKind::PopoverFields,
                    SurfaceKind::WebviewPage,
                    SurfaceKind::SystemBrowser,
                ],
                embed: EMBED_OK,
                leave: STRIPE_LEAVE,
                settles: Settles::Days,
            },
            MethodSpec {
                method: Method::PayPal,
                chain: BROWSER,
                embed: EMBED_LOGIN,
                leave: &[],
                settles: Settles::Instant,
            },
            MethodSpec {
                method: Method::ApplePay,
                chain: &[SurfaceKind::NativeSheet, SurfaceKind::SystemBrowser],
                embed: EmbedPolicy {
                    webview: Webview::Forbidden,
                    popups: Popups::SystemBrowser,
                    breaks: &[Breaks::ApplePayWeb],
                },
                leave: &[],
                settles: Settles::Instant,
            },
            // The QR page in a web view is unverified: the browser until it is tested.
            MethodSpec {
                method: Method::Wero,
                chain: BROWSER,
                embed: EmbedPolicy {
                    webview: Webview::Discouraged,
                    popups: Popups::SystemBrowser,
                    breaks: &[Breaks::BankAppSwitch],
                },
                leave: &[],
                settles: Settles::Instant,
            },
        ],
        icon: "credit_card",
        fake: false,
    },
    // SEPA Direct Debit: the Drop-in on our fields page, the hosted Billing Request Flow, the
    // browser. No statement on web views either way: the chain keeps what is tested.
    ProviderSpec {
        id: "gocardless",
        kind: ProviderKind::Processor,
        name: "GoCardless",
        legal_name: "GoCardless Ltd",
        origins: &[exact("pay.gocardless.com"), exact("pay-sandbox.gocardless.com")],
        pages: PAY_PAGES,
        fields: Some("/fields/gocardless/v1"),
        methods: &[MethodSpec {
            method: Method::SepaDebit,
            chain: FIELDS_PAGE_BROWSER,
            embed: EmbedPolicy {
                webview: Webview::Discouraged,
                popups: Popups::SystemBrowser,
                breaks: &[],
            },
            leave: &[],
            settles: Settles::Days,
        }],
        icon: "account_balance",
        fake: false,
    },
];

/// The fakes' pages: the mock token server's host.
#[cfg(feature = "fake-providers")]
const FAKE_PAGES: Origin = Origin::LoopbackHost(Cow::Borrowed("127.0.0.1"));
/// A fake provider login: the same computer under another name.
#[cfg(feature = "fake-providers")]
const FAKE_LOGIN: Origin = Origin::LoopbackHost(Cow::Borrowed("localhost"));

#[cfg(feature = "fake-providers")]
static FAKE_PROVIDERS: &[ProviderSpec] = &[
    // Stripe's shape: a fields page that takes Stripe's test card numbers, a hosted page, a
    // PayPal login on "localhost" that the card page may jump to.
    ProviderSpec {
        id: "fake-stripe",
        kind: ProviderKind::Processor,
        name: "Fake Stripe",
        legal_name: "Fake Stripe (a local test provider)",
        origins: &[FAKE_PAGES],
        pages: FAKE_PAGES,
        fields: Some("/fields/fake-stripe/v1"),
        methods: &[
            MethodSpec {
                method: Method::Card,
                chain: FIELDS_PAGE_BROWSER,
                embed: EMBED_OK,
                leave: &[FAKE_LOGIN],
                settles: Settles::Instant,
            },
            MethodSpec {
                method: Method::SepaDebit,
                chain: FIELDS_PAGE_BROWSER,
                embed: EMBED_OK,
                leave: &[FAKE_LOGIN],
                settles: Settles::Days,
            },
            MethodSpec {
                method: Method::PayPal,
                chain: BROWSER,
                embed: EMBED_LOGIN,
                leave: &[],
                settles: Settles::Instant,
            },
        ],
        icon: "credit_card",
        fake: true,
    },
    // GoCardless's shape: a hosted Billing Request Flow page taking a test IBAN.
    ProviderSpec {
        id: "fake-gocardless",
        kind: ProviderKind::Processor,
        name: "Fake GoCardless",
        legal_name: "Fake GoCardless (a local test provider)",
        origins: &[FAKE_PAGES],
        pages: FAKE_PAGES,
        fields: None,
        methods: &[MethodSpec {
            method: Method::SepaDebit,
            chain: PAGE_BROWSER,
            embed: EMBED_OK,
            leave: &[],
            settles: Settles::Days,
        }],
        icon: "account_balance",
        fake: true,
    },
    // PayPal's shape: a login and approve page, the system browser only.
    ProviderSpec {
        id: "fake-paypal",
        kind: ProviderKind::Processor,
        name: "Fake PayPal",
        legal_name: "Fake PayPal (a local test provider)",
        origins: &[FAKE_LOGIN],
        pages: FAKE_PAGES,
        fields: None,
        methods: &[MethodSpec {
            method: Method::PayPal,
            chain: BROWSER,
            embed: EMBED_LOGIN,
            leave: &[],
            settles: Settles::Instant,
        }],
        icon: "account_balance_wallet",
        fake: true,
    },
    // A merchant of record in Polar's shape: its embed / hosted page, the browser.
    ProviderSpec {
        id: "fake-mor",
        kind: ProviderKind::MerchantOfRecord {
            seller: "Fake MoR Inc.",
        },
        name: "Fake MoR",
        legal_name: "Fake MoR Inc. (a local test merchant of record)",
        origins: &[FAKE_PAGES],
        pages: FAKE_PAGES,
        fields: None,
        methods: &[MethodSpec {
            method: Method::Card,
            chain: PAGE_BROWSER,
            embed: EMBED_OK,
            leave: &[FAKE_LOGIN],
            settles: Settles::Instant,
        }],
        icon: "storefront",
        fake: true,
    },
];

#[cfg(not(feature = "fake-providers"))]
static FAKE_PROVIDERS: &[ProviderSpec] = &[];
