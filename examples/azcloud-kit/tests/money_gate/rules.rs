//! The rules of the money gate (ARCH28 Z-18; RF51 / RF52), as data: what each finds in a file's
//! tokens and its own planted and clean cases. They live apart from the gate's tests so that a
//! tool can run them without building the kit.
//!
//! The finders read tokens, not types ([`azul_srcgate`]): "on a `TokenError`" is "in a file that
//! names `TokenError`", and a rule says so in its cases.

use azul_srcgate::{
    calls, idents, strings, Allow, Budget, Gate, Located, Orphans, Root, Rule, Tok,
};

const CONTRACT: Root = Root {
    dir: "examples/azlin-contract/src",
    min_files: 8,
};

/// The client side: the kit, the checkout's client half, the payment methods' data and AzDrive.
pub const CLIENT: &[Root] = &[
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 40,
    },
    Root {
        dir: "examples/azul-pay/src",
        min_files: 5,
    },
    Root {
        dir: "examples/azul-pay-methods/src",
        min_files: 2,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 60,
    },
];

/// The wire (the contract) and the client side: where a bare pair of amount and currency may
/// stand is counted over both.
pub const WIRE_AND_CLIENT: &[Root] = &[
    CONTRACT,
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 40,
    },
    Root {
        dir: "examples/azul-pay/src",
        min_files: 5,
    },
    Root {
        dir: "examples/azul-pay-methods/src",
        min_files: 2,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 60,
    },
];

// ==== Helpers over the tokens ====

fn punct_at(t: &[Located], i: usize, c: char) -> bool {
    t.get(i).is_some_and(|x| x.tok.is_punct(c))
}

fn ident_at(t: &[Located], i: usize, name: &str) -> bool {
    t.get(i).is_some_and(|x| x.tok.is_ident(name))
}

/// `==` or `!=` starts at `i`.
fn comparison_at(t: &[Located], i: usize) -> bool {
    (punct_at(t, i, '=') || punct_at(t, i, '!')) && punct_at(t, i + 1, '=')
}

fn string_at(t: &[Located], i: usize) -> bool {
    t.get(i).is_some_and(|x| matches!(x.tok, Tok::Str(_)))
}

/// An expression that is a status - a bare binding (`s`, `status`) or `x.status()` - starts at
/// `start` and ends before `end`, and is DECIDED by a status number: compared (`s == 404`,
/// `s >= 500`), matched (`match s { 404 => .. }`, `matches!(s, 404)`) or ranged (`(400..=499)
/// .contains(&s)`). The forms a binding renamed to dodge `status: 404` is read in.
fn status_decided(t: &[Located], start: usize, end: usize) -> bool {
    if operator_len(t, end).is_some_and(|len| status_number_at(t, end + len)) {
        return true;
    }
    // `matches!(s, 404 ..)`
    if start >= 3
        && ident_at(t, start - 3, "matches")
        && punct_at(t, start - 2, '!')
        && punct_at(t, start - 1, '(')
        && punct_at(t, end, ',')
        && status_number_at(t, end + 1)
    {
        return true;
    }
    // `match s { 404 => ..`
    if start >= 1
        && ident_at(t, start - 1, "match")
        && punct_at(t, end, '{')
        && status_number_at(t, end + 1)
    {
        return true;
    }
    // `(400..=499).contains(&s)`: a number right before the receiver's `.contains(`
    if start >= 4
        && punct_at(t, start - 1, '&')
        && punct_at(t, start - 2, '(')
        && ident_at(t, start - 3, "contains")
        && punct_at(t, start - 4, '.')
        && punct_at(t, end, ')')
    {
        return (start.saturating_sub(14)..start - 4).any(|k| status_number_at(t, k));
    }
    false
}

/// A three-digit HTTP status as the lexer reads a number (an identifier of digits).
fn status_number_at(t: &[Located], i: usize) -> bool {
    t.get(i).is_some_and(|x| match &x.tok {
        Tok::Ident(n) => {
            let digits = n.chars().take_while(char::is_ascii_digit).count();
            digits == 3 && (n.len() == 3 || &n[3..] == "u16")
        }
        _ => false,
    })
}

/// The file names the token server's error or its typed refusal (`TokenError`, `Refusal`,
/// `Unclassified`): what "on a refusal" means to these rules. A file that names `Refusal` and
/// never `TokenError` is read as well - `Refusal::Unclassified { status, code }` is public.
fn names_token_error(t: &[Located]) -> bool {
    t.iter().any(|x| {
        ["TokenError", "Refusal", "Unclassified"]
            .iter()
            .any(|n| x.tok.is_ident(n))
    })
}

/// The length, in tokens, of the comparison operator that starts at `i`: `==`, `!=`, `>=` and
/// `<=` are two, a lone `>` or `<` is one; `None` for anything else.
fn operator_len(t: &[Located], i: usize) -> Option<usize> {
    let angle = punct_at(t, i, '>') || punct_at(t, i, '<');
    if comparison_at(t, i) || (angle && punct_at(t, i + 1, '=')) {
        return Some(2);
    }
    (angle && !punct_at(t, i + 1, '>') && !punct_at(t, i + 1, '<')).then_some(1)
}

/// A number literal that is zero, in any spelling and of any type: `0`, `00`, `0u64`, `0_u64`,
/// `0usize`, `0i64`.
fn is_zero_literal(name: &str) -> bool {
    if !name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return false;
    }
    let end = name
        .find(|c: char| !(c.is_ascii_digit() || c == '_'))
        .unwrap_or(name.len());
    let (number, suffix) = name.split_at(end);
    let digits: String = number.chars().filter(|c| *c != '_').collect();
    !digits.is_empty()
        && digits.chars().all(|c| c == '0')
        && matches!(
            suffix,
            "" | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "f32"
                | "f64"
        )
}

fn zero_at(t: &[Located], i: usize) -> bool {
    t.get(i)
        .is_some_and(|x| x.tok.ident().is_some_and(is_zero_literal))
}

// ==== 1. A checkout leaves the list only with evidence ====

/// `Settled(` - the checkout-ending proof made by hand.
fn settled_made(t: &[Located]) -> Vec<usize> {
    calls(t, "Settled")
}

/// `SettledBy` named at all.
fn settled_by(t: &[Located]) -> Vec<usize> {
    idents(t, "SettledBy")
}

/// `Settled::abandoned(..)`, `Settled::gone(..)`: any constructor of the proof called by its path.
/// Each takes evidence only `pending/` can make, but an answer that is a public enum
/// ([`azlin_contract::AbandonAnswer`]) is no evidence: a caller outside `pending/` could end a
/// checkout with one it made up (X-DELTA-AZDRIVE-MONEY-11).
fn settled_path_calls(t: &[Located]) -> Vec<usize> {
    (0..t.len())
        .filter(|&i| {
            ident_at(t, i, "Settled")
                && punct_at(t, i + 1, ':')
                && punct_at(t, i + 2, ':')
                && t.get(i + 3).is_some_and(|x| x.tok.ident().is_some())
                && punct_at(t, i + 4, '(')
        })
        .map(|i| t[i].line)
        .collect()
}

pub const SETTLED_MADE: &str = "settled-made";
pub const SETTLED_BY: &str = "settled-by";
pub const SETTLED_PATH: &str = "settled-path";

#[must_use]
pub fn settled_rules() -> Vec<Rule> {
    vec![
        Rule::new(
            SETTLED_MADE,
            settled_made,
            &["let s = Settled(SettledBy::Gone);"],
            &[
                "let s = Settled::gone(&proof);",
                "let s: Option<Settled> = None;",
            ],
        ),
        Rule::new(
            SETTLED_BY,
            settled_by,
            &["let by = SettledBy::Gone;"],
            &["let by = Settled::gone(&proof);"],
        ),
        Rule::new(
            SETTLED_PATH,
            settled_path_calls,
            &[
                "let s = Settled::abandoned(answer);",
                "let s = pending::Settled::gone(&proof);",
            ],
            &[
                "let s: Option<Settled> = None;",
                "fn f(proof: Settled) {}",
                "let s = Settled { };",
            ],
        ),
    ]
}

// ==== 2. A refusal is decided by its type, not by a status number or a word ====

/// `status: 404` in a file that names `TokenError` or `Refusal`: a pattern or a literal that
/// decides by the HTTP status - and `status: s` with `s` compared with a status number soon after
/// (`Refused { status: s, .. } if s == 404`, a binding renamed to dodge the literal).
fn status_literal(t: &[Located]) -> Vec<usize> {
    if !names_token_error(t) {
        return Vec::new();
    }
    (0..t.len())
        .filter(|&i| {
            if !t[i].tok.is_ident("status") || !punct_at(t, i + 1, ':') || punct_at(t, i + 2, ':') {
                return false;
            }
            if status_number_at(t, i + 2) {
                return true;
            }
            // `status: bound` ... `bound <op> 404`
            let Some(bound) = t.get(i + 2).and_then(|x| x.tok.ident()) else {
                return false;
            };
            if matches!(bound, "_" | "ref" | "mut")
                || bound.starts_with(|c: char| c.is_ascii_digit())
            {
                return false;
            }
            (i + 3..(i + 24).min(t.len()))
                .any(|k| t[k].tok.is_ident(bound) && status_decided(t, k, k + 1))
        })
        .map(|i| t[i].line)
        .collect()
}

/// `status == 401`, `status != 200`, `.status() == 404` in a file that names `TokenError` - but
/// not `reply.status`: the HTTP answer before it is a `TokenError`.
fn status_compared(t: &[Located]) -> Vec<usize> {
    if !names_token_error(t) {
        return Vec::new();
    }
    (0..t.len())
        .filter(|&i| {
            if !t[i].tok.is_ident("status") {
                return false;
            }
            let on_reply = i >= 2 && punct_at(t, i - 1, '.') && ident_at(t, i - 2, "reply");
            let called = punct_at(t, i + 1, '(') && punct_at(t, i + 2, ')');
            let after = if called { i + 3 } else { i + 1 };
            // `status` bare, or `x.status()`: where the expression starts
            let start = if called && i >= 2 && punct_at(t, i - 1, '.') {
                i - 2
            } else {
                i
            };
            !on_reply && status_decided(t, start, after)
        })
        .map(|i| t[i].line)
        .collect()
}

/// The no-argument string adapters that change a text and not what it says: a question put to
/// `e.to_string().to_lowercase()` is a question put to the error's text.
const TEXT_ADAPTERS: &[&str] = &[
    "to_lowercase",
    "to_ascii_lowercase",
    "to_uppercase",
    "to_ascii_uppercase",
    "trim",
    "as_str",
];

/// The receiver of a `.contains(..)` / `.starts_with(..)` / `.ends_with(..)` that ends with the `)`
/// at `close` is an error's text or a refusal's code: `e.to_string()`, `e.said()`,
/// `code.as_str()` - with any of the [`TEXT_ADAPTERS`] after it (`e.to_string().to_lowercase()`).
fn error_text_receiver(t: &[Located], mut close: usize) -> bool {
    for _ in 0..6 {
        if !(punct_at(t, close, ')') && close >= 2 && punct_at(t, close - 1, '(')) {
            return false;
        }
        let name = close - 2;
        if ident_at(t, name, "to_string") || ident_at(t, name, "said") {
            return true;
        }
        if ident_at(t, name, "as_str") && name >= 2 && ident_at(t, name - 2, "code") {
            return true;
        }
        // `<receiver>.adapter()`: look at what the adapter was called on
        if name >= 3
            && punct_at(t, name - 1, '.')
            && punct_at(t, name - 2, ')')
            && TEXT_ADAPTERS.iter().any(|a| ident_at(t, name, a))
        {
            close = name - 2;
            continue;
        }
        return false;
    }
    false
}

/// An occurrence of `bound` at `k` - a binding of a refusal's `code` under another name
/// (`Refused { code: c, .. }`) - asks about a word: `c == "word"`, `"word" == c`,
/// `c.starts_with("..")`, `c.contains("..")`, `c.ends_with("..")`, `matches!(c.as_str(), ..)`,
/// `WORDS.contains(&c.as_str())`.
fn code_asked_as(t: &[Located], k: usize) -> bool {
    let against_word = comparison_at(t, k + 1) && string_at(t, k + 3);
    let word_against = k >= 3 && comparison_at(t, k - 2) && string_at(t, k - 3);
    let method = punct_at(t, k + 1, '.')
        && ["starts_with", "ends_with", "contains"]
            .iter()
            .any(|m| ident_at(t, k + 2, m))
        && punct_at(t, k + 3, '(')
        && string_at(t, k + 4);
    let as_str = punct_at(t, k + 1, '.') && ident_at(t, k + 2, "as_str");
    let in_list = as_str && k >= 3 && ident_at(t, k - 3, "contains") && punct_at(t, k - 1, '&');
    let in_match = as_str && k >= 3 && ident_at(t, k - 3, "matches") && punct_at(t, k - 2, '!');
    against_word || word_against || method || in_list || in_match
}

/// `code == "word"`, `"word" == code`, `.contains(&code.as_str())`, `matches!(code.as_str(), ..)`
/// and a question about a word put to the code or to the error's text - `code.starts_with("..")`,
/// `code.as_str().contains("..")`, `e.to_string().contains("..")`, `e.said().ends_with("..")`,
/// `e.to_string().to_lowercase().contains("..")` - in a file that names `TokenError` or
/// `Refusal`; and the same of the code under another name: `Refused { code: c, .. } if
/// c.starts_with("lock_")`.
fn code_compared(t: &[Located]) -> Vec<usize> {
    if !names_token_error(t) {
        return Vec::new();
    }
    (0..t.len())
        .filter(|&i| {
            // a method that asks about a word: `<receiver>.contains("..")`
            if ["contains", "starts_with", "ends_with"]
                .iter()
                .any(|m| t[i].tok.is_ident(m))
                && i >= 2
                && punct_at(t, i - 1, '.')
                && punct_at(t, i + 1, '(')
                && string_at(t, i + 2)
                && (ident_at(t, i - 2, "code") || error_text_receiver(t, i - 2))
            {
                return true;
            }
            if !t[i].tok.is_ident("code") {
                return false;
            }
            // `code: c` binds the code under another name: `c` is asked about soon after
            if punct_at(t, i + 1, ':') && !punct_at(t, i + 2, ':') {
                let Some(bound) = t.get(i + 2).and_then(|x| x.tok.ident()) else {
                    return false;
                };
                if matches!(bound, "_" | "ref" | "mut")
                    || bound.starts_with(|c: char| c.is_ascii_digit())
                {
                    return false;
                }
                return (i + 3..(i + 24).min(t.len()))
                    .any(|k| t[k].tok.is_ident(bound) && code_asked_as(t, k));
            }
            code_asked_as(t, i)
        })
        .map(|i| t[i].line)
        .collect()
}

pub const STATUS_LITERAL: &str = "status-literal";
pub const STATUS_COMPARED: &str = "status-compared";
pub const CODE_COMPARED: &str = "code-compared";
pub const CODE_WORD: &str = "contract-code-word";

/// The rules that read a refusal's status or word; `words` are the contract's code words
/// ([`CODE_WORD`]: a string literal that IS one of them).
///
/// # Panics
///
/// With no words: a rule that matches nothing cannot pass for a clean tree.
#[must_use]
pub fn refusal_rules(words: &[String]) -> Vec<Rule> {
    assert!(!words.is_empty(), "the contract's code words");
    let list: Vec<String> = words.to_vec();
    let planted_word = format!("let w = \"{}\";", words[0]);
    vec![
        Rule::new(
            STATUS_LITERAL,
            status_literal,
            &[
                "match e { TokenError::Refused { status: 404, .. } => {} _ => {} }",
                "match e { TokenError::Refused { status: 400..=499, .. } => {} _ => {} }",
                "match e { TokenError::Refused { status: s, .. } if s == 404 => {} _ => {} }",
                "match r { Refusal::Unclassified { status: gone, .. } if gone >= 500 => {} _ => {} }",
                "match r { Refusal::Unclassified { status: 404, .. } => {} _ => {} }",
                "match e { TokenError::Refused { status: s, .. } if matches!(s, 404 | 410) => {} _ => {} }",
                "match e { TokenError::Refused { status: s, .. } => match s { 404 => {} _ => {} } }",
                "match e { TokenError::Refused { status: s, .. } if (400..=499).contains(&s) => {} _ => {} }",
            ],
            &[
                "match e { TokenError::Refused { status, .. } => drop(status), _ => {} }",
                "let reply = HttpReply { status: 200 };",
                "match e { TokenError::Refused { status: s, .. } => drop(s), _ => {} }",
                "match e { TokenError::Refused { status: s, .. } if other == 404 => drop(s), _ => {} }",
                "match e { TokenError::Refused { status: s, .. } if matches!(other, 404) => drop(s), _ => {} }",
                "match e { TokenError::Refused { status: s, .. } if (400..=499).contains(&other) => drop(s), _ => {} }",
                "match e { TokenError::Refused { status: s, .. } => match other { 404 => drop(s), _ => {} } }",
            ],
        ),
        Rule::new(
            STATUS_COMPARED,
            status_compared,
            &[
                "if matches!(e, TokenError::Busy { .. }) && status == 429 {}",
                "let gone = TokenError::Protocol(String::new()); if e.status() != 200 {}",
                "let r: Refusal = x; if status >= 500 {}",
                "let e: TokenError = x; if e.status() < 500 {}",
                "let e: TokenError = x; if matches!(status, 404) {}",
                "let e: TokenError = x; if matches!(e.status(), 404 | 410) {}",
                "let e: TokenError = x; match status { 404 => {} _ => {} }",
                "let e: TokenError = x; if (400..=499).contains(&status) {}",
                "let e: TokenError = x; if (500..=599).contains(&e.status()) {}",
            ],
            &[
                "let signed_out = TokenError::SignIn(String::new()); if reply.status == 401 {}",
                "let ok = TokenError::Config(String::new()); let status = 4;",
                "let e: TokenError = x; if status == other {}",
                "let e: TokenError = x; if matches!(status, other) {}",
                "let e: TokenError = x; if (lo..=hi).contains(&status) {}",
                "let e: TokenError = x; match status { s => drop(s) }",
                "let ok = TokenError::Config(String::new()); if matches!(reply.status, 404) {}",
            ],
        ),
        Rule::new(
            CODE_COMPARED,
            code_compared,
            &[
                "let e: TokenError = x; if code == \"no_such_checkout\" {}",
                "let e: TokenError = x; if \"not_paid\" != code {}",
                "let e: TokenError = x; if WORDS.contains(&code.as_str()) {}",
                "let e: TokenError = x; let m = matches!(code.as_str(), \"a\" | \"b\");",
                "let e: TokenError = x; if code.starts_with(\"lock_\") {}",
                "let e: TokenError = x; if code.as_str().contains(\"paid\") {}",
                "let e: TokenError = x; if e.to_string().contains(\"no such checkout\") {}",
                "let r: Refusal = x; if e.said().ends_with(\"gone\") {}",
                "let e: TokenError = x; if e.to_string().to_lowercase().contains(\"no such\") {}",
                "let e: TokenError = x; if e.said().trim().to_ascii_lowercase().starts_with(\"lock\") {}",
                "match e { TokenError::Refused { code: c, .. } if c.starts_with(\"lock_\") => {} _ => {} }",
                "match e { TokenError::Refused { code: word, .. } if word == \"no_such_checkout\" => {} _ => {} }",
                "match e { TokenError::Refused { code: w, .. } if matches!(w.as_str(), \"a\" | \"b\") => {} _ => {} }",
                "match e { TokenError::Refused { code: w, .. } if WORDS.contains(&w.as_str()) => {} _ => {} }",
            ],
            &[
                "let e: TokenError = x; if code == other {}",
                "let e: TokenError = x; let s = code.as_str();",
                "let e = 1; if code == \"word\" {}",
                "let e: TokenError = x; if text.contains(\"word\") {}",
                "let e: TokenError = x; if e.to_string().contains(&needle) {}",
                "let e = 1; if code.starts_with(\"lock_\") {}",
                "let e: TokenError = x; if text.to_lowercase().contains(\"word\") {}",
                "let e: TokenError = x; if e.to_string().to_lowercase().contains(&needle) {}",
                "match e { TokenError::Refused { code: c, .. } => drop(c), _ => {} }",
                "match e { TokenError::Refused { code: c, .. } if other.starts_with(\"lock_\") => drop(c), _ => {} }",
                "match e { TokenError::Refused { code: _, .. } => {} _ => {} }",
            ],
        ),
        Rule::new(
            CODE_WORD,
            move |t| strings(t, |s| list.iter().any(|w| w == s)),
            &[planted_word.as_str()],
            &["let w = \"a word of nobody's contract\";"],
        ),
    ]
}

// ==== 3. Money is Cents, not a bare pair ====

/// A name that holds an amount: it says `amount`, `cents` or `price`.
fn is_amount_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["amount", "cents", "price"]
        .iter()
        .any(|w| lower.contains(w))
}

/// The whole-number types an amount is held in when it stands alone.
const NUMBER_TYPES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// A whole-number type starts at `i` - after a `&` or a `mut`, or inside an `Option<`.
fn number_type_at(t: &[Located], mut i: usize) -> bool {
    while punct_at(t, i, '&') || ident_at(t, i, "mut") {
        i += 1;
    }
    if ident_at(t, i, "Option") && punct_at(t, i + 1, '<') {
        i += 2;
    }
    // `u32::from_be_bytes(..)` after `amount_cents:` is a value being made, not a type
    t.get(i)
        .is_some_and(|x| x.tok.ident().is_some_and(|n| NUMBER_TYPES.contains(&n)))
        && !(punct_at(t, i + 1, ':') && punct_at(t, i + 2, ':'))
}

/// `amount_cents: u64`, `price_cents: i64`, `amount: Option<u64>` - a field or a parameter whose
/// name says it holds an amount (`amount`, `cents`, `price`) and that is a bare whole number: an
/// amount without its currency.
fn bare_amount(t: &[Located]) -> Vec<usize> {
    (0..t.len())
        .filter(|&i| {
            t[i].tok.ident().is_some_and(is_amount_name)
                && punct_at(t, i + 1, ':')
                && !punct_at(t, i + 2, ':')
                && number_type_at(t, i + 2)
        })
        .map(|i| t[i].line)
        .collect()
}

/// `currency: String`, `currency: Option<String>`, `currency: &String` - a currency as free text
/// (a borrowed `&str` is a request parameter or a view of a typed one, and is not read).
fn bare_currency(t: &[Located]) -> Vec<usize> {
    (0..t.len())
        .filter(|&i| {
            if !(t[i].tok.is_ident("currency")
                && punct_at(t, i + 1, ':')
                && !punct_at(t, i + 2, ':'))
            {
                return false;
            }
            let mut at = i + 2;
            if punct_at(t, at, '&') {
                at += 1;
            }
            if ident_at(t, at, "Option") && punct_at(t, at + 1, '<') {
                at += 2;
            }
            ident_at(t, at, "String")
        })
        .map(|i| t[i].line)
        .collect()
}

pub const BARE_AMOUNT: &str = "bare-amount";
pub const BARE_CURRENCY: &str = "bare-currency";

#[must_use]
pub fn pair_rules() -> Vec<Rule> {
    vec![
        Rule::new(
            BARE_AMOUNT,
            bare_amount,
            &[
                "struct S { amount_cents: u64 }",
                "struct S { price_cents: i64 }",
                "struct S { amount_cents: Option<u64> }",
                "struct S { amount_cents: u32 }",
                "fn f(amount: u64) {}",
                "fn f(price: &u64) {}",
            ],
            &[
                "struct S { price: Cents }",
                "let amount_cents = 5_u64; let n: u64 = 1;",
                "struct S { count: u64, months: u32 }",
                "struct S { amount: Amount, price: Option<Cents> }",
                "let c = Config::amount::default();",
                "let s = S { amount_cents: u32::from_be_bytes(b), price: u64::try_from(n)? };",
            ],
        ),
        Rule::new(
            BARE_CURRENCY,
            bare_currency,
            &[
                "struct S { currency: String }",
                "struct S { currency: Option<String> }",
                "fn f(currency: &String) {}",
            ],
            &[
                "struct S { currency: Currency }",
                "fn f(currency: &str) {}",
                "struct S { currency: Option<Currency> }",
            ],
        ),
    ]
}

// ==== 4. Nothing about money is defaulted ====

/// A name that is about money.
fn is_money_word(name: &str) -> bool {
    ["amount", "cents", "vat", "currency"]
        .iter()
        .any(|w| name.to_ascii_lowercase().contains(w))
}

/// The index after the `)` that closes the `(` at `open`.
fn call_end(t: &[Located], open: usize) -> usize {
    let mut depth = 0_i32;
    for (j, x) in t.iter().enumerate().skip(open) {
        if x.tok.is_punct('(') {
            depth += 1;
        } else if x.tok.is_punct(')') {
            depth -= 1;
            if depth == 0 {
                return j + 1;
            }
        }
    }
    t.len()
}

/// The methods that supply a value for a missing one.
const DEFAULTERS: &[&str] = &[
    "unwrap_or",
    "unwrap_or_else",
    "map_or",
    "map_or_else",
    "or",
    "or_else",
];

/// A constant that is the euro by name: `DEFAULT_CURRENCY`, `DEFAULT_EUR`.
fn is_default_currency_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("default") && (lower.contains("currency") || lower.contains("eur"))
}

/// `.unwrap_or("EUR")`, `.unwrap_or_else(|| String::from("EUR"))`, `.map_or("EUR", ..)`,
/// `.map_or_else(|| "EUR".to_string(), ..)`, `.unwrap_or(DEFAULT_CURRENCY)`: the euro as the
/// answer to a currency nobody named.
fn euro_defaulted(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for i in 1..t.len() {
        let Some(name) = t[i].tok.ident() else {
            continue;
        };
        if !DEFAULTERS.contains(&name) || !punct_at(t, i - 1, '.') || !punct_at(t, i + 1, '(') {
            continue;
        }
        let end = call_end(t, i + 1);
        let euro = t[i + 1..end].iter().any(|x| match &x.tok {
            Tok::Str(s) => s.eq_ignore_ascii_case("EUR"),
            Tok::Ident(n) => is_default_currency_name(n),
            Tok::Punct(_) => false,
        });
        if euro {
            found.push(t[i].line);
        }
    }
    found
}

/// A zero as the default of a call that starts at `open` (its `(`): `(0)`, `(0u64)`, `(0_u64)`,
/// `(0, ..)`, `(|| 0)`, `(|| 0_i64, ..)`, `(|_| 0)`, `(|e| 0, ..)`.
fn zero_default_at(t: &[Located], open: usize) -> bool {
    let a = open + 1;
    let ends = |i: usize| punct_at(t, i, ')') || punct_at(t, i, ',');
    let no_arguments =
        punct_at(t, a, '|') && punct_at(t, a + 1, '|') && zero_at(t, a + 2) && ends(a + 3);
    // `|_| 0`, `|e| 0`: a closure that takes the error and ignores it
    let one_argument = punct_at(t, a, '|')
        && t.get(a + 1).is_some_and(|x| x.tok.ident().is_some())
        && punct_at(t, a + 2, '|')
        && zero_at(t, a + 3)
        && ends(a + 4);
    (zero_at(t, a) && ends(a + 1)) || no_arguments || one_argument
}

/// A missing figure read as zero in a statement that talks of an amount, cents, VAT or a
/// currency: `.unwrap_or(0)` of any spelling and type (`0u64`, `0_u64`), `.unwrap_or_else(|| 0)`,
/// `.map_or(0, ..)`, `.map_or_else(|| 0, ..)` and `.unwrap_or_default()`.
fn money_zeroed(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for i in 1..t.len() {
        let Some(name) = t[i].tok.ident() else {
            continue;
        };
        let zeroed = match name {
            "unwrap_or_default" => punct_at(t, i + 1, '('),
            "unwrap_or" | "unwrap_or_else" | "map_or" | "map_or_else" => {
                punct_at(t, i + 1, '(') && zero_default_at(t, i + 1)
            }
            _ => false,
        };
        if !zeroed || !punct_at(t, i - 1, '.') {
            continue;
        }
        // the statement so far: back to the previous `;`, `,`, `{` or `}` (a struct literal's
        // other fields are not this one's neighbours)
        let from = (0..i)
            .rev()
            .find(|&k| [';', ',', '{', '}'].iter().any(|&c| t[k].tok.is_punct(c)))
            .map_or(0, |k| k + 1);
        if t[from..i]
            .iter()
            .any(|x| x.tok.ident().is_some_and(is_money_word))
        {
            found.push(t[i].line);
        }
    }
    found
}

pub const EURO_DEFAULTED: &str = "euro-defaulted";
pub const MONEY_ZEROED: &str = "money-zeroed";

#[must_use]
pub fn default_rules() -> Vec<Rule> {
    vec![
        Rule::new(
            EURO_DEFAULTED,
            euro_defaulted,
            &[
                "let c = answer.currency.unwrap_or(\"EUR\");",
                "let c = answer.currency.unwrap_or_else(|| String::from(\"EUR\"));",
                "let c = answer.currency.map_or(\"EUR\", str::to_string);",
                "let c = answer.currency.map_or_else(|| \"EUR\".to_string(), |c| c);",
                "let c = answer.currency.unwrap_or(DEFAULT_CURRENCY);",
                "let c = answer.currency.or_else(|| Some(\"eur\".to_string()));",
            ],
            &[
                "let c = answer.currency.unwrap_or(\"PLN\");",
                "let c = Currency::parse(\"EUR\")?;",
                "let c = parsed(\"EUR\").unwrap_or(other);",
                "let c = answer.currency.map_or(\"PLN\", str::to_string);",
                "let c = answer.currency.unwrap_or(DEFAULT_PAGE_SIZE);",
            ],
        ),
        Rule::new(
            MONEY_ZEROED,
            money_zeroed,
            &[
                "let a = answer.amount_cents.unwrap_or(0);",
                "let v = receipt.vat.unwrap_or_default();",
                "let a = answer.amount_cents.unwrap_or(0u64);",
                "let a = answer.amount_cents.unwrap_or(0_u64);",
                "let a = answer.amount_cents.map_or(0, |a| a);",
                "let a = answer.amount_cents.unwrap_or_else(|| 0);",
                "let a = answer.amount_cents.map_or_else(|| 0_i64, |a| a);",
                "let a = answer.amount_cents.unwrap_or_else(|_| 0);",
                "let a = answer.amount_cents.map_or_else(|e| 0_u64, |a| a);",
            ],
            &[
                "let n = count.unwrap_or(0);",
                "let a = answer.amount_cents.unwrap_or(1);",
                "let a = answer.amount_cents; let n = count.unwrap_or(0);",
                "let n = hits.map_or(0, |h| h.len());",
                "let a = answer.amount_cents.map_or(1, |a| a);",
                "let n = count.unwrap_or_else(|| 0u64);",
                "let a = answer.amount_cents.unwrap_or_else(|_| 1);",
                "let n = hits.map_or_else(|e| 0, |h| h.len());",
            ],
        ),
    ]
}

// ==== 5. Money is shown by one function ====

/// `l10n::money(` and `money(` - the grouped digits and the currency's sign - and `money as m`,
/// the function imported under another name to be called as that.
fn money_shown(t: &[Located]) -> Vec<usize> {
    let mut found = calls(t, "l10n::money");
    found.extend(calls(t, "money"));
    found.extend(
        (0..t.len())
            .filter(|&i| ident_at(t, i, "money") && ident_at(t, i + 1, "as"))
            .map(|i| t[i].line),
    );
    found.sort_unstable();
    found
}

pub const MONEY_SHOWN: &str = "money-shown";

#[must_use]
pub fn show_rules() -> Vec<Rule> {
    vec![Rule::new(
        MONEY_SHOWN,
        money_shown,
        &[
            "let s = l10n::money(c.amount, c.currency.as_str());",
            "let s = azul_appkit::l10n::money(5, \"EUR\");",
            "let s = money(5, \"EUR\");",
            "use azul_appkit::l10n::money as m;",
        ],
        &["let s = show(&c);", "let m = c.money(5);"],
    )]
}

// ==== 6. The v1 order button ends a checkout in flight before it orders again ====

/// The index of the `}` that closes the `{` at `open`.
fn block_end(t: &[Located], open: usize) -> usize {
    let mut depth = 0_i32;
    for (j, x) in t.iter().enumerate().skip(open) {
        if x.tok.is_punct('{') {
            depth += 1;
        } else if x.tok.is_punct('}') {
            depth -= 1;
            if depth == 0 {
                return j;
            }
        }
    }
    t.len()
}

/// The `fn buy_on_the_payment_page` items of a file: the index of each one's name and of the `{`
/// that opens its body.
fn v1_order_fns(t: &[Located]) -> Vec<(usize, usize)> {
    (1..t.len())
        .filter(|&i| ident_at(t, i, "buy_on_the_payment_page") && ident_at(t, i - 1, "fn"))
        .filter_map(|i| {
            let open = (i + 1..t.len()).find(|&k| punct_at(t, k, '{') || punct_at(t, k, ';'))?;
            punct_at(t, open, '{').then_some((i, open))
        })
        .collect()
}

/// The first index in `from..to` where the identifiers `first :: second` stand.
fn path_in(t: &[Located], from: usize, to: usize, first: &str, second: &str) -> Option<usize> {
    (from..to.min(t.len())).find(|&k| {
        ident_at(t, k, first)
            && punct_at(t, k + 1, ':')
            && punct_at(t, k + 2, ':')
            && ident_at(t, k + 3, second)
    })
}

/// `fn buy_on_the_payment_page`: the one function that is the v1 payment page's order button, once
/// per definition - what the wiring rule below must find to judge (a renamed or removed function
/// would leave it judging nothing).
fn v1_order_function(t: &[Located]) -> Vec<usize> {
    v1_order_fns(t)
        .into_iter()
        .map(|(name, _)| t[name].line)
        .collect()
}

/// A `buy_on_the_payment_page` that does not end a checkout in flight before it places the order:
/// its body must ask `must_release_first` and send `Event::Release` BEFORE the `Job::Checkout` it
/// spawns - a checkout in flight may be paid already or any moment, and the buyer never pays twice
/// for one thing (AZ-P4). The predicate `must_release_first` is tested on its own
/// (`add_drive_tests`); this is the test that deleting its call, or the release it sends, fails.
fn v1_order_without_release(t: &[Located]) -> Vec<usize> {
    v1_order_fns(t)
        .into_iter()
        .filter(|&(_, open)| {
            let close = block_end(t, open);
            let order = path_in(t, open, close, "Job", "Checkout").unwrap_or(close);
            let asks = (open..order).any(|k| ident_at(t, k, "must_release_first"));
            let releases = path_in(t, open, order, "Event", "Release").is_some();
            !(asks && releases)
        })
        .map(|(name, _)| t[name].line)
        .collect()
}

pub const V1_ORDER_FN: &str = "v1-order-function";
pub const V1_ORDER_RELEASE: &str = "v1-order-releases-first";

#[must_use]
pub fn wiring_rules() -> Vec<Rule> {
    vec![
        Rule::new(
            V1_ORDER_FN,
            v1_order_function,
            &["fn buy_on_the_payment_page() {}"],
            &["fn buy_on_the_card_page() {}", "let buy_on_the_payment_page = 1;"],
        ),
        Rule::new(
            V1_ORDER_RELEASE,
            v1_order_without_release,
            &[
                // no release at all
                "fn buy_on_the_payment_page() { spawn(Job::Checkout { serial }); }",
                // the release after the order
                "fn buy_on_the_payment_page() { spawn(Job::Checkout { serial }); \
                 if dialog_ref(s).is_some_and(must_release_first) { pay(Event::Release); } }",
                // the predicate asked, nothing sent
                "fn buy_on_the_payment_page() { if dialog_ref(s).is_some_and(must_release_first) { return; } \
                 spawn(Job::Checkout { serial }); }",
                // sent, never asked: every order would end the checkout it just made
                "fn buy_on_the_payment_page() { pay(Event::Release); spawn(Job::Checkout { serial }); }",
            ],
            &[
                "fn buy_on_the_payment_page() { \
                 if dialog_ref(s).is_some_and(must_release_first) { let _ = pay(info, app, s, Event::Release); return; } \
                 spawn(info, app, s, Job::Checkout { serial }); }",
                "fn buy_on_the_card_page() { spawn(Job::Checkout { serial }); }",
            ],
        ),
    ]
}

// ==== 7. The renewal toast is made from what the vault will do ====

/// A string literal that is one of the renewal toast's keys.
fn toast_key(t: &[Located]) -> Vec<usize> {
    strings(t, |s| {
        matches!(
            s,
            "azdrive-months-renewed"
                | "azdrive-months-renewed-next-look"
                | "azdrive-tier-change-bought"
        )
    })
}

/// A call of `bought_text(`: the one function that words a claimed renewal's toast.
fn toast_call(t: &[Located]) -> Vec<usize> {
    calls(t, "bought_text")
}

/// A call of `month_due_at_once(`: the fact about the drive's period `renewed` reads before it
/// asks for the words.
fn toast_fact(t: &[Located]) -> Vec<usize> {
    calls(t, "month_due_at_once")
}

/// The index of the `)` that closes the `(` at `open`.
fn paren_end(t: &[Located], open: usize) -> usize {
    let mut depth = 0_i32;
    for (j, x) in t.iter().enumerate().skip(open) {
        if x.tok.is_punct('(') {
            depth += 1;
        } else if x.tok.is_punct(')') {
            depth -= 1;
            if depth == 0 {
                return j;
            }
        }
    }
    t.len()
}

/// A call of `Takes::of(` with the fact about the drive's period among its arguments: what the
/// months do is decided by that fact, not by a value made up on the spot (X-DELTA-INTEGRITY3-20).
fn toast_fact_decides(t: &[Located]) -> Vec<usize> {
    (0..t.len())
        .filter(|&i| {
            ident_at(t, i, "Takes")
                && punct_at(t, i + 1, ':')
                && punct_at(t, i + 2, ':')
                && ident_at(t, i + 3, "of")
                && punct_at(t, i + 4, '(')
        })
        .filter(|&i| (i + 4..paren_end(t, i + 4)).any(|k| ident_at(t, k, "month_due_at_once")))
        .map(|i| t[i].line)
        .collect()
}

/// A call of `bought_text(` that is given the `takes` the page made of that fact: the words say what
/// the vault will do, not a sentence chosen on the side.
fn toast_words_follow(t: &[Located]) -> Vec<usize> {
    (1..t.len())
        .filter(|&i| {
            ident_at(t, i, "bought_text")
                && punct_at(t, i + 1, '(')
                && !ident_at(t, i - 1, "fn")
                && !punct_at(t, i - 1, '.')
        })
        .filter(|&i| (i + 1..paren_end(t, i + 1)).any(|k| ident_at(t, k, "takes")))
        .map(|i| t[i].line)
        .collect()
}

pub const TOAST_KEY: &str = "renewal-toast-key";
pub const TOAST_CALL: &str = "renewal-toast-call";
pub const TOAST_FACT: &str = "renewal-toast-fact";
pub const TOAST_DECIDES: &str = "renewal-toast-fact-decides";
pub const TOAST_FOLLOWS: &str = "renewal-toast-words-follow";

#[must_use]
pub fn toast_rules() -> Vec<Rule> {
    vec![
        Rule::new(
            TOAST_KEY,
            toast_key,
            &[
                "let k = \"azdrive-months-renewed\";",
                "let p = Phrase::new(\"azdrive-tier-change-bought\");",
            ],
            &[
                "let k = \"azdrive-add-buy\";",
                "let p = Phrase::new(\"azdrive-months-bought\");",
            ],
        ),
        Rule::new(
            TOAST_CALL,
            toast_call,
            &["let toast = bought_text(&name, months, takes);"],
            &[
                "let toast = s.bought_text(1);",
                "let toast = bought_texts(&name);",
            ],
        ),
        Rule::new(
            TOAST_FACT,
            toast_fact,
            &["let takes = Takes::of(change, month_due_at_once(s, id, now));"],
            &[
                "let due = s.month_due_at_once(id);",
                "let due = month_due_at_onceish(id);",
            ],
        ),
        // the fact reaches the function that says what the months do
        Rule::new(
            TOAST_DECIDES,
            toast_fact_decides,
            &[
                "let takes = Takes::of(change, month_due_at_once(s, id, now));",
                "let takes = Takes::of(change_tier, month_due_at_once(&s.period_facts(id), id, now));",
            ],
            &[
                "let takes = Takes::of(change, false);",
                "let due = month_due_at_once(s, id, now); let takes = Takes::of(change, due);",
                "let takes = Retakes::of(change, month_due_at_once(s, id, now));",
            ],
        ),
        // and what it says reaches the words
        Rule::new(
            TOAST_FOLLOWS,
            toast_words_follow,
            &[
                "let toast = bought_text(&name, months, takes);",
                "let t = bought_text(&n, 1, takes);",
            ],
            &[
                "let toast = bought_text(&name, months, Takes::AWeekBefore);",
                "fn bought_text(drive: &str, months: u32, takes: Takes) {}",
                "let toast = s.bought_text(takes);",
            ],
        ),
    ]
}

// ==== The allowances, and the gates ====

/// A `Settled` is made in `pending/proof.rs`, from evidence nobody else can construct.
pub const ALLOWED_SETTLED: &[Allow] = &[
    Allow::rule(
        "examples/azcloud-kit/src/pending/proof.rs",
        SETTLED_MADE,
        "the one file that makes a Settled, from evidence that has no public constructor either",
    ),
    Allow::rule(
        "examples/azcloud-kit/src/pending/proof.rs",
        SETTLED_BY,
        "the proof's own kinds, named where it is made",
    ),
    Allow::rule(
        "examples/azcloud-kit/src/pending/mod.rs",
        SETTLED_PATH,
        "the list asks the proof's constructors, which take the evidence this module has \
         and nobody else can make; the abandon's answer is read here",
    ),
];

/// Where a refusal's status or word is read. Counts only go down: a lane that types one of the
/// words below lowers its line, and the gate says so when it does.
pub const ALLOWED_REFUSALS: &[Allow] = &[
    // ---- the decoders: a token server's status and word become a type here, nowhere else ----
    Allow::file(
        "examples/azcloud-kit/src/token/refusal.rs",
        "Refusal::decode: the one place a status and a word become a typed refusal",
    ),
    Allow::file(
        "examples/azcloud-kit/src/answer.rs",
        "classify: the table that says what each status and word means (pass, wait, stop)",
    ),
    // ---- not decisions ----
    Allow::exactly(
        "examples/azcloud-kit/src/drive.rs",
        STATUS_LITERAL,
        1,
        "builds the drive's 401 out of a refused drive token (SignIn): a construction",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/user_errors.rs",
        CODE_WORD,
        7,
        "the errors table's own names (Code::as_str) for the contract's words: keyed by them, \
         deciding nothing",
    ),
];

/// Where an amount or a currency stands alone. All of them are the wire, a record of the wire,
/// or a registry's own text.
///
/// The rules read `amount`, `cents` and `price` names of every whole-number type and `Option` of
/// one, and `currency` as `String`, `Option<String>` or `&String`, since X-DELTA-AZDRIVE-MONEY-11
/// (they read `amount_cents: u64` and `currency: String` before): 7 sites then, 17 now - counted
/// by the gate's own scanner. The 10 new are code that stood outside the rule, none is new code:
/// seven more records of the contract's wire (the eighth was counted), `Cents` itself, the cash
/// slip's printed amount and the offer's currency text. The sites that were not records were
/// converted instead (`renewals::show` takes `Cents`, `Tier::of_offer`'s closure is untyped).
pub const ALLOWED_PAIRS: &[Allow] = &[
    Allow::exactly(
        "examples/azlin-contract/src/checkout.rs",
        BARE_AMOUNT,
        8,
        "the wire: the contract's records say what the token server says (CheckoutAnswer.\
         amount_cents, the ladder's month_cents / year_cents, a tier's price_cents_month / \
         price_cents_year / cents_per_tb_month, a receipt's gross_cents and EurAmountWire.cents); \
         the kit reads each with its currency into Cents (token/money.rs, token/checkout.rs)",
    ),
    Allow::exactly(
        "examples/azlin-contract/src/ids.rs",
        BARE_AMOUNT,
        1,
        "Cents itself: the amount beside its typed currency, the type every other site uses",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/pending/mod.rs",
        BARE_AMOUNT,
        1,
        "CashKept: the slip as the keyring keeps it (serde); CashKept::charge() reads it as Cents",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/pending/mod.rs",
        BARE_CURRENCY,
        1,
        "CashKept, as above",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/cash.rs",
        BARE_AMOUNT,
        1,
        "ActivationCode: the amount as printed on the slip, four bytes of the code (cash contract \
         v1); ActivationCode::check(&Cents) compares it",
    ),
    Allow::exactly(
        "examples/azcloud-kit/src/cash.rs",
        BARE_CURRENCY,
        1,
        "ActivationCode: the code as printed on the slip (cash contract v1); \
         ActivationCode::check(&Cents) compares it",
    ),
    Allow::exactly(
        "examples/azul-drive/src/jobs.rs",
        BARE_CURRENCY,
        1,
        "Job::Options: the payer's currency as a request parameter, with no amount",
    ),
    Allow::exactly(
        "examples/azul-pay/src/offer.rs",
        BARE_CURRENCY,
        1,
        "Offer.currency: the currency the token server priced the offer in, as it wrote it - no \
         amount stands beside it (the price has its own Cents)",
    ),
    Allow::exactly(
        "examples/azul-pay-methods/src/lib.rs",
        BARE_CURRENCY,
        2,
        "the methods registry's own data (a country's currency, the registry's Amount), parsed \
         from its text by its checked parser",
    ),
];

/// Nothing about money is defaulted: no allowance.
pub const ALLOWED_DEFAULTS: &[Allow] = &[];

/// Money is written for a person in one place.
pub const ALLOWED_SHOWN: &[Allow] = &[Allow::exactly(
    "examples/azul-drive/src/pay_words.rs",
    MONEY_SHOWN,
    1,
    "show(&Cents): the one place money is written for a person",
)];

/// Gate 1: a checkout leaves the keyring's list only with a `Settled` (RF51), which only
/// `pending/proof.rs` makes.
#[must_use]
pub fn settled_gate() -> Gate<'static> {
    Gate {
        name: "money_gate: Settled",
        roots: CLIENT,
        rules: settled_rules(),
        budget: Budget(ALLOWED_SETTLED),
        orphans: Orphans::Fail,
    }
}

/// Gate 2: a refusal is decided by its type, not by a status number or a word (RF51).
#[must_use]
pub fn refusal_gate(words: &[String]) -> Gate<'static> {
    Gate {
        name: "money_gate: refusals",
        roots: CLIENT,
        rules: refusal_rules(words),
        budget: Budget(ALLOWED_REFUSALS),
        orphans: Orphans::Fail,
    }
}

/// Gate 3: money is `Cents`, not a bare pair (RF52).
#[must_use]
pub fn pair_gate() -> Gate<'static> {
    Gate {
        name: "money_gate: bare pairs",
        roots: WIRE_AND_CLIENT,
        rules: pair_rules(),
        budget: Budget(ALLOWED_PAIRS),
        orphans: Orphans::Fail,
    }
}

/// Gate 4: no amount, VAT or currency is defaulted (RF52).
#[must_use]
pub fn default_gate() -> Gate<'static> {
    Gate {
        name: "money_gate: defaults",
        roots: WIRE_AND_CLIENT,
        rules: default_rules(),
        budget: Budget(ALLOWED_DEFAULTS),
        orphans: Orphans::Fail,
    }
}

/// The v1 order button is one function, in one file.
pub const ALLOWED_WIRING: &[Allow] = &[Allow::exactly(
    "examples/azul-drive/src/add_flow.rs",
    V1_ORDER_FN,
    1,
    "buy_on_the_payment_page: the one order button of the v1 payment page, which must release a \
     checkout in flight first",
)];

/// A renewal's toast is worded in one place from one fact: three keys in `renewals::bought_text`,
/// one call of it and one read of the drive's period in `add_flow::renewed`.
pub const ALLOWED_TOAST: &[Allow] = &[
    Allow::exactly(
        "examples/azul-drive/src/renewals.rs",
        TOAST_KEY,
        3,
        "bought_text: the three keys of the toast, one for each thing the vault does with the months",
    ),
    Allow::exactly(
        "examples/azul-drive/src/add_flow.rs",
        TOAST_CALL,
        1,
        "renewed: the toast of a claimed renewal says what the vault will do with its months",
    ),
    Allow::exactly(
        "examples/azul-drive/src/add_flow.rs",
        TOAST_FACT,
        1,
        "renewed: reads whether the drive's next month is due at once before it asks for the words",
    ),
    Allow::exactly(
        "examples/azul-drive/src/add_flow.rs",
        TOAST_DECIDES,
        1,
        "renewed: that fact is what decides what the months do (`Takes::of`), not a value made up \
         on the spot",
    ),
    Allow::exactly(
        "examples/azul-drive/src/add_flow.rs",
        TOAST_FOLLOWS,
        1,
        "renewed: the words are asked with the `takes` that fact decided",
    ),
];

/// Gate 7: a claimed renewal's toast is worded by `bought_text` from what the vault will do
/// (X-DELTA-MONEY2-13): no other place writes one of its keys, and `renewed` calls it.
#[must_use]
pub fn toast_gate() -> Gate<'static> {
    Gate {
        name: "money_gate: renewal toast",
        roots: CLIENT,
        rules: toast_rules(),
        budget: Budget(ALLOWED_TOAST),
        orphans: Orphans::Fail,
    }
}

/// Gate 6: the v1 order button ends a checkout in flight before it orders again (AZ-P4,
/// X-DELTA-AZDRIVE-MONEY-15).
#[must_use]
pub fn wiring_gate() -> Gate<'static> {
    Gate {
        name: "money_gate: v1 order wiring",
        roots: CLIENT,
        rules: wiring_rules(),
        budget: Budget(ALLOWED_WIRING),
        orphans: Orphans::Fail,
    }
}

/// Gate 5: money is written for a person by one function (RF52).
#[must_use]
pub fn show_gate() -> Gate<'static> {
    Gate {
        name: "money_gate: shown",
        roots: CLIENT,
        rules: show_rules(),
        budget: Budget(ALLOWED_SHOWN),
        orphans: Orphans::Fail,
    }
}
