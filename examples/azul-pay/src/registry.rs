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

/// How a payer pays: the pill. The wire names are the payment methods registry's (azul-apps
/// `azul-pay-methods`), so the token server and the app name a method the same way. The app knows
/// the methods its providers can carry; a method it does not know is never shown (adding one is an
/// app update, like adding a provider).
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
    /// Cash in an envelope with the printed slip (cash contract v1): the operator activates the
    /// checkout when the letter arrived.
    Cash,
    GooglePay,
    /// Stripe's saved-payment login.
    Link,
    AmazonPay,
    Klarna,
    /// ACH Direct Debit (the United States).
    UsBankAccount,
    /// Bacs Direct Debit (the United Kingdom).
    BacsDebit,
    /// Autogiro (Sweden).
    Autogiro,
    /// Betalingsservice (Denmark).
    Betalingsservice,
    /// BECS Direct Debit (Australia).
    BecsDebit,
    /// BECS Direct Debit (New Zealand).
    BecsNzDebit,
    /// Pre-authorized debit (Canada).
    PadDebit,
    /// iDEAL (the Netherlands): a login at the payer's bank.
    Ideal,
    Bancontact,
    Eps,
    /// Przelewy24 (Poland).
    P24,
    /// BLIK (Poland): a six-digit code from the banking app.
    Blik,
    Twint,
    Swish,
    MobilePay,
    Vipps,
    MbWay,
    Multibanco,
    Bizum,
    Satispay,
    RevolutPay,
    /// Pay by Bank (the United Kingdom).
    PayByBank,
    Upi,
    Pix,
    Boleto,
    Alipay,
    WechatPay,
    /// A Korean card through the Korean card networks' own page.
    KrCard,
    KakaoPay,
    NaverPay,
    Payco,
    SamsungPay,
    /// Konbini (Japan): paid in cash at a convenience store.
    Konbini,
    PayPay,
    Oxxo,
    PayNow,
    PromptPay,
    Fpx,
    GrabPay,
}

/// What the payer does to pay with a method (the registry's `flow`): the popover's words and the
/// pill's help follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Flow {
    /// Types card or account details into fields.
    Fields,
    /// Goes to another site (a bank, a wallet) and comes back.
    Redirect,
    /// Scans a QR code with a phone.
    Qr,
    /// Types a short code that an app on the phone shows.
    Code,
    /// Confirms a request the phone app receives.
    Push,
    /// Pays a reference later, in a shop or at a bank.
    Voucher,
    /// Gives a mandate for a direct debit.
    Debit,
    /// Sends a bank transfer.
    Transfer,
    /// Posts a printed slip with cash (Azlin's own).
    Paper,
}

/// Where the payer finishes paying (the registry's `handoff`): what a pill says before it is
/// picked - a phone app, a bank's website, a shop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handoff {
    /// In the payment page itself.
    None,
    /// A QR code scanned with a phone app.
    PhoneQr,
    /// A request confirmed in a phone app.
    PhonePush,
    /// A code read from the banking app.
    BankApp,
    /// A login at the payer's bank.
    BankLogin,
    /// A login at the wallet's provider (PayPal, Alipay).
    ProviderLogin,
    /// Cash paid in a shop with a reference.
    StoreCash,
    /// A transfer made from the payer's bank.
    BankTransfer,
    /// A letter with cash.
    Post,
}

impl Handoff {
    /// The wire name (`phone_qr`, `bank_login`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Handoff::None => "none",
            Handoff::PhoneQr => "phone_qr",
            Handoff::PhonePush => "phone_push",
            Handoff::BankApp => "bank_app",
            Handoff::BankLogin => "bank_login",
            Handoff::ProviderLogin => "provider_login",
            Handoff::StoreCash => "store_cash",
            Handoff::BankTransfer => "bank_transfer",
            Handoff::Post => "post",
        }
    }

    /// The payer needs a phone app to finish.
    #[must_use]
    pub const fn needs_phone(self) -> bool {
        matches!(self, Handoff::PhoneQr | Handoff::PhonePush | Handoff::BankApp)
    }

    /// The payer logs in somewhere else (a bank, a wallet): the page leaves for another site.
    #[must_use]
    pub const fn leaves(self) -> bool {
        matches!(self, Handoff::BankLogin | Handoff::ProviderLogin)
    }
}

impl Flow {
    /// The wire name (`fields`, `redirect`, `qr`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Flow::Fields => "fields",
            Flow::Redirect => "redirect",
            Flow::Qr => "qr",
            Flow::Code => "code",
            Flow::Push => "push",
            Flow::Voucher => "voucher",
            Flow::Debit => "debit",
            Flow::Transfer => "transfer",
            Flow::Paper => "paper",
        }
    }
}

/// One row of the method table: what [`Method`]'s accessors read.
struct MethodRow {
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    flow: Flow,
    handoff: Handoff,
}

const fn row(
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    flow: Flow,
    handoff: Handoff,
) -> MethodRow {
    MethodRow {
        id,
        label,
        icon,
        flow,
        handoff,
    }
}

impl Method {
    /// Every method, in the order the app lists them when nothing else orders them.
    pub const ALL: [Method; 51] = [
        Method::SepaDebit,
        Method::Card,
        Method::ApplePay,
        Method::PayPal,
        Method::Wero,
        Method::BankTransfer,
        Method::Voucher,
        Method::Cash,
        Method::GooglePay,
        Method::Link,
        Method::AmazonPay,
        Method::Klarna,
        Method::UsBankAccount,
        Method::BacsDebit,
        Method::Autogiro,
        Method::Betalingsservice,
        Method::BecsDebit,
        Method::BecsNzDebit,
        Method::PadDebit,
        Method::Ideal,
        Method::Bancontact,
        Method::Eps,
        Method::P24,
        Method::Blik,
        Method::Twint,
        Method::Swish,
        Method::MobilePay,
        Method::Vipps,
        Method::MbWay,
        Method::Multibanco,
        Method::Bizum,
        Method::Satispay,
        Method::RevolutPay,
        Method::PayByBank,
        Method::Upi,
        Method::Pix,
        Method::Boleto,
        Method::Alipay,
        Method::WechatPay,
        Method::KrCard,
        Method::KakaoPay,
        Method::NaverPay,
        Method::Payco,
        Method::SamsungPay,
        Method::Konbini,
        Method::PayPay,
        Method::Oxxo,
        Method::PayNow,
        Method::PromptPay,
        Method::Fpx,
        Method::GrabPay,
    ];

    /// The table's row: the wire name, the English words, the icon, the flow and the handoff
    /// (as the payment methods registry has them, read 2026-10-17).
    const fn row(self) -> MethodRow {
        use Flow as F;
        use Handoff as H;
        match self {
            Method::SepaDebit => row("sepa_debit", "Direct debit", "account_balance", F::Debit, H::None),
            Method::Card => row("card", "Card", "credit_card", F::Fields, H::None),
            Method::ApplePay => row("apple_pay", "Apple Pay", "phone_iphone", F::Fields, H::None),
            Method::PayPal => row("paypal", "PayPal", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::Wero => row("wero", "Wero", "smartphone", F::Qr, H::PhoneQr),
            Method::BankTransfer => row("bank_transfer", "Bank transfer", "receipt_long", F::Transfer, H::BankTransfer),
            Method::Voucher => row("voucher", "Voucher", "redeem", F::Fields, H::None),
            Method::Cash => row("cash", "Cash by post", "mail", F::Paper, H::Post),
            Method::GooglePay => row("google_pay", "Google Pay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::Link => row("link", "Link", "link", F::Redirect, H::ProviderLogin),
            Method::AmazonPay => row("amazon_pay", "Amazon Pay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::Klarna => row("klarna", "Klarna", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::UsBankAccount => row("us_bank_account", "ACH Direct Debit", "account_balance", F::Debit, H::BankLogin),
            Method::BacsDebit => row("bacs_debit", "Bacs Direct Debit", "account_balance", F::Debit, H::None),
            Method::Autogiro => row("autogiro", "Autogiro", "account_balance", F::Debit, H::None),
            Method::Betalingsservice => row("betalingsservice", "Betalingsservice", "account_balance", F::Debit, H::None),
            Method::BecsDebit => row("becs_debit", "BECS Direct Debit", "account_balance", F::Debit, H::None),
            Method::BecsNzDebit => row("becs_nz_debit", "BECS Direct Debit (New Zealand)", "account_balance", F::Debit, H::None),
            Method::PadDebit => row("pad_debit", "Pre-authorized debit", "account_balance", F::Debit, H::None),
            Method::Ideal => row("ideal", "iDEAL", "account_balance", F::Redirect, H::BankLogin),
            Method::Bancontact => row("bancontact", "Bancontact", "account_balance", F::Redirect, H::BankLogin),
            Method::Eps => row("eps", "EPS", "account_balance", F::Redirect, H::BankLogin),
            Method::P24 => row("p24", "Przelewy24", "account_balance", F::Redirect, H::BankLogin),
            Method::Blik => row("blik", "BLIK", "dialpad", F::Code, H::BankApp),
            Method::Twint => row("twint", "TWINT", "qr_code", F::Qr, H::PhoneQr),
            Method::Swish => row("swish", "Swish", "qr_code", F::Qr, H::PhoneQr),
            Method::MobilePay => row("mobilepay", "MobilePay", "smartphone", F::Push, H::PhonePush),
            Method::Vipps => row("vipps", "Vipps", "smartphone", F::Push, H::PhonePush),
            Method::MbWay => row("mb_way", "MB WAY", "smartphone", F::Push, H::PhonePush),
            Method::Multibanco => row("multibanco", "Multibanco", "receipt_long", F::Voucher, H::StoreCash),
            Method::Bizum => row("bizum", "Bizum", "smartphone", F::Push, H::PhonePush),
            Method::Satispay => row("satispay", "Satispay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::RevolutPay => row("revolut_pay", "Revolut Pay", "qr_code", F::Qr, H::PhoneQr),
            Method::PayByBank => row("pay_by_bank", "Pay by Bank", "account_balance", F::Redirect, H::BankLogin),
            Method::Upi => row("upi", "UPI", "qr_code", F::Qr, H::PhoneQr),
            Method::Pix => row("pix", "Pix", "qr_code", F::Qr, H::PhoneQr),
            Method::Boleto => row("boleto", "Boleto", "receipt_long", F::Voucher, H::StoreCash),
            Method::Alipay => row("alipay", "Alipay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::WechatPay => row("wechat_pay", "WeChat Pay", "qr_code", F::Qr, H::PhoneQr),
            Method::KrCard => row("kr_card", "Korean card", "credit_card", F::Redirect, H::BankLogin),
            Method::KakaoPay => row("kakao_pay", "Kakao Pay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::NaverPay => row("naver_pay", "Naver Pay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::Payco => row("payco", "PAYCO", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::SamsungPay => row("samsung_pay", "Samsung Pay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::Konbini => row("konbini", "Konbini", "storefront", F::Voucher, H::StoreCash),
            Method::PayPay => row("paypay", "PayPay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
            Method::Oxxo => row("oxxo", "OXXO", "storefront", F::Voucher, H::StoreCash),
            Method::PayNow => row("paynow", "PayNow", "qr_code", F::Qr, H::PhoneQr),
            Method::PromptPay => row("promptpay", "PromptPay", "qr_code", F::Qr, H::PhoneQr),
            Method::Fpx => row("fpx", "FPX", "account_balance", F::Redirect, H::BankLogin),
            Method::GrabPay => row("grabpay", "GrabPay", "account_balance_wallet", F::Redirect, H::ProviderLogin),
        }
    }

    /// The wire name (`sepa_debit`, `card`, `blik`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.row().id
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

    /// The pill's words in English (an app says them in its own language by the wire name).
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.row().label
    }

    /// The pill's icon: a Material icon name the app ships (never loaded from the network).
    #[must_use]
    pub const fn icon(self) -> &'static str {
        self.row().icon
    }

    /// What the payer does.
    #[must_use]
    pub const fn flow(self) -> Flow {
        self.row().flow
    }

    /// Where the payer finishes.
    #[must_use]
    pub const fn handoff(self) -> Handoff {
        self.row().handoff
    }

    /// Paid without telling anyone who pays: cash by post and Azlin's vouchers. Every other
    /// method passes through a bank, a card network or a wallet that knows the payer.
    #[must_use]
    pub const fn anonymous(self) -> bool {
        matches!(self, Method::Cash | Method::Voucher)
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
    /// Paper: the app prints the slip (and the buyer's copy); nothing opens. Cash by post.
    Paper,
}

impl SurfaceKind {
    /// Every surface.
    pub const ALL: [SurfaceKind; 7] = [
        SurfaceKind::NativeSheet,
        SurfaceKind::NativeIban,
        SurfaceKind::NativeVoucher,
        SurfaceKind::PopoverFields,
        SurfaceKind::WebviewPage,
        SurfaceKind::SystemBrowser,
        SurfaceKind::Paper,
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
            SurfaceKind::Paper => "paper",
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
    /// The money comes by post (cash in an envelope): days or weeks; the app never waits in
    /// the dialog and looks once a day.
    Post,
}

impl Settles {
    /// The wire name: `instant`, `days`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Settles::Instant => "instant",
            Settles::Days => "days",
            Settles::Post => "post",
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
/// A slip on paper, nothing else.
const PAPER: &[SurfaceKind] = &[SurfaceKind::Paper];

/// Nothing of the provider is ever shown in a web view (nor anywhere else: it is paper).
const EMBED_NONE: EmbedPolicy = EmbedPolicy {
    webview: Webview::Forbidden,
    popups: Popups::None,
    breaks: &[],
};
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
    // Cash by post (cash contract v1): Azlin sells, the buyer prints a slip and posts it with
    // the cash; the operator activates the checkout when the letter arrived. No page anywhere:
    // no web view, no browser, no origins.
    ProviderSpec {
        id: "cash",
        kind: ProviderKind::Processor,
        name: "Azlin",
        legal_name: "Azlin",
        origins: &[],
        pages: PAY_PAGES,
        fields: None,
        methods: &[MethodSpec {
            method: Method::Cash,
            chain: PAPER,
            embed: EMBED_NONE,
            leave: &[],
            settles: Settles::Post,
        }],
        icon: "mail",
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
