//! Buy storage's first question, "Where do you live?": the countries listed, the guess from
//! the locale, the search, the declared country.

use crate::countries::{declared, guess, name, offered, region_of_locale, search, COUNTRIES, NOT_SERVED};

fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_string())
    }
}

#[test]
fn a_locale_names_its_country_by_its_region() {
    for (locale, country) in [
        ("de-DE", "DE"),
        ("pt-BR", "BR"),
        ("en_GB", "GB"),
        ("de_DE.UTF-8", "DE"),
        ("nl_NL@euro", "NL"),
        ("zh-Hant-TW", "TW"),
        ("fr-ch", "CH"),
    ] {
        assert_eq!(region_of_locale(locale).as_deref(), Some(country), "{locale}");
    }
    for locale in ["de", "en", "es-419", "C", "POSIX", "", "en_ZZ.UTF-8", "ru-RU", "be-BY"] {
        assert_eq!(region_of_locale(locale), None, "{locale}: no country to guess");
    }
}

#[test]
fn the_guess_takes_the_windows_locale_first_then_the_systems() {
    assert_eq!(guess("de-DE", env(&[("LANG", "pl_PL.UTF-8")])).as_deref(), Some("DE"));
    assert_eq!(
        guess("en", env(&[("LANG", "pl_PL.UTF-8")])).as_deref(),
        Some("PL"),
        "a language alone guesses nothing: the system's locale does"
    );
    assert_eq!(
        guess("de", env(&[("LC_ALL", "C"), ("LANG", "pt_BR.UTF-8")])).as_deref(),
        Some("BR")
    );
    assert_eq!(guess("en", env(&[])), None, "no guess: the buyer picks");
    assert_eq!(guess("ru-RU", env(&[("LANG", "be_BY.UTF-8")])), None);
    assert_eq!(guess("fa-IR", env(&[])).as_deref(), Some("IR"));
}

#[test]
fn countries_no_checkout_may_come_from_are_never_listed() {
    for code in NOT_SERVED {
        assert!(!COUNTRIES.contains(code), "{code}");
        assert!(!offered(code), "{code}");
    }
    assert!(offered("de") && offered("PL") && offered(" br "));
    // The EU embargoes none of these (its Blocking Statute even forbids following others' for
    // some): the token server sells there, so the picker lists them.
    assert!(offered("IR") && offered("SY") && offered("CU"));
    assert_eq!(NOT_SERVED, &["BY", "KP", "RU"]);
    assert!(!offered("XX") && !offered(""));
    let mut sorted = COUNTRIES.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), COUNTRIES.len(), "each country once");
    assert!(COUNTRIES.len() > 230, "every country people live in");
}

#[test]
fn every_listed_country_has_a_name_in_english_and_in_german() {
    for (lang, text) in [("en", crate::l10n::EN), ("de", crate::l10n::DE)] {
        for code in COUNTRIES {
            let key = format!("azdrive-country-{} = ", code.to_ascii_lowercase());
            assert!(
                text.lines().any(|line| line.starts_with(&key) && line.len() > key.len()),
                "{lang} has no name for {code}"
            );
        }
    }
}

#[test]
fn the_search_finds_a_country_by_its_name_or_its_code_best_first() {
    crate::l10n::in_english();
    assert_eq!(name("pl"), "Poland");
    assert_eq!(search("pol").first(), Some(&"PL"));
    assert_eq!(search("PL").first(), Some(&"PL"), "the code typed in full");
    assert_eq!(search("united"), vec!["AE", "GB", "US"]);
    assert_eq!(
        search("guinea").first(),
        Some(&"GN"),
        "the name that starts with it before the names that hold it"
    );
    assert!(search("zzzz").is_empty());
    let all = search("");
    assert_eq!(all.len(), COUNTRIES.len(), "an empty query lists every country");
    assert_eq!(all.first(), Some(&"AF"), "by name: Afghanistan first");
    azul_appkit::l10n::set_locale("de-DE");
    assert_eq!(name("AT"), "Österreich");
    assert_eq!(search("oster").first(), Some(&"AT"), "accents do not matter");
    assert_eq!(search("vereinigte").len(), 3);
    crate::l10n::in_english();
}

#[test]
fn azlin_country_declares_the_country_before_the_remembered_one() {
    assert_eq!(declared(Some("PL"), env(&[("AZLIN_COUNTRY", "fr")])).as_deref(), Some("FR"));
    assert_eq!(declared(Some("PL"), env(&[])).as_deref(), Some("PL"));
    assert_eq!(declared(Some("pl"), env(&[])).as_deref(), Some("PL"));
    assert_eq!(declared(Some("RU"), env(&[])), None, "never a country not served");
    assert_eq!(
        declared(None, env(&[("AZLIN_COUNTRY", "XX")])),
        None,
        "nothing declared: the dialog asks"
    );
}
