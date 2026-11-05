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

/// The file names the token server's error: what "on a `TokenError`" means to these rules.
fn names_token_error(t: &[Located]) -> bool {
    t.iter().any(|x| x.tok.is_ident("TokenError"))
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

pub const SETTLED_MADE: &str = "settled-made";
pub const SETTLED_BY: &str = "settled-by";

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
    ]
}

// ==== 2. A refusal is decided by its type, not by a status number or a word ====

/// `status: 404` in a file that names `TokenError`: a pattern or a literal that decides by the
/// HTTP status.
fn status_literal(t: &[Located]) -> Vec<usize> {
    if !names_token_error(t) {
        return Vec::new();
    }
    (0..t.len())
        .filter(|&i| {
            t[i].tok.is_ident("status") && punct_at(t, i + 1, ':') && status_number_at(t, i + 2)
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
            !on_reply && comparison_at(t, after)
        })
        .map(|i| t[i].line)
        .collect()
}

/// `code == "word"`, `"word" == code`, `.contains(&code.as_str())`, `matches!(code.as_str(), ..)`
/// in a file that names `TokenError`.
fn code_compared(t: &[Located]) -> Vec<usize> {
    if !names_token_error(t) {
        return Vec::new();
    }
    (0..t.len())
        .filter(|&i| {
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
            &["match e { TokenError::Refused { status: 404, .. } => {} _ => {} }"],
            &[
                "match e { TokenError::Refused { status, .. } => drop(status), _ => {} }",
                "let reply = HttpReply { status: 200 };",
            ],
        ),
        Rule::new(
            STATUS_COMPARED,
            status_compared,
            &[
                "if matches!(e, TokenError::Busy { .. }) && status == 429 {}",
                "let gone = TokenError::Protocol(String::new()); if e.status() != 200 {}",
            ],
            &[
                "let signed_out = TokenError::SignIn(String::new()); if reply.status == 401 {}",
                "let ok = TokenError::Config(String::new()); let status = 4;",
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
            ],
            &[
                "let e: TokenError = x; if code == other {}",
                "let e: TokenError = x; let s = code.as_str();",
                "let e = 1; if code == \"word\" {}",
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

/// `amount_cents: u64` - a field or a parameter that holds an amount without its currency.
fn bare_amount(t: &[Located]) -> Vec<usize> {
    (0..t.len())
        .filter(|&i| {
            t[i].tok.is_ident("amount_cents")
                && punct_at(t, i + 1, ':')
                && ident_at(t, i + 2, "u64")
        })
        .map(|i| t[i].line)
        .collect()
}

/// `currency: String` - a currency as free text.
fn bare_currency(t: &[Located]) -> Vec<usize> {
    (0..t.len())
        .filter(|&i| {
            t[i].tok.is_ident("currency")
                && punct_at(t, i + 1, ':')
                && !punct_at(t, i + 2, ':')
                && ident_at(t, i + 2, "String")
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
            &["struct S { amount_cents: u64 }"],
            &[
                "struct S { price: Cents }",
                "let amount_cents = 5_u64; let n: u64 = 1;",
            ],
        ),
        Rule::new(
            BARE_CURRENCY,
            bare_currency,
            &["struct S { currency: String }"],
            &["struct S { currency: Currency }", "fn f(currency: &str) {}"],
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

/// `.unwrap_or("EUR")`, `.unwrap_or_else(|| String::from("EUR"))`: the euro as the answer to a
/// currency nobody named.
fn euro_defaulted(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for i in 1..t.len() {
        let name = match t[i].tok.ident() {
            Some(n @ ("unwrap_or" | "unwrap_or_else")) => n,
            _ => continue,
        };
        let _ = name;
        if !punct_at(t, i - 1, '.') || !punct_at(t, i + 1, '(') {
            continue;
        }
        let end = call_end(t, i + 1);
        let euro = t[i + 1..end]
            .iter()
            .any(|x| matches!(&x.tok, Tok::Str(s) if s.eq_ignore_ascii_case("EUR")));
        if euro {
            found.push(t[i].line);
        }
    }
    found
}

/// `.unwrap_or(0)` and `.unwrap_or_default()` in a statement that talks of an amount, cents, VAT
/// or a currency: a missing figure read as zero.
fn money_zeroed(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for i in 1..t.len() {
        let zero = ident_at(t, i, "unwrap_or")
            && punct_at(t, i + 1, '(')
            && ident_at(t, i + 2, "0")
            && punct_at(t, i + 3, ')');
        let default = ident_at(t, i, "unwrap_or_default") && punct_at(t, i + 1, '(');
        if !(zero || default) || !punct_at(t, i - 1, '.') {
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
            ],
            &[
                "let c = answer.currency.unwrap_or(\"PLN\");",
                "let c = Currency::parse(\"EUR\")?;",
                "let c = parsed(\"EUR\").unwrap_or(other);",
            ],
        ),
        Rule::new(
            MONEY_ZEROED,
            money_zeroed,
            &[
                "let a = answer.amount_cents.unwrap_or(0);",
                "let v = receipt.vat.unwrap_or_default();",
            ],
            &[
                "let n = count.unwrap_or(0);",
                "let a = answer.amount_cents.unwrap_or(1);",
                "let a = answer.amount_cents; let n = count.unwrap_or(0);",
            ],
        ),
    ]
}

// ==== 5. Money is shown by one function ====

/// `l10n::money(` and `money(` - the grouped digits and the currency's sign.
fn money_shown(t: &[Located]) -> Vec<usize> {
    let mut found = calls(t, "l10n::money");
    found.extend(calls(t, "money"));
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
        ],
        &["let s = show(&c);", "let m = c.money(5);"],
    )]
}

// ==== The allowances, and the gates ====

/// A `Settled` is made in `pending/proof.rs`, from evidence nobody else can construct.
pub const ALLOWED_SETTLED: &[Allow] = &[Allow::file(
    "examples/azcloud-kit/src/pending/proof.rs",
    "the one file that makes a Settled, from evidence that has no public constructor either",
)];

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
    // ---- debts of other lanes, counted ----
    Allow::exactly(
        "examples/azul-drive/src/encryption.rs",
        STATUS_LITERAL,
        5,
        "Z30-A1B: the recovery-key calls still match `Refused { status: 401, .. }`; the token \
         server's recovery refusals have no typed value yet",
    ),
    Allow::exactly(
        "examples/azul-drive/src/encryption.rs",
        CODE_COMPARED,
        3,
        "Z30-A1B: recovery_key_exists, no_pending_lockdown and no_recovery_key compared as words",
    ),
    Allow::exactly(
        "examples/azul-drive/src/recovery_keys.rs",
        CODE_COMPARED,
        1,
        "Z30-A1B: last_recovery_key compared as a word (user_errors has Code::LastRecoveryKey)",
    ),
    Allow::exactly(
        "examples/azul-drive/src/archive.rs",
        CODE_COMPARED,
        1,
        "bad_signature compared as a word (the contract's ArchiveRefusal::BadSignature)",
    ),
];

/// Where an amount or a currency stands alone. All of them are the wire, a record of the wire,
/// or a registry's own text.
pub const ALLOWED_PAIRS: &[Allow] = &[
    Allow::exactly(
        "examples/azlin-contract/src/checkout.rs",
        BARE_AMOUNT,
        1,
        "the wire: CheckoutAnswer.amount_cents is what the token server says; the kit reads it \
         with its currency into a Charge (token/money.rs)",
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
