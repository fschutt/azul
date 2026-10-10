//! The compiled-in registry: what the reviewed table of CHECKOUT-PLAN §1.3 says about each
//! provider and method, and that it says it consistently.

use crate::{
    registry::{
        self, Breaks, Method, ProviderKind, SurfaceKind, Webview, BRIDGE_PREFIX, FIELDS_PREFIX,
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
        assert!(!spec.origins.is_empty(), "{} has no origins", spec.id);
        assert!(!spec.methods.is_empty(), "{} has no methods", spec.id);
        assert!(!spec.icon.is_empty(), "{}", spec.id);
        assert_eq!(registry::provider(spec.id).map(|s| s.id), Some(spec.id));
    }
    assert!(ids.contains(&"stripe") && ids.contains(&"gocardless"), "{ids:?}");
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
fn every_chain_ends_in_the_system_browser() {
    for spec in registry::providers() {
        for method in spec.methods {
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
    for kind in SurfaceKind::ALL {
        assert_eq!(SurfaceKind::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(SurfaceKind::parse("popup"), None);
    assert!(SurfaceKind::PopoverFields.in_webview() && SurfaceKind::WebviewPage.in_webview());
    assert!(!SurfaceKind::SystemBrowser.in_webview() && !SurfaceKind::NativeSheet.in_webview());
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
