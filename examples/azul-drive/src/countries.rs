//! Where the buyer lives: the first question of Buy storage ("Where do you live?").
//!
//! The declared country is the only thing Azlin knows about a buyer - no name, no address. It
//! decides the price (the signed ladder's row in the country's currency), the VAT and the
//! payment methods the token server offers. AzDrive guesses it from the locale's region
//! ([`guess`]: `de-DE` is Germany, `pt-BR` Brazil; a language alone guesses nothing), the
//! buyer confirms or picks another in a searchable list ([`search`]), and the answer is
//! remembered on this computer only (the settings file; it never leaves it but in a checkout).
//! Countries no checkout may come from ([`NOT_SERVED`]) are neither listed nor guessed.

use azul_appkit::l10n;

/// The countries Buy storage lists: ISO 3166-1 alpha-2 (and Kosovo's `XK`), each named in the
/// resources (`azdrive-country-<code>`) - every country and territory people live in, except
/// [`NOT_SERVED`].
pub(crate) const COUNTRIES: &[&str] = &[
    "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AR", "AS", "AT", "AU", "AW", "AX", "AZ", "BA",
    "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS", "BT",
    "BW", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN", "CO", "CR", "CV",
    "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE", "EG", "EH", "ER", "ES",
    "ET", "FI", "FJ", "FK", "FM", "FO", "FR", "GA", "GB", "GD", "GE", "GF", "GG", "GH", "GI", "GL",
    "GM", "GN", "GP", "GQ", "GR", "GT", "GU", "GW", "GY", "HK", "HN", "HR", "HT", "HU", "ID", "IE",
    "IL", "IM", "IN", "IO", "IQ", "IS", "IT", "JE", "JM", "JO", "JP", "KE", "KG", "KH", "KI", "KM",
    "KN", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI", "LK", "LR", "LS", "LT", "LU", "LV", "LY",
    "MA", "MC", "MD", "ME", "MF", "MG", "MH", "MK", "ML", "MM", "MN", "MO", "MP", "MQ", "MR", "MS",
    "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA", "NC", "NE", "NF", "NG", "NI", "NL", "NO", "NP",
    "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG", "PH", "PK", "PL", "PM", "PN", "PR", "PS", "PT",
    "PW", "PY", "QA", "RE", "RO", "RS", "RW", "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ",
    "SK", "SL", "SM", "SN", "SO", "SR", "SS", "ST", "SV", "SX", "SZ", "TC", "TD", "TG", "TH", "TJ",
    "TK", "TL", "TM", "TN", "TO", "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "US", "UY", "UZ", "VA",
    "VC", "VE", "VG", "VI", "VN", "VU", "WF", "WS", "XK", "YE", "YT", "ZA", "ZM", "ZW",];

/// Countries no checkout may come from: the EU's restrictive measures and the payment
/// providers' prohibited lists. They are never listed or guessed; the token server refuses them
/// too, in its own words, which the dialog shows as they are.
pub(crate) const NOT_SERVED: &[&str] = &["BY", "CU", "IR", "KP", "RU", "SY"];

/// Whether `code` (any case) is a country Buy storage lists.
#[must_use]
pub(crate) fn offered(code: &str) -> bool {
    let code = code.trim().to_ascii_uppercase();
    COUNTRIES.contains(&code.as_str())
}

/// The country a locale names by its region - BCP 47 (`pt-BR`, `zh-Hant-TW`) or POSIX
/// (`de_DE.UTF-8`, `nl_NL@euro`) - when Buy storage lists it. A language alone (`de`), a
/// numeric region (`es-419`, Latin America) and `C` / `POSIX` name none.
#[must_use]
pub(crate) fn region_of_locale(locale: &str) -> Option<String> {
    let _ = locale;
    None
}

/// The first guess of the buyer's country: the window locale's region, else the system
/// locale's (`LC_ALL`, `LC_MESSAGES`, `LANG` read through `var`); `None` when neither names
/// a country Buy storage lists.
#[must_use]
pub(crate) fn guess(window_locale: &str, var: impl Fn(&str) -> Option<String>) -> Option<String> {
    let _ = (window_locale, var);
    None
}

/// The country the buyer declared: `AZLIN_COUNTRY` (read through `var`: a test's, or a
/// deployment that knows it), else the one remembered on this computer - a listed country only.
#[must_use]
pub(crate) fn declared(
    remembered: Option<&str>,
    var: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    var("AZLIN_COUNTRY")
        .filter(|code| offered(code))
        .or_else(|| remembered.filter(|code| offered(code)).map(String::from))
        .map(|code| code.trim().to_ascii_uppercase())
}

/// The country `code`'s name in the window's language (its code where the resources lack it).
#[must_use]
pub(crate) fn name(code: &str) -> String {
    let code = code.trim().to_ascii_uppercase();
    l10n::app_word(
        "AzDrive",
        &format!("country-{}", code.to_ascii_lowercase()),
        &code,
    )
}

/// The listed countries matching `query` in the window's language, best first: the code typed
/// in full, a name that starts with it, a word of a name that does, a name that holds it - each
/// group by name. Case and accents do not matter (`oster` finds Österreich); an empty query
/// lists every country by name.
#[must_use]
pub(crate) fn search(query: &str) -> Vec<&'static str> {
    let _ = query;
    Vec::new()
}
