//! The registry's rules, checked on the shipped data and on small hand-made tables that break one
//! rule each.

use crate::*;

/// The smallest registry that keeps every rule: one provider, a card and a bank login, a default
/// country. Each test breaks it in one place.
const TINY: &str = r#"
format = 2
read_on = "2026-10-17"

[[provider]]
id = "p"
name = "Provider"
kind = "processor"
onboarding = "self_serve"
merchant_countries = ["*"]
origins = ["pay.example"]
webhook = "stripe_signature"
payout = "EUR"
docs = "https://pay.example/docs"
sources = ["https://pay.example/docs"]

[[method]]
id = "card"
name = "Card"
family = "card"
flow = "fields"
handoff = "none"
countries = ["*"]
currencies = ["*"]

[[method]]
id = "bank"
name = "Bank"
family = "bank_redirect"
flow = "redirect"
handoff = "bank_login"
countries = ["NL"]
currencies = ["EUR"]

[[route]]
method = "card"
provider = "p"
provider_type = "card"
status = "ga"
merchant_countries = []
currencies = ["*"]
adaptive = true
adaptive_recurring = true
recurring = "mandate"
recurring_cap = ""
min = ""
max = ""
expires_minutes = 0
settles = "instant"
surfaces = ["fields", "page", "browser"]
webview = "allowed"
fee_known = true
fee_bps = 150
fee_fixed = "EUR 0.25"
fee_cap = ""
fee_note = ""
refund_days = 180
disputes = "chargeback"
payer_data = []
test_inputs = [
  { field = "card_number", value = "4242424242424242", outcome = "succeeded", note = "Visa", after_minutes = 0 },
]
sources = ["https://pay.example/docs"]
read_on = "2026-10-17"
note = ""
request_data = []

[[route]]
method = "bank"
provider = "p"
provider_type = "bank"
status = "ga"
merchant_countries = []
currencies = ["EUR"]
adaptive = false
adaptive_recurring = false
recurring = "none"
recurring_cap = ""
min = "EUR 0.50"
max = "EUR 100.00"
expires_minutes = 0
settles = "instant"
surfaces = ["browser"]
webview = "forbidden"
fee_known = true
fee_bps = 0
fee_fixed = "EUR 0.29"
fee_cap = ""
fee_note = ""
refund_days = 180
disputes = "none"
payer_data = []
test_inputs = []
sources = ["https://pay.example/docs"]
read_on = "2026-10-17"
note = ""
request_data = ["name", "email"]

[[country]]
code = "*"
name = "Any other country"
currency = ""
mix = {}
mix_year = 0
prefer = ["card"]
sources = []

[[country]]
code = "NL"
name = "Netherlands"
currency = "EUR"
mix = { wallets = 11, a2a = 67, bnpl = 10, crypto = 0, credit = 8, debit = 3, cash = 2 }
mix_year = 2024
prefer = ["bank", "card"]
sources = ["https://example.org/report"]
"#;

fn tiny_with(from: &str, to: &str) -> String {
    assert!(TINY.contains(from), "the test table has no `{from}`");
    TINY.replacen(from, to, 1)
}

fn refused(text: &str, why: &str) {
    match Registry::parse(text) {
        Ok(_) => panic!("a registry that breaks the rule ({why}) was accepted"),
        Err(e) => assert!(
            e.problems.iter().any(|p| p.contains(why)),
            "the refusal does not say `{why}`: {:?}",
            e.problems
        ),
    }
}

fn eur(minor: i64) -> Amount {
    Amount::new("EUR", minor)
}

fn builtin() -> Registry {
    match Registry::builtin() {
        Ok(r) => r,
        Err(e) => panic!("the shipped registry breaks its rules: {:?}", e.problems),
    }
}

fn route<'a>(r: &'a Registry, method: &str, provider: &str) -> &'a Route {
    match r.routes_of(method).find(|x| x.provider == provider) {
        Some(x) => x,
        None => panic!("no route {method} via {provider}"),
    }
}

#[test]
fn the_shipped_registry_parses_and_keeps_every_rule() {
    let r = builtin();
    assert_eq!(r.format, FORMAT);
    assert!(r.providers.len() >= 10 && r.methods.len() >= 80 && r.routes.len() >= 150);
    assert!(r.method("blik").is_some() && r.provider("stripe").is_some());
    assert_eq!(route(&r, "blik", "stripe").expires_minutes, 2);
}

#[test]
fn the_small_table_is_valid() {
    let r = match Registry::parse(TINY) {
        Ok(r) => r,
        Err(e) => panic!("the test table breaks a rule: {:?}", e.problems),
    };
    assert_eq!(r.routes.len(), 2);
}

#[test]
fn a_bank_login_in_the_webview_is_refused() {
    refused(
        &tiny_with(r#"surfaces = ["browser"]
webview = "forbidden""#, r#"surfaces = ["page", "browser"]
webview = "allowed""#),
        "login",
    );
}

#[test]
fn a_page_where_the_provider_forbids_webviews_is_refused() {
    let card_page = tiny_with(r#"surfaces = ["fields", "page", "browser"]
webview = "allowed""#, r#"surfaces = ["fields", "page", "browser"]
webview = "forbidden""#);
    refused(&card_page, "webview");
}

#[test]
fn a_provider_without_origins_cannot_show_a_page() {
    refused(&tiny_with(r#"origins = ["pay.example"]"#, "origins = []"), "origins");
}

#[test]
fn a_route_naming_an_unknown_method_is_refused() {
    refused(&tiny_with(r#"method = "bank"
provider = "p""#, r#"method = "telepathy"
provider = "p""#), "unknown method");
}

#[test]
fn a_route_naming_an_unknown_provider_is_refused() {
    refused(&tiny_with(r#"method = "bank"
provider = "p""#, r#"method = "bank"
provider = "q""#), "unknown provider");
}

#[test]
fn the_same_method_twice_through_one_provider_is_refused() {
    refused(&tiny_with(r#"method = "bank"
provider = "p""#, r#"method = "card"
provider = "p""#), "twice");
}

#[test]
fn a_method_nobody_carries_is_refused() {
    let lonely = TINY.replacen(
        "[[route]]",
        "[[method]]\nid = \"orphan\"\nname = \"Orphan\"\nfamily = \"wallet\"\nflow = \"qr\"\nhandoff = \"phone_qr\"\ncountries = [\"*\"]\ncurrencies = [\"*\"]\n\n[[route]]",
        1,
    );
    refused(&lonely, "no route");
}

#[test]
fn a_country_order_naming_an_unknown_method_is_refused() {
    refused(&tiny_with(r#"prefer = ["bank", "card"]"#, r#"prefer = ["bank", "cheque"]"#), "unknown method");
}

#[test]
fn test_data_without_a_success_is_refused() {
    refused(&tiny_with(r#"outcome = "succeeded""#, r#"outcome = "failed""#), "succeeded");
}

#[test]
fn a_mix_far_from_one_hundred_percent_is_refused() {
    refused(&tiny_with("a2a = 67", "a2a = 7"), "mix");
}

#[test]
fn an_unknown_value_is_refused() {
    assert!(Registry::parse(&tiny_with(r#"flow = "redirect""#, r#"flow = "telepathy""#)).is_err());
}

#[test]
fn a_missing_key_is_refused() {
    assert!(Registry::parse(&tiny_with("fee_note = \"\"\n", "")).is_err());
}

#[test]
fn an_unknown_fee_has_no_price_in_it() {
    refused(&tiny_with("fee_known = true\nfee_bps = 150", "fee_known = false\nfee_bps = 150"), "fee");
}

#[test]
fn a_badly_formed_code_is_refused() {
    refused(&tiny_with(r#"code = "NL""#, r#"code = "Netherlands""#), "country code");
    refused(&tiny_with(r#"currencies = ["EUR"]
adaptive = false"#, r#"currencies = ["euro"]
adaptive = false"#), "currency");
}

#[test]
fn a_recurring_conversion_needs_the_one_off_conversion() {
    refused(&tiny_with("adaptive = true\nadaptive_recurring = true", "adaptive = false\nadaptive_recurring = true"), "adaptive");
}

#[test]
fn amounts_are_exact_in_their_currency() {
    assert_eq!(Amount::parse("EUR 0.25"), Ok(eur(25)));
    assert_eq!(Amount::parse("EUR 5000"), Ok(eur(500_000)));
    assert_eq!(Amount::parse("BRL 49999.99"), Ok(Amount::new("BRL", 4_999_999)));
    assert_eq!(Amount::parse("JPY 300000"), Ok(Amount::new("JPY", 300_000)));
    assert_eq!(Amount::parse("KWD 1.234"), Ok(Amount::new("KWD", 1_234)));
    for bad in ["JPY 1.5", "EUR 0.255", "eur 1", "EUR", "EUR -1", "EUR 1,50", "", "EUR 1e3", "EUR 99999999999999999999"] {
        assert!(Amount::parse(bad).is_err(), "{bad:?} was taken as an amount");
    }
}

#[test]
fn an_amount_with_more_decimals_than_its_currency_has_is_refused_in_the_table() {
    refused(&tiny_with(r#"min = "EUR 0.50""#, r#"min = "EUR 0.505""#), "amount");
}

#[test]
fn the_fee_of_blik_on_2_49_eur_is_29_cents() {
    let r = builtin();
    assert_eq!(route(&r, "blik", "stripe").fee(&eur(249)), Some(29));
}

#[test]
fn a_capped_fee_stops_at_its_cap() {
    let r = builtin();
    assert_eq!(route(&r, "swish", "stripe").fee(&eur(8_649)), Some(70));
    assert_eq!(route(&r, "sepa_debit", "gocardless").fee(&eur(8_649)), Some(100));
    assert_eq!(route(&r, "sepa_debit", "gocardless").fee(&eur(99)), Some(21));
}

#[test]
fn a_percent_only_fee_has_no_fixed_part() {
    let r = builtin();
    assert_eq!(route(&r, "pix", "stripe").fee(&eur(99)), Some(2));
    assert_eq!(route(&r, "upi", "stripe").fee(&eur(899)), Some(18));
}

#[test]
fn a_fee_in_another_currency_than_its_fixed_part_is_unknown() {
    let r = builtin();
    assert_eq!(route(&r, "blik", "stripe").fee(&Amount::new("PLN", 1_000)), None);
}

#[test]
fn a_contract_price_is_unknown() {
    let r = builtin();
    assert_eq!(route(&r, "konbini", "komoju").fee(&Amount::new("JPY", 1_000)), None);
}

#[test]
fn a_country_without_its_own_entry_gets_the_default_order() {
    let r = builtin();
    assert_eq!(r.country("XX").map(|c| c.code.as_str()), Some("*"));
    let ids: Vec<&str> = r.methods_for("XX").iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids.first(), Some(&"card"));
}

#[test]
fn people_see_their_own_favourite_first() {
    let r = builtin();
    let first = |c: &str| r.methods_for(c).first().map(|m| m.id.clone());
    assert_eq!(first("NL").as_deref(), Some("ideal"));
    assert_eq!(first("PL").as_deref(), Some("blik"));
    assert_eq!(first("IN").as_deref(), Some("upi"));
    assert_eq!(first("BR").as_deref(), Some("pix"));
}

#[test]
fn a_country_is_offered_exactly_its_own_list_in_its_order() {
    let r = builtin();
    let ids: Vec<&str> = r.methods_for("PL").iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["blik", "p24", "card", "paypal", "apple_pay", "google_pay"]);
    // Revolut Pay serves Poland too, but Polish payers do not use it enough to be offered it
    assert!(r.method("revolut_pay").is_some_and(|m| m.serves("PL")));
    assert!(!ids.contains(&"revolut_pay"));
}

#[test]
fn a_country_without_its_own_entry_gets_the_default_list_and_nothing_more() {
    let r = builtin();
    let ids: Vec<&str> = r.methods_for("UY").iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["card", "paypal", "apple_pay", "google_pay"]);
}

#[test]
fn every_euro_area_country_can_pay_by_sepa_direct_debit() {
    let r = builtin();
    let euro = [
        "AT", "BE", "BG", "CY", "DE", "EE", "ES", "FI", "FR", "GR", "HR", "IE", "IT", "LT", "LU", "LV", "MT", "NL",
        "PT", "SI", "SK",
    ];
    for c in euro {
        let ids: Vec<&str> = r.methods_for(c).iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"sepa_debit"), "{c}: {ids:?}");
        assert_eq!(r.country(c).map(|x| x.currency.as_str()), Some("EUR"), "{c}");
    }
}

#[test]
fn a_table_in_the_older_format_is_refused_rather_than_misread() {
    // format 1 meant "favourites first, every other method after them"
    refused(&tiny_with("format = 2", "format = 1"), "format");
}

#[test]
fn a_route_says_which_personal_data_its_provider_needs_and_who_collects_it() {
    let r = builtin();
    // dLocal's API wants the payer's name, email and national id in the merchant's request
    let dlocal = route(&r, "pix", "dlocal");
    assert_eq!(dlocal.request_data, [PayerData::Name, PayerData::Email, PayerData::NationalId]);
    assert_eq!(dlocal.needs_identity(), [PayerData::Name, PayerData::Email, PayerData::NationalId]);
    // EBANX's payment page wants the name and email in the request and asks the rest itself
    let ebanx = route(&r, "pix", "ebanx");
    assert_eq!(ebanx.request_data, [PayerData::Name, PayerData::Email]);
    assert!(ebanx.payer_data.contains(&PayerData::TaxId), "the page asks the CPF");
    assert!(ebanx.needs_identity().contains(&PayerData::TaxId));
    // GoCardless's own pages ask the account holder: nothing passes through the merchant
    let gc = route(&r, "sepa_debit", "gocardless");
    assert!(gc.request_data.is_empty());
    assert!(gc.needs_identity().contains(&PayerData::Name));
    // a card on Stripe needs no identity at all
    assert!(route(&r, "card", "stripe").needs_identity().is_empty());
    // KOMOJU's hosted page asks for what Konbini needs itself
    assert!(route(&r, "konbini", "komoju").request_data.is_empty());
}

#[test]
fn identity_needs_come_out_once_each_in_a_fixed_order() {
    let r = Registry::parse(TINY).expect("tiny");
    let bank = route(&r, "bank", "p");
    assert_eq!(bank.needs_identity(), [PayerData::Name, PayerData::Email]);
    let twice = tiny_with(r#"request_data = ["name", "email"]"#, r#"request_data = ["email", "name", "email"]"#);
    refused(&twice, "request_data");
}

#[test]
fn a_route_takes_only_its_currencies() {
    let r = builtin();
    assert!(route(&r, "blik", "stripe").takes_currency("PLN"));
    assert!(!route(&r, "blik", "stripe").takes_currency("EUR"));
    assert!(route(&r, "card", "stripe").takes_currency("JPY"));
}

#[test]
fn a_route_open_to_some_merchant_countries_only_is_closed_to_others() {
    let r = builtin();
    let stripe = match r.provider("stripe") {
        Some(p) => p,
        None => panic!("no stripe"),
    };
    assert!(!route(&r, "konbini", "stripe").open_to(stripe, "DE"));
    assert!(route(&r, "konbini", "stripe").open_to(stripe, "JP"));
    assert!(route(&r, "blik", "stripe").open_to(stripe, "DE"));
    assert!(!route(&r, "blik", "stripe").open_to(stripe, "KP"), "a country Stripe takes no business from");
}

#[test]
fn amounts_outside_a_route_s_limits_are_not_taken() {
    let r = builtin();
    let mb_way = route(&r, "mb_way", "stripe");
    assert!(mb_way.amount_ok(&eur(249)));
    assert!(!mb_way.amount_ok(&eur(49)));
    assert!(!mb_way.amount_ok(&eur(500_001)));
    assert!(mb_way.amount_ok(&Amount::new("USD", 1)), "a limit in another currency is the provider's to check");
}

#[test]
fn the_sandbox_fakes_the_routes_with_documented_test_data_only() {
    let r = builtin();
    for (m, p) in [("blik", "stripe"), ("ideal", "stripe"), ("twint", "stripe"), ("mb_way", "stripe"),
                   ("multibanco", "stripe"), ("pix", "stripe"), ("upi", "stripe"), ("bacs_debit", "gocardless")] {
        assert!(route(&r, m, p).faked(), "{m} via {p} has documented test data");
    }
    // KOMOJU's test mode documents the konbini's capture; nothing for its Korean wallets
    assert!(route(&r, "konbini", "komoju").faked());
    assert!(!route(&r, "kakao_pay", "komoju").faked());
}

#[test]
fn a_recurring_route_says_how() {
    let r = builtin();
    assert!(route(&r, "blik", "stripe").recurs());
    assert!(route(&r, "ideal", "stripe").recurs(), "the first payment becomes a SEPA mandate");
    assert!(!route(&r, "eps", "stripe").recurs());
    assert!(!route(&r, "multibanco", "stripe").recurs(), "an invoice paid again each period is prepaid");
}

#[test]
fn no_login_is_ever_offered_in_the_webview() {
    let r = builtin();
    for route in &r.routes {
        let method = match r.method(&route.method) {
            Some(m) => m,
            None => panic!("route of an unknown method"),
        };
        if matches!(method.handoff, Handoff::BankLogin | Handoff::ProviderLogin) {
            assert!(
                route.surfaces.iter().all(|s| matches!(s, Surface::Browser | Surface::Sheet)),
                "{} via {} frames a login",
                route.method,
                route.provider
            );
        }
    }
}

#[test]
fn the_surface_names_are_the_token_server_s() {
    let names: Vec<&str> = Surface::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(names, ["sheet", "iban", "fields", "page", "browser"]);
}

#[test]
fn a_test_input_says_how_long_the_provider_takes() {
    let r = builtin();
    let mb_way = route(&r, "mb_way", "stripe");
    let slow = mb_way.test_inputs.iter().find(|t| t.value == "+351911111112").expect("the slow number");
    assert!(slow.after_minutes > 0, "about 30 seconds");
    let pix = route(&r, "pix", "stripe");
    let now = pix.test_inputs.iter().find(|t| t.value.starts_with("succeed_immediately@")).expect("at once");
    assert_eq!(now.after_minutes, 0);
    refused(&tiny_with("after_minutes = 0", "after_minutes = -1"), "after_minutes");
}
