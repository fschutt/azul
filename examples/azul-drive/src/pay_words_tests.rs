//! Buy storage's payment words in the window's language: a tier's size and price, an offer's
//! price, a method's name, the popover's chip, the checkout machine's notices - azul-pay's
//! English words in English, German ones in German.

use azcloud_kit::Tier;
use azul_appkit::l10n::set_locale;
use azul_pay::{
    machine::{ChipPage, Notice},
    offer::Price,
    Method,
};

use crate::pay_words;

fn tier() -> Tier {
    Tier {
        id: String::from("1.5TB"),
        quota_bytes: 1_500_000_000_000,
        price_cents_month: Some(99),
        price_cents_year: Some(4990),
        currency: String::from("EUR"),
        first_month_free: false,
    }
}

fn price() -> Price {
    Price {
        amount_cents: 4990,
        currency: String::from("EUR"),
        vat_rate_permille: 190,
        vat_cents: 797,
        vat_included: true,
    }
}

/// Every notice the machine says, each with an argument where it takes one.
fn notices() -> Vec<Notice> {
    let host = || String::from("pay.example.com");
    vec![
        Notice::ConsentRequired,
        Notice::NoSurface,
        Notice::CreateFailed(String::from("no answer")),
        Notice::Blocked { host: host() },
        Notice::LeftForBrowser { host: host() },
        Notice::BrowserOpened { host: host() },
        Notice::ProviderError(String::from("Your card was declined.")),
        Notice::TryAnotherMethod,
        Notice::FieldsIncomplete,
        Notice::LoadFailed { host: host() },
        Notice::ProviderUnavailable {
            provider: String::from("Stripe"),
        },
        Notice::Cancelled,
        Notice::SettlesInDays,
        Notice::StoppedWaiting,
        Notice::Declined(String::from("insufficient funds")),
        Notice::NoBrowserSurface,
        Notice::SurfaceRefused(String::from("no surface")),
        Notice::WaitingForLetter,
    ]
}

#[test]
fn buy_storage_says_its_prices_and_payments_in_the_windows_language() {
    crate::l10n::in_english();
    let tier = tier();
    assert_eq!(pay_words::quota(&tier), "1.5 TB");
    assert_eq!(
        pay_words::tier_price(&tier, false).as_deref(),
        Some("EUR 0.99 a month")
    );
    assert_eq!(pay_words::price(&price()), price().text(), "azul-pay's own words");
    assert_eq!(pay_words::method(Method::SepaDebit), Method::SepaDebit.label());
    assert_eq!(
        pay_words::chip_page(ChipPage::CardFields, "Stripe"),
        ChipPage::CardFields.text("Stripe")
    );
    for notice in notices() {
        assert_eq!(pay_words::notice(&notice), notice.text(), "{notice:?}");
    }
    set_locale("de-DE");
    assert_eq!(pay_words::quota(&tier), "1,5 TB");
    assert_eq!(
        pay_words::tier_price(&tier, true).as_deref(),
        Some("49,90 EUR im Jahr")
    );
    assert_eq!(
        pay_words::price(&price()),
        "49,90 EUR, inkl. 19 % MwSt. (7,97 EUR)"
    );
    assert_eq!(pay_words::method(Method::SepaDebit), "Lastschrift");
    assert_eq!(
        pay_words::chip_page(ChipPage::Page, "Stripe"),
        "Zahlungsseite von Stripe"
    );
    assert_eq!(
        pay_words::notice(&Notice::Cancelled),
        "Die Zahlung wurde abgebrochen; es wurde nichts berechnet."
    );
    for notice in notices() {
        // The provider's own message stays the provider's.
        let own = matches!(notice, Notice::ProviderError(_));
        let said = pay_words::notice(&notice);
        assert!(
            own || (!said.contains("azdrive-") && said != notice.text()),
            "{notice:?}: {said}"
        );
    }
    set_locale("en-US");
}
