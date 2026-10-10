//! The expression TYPESET, as a book sets it: `a/b` a fraction over a bar,
//! `x^2` a raised exponent, `sqrt(x)` a radical with its vinculum, `abs(x)`
//! between bars, `2 × x` as `2x`, `e^x` and `10^x` raised, `log2` with its
//! base lowered.
//!
//! azul has no MathML, and needs none: [`layout`] turns the parsed
//! expression into a small tree of boxes ([`M`]) - rows, fractions,
//! scripts, radicals, fences - and [`to_dom`] makes each a flex box: a
//! fraction is a column (numerator, a one-pixel rule, denominator), a
//! script a smaller box lifted by `position: relative`, a radical the root
//! sign beside a box whose top border is the vinculum, a fence's
//! parentheses grown with the fractions inside it. Text stays text: crisp
//! at any size, selectable by a screen reader.
//!
//! The entry being typed is set too: an operand still to come is a hole
//! (`□`), parentheses still open close at the end, and the letters of a
//! name being typed follow in the soft ink ([`entry`]).

use azul::prelude::*;

use crate::expr::{self, BinOp, Const, Expr, Func, Post, Tok};
use crate::look;
use crate::num::format_typed;

/// What an operand still to come shows as.
const HOLE: &str = "\u{25a1}";
/// The marker of a hole among the tokens (no number starts with it).
const HOLE_MARK: char = '\u{1}';
/// The marker of letters being typed among the tokens.
const WORD_MARK: char = '\u{2}';

/// A typeset box.
#[derive(Clone, Debug, PartialEq)]
pub enum M {
    /// Figures (`1,280`, `−3`).
    Num(String),
    /// A variable or `e`: italic.
    Var(String),
    /// A function's name (`sin`), upright; or a symbol like `π`.
    Ident(String),
    /// A binary operator, spaced (`+`, `−`, `mod`); an empty one is the
    /// thin space of an implicit product.
    Op(String),
    /// A sign before its operand (`−`, `NOT`), unspaced.
    Prefix(String),
    /// A sign after its operand (`!`, `%`).
    Suffix(String),
    Row(Vec<M>),
    Frac(Box<M>, Box<M>),
    /// Base, superscript.
    Sup(Box<M>, Box<M>),
    /// Base, subscript.
    Sub(Box<M>, Box<M>),
    /// The index (3 for the cube root), the radicand.
    Root(Option<Box<M>>, Box<M>),
    /// In parentheses.
    Paren(Box<M>),
    /// Between bars.
    Abs(Box<M>),
    /// An operand still to come.
    Hole,
    /// Letters being typed.
    Word(String),
}

impl M {
    /// How many fractions are stacked in it: what a fence or a radical
    /// around it grows by.
    #[must_use]
    pub fn height(&self) -> u32 {
        match self {
            M::Frac(a, b) => 1 + a.height().max(b.height()),
            M::Row(v) => v.iter().map(M::height).max().unwrap_or(0),
            M::Sup(a, _) | M::Sub(a, _) => a.height(),
            M::Root(_, a) | M::Paren(a) | M::Abs(a) => a.height(),
            _ => 0,
        }
    }

    /// The text a script reads (for tests and the accessible name).
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            M::Num(s) | M::Var(s) | M::Ident(s) | M::Prefix(s) | M::Suffix(s) | M::Word(s) => s.clone(),
            M::Op(s) if s.is_empty() => String::new(),
            M::Op(s) => format!(" {s} "),
            M::Row(v) => v.iter().map(M::text).collect(),
            M::Frac(a, b) => format!("({})/({})", a.text(), b.text()),
            M::Sup(a, b) => format!("{}^({})", a.text(), b.text()),
            M::Sub(a, b) => format!("{}_{}", a.text(), b.text()),
            M::Root(None, a) => format!("\u{221a}({})", a.text()),
            M::Root(Some(i), a) => format!("{}\u{221a}({})", i.text(), a.text()),
            M::Paren(a) => format!("({})", a.text()),
            M::Abs(a) => format!("|{}|", a.text()),
            M::Hole => HOLE.to_string(),
        }
    }
}

/// How tightly an expression binds, for the parentheses a parent needs
/// around it (higher binds tighter).
fn tightness(e: &Expr) -> u8 {
    match e {
        Expr::Bin(op, ..) => match op {
            BinOp::Add | BinOp::Sub => 2,
            BinOp::Mul | BinOp::Mod => 3,
            // A fraction is a box of its own.
            BinOp::Div => 5,
            BinOp::Pow => 6,
            _ => 1,
        },
        Expr::Neg(_) | Expr::Not(_) => 4,
        Expr::Post(..) => 7,
        Expr::Num(t) if t.starts_with('-') => 4,
        _ => 9,
    }
}

/// `e` in parentheses when it binds looser than `min`.
fn grouped(e: &Expr, min: u8, grouping: bool) -> M {
    let m = layout(e, grouping);
    if tightness(e) < min {
        M::Paren(Box::new(m))
    } else {
        m
    }
}

/// An operand that a script or a suffix sits on: anything but a single
/// number, letter or call goes in parentheses (`(−2)²`, `(a/b)²`, `(3!)²`).
fn base(e: &Expr, grouping: bool) -> M {
    let atomic = match e {
        Expr::Num(t) => !t.starts_with('-'),
        Expr::Var | Expr::Const(_) => true,
        Expr::Func(f, _) => !matches!(f, Func::Exp | Func::Pow10 | Func::Recip),
        _ => false,
    };
    let m = layout(e, grouping);
    if atomic {
        m
    } else {
        M::Paren(Box::new(m))
    }
}

/// Whether `2 × e` may be set `2e`: a number before a letter, a call or
/// something in parentheses.
fn implicit_product(a: &Expr, b: &Expr) -> bool {
    let number = matches!(a, Expr::Num(t) if !t.starts_with('-'));
    let starts_with_letter = match b {
        Expr::Var | Expr::Const(_) => true,
        Expr::Func(f, _) => !matches!(f, Func::Recip | Func::Pow10),
        Expr::Bin(BinOp::Pow, x, _) => matches!(x.as_ref(), Expr::Var | Expr::Const(_)),
        Expr::Post(Post::Square | Post::Cube, x) => matches!(x.as_ref(), Expr::Var | Expr::Const(_)),
        _ => false,
    };
    number && starts_with_letter
}

/// A function applied to its argument: a name and the argument in
/// parentheses, or the function's own form (a radical, bars, a power).
fn call(f: Func, x: &Expr, grouping: bool) -> M {
    let arg = || Box::new(layout(x, grouping));
    let named = |name: &str| M::Row(vec![M::Ident(name.to_string()), M::Paren(arg())]);
    let inverse = |name: &str| {
        M::Row(vec![
            M::Sup(Box::new(M::Ident(name.to_string())), Box::new(M::Num("\u{2212}1".to_string()))),
            M::Paren(arg()),
        ])
    };
    match f {
        Func::Sqrt => M::Root(None, arg()),
        Func::Cbrt => M::Root(Some(Box::new(M::Num("3".to_string()))), arg()),
        Func::Abs => M::Abs(arg()),
        Func::Exp => M::Sup(Box::new(M::Var("e".to_string())), arg()),
        Func::Pow10 => M::Sup(Box::new(M::Num("10".to_string())), arg()),
        Func::Recip => M::Frac(Box::new(M::Num("1".to_string())), arg()),
        Func::Log2 => M::Row(vec![
            M::Sub(Box::new(M::Ident("log".to_string())), Box::new(M::Num("2".to_string()))),
            M::Paren(arg()),
        ]),
        Func::Asin => inverse("sin"),
        Func::Acos => inverse("cos"),
        Func::Atan => inverse("tan"),
        other => named(other.name()),
    }
}

/// The parsed expression as boxes.
#[must_use]
pub fn layout(e: &Expr, grouping: bool) -> M {
    match e {
        Expr::Num(t) => {
            if t.starts_with(HOLE_MARK) {
                M::Hole
            } else if let Some(word) = t.strip_prefix(WORD_MARK) {
                M::Word(word.to_string())
            } else {
                M::Num(format_typed(t, grouping).replacen('-', "\u{2212}", 1))
            }
        }
        Expr::Var => M::Var("x".to_string()),
        Expr::Const(Const::Pi) => M::Ident("\u{3c0}".to_string()),
        Expr::Const(Const::E) => M::Var("e".to_string()),
        Expr::Neg(x) => M::Row(vec![M::Prefix("\u{2212}".to_string()), grouped(x, 3, grouping)]),
        Expr::Not(x) => M::Row(vec![M::Prefix("NOT ".to_string()), M::Paren(Box::new(layout(x, grouping)))]),
        Expr::Bin(op, a, b) => match op {
            BinOp::Div => M::Frac(Box::new(layout(a, grouping)), Box::new(layout(b, grouping))),
            BinOp::Pow => M::Sup(Box::new(base(a, grouping)), Box::new(layout(b, grouping))),
            BinOp::Add => M::Row(vec![grouped(a, 2, grouping), M::Op("+".to_string()), grouped(b, 2, grouping)]),
            BinOp::Sub => M::Row(vec![
                grouped(a, 2, grouping),
                M::Op("\u{2212}".to_string()),
                grouped(b, 3, grouping),
            ]),
            BinOp::Mul => {
                let sign = if implicit_product(a, b) { "" } else { "\u{b7}" };
                M::Row(vec![grouped(a, 3, grouping), M::Op(sign.to_string()), grouped(b, 3, grouping)])
            }
            BinOp::Mod => M::Row(vec![grouped(a, 3, grouping), M::Op("mod".to_string()), grouped(b, 4, grouping)]),
            other => M::Row(vec![
                grouped(a, 2, grouping),
                M::Op(other.symbol().to_string()),
                grouped(b, 2, grouping),
            ]),
        },
        Expr::Func(f, x) => call(*f, x, grouping),
        Expr::Post(p, x) => match p {
            Post::Square => M::Sup(Box::new(base(x, grouping)), Box::new(M::Num("2".to_string()))),
            Post::Cube => M::Sup(Box::new(base(x, grouping)), Box::new(M::Num("3".to_string()))),
            Post::Factorial => M::Row(vec![base(x, grouping), M::Suffix("!".to_string())]),
            Post::Percent => M::Row(vec![base(x, grouping), M::Suffix("%".to_string())]),
        },
    }
}

/// The entry being typed, set: `tokens` (the number being typed among
/// them), an operand still to come a hole, open parentheses closed, and
/// `letters` (a name being typed) at the end. `None` if even so the tokens
/// do not parse.
#[must_use]
pub fn entry(tokens: &[Tok], letters: &str, grouping: bool) -> Option<M> {
    let mut t: Vec<Tok> = tokens.to_vec();
    if !letters.is_empty() {
        t.push(Tok::Num(format!("{WORD_MARK}{letters}")));
    }
    let wants_operand = match t.last() {
        None => true,
        Some(last) => matches!(last, Tok::Op(_) | Tok::Neg | Tok::Not | Tok::Func(_) | Tok::LParen),
    };
    if wants_operand {
        t.push(Tok::Num(HOLE_MARK.to_string()));
    }
    let e = expr::parse(&t).ok()?;
    Some(layout(&e, grouping))
}

// ==== As boxes on screen ====

/// `m` as nested flex boxes (the font size is the parent's).
#[must_use]
pub fn to_dom(m: &M) -> Dom {
    match m {
        M::Num(s) => text(s, ""),
        M::Var(s) => text(s, "font-style: italic; padding-right: 0.06em;"),
        M::Ident(s) => text(s, "padding-right: 0.08em;"),
        M::Op(s) if s.is_empty() => Dom::create_div().with_css("width: 0.12em; flex-shrink: 0;"),
        M::Op(s) => text(s, "padding: 0px 0.24em;"),
        M::Prefix(s) => text(s, "padding-right: 0.04em;"),
        M::Suffix(s) => text(s, ""),
        M::Word(s) => text(s, &format!("font-style: italic; {}", look::MATH_DIM)),
        M::Hole => text(HOLE, look::MATH_DIM),
        M::Row(v) => row(v.iter().map(to_dom).collect()),
        M::Frac(a, b) => Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; align-items: center; \
                 margin: 0px 0.12em; font-size: 86%;",
            )
            .with_child(to_dom(a).with_css("padding: 0px 0.15em 1px 0.15em;"))
            .with_child(
                Dom::create_div()
                    .with_css(format!("align-self: stretch; height: 1px; flex-shrink: 0; {}", look::MATH_RULE)),
            )
            .with_child(to_dom(b).with_css("padding: 1px 0.15em 0px 0.15em;")),
        M::Sup(a, b) => Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: flex-start;")
            .with_child(to_dom(a))
            .with_child(
                Dom::create_div()
                    .with_css("font-size: 66%; position: relative; top: -0.45em; padding-left: 0.05em;")
                    .with_child(to_dom(b)),
            ),
        M::Sub(a, b) => Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: flex-end;")
            .with_child(to_dom(a))
            .with_child(
                Dom::create_div()
                    .with_css("font-size: 66%; position: relative; top: 0.3em; padding-left: 0.03em;")
                    .with_child(to_dom(b)),
            ),
        M::Root(index, body) => {
            let grow = 100 + 30 * body.height().min(4);
            let mut root = Dom::create_div().with_css("display: flex; flex-direction: row; align-items: stretch;");
            if let Some(i) = index {
                root.add_child(
                    Dom::create_div()
                        .with_css("font-size: 55%; position: relative; top: -0.2em; margin-right: -0.35em;")
                        .with_child(to_dom(i)),
                );
            }
            root.with_child(text("\u{221a}", &format!("font-size: {grow}%; line-height: 1;")))
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column;")
                        .with_child(
                            Dom::create_div()
                                .with_css(format!("height: 1px; flex-shrink: 0; {}", look::MATH_RULE)),
                        )
                        .with_child(to_dom(body).with_css("padding: 1px 0.12em 0px 0.08em;")),
                )
        }
        M::Paren(body) => {
            let grow = 100 + 45 * body.height().min(4);
            let fence = format!("font-size: {grow}%; line-height: 1;");
            row(vec![text("(", &fence), to_dom(body), text(")", &fence)])
        }
        M::Abs(body) => {
            let grow = 100 + 45 * body.height().min(4);
            let fence = format!("font-size: {grow}%; line-height: 1; padding: 0px 0.08em;");
            row(vec![text("|", &fence), to_dom(body), text("|", &fence)])
        }
    }
}

fn text(s: &str, css: &str) -> Dom {
    Dom::create_div_with_text(s.to_string()).with_css(format!("flex-shrink: 0; white-space: pre; {css}"))
}

fn row(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_children(DomVec::from_vec(children))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{parse, tokenize, Domain};

    fn set(text: &str) -> M {
        layout(&parse(&tokenize(text, Domain::Decimal).unwrap()).unwrap(), true)
    }

    #[test]
    fn division_is_a_fraction_and_powers_are_raised() {
        assert_eq!(set("1/2").text(), "(1)/(2)");
        assert!(matches!(set("1/2"), M::Frac(..)));
        assert_eq!(set("x^2").text(), "x^(2)");
        assert_eq!(set("(1+x)^2").text(), "(x+1)^(2)".replace("x+1", "1 + x"));
        assert_eq!(set("-2^2").text(), "\u{2212}2^(2)", "-2^2 is -(2^2)");
    }

    #[test]
    fn functions_take_their_own_forms() {
        assert_eq!(set("sqrt(x)").text(), "\u{221a}(x)");
        assert_eq!(set("cbrt(8)").text(), "3\u{221a}(8)");
        assert_eq!(set("abs(x)").text(), "|x|");
        assert_eq!(set("exp(x)").text(), "e^(x)");
        assert_eq!(set("sin(x)").text(), "sin(x)");
        assert_eq!(set("log2(8)").text(), "log_2(8)");
        assert_eq!(set("asin(1)").text(), "sin^(\u{2212}1)(1)");
    }

    #[test]
    fn a_number_times_a_letter_is_set_without_a_dot() {
        assert_eq!(set("2x").text(), "2x");
        assert_eq!(set("2*pi").text(), "2\u{3c0}");
        assert_eq!(set("x*x").text(), "x \u{b7} x");
        assert_eq!(set("sin(x)*x^2").text(), "sin(x) \u{b7} x^(2)");
        assert_eq!(set("2*3").text(), "2 \u{b7} 3");
    }

    #[test]
    fn parentheses_only_where_they_are_needed() {
        assert_eq!(set("(1+2)*3").text(), "(1 + 2) \u{b7} 3");
        assert_eq!(set("1-(2+3)").text(), "1 \u{2212} (2 + 3)");
        assert_eq!(set("1+(2+3)").text(), "1 + 2 + 3");
        assert_eq!(set("-(1+2)").text(), "\u{2212}(1 + 2)");
    }

    #[test]
    fn fractions_grow_the_fences_around_them() {
        assert_eq!(set("(1/2)").height(), 1);
        assert_eq!(set("1/(1/2)").height(), 2);
        assert_eq!(set("x+1").height(), 0);
    }

    #[test]
    fn the_entry_being_typed_has_holes_and_letters() {
        let t = tokenize("2 +", Domain::Decimal).unwrap();
        assert_eq!(entry(&t, "", true).unwrap().text(), "2 + \u{25a1}");
        assert_eq!(entry(&t, "si", true).unwrap().text(), "2 + si");
        let t = tokenize("sqrt(", Domain::Decimal).unwrap();
        assert_eq!(entry(&t, "", true).unwrap().text(), "\u{221a}(\u{25a1})");
        assert_eq!(entry(&[], "", true).unwrap().text(), "\u{25a1}");
        let t = tokenize("1280*0.19", Domain::Decimal).unwrap();
        assert_eq!(entry(&t, "", true).unwrap().text(), "1,280 \u{b7} 0.19");
    }
}
