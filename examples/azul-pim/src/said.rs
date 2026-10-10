//! Words for people as messages of azul-appkit's resources (`kit-rule-weekly`) and their
//! arguments: azul-pim knows what to say, an app says it in the window's language
//! (azul-appkit's `l10n::t_said`). No words of any language here.

use chrono::NaiveDate;

/// A message and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Said {
    /// The message's id in azul-appkit's resources.
    pub id: &'static str,
    /// Its arguments, by name.
    pub args: Vec<(&'static str, SaidArg)>,
}

/// An argument of a [`Said`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaidArg {
    /// A number (a count: the language picks the plural form by it).
    Number(i64),
    /// Other words, said first.
    Said(Said),
    /// Words said one by one and joined as a list: "Monday, Wednesday and Friday".
    List(Vec<Said>),
    /// A day and its month: "30 September".
    DayMonth(NaiveDate),
    /// A date: "31 December 2026".
    Date(NaiveDate),
}

impl Said {
    /// The message `id` without arguments.
    #[must_use]
    pub fn new(id: &'static str) -> Said {
        Said {
            id,
            args: Vec::new(),
        }
    }

    /// With the argument `name`.
    #[must_use]
    pub fn arg(mut self, name: &'static str, value: SaidArg) -> Said {
        self.args.push((name, value));
        self
    }
}
