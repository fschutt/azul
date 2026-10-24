//! The compiled-in registry: what the reviewed table of CHECKOUT-PLAN §1.3 says about each
//! provider and method, and that it says it consistently.

use crate::{
    registry::{
        self, Breaks, Method, ProviderKind, Settles, SurfaceKind, Webview, BRIDGE_PREFIX, FIELDS_PREFIX,
        PAY_HOST, RETURN_CANCEL, RETURN_PENDING, RETURN_SUCCESS,
    },
    url::{Origin, WebUrl},
};

#[test]
fn every_provider_has_a_unique_id_its_names_its_origins_and_a_method() {
    let mut ids = Vec::new();
    for spec in registry::providers() {
        assert!(!spec.id.is_empty());
        assert!(
            spec.id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{}",
            spec.id
        );
        assert!(!ids.contains(&spec.id), "{} twice", spec.id);
        ids.push(spec.id);
        assert!(!spec.name.is_empty() && !spec.legal_name.is_empty(), "{}", spec.id);
        // Only a provider that takes payments on paper (cash by post) has no pages.
        let paper_only = spec
            .methods
            .iter()
            .all(|m| m.chain == [SurfaceKind::Paper].as_slice());
        assert!(
            !spec.origins.is_empty() || paper_only,
            "{} has no origins",
            spec.id
        );
        assert!(!spec.methods.is_empty(), "{} has no methods", spec.id);
        assert!(!spec.icon.is_empty(), "{}", spec.id);
        assert_eq!(registry::provider(spec.id).map(|s| s.id), Some(spec.id));
    }
    assert!(
        ids.contains(&"stripe") && ids.contains(&"gocardless") && ids.contains(&"cash"),
        "{ids:?}"
    );
    assert!(registry::provider("no-such-provider").is_none());
}

#[test]
fn a_real_provider_lives_on_https_and_its_pages_on_the_pay_host() {
    for spec in registry::providers().filter(|s| !s.fake) {
        for origin in spec.origins {
            assert!(!origin.is_loopback(), "{}: {origin:?}", spec.id);
        }
        assert_eq!(spec.pages, Origin::Exact(PAY_HOST.into()), "{}", spec.id);
        let ok = WebUrl::parse(&format!("https://{PAY_HOST}{RETURN_SUCCESS}")).unwrap();
        assert!(spec.pages.matches(&ok));
        for method in spec.methods {
            for origin in method.leave {
                assert!(!origin.is_loopback(), "{}: {origin:?}", spec.id);
            }
        }
    }
}

#[test]
fn every_fields_page_and_return_path_is_well_formed() {
    for path in [RETURN_SUCCESS, RETURN_CANCEL, RETURN_PENDING, BRIDGE_PREFIX, FIELDS_PREFIX] {
        assert!(path.starts_with('/'), "{path}");
    }
    for spec in registry::providers() {
        if let Some(fields) = spec.fields {
            assert!(
                fields.starts_with(&format!("{FIELDS_PREFIX}{}/", spec.id)),
                "{}: {fields}",
                spec.id
            );
        }
        for method in spec.methods {
            if method.chain.contains(&SurfaceKind::PopoverFields) {
                assert!(
                    spec.fields.is_some(),
                    "{} offers {:?} in a popover without a fields page",
                    spec.id,
                    method.method
                );
            }
        }
    }
}

#[test]
fn a_method_whose_provider_forbids_web_views_never_has_a_web_view_surface() {
    for spec in registry::providers() {
        for method in spec.methods {
            if method.embed.webview == Webview::Forbidden {
                assert!(
                    !method.chain.iter().any(|s| s.in_webview()),
                    "{} {:?}: {:?}",
                    spec.id,
                    method.method,
                    method.chain
                );
            }
            if method.embed.breaks.contains(&Breaks::ProviderLogin) {
                assert_eq!(
                    method.embed.webview,
                    Webview::Forbidden,
                    "a provider login never goes into a web view: {} {:?}",
                    spec.id,
                    method.method
                );
            }
        }
    }
}

#[test]
fn paypal_goes_to_the_system_browser_whichever_provider_carries_it() {
    let mut seen = 0;
    for spec in registry::providers() {
        if let Some(method) = spec.method(Method::PayPal) {
            seen += 1;
            assert_eq!(method.chain, &[SurfaceKind::SystemBrowser], "{}", spec.id);
        }
    }
    assert!(seen >= 1);
}

#[test]
fn every_chain_ends_in_the_system_browser_unless_it_is_paper_alone() {
    for spec in registry::providers() {
        for method in spec.methods {
            if method.chain == [SurfaceKind::Paper].as_slice() {
                // Cash by post: nothing opens, the slip is printed.
                continue;
            }
            assert!(
                !method.chain.contains(&SurfaceKind::Paper),
                "paper is a chain of its own: {} {:?}",
                spec.id,
                method.method
            );
            assert_eq!(
                method.chain.last(),
                Some(&SurfaceKind::SystemBrowser),
                "{} {:?} has no last resort",
                spec.id,
                method.method
            );
            let mut sorted = method.chain.to_vec();
            sorted.dedup();
            assert_eq!(sorted.len(), method.chain.len(), "a surface twice in a chain");
        }
    }
}

#[test]
fn a_card_with_stripe_is_tried_in_the_popover_first() {
    let stripe = registry::provider("stripe").unwrap();
    assert_eq!(stripe.kind, ProviderKind::Processor);
    let card = stripe.method(Method::Card).unwrap();
    assert_eq!(
        card.chain,
        &[
            SurfaceKind::PopoverFields,
            SurfaceKind::WebviewPage,
            SurfaceKind::SystemBrowser
        ]
    );
    let checkout = WebUrl::parse("https://checkout.stripe.com/c/pay/cs_test_1").unwrap();
    assert!(stripe.origins.iter().any(|o| o.matches(&checkout)));
    let paypal = WebUrl::parse("https://www.paypal.com/checkoutnow?token=1").unwrap();
    assert!(card.leave.iter().any(|o| o.matches(&paypal)), "a PayPal login leaves");
}

#[test]
fn the_wire_names_of_methods_and_surfaces_read_back() {
    for method in Method::ALL {
        assert_eq!(Method::parse(method.as_str()), Some(method));
        assert!(!method.label().is_empty() && !method.icon().is_empty());
    }
    assert_eq!(Method::parse("sepa"), Some(Method::SepaDebit), "the v1 name");
    assert_eq!(Method::parse("Card"), Some(Method::Card));
    assert_eq!(Method::parse("bitcoin"), None);
    assert_eq!(Method::parse("cash"), Some(Method::Cash));
    for kind in SurfaceKind::ALL {
        assert_eq!(SurfaceKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(SurfaceKind::parse("popup"), None);
    assert!(SurfaceKind::PopoverFields.in_webview() && SurfaceKind::WebviewPage.in_webview());
    assert!(!SurfaceKind::SystemBrowser.in_webview() && !SurfaceKind::NativeSheet.in_webview());
    assert!(!SurfaceKind::Paper.in_webview(), "paper is printed, never shown in a web view");
}

#[cfg(not(feature = "fake-providers"))]
#[test]
fn a_build_without_the_feature_knows_no_fake_provider() {
    assert!(registry::providers().all(|s| !s.fake));
    assert!(registry::provider("fake-stripe").is_none());
}

#[cfg(feature = "fake-providers")]
#[test]
fn the_fake_providers_live_on_this_computer_only() {
    for id in ["fake-stripe", "fake-gocardless", "fake-paypal", "fake-mor"] {
        let spec = registry::provider(id).unwrap_or_else(|| panic!("{id}"));
        assert!(spec.fake, "{id}");
        assert!(spec.origins.iter().all(Origin::is_loopback), "{id}");
        assert!(spec.pages.is_loopback(), "{id}");
    }
    assert!(matches!(
        registry::provider("fake-mor").unwrap().kind,
        ProviderKind::MerchantOfRecord { .. }
    ));
    let paypal = registry::provider("fake-paypal").unwrap();
    assert_eq!(
        paypal.method(Method::PayPal).unwrap().chain,
        &[SurfaceKind::SystemBrowser]
    );
}

// ==== The local methods ====

#[test]
fn every_method_says_what_the_payer_does_and_where_they_finish() {
    use crate::registry::{Flow, Handoff};
    assert_eq!(Method::ALL.len(), 81);
    for method in Method::ALL {
        assert!(
            method
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "{method:?}"
        );
        // A method that logs in at a bank or a wallet leaves the payment page; one that needs a
        // phone shows a QR code, a push or a code - never both.
        assert!(!(method.handoff().leaves() && method.handoff().needs_phone()), "{method:?}");
    }
    assert_eq!((Method::Blik.flow(), Method::Blik.handoff()), (Flow::Code, Handoff::BankApp));
    assert!(Method::Blik.handoff().needs_phone());
    assert!(Method::Twint.handoff().needs_phone() && Method::MbWay.handoff().needs_phone());
    assert!(Method::Ideal.handoff().leaves() && Method::PayPal.handoff().leaves());
    assert!(!Method::Card.handoff().leaves() && !Method::Card.handoff().needs_phone());
    assert_eq!(Method::Multibanco.handoff(), Handoff::StoreCash);
    assert!(Method::Cash.anonymous() && Method::Voucher.anonymous());
    assert!(!Method::Card.anonymous() && !Method::Blik.anonymous());
    assert_eq!(Method::parse("BLIK"), Some(Method::Blik));
    assert_eq!(Method::parse("mb_way"), Some(Method::MbWay));
    // dLocal's, EBANX's and KOMOJU's own: a transfer, a push to a phone, a QR code, cash in a shop,
    // a wallet's or a bank's login
    assert_eq!((Method::Spei.flow(), Method::Spei.handoff()), (Flow::Transfer, Handoff::BankTransfer));
    assert_eq!((Method::Mpesa.flow(), Method::Mpesa.handoff()), (Flow::Push, Handoff::PhonePush));
    assert_eq!((Method::Qris.flow(), Method::Qris.handoff()), (Flow::Qr, Handoff::PhoneQr));
    assert_eq!((Method::Efecty.flow(), Method::Efecty.handoff()), (Flow::Voucher, Handoff::StoreCash));
    assert_eq!((Method::PayEasy.flow(), Method::PayEasy.handoff()), (Flow::Voucher, Handoff::BankTransfer));
    assert!(Method::MercadoPago.handoff().leaves() && Method::Pse.handoff().leaves());
    assert_eq!(Method::parse("touch_n_go"), Some(Method::TouchNGo));
    assert_eq!(Method::parse("netbanking_in"), Some(Method::NetbankingIn));
}

// ==== dLocal, EBANX and KOMOJU ====

/// The methods each one's routes carry (azul-apps' payment methods registry, read 2026-10-19).
const DLOCAL_METHODS: [&str; 32] = [
    "airtel_money", "au_pay", "boleto", "capitec_pay", "dana", "efecty", "fawry", "gcash", "grabpay",
    "khipu", "konbini", "maya", "mercado_pago", "momo", "mpesa", "nequi", "netbanking_in",
    "ng_bank_transfer", "opay", "ovo", "oxxo", "ozow", "pagoefectivo", "pay_easy", "pix", "pse",
    "qris", "spei", "upi", "webpay", "yape", "zalopay",
];
const EBANX_METHODS: [&str; 18] = [
    "airtel_money", "boleto", "capitec_pay", "efecty", "mercado_pago", "mpesa", "nequi",
    "ng_bank_transfer", "opay", "oxxo", "ozow", "pagoefectivo", "pix", "pse", "spei", "upi", "webpay",
    "yape",
];
const KOMOJU_METHODS: [&str; 19] = [
    "alipay_hk", "au_pay", "bank_transfer", "blik", "dana", "fpx", "gcash", "kakao_pay", "konbini",
    "kr_card", "merpay", "naver_pay", "ovo", "p24", "paidy", "pay_easy", "payco", "paypay",
    "touch_n_go",
];

/// dLocal's pages may never be framed, EBANX's Payment Page asks the payer for details we never
/// see and KOMOJU's hosted page is untested in a web view: all three go to the system browser,
/// and each carries exactly the methods of its routes - a method the app does not know would be
/// dropped from the token server's offer with a log line.
#[test]
fn dlocal_ebanx_and_komoju_carry_every_method_of_their_routes_in_the_system_browser() {
    for (id, methods, host) in [
        ("dlocal", &DLOCAL_METHODS[..], "pay.dlocal.com"),
        ("ebanx", &EBANX_METHODS[..], "sandbox.ebanxpay.com"),
        ("komoju", &KOMOJU_METHODS[..], "komoju.com"),
    ] {
        let spec = registry::provider(id).unwrap_or_else(|| panic!("no {id}"));
        assert!(!spec.fake, "{id}");
        assert_eq!(spec.fields, None, "{id}: no fields page of ours");
        for name in methods {
            let method = Method::parse(name).unwrap_or_else(|| panic!("{id}: the app knows no {name}"));
            let m = spec.method(method).unwrap_or_else(|| panic!("{id} lacks {name}"));
            assert_eq!(m.chain, [SurfaceKind::SystemBrowser].as_slice(), "{id} {name}");
            assert_eq!(m.embed.webview, Webview::Forbidden, "{id} {name}");
        }
        assert_eq!(spec.methods.len(), methods.len(), "{id}: its routes' methods, no other");
        let page = WebUrl::parse(&format!("https://{host}/checkout")).unwrap();
        assert!(spec.owns(&page), "{id}: {host}");
    }
    let page = |u: &str| WebUrl::parse(u).unwrap();
    assert!(registry::provider("dlocal").unwrap().owns(&page("https://sandbox.dlocal.com/collect/x")));
    assert!(registry::provider("ebanx").unwrap().owns(&page("https://checkout.ebanxpay.com/x")));
    assert!(!registry::provider("ebanx").unwrap().owns(&page("https://ebanxpay.com.example.org/x")));
}

#[cfg(feature = "fake-providers")]
#[test]
fn the_dlocal_and_ebanx_stand_ins_take_pix_boleto_and_oxxo_in_the_browser_on_this_computer() {
    for id in ["fake-dlocal", "fake-ebanx"] {
        let spec = registry::provider(id).unwrap_or_else(|| panic!("no {id}"));
        assert!(spec.fake && spec.origins.iter().all(Origin::is_loopback), "{id}");
        for method in [Method::Pix, Method::Boleto, Method::Oxxo] {
            let m = spec.method(method).unwrap_or_else(|| panic!("{id} lacks {method:?}"));
            assert_eq!(m.chain, [SurfaceKind::SystemBrowser].as_slice(), "{id} {method:?}");
        }
    }
}

#[test]
fn stripe_carries_the_local_methods_of_its_routes_on_the_surfaces_they_allow() {
    let stripe = registry::provider("stripe").unwrap();
    let page_browser = [SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser];
    for method in [
        Method::Blik,
        Method::Twint,
        Method::Swish,
        Method::MobilePay,
        Method::Vipps,
        Method::MbWay,
        Method::Multibanco,
        Method::Bizum,
        Method::RevolutPay,
        Method::Upi,
        Method::Pix,
        Method::Boleto,
        Method::WechatPay,
        Method::Konbini,
        Method::Oxxo,
        Method::PayNow,
        Method::PromptPay,
    ] {
        let spec = stripe.method(method).unwrap_or_else(|| panic!("{method:?}"));
        assert_eq!(spec.chain, page_browser.as_slice(), "{method:?}: its page, then the browser");
    }
    for method in [
        Method::Ideal,
        Method::Bancontact,
        Method::Eps,
        Method::P24,
        Method::Klarna,
        Method::Satispay,
        Method::PayByBank,
        Method::Alipay,
        Method::GooglePay,
        Method::Link,
        Method::AmazonPay,
        Method::UsBankAccount,
        Method::KrCard,
        Method::KakaoPay,
        Method::NaverPay,
        Method::Payco,
        Method::SamsungPay,
        Method::PayPay,
        Method::Fpx,
        Method::GrabPay,
    ] {
        let spec = stripe.method(method).unwrap_or_else(|| panic!("{method:?}"));
        assert_eq!(
            spec.chain,
            [SurfaceKind::SystemBrowser].as_slice(),
            "{method:?}: a login or a bank, the system browser only"
        );
    }
    assert_eq!(stripe.method(Method::BankTransfer).unwrap().chain, page_browser.as_slice());
    assert_eq!(stripe.method(Method::P24).unwrap().settles, Settles::Days);
    assert_eq!(stripe.method(Method::Blik).unwrap().settles, Settles::Instant);
}

#[test]
fn gocardless_carries_its_direct_debit_schemes_on_its_page_and_the_browser() {
    let gocardless = registry::provider("gocardless").unwrap();
    for method in [
        Method::BacsDebit,
        Method::Autogiro,
        Method::Betalingsservice,
        Method::BecsDebit,
        Method::BecsNzDebit,
        Method::PadDebit,
    ] {
        let spec = gocardless.method(method).unwrap_or_else(|| panic!("{method:?}"));
        assert_eq!(
            spec.chain,
            [SurfaceKind::WebviewPage, SurfaceKind::SystemBrowser].as_slice(),
            "{method:?}"
        );
        assert_eq!(spec.settles, Settles::Days, "{method:?}");
    }
    assert_eq!(
        gocardless.method(Method::UsBankAccount).unwrap().chain,
        [SurfaceKind::SystemBrowser].as_slice(),
        "ACH logs in at the bank"
    );
}

#[test]
fn a_method_that_logs_in_at_a_bank_or_a_wallet_never_shows_in_a_web_view() {
    for spec in registry::providers() {
        for method in spec.methods {
            if method.method.handoff().leaves() {
                assert!(
                    method.chain.iter().all(|s| !s.in_webview()),
                    "{} {:?}",
                    spec.id,
                    method.method
                );
            }
        }
    }
}

#[cfg(feature = "fake-providers")]
#[test]
fn the_stripe_and_gocardless_stand_ins_carry_the_real_ones_methods_on_this_computer() {
    for (real, fake) in [("stripe", "fake-stripe"), ("gocardless", "fake-gocardless")] {
        let real = registry::provider(real).unwrap();
        let fake = registry::provider(fake).unwrap();
        for method in real.methods {
            // Apple Pay needs the system's sheet: no stand-in has one.
            if method.method == Method::ApplePay {
                continue;
            }
            let stand_in = fake
                .method(method.method)
                .unwrap_or_else(|| panic!("{} lacks {:?}", fake.id, method.method));
            assert_eq!(
                stand_in.chain.last(),
                method.chain.last(),
                "{} {:?}",
                fake.id,
                method.method
            );
            assert_eq!(stand_in.settles, method.settles, "{} {:?}", fake.id, method.method);
        }
        assert!(fake.origins.iter().all(Origin::is_loopback), "{}", fake.id);
    }
}
