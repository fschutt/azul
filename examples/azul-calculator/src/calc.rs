//! The calculator's input model: what a key press does to the expression.
//!
//! The keypad and the keyboard send [`Cmd`]s; the model keeps the token
//! list of the expression being built ([`Tok`]), the number being typed (the
//! last token, while `editing`), the last result, the memory and the
//! history, and answers the two display lines: the expression line
//! (`1,280 × 0.19 =`) and the result line (`243.2`). It knows nothing about
//! azul: every rule is a unit test.
//!
//! The rules follow the desk calculators people know (Windows, GNOME, macOS):
//! a digit after `=` starts over, an operator after `=` continues with the
//! result, `=` again repeats the last operation (`+ 2 = =`), a second
//! operator replaces the first (`5 + ×` is `5 ×`), `5 × =` is `5 × 5`,
//! a function key wraps the current operand (`√(2)`) or opens `sin(` when
//! there is none, `+/-` changes the sign of the number being typed (or of
//! its exponent after Exp), CE clears the entry, C everything but the
//! memory and the history, Backspace deletes a digit or the last token.
//!
//! TYPING ([`Calculator::type_char`]) takes the CHARACTER the user's
//! keyboard layout produced - `*` is Shift+8 on a US keyboard and Shift++ on
//! a German one, the key position says nothing - and names letter by letter
//! (`sqrt`, `sin`, `pi`, `x`; [`crate::typing`]). An entry with `x` in it is a
//! function: `=` hands it to the graph ([`Plot`]) instead of evaluating it,
//! and `y =` starts one explicitly.

use crate::expr::{self, AngleUnit, BinOp, Const, Domain, Func, Post, Tok};
use crate::history::{HistoryEntry, Memory, MAX_HISTORY};
use crate::num::{format_typed, CalcError, Format, Num, DISPLAY_DIGITS};
use crate::programmer::{self, Base, WordSize};
use crate::typing::{self, Word};

/// The keypad modes (Date and Convert are other screens).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CalcMode {
    #[default]
    Standard,
    Scientific,
    Programmer,
}

impl CalcMode {
    /// The name in the history file and the settings.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            CalcMode::Standard => "standard",
            CalcMode::Scientific => "scientific",
            CalcMode::Programmer => "programmer",
        }
    }

    #[must_use]
    pub fn by_key(key: &str) -> Option<CalcMode> {
        [CalcMode::Standard, CalcMode::Scientific, CalcMode::Programmer]
            .into_iter()
            .find(|m| m.key() == key)
    }

    #[must_use]
    pub fn is_programmer(self) -> bool {
        self == CalcMode::Programmer
    }
}

/// A key's meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// 0..=9, and A..=F (10..=15) in Programmer mode.
    Digit(u8),
    Point,
    Op(BinOp),
    /// A function KEY: wraps the current operand (`√(2)`), or opens the call.
    Func(Func),
    /// A function NAME typed: always opens `name(` (`2 sqrt` is `2 × √(`).
    Call(Func),
    Post(Post),
    /// +/-.
    Negate,
    /// Exp: scientific-notation entry (`1.5E3`).
    Exp,
    Const(Const),
    /// The graph's variable `x`.
    Var,
    LParen,
    RParen,
    Equals,
    ClearEntry,
    Clear,
    Backspace,
    /// Bitwise NOT (Programmer).
    Not,
    MemClear,
    MemRecall,
    MemAdd,
    MemSub,
    MemStore,
}

/// A result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Dec(Num),
    Int(i128),
}

/// A function of `x` the graph plots, committed with `=` (or `y = ...`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plot {
    pub tokens: Vec<Tok>,
    /// As the expression line shows it: `sin(x) × x^2`.
    pub text: String,
}

/// The most typed significant digits a decimal number takes.
pub const MAX_TYPED_DIGITS: usize = 32;

/// The most functions the graph keeps (the oldest goes first).
pub const MAX_PLOTS: usize = 8;

/// `1` -> `₁`: the graph's function names (`y₁`).
#[must_use]
pub fn subscript(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|c| match c.to_digit(10) {
            Some(d) => char::from_u32(0x2080 + d).unwrap_or(c),
            None => c,
        })
        .collect()
}

/// The calculator.
#[derive(Clone, Debug, PartialEq)]
pub struct Calculator {
    pub mode: CalcMode,
    /// The expression being built.
    pub tokens: Vec<Tok>,
    /// The last token is a number still being typed.
    pub editing: bool,
    /// `=` was the last key: the expression line shows `shown_expr`.
    pub just_evaluated: bool,
    /// The expression line after `=`: `1,280 × 0.19 =`.
    pub shown_expr: String,
    /// The tokens of `shown_expr` (the typeset view draws them).
    pub shown_tokens: Vec<Tok>,
    /// The last result.
    pub result: Option<Value>,
    /// The error the result line shows instead of a number.
    pub error: Option<String>,
    pub angle: AngleUnit,
    /// F-E: results in scientific notation.
    pub fe: bool,
    /// 2nd: the function row's second meanings.
    pub second: bool,
    pub word: WordSize,
    pub base: Base,
    /// Thousands grouping in the display.
    pub grouping: bool,
    pub memory: Memory,
    /// Oldest first.
    pub history: Vec<HistoryEntry>,
    /// The last operation, repeated by `=` (`+ 2`).
    pub last_op: Option<(BinOp, String)>,
    /// A history entry was added since the history was last saved.
    pub history_dirty: bool,
    /// Letters typed that are not a name yet (`si` on the way to `sin`):
    /// shown where a number being typed is shown.
    pub letters: String,
    /// A typed name opened its own `(`: a `(` typed right after it is that one.
    pub skip_paren: bool,
    /// `y =` was typed: the entry is a function for the graph.
    pub defining: bool,
    /// The graph's functions, oldest first.
    pub plots: Vec<Plot>,
    /// `=` plotted the entry as `plots[i]` (the result line says so).
    pub plotted: Option<usize>,
    /// What the last typed character could not be, for the notice line.
    pub hint: Option<String>,
}

impl Default for Calculator {
    fn default() -> Self {
        Calculator::new()
    }
}

/// A decimal literal as an integer of the word, if it IS an integer.
fn dec_literal_to_int(text: &str, word: WordSize) -> Option<i128> {
    let n = Num::parse(text).ok()?;
    if !n.is_integer() {
        return None;
    }
    dec_to_int(&n, word)
}

/// The integer part of a decimal value, wrapped to the word (what a result
/// becomes in Programmer mode); `None` beyond 10^32.
fn dec_to_int(n: &Num, word: WordSize) -> Option<i128> {
    let lit = n.to_literal();
    if let Some(p) = lit.find('e') {
        return lit[p..].contains('-').then_some(0);
    }
    let whole = lit.split('.').next()?;
    whole.parse::<i128>().ok().map(|v| word.wrap(v))
}

/// The base a letter right after a lone `0` switches Programmer mode to
/// (`0x` HEX, `0b` BIN, `0o` OCT, `0d` DEC) - unless the base takes the
/// letter as a digit (`0b` in HEX is the number B).
fn base_prefix(c: char, base: Base) -> Option<Base> {
    let target = match c.to_ascii_lowercase() {
        'x' => Base::Hex,
        'b' => Base::Bin,
        'o' => Base::Oct,
        'd' => Base::Dec,
        _ => return None,
    };
    if c.to_digit(16).is_some_and(|d| base.accepts(d as u8)) {
        return None;
    }
    Some(target)
}

impl Calculator {
    #[must_use]
    pub fn new() -> Calculator {
        Calculator {
            mode: CalcMode::Standard,
            tokens: Vec::new(),
            editing: false,
            just_evaluated: false,
            shown_expr: String::new(),
            shown_tokens: Vec::new(),
            result: None,
            error: None,
            angle: AngleUnit::Deg,
            fe: false,
            second: false,
            word: WordSize::Qword,
            base: Base::Dec,
            grouping: true,
            memory: Memory::default(),
            history: Vec::new(),
            last_op: None,
            history_dirty: false,
            letters: String::new(),
            skip_paren: false,
            defining: false,
            plots: Vec::new(),
            plotted: None,
            hint: None,
        }
    }

    fn domain(&self) -> Domain {
        if self.mode.is_programmer() {
            Domain::Integer(self.base)
        } else {
            Domain::Decimal
        }
    }

    fn format(&self) -> Format {
        Format {
            digits: DISPLAY_DIGITS,
            grouping: self.grouping,
            scientific: self.fe,
        }
    }

    /// The names this mode's keyboard knows.
    fn names(&self) -> &'static [(&'static str, Word)] {
        if self.mode.is_programmer() {
            typing::PROGRAMMER_NAMES
        } else {
            typing::DECIMAL_NAMES
        }
    }

    /// A value as the result line shows it.
    #[must_use]
    pub fn show(&self, v: &Value) -> String {
        match v {
            Value::Dec(n) => n.format(&self.format()),
            Value::Int(i) => programmer::format_int(*i, self.base, self.word, self.grouping),
        }
    }

    /// A value as a literal to type on with (plain, in the current base).
    fn literal(&self, v: &Value) -> String {
        match v {
            Value::Dec(n) => n.to_literal(),
            Value::Int(i) => programmer::format_int(*i, self.base, self.word, false),
        }
    }

    /// Whether the entry is a function of `x` (the graph's, not a number).
    #[must_use]
    pub fn entry_is_function(&self) -> bool {
        !self.just_evaluated && (self.defining || expr::contains_var(&self.tokens))
    }

    // ==== The two display lines ====

    /// The expression line: after `=` the evaluated expression with ` =`;
    /// otherwise what is built so far without the number being typed.
    #[must_use]
    pub fn expression_line(&self) -> String {
        if self.just_evaluated {
            return self.shown_expr.clone();
        }
        let end = if self.editing {
            self.tokens.len().saturating_sub(1)
        } else {
            self.tokens.len()
        };
        let line = expr::display(&self.tokens[..end], self.grouping, self.mode.is_programmer())
            .trim_end()
            .to_string();
        if self.defining {
            return if line.is_empty() {
                "y =".to_string()
            } else {
                format!("y = {line}")
            };
        }
        line
    }

    /// The result line: the error, the name or number being typed (as
    /// typed), `f(x)` for a function, or the value of what is built so far.
    #[must_use]
    pub fn result_line(&self) -> String {
        if let Some(e) = &self.error {
            return e.clone();
        }
        if self.just_evaluated {
            if let Some(i) = self.plotted {
                return format!("Plotted as y{}", subscript(i + 1));
            }
        } else {
            if !self.letters.is_empty() {
                return self.letters.clone();
            }
            if self.editing {
                if let Some(Tok::Num(text)) = self.tokens.last() {
                    return if self.mode.is_programmer() {
                        text.clone()
                    } else {
                        format_typed(text, self.grouping)
                    };
                }
            }
            if self.entry_is_function() {
                return "f(x)".to_string();
            }
        }
        match self.current_value() {
            Some(v) => self.show(&v),
            None => "0".to_string(),
        }
    }

    /// How many `(` are open.
    #[must_use]
    pub fn open_parens(&self) -> usize {
        expr::open_parens(&self.tokens)
    }

    /// The value the display stands for: the result after `=`, the number
    /// being typed, or what is built so far (an unfinished tail left off).
    #[must_use]
    pub fn current_value(&self) -> Option<Value> {
        if self.just_evaluated {
            return self.result.clone();
        }
        if self.editing {
            if let Some(Tok::Num(text)) = self.tokens.last() {
                return self.eval_tokens(std::slice::from_ref(&Tok::Num(text.clone()))).ok();
            }
        }
        let mut end = self.tokens.len();
        while end > 0
            && matches!(
                self.tokens[end - 1],
                Tok::Op(_) | Tok::Neg | Tok::Not | Tok::Func(_) | Tok::LParen
            )
        {
            end -= 1;
        }
        if end == 0 {
            return None;
        }
        self.eval_tokens(&self.tokens[..end]).ok()
    }

    /// The four bases of Programmer mode's current value.
    #[must_use]
    pub fn programmer_lines(&self) -> Vec<(Base, String)> {
        let v = match self.current_value() {
            Some(Value::Int(i)) => i,
            _ => 0,
        };
        Base::ALL
            .iter()
            .map(|b| (*b, programmer::format_int(v, *b, self.word, true)))
            .collect()
    }

    /// The text Ctrl+C copies: the result line's value without grouping, or
    /// a function as it is written.
    #[must_use]
    pub fn copy_text(&self) -> String {
        if self.entry_is_function() {
            return expr::display(&self.tokens, false, false);
        }
        if self.just_evaluated {
            if let Some(plot) = self.plotted.and_then(|i| self.plots.get(i)) {
                return expr::display(&plot.tokens, false, false);
            }
        }
        match self.current_value() {
            Some(Value::Dec(n)) => n.format(&Format {
                digits: DISPLAY_DIGITS,
                grouping: false,
                scientific: self.fe,
            }),
            Some(Value::Int(i)) => programmer::format_int(i, self.base, self.word, false),
            None => "0".to_string(),
        }
    }

    fn eval_tokens(&self, tokens: &[Tok]) -> Result<Value, CalcError> {
        let e = expr::parse(tokens)?;
        if self.mode.is_programmer() {
            expr::eval_int(&e, self.word, self.base).map(Value::Int)
        } else {
            expr::eval_dec(&e, self.angle).map(Value::Dec)
        }
    }

    // ==== Commands ====

    /// Starts over after `=` or an error (the memory, the history and the
    /// graph's functions stay).
    fn fresh(&mut self) {
        self.tokens.clear();
        self.editing = false;
        self.just_evaluated = false;
        self.shown_expr.clear();
        self.shown_tokens.clear();
        self.error = None;
        self.letters.clear();
        self.skip_paren = false;
        self.defining = false;
        self.plotted = None;
    }

    /// After `=`: the result becomes the first operand.
    fn continue_with_result(&mut self) {
        let lit = self.result.clone().map(|v| self.literal(&v));
        self.fresh();
        if let Some(lit) = lit {
            self.tokens.push(Tok::Num(lit));
        }
    }

    /// The number being typed is done: `5.` is `5`, a dangling `E` goes.
    fn finish_number(&mut self) {
        if self.editing {
            if let Some(Tok::Num(text)) = self.tokens.last_mut() {
                if let Some(p) = text.find('E') {
                    if !text[p + 1..].chars().any(|c| c.is_ascii_digit()) {
                        text.truncate(p);
                    }
                }
                if text.ends_with('.') {
                    text.pop();
                }
                if text.is_empty() || text == "-" {
                    self.tokens.pop();
                }
            }
        }
        self.editing = false;
    }

    fn last_ends_operand(&self) -> bool {
        self.tokens.last().is_some_and(Tok::ends_operand)
    }

    /// Before a new operand: after `=` start over; a finished number is
    /// replaced (a recalled value, a result); after another operand an
    /// explicit `×` goes in.
    fn begin_operand(&mut self) {
        if self.just_evaluated || self.error.is_some() {
            self.fresh();
            return;
        }
        if !self.editing && matches!(self.tokens.last(), Some(Tok::Num(_))) {
            self.tokens.pop();
            return;
        }
        if self.last_ends_operand() {
            self.finish_number();
            self.tokens.push(Tok::Op(BinOp::Mul));
        }
    }

    /// Where the last operand starts (its sign, function or parentheses included).
    fn operand_start(&self) -> Option<usize> {
        let mut i = self.tokens.len().checked_sub(1)?;
        // Postfix operators belong to the operand before them.
        while matches!(self.tokens[i], Tok::Post(_)) {
            i = i.checked_sub(1)?;
        }
        match &self.tokens[i] {
            Tok::Num(_) | Tok::Const(_) | Tok::Var => {}
            Tok::RParen => {
                let mut depth = 0usize;
                loop {
                    match self.tokens[i] {
                        Tok::RParen => depth += 1,
                        Tok::LParen => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i = i.checked_sub(1)?;
                }
                if i > 0 && matches!(self.tokens[i - 1], Tok::Func(_) | Tok::Not) {
                    i -= 1;
                }
            }
            _ => return None,
        }
        while i > 0 && matches!(self.tokens[i - 1], Tok::Neg) {
            i -= 1;
        }
        Some(i)
    }

    /// Wraps the current operand (or the result) in `head ( ... )`.
    fn wrap_operand(&mut self, head: Tok) -> bool {
        if self.just_evaluated {
            if self.result.is_none() {
                return false;
            }
            self.continue_with_result();
        }
        self.finish_number();
        match self.operand_start() {
            Some(start) => {
                let operand: Vec<Tok> = self.tokens.drain(start..).collect();
                self.tokens.push(head);
                self.tokens.push(Tok::LParen);
                self.tokens.extend(operand);
                self.tokens.push(Tok::RParen);
                true
            }
            None => false,
        }
    }

    /// Applies one key. `now` (seconds since 1970) stamps a history entry.
    pub fn apply(&mut self, cmd: Cmd, now: u64) {
        self.hint = None;
        // Letters being typed: Backspace and CE work on them; any other key
        // takes the name typed so far first (`si` + `n` was `sin`, `sin` + `7`
        // opens `sin(7`).
        if !self.letters.is_empty() {
            match cmd {
                Cmd::Backspace => {
                    self.letters.pop();
                    return;
                }
                Cmd::ClearEntry => {
                    self.letters.clear();
                    return;
                }
                Cmd::Clear => self.letters.clear(),
                _ => self.flush_word(now),
            }
        }
        self.skip_paren = false;
        let programmer = self.mode.is_programmer();
        match cmd {
            Cmd::Digit(d) => self.digit(d),
            Cmd::Point => {
                if programmer {
                    return;
                }
                if self.editing {
                    if let Some(Tok::Num(text)) = self.tokens.last_mut() {
                        if !text.contains('.') && !text.contains('E') {
                            if text.is_empty() || text == "-" {
                                text.push('0');
                            }
                            text.push('.');
                        }
                    }
                    return;
                }
                self.begin_operand();
                self.tokens.push(Tok::Num("0.".to_string()));
                self.editing = true;
            }
            Cmd::Op(op) => self.operator(op),
            Cmd::Func(f) => {
                if programmer {
                    return;
                }
                if !self.wrap_operand(Tok::Func(f)) {
                    if self.just_evaluated || self.error.is_some() {
                        self.fresh();
                    }
                    self.tokens.push(Tok::Func(f));
                    self.tokens.push(Tok::LParen);
                }
            }
            Cmd::Call(f) => {
                if programmer {
                    return;
                }
                self.begin_operand();
                self.tokens.push(Tok::Func(f));
                self.tokens.push(Tok::LParen);
            }
            Cmd::Not => {
                if !programmer {
                    return;
                }
                if !self.wrap_operand(Tok::Not) {
                    if self.just_evaluated || self.error.is_some() {
                        self.fresh();
                    }
                    self.tokens.push(Tok::Not);
                }
            }
            Cmd::Post(p) => {
                if programmer && p == Post::Percent {
                    return;
                }
                if self.just_evaluated {
                    if self.result.is_none() {
                        return;
                    }
                    self.continue_with_result();
                }
                self.finish_number();
                if self.last_ends_operand() {
                    self.tokens.push(Tok::Post(p));
                }
            }
            Cmd::Negate => self.negate(),
            Cmd::Exp => {
                if programmer || !self.editing {
                    return;
                }
                if let Some(Tok::Num(text)) = self.tokens.last_mut() {
                    if !text.contains('E') {
                        if text.ends_with('.') {
                            text.pop();
                        }
                        text.push('E');
                    }
                }
            }
            Cmd::Const(c) => {
                if programmer {
                    return;
                }
                self.begin_operand();
                self.tokens.push(Tok::Const(c));
            }
            Cmd::Var => {
                if programmer {
                    return;
                }
                self.begin_operand();
                self.tokens.push(Tok::Var);
            }
            Cmd::LParen => {
                self.begin_operand();
                self.tokens.push(Tok::LParen);
            }
            Cmd::RParen => {
                if self.just_evaluated || self.open_parens() == 0 {
                    return;
                }
                self.finish_number();
                if self.last_ends_operand() {
                    self.tokens.push(Tok::RParen);
                }
            }
            Cmd::Equals => self.equals(now),
            Cmd::ClearEntry => {
                if self.just_evaluated || self.error.is_some() {
                    self.clear();
                } else if self.editing {
                    self.tokens.pop();
                    self.editing = false;
                }
            }
            Cmd::Clear => self.clear(),
            Cmd::Backspace => self.backspace(),
            Cmd::MemStore => {
                if let Some(v) = self.current_value() {
                    self.memory.store(&self.memory_text(&v));
                }
            }
            Cmd::MemClear => self.memory.clear(),
            Cmd::MemRecall => {
                let Some(text) = self.memory.recall().map(str::to_string) else {
                    return;
                };
                if let Some(lit) = self.memory_literal(&text) {
                    self.begin_operand();
                    self.finish_number();
                    self.tokens.push(Tok::Num(lit));
                }
            }
            Cmd::MemAdd | Cmd::MemSub => {
                let Some(v) = self.current_value() else {
                    return;
                };
                let operand = self.memory_text(&v);
                let subtract = cmd == Cmd::MemSub;
                self.memory.update(|top| {
                    let a = Num::parse(top).ok()?;
                    let b = Num::parse(&operand).ok()?;
                    let r = if subtract { a.sub(&b) } else { a.add(&b) };
                    r.ok().map(|n| n.to_literal())
                });
            }
        }
    }

    // ==== Typing ====

    /// One character the keyboard TYPED - the character the user's layout
    /// produced, not the key's position: digits, `. ,` (both the decimal
    /// point), `+ - * / ^ % ! ( ) =`, the typographic `× ÷ − · √ π ² ³`,
    /// names letter by letter (`sqrt`, `sin`, `pi`, `x`, `mod`; `and`, `xor`,
    /// `shl` in Programmer mode, where a-f are digits the base takes and
    /// `0x` / `0b` / `0o` after a lone 0 switch the base), `E` after a number
    /// for its exponent, and `y =` to start a function for the graph.
    /// Returns whether the character meant something.
    pub fn type_char(&mut self, c: char, now: u64) -> bool {
        let skip_paren = std::mem::take(&mut self.skip_paren);
        self.hint = None;
        if c.is_control() {
            return false;
        }
        if c.is_whitespace() {
            // A space ends a name (`sin 30`) and is nothing else.
            if self.letters.is_empty() {
                return false;
            }
            self.flush_word(now);
            return true;
        }
        if c == '(' && skip_paren && self.letters.is_empty() {
            // The name typed before opened this parenthesis already.
            return true;
        }
        let programmer = self.mode.is_programmer();
        if programmer && self.letters.is_empty() {
            if let Some(base) = base_prefix(c, self.base) {
                let lone_zero =
                    matches!(self.tokens.last(), Some(Tok::Num(t)) if t == "0");
                if self.editing && !self.just_evaluated && lone_zero {
                    self.tokens.pop();
                    self.editing = false;
                    self.set_base(base);
                    self.tokens.push(Tok::Num("0".to_string()));
                    self.editing = true;
                    return true;
                }
            }
            if let Some(d) = c.to_digit(16) {
                if c.is_ascii_digit() || self.base.accepts(d as u8) {
                    self.apply(Cmd::Digit(d as u8), now);
                    return true;
                }
            }
        }
        if c.is_alphabetic() && c != '\u{3c0}' {
            if !programmer && c == 'E' && self.letters.is_empty() && self.editing && !self.just_evaluated {
                self.apply(Cmd::Exp, now);
                return true;
            }
            let lower = c.to_lowercase().next().unwrap_or(c);
            self.letters.push(lower);
            self.settle_word(now, false);
            return true;
        }
        if c.is_ascii_digit() && !self.letters.is_empty() {
            let longer = format!("{}{c}", self.letters);
            if typing::is_prefix(self.names(), &longer) {
                // `log2`, `log10`, `pow10`.
                self.letters = longer;
                self.settle_word(now, false);
                return true;
            }
        }
        if c == '=' && self.letters == "y" {
            self.letters.clear();
            if self.just_evaluated || self.error.is_some() {
                self.fresh();
            }
            if self.tokens.is_empty() && !programmer {
                self.defining = true;
            } else {
                self.hint = Some("y = starts a function: type it on an empty entry".to_string());
            }
            return true;
        }
        // A typed root sign opens a call, as typing `sqrt` does.
        let call = match c {
            '\u{221a}' => Some(Func::Sqrt),
            '\u{221b}' => Some(Func::Cbrt),
            _ => None,
        };
        if let Some(f) = call.filter(|_| !programmer) {
            self.flush_word(now);
            self.apply_word(Word::Call(f), now);
            return true;
        }
        // Anything else ends a name: the name typed so far goes in first.
        let had_letters = !self.letters.is_empty();
        if had_letters {
            self.flush_word(now);
            if c == '(' && self.skip_paren {
                self.skip_paren = false;
                return true;
            }
        }
        match char_command(c, self.mode) {
            Some(cmd) => {
                self.apply(cmd, now);
                true
            }
            None => had_letters,
        }
    }

    /// [`Self::type_char`] for every character of `text`; how many meant something.
    pub fn type_text(&mut self, text: &str, now: u64) -> usize {
        text.chars().filter(|c| self.type_char(*c, now)).count()
    }

    /// The letters typed so far, read as names: a complete name that no
    /// longer name extends goes in now; letters no name starts with are cut
    /// at the longest name they begin with (`sinx` = `sin` + `x`), and a
    /// letter that starts no name at all means nothing (the hint says so).
    /// `flush`: the word is over (another character came) - everything goes.
    fn settle_word(&mut self, now: u64, flush: bool) {
        let names = self.names();
        let mut dropped = String::new();
        while !self.letters.is_empty() {
            if !flush && typing::is_prefix(names, &self.letters) {
                if let Some(w) = typing::exact(names, &self.letters) {
                    if w != Word::Define && !typing::extends(names, &self.letters) {
                        self.letters.clear();
                        self.apply_word(w, now);
                    }
                }
                break;
            }
            match typing::longest_prefix(names, &self.letters) {
                Some((len, w)) => {
                    let rest = self.letters[len..].to_string();
                    self.letters.clear();
                    if w == Word::Define {
                        dropped.push('y');
                    } else {
                        self.apply_word(w, now);
                    }
                    self.letters = rest;
                }
                None => {
                    let first = self.letters.chars().next().unwrap_or(' ');
                    dropped.push(first);
                    self.letters = self.letters[first.len_utf8()..].to_string();
                }
            }
        }
        if !dropped.is_empty() {
            self.hint = Some(if self.mode.is_programmer() {
                format!(
                    "\u{201c}{dropped}\u{201d} is not a {} digit or an operator",
                    self.base.label()
                )
            } else {
                format!("\u{201c}{dropped}\u{201d} is not a name the calculator knows")
            });
        }
    }

    /// The letters typed so far are over: every name in them goes in.
    fn flush_word(&mut self, now: u64) {
        if !self.letters.is_empty() {
            self.settle_word(now, true);
        }
    }

    fn apply_word(&mut self, w: Word, now: u64) {
        match w {
            Word::Call(f) => {
                self.apply(Cmd::Call(f), now);
                self.skip_paren = !self.mode.is_programmer();
            }
            Word::Const(k) => self.apply(Cmd::Const(k), now),
            Word::Var => self.apply(Cmd::Var, now),
            Word::Op(op) => self.apply(Cmd::Op(op), now),
            Word::Not => self.apply(Cmd::Not, now),
            Word::Define => {}
        }
    }

    /// The memory keeps decimal text, whatever the mode.
    fn memory_text(&self, v: &Value) -> String {
        match v {
            Value::Dec(n) => n.to_literal(),
            Value::Int(i) => i.to_string(),
        }
    }

    /// A memory value as a literal of this mode (an integer part in Programmer mode).
    fn memory_literal(&self, text: &str) -> Option<String> {
        let n = Num::parse(text).ok()?;
        if self.mode.is_programmer() {
            let i = n.to_i64_exact().map(i128::from).or_else(|| {
                // Not an integer: its integer part.
                let whole = text.split('.').next()?;
                whole.parse::<i128>().ok()
            })?;
            Some(programmer::format_int(self.word.wrap(i), self.base, self.word, false))
        } else {
            Some(n.to_literal())
        }
    }

    fn digit(&mut self, d: u8) {
        let programmer = self.mode.is_programmer();
        if (programmer && !self.base.accepts(d)) || (!programmer && d > 9) {
            return;
        }
        let c = char::from_digit(u32::from(d), 16).unwrap_or('0').to_ascii_uppercase();
        if self.editing && !self.just_evaluated && self.error.is_none() {
            if let Some(Tok::Num(text)) = self.tokens.last_mut() {
                if programmer {
                    let candidate = if text == "0" { c.to_string() } else { format!("{text}{c}") };
                    if programmer::fits(&candidate, self.base, self.word) {
                        *text = candidate;
                    }
                    return;
                }
                if let Some(p) = text.find('E') {
                    let exponent_digits = text[p + 1..].chars().filter(char::is_ascii_digit).count();
                    if exponent_digits < 4 {
                        text.push(c);
                    }
                    return;
                }
                if text == "0" || text == "-0" {
                    text.pop();
                    text.push(c);
                    return;
                }
                let significant = text.chars().filter(char::is_ascii_digit).count();
                if significant < MAX_TYPED_DIGITS {
                    text.push(c);
                }
                return;
            }
        }
        self.begin_operand();
        let start = if matches!(self.tokens.last(), Some(Tok::Neg)) && !programmer {
            // A sign typed before the number joins it: `−` `5` is -5.
            self.tokens.pop();
            format!("-{c}")
        } else {
            c.to_string()
        };
        self.tokens.push(Tok::Num(start));
        self.editing = true;
    }

    fn operator(&mut self, op: BinOp) {
        let programmer = self.mode.is_programmer();
        if op.is_bitwise() && !programmer {
            return;
        }
        if self.error.is_some() {
            self.fresh();
            return;
        }
        if self.just_evaluated {
            if self.result.is_none() {
                return;
            }
            self.continue_with_result();
        }
        self.finish_number();
        match self.tokens.last() {
            None => {
                self.tokens.push(Tok::Num("0".to_string()));
                self.tokens.push(Tok::Op(op));
            }
            Some(Tok::Op(prev)) => {
                if op == BinOp::Sub && matches!(prev, BinOp::Mul | BinOp::Div | BinOp::Pow | BinOp::Mod) {
                    self.tokens.push(Tok::Neg); // `2 × −3`
                } else {
                    let len = self.tokens.len();
                    self.tokens[len - 1] = Tok::Op(op);
                }
            }
            Some(Tok::LParen | Tok::Neg | Tok::Not | Tok::Func(_)) => {
                if op == BinOp::Sub && !matches!(self.tokens.last(), Some(Tok::Neg)) {
                    self.tokens.push(Tok::Neg);
                }
            }
            Some(_) => self.tokens.push(Tok::Op(op)),
        }
    }

    fn negate(&mut self) {
        let programmer = self.mode.is_programmer();
        if self.error.is_some() {
            return;
        }
        if self.just_evaluated {
            let Some(v) = self.result.clone() else {
                return;
            };
            let negated = match v {
                Value::Dec(n) => Value::Dec(n.neg()),
                Value::Int(i) => Value::Int(self.word.wrap(i.wrapping_neg())),
            };
            let lit = self.literal(&negated);
            self.fresh();
            self.tokens.push(Tok::Num(lit));
            return;
        }
        if let Some(Tok::Num(text)) = self.tokens.last_mut() {
            if programmer {
                if let Ok(v) = programmer::parse_int(text, self.base, self.word) {
                    *text = programmer::format_int(self.word.wrap(v.wrapping_neg()), self.base, self.word, false);
                }
                return;
            }
            if let Some(p) = text.find('E') {
                // In the exponent: its sign.
                let (mantissa, exponent) = text.split_at(p + 1);
                let flipped = match exponent.strip_prefix('-') {
                    Some(rest) => format!("{mantissa}{rest}"),
                    None => format!("{mantissa}-{exponent}"),
                };
                *text = flipped;
                return;
            }
            if text == "0" || text == "0." {
                return;
            }
            if let Some(rest) = text.strip_prefix('-') {
                *text = rest.to_string();
            } else {
                text.insert(0, '-');
            }
            return;
        }
        if self.last_ends_operand() {
            if let Some(start) = self.operand_start() {
                self.tokens.insert(start, Tok::Neg);
            }
            return;
        }
        if !matches!(self.tokens.last(), Some(Tok::Neg)) {
            self.tokens.push(Tok::Neg);
        } else {
            self.tokens.pop();
        }
    }

    fn equals(&mut self, now: u64) {
        if self.error.is_some() {
            self.fresh();
            return;
        }
        if self.entry_is_function() {
            self.plot_entry();
            return;
        }
        if self.just_evaluated {
            // `=` again repeats the last operation on the result.
            let (Some(result), Some((op, operand))) = (self.result.clone(), self.last_op.clone()) else {
                return;
            };
            let lit = self.literal(&result);
            self.fresh();
            self.tokens = vec![Tok::Num(lit), Tok::Op(op), Tok::Num(operand)];
        }
        self.finish_number();
        if self.tokens.is_empty() {
            return;
        }
        // `5 × =` is `5 × 5`: the value before the operator is the operand.
        if let Some(Tok::Op(_)) = self.tokens.last() {
            let before = self.eval_tokens(&self.tokens[..self.tokens.len() - 1]);
            match before {
                Ok(v) => {
                    let lit = self.literal(&v);
                    self.tokens.push(Tok::Num(lit));
                }
                Err(_) => {
                    self.tokens.pop();
                }
            }
        }
        while matches!(self.tokens.last(), Some(Tok::Neg | Tok::Not | Tok::Func(_) | Tok::LParen)) {
            self.tokens.pop();
        }
        if self.tokens.is_empty() {
            return;
        }
        for _ in 0..self.open_parens() {
            self.tokens.push(Tok::RParen);
        }
        let shown = expr::display(&self.tokens, self.grouping, self.mode.is_programmer());
        self.last_op = match self.tokens.as_slice() {
            [.., Tok::Op(op), Tok::Num(n)] if self.tokens.len() >= 3 => Some((*op, n.clone())),
            _ => None,
        };
        let value = self.eval_tokens(&self.tokens);
        self.shown_expr = format!("{shown} =");
        self.just_evaluated = true;
        self.editing = false;
        self.shown_tokens = std::mem::take(&mut self.tokens);
        match value {
            Ok(v) => {
                self.history.push(HistoryEntry {
                    expr: shown,
                    result: self.show(&v),
                    mode: self.mode.key().to_string(),
                    at: now,
                });
                if self.history.len() > MAX_HISTORY {
                    self.history.remove(0);
                }
                self.history_dirty = true;
                self.result = Some(v);
                self.error = None;
            }
            Err(e) => {
                self.result = None;
                self.last_op = None;
                self.error = Some(e.to_string());
            }
        }
    }

    /// `=` on a function of `x`: it goes to the graph (`y₁`, `y₂`, ...).
    fn plot_entry(&mut self) {
        self.finish_number();
        while matches!(self.tokens.last(), Some(Tok::Op(_) | Tok::Neg | Tok::Not | Tok::Func(_) | Tok::LParen)) {
            self.tokens.pop();
        }
        if self.tokens.is_empty() {
            // `y =` and nothing yet: wait for the function.
            return;
        }
        for _ in 0..self.open_parens() {
            self.tokens.push(Tok::RParen);
        }
        let text = expr::display(&self.tokens, self.grouping, false);
        match expr::parse(&self.tokens) {
            Ok(_) => {
                if self.plots.len() >= MAX_PLOTS {
                    self.plots.remove(0);
                }
                self.plots.push(Plot {
                    tokens: self.tokens.clone(),
                    text: text.clone(),
                });
                self.plotted = Some(self.plots.len() - 1);
                self.error = None;
            }
            Err(e) => {
                self.plotted = None;
                self.error = Some(e.to_string());
            }
        }
        self.shown_expr = format!("y = {text}");
        self.shown_tokens = std::mem::take(&mut self.tokens);
        self.just_evaluated = true;
        self.editing = false;
        self.defining = false;
        self.result = None;
        self.last_op = None;
    }

    /// Removes the graph's function `i`.
    pub fn remove_plot(&mut self, i: usize) {
        if i < self.plots.len() {
            self.plots.remove(i);
            self.plotted = None;
        }
    }

    /// Removes every function from the graph.
    pub fn clear_plots(&mut self) {
        self.plots.clear();
        self.plotted = None;
    }

    /// C: everything but the memory, the history and the graph's functions.
    pub fn clear(&mut self) {
        self.fresh();
        self.result = None;
        self.last_op = None;
    }

    fn backspace(&mut self) {
        if self.error.is_some() {
            self.clear();
            return;
        }
        if self.just_evaluated {
            self.shown_expr.clear();
            self.shown_tokens.clear();
            return;
        }
        if self.editing {
            if let Some(Tok::Num(text)) = self.tokens.last_mut() {
                text.pop();
                if text.is_empty() || text == "-" {
                    self.tokens.pop();
                    self.editing = false;
                }
            }
            return;
        }
        if self.tokens.is_empty() && self.defining {
            self.defining = false;
            return;
        }
        if let Some(t) = self.tokens.pop() {
            if t == Tok::LParen && matches!(self.tokens.last(), Some(Tok::Func(_))) {
                self.tokens.pop();
            }
        }
        self.editing = matches!(self.tokens.last(), Some(Tok::Num(_)));
    }

    // ==== Modes, bases, history ====

    /// Switches the keypad mode. Between the decimals and Programmer's
    /// integers the entry is carried over - its numbers rewritten (`12 × 3`
    /// stays `12 × 3`; a result becomes its integer part) - and starts over
    /// with its value only where the other mode lacks something in it (a
    /// fraction, `sin`, `x`, a bitwise operator). The memory, the history
    /// and the graph's functions stay.
    pub fn set_mode(&mut self, mode: CalcMode) {
        if mode.is_programmer() != self.mode.is_programmer() {
            self.convert_domain(mode.is_programmer());
        }
        self.mode = mode;
        self.second = false;
    }

    /// The entry in the other domain (the mode still says the old one).
    fn convert_domain(&mut self, to_programmer: bool) {
        self.letters.clear();
        self.skip_paren = false;
        self.defining = false;
        self.last_op = None;
        if self.just_evaluated {
            match self.result.take().and_then(|v| self.convert_value(v, to_programmer)) {
                Some(v) => {
                    self.result = Some(v);
                    self.shown_tokens = self
                        .convert_tokens(&self.shown_tokens, to_programmer)
                        .unwrap_or_default();
                }
                None => self.clear(),
            }
            return;
        }
        if let Some(tokens) = self.convert_tokens(&self.tokens, to_programmer) {
            self.tokens = tokens;
            return;
        }
        // Something the other mode has no word for: the entry's value goes
        // on, as a result does (a digit starts over, an operator continues).
        let value = self
            .entry_value()
            .and_then(|v| self.convert_value(v, to_programmer));
        self.clear();
        if let Some(v) = value {
            self.result = Some(v);
            self.just_evaluated = true;
        }
    }

    /// The value of the whole entry (an unfinished tail left off, open
    /// parentheses closed), or the result after `=`.
    fn entry_value(&self) -> Option<Value> {
        if self.just_evaluated {
            return self.result.clone();
        }
        let mut end = self.tokens.len();
        while end > 0
            && matches!(
                self.tokens[end - 1],
                Tok::Op(_) | Tok::Neg | Tok::Not | Tok::Func(_) | Tok::LParen
            )
        {
            end -= 1;
        }
        if end == 0 {
            return None;
        }
        self.eval_tokens(&self.tokens[..end]).ok()
    }

    fn convert_value(&self, v: Value, to_programmer: bool) -> Option<Value> {
        match (v, to_programmer) {
            (Value::Dec(n), true) => dec_to_int(&n, self.word).map(Value::Int),
            (Value::Int(i), false) => Num::parse(&i.to_string()).ok().map(Value::Dec),
            (v, _) => Some(v),
        }
    }

    /// `tokens` in the other domain, or `None` if one of them has no
    /// counterpart there.
    fn convert_tokens(&self, tokens: &[Tok], to_programmer: bool) -> Option<Vec<Tok>> {
        let mut out = Vec::with_capacity(tokens.len());
        for t in tokens {
            out.push(match t {
                Tok::Num(text) if to_programmer => {
                    let v = dec_literal_to_int(text, self.word)?;
                    Tok::Num(programmer::format_int(v, self.base, self.word, false))
                }
                Tok::Num(text) => {
                    let v = programmer::parse_int(text, self.base, self.word).ok()?;
                    Tok::Num(v.to_string())
                }
                Tok::Op(op) if op.is_bitwise() && !to_programmer => return None,
                Tok::Not if !to_programmer => return None,
                Tok::Func(_) | Tok::Const(_) | Tok::Var | Tok::Post(Post::Percent) if to_programmer => {
                    return None
                }
                other => other.clone(),
            });
        }
        Some(out)
    }

    /// Programmer mode's input base: the numbers typed so far are rewritten in it.
    pub fn set_base(&mut self, base: Base) {
        let (old, word) = (self.base, self.word);
        for t in &mut self.tokens {
            if let Tok::Num(text) = t {
                if let Ok(v) = programmer::parse_int(text, old, word) {
                    *text = programmer::format_int(v, base, word, false);
                }
            }
        }
        self.base = base;
    }

    /// Programmer mode's word size: values are cut to it.
    pub fn set_word(&mut self, word: WordSize) {
        let base = self.base;
        let old = self.word;
        for t in &mut self.tokens {
            if let Tok::Num(text) = t {
                if let Ok(v) = programmer::parse_int(text, base, old) {
                    *text = programmer::format_int(word.wrap(v), base, word, false);
                }
            }
        }
        if let Some(Value::Int(i)) = self.result {
            self.result = Some(Value::Int(word.wrap(i)));
        }
        self.word = word;
    }

    /// Programmer mode's bit field: flips bit `i` of the current value.
    pub fn toggle_bit(&mut self, i: u32) {
        if !self.mode.is_programmer() {
            return;
        }
        let v = match self.current_value() {
            Some(Value::Int(v)) => v,
            _ => 0,
        };
        let flipped = programmer::toggle_bit(v, i, self.word);
        let lit = programmer::format_int(flipped, self.base, self.word, false);
        if self.just_evaluated || self.error.is_some() {
            self.fresh();
        }
        if let Some(Tok::Num(text)) = self.tokens.last_mut() {
            *text = lit;
        } else {
            if self.last_ends_operand() {
                // The value of a whole expression: it becomes the entry.
                self.tokens.clear();
            }
            self.tokens.push(Tok::Num(lit));
        }
        self.editing = true;
    }

    /// Puts a history entry back: its expression on the expression line, its
    /// result as the value to go on with.
    pub fn use_history(&mut self, index: usize) {
        let Some(entry) = self.history.get(index).cloned() else {
            return;
        };
        let value = if entry.mode == CalcMode::Programmer.key() {
            if !self.mode.is_programmer() {
                return;
            }
            let text: String = entry.result.chars().filter(|c| *c != ' ' && *c != ',').collect();
            programmer::parse_int(&text, self.base, self.word).ok().map(Value::Int)
        } else {
            if self.mode.is_programmer() {
                return;
            }
            Num::parse(&entry.result).ok().map(Value::Dec)
        };
        let Some(value) = value else {
            return;
        };
        self.fresh();
        self.shown_expr = format!("{} =", entry.expr);
        self.shown_tokens = expr::tokenize(&entry.expr, self.domain()).unwrap_or_default();
        self.result = Some(value);
        self.just_evaluated = true;
        self.last_op = None;
    }

    /// Clear history.
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.history_dirty = true;
    }

    /// Pasted text: tokens of the current mode, replacing the entry after
    /// `=`, appended otherwise; `y = ...` starts a function for the graph.
    /// `Err` says why it could not be read.
    pub fn paste(&mut self, text: &str) -> Result<(), String> {
        let mut body = text.trim();
        let mut define = false;
        if !self.mode.is_programmer() {
            for prefix in ["y=", "y =", "Y=", "Y =", "f(x)=", "f(x) ="] {
                if let Some(rest) = body.strip_prefix(prefix) {
                    body = rest.trim_start();
                    define = true;
                    break;
                }
            }
        }
        let tokens = expr::tokenize(body, self.domain()).map_err(|e| e.to_string())?;
        if tokens.is_empty() && !define {
            return Ok(());
        }
        self.flush_word(0);
        if self.just_evaluated || self.error.is_some() || define {
            self.fresh();
        }
        self.defining |= define;
        self.finish_number();
        if self.last_ends_operand() && tokens.first().is_some_and(Tok::starts_operand) {
            self.tokens.push(Tok::Op(BinOp::Mul));
        }
        self.tokens.extend(tokens);
        self.editing = matches!(self.tokens.last(), Some(Tok::Num(_)));
        Ok(())
    }
}

/// What a typed character that is not a letter does in a mode: digits,
/// operators, `.` and `,` (both the decimal point), `^` (power; XOR in
/// Programmer mode), `!`, `%`, the typographic `× ÷ − · π ² ³`, Windows'
/// `@` (square root of the operand), and Programmer mode's a-f, `&` `|`
/// `~` `<` `>` and `%` (mod). Letters are names ([`Calculator::type_char`]).
#[must_use]
pub fn char_command(c: char, mode: CalcMode) -> Option<Cmd> {
    if let Some(d) = c.to_digit(10) {
        return Some(Cmd::Digit(d as u8));
    }
    if mode.is_programmer() {
        if let Some(d) = c.to_digit(16) {
            return Some(Cmd::Digit(d as u8));
        }
        return match c {
            '+' => Some(Cmd::Op(BinOp::Add)),
            '-' | '\u{2212}' => Some(Cmd::Op(BinOp::Sub)),
            '*' | '\u{d7}' | '\u{b7}' => Some(Cmd::Op(BinOp::Mul)),
            '/' | '\u{f7}' | ':' => Some(Cmd::Op(BinOp::Div)),
            '%' => Some(Cmd::Op(BinOp::Mod)),
            '&' => Some(Cmd::Op(BinOp::And)),
            '|' => Some(Cmd::Op(BinOp::Or)),
            '^' => Some(Cmd::Op(BinOp::Xor)),
            '~' | '\u{ac}' => Some(Cmd::Not),
            '<' => Some(Cmd::Op(BinOp::Shl)),
            '>' => Some(Cmd::Op(BinOp::Shr)),
            '!' => Some(Cmd::Post(Post::Factorial)),
            '\u{b2}' => Some(Cmd::Post(Post::Square)),
            '\u{b3}' => Some(Cmd::Post(Post::Cube)),
            '(' | '[' => Some(Cmd::LParen),
            ')' | ']' => Some(Cmd::RParen),
            '=' => Some(Cmd::Equals),
            _ => None,
        };
    }
    match c {
        '.' | ',' | '\u{66b}' => Some(Cmd::Point),
        '+' => Some(Cmd::Op(BinOp::Add)),
        '-' | '\u{2212}' => Some(Cmd::Op(BinOp::Sub)),
        '*' | '\u{d7}' | '\u{b7}' | '\u{22c5}' => Some(Cmd::Op(BinOp::Mul)),
        '/' | '\u{f7}' | ':' | '\u{2215}' => Some(Cmd::Op(BinOp::Div)),
        '^' => Some(Cmd::Op(BinOp::Pow)),
        '%' => Some(Cmd::Post(Post::Percent)),
        '!' => Some(Cmd::Post(Post::Factorial)),
        '\u{b2}' => Some(Cmd::Post(Post::Square)),
        '\u{b3}' => Some(Cmd::Post(Post::Cube)),
        '\u{3c0}' => Some(Cmd::Const(Const::Pi)),
        '=' => Some(Cmd::Equals),
        '@' => Some(Cmd::Func(Func::Sqrt)),
        '(' | '[' => Some(Cmd::LParen),
        ')' | ']' => Some(Cmd::RParen),
        _ => None,
    }
}

/// The keys with names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedKey {
    Enter,
    Backspace,
    Escape,
    Delete,
    /// F9: +/- (Windows).
    F9,
}

/// What a named key does.
#[must_use]
pub fn named_command(key: NamedKey) -> Cmd {
    match key {
        NamedKey::Enter => Cmd::Equals,
        NamedKey::Backspace => Cmd::Backspace,
        NamedKey::Escape => Cmd::Clear,
        NamedKey::Delete => Cmd::ClearEntry,
        NamedKey::F9 => Cmd::Negate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `keys` into a calculator in `mode` as a keyboard does
    /// ([`Calculator::type_char`]), except four test letters: `C` clears,
    /// `B` is Backspace, `N` +/-, `R` CE.
    fn typed(mode: CalcMode, keys: &str) -> Calculator {
        let mut c = Calculator::new();
        c.set_mode(mode);
        for ch in keys.chars() {
            let cmd = match ch {
                ' ' => continue,
                'C' => Cmd::Clear,
                'B' => Cmd::Backspace,
                'N' => Cmd::Negate,
                'R' => Cmd::ClearEntry,
                other => {
                    assert!(c.type_char(other, 1), "{other:?} meant nothing");
                    continue;
                }
            };
            c.apply(cmd, 1);
        }
        c
    }

    fn std(keys: &str) -> Calculator {
        typed(CalcMode::Standard, keys)
    }

    fn sci(keys: &str) -> Calculator {
        typed(CalcMode::Scientific, keys)
    }

    fn lines(c: &Calculator) -> (String, String) {
        (c.expression_line(), c.result_line())
    }

    #[test]
    fn the_standard_sample_1280_times_0_19() {
        let c = std("1280*0.19=");
        assert_eq!(lines(&c), ("1,280 \u{d7} 0.19 =".to_string(), "243.2".to_string()));
        assert_eq!(c.history.len(), 1);
        assert_eq!(c.history[0].expr, "1,280 \u{d7} 0.19");
        assert_eq!(c.history[0].result, "243.2");
        assert!(c.history_dirty);
    }

    #[test]
    fn while_typing_the_expression_line_holds_what_is_committed() {
        let c = std("1280*0.1");
        assert_eq!(lines(&c), ("1,280 \u{d7}".to_string(), "0.1".to_string()));
        let c = std("1280*");
        assert_eq!(lines(&c), ("1,280 \u{d7}".to_string(), "1,280".to_string()));
        let c = std("2+3*");
        assert_eq!(c.result_line(), "5", "the value so far, unfinished tail left off");
        let c = std("0.10");
        assert_eq!(c.result_line(), "0.10", "a typed trailing zero stays");
    }

    #[test]
    fn no_binary_artefacts_and_precedence() {
        assert_eq!(std("0.1+0.2=").result_line(), "0.3");
        assert_eq!(std("2+3*4=").result_line(), "14");
        assert_eq!(std("(2+3)*4=").result_line(), "20");
    }

    #[test]
    fn a_digit_after_equals_starts_over_and_an_operator_continues() {
        let c = std("2+3=4");
        assert_eq!(lines(&c), (String::new(), "4".to_string()));
        let c = std("2+3=*2=");
        assert_eq!(lines(&c), ("5 \u{d7} 2 =".to_string(), "10".to_string()));
    }

    #[test]
    fn equals_again_repeats_the_last_operation() {
        let c = std("10+2===");
        assert_eq!(c.result_line(), "16");
        assert_eq!(c.expression_line(), "14 + 2 =");
        assert_eq!(c.history.len(), 3);
    }

    #[test]
    fn a_second_operator_replaces_the_first() {
        assert_eq!(std("5+*2=").result_line(), "10");
        assert_eq!(std("5*-2=").result_line(), "-10", "minus after times is a sign");
    }

    #[test]
    fn times_equals_squares_the_left_side() {
        assert_eq!(std("5*=").result_line(), "25");
        assert_eq!(std("5*=").expression_line(), "5 \u{d7} 5 =");
    }

    #[test]
    fn the_percent_keys() {
        assert_eq!(std("50+10%=").result_line(), "55");
        assert_eq!(std("200*10%=").result_line(), "20");
        assert_eq!(std("50+10%").result_line(), "55", "live");
    }

    #[test]
    fn plus_minus_changes_the_typed_numbers_sign() {
        assert_eq!(std("5N").result_line(), "-5");
        assert_eq!(std("5NN").result_line(), "5");
        assert_eq!(std("3*5N=").result_line(), "-15");
        assert_eq!(std("0N").result_line(), "0", "zero has no sign");
        let c = std("2+3=N");
        assert_eq!(c.result_line(), "-5");
        assert_eq!(std("2+3=N+1=").result_line(), "-4");
    }

    #[test]
    fn backspace_ce_and_c() {
        assert_eq!(std("123B").result_line(), "12");
        // After a result, a new number typed is a number being typed: Backspace
        // deletes its digit (AzCalculator E2E 2026-10-02: 7*6= 123 B kept 123).
        assert_eq!(std("7*6=123B").result_line(), "12");
        assert_eq!(std("1280*0.19=C0.1+0.2=7*6=123B").result_line(), "12");
        assert_eq!(std("1BB").result_line(), "0");
        assert_eq!(std("12+B").expression_line(), "", "the operator goes, the 12 is typed on");
        assert_eq!(std("12+B3").result_line(), "123");
        assert_eq!(std("12+34R").result_line(), "12", "CE drops the entry");
        assert_eq!(std("12+34R5=").result_line(), "17");
        let c = std("12+34C");
        assert_eq!(lines(&c), (String::new(), "0".to_string()));
    }

    #[test]
    fn the_point_and_leading_zeros() {
        assert_eq!(std(".5").result_line(), "0.5");
        assert_eq!(std("0.5.5").result_line(), "0.55", "a second point is ignored");
        assert_eq!(std("007").result_line(), "7");
        assert_eq!(std("5.+1=").result_line(), "6");
        assert_eq!(std("5.+").expression_line(), "5 +");
        assert_eq!(std("2,5*2=").result_line(), "5", "a typed comma is the decimal point");
    }

    #[test]
    fn typing_stops_at_thirty_two_digits() {
        let c = std(&"1".repeat(40));
        assert_eq!(c.result_line().chars().filter(char::is_ascii_digit).count(), 32);
    }

    #[test]
    fn dividing_by_zero_shows_an_error_until_the_next_key() {
        let c = std("5/0=");
        assert_eq!(c.result_line(), "Cannot divide by zero");
        assert_eq!(c.expression_line(), "5 \u{f7} 0 =");
        assert!(c.history.is_empty(), "an error is not history");
        assert_eq!(std("5/0=7").result_line(), "7");
        assert_eq!(std("5/0=C").result_line(), "0");
    }

    #[test]
    fn function_keys_wrap_the_operand_or_open_a_call() {
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Scientific);
        c.type_char('2', 1);
        c.apply(Cmd::Func(Func::Sqrt), 1);
        assert_eq!(c.expression_line(), "\u{221a}(2)");
        assert_eq!(c.result_line(), "1.4142135623730950488016887242097");
        c.apply(Cmd::Op(BinOp::Add), 1);
        c.apply(Cmd::Func(Func::Sin), 1);
        assert_eq!(c.expression_line(), "\u{221a}(2) + sin(");
        c.type_text("30)=", 1);
        assert_eq!(c.expression_line(), "\u{221a}(2) + sin(30) =");
        assert!(c.result_line().starts_with("1.9142135623730"), "{}", c.result_line());
    }

    #[test]
    fn the_scientific_sample_by_keyboard() {
        let c = sci("sin30)+2^10=");
        assert_eq!(c.expression_line(), "sin(30) + 2^10 =");
        assert_eq!(c.result_line(), "1,024.5");
    }

    #[test]
    fn a_function_key_after_equals_wraps_the_result() {
        let mut c = std("9+16=");
        c.apply(Cmd::Func(Func::Sqrt), 1);
        assert_eq!(lines(&c), ("\u{221a}(25)".to_string(), "5".to_string()));
        let mut c = std("5");
        c.apply(Cmd::Post(Post::Square), 1);
        assert_eq!(lines(&c), ("5\u{b2}".to_string(), "25".to_string()));
        c.apply(Cmd::Func(Func::Recip), 1);
        assert_eq!(lines(&c), ("1/(5\u{b2})".to_string(), "0.04".to_string()));
    }

    #[test]
    fn parentheses_close_themselves_on_equals() {
        let c = sci("2*(3+4=");
        assert_eq!(c.expression_line(), "2 \u{d7} (3 + 4) =");
        assert_eq!(c.result_line(), "14");
        let c = sci("2*(3+4");
        assert_eq!(c.open_parens(), 1);
        let c = sci(")");
        assert!(c.tokens.is_empty(), "a ) with nothing open is ignored");
        let c = sci("2(3)=");
        assert_eq!(c.expression_line(), "2 \u{d7} (3) =");
        assert_eq!(c.result_line(), "6");
    }

    #[test]
    fn the_exponent_key_enters_scientific_notation() {
        let c = sci("1.5E3");
        assert_eq!(c.result_line(), "1.5e+3");
        let c = sci("1.5E3N");
        assert_eq!(c.result_line(), "1.5e-3", "+/- after Exp flips the exponent's sign");
        assert_eq!(sci("1.5E3+1=").result_line(), "1,501");
        assert_eq!(sci("2E+1=").result_line(), "3", "a dangling E goes");
    }

    #[test]
    fn constants_multiply_implicitly() {
        let c = sci("2pi=");
        assert_eq!(c.expression_line(), "2 \u{d7} \u{3c0} =");
        assert!(c.result_line().starts_with("6.283185307179586"));
    }

    #[test]
    fn memory_keys() {
        let mut c = std("5");
        c.apply(Cmd::MemStore, 1);
        c.apply(Cmd::Clear, 1);
        c.apply(Cmd::MemRecall, 1);
        assert_eq!(c.result_line(), "5");
        c.apply(Cmd::Op(BinOp::Add), 1);
        c.apply(Cmd::MemRecall, 1);
        c.apply(Cmd::Equals, 1);
        assert_eq!(c.result_line(), "10");
        c.apply(Cmd::MemAdd, 1);
        assert_eq!(c.memory.recall(), Some("15"));
        c.apply(Cmd::MemSub, 1);
        assert_eq!(c.memory.recall(), Some("5"));
        c.apply(Cmd::MemClear, 1);
        assert_eq!(c.memory.recall(), None);
        let mut c = std("7");
        c.apply(Cmd::MemAdd, 1);
        assert_eq!(c.memory.recall(), Some("7"), "M+ on an empty memory stores the value");
    }

    #[test]
    fn a_recalled_value_is_replaced_by_typing() {
        let mut c = std("5");
        c.apply(Cmd::MemStore, 1);
        c.apply(Cmd::Op(BinOp::Add), 1);
        c.apply(Cmd::MemRecall, 1);
        c.apply(Cmd::Digit(3), 1);
        c.apply(Cmd::Equals, 1);
        assert_eq!(c.result_line(), "8");
    }

    #[test]
    fn programmer_mode_types_in_the_chosen_base() {
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Programmer);
        c.set_base(Base::Hex);
        c.type_text("2a5f", 1);
        assert_eq!(c.result_line(), "2A5F");
        let lines = c.programmer_lines();
        assert_eq!(lines[0], (Base::Hex, "2A5F".to_string()));
        assert_eq!(lines[1], (Base::Dec, "10,847".to_string()));
        assert_eq!(lines[2], (Base::Oct, "25 137".to_string()));
        assert_eq!(lines[3], (Base::Bin, "10 1010 0101 1111".to_string()));
        c.set_base(Base::Dec);
        assert_eq!(c.result_line(), "10847", "the typed number is rewritten in the new base");
        c.apply(Cmd::Digit(10), 1);
        assert_eq!(c.result_line(), "10847", "A is not a decimal digit");
    }

    #[test]
    fn programmer_arithmetic_and_bitwise_keys() {
        let c = typed(CalcMode::Programmer, "17/5=");
        assert_eq!(c.result_line(), "3");
        let c = typed(CalcMode::Programmer, "6&3=");
        assert_eq!(c.result_line(), "2");
        let c = typed(CalcMode::Programmer, "1<4=");
        assert_eq!(c.result_line(), "16");
        let c = typed(CalcMode::Programmer, "6^3=");
        assert_eq!(c.result_line(), "5", "^ is XOR in Programmer mode");
        let mut c = typed(CalcMode::Programmer, "0");
        c.apply(Cmd::Not, 1);
        assert_eq!(c.expression_line(), "NOT (0)");
        assert_eq!(c.result_line(), "-1");
        c.apply(Cmd::Point, 1);
        assert_eq!(c.result_line(), "-1", "no point in Programmer mode");
    }

    #[test]
    fn the_word_size_caps_typing_and_wraps_results() {
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Programmer);
        c.set_word(WordSize::Byte);
        c.type_text("999", 1);
        assert_eq!(c.result_line(), "99", "999 does not fit a byte");
        let c2 = {
            let mut c = Calculator::new();
            c.set_mode(CalcMode::Programmer);
            c.set_word(WordSize::Byte);
            c.type_text("127+1=", 1);
            c
        };
        assert_eq!(c2.result_line(), "-128", "overflow wraps");
        c.set_word(WordSize::Word);
        assert_eq!(c.result_line(), "99");
    }

    #[test]
    fn the_bit_field_toggles_the_current_value() {
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Programmer);
        c.toggle_bit(0);
        c.toggle_bit(3);
        assert_eq!(c.result_line(), "9");
        c.toggle_bit(0);
        assert_eq!(c.result_line(), "8");
        c.type_text("+1=", 1);
        assert_eq!(c.result_line(), "9");
        c.toggle_bit(1);
        assert_eq!(c.result_line(), "11", "after = the result is toggled");
    }

    #[test]
    fn switching_between_decimals_and_programmer_keeps_the_entry() {
        let mut c = std("12+3=");
        c.set_mode(CalcMode::Scientific);
        assert_eq!(c.result_line(), "15", "Standard and Scientific share the entry");
        c.set_mode(CalcMode::Programmer);
        assert_eq!(c.result_line(), "15", "the result is an integer: it carries over");
        assert_eq!(c.history.len(), 1, "the history stays");
        // An unfinished entry keeps its tokens, in the base.
        let mut c = std("12*3");
        c.set_mode(CalcMode::Programmer);
        assert_eq!(lines(&c), ("12 \u{d7}".to_string(), "3".to_string()));
        c.set_base(Base::Hex);
        assert_eq!(lines(&c), ("C \u{d7}".to_string(), "3".to_string()));
        c.set_mode(CalcMode::Scientific);
        assert_eq!(lines(&c), ("12 \u{d7}".to_string(), "3".to_string()));
        c.type_char('=', 1);
        assert_eq!(c.result_line(), "36");
        // What Programmer mode has no word for goes on as its value.
        let mut c = std("2.5*3");
        c.set_mode(CalcMode::Programmer);
        assert_eq!(lines(&c), (String::new(), "7".to_string()), "7.5 has the integer part 7");
        let mut c = typed(CalcMode::Programmer, "6&3");
        c.set_mode(CalcMode::Standard);
        assert_eq!(c.result_line(), "2", "AND has no decimal twin: its value goes on");
    }

    #[test]
    fn a_history_entry_comes_back_as_the_value_to_go_on_with() {
        let mut c = std("1280*0.19=C");
        c.use_history(0);
        assert_eq!(lines(&c), ("1,280 \u{d7} 0.19 =".to_string(), "243.2".to_string()));
        c.apply(Cmd::Op(BinOp::Add), 1);
        c.type_text("18.5=", 1);
        assert_eq!(c.result_line(), "261.7");
        c.clear_history();
        assert!(c.history.is_empty());
    }

    #[test]
    fn copy_gives_the_plain_value_and_paste_reads_an_expression() {
        let c = std("1234*2=");
        assert_eq!(c.result_line(), "2,468");
        assert_eq!(c.copy_text(), "2468");
        let mut c = Calculator::new();
        c.paste("1,280 \u{d7} 0.19").unwrap();
        c.apply(Cmd::Equals, 1);
        assert_eq!(c.result_line(), "243.2");
        let mut c = std("2");
        c.paste("(3)").unwrap();
        c.apply(Cmd::Equals, 1);
        assert_eq!(c.result_line(), "6", "pasted after a number multiplies");
        assert!(Calculator::new().paste("hello").is_err());
    }

    #[test]
    fn the_keyboard_map_per_mode() {
        assert_eq!(char_command('7', CalcMode::Standard), Some(Cmd::Digit(7)));
        assert_eq!(char_command(',', CalcMode::Standard), Some(Cmd::Point));
        assert_eq!(char_command('@', CalcMode::Standard), Some(Cmd::Func(Func::Sqrt)));
        assert_eq!(char_command('*', CalcMode::Standard), Some(Cmd::Op(BinOp::Mul)));
        assert_eq!(char_command('^', CalcMode::Standard), Some(Cmd::Op(BinOp::Pow)), "in every decimal mode");
        assert_eq!(char_command('!', CalcMode::Standard), Some(Cmd::Post(Post::Factorial)));
        assert_eq!(char_command('s', CalcMode::Scientific), None, "letters are names: type_char");
        assert_eq!(char_command('f', CalcMode::Programmer), Some(Cmd::Digit(15)));
        assert_eq!(char_command('%', CalcMode::Programmer), Some(Cmd::Op(BinOp::Mod)));
        assert_eq!(char_command('%', CalcMode::Standard), Some(Cmd::Post(Post::Percent)));
        assert_eq!(named_command(NamedKey::Enter), Cmd::Equals);
        assert_eq!(named_command(NamedKey::Escape), Cmd::Clear);
        assert_eq!(named_command(NamedKey::Delete), Cmd::ClearEntry);
        assert_eq!(named_command(NamedKey::F9), Cmd::Negate);
        assert_eq!(CalcMode::by_key("programmer"), Some(CalcMode::Programmer));
    }

    // ==== Typing: the characters a keyboard produces ====

    #[test]
    fn typed_operators_from_any_layout_compute() {
        // `*` arrives as the CHARACTER, whatever key made it (Shift+8 on a US
        // keyboard, Shift++ on a German one, the keypad's `*`).
        assert_eq!(lines(&std("12*3=")), ("12 \u{d7} 3 =".to_string(), "36".to_string()));
        assert_eq!(std("2^10=").result_line(), "1,024");
        assert_eq!(std("(1+2)*3=").result_line(), "9");
        assert_eq!(std("5!=").result_line(), "120");
        assert_eq!(std("7\u{d7}6=").result_line(), "42", "the typographic times sign");
        assert_eq!(std("84\u{f7}2=").result_line(), "42");
        assert_eq!(std("50\u{2212}8=").result_line(), "42");
    }

    #[test]
    fn typed_names_open_their_calls() {
        let c = std("sqrt(16)=");
        assert_eq!(lines(&c), ("\u{221a}(16) =".to_string(), "4".to_string()));
        let c = std("sqrt16=");
        assert_eq!(c.result_line(), "4", "the name opened its parenthesis; = closes it");
        assert_eq!(std("abs(-7)=").result_line(), "7");
        assert_eq!(std("ln(e)=").result_line(), "1");
        assert_eq!(std("log(1000)=").result_line(), "3");
        assert_eq!(std("log2(8)=").result_line(), "3");
        assert_eq!(std("cos(60)=").result_line(), "0.5");
        assert_eq!(std("2sqrt(9)=").result_line(), "6", "a name after a number multiplies");
        assert_eq!(std("\u{221a}25=").result_line(), "5", "a typed root sign is sqrt");
    }

    #[test]
    fn a_name_shows_while_it_is_typed_and_backspace_takes_its_letters() {
        let mut c = Calculator::new();
        c.type_text("2+si", 1);
        assert_eq!(lines(&c), ("2 +".to_string(), "si".to_string()));
        c.apply(Cmd::Backspace, 1);
        assert_eq!(c.result_line(), "s");
        c.type_text("qrt", 1);
        assert_eq!(c.expression_line(), "2 + \u{221a}(", "sqrt went in at its last letter");
        assert!(c.letters.is_empty());
        let mut c = Calculator::new();
        c.type_text("sin", 1);
        assert_eq!(c.result_line(), "sin", "sin waits: sinh is a name too");
        c.type_char('h', 1);
        assert_eq!(c.expression_line(), "sinh(");
    }

    #[test]
    fn letters_that_are_no_name_are_dropped_with_a_hint() {
        let mut c = Calculator::new();
        c.type_text("q", 1);
        assert!(c.letters.is_empty());
        assert!(c.hint.as_deref().is_some_and(|h| h.contains('q')), "{:?}", c.hint);
        c.type_char('5', 1);
        assert_eq!(c.hint, None, "the next key clears it");
        assert_eq!(c.result_line(), "5");
    }

    #[test]
    fn x_makes_the_entry_a_function_and_equals_plots_it() {
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Scientific);
        c.type_text("sin(x)*x^2", 1);
        assert!(c.entry_is_function());
        assert_eq!(c.expression_line(), "sin(x) \u{d7} x^");
        assert_eq!(c.result_line(), "2");
        c.type_char('=', 1);
        assert_eq!(c.plots.len(), 1);
        assert_eq!(c.plots[0].text, "sin(x) \u{d7} x^2");
        assert_eq!(lines(&c), ("y = sin(x) \u{d7} x^2".to_string(), "Plotted as y\u{2081}".to_string()));
        assert!(c.history.is_empty(), "a function is not a calculation");
        c.type_text("2x+1=", 1);
        assert_eq!(c.plots.len(), 2);
        assert_eq!(c.plots[1].text, "2 \u{d7} x + 1");
        c.remove_plot(0);
        assert_eq!(c.plots.len(), 1);
    }

    #[test]
    fn y_equals_defines_a_function_even_without_x() {
        let mut c = Calculator::new();
        c.type_text("y=", 1);
        assert!(c.defining);
        assert_eq!(c.expression_line(), "y =");
        c.type_text("3=", 1);
        assert_eq!(c.plots.len(), 1, "y = 3 is a horizontal line");
        assert_eq!(c.expression_line(), "y = 3");
        let mut c = Calculator::new();
        c.paste("y = x^2").unwrap();
        c.apply(Cmd::Equals, 1);
        assert_eq!(c.plots[0].text, "x^2");
    }

    #[test]
    fn programmer_prefixes_switch_the_base() {
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Programmer);
        c.type_text("0xff", 1);
        assert_eq!(c.base, Base::Hex);
        assert_eq!(c.result_line(), "FF");
        assert_eq!(c.programmer_lines()[1], (Base::Dec, "255".to_string()));
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Programmer);
        c.type_text("0b101+1=", 1);
        assert_eq!(c.base, Base::Bin);
        assert_eq!(c.result_line(), "110");
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Programmer);
        c.type_text("6 xor 3=", 1);
        assert_eq!(c.result_line(), "5", "operator words");
        let mut c = Calculator::new();
        c.set_mode(CalcMode::Programmer);
        c.type_text("3x4=", 1);
        assert_eq!(c.result_line(), "12", "x is times in Programmer mode");
    }

    #[test]
    fn subscripts_name_the_graphs_functions() {
        assert_eq!(subscript(1), "\u{2081}");
        assert_eq!(subscript(12), "\u{2081}\u{2082}");
    }
}
