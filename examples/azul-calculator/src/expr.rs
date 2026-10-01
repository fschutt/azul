//! The expression engine: tokens, a precedence parser and two evaluators.
//!
//! The keypad and the keyboard build a token list ([`Tok`]); pasted text is
//! turned into the same tokens by [`tokenize`]. [`parse`] makes a tree of
//! them with the usual precedence (from loose to tight):
//!
//! | level | operators |
//! |---|---|
//! | 1 | OR, NOR |
//! | 2 | XOR |
//! | 3 | AND, NAND |
//! | 4 | `<<` `>>` ROL ROR |
//! | 5 | `+` `-` |
//! | 6 | `x` `/` mod, implicit multiplication (`2pi`, `3(4)`) |
//! | 7 | unary minus, NOT, functions (`sin 30`) |
//! | 8 | `^` (right-associative: `2^3^2` = 2^9; `-2^2` = -4) |
//! | 9 | postfix `%` `!` `²` `³` |
//!
//! Open parentheses are closed at the end (`(1+2` is 3), as on a desk
//! calculator. The percent rules: `a + b%` = a + a*b/100, `a - b%` =
//! a - a*b/100, elsewhere `b%` = b/100 (`200 x 10%` = 20).
//!
//! [`eval_dec`] evaluates over decimals ([`Num`]) with an angle unit;
//! [`eval_int`] over Programmer mode's integers of a word size.

use crate::num::{CalcError, Num};
use crate::programmer::{self, Base, WordSize};

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Shl,
    Shr,
    Rol,
    Ror,
}

impl BinOp {
    /// How it is shown in the expression line.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "\u{2212}",
            BinOp::Mul => "\u{d7}",
            BinOp::Div => "\u{f7}",
            BinOp::Mod => "mod",
            BinOp::Pow => "^",
            BinOp::And => "AND",
            BinOp::Or => "OR",
            BinOp::Xor => "XOR",
            BinOp::Nand => "NAND",
            BinOp::Nor => "NOR",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Rol => "ROL",
            BinOp::Ror => "ROR",
        }
    }

    /// Left and right binding power (a right power below the left one makes
    /// the operator right-associative).
    fn binding(self) -> (u8, u8) {
        match self {
            BinOp::Or | BinOp::Nor => (10, 11),
            BinOp::Xor => (12, 13),
            BinOp::And | BinOp::Nand => (14, 15),
            BinOp::Shl | BinOp::Shr | BinOp::Rol | BinOp::Ror => (16, 17),
            BinOp::Add | BinOp::Sub => (20, 21),
            BinOp::Mul | BinOp::Div | BinOp::Mod => (30, 31),
            BinOp::Pow => (50, 49),
        }
    }

    /// Only meaningful on Programmer mode's integers.
    #[must_use]
    pub fn is_bitwise(self) -> bool {
        matches!(
            self,
            BinOp::And
                | BinOp::Or
                | BinOp::Xor
                | BinOp::Nand
                | BinOp::Nor
                | BinOp::Shl
                | BinOp::Shr
                | BinOp::Rol
                | BinOp::Ror
        )
    }
}

/// The binding power of a prefix operator (unary minus, NOT, a function).
const PREFIX: u8 = 40;

/// A function of one argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Asinh,
    Acosh,
    Atanh,
    /// Natural logarithm.
    Ln,
    /// Base-10 logarithm.
    Log,
    Log2,
    Sqrt,
    Cbrt,
    Abs,
    /// e^x.
    Exp,
    /// 10^x.
    Pow10,
    /// 1/x.
    Recip,
}

impl Func {
    const ALL: [Func; 21] = [
        Func::Sin,
        Func::Cos,
        Func::Tan,
        Func::Asin,
        Func::Acos,
        Func::Atan,
        Func::Sinh,
        Func::Cosh,
        Func::Tanh,
        Func::Asinh,
        Func::Acosh,
        Func::Atanh,
        Func::Ln,
        Func::Log,
        Func::Log2,
        Func::Sqrt,
        Func::Cbrt,
        Func::Abs,
        Func::Exp,
        Func::Pow10,
        Func::Recip,
    ];

    /// The name typed or pasted (`sin`, `sqrt`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Asin => "asin",
            Func::Acos => "acos",
            Func::Atan => "atan",
            Func::Sinh => "sinh",
            Func::Cosh => "cosh",
            Func::Tanh => "tanh",
            Func::Asinh => "asinh",
            Func::Acosh => "acosh",
            Func::Atanh => "atanh",
            Func::Ln => "ln",
            Func::Log => "log",
            Func::Log2 => "log2",
            Func::Sqrt => "sqrt",
            Func::Cbrt => "cbrt",
            Func::Abs => "abs",
            Func::Exp => "exp",
            Func::Pow10 => "pow10",
            Func::Recip => "recip",
        }
    }

    /// How it is shown in the expression line, before its `(`.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Func::Sqrt => "\u{221a}",
            Func::Cbrt => "\u{221b}",
            Func::Exp => "e^",
            Func::Pow10 => "10^",
            Func::Recip => "1/",
            other => other.name(),
        }
    }

    /// A function by name (any case; `arcsin`, `√`, `∛` too).
    #[must_use]
    pub fn by_name(name: &str) -> Option<Func> {
        let lower = name.to_lowercase();
        let alias = match lower.as_str() {
            "arcsin" => "asin",
            "arccos" => "acos",
            "arctan" => "atan",
            "\u{221a}" | "root" => "sqrt",
            "\u{221b}" => "cbrt",
            "lg" | "log10" => "log",
            "ld" | "lb" => "log2",
            other => other,
        };
        Func::ALL.into_iter().find(|f| f.name() == alias)
    }
}

/// A postfix operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Post {
    Percent,
    Factorial,
    Square,
    Cube,
}

impl Post {
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Post::Percent => "%",
            Post::Factorial => "!",
            Post::Square => "\u{b2}",
            Post::Cube => "\u{b3}",
        }
    }
}

/// A constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Const {
    Pi,
    E,
}

impl Const {
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Const::Pi => "\u{3c0}",
            Const::E => "e",
        }
    }

    /// The value to 50 digits.
    #[must_use]
    pub fn value(self) -> Num {
        let text = match self {
            Const::Pi => "3.1415926535897932384626433832795028841971693993751",
            Const::E => "2.7182818284590452353602874713526624977572470936999",
        };
        Num::parse(text).unwrap_or_else(|_| Num::zero())
    }
}

/// One token of an expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A number as typed: `1280`, `-0.19`, `1.5E3`; in Programmer mode the
    /// digits of the current base (`2A5F`).
    Num(String),
    Op(BinOp),
    /// Unary minus.
    Neg,
    /// Bitwise NOT.
    Not,
    Func(Func),
    LParen,
    RParen,
    Const(Const),
    Post(Post),
}

impl Tok {
    /// Whether an operand ends with this token (a following number or `(`
    /// multiplies).
    #[must_use]
    pub fn ends_operand(&self) -> bool {
        matches!(self, Tok::Num(_) | Tok::RParen | Tok::Const(_) | Tok::Post(_))
    }

    /// Whether an operand can start with this token.
    #[must_use]
    pub fn starts_operand(&self) -> bool {
        matches!(
            self,
            Tok::Num(_) | Tok::LParen | Tok::Const(_) | Tok::Func(_) | Tok::Neg | Tok::Not
        )
    }
}

/// What the numbers are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    /// Decimals (Standard, Scientific).
    Decimal,
    /// Programmer mode's integers, typed in this base.
    Integer(Base),
}

/// The angle unit of the trigonometric functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AngleUnit {
    #[default]
    Deg,
    Rad,
    Grad,
}

impl AngleUnit {
    pub const ALL: [AngleUnit; 3] = [AngleUnit::Deg, AngleUnit::Rad, AngleUnit::Grad];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AngleUnit::Deg => "DEG",
            AngleUnit::Rad => "RAD",
            AngleUnit::Grad => "GRAD",
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        AngleUnit::ALL.iter().position(|a| *a == self).unwrap_or(0)
    }
}

/// The parsed expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Num(String),
    Const(Const),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Func(Func, Box<Expr>),
    Post(Post, Box<Expr>),
}

fn syntax(what: &str) -> CalcError {
    CalcError::Syntax(what.to_string())
}

/// The words Programmer mode reads as operators (everything else made of
/// a-f is a hex number).
fn operator_word(word: &str) -> Option<Tok> {
    Some(match word {
        "and" => Tok::Op(BinOp::And),
        "or" => Tok::Op(BinOp::Or),
        "xor" => Tok::Op(BinOp::Xor),
        "nand" => Tok::Op(BinOp::Nand),
        "nor" => Tok::Op(BinOp::Nor),
        "not" => Tok::Not,
        "mod" => Tok::Op(BinOp::Mod),
        "shl" | "lsh" => Tok::Op(BinOp::Shl),
        "shr" | "rsh" => Tok::Op(BinOp::Shr),
        "rol" => Tok::Op(BinOp::Rol),
        "ror" => Tok::Op(BinOp::Ror),
        _ => return None,
    })
}

/// Whether the previous token leaves room for an operand (so `-` is a sign).
fn expects_operand(prev: Option<&Tok>) -> bool {
    match prev {
        None => true,
        Some(t) => matches!(t, Tok::Op(_) | Tok::Neg | Tok::Not | Tok::LParen | Tok::Func(_)),
    }
}

/// Turns typed or pasted text into tokens: `1,280 x 0.19`, `sin(30)+2^10`,
/// `2pi`, `5!`, `√2`, `50 + 10%`; in Programmer mode `2A5F and FF`, `1 << 4`.
pub fn tokenize(text: &str, domain: Domain) -> Result<Vec<Tok>, CalcError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // Programmer mode: a word that is an operator, else hex digits.
        if let Domain::Integer(base) = domain {
            if c.is_ascii_alphanumeric() {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
                if let Some(tok) = operator_word(&word) {
                    out.push(tok);
                    continue;
                }
                if word == "x" {
                    out.push(Tok::Op(BinOp::Mul));
                    continue;
                }
                let digits = word
                    .strip_prefix("0x")
                    .filter(|_| base == Base::Hex)
                    .unwrap_or(word.as_str());
                let digits = digits.strip_prefix("0b").filter(|_| base == Base::Bin).unwrap_or(digits);
                let digits = digits.strip_prefix("0o").filter(|_| base == Base::Oct).unwrap_or(digits);
                if digits.is_empty() || !digits.chars().all(|d| d.is_digit(base.radix())) {
                    return Err(CalcError::Syntax(format!(
                        "\"{word}\" is not a {} number",
                        base.label()
                    )));
                }
                out.push(Tok::Num(digits.to_uppercase()));
                continue;
            }
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let start = i;
            while i < chars.len() {
                let d = chars[i];
                if d.is_ascii_digit() || d == '.' {
                    i += 1;
                } else if d == ',' && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                    i += 1; // a grouping comma
                } else if (d == 'e' || d == 'E')
                    && (chars.get(i + 1).is_some_and(char::is_ascii_digit)
                        || (matches!(chars.get(i + 1), Some('+' | '-'))
                            && chars.get(i + 2).is_some_and(char::is_ascii_digit)))
                {
                    i += 2;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    break;
                } else {
                    break;
                }
            }
            let literal: String = chars[start..i].iter().filter(|c| **c != ',').collect();
            if literal.matches('.').count() > 1 {
                return Err(CalcError::Syntax(format!("{literal:?} has two points")));
            }
            out.push(Tok::Num(literal));
            continue;
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphabetic() {
                i += 1;
            }
            let mut word: String = chars[start..i].iter().collect();
            // The two names that end in digits: log2 and pow10.
            for (stem, digits) in [("log", "2"), ("pow", "10")] {
                let follows: String = chars[i..].iter().take(digits.len()).collect();
                let after = chars.get(i + digits.len());
                if word.eq_ignore_ascii_case(stem)
                    && follows == digits
                    && !after.is_some_and(char::is_ascii_digit)
                {
                    word.push_str(digits);
                    i += digits.len();
                }
            }
            let lower = word.to_lowercase();
            let tok = match lower.as_str() {
                "pi" | "\u{3c0}" => Tok::Const(Const::Pi),
                "e" => Tok::Const(Const::E),
                "mod" => Tok::Op(BinOp::Mod),
                "x" => Tok::Op(BinOp::Mul),
                _ => match Func::by_name(&lower) {
                    Some(f) => Tok::Func(f),
                    None => return Err(CalcError::Syntax(format!("unknown name \"{word}\""))),
                },
            };
            out.push(tok);
            continue;
        }
        let next = chars.get(i + 1).copied();
        let tok = match c {
            '+' => {
                if expects_operand(out.last()) {
                    i += 1;
                    continue; // a plus sign changes nothing
                }
                Tok::Op(BinOp::Add)
            }
            '-' | '\u{2212}' => {
                if expects_operand(out.last()) {
                    Tok::Neg
                } else {
                    Tok::Op(BinOp::Sub)
                }
            }
            '*' | '\u{d7}' | '\u{b7}' => Tok::Op(BinOp::Mul),
            '/' | '\u{f7}' | ':' => Tok::Op(BinOp::Div),
            '^' => Tok::Op(BinOp::Pow),
            '%' => Tok::Post(Post::Percent),
            '!' => Tok::Post(Post::Factorial),
            '\u{b2}' => Tok::Post(Post::Square),
            '\u{b3}' => Tok::Post(Post::Cube),
            '(' | '[' => Tok::LParen,
            ')' | ']' => Tok::RParen,
            '\u{3c0}' => Tok::Const(Const::Pi),
            '\u{221a}' => Tok::Func(Func::Sqrt),
            '\u{221b}' => Tok::Func(Func::Cbrt),
            '&' => Tok::Op(BinOp::And),
            '|' => Tok::Op(BinOp::Or),
            '~' => Tok::Not,
            '<' if next == Some('<') => {
                i += 1;
                Tok::Op(BinOp::Shl)
            }
            '>' if next == Some('>') => {
                i += 1;
                Tok::Op(BinOp::Shr)
            }
            other => return Err(CalcError::Syntax(format!("unexpected \"{other}\""))),
        };
        out.push(tok);
        i += 1;
    }
    Ok(out)
}

/// The tokens with the implicit multiplications written out: `2π` is
/// `2 x π`, `3(4)` is `3 x (4)`, `(1)(2)` is `(1) x (2)`.
#[must_use]
pub fn with_implicit_mul(tokens: &[Tok]) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(tokens.len());
    for t in tokens {
        if let Some(prev) = out.last() {
            if prev.ends_operand() && t.starts_operand() && !matches!(t, Tok::Neg) {
                out.push(Tok::Op(BinOp::Mul));
            }
        }
        out.push(t.clone());
    }
    out
}

/// How many `(` are still open.
#[must_use]
pub fn open_parens(tokens: &[Tok]) -> usize {
    let mut depth: usize = 0;
    for t in tokens {
        match t {
            Tok::LParen => depth += 1,
            Tok::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expr(&mut self, min_bp: u8) -> Result<Expr, CalcError> {
        let mut lhs = self.prefix()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(op)) => *op,
                Some(Tok::RParen) | None => break,
                Some(other) => {
                    return Err(CalcError::Syntax(format!("unexpected {}", tok_text(other))))
                }
            };
            let (lbp, rbp) = op.binding();
            if lbp < min_bp {
                break;
            }
            self.pos += 1;
            if self.peek().is_none() {
                return Err(syntax("Incomplete expression"));
            }
            let rhs = self.expr(rbp)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn prefix(&mut self) -> Result<Expr, CalcError> {
        match self.peek() {
            Some(Tok::Neg) => {
                self.pos += 1;
                Ok(Expr::Neg(Box::new(self.operand_of_prefix()?)))
            }
            Some(Tok::Not) => {
                self.pos += 1;
                Ok(Expr::Not(Box::new(self.operand_of_prefix()?)))
            }
            _ => self.postfix(),
        }
    }

    fn operand_of_prefix(&mut self) -> Result<Expr, CalcError> {
        if self.peek().is_none() {
            return Err(syntax("Incomplete expression"));
        }
        self.expr(PREFIX)
    }

    fn postfix(&mut self) -> Result<Expr, CalcError> {
        let mut e = self.primary()?;
        while let Some(Tok::Post(p)) = self.peek() {
            e = Expr::Post(*p, Box::new(e));
            self.pos += 1;
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Expr, CalcError> {
        match self.next() {
            Some(Tok::Num(text)) => Ok(Expr::Num(text)),
            Some(Tok::Const(c)) => Ok(Expr::Const(c)),
            Some(Tok::LParen) => {
                if matches!(self.peek(), Some(Tok::RParen)) {
                    return Err(syntax("Empty parentheses"));
                }
                if self.peek().is_none() {
                    return Err(syntax("Incomplete expression"));
                }
                let inner = self.expr(0)?;
                match self.next() {
                    Some(Tok::RParen) | None => Ok(inner), // closed, or closed at the end
                    Some(other) => Err(CalcError::Syntax(format!("unexpected {}", tok_text(&other)))),
                }
            }
            Some(Tok::Func(f)) => {
                if self.peek().is_none() {
                    return Err(syntax("Incomplete expression"));
                }
                Ok(Expr::Func(f, Box::new(self.expr(PREFIX)?)))
            }
            Some(Tok::Neg) | Some(Tok::Not) => {
                self.pos -= 1;
                self.prefix()
            }
            Some(Tok::RParen) => Err(syntax("Unmatched \")\"")),
            Some(Tok::Op(op)) => Err(CalcError::Syntax(format!("unexpected {}", op.symbol()))),
            Some(Tok::Post(p)) => Err(CalcError::Syntax(format!("unexpected {}", p.symbol()))),
            None => Err(syntax("Incomplete expression")),
        }
    }
}

fn tok_text(t: &Tok) -> String {
    match t {
        Tok::Num(n) => n.clone(),
        Tok::Op(op) => op.symbol().to_string(),
        Tok::Neg => "\u{2212}".to_string(),
        Tok::Not => "NOT".to_string(),
        Tok::Func(f) => f.symbol().to_string(),
        Tok::LParen => "(".to_string(),
        Tok::RParen => ")".to_string(),
        Tok::Const(c) => c.symbol().to_string(),
        Tok::Post(p) => p.symbol().to_string(),
    }
}

/// Parses tokens (implicit multiplications are added first; open
/// parentheses close at the end).
pub fn parse(tokens: &[Tok]) -> Result<Expr, CalcError> {
    if tokens.is_empty() {
        return Err(syntax("Empty expression"));
    }
    let toks = with_implicit_mul(tokens);
    let mut p = Parser { toks: &toks, pos: 0 };
    let e = p.expr(0)?;
    match p.peek() {
        None => Ok(e),
        Some(Tok::RParen) => Err(syntax("Unmatched \")\"")),
        Some(other) => Err(CalcError::Syntax(format!("unexpected {}", tok_text(other)))),
    }
}

/// The expression line: `1,280 × 0.19`, `sin(30) + 2^10`, `√(2)`, `5!`.
#[must_use]
pub fn display(tokens: &[Tok], grouping: bool, programmer: bool) -> String {
    let mut out = String::new();
    for t in tokens {
        match t {
            Tok::Op(op) => {
                let spaced = !matches!(op, BinOp::Pow);
                if spaced {
                    out.push(' ');
                    out.push_str(op.symbol());
                    out.push(' ');
                } else {
                    out.push_str(op.symbol());
                }
            }
            Tok::Num(n) => {
                if programmer {
                    out.push_str(n);
                } else {
                    out.push_str(&crate::num::format_typed(n, grouping));
                }
            }
            Tok::Not => out.push_str("NOT "),
            other => out.push_str(&tok_text(other)),
        }
    }
    out
}

// ==== Decimal evaluation ====

/// Trigonometric results this close to zero are zero (sin(pi), cos(90deg)).
const TRIG_ZERO: f64 = 1e-14;

fn to_radians(v: &Num, angle: AngleUnit) -> Result<f64, CalcError> {
    // Whole turns off first, exactly, so sin(1e20 deg) is not f64 noise.
    let x = match angle {
        AngleUnit::Deg => v.rem(&Num::from_i64(360))?.to_f64()?,
        AngleUnit::Grad => v.rem(&Num::from_i64(400))?.to_f64()?,
        AngleUnit::Rad => v.to_f64()?,
    };
    Ok(match angle {
        AngleUnit::Deg => x.to_radians(),
        AngleUnit::Rad => x,
        AngleUnit::Grad => x * std::f64::consts::PI / 200.0,
    })
}

fn from_radians(r: f64, angle: AngleUnit) -> f64 {
    match angle {
        AngleUnit::Deg => r.to_degrees(),
        AngleUnit::Rad => r,
        AngleUnit::Grad => r * 200.0 / std::f64::consts::PI,
    }
}

/// sin / cos / tan of an angle that is a whole multiple of a quarter turn, exactly.
fn quarter_turn(f: Func, v: &Num, angle: AngleUnit) -> Result<Option<Num>, CalcError> {
    let quarter = match angle {
        AngleUnit::Deg => Num::from_i64(90),
        AngleUnit::Grad => Num::from_i64(100),
        AngleUnit::Rad => return Ok(None),
    };
    if !v.rem(&quarter)?.is_zero() {
        return Ok(None);
    }
    let k = v.div(&quarter)?.rem(&Num::from_i64(4))?.to_i64_exact().unwrap_or(0);
    let k = k.rem_euclid(4);
    let (s, c) = [(0, 1), (1, 0), (0, -1), (-1, 0)][k as usize];
    Ok(Some(match f {
        Func::Sin => Num::from_i64(s),
        Func::Cos => Num::from_i64(c),
        _ => {
            if c == 0 {
                return Err(CalcError::InvalidInput);
            }
            Num::from_i64(s * c)
        }
    }))
}

fn snap(v: f64) -> f64 {
    if v.abs() < TRIG_ZERO {
        0.0
    } else {
        v
    }
}

/// 10^t for a large or fractional t: 10^frac (f64) times 10^int (exact).
fn pow10_f64(t: f64) -> Result<Num, CalcError> {
    if !t.is_finite() || t > crate::num::MAX_EXPONENT as f64 + 1.0 {
        return Err(CalcError::Overflow);
    }
    if t < -(crate::num::MAX_EXPONENT as f64) - 1.0 {
        return Ok(Num::zero());
    }
    let k = t.floor();
    let mantissa = Num::from_f64(10f64.powf(t - k))?;
    mantissa.mul(&Num::pow10(k as i64)?)
}

fn eval_func(f: Func, v: &Num, angle: AngleUnit) -> Result<Num, CalcError> {
    match f {
        Func::Sin | Func::Cos | Func::Tan => {
            if let Some(exact) = quarter_turn(f, v, angle)? {
                return Ok(exact);
            }
            let r = to_radians(v, angle)?;
            let y = match f {
                Func::Sin => snap(r.sin()),
                Func::Cos => snap(r.cos()),
                _ => {
                    let c = r.cos();
                    if c.abs() < TRIG_ZERO {
                        return Err(CalcError::InvalidInput);
                    }
                    snap(r.sin() / c)
                }
            };
            Num::from_f64(y)
        }
        Func::Asin | Func::Acos => {
            let x = v.to_f64()?;
            if !(-1.0..=1.0).contains(&x) {
                return Err(CalcError::InvalidInput);
            }
            let r = if f == Func::Asin { x.asin() } else { x.acos() };
            Num::from_f64(from_radians(r, angle))
        }
        Func::Atan => Num::from_f64(from_radians(v.to_f64()?.atan(), angle)),
        Func::Sinh => v.via_f64(|x| Some(x.sinh())),
        Func::Cosh => v.via_f64(|x| Some(x.cosh())),
        Func::Tanh => v.via_f64(|x| Some(x.tanh())),
        Func::Asinh => v.via_f64(|x| Some(x.asinh())),
        Func::Acosh => v.via_f64(|x| (x >= 1.0).then(|| x.acosh())),
        Func::Atanh => v.via_f64(|x| (x.abs() < 1.0).then(|| x.atanh())),
        Func::Ln | Func::Log | Func::Log2 => {
            if v.is_negative() || v.is_zero() {
                return Err(CalcError::InvalidInput);
            }
            if f == Func::Log {
                // A power of ten has an exact logarithm: log(1000) = 3.
                let sci = v.normalized().format(&crate::num::Format {
                    digits: crate::num::WORKING_DIGITS,
                    grouping: false,
                    scientific: true,
                });
                if sci.starts_with("1e") {
                    return Ok(Num::from_i64(v.exponent().unwrap_or(0)));
                }
            }
            // ln / log of a number beyond f64: through its exponent.
            let e = v.exponent().unwrap_or(0);
            let scaled = v.div(&Num::pow10(e)?)?.to_f64()?;
            let log10 = scaled.log10() + e as f64;
            Num::from_f64(match f {
                Func::Ln => log10 * std::f64::consts::LN_10,
                Func::Log => log10,
                _ => log10 * std::f64::consts::LOG2_10,
            })
        }
        Func::Sqrt => v.sqrt(),
        Func::Cbrt => v.cbrt(),
        Func::Abs => Ok(v.abs()),
        Func::Exp => pow10_f64(v.to_f64()? * std::f64::consts::LOG10_E),
        Func::Pow10 => match v.to_i64_exact() {
            Some(n) => Num::pow10(n),
            None => pow10_f64(v.to_f64()?),
        },
        Func::Recip => Num::from_i64(1).div(v),
    }
}

/// Evaluates over decimals; trigonometric functions use `angle`.
pub fn eval_dec(e: &Expr, angle: AngleUnit) -> Result<Num, CalcError> {
    match e {
        Expr::Num(text) => Num::parse(text),
        Expr::Const(c) => Ok(c.value()),
        Expr::Neg(x) => Ok(eval_dec(x, angle)?.neg()),
        Expr::Not(_) => Err(syntax("NOT needs Programmer mode")),
        Expr::Bin(op, l, r) => {
            // The percent rules: a + b% and a - b% take b percent OF a.
            if let (BinOp::Add | BinOp::Sub, Expr::Post(Post::Percent, pct)) = (op, r.as_ref()) {
                let base = eval_dec(l, angle)?;
                let delta = base.mul(&eval_dec(pct, angle)?)?.percent()?;
                return if *op == BinOp::Add {
                    base.add(&delta)
                } else {
                    base.sub(&delta)
                };
            }
            let a = eval_dec(l, angle)?;
            let b = eval_dec(r, angle)?;
            match op {
                BinOp::Add => a.add(&b),
                BinOp::Sub => a.sub(&b),
                BinOp::Mul => a.mul(&b),
                BinOp::Div => a.div(&b),
                BinOp::Mod => a.rem(&b),
                BinOp::Pow => a.pow(&b),
                _ => Err(CalcError::Syntax(format!("{} needs Programmer mode", op.symbol()))),
            }
        }
        Expr::Func(f, x) => eval_func(*f, &eval_dec(x, angle)?, angle),
        Expr::Post(p, x) => {
            let v = eval_dec(x, angle)?;
            match p {
                Post::Percent => v.percent(),
                Post::Factorial => v.factorial(),
                Post::Square => v.mul(&v),
                Post::Cube => v.mul(&v)?.mul(&v),
            }
        }
    }
}

/// Text to a decimal result (paste, tests).
pub fn evaluate_text(text: &str, angle: AngleUnit) -> Result<Num, CalcError> {
    let tokens = tokenize(text, Domain::Decimal)?;
    eval_dec(&parse(&tokens)?, angle)
}

// ==== Integer evaluation ====

/// Evaluates over Programmer mode's integers: every step wraps to the word;
/// literals are in `base`.
pub fn eval_int(e: &Expr, word: WordSize, base: Base) -> Result<i128, CalcError> {
    let w = |v: i128| word.wrap(v);
    match e {
        Expr::Num(text) => programmer::parse_int(text, base, word),
        Expr::Neg(x) => Ok(w(eval_int(x, word, base)?.wrapping_neg())),
        Expr::Not(x) => Ok(w(!eval_int(x, word, base)?)),
        Expr::Bin(op, l, r) => {
            let a = eval_int(l, word, base)?;
            let b = eval_int(r, word, base)?;
            Ok(match op {
                BinOp::Add => w(a.wrapping_add(b)),
                BinOp::Sub => w(a.wrapping_sub(b)),
                BinOp::Mul => w(a.wrapping_mul(b)),
                BinOp::Div => {
                    if b == 0 {
                        return Err(CalcError::DivideByZero);
                    }
                    w(a.wrapping_div(b))
                }
                BinOp::Mod => {
                    if b == 0 {
                        return Err(CalcError::DivideByZero);
                    }
                    w(a.wrapping_rem(b))
                }
                BinOp::Pow => {
                    if b < 0 {
                        return Err(CalcError::InvalidInput);
                    }
                    let (mut result, mut square, mut n) = (1i128, a, b);
                    while n > 0 {
                        if n & 1 == 1 {
                            result = w(result.wrapping_mul(square));
                        }
                        square = w(square.wrapping_mul(square));
                        n >>= 1;
                    }
                    result
                }
                BinOp::And => w(a & b),
                BinOp::Or => w(a | b),
                BinOp::Xor => w(a ^ b),
                BinOp::Nand => w(!(a & b)),
                BinOp::Nor => w(!(a | b)),
                BinOp::Shl => programmer::shift_left(a, b, word)?,
                BinOp::Shr => programmer::shift_right(a, b, word)?,
                BinOp::Rol => programmer::rotate_left(a, b, word)?,
                BinOp::Ror => programmer::rotate_right(a, b, word)?,
            })
        }
        Expr::Post(Post::Square, x) => {
            let v = eval_int(x, word, base)?;
            Ok(w(v.wrapping_mul(v)))
        }
        Expr::Post(Post::Cube, x) => {
            let v = eval_int(x, word, base)?;
            Ok(w(w(v.wrapping_mul(v)).wrapping_mul(v)))
        }
        Expr::Post(Post::Factorial, x) => {
            let v = eval_int(x, word, base)?;
            if v < 0 {
                return Err(CalcError::InvalidInput);
            }
            let mut acc = 1i128;
            for i in 2..=v.min(200) {
                acc = w(acc.wrapping_mul(i));
            }
            Ok(acc)
        }
        Expr::Post(Post::Percent, _) => Err(syntax("% needs Standard or Scientific mode")),
        Expr::Const(_) | Expr::Func(..) => {
            Err(syntax("Functions need Standard or Scientific mode"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::Format;

    fn ev(text: &str) -> String {
        match evaluate_text(text, AngleUnit::Deg) {
            Ok(v) => v.format(&Format::default()),
            Err(e) => format!("error: {e}"),
        }
    }

    fn ev_rad(text: &str) -> String {
        evaluate_text(text, AngleUnit::Rad)
            .map(|v| v.format(&Format::default()))
            .unwrap_or_else(|e| format!("error: {e}"))
    }

    fn int(text: &str, word: WordSize, base: Base) -> Result<i128, CalcError> {
        let tokens = tokenize(text, Domain::Integer(base))?;
        eval_int(&parse(&tokens)?, word, base)
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(ev("1 + 2 * 3"), "7");
        assert_eq!(ev("(1 + 2) * 3"), "9");
        assert_eq!(ev("10 - 4 - 3"), "3", "left-associative");
        assert_eq!(ev("100 / 10 / 5"), "2");
        assert_eq!(ev("1,280 x 0.19"), "243.2");
        assert_eq!(ev("1280\u{d7}0.19"), "243.2");
    }

    #[test]
    fn powers_are_right_associative_and_tighter_than_unary_minus() {
        assert_eq!(ev("2^3^2"), "512");
        assert_eq!(ev("-2^2"), "-4");
        assert_eq!(ev("(-2)^2"), "4");
        assert_eq!(ev("2^-1"), "0.5");
        assert_eq!(ev("2 * 3^2"), "18");
    }

    #[test]
    fn unary_minus_and_plus_signs() {
        assert_eq!(ev("-3 + 5"), "2");
        assert_eq!(ev("3 - -2"), "5");
        assert_eq!(ev("+4"), "4");
        assert_eq!(ev("-(2 + 3)"), "-5");
        assert_eq!(ev("2 * -3"), "-6");
    }

    #[test]
    fn open_parentheses_close_at_the_end() {
        assert_eq!(ev("(1 + 2"), "3");
        assert_eq!(ev("2 * (3 + (4"), "14");
        assert_eq!(open_parens(&tokenize("2*(3+(4", Domain::Decimal).unwrap()), 2);
        assert_eq!(ev("1 + 2)"), "error: Unmatched \")\"");
        assert_eq!(ev("()"), "error: Empty parentheses");
    }

    #[test]
    fn implicit_multiplication() {
        assert_eq!(ev("2(3 + 4)"), "14");
        assert_eq!(ev("(1 + 1)(2 + 2)"), "8");
        assert_eq!(ev("2pi"), ev("2 * pi"));
        assert_eq!(ev("3 sqrt(4)"), "6");
        assert_eq!(
            with_implicit_mul(&[Tok::Num("2".into()), Tok::Const(Const::Pi)]),
            vec![Tok::Num("2".into()), Tok::Op(BinOp::Mul), Tok::Const(Const::Pi)]
        );
    }

    #[test]
    fn the_percent_rules() {
        assert_eq!(ev("50 + 10%"), "55");
        assert_eq!(ev("50 - 10%"), "45");
        assert_eq!(ev("200 * 10%"), "20");
        assert_eq!(ev("200 / 10%"), "2,000");
        assert_eq!(ev("10%"), "0.1");
        assert_eq!(ev("1,000 + 2.5%"), "1,025");
    }

    #[test]
    fn functions_constants_and_postfix_operators() {
        assert_eq!(ev("sqrt(2)"), "1.4142135623730950488016887242097");
        assert_eq!(ev("\u{221a}16"), "4");
        assert_eq!(ev("5!"), "120");
        assert_eq!(ev("3\u{b2}"), "9");
        assert_eq!(ev("2\u{b3}"), "8");
        assert_eq!(ev("abs(-7)"), "7");
        assert_eq!(ev("ln(e)"), "1");
        assert_eq!(ev("log(1000)"), "3");
        assert_eq!(ev("log(0.01)"), "-2");
        assert_eq!(ev("log2(8)"), "3");
        assert_eq!(ev("log2 8"), "3");
        assert_eq!(ev("log 100"), "2");
        assert_eq!(ev("exp(0)"), "1");
        assert_eq!(ev("pow10(3)"), "1,000");
        assert_eq!(ev("recip(4)"), "0.25");
        assert_eq!(ev("17 mod 5"), "2");
        assert_eq!(ev("cbrt(-8)"), "-2");
        assert!(ev("pi").len() > 30);
        assert_eq!(ev("2x3"), "6");
        assert_eq!(ev("\u{3c0}"), ev("pi"));
    }

    #[test]
    fn the_scientific_sample_sin_30_plus_2_to_the_10() {
        assert_eq!(ev("sin(30) + 2^10"), "1,024.5");
    }

    #[test]
    fn degrees_radians_and_grads() {
        assert_eq!(ev("sin(90)"), "1");
        assert_eq!(ev("cos(90)"), "0");
        assert_eq!(ev("cos(60)"), "0.5");
        assert_eq!(ev("tan(45)"), "1");
        assert_eq!(ev("tan(90)"), "error: Invalid input");
        assert_eq!(ev("sin(-270)"), "1");
        assert_eq!(ev("asin(0.5)"), "30");
        assert_eq!(ev("acos(2)"), "error: Invalid input");
        assert_eq!(ev_rad("sin(pi)"), "0");
        assert_eq!(ev_rad("cos(pi)"), "-1");
        assert!(ev_rad("atan(1) * 4").starts_with("3.14159265358979"));
        let grad = evaluate_text("sin(100)", AngleUnit::Grad).unwrap();
        assert_eq!(grad.format(&Format::default()), "1");
    }

    #[test]
    fn functions_outside_their_domain_are_invalid_input() {
        assert_eq!(ev("sqrt(-1)"), "error: Invalid input");
        assert_eq!(ev("ln(0)"), "error: Invalid input");
        assert_eq!(ev("log(-5)"), "error: Invalid input");
        assert_eq!(ev("(2.5)!"), "error: Invalid input");
        assert_eq!(ev("acosh(0.5)"), "error: Invalid input");
        assert_eq!(ev("1/0"), "error: Cannot divide by zero");
        assert_eq!(ev("recip(0)"), "error: Cannot divide by zero");
    }

    #[test]
    fn huge_exponentials_stay_decimal_beyond_f64() {
        assert!(ev("exp(1000)").starts_with("1.9700711140"), "{}", ev("exp(1000)"));
        assert!(ev("exp(1000)").ends_with("e+434"), "{}", ev("exp(1000)"));
        let ln = ev("ln(exp(1000))");
        assert!(ln.starts_with("1,000") || ln.starts_with("999.99999"), "{ln}");
        assert_eq!(ev("exp(100000)"), "error: Overflow");
        assert_eq!(ev("10^9999 * 10"), "error: Overflow");
    }

    #[test]
    fn scientific_notation_literals_and_the_constant_e() {
        assert_eq!(ev("1.5e3"), "1,500");
        assert_eq!(ev("2e-2"), "0.02");
        assert_eq!(ev("2e"), ev("2 * e"), "a lone e after a number is the constant");
        assert_eq!(ev("1E+2"), "100");
    }

    #[test]
    fn incomplete_or_garbled_input_says_what_is_wrong() {
        assert_eq!(ev("2 +"), "error: Incomplete expression");
        assert_eq!(ev("sin"), "error: Incomplete expression");
        assert_eq!(ev("* 3"), "error: unexpected \u{d7}");
        assert_eq!(ev("foo(2)"), "error: unknown name \"foo\"");
        assert_eq!(ev("1.2.3"), "error: \"1.2.3\" has two points");
        assert_eq!(ev(""), "error: Empty expression");
        assert_eq!(ev("2 # 3"), "error: unexpected \"#\"");
    }

    #[test]
    fn the_expression_line_shows_typographic_operators_and_grouping() {
        let t = tokenize("1280*0.19", Domain::Decimal).unwrap();
        assert_eq!(display(&t, true, false), "1,280 \u{d7} 0.19");
        let t = tokenize("sqrt(2)+3^2-1%", Domain::Decimal).unwrap();
        assert_eq!(display(&t, true, false), "\u{221a}(2) + 3^2 \u{2212} 1%");
        let t = tokenize("-5!", Domain::Decimal).unwrap();
        assert_eq!(display(&t, true, false), "\u{2212}5!");
    }

    #[test]
    fn programmer_expressions_wrap_to_the_word() {
        assert_eq!(int("2A5F", WordSize::Qword, Base::Hex).unwrap(), 10847);
        assert_eq!(int("FF + 1", WordSize::Byte, Base::Hex).unwrap(), 0);
        assert_eq!(int("7F + 1", WordSize::Byte, Base::Hex).unwrap(), -128);
        assert_eq!(int("0 - 1", WordSize::Word, Base::Dec).unwrap(), -1);
        assert_eq!(int("17 / 5", WordSize::Qword, Base::Dec).unwrap(), 3, "integer division");
        assert_eq!(int("-17 / 5", WordSize::Qword, Base::Dec).unwrap(), -3, "towards zero");
        assert_eq!(int("17 mod 5", WordSize::Qword, Base::Dec).unwrap(), 2);
        assert_eq!(int("2 ^ 10", WordSize::Qword, Base::Dec).unwrap(), 1024);
        assert_eq!(int("2 ^ 64", WordSize::Qword, Base::Dec).unwrap(), 0);
        assert_eq!(int("1 / 0", WordSize::Qword, Base::Dec), Err(CalcError::DivideByZero));
    }

    #[test]
    fn programmer_bitwise_operators_and_their_precedence() {
        assert_eq!(int("F0 and 3C", WordSize::Qword, Base::Hex).unwrap(), 0x30);
        assert_eq!(int("F0 or 0F", WordSize::Qword, Base::Hex).unwrap(), 0xFF);
        assert_eq!(int("FF xor 0F", WordSize::Qword, Base::Hex).unwrap(), 0xF0);
        assert_eq!(int("not 0", WordSize::Byte, Base::Hex).unwrap(), -1);
        assert_eq!(int("~0", WordSize::Qword, Base::Dec).unwrap(), -1);
        assert_eq!(int("FF nand 0F", WordSize::Byte, Base::Hex).unwrap(), WordSize::Byte.wrap(0xF0));
        assert_eq!(int("1 << 4", WordSize::Qword, Base::Dec).unwrap(), 16);
        assert_eq!(int("1 + 1 << 4", WordSize::Qword, Base::Dec).unwrap(), 32, "shift is looser than +");
        assert_eq!(int("1 or 2 and 3", WordSize::Qword, Base::Dec).unwrap(), 3, "and before or");
        assert_eq!(int("81 rol 1", WordSize::Byte, Base::Hex).unwrap(), 3);
        assert_eq!(int("1010 & 0110", WordSize::Qword, Base::Bin).unwrap(), 0b0010);
    }

    #[test]
    fn hex_words_that_are_not_operators_are_numbers() {
        assert_eq!(int("add", WordSize::Qword, Base::Hex).unwrap(), 0xADD);
        assert_eq!(int("bad + 1", WordSize::Qword, Base::Hex).unwrap(), 0xBAE);
        assert_eq!(int("0x10", WordSize::Qword, Base::Hex).unwrap(), 16);
        assert!(tokenize("12", Domain::Integer(Base::Bin)).is_err());
        assert!(int("sin(3)", WordSize::Qword, Base::Dec).is_err());
        assert!(int("5 %", WordSize::Qword, Base::Dec).is_err());
        assert_eq!(int("3 x 4", WordSize::Qword, Base::Dec).unwrap(), 12);
    }
}
