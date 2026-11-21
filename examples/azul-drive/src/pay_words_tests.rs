//! Buy storage's payment words in the window's language: a tier's size and price, an offer's
//! price, a method's name, the popover's chip, the checkout machine's notices - azul-pay's
//! English words in English, German ones in German.

use azcloud_kit::{
    azlin_contract::{Cents, Currency},
    Tier,
};
use azul_appkit::l10n::set_locale;
use azul_pay::{
    machine::{ChipPage, Notice},
    offer::{Price, Vat},
    Method,
};

use crate::pay_words;

/// `amount` in the smallest unit of `currency`.
pub(crate) fn cents(amount: u64, code: &str) -> Cents {
    Cents {
        amount,
        currency: currency(code),
    }
}

/// The currency `code`.
pub(crate) fn currency(code: &str) -> Currency {
    Currency::parse(code).expect("a currency")
}

fn tier() -> Tier {
    Tier {
        id: String::from("1.5TB"),
        quota_bytes: 1_500_000_000_000,
        price_month: Some(cents(99, "EUR")),
        price_year: Some(cents(4990, "EUR")),
        currency: currency("EUR"),
        first_month_free: false,
        yearly_only: false,
        prepay: azcloud_kit::azlin_contract::Prepay::new(
            azcloud_kit::azlin_contract::Months::ALL.to_vec(),
        )
        .expect("every period"),
        methods: Vec::new(),
    }
}

fn price() -> Price {
    Price {
        amount: cents(4990, "EUR"),
        vat: Some(Vat {
            rate_permille: 190,
            amount: cents(797, "EUR"),
            included: true,
        }),
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
        Notice::CheckingEarlierOrder,
        Notice::EarlierOrderPaid,
        Notice::EarlierOrderOpen,
        Notice::IdentityIncomplete(vec![
            azul_pay::PayerField::Name,
            azul_pay::PayerField::Email,
            azul_pay::PayerField::NationalId,
        ]),
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
    assert_eq!(
        pay_words::price(&price()),
        "EUR 49.90, incl. 19 % VAT (EUR 7.97)"
    );
    assert_eq!(
        pay_words::method(Method::SepaDebit),
        Method::SepaDebit.label()
    );
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

/// Every method the app knows has its pill words in English and in German (built from the wire
/// name, so the source scan of l10n_tests cannot see them).
#[test]
fn every_method_has_its_words_in_english_and_german() {
    let ftl = |lang: &str| {
        std::fs::read_to_string(format!("{}/resources/{lang}.ftl", env!("CARGO_MANIFEST_DIR")))
            .expect("the ftl file")
    };
    for lang in ["en", "de"] {
        let text = ftl(lang);
        for method in Method::ALL {
            let key = format!("azdrive-pay-method-{} =", method.as_str().replace('_', "-"));
            assert!(text.contains(&key), "{lang}: no {key}");
        }
    }
}

// ==== The pills in the payer's country: where the payer finishes, what a method tells ====

/// The choice of `method` in a live offer of Stripe's whose method is described by `extra`.
fn choice_of(method: Method, extra: serde_json::Value) -> azul_pay::Choice {
    let mut described = serde_json::json!({"method": method.as_str()});
    if let (Some(m), Some(e)) = (described.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            m.insert(k.clone(), v.clone());
        }
    }
    let provider = if method == Method::Cash { "cash" } else { "stripe" };
    let offer = azul_pay::Offer::parse(
        &serde_json::json!({"offers": [{"provider": provider, "methods": [described]}]})
            .to_string(),
        &azul_pay::OfferContext::for_token_url("https://token.azlin.io"),
    )
    .unwrap();
    let pill = azul_pay::Pill {
        method,
        providers: vec![0],
        chosen: 0,
    };
    let ctx = azul_pay::PillContext {
        country: "PL",
        currency: "PLN",
        months: 1,
        recurring: false,
        surfaces: crate::add_drive::PAY_SURFACES,
    };
    azul_pay::Choice::of(&offer, &pill, &ctx).expect("a choice")
}

#[test]
fn a_pill_says_where_the_payer_finishes_before_it_is_picked() {
    crate::l10n::in_english();
    let none = serde_json::json!({});
    for (method, hint) in [
        (Method::Blik, Some("Code from your banking app")),
        (Method::Twint, Some("Scan with your phone")),
        (Method::MbWay, Some("Confirm in your phone's app")),
        (Method::Ideal, Some("Opens your bank's website")),
        (Method::PayPal, Some("Opens PayPal's website")),
        (Method::Multibanco, Some("Pay at a shop or ATM")),
        (Method::Card, None),
        (Method::SepaDebit, None),
    ] {
        assert_eq!(
            pay_words::pill_hint(&choice_of(method, none.clone())).as_deref(),
            hint,
            "{method:?}"
        );
    }
    set_locale("de-DE");
    assert_eq!(
        pay_words::pill_hint(&choice_of(Method::Blik, none)).as_deref(),
        Some("Code aus der Banking-App")
    );
    assert_eq!(pay_words::method(Method::BankTransfer), "Überweisung");
    set_locale("en-US");
}

#[test]
fn a_pills_note_says_what_the_method_tells_about_the_payer() {
    crate::l10n::in_english();
    let said = |choice: &azul_pay::Choice| pay_words::pill_note(choice).join(" ");
    let files = "Your files stay encrypted and are not linked to the payment.";
    let card = choice_of(Method::Card, serde_json::json!({"needs_identity": ["name"]}));
    assert_eq!(
        said(&card),
        format!("Card: the card network sees your name. {files}")
    );
    let cash = choice_of(Method::Cash, serde_json::json!({"surfaces": ["paper"]}));
    assert!(said(&cash)
        .contains("Cash by post needs nothing about you: no name, no account, no email address."));
    let asked = choice_of(
        Method::Card,
        serde_json::json!({"needs_identity": ["name", "email", "national_id"],
                           "identity_in_app": ["name", "email", "national_id"]}),
    );
    assert!(said(&asked).starts_with(
        "Card: Stripe needs your name, email address and national ID number with the order. \
         AzDrive sends them once and does not keep them. Your files stay encrypted"
    ));
    // EBANX's shape: the name and the email in the merchant's request, the tax number on its page.
    let split = choice_of(
        Method::Boleto,
        serde_json::json!({"needs_identity": ["name", "email", "tax_id"],
                           "identity_in_app": ["name", "email"]}),
    );
    let text = said(&split);
    assert!(
        text.contains("Boleto: Stripe needs your name and email address with the order.")
            && text.contains("Boleto: Stripe asks for your tax number on its own page.")
            && text.matches(files).count() == 1,
        "{text}"
    );
    let blik = choice_of(Method::Blik, serde_json::json!({"expires_minutes": 2}));
    assert!(said(&blik).contains("2 minutes"), "{}", said(&blik));
    assert!(!said(&blik).contains(files), "BLIK tells nothing the server names");
    let p24 = choice_of(Method::P24, serde_json::json!({}));
    assert!(said(&p24).contains("opens in your browser"), "{}", said(&p24));
    set_locale("de-DE");
    let german = said(&card);
    assert!(german.contains("Kartennetz") && german.contains("verschlüsselt"), "{german}");
    set_locale("en-US");
}

#[test]
fn prices_in_the_buyers_currency_keep_its_decimals() {
    crate::l10n::in_english();
    let yen = Tier {
        currency: currency("JPY"),
        price_month: Some(cents(160, "JPY")),
        price_year: Some(cents(1_600, "JPY")),
        ..tier()
    };
    assert_eq!(pay_words::tier_price(&yen, false).as_deref(), Some("JPY 160 a month"));
    let zloty = Price {
        amount: cents(449, "PLN"),
        vat: Some(Vat {
            rate_permille: 230,
            amount: cents(84, "PLN"),
            included: true,
        }),
    };
    assert_eq!(pay_words::price(&zloty), "PLN 4.49, incl. 23 % VAT (PLN 0.84)");
    set_locale("de-DE");
    assert_eq!(pay_words::price(&zloty), "4,49 PLN, inkl. 23 % MwSt. (0,84 PLN)");
    set_locale("en-US");
}

/// X-DELTA-AZDRIVE-MONEY-9 (and X-AR5b-18): the formatter writes the decimals of the contract's ONE
/// table for every currency - the seventeen whole-unit ones (the CFA franc, the Ugandan shilling,
/// the guaraní among them: the widget's own table knew five), the seven thousandths, two for the
/// rest. Before, "XOF 50.00" stood for a charge of 5000 whole units.
#[test]
fn x_delta_azdrive_money_9_every_currency_is_written_with_its_own_decimals() {
    crate::l10n::in_english();
    let none = [
        "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "MGA", "PYG", "RWF", "UGX", "VND",
        "VUV", "XAF", "XOF", "XPF",
    ];
    for code in none {
        assert_eq!(
            pay_words::show(&cents(160, code)),
            format!("{code} 160"),
            "{code}"
        );
        assert_eq!(
            pay_words::show(&cents(5_000, code)),
            format!("{code} 5,000"),
            "{code}: 5000 whole units, not 50.00"
        );
    }
    for code in ["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"] {
        assert_eq!(
            pay_words::show(&cents(1_234, code)),
            format!("{code} 1.234"),
            "{code}"
        );
    }
    for code in ["EUR", "USD", "PLN", "GBP", "CHF", "BRL", "SEK"] {
        assert_eq!(
            pay_words::show(&cents(99, code)),
            format!("{code} 0.99"),
            "{code}"
        );
    }
    // The slip's amount in words reads the same table (azul-pay's `minor_digits` is the contract's).
    for code in none {
        assert_eq!(azul_pay::money::minor_digits(code), 0, "{code}");
    }
}

/// X-AR5b-18: one formatter writes money, and it writes the currency's own decimals - none for
/// the yen, three for the Kuwaiti dinar - in the language's marks, not two decimals for all.
#[test]
fn x_ar5b_18_money_is_written_with_the_currencys_own_decimals() {
    crate::l10n::in_english();
    assert_eq!(pay_words::show(&cents(99, "EUR")), "EUR 0.99");
    assert_eq!(pay_words::show(&cents(160, "JPY")), "JPY 160");
    assert_eq!(pay_words::show(&cents(1_234, "KWD")), "KWD 1.234");
    assert_eq!(pay_words::show(&cents(123_456, "EUR")), "EUR 1,234.56");
    // A price without a VAT line says the amount alone (never "0 %").
    let mut plain = price();
    plain.vat = None;
    assert_eq!(pay_words::price(&plain), "EUR 49.90");
    set_locale("de-DE");
    assert_eq!(pay_words::show(&cents(123_456, "EUR")), "1.234,56 EUR");
    assert_eq!(pay_words::show(&cents(160, "JPY")), "160 JPY");
    set_locale("en-US");
}
