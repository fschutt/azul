//! What Buy storage says of azul-pay's and azcloud-kit's parts, in the window's language: a
//! payment method's name, its provider's line, the popover's chip, the checkout machine's
//! notices, an offer's price, a tier's size and price. azul-pay and azcloud-kit say them in
//! English (their tests, the command line); the words here are AzDrive's resources, the money
//! and the decimals appkit's (`l10n::money`, `l10n::decimal`).

use azcloud_kit::{azlin_contract::Cents, Tier};
use azul_appkit::l10n::{self, t, t_args, Arg};
use azul_pay::{
    machine::{ChipPage, Notice},
    offer::Price,
    Choice, Handoff, Method, OfferedProvider, PayerField, SurfaceKind,
};

/// `cents` as the window's language writes money: the currency's own decimals, the language's
/// marks and the currency where it puts it (`EUR 0.99`, `0,99 EUR`). The ONE place AzDrive turns
/// an amount into words: no other file formats cents (`money_gate` reads the sources).
#[must_use]
pub(crate) fn show(cents: &Cents) -> String {
    l10n::money(cents.amount, cents.currency.as_str())
}

/// A payment method's name (a pill, the popover's title): `Direct debit`, `Lastschrift`.
#[must_use]
pub(crate) fn method(method: Method) -> String {
    l10n::app_word(
        "AzDrive",
        &format!("pay-method-{}", method.as_str().replace('_', "-")),
        method.label(),
    )
}

/// A pill's provider line: `via Stripe`, `via Fake MoR - sold by Fake MoR Inc.`.
#[must_use]
pub(crate) fn via(provider: &OfferedProvider) -> String {
    let name = Arg::from(provider.spec.name);
    match provider.spec.kind.seller() {
        Some(seller) => t_args(
            "azdrive-pay-via-sold-by",
            &[("provider", name), ("seller", Arg::from(seller))],
        ),
        None => t_args("azdrive-pay-via", &[("provider", name)]),
    }
}

/// What the popover's page is, with its provider: `card fields by Stripe`.
#[must_use]
pub(crate) fn chip_page(page: ChipPage, provider: &str) -> String {
    let key = match page {
        ChipPage::CardFields => "azdrive-pay-chip-card-fields",
        ChipPage::DebitFields => "azdrive-pay-chip-debit-fields",
        ChipPage::Fields => "azdrive-pay-chip-fields",
        ChipPage::Page => "azdrive-pay-chip-page",
    };
    t_args(key, &[("provider", Arg::from(provider))])
}

/// The checkout machine's notice under the order (the popover's line).
#[must_use]
pub(crate) fn notice(notice: &Notice) -> String {
    let host = |key: &str, host: &str| t_args(key, &[("host", Arg::from(host))]);
    let why = |key: &str, why: &str| t_args(key, &[("why", Arg::from(l10n::t_label(why)))]);
    match notice {
        Notice::ConsentRequired => t("azdrive-pay-notice-consent"),
        Notice::NoSurface => t("azdrive-pay-notice-no-surface"),
        Notice::CreateFailed(reason) => why("azdrive-add-pay-not-prepared", reason),
        Notice::Blocked { host: h } => host("azdrive-pay-notice-blocked", h),
        Notice::LeftForBrowser { host: h } => host("azdrive-pay-notice-left-for-browser", h),
        Notice::BrowserOpened { host: h } => host("azdrive-pay-notice-browser-opened", h),
        // The provider's own words, in its language.
        Notice::ProviderError(text) => text.clone(),
        Notice::TryAnotherMethod => t("azdrive-pay-notice-try-another"),
        Notice::FieldsIncomplete => t("azdrive-pay-notice-fields-incomplete"),
        Notice::LoadFailed { host: h } => host("azdrive-pay-notice-load-failed", h),
        Notice::ProviderUnavailable { provider } => t_args(
            "azdrive-pay-notice-provider-unavailable",
            &[("provider", Arg::from(provider.as_str()))],
        ),
        Notice::Cancelled => t("azdrive-pay-notice-cancelled"),
        Notice::SettlesInDays => t("azdrive-pay-notice-settles-in-days"),
        Notice::StoppedWaiting => t("azdrive-pay-notice-stopped-waiting"),
        Notice::Declined(reason) => why("azdrive-pay-notice-declined", reason),
        Notice::NoBrowserSurface => t("azdrive-pay-notice-no-browser"),
        Notice::SurfaceRefused(reason) => why("azdrive-pay-notice-surface-refused", reason),
        Notice::WaitingForLetter => t("azdrive-pay-notice-waiting-for-letter"),
        Notice::CheckingEarlierOrder => t("azdrive-pay-notice-checking-earlier"),
        Notice::EarlierOrderPaid => t("azdrive-pay-notice-earlier-paid"),
        Notice::EarlierOrderOpen => t("azdrive-pay-notice-earlier-open"),
        Notice::IdentityIncomplete(fields) => t_args(
            "azdrive-pay-notice-identity-incomplete",
            &[("fields", Arg::from(payer_fields(fields)))],
        ),
    }
}

/// An offer's price with its VAT: `EUR 49.90, incl. 19 % VAT (EUR 7.97)`; without the VAT line
/// when the offer says none of it.
#[must_use]
pub(crate) fn price(price: &Price) -> String {
    let Some(vat) = &price.vat else {
        return show(&price.amount);
    };
    let rate = if vat.rate_permille % 10 == 0 {
        (vat.rate_permille / 10).to_string()
    } else {
        l10n::decimal(&format!(
            "{}.{}",
            vat.rate_permille / 10,
            vat.rate_permille % 10
        ))
    };
    t_args(
        if vat.included {
            "azdrive-pay-price-incl-vat"
        } else {
            "azdrive-pay-price-plus-vat"
        },
        &[
            ("amount", Arg::from(show(&price.amount))),
            ("rate", Arg::from(rate)),
            ("vat", Arg::from(show(&vat.amount))),
        ],
    )
}

/// A tier's size: `100 GB`, `1,5 TB` in German.
#[must_use]
pub(crate) fn quota(tier: &Tier) -> String {
    l10n::decimal(&tier.quota_text())
}

/// A tier's price: `EUR 0.99 a month`, with `yearly` `EUR 9.90 a year`; `None` when the server
/// names none.
#[must_use]
pub(crate) fn tier_price(tier: &Tier, yearly: bool) -> Option<String> {
    let price = tier.price(yearly)?;
    let key = if yearly {
        "azdrive-pay-a-year"
    } else {
        "azdrive-pay-a-month"
    };
    Some(t_args(key, &[("price", Arg::from(show(price)))]))
}

/// `price` for `months` months paid at once: `EUR 9.90 for 12 months` - the months paid, never a
/// rate.
#[must_use]
pub(crate) fn prepay_text(price: &Cents, months: u32) -> String {
    t_args(
        "azdrive-pay-for-months",
        &[
            ("price", Arg::from(show(price))),
            ("months", Arg::from(months)),
        ],
    )
}

/// The yearly switch's words: what a year saves against twelve months, worked out from the tier's
/// own prices (the signed ladder's), never a fixed promise (AZ-P11) - `Pay yearly (saves 16 %)`;
/// a tier whose year is not cheaper, or that names no price for one of them, says `Pay yearly`.
#[must_use]
pub(crate) fn yearly_label(tier: Option<&Tier>) -> String {
    let saved = tier.and_then(|tier| {
        let (month, year) = (tier.price_month.as_ref()?, tier.price_year.as_ref()?);
        if month.currency != year.currency {
            return None;
        }
        let twelve = u128::from(month.amount) * 12;
        let cheaper = twelve
            .checked_sub(u128::from(year.amount))
            .filter(|c| *c > 0)?;
        // Rounded down: a saving is never claimed bigger than it is.
        u32::try_from(cheaper * 100 / twelve)
            .ok()
            .filter(|percent| *percent > 0)
    });
    match saved {
        Some(percent) => t_args(
            "azdrive-add-pay-yearly-saves",
            &[("percent", Arg::from(percent))],
        ),
        None => t("azdrive-add-pay-yearly"),
    }
}

/// The payer details a method asks for, joined as the window's language joins a list: `name
/// and email address`, `Name und E-Mail-Adresse`.
#[must_use]
pub(crate) fn payer_fields(fields: &[PayerField]) -> String {
    let words: Vec<String> = fields.iter().map(|f| payer_field(*f)).collect();
    l10n::and_list(words)
}

/// One payer detail's words: `email address`, `E-Mail-Adresse`.
#[must_use]
pub(crate) fn payer_field(field: PayerField) -> String {
    l10n::app_word(
        "AzDrive",
        &format!("pay-field-{}", field.as_str().replace('_', "-")),
        field.label(),
    )
}

/// A payer detail's field label in the popover's form: `Name`, `Email address`.
#[must_use]
pub(crate) fn payer_label(field: PayerField) -> String {
    t(&format!("azdrive-pay-payer-{}", field.as_str().replace('_', "-")))
}

/// The pill's short line under its name: where the payer finishes - a code from the banking
/// app, a phone, the bank's or the wallet's website, a shop, the browser - said before the pill
/// is picked; `None` for a method paid on the page itself.
#[must_use]
pub(crate) fn pill_hint(choice: &Choice) -> Option<String> {
    let method = choice.method.method;
    let key = match method.handoff() {
        Handoff::PhoneQr => "azdrive-pay-hint-phone-qr",
        Handoff::PhonePush => "azdrive-pay-hint-phone-push",
        Handoff::BankApp => "azdrive-pay-hint-bank-app",
        Handoff::BankLogin => "azdrive-pay-hint-bank-login",
        Handoff::ProviderLogin => {
            return Some(t_args(
                "azdrive-pay-hint-provider-login",
                &[("method", Arg::from(self::method(method)))],
            ))
        }
        Handoff::StoreCash => "azdrive-pay-hint-store-cash",
        Handoff::BankTransfer => "azdrive-pay-hint-bank-transfer",
        Handoff::Post => "azdrive-pay-hint-post",
        Handoff::None if browser_only(choice) => "azdrive-pay-hint-browser",
        Handoff::None => return None,
    };
    Some(t(key))
}

/// The chosen pill's sentences, under the pills: what the method tells about the payer and who
/// asks for it, what the payer does, how long a code lives, that it opens in the browser, a
/// converted price.
#[must_use]
pub(crate) fn pill_note(choice: &Choice) -> Vec<String> {
    let offered = &choice.method;
    let method = offered.method;
    let name = Arg::from(self::method(method));
    let mut out = Vec::new();
    let identity = &offered.identity;
    let provider = Arg::from(choice.provider.spec.name);
    if method.anonymous() && identity.is_none() {
        out.push(t_args("azdrive-pay-note-anonymous", &[("method", name.clone())]));
    }
    // What the app sends with the order, then what the provider's own page asks for (a card's
    // is the card network's), then once that the files stay encrypted.
    if !identity.app_fields().is_empty() {
        out.push(t_args(
            "azdrive-pay-note-identity-app",
            &[
                ("method", name.clone()),
                ("provider", provider.clone()),
                ("fields", Arg::from(payer_fields(identity.app_fields()))),
            ],
        ));
    }
    let on_page = identity.page_fields();
    if !on_page.is_empty() {
        let fields = Arg::from(payer_fields(&on_page));
        out.push(if pays_by_card(method) {
            t_args(
                "azdrive-pay-note-identity-card",
                &[("method", name.clone()), ("fields", fields)],
            )
        } else {
            t_args(
                "azdrive-pay-note-identity-provider",
                &[("method", name.clone()), ("provider", provider.clone()), ("fields", fields)],
            )
        });
    }
    if !identity.is_none() {
        out.push(t("azdrive-pay-note-files"));
    }
    let does = match method.handoff() {
        Handoff::PhoneQr => Some("azdrive-pay-note-phone-qr"),
        Handoff::PhonePush => Some("azdrive-pay-note-phone-push"),
        Handoff::BankApp => Some("azdrive-pay-note-bank-app"),
        Handoff::BankLogin => Some("azdrive-pay-note-bank-login"),
        Handoff::ProviderLogin => Some("azdrive-pay-note-provider-login"),
        Handoff::StoreCash => Some("azdrive-pay-note-store-cash"),
        Handoff::BankTransfer => Some("azdrive-pay-note-bank-transfer"),
        Handoff::Post | Handoff::None => None,
    };
    if let Some(key) = does {
        out.push(t_args(key, &[("method", name)]));
    }
    if offered.expires_minutes > 0 {
        out.push(lifetime(offered.expires_minutes));
    }
    if browser_only(choice) {
        out.push(t("azdrive-pay-note-browser"));
    }
    if let Some(currency) = &offered.presented_in {
        out.push(t_args(
            "azdrive-pay-note-converted",
            &[
                ("provider", Arg::from(choice.provider.spec.name)),
                ("currency", Arg::from(currency.as_str())),
            ],
        ));
    }
    out
}

/// How long a method's code, QR code or voucher stays valid: in minutes up to two hours, in
/// hours up to two days, else in days.
fn lifetime(minutes: u32) -> String {
    let (key, count) = if minutes < 120 {
        ("azdrive-pay-note-valid-minutes", minutes)
    } else if minutes < 2880 {
        ("azdrive-pay-note-valid-hours", minutes / 60)
    } else {
        ("azdrive-pay-note-valid-days", minutes / 1440)
    };
    t_args(key, &[("count", Arg::from(u64::from(count)))])
}

/// The choice opens in the system browser only.
fn browser_only(choice: &Choice) -> bool {
    choice.method.surfaces == [SurfaceKind::SystemBrowser]
}

/// A method paid with a card, whose card network sees the cardholder's name.
fn pays_by_card(method: Method) -> bool {
    matches!(
        method,
        Method::Card | Method::ApplePay | Method::GooglePay | Method::KrCard
    )
}
