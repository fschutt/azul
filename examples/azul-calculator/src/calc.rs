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

use crate::expr::{self, AngleUnit, BinOp, Const, Domain, Func, Post, Tok};
use crate::history::{HistoryEntry, Memory, MAX_HISTORY};
use crate::num::{format_typed, CalcError, Format, Num, DISPLAY_DIGITS};
use crate::programmer::{self, Base, WordSize};

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
    Func(Func),
    Post(Post),
    /// +/-.
    Negate,
    /// Exp: scientific-notation entry (`1.5E3`).
    Exp,
    Const(Const),
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

/// The most significant digits a typed decimal number takes.
pub const MAX_TYPED_DIGITS: usize = 32;

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
}

impl Default for Calculator {
    fn default() -> Self {
        Calculator::new()
    }
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
        expr::display(&self.tokens[..end], self.grouping, self.mode.is_programmer())
            .trim_end()
            .to_string()
    }

    /// The result line: the error, the number being typed (as typed), or the
    /// value of what is built so far.
    #[must_use]
    pub fn result_line(&self) -> String {
        if let Some(e) = &self.error {
            return e.clone();
        }
        if self.editing && !self.just_evaluated {
            if let Some(Tok::Num(text)) = self.tokens.last() {
                return if self.mode.is_programmer() {
                    text.clone()
                } else {
                    format_typed(text, self.grouping)
                };
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

    /// The text Ctrl+C copies: the result line's value without grouping.
    #[must_use]
    pub fn copy_text(&self) -> String {
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

    /// Starts over after `=` or an error (the memory and history stay).
    fn fresh(&mut self) {
        self.tokens.clear();
        self.editing = false;
        self.just_evaluated = false;
        self.shown_expr.clear();
        self.error = None;
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
            Tok::Num(_) | Tok::Const(_) => {}
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
        self.tokens.clear();
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

    /// C: everything but the memory and the history.
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
        if let Some(t) = self.tokens.pop() {
            if t == Tok::LParen && matches!(self.tokens.last(), Some(Tok::Func(_))) {
                self.tokens.pop();
            }
        }
        self.editing = matches!(self.tokens.last(), Some(Tok::Num(_)));
    }

    // ==== Modes, bases, history ====

    /// Switches the keypad mode; between decimals and Programmer's integers
    /// the entry starts over (the memory and the history stay).
    pub fn set_mode(&mut self, mode: CalcMode) {
        if mode.is_programmer() != self.mode.is_programmer() {
            self.clear();
        }
        self.mode = mode;
        self.second = false;
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
    /// `=`, appended otherwise. `Err` says why it could not be read.
    pub fn paste(&mut self, text: &str) -> Result<(), String> {
        let tokens = expr::tokenize(text.trim(), self.domain()).map_err(|e| e.to_string())?;
        if tokens.is_empty() {
            return Ok(());
        }
        if self.just_evaluated || self.error.is_some() {
            self.fresh();
        }
        self.finish_number();
        if self.last_ends_operand() && tokens[0].starts_operand() {
            self.tokens.push(Tok::Op(BinOp::Mul));
        }
        self.tokens.extend(tokens);
        self.editing = matches!(self.tokens.last(), Some(Tok::Num(_)));
        Ok(())
    }
}

/// What a typed character does in a mode: digits, operators, `.`, the
/// Windows calculator's letters (`@` √, `q` x², `r` 1/x, `s` sin, `o` cos,
/// `t` tan, `n` ln, `l` log, `p` π, `e` e, `E` Exp, `!` n!) and Programmer
/// mode's `a`-`f`, `&` `|` `^` (XOR) `~` `<` `>` and `%` (mod).
#[must_use]
pub fn char_command(c: char, mode: CalcMode) -> Option<Cmd> {
    if let Some(d) = c.to_digit(10) {
        return Some(Cmd::Digit(d as u8));
    }
    let programmer = mode.is_programmer();
    if programmer {
        if let Some(d) = c.to_digit(16) {
            return Some(Cmd::Digit(d as u8));
        }
        return match c {
            '+' => Some(Cmd::Op(BinOp::Add)),
            '-' => Some(Cmd::Op(BinOp::Sub)),
            '*' | 'x' => Some(Cmd::Op(BinOp::Mul)),
            '/' => Some(Cmd::Op(BinOp::Div)),
            '%' => Some(Cmd::Op(BinOp::Mod)),
            '&' => Some(Cmd::Op(BinOp::And)),
            '|' => Some(Cmd::Op(BinOp::Or)),
            '^' => Some(Cmd::Op(BinOp::Xor)),
            '~' => Some(Cmd::Not),
            '<' => Some(Cmd::Op(BinOp::Shl)),
            '>' => Some(Cmd::Op(BinOp::Shr)),
            '(' => Some(Cmd::LParen),
            ')' => Some(Cmd::RParen),
            '=' => Some(Cmd::Equals),
            _ => None,
        };
    }
    let scientific = mode == CalcMode::Scientific;
    match c {
        '.' | ',' => Some(Cmd::Point),
        '+' => Some(Cmd::Op(BinOp::Add)),
        '-' => Some(Cmd::Op(BinOp::Sub)),
        '*' | 'x' | 'X' => Some(Cmd::Op(BinOp::Mul)),
        '/' => Some(Cmd::Op(BinOp::Div)),
        '%' => Some(Cmd::Post(Post::Percent)),
        '=' => Some(Cmd::Equals),
        '@' => Some(Cmd::Func(Func::Sqrt)),
        'q' => Some(Cmd::Post(Post::Square)),
        'r' => Some(Cmd::Func(Func::Recip)),
        '(' => Some(Cmd::LParen),
        ')' => Some(Cmd::RParen),
        '^' if scientific => Some(Cmd::Op(BinOp::Pow)),
        '!' if scientific => Some(Cmd::Post(Post::Factorial)),
        's' if scientific => Some(Cmd::Func(Func::Sin)),
        'o' if scientific => Some(Cmd::Func(Func::Cos)),
        't' if scientific => Some(Cmd::Func(Func::Tan)),
        'n' if scientific => Some(Cmd::Func(Func::Ln)),
        'l' if scientific => Some(Cmd::Func(Func::Log)),
        'p' if scientific => Some(Cmd::Const(Const::Pi)),
        'e' if scientific => Some(Cmd::Const(Const::E)),
        'E' if scientific => Some(Cmd::Exp),
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

    /// Types `keys` (one char each, `=` evaluates; `C` clears, `B`
    /// backspace, `N` negate, `R` CE) into a calculator in `mode`.
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
                other => char_command(other, mode).unwrap_or_else(|| panic!("no key {other:?}")),
            };
            c.apply(cmd, 1);
        }
        c
    }

    fn std(keys: &str) -> Calculator {
        typed(CalcMode::Standard, keys)
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
        for k in "2".chars() {
            c.apply(char_command(k, c.mode).unwrap(), 1);
        }
        c.apply(Cmd::Func(Func::Sqrt), 1);
        assert_eq!(c.expression_line(), "\u{221a}(2)");
        assert_eq!(c.result_line(), "1.4142135623730950488016887242097");
        c.apply(Cmd::Op(BinOp::Add), 1);
        c.apply(Cmd::Func(Func::Sin), 1);
        assert_eq!(c.expression_line(), "\u{221a}(2) + sin(");
        for k in "30)=".chars() {
            c.apply(char_command(k, c.mode).unwrap(), 1);
        }
        assert_eq!(c.expression_line(), "\u{221a}(2) + sin(30) =");
        assert!(c.result_line().starts_with("1.9142135623730"), "{}", c.result_line());
    }

    #[test]
    fn the_scientific_sample_by_keyboard() {
        let c = typed(CalcMode::Scientific, "s30)+2^10=");
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
        let c = typed(CalcMode::Scientific, "2*(3+4=");
        assert_eq!(c.expression_line(), "2 \u{d7} (3 + 4) =");
        assert_eq!(c.result_line(), "14");
        let c = typed(CalcMode::Scientific, "2*(3+4");
        assert_eq!(c.open_parens(), 1);
        let c = typed(CalcMode::Scientific, ")");
        assert!(c.tokens.is_empty(), "a ) with nothing open is ignored");
        let c = typed(CalcMode::Scientific, "2(3)=");
        assert_eq!(c.expression_line(), "2 \u{d7} (3) =");
        assert_eq!(c.result_line(), "6");
    }

    #[test]
    fn the_exponent_key_enters_scientific_notation() {
        let c = typed(CalcMode::Scientific, "1.5E3");
        assert_eq!(c.result_line(), "1.5e+3");
        let c = typed(CalcMode::Scientific, "1.5E3N");
        assert_eq!(c.result_line(), "1.5e-3", "+/- after Exp flips the exponent's sign");
        assert_eq!(typed(CalcMode::Scientific, "1.5E3+1=").result_line(), "1,501");
        assert_eq!(typed(CalcMode::Scientific, "2E+1=").result_line(), "3", "a dangling E goes");
    }

    #[test]
    fn constants_multiply_implicitly() {
        let c = typed(CalcMode::Scientific, "2p=");
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
        for k in "2a5f".chars() {
            c.apply(char_command(k, c.mode).unwrap(), 1);
        }
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
        for k in "999".chars() {
            c.apply(char_command(k, c.mode).unwrap(), 1);
        }
        assert_eq!(c.result_line(), "99", "999 does not fit a byte");
        let c2 = {
            let mut c = Calculator::new();
            c.set_mode(CalcMode::Programmer);
            c.set_word(WordSize::Byte);
            for k in "127+1=".chars() {
                c.apply(char_command(k, c.mode).unwrap(), 1);
            }
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
        for k in "+1=".chars() {
            c.apply(char_command(k, c.mode).unwrap(), 1);
        }
        assert_eq!(c.result_line(), "9");
        c.toggle_bit(1);
        assert_eq!(c.result_line(), "11", "after = the result is toggled");
    }

    #[test]
    fn switching_between_decimals_and_programmer_starts_over() {
        let mut c = std("12+3=");
        c.set_mode(CalcMode::Scientific);
        assert_eq!(c.result_line(), "15", "Standard and Scientific share the entry");
        c.set_mode(CalcMode::Programmer);
        assert_eq!(c.result_line(), "0");
        assert_eq!(c.history.len(), 1, "the history stays");
    }

    #[test]
    fn a_history_entry_comes_back_as_the_value_to_go_on_with() {
        let mut c = std("1280*0.19=C");
        c.use_history(0);
        assert_eq!(lines(&c), ("1,280 \u{d7} 0.19 =".to_string(), "243.2".to_string()));
        c.apply(Cmd::Op(BinOp::Add), 1);
        for k in "18.5=".chars() {
            c.apply(char_command(k, c.mode).unwrap(), 1);
        }
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
        assert_eq!(char_command('s', CalcMode::Standard), None, "sin needs Scientific");
        assert_eq!(char_command('s', CalcMode::Scientific), Some(Cmd::Func(Func::Sin)));
        assert_eq!(char_command('f', CalcMode::Programmer), Some(Cmd::Digit(15)));
        assert_eq!(char_command('%', CalcMode::Programmer), Some(Cmd::Op(BinOp::Mod)));
        assert_eq!(char_command('%', CalcMode::Standard), Some(Cmd::Post(Post::Percent)));
        assert_eq!(named_command(NamedKey::Enter), Cmd::Equals);
        assert_eq!(named_command(NamedKey::Escape), Cmd::Clear);
        assert_eq!(named_command(NamedKey::Delete), Cmd::ClearEntry);
        assert_eq!(named_command(NamedKey::F9), Cmd::Negate);
        assert_eq!(CalcMode::by_key("programmer"), Some(CalcMode::Programmer));
    }
}
