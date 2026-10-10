//! What Buy storage says of azul-pay's and azcloud-kit's parts, in the window's language: a
//! payment method's name, its provider's line, the popover's chip, the checkout machine's
//! notices, an offer's price, a tier's size and price. azul-pay and azcloud-kit say them in
//! English (their tests, the command line); the words here are AzDrive's resources, the money
//! and the decimals appkit's (`l10n::money`, `l10n::decimal`).

use azcloud_kit::Tier;
use azul_appkit::l10n::{self, t, t_args, Arg};
use azul_pay::{
    machine::{ChipPage, Notice},
    offer::Price,
    Method, OfferedProvider,
};

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
    }
}

/// An offer's price with its VAT: `EUR 49.90, incl. 19 % VAT (EUR 7.97)`.
#[must_use]
pub(crate) fn price(price: &Price) -> String {
    let rate = if price.vat_rate_permille % 10 == 0 {
        (price.vat_rate_permille / 10).to_string()
    } else {
        l10n::decimal(&format!(
            "{}.{}",
            price.vat_rate_permille / 10,
            price.vat_rate_permille % 10
        ))
    };
    t_args(
        if price.vat_included {
            "azdrive-pay-price-incl-vat"
        } else {
            "azdrive-pay-price-plus-vat"
        },
        &[
            (
                "amount",
                Arg::from(l10n::money(price.amount_cents, &price.currency)),
            ),
            ("rate", Arg::from(rate)),
            (
                "vat",
                Arg::from(l10n::money(price.vat_cents, &price.currency)),
            ),
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
    let (cents, key) = if yearly {
        (tier.price_cents_year?, "azdrive-pay-a-year")
    } else {
        (tier.price_cents_month?, "azdrive-pay-a-month")
    };
    Some(t_args(
        key,
        &[("price", Arg::from(l10n::money(cents, &tier.currency)))],
    ))
}
