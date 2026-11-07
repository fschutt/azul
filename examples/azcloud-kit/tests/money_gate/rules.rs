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
            (i + 3..(i + 24).min(t.len())).any(|k| {
                t[k].tok.is_ident(bound)
                    && operator_len(t, k + 1).is_some_and(|len| status_number_at(t, k + 1 + len))
            })
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
            let after = if punct_at(t, i + 1, '(') && punct_at(t, i + 2, ')') {
                i + 3
            } else {
                i + 1
            };
            !on_reply && operator_len(t, after).is_some_and(|len| status_number_at(t, after + len))
        })
        .map(|i| t[i].line)
        .collect()
}

/// `code == "word"`, `"word" == code`, `.contains(&code.as_str())`, `matches!(code.as_str(), ..)`
/// and a question about a word put to the code or to the error's text - `code.starts_with("..")`,
/// `code.as_str().contains("..")`, `e.to_string().contains("..")`, `e.said().ends_with("..")` -
/// in a file that names `TokenError` or `Refusal`.
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
            {
                let on_code = ident_at(t, i - 2, "code");
                let on_call = punct_at(t, i - 2, ')')
                    && i >= 4
                    && punct_at(t, i - 3, '(')
                    && (ident_at(t, i - 4, "to_string")
                        || ident_at(t, i - 4, "said")
                        || (ident_at(t, i - 4, "as_str") && i >= 6 && ident_at(t, i - 6, "code")));
                if on_code || on_call {
                    return true;
                }
            }
            if !t[i].tok.is_ident("code") {
                return false;
            }
            let against_word = comparison_at(t, i + 1) && string_at(t, i + 3);
            let word_against = i >= 3 && comparison_at(t, i - 2) && string_at(t, i - 3);
            let as_str = punct_at(t, i + 1, '.') && ident_at(t, i + 2, "as_str");
            let in_list =
                as_str && i >= 3 && ident_at(t, i - 3, "contains") && punct_at(t, i - 1, '&');
            let in_match =
                as_str && i >= 3 && ident_at(t, i - 3, "matches") && punct_at(t, i - 2, '!');
            against_word || word_against || in_list || in_match
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
            ],
            &[
                "match e { TokenError::Refused { status, .. } => drop(status), _ => {} }",
                "let reply = HttpReply { status: 200 };",
                "match e { TokenError::Refused { status: s, .. } => drop(s), _ => {} }",
                "match e { TokenError::Refused { status: s, .. } if other == 404 => drop(s), _ => {} }",
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
            ],
            &[
                "let signed_out = TokenError::SignIn(String::new()); if reply.status == 401 {}",
                "let ok = TokenError::Config(String::new()); let status = 4;",
                "let e: TokenError = x; if status == other {}",
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
            ],
            &[
                "let e: TokenError = x; if code == other {}",
                "let e: TokenError = x; let s = code.as_str();",
                "let e = 1; if code == \"word\" {}",
                "let e: TokenError = x; if text.contains(\"word\") {}",
                "let e: TokenError = x; if e.to_string().contains(&needle) {}",
                "let e = 1; if code.starts_with(\"lock_\") {}",
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
/// `(0, ..)`, `(|| 0)`, `(|| 0_i64, ..)`.
fn zero_default_at(t: &[Located], open: usize) -> bool {
    let a = open + 1;
    let ends = |i: usize| punct_at(t, i, ')') || punct_at(t, i, ',');
    (zero_at(t, a) && ends(a + 1))
        || (punct_at(t, a, '|') && punct_at(t, a + 1, '|') && zero_at(t, a + 2) && ends(a + 3))
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
            ],
            &[
                "let n = count.unwrap_or(0);",
                "let a = answer.amount_cents.unwrap_or(1);",
                "let a = answer.amount_cents; let n = count.unwrap_or(0);",
                "let n = hits.map_or(0, |h| h.len());",
                "let a = answer.amount_cents.map_or(1, |a| a);",
                "let n = count.unwrap_or_else(|| 0u64);",
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
