//! The URL policy: what a payment page may navigate to, against the classics of phishing a
//! web view without an address bar (CHECKOUT-PLAN §3.7).

use std::borrow::Cow;

use crate::url::{is_loopback_host, shown_host, Origin, Refused, WebUrl};

fn url(text: &str) -> WebUrl {
    WebUrl::parse(text).unwrap_or_else(|e| panic!("{text} should parse: {e}"))
}

fn exact(host: &'static str) -> Origin {
    Origin::Exact(Cow::Borrowed(host))
}

fn suffix(suffix: &'static str) -> Origin {
    Origin::Suffix(Cow::Borrowed(suffix))
}

#[test]
fn a_plain_https_url_reads_its_host_path_query_and_fragment() {
    let u = url("https://checkout.stripe.com/c/pay/cs_test_1?x=1#pk=abc");
    assert!(u.https());
    assert_eq!(u.host(), "checkout.stripe.com");
    assert_eq!(u.port(), None);
    assert_eq!(u.path(), "/c/pay/cs_test_1");
    assert_eq!(u.query(), "x=1");
    assert_eq!(u.fragment(), "pk=abc");
    assert_eq!(u.origin_text(), "https://checkout.stripe.com");
    assert_eq!(u.without_fragment(), "https://checkout.stripe.com/c/pay/cs_test_1?x=1");
    assert_eq!(u.to_text(), "https://checkout.stripe.com/c/pay/cs_test_1?x=1#pk=abc");
    let bare = url("https://pay.azlin.io");
    assert_eq!(bare.path(), "/", "an empty path is the root");
    assert_eq!(bare.query(), "");
    assert_eq!(bare.fragment(), "");
}

#[test]
fn the_scheme_and_host_are_compared_in_lower_case() {
    let u = url("HTTPS://CHECKOUT.Stripe.COM/Path");
    assert!(u.https());
    assert_eq!(u.host(), "checkout.stripe.com");
    assert_eq!(u.path(), "/Path", "the path keeps its case");
    assert!(exact("checkout.stripe.com").matches(&u));
}

#[test]
fn the_default_port_is_no_port_and_another_one_is_kept() {
    assert_eq!(url("https://pay.azlin.io:443/x").port(), None);
    assert_eq!(url("http://127.0.0.1:80/x").port(), None);
    assert_eq!(url("https://pay.azlin.io:8443/x").port(), Some(8443));
    assert_eq!(url("http://127.0.0.1:8081/x").origin_text(), "http://127.0.0.1:8081");
}

#[test]
fn a_port_that_is_no_port_is_refused() {
    for text in [
        "https://pay.azlin.io:/x",
        "https://pay.azlin.io:0/x",
        "https://pay.azlin.io:65536/x",
        "https://pay.azlin.io:8a/x",
        "https://pay.azlin.io:-1/x",
    ] {
        assert_eq!(WebUrl::parse(text), Err(Refused::Port), "{text}");
    }
}

#[test]
fn userinfo_is_refused_even_when_it_names_the_provider() {
    for text in [
        "https://checkout.stripe.com@evil.example/",
        "https://evil.example@checkout.stripe.com/",
        "https://user:password@checkout.stripe.com/",
        "https://@checkout.stripe.com/",
    ] {
        assert_eq!(WebUrl::parse(text), Err(Refused::Userinfo), "{text}");
    }
}

#[test]
fn a_backslash_anywhere_is_refused() {
    // WHATWG reads `\` as `/` in an http(s) URL: `https://evil.example\@stripe.com` goes to
    // evil.example. A strict reader refuses it rather than guess.
    for text in [
        "https://evil.example\\@checkout.stripe.com/",
        "https://checkout.stripe.com\\evil.example/",
        "https://checkout.stripe.com/c\\pay",
    ] {
        assert_eq!(WebUrl::parse(text), Err(Refused::Characters), "{text}");
    }
}

#[test]
fn a_host_that_is_not_plain_ascii_is_refused() {
    assert_eq!(
        WebUrl::parse("https://checkout%2Estripe.com/"),
        Err(Refused::Host),
        "a percent-encoded host"
    );
    // A homograph: the second letter of "chеckout" is a Cyrillic e.
    assert_eq!(
        WebUrl::parse("https://ch\u{0435}ckout.stripe.com/"),
        Err(Refused::Characters)
    );
    assert_eq!(WebUrl::parse("https://check_out.stripe.com/"), Err(Refused::Host));
    assert_eq!(WebUrl::parse("https:///path"), Err(Refused::Host), "no host");
    // Punycode is plain ASCII: it parses, and matches only a rule that names it.
    let puny = url("https://xn--checkut-1fg.stripe.com/");
    assert!(!exact("checkout.stripe.com").matches(&puny));
}

#[test]
fn a_trailing_dot_or_an_empty_label_is_refused() {
    for text in [
        "https://checkout.stripe.com./",
        "https://.stripe.com/",
        "https://checkout..stripe.com/",
    ] {
        assert_eq!(WebUrl::parse(text), Err(Refused::Host), "{text}");
    }
}

#[test]
fn white_space_and_control_characters_are_refused() {
    for text in [
        " https://pay.azlin.io/",
        "https://pay.azlin.io/ ",
        "https://pay.azlin.io/a b",
        "https://pay.azlin.io/\t",
        "https://pay.azlin.io/\u{7f}",
        "https://pay\n.azlin.io/",
    ] {
        assert_eq!(WebUrl::parse(text), Err(Refused::Characters), "{text:?}");
    }
}

#[test]
fn only_http_and_https_are_web_urls() {
    for (text, scheme) in [
        ("javascript:alert(1)", "javascript"),
        ("data:text/html,<p>hi</p>", "data"),
        ("blob:https://pay.azlin.io/123", "blob"),
        ("file:///etc/passwd", "file"),
        ("about:blank", "about"),
        ("azdrive://pay", "azdrive"),
        ("ftp://pay.azlin.io/", "ftp"),
    ] {
        assert_eq!(
            WebUrl::parse(text),
            Err(Refused::NotWeb(scheme.to_string())),
            "{text}"
        );
    }
    assert_eq!(WebUrl::parse(""), Err(Refused::Empty));
    assert_eq!(WebUrl::parse("pay.azlin.io/x"), Err(Refused::NotWeb(String::new())));
}

#[test]
fn an_exact_origin_matches_its_host_on_https_and_the_default_port_only() {
    let rule = exact("checkout.stripe.com");
    assert!(rule.matches(&url("https://checkout.stripe.com/c/pay/cs_1")));
    assert!(rule.matches(&url("https://checkout.stripe.com:443/")));
    assert!(!rule.matches(&url("http://checkout.stripe.com/")), "plain http");
    assert!(!rule.matches(&url("https://checkout.stripe.com:8443/")), "another port");
    assert!(!rule.matches(&url("https://hooks.stripe.com/")));
    assert!(!rule.matches(&url("https://checkout.stripe.com.evil.example/")));
}

#[test]
fn a_suffix_origin_matches_subdomains_but_not_its_apex_nor_look_alikes() {
    let rule = suffix(".stripe.com");
    assert!(rule.matches(&url("https://checkout.stripe.com/")));
    assert!(rule.matches(&url("https://a.b.stripe.com/")));
    assert!(!rule.matches(&url("https://stripe.com/")), "the apex is not a subdomain");
    assert!(!rule.matches(&url("https://evilstripe.com/")));
    assert!(!rule.matches(&url("https://checkout.stripe.com.evil.example/")));
    assert!(!rule.matches(&url("https://stripe.com.evil.example/")));
    assert!(!rule.matches(&url("http://checkout.stripe.com/")), "plain http");
    assert!(rule.matches(&url("HTTPS://CHECKOUT.STRIPE.COM/")));
}

#[test]
fn the_loopback_origin_takes_http_and_any_port_on_this_computer_only() {
    let rule = Origin::Loopback;
    for text in [
        "http://127.0.0.1:8081/fields/fake-stripe/v1",
        "http://127.3.2.1/",
        "https://127.0.0.1:9443/",
        "http://localhost:8081/",
        "http://[::1]:8081/",
    ] {
        assert!(rule.matches(&url(text)), "{text}");
    }
    for text in [
        "http://127.0.0.1.evil.example/",
        "http://localhost.evil.example/",
        "http://10.0.0.1/",
        "http://192.168.1.10:8081/",
        "https://pay.azlin.io/",
        "http://[::2]/",
    ] {
        assert!(!rule.matches(&url(text)), "{text}");
    }
    assert!(is_loopback_host("127.0.0.1"));
    assert!(is_loopback_host("[::1]"));
    assert!(!is_loopback_host("127.0.0.256"));
    assert!(!is_loopback_host("127.0.0"));
}

#[test]
fn a_loopback_host_origin_takes_that_host_only() {
    let rule = Origin::LoopbackHost(Cow::Borrowed("localhost"));
    assert!(rule.matches(&url("http://localhost:8081/fake-paypal/checkoutnow")));
    assert!(!rule.matches(&url("http://127.0.0.1:8081/fake-paypal/checkoutnow")));
    let numeric = Origin::LoopbackHost(Cow::Borrowed("127.0.0.1"));
    assert!(numeric.matches(&url("http://127.0.0.1:8081/")));
    assert!(!numeric.matches(&url("http://localhost:8081/")));
    // A loopback rule never names a host outside this computer, however it is written.
    let wrong = Origin::LoopbackHost(Cow::Borrowed("evil.example"));
    assert!(!wrong.matches(&url("http://evil.example/")));
}

#[test]
fn a_server_pattern_reads_as_a_host_a_suffix_or_loopback() {
    assert_eq!(
        Origin::parse("checkout.stripe.com"),
        Some(Origin::Exact(Cow::Owned("checkout.stripe.com".to_string())))
    );
    assert_eq!(
        Origin::parse(".Stripe.com"),
        Some(Origin::Suffix(Cow::Owned(".stripe.com".to_string())))
    );
    assert_eq!(Origin::parse("loopback"), Some(Origin::Loopback));
    assert_eq!(
        Origin::parse("localhost"),
        Some(Origin::LoopbackHost(Cow::Owned("localhost".to_string())))
    );
    for junk in ["", ".", "*.stripe.com", "https://stripe.com", "stripe.com/", "a@b.com", "..com"] {
        assert_eq!(Origin::parse(junk), None, "{junk:?}");
    }
}

#[test]
fn a_server_pattern_is_covered_only_by_a_registry_rule_as_wide_or_wider() {
    let registry = [exact("pay.gocardless.com"), suffix(".stripe.com"), Origin::Loopback];
    let covered = |pattern: &str| {
        let p = Origin::parse(pattern).unwrap();
        registry.iter().any(|rule| rule.covers(&p))
    };
    assert!(covered("checkout.stripe.com"));
    assert!(covered(".checkout.stripe.com"));
    assert!(covered(".stripe.com"));
    assert!(covered("pay.gocardless.com"));
    assert!(covered("loopback"));
    assert!(covered("127.0.0.1"));
    assert!(!covered("stripe.com"), "the apex is wider than the subdomains");
    assert!(!covered(".com"));
    assert!(!covered("gocardless.com"));
    assert!(!covered(".gocardless.com"));
    assert!(!covered("evil.example"));
    let narrow = [exact("checkout.stripe.com")];
    assert!(!narrow.iter().any(|r| r.covers(&Origin::Loopback)));
}

#[test]
fn a_shown_host_never_carries_userinfo_a_path_or_a_secret() {
    assert_eq!(shown_host("https://checkout.stripe.com/c/pay/cs_1#pk=abc"), "checkout.stripe.com");
    assert_eq!(shown_host("https://user:pw@evil.example/x"), "evil.example");
    assert_eq!(shown_host("http://127.0.0.1:8081/x"), "127.0.0.1:8081");
    assert_eq!(shown_host("javascript:alert(1)"), "a javascript: address");
    assert_eq!(shown_host("evil"), "an address that is no web address");
    let long = format!("https://{}.example/", "a".repeat(200));
    assert!(shown_host(&long).chars().count() <= 64);
}

#[test]
fn the_debug_text_of_a_url_shows_no_query_and_no_fragment() {
    let u = url("https://checkout.stripe.com/c/pay/cs_live_secret?client_secret=pi_1_secret_2#pk=pk_live_3");
    let text = format!("{u:?}");
    assert!(text.contains("checkout.stripe.com"), "{text}");
    for secret in ["cs_live_secret", "pi_1_secret_2", "pk_live_3"] {
        assert!(!text.contains(secret), "{text}");
    }
}
