//! The source-gate kit of azul's apps (ARCH28 AR5b-0 / AR5a-10).
//!
//! A gate is a test that reads the shipped code and fails when a rule is broken where it must
//! not be: "no `todo!()` in the storage crates", "the test-hook switches are read only in these
//! files". It never runs the code it reads, and it reads the way a compiler lays the files out:
//!
//! - [`code`] blanks comments and `#[cfg(test)]` items with a lexer, not by text, so what
//!   follows a test module in the middle of a file is read, a brace in a string is none, and a
//!   line number is the file's;
//! - [`files`] (module [`read`]) reads every file a build compiles under the roots a gate names (a crate's module
//!   tree from its targets); a file only a test build compiles is a test file, and a file under
//!   a root that no build compiles fails the gate, so no code escapes by not being declared;
//! - the finders ([`calls`], [`method_calls`], [`macro_calls`], [`idents`], [`strings`]) answer
//!   as the lines where a thing is written;
//! - a [`Rule`] is a finder with its own planted and clean cases ([`self_test`]); a [`Budget`]
//!   takes the hits by file with a reason and a count that only goes down;
//! - a [`Gate`] runs all of it.
//!
//! It reads source text, it does not parse Rust: `use a::b as c; c()` is a call of `c`. The
//! rule's cases show what it relies on. Ported from the backend's `azlin-proto` `pins` (the
//! azul-apps repository), which reads that tree's code the same way.
//!
//! The gates of the apps are `tests/*_gate.rs` of this crate (they read other crates' sources
//! by path from the workspace's root): `tests/keyring_gate.rs`, `tests/todo_gate.rs`,
//! `tests/durable_gate.rs` (a file is put in place only through `azul_storage::durable`) and
//! `tests/names_gate.rs` (the engine's own names are spelled in `names.rs`) today; a later work
//! order adds its own next to them.

pub mod find;
pub mod gate;
pub mod lex;
pub mod read;
pub mod tree;

pub use find::{calls, idents, locate, macro_calls, method_calls, strings, Located};
pub use gate::{hits, self_test, workspace, Allow, Budget, Gate, Hit, Rule};
pub use lex::{code, lex, lex_spans, Spanned, Tok};
pub use read::{files, Code, Orphans, Root};
