//! The sample workspace (`--sample`): a small Rust crate, its README and
//! `huge.rs` - 100,000 generated lines, the acceptance test of the
//! CodeView's line windowing and of the highlighter's checkpoints. Written
//! once into the data tree (`code/sample/...`) through the Drive.

use azul_appkit::{data::app_key, files::FileJob};

/// The app's folder in the data tree.
pub const APP_FOLDER: &str = "code";
/// The sample workspace's folder in the app's folder.
pub const SAMPLE_FOLDER: &str = "sample";
/// The lines of `huge.rs`.
pub const HUGE_LINES: usize = 100_000;

/// `src/main.rs`.
pub const MAIN_RS: &str = r#"//! A tiny program to edit.

use std::collections::BTreeMap;

/// Counts the words of `text`.
fn word_counts(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    counts
}

fn main() {
    let text = "the quick brown fox jumps over the lazy dog";
    /* the counts, most frequent first */
    let mut counts: Vec<(String, usize)> = word_counts(text).into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    for (word, n) in counts.iter().take(3) {
        println!("{word}: {n}");
    }
}
"#;

/// `src/lib.rs`.
pub const LIB_RS: &str = r#"//! Shapes and their areas.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Circle { radius: f64 },
    Rect { width: f64, height: f64 },
}

impl Shape {
    /// The shape's area.
    pub fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rect { width, height } => width * height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_square_has_area_one() {
        assert_eq!(Shape::Rect { width: 1.0, height: 1.0 }.area(), 1.0);
    }
}
"#;

/// `Cargo.toml`.
pub const CARGO_TOML: &str = r#"[package]
name = "sample"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;

/// `README.md`.
pub const README_MD: &str = "# Sample\n\nA small crate to try AzCode on.\n\n- `src/main.rs` counts words\n- `src/lib.rs` has shapes\n- `huge.rs` has 100,000 lines: scroll it, jump to line 90000 (Ctrl+G), search it (Ctrl+F)\n";

/// `huge.rs`: `lines` generated lines of Rust (a function every ten lines,
/// a block comment every thousand).
#[must_use]
pub fn huge_rs(lines: usize) -> String {
    let mut out = String::with_capacity(lines * 32);
    let mut n = 0;
    while n < lines {
        let block = n / 10;
        let line = match n % 10 {
            0 if block % 100 == 0 => format!("/* block {block}: a comment"),
            1 if block % 100 == 0 => "   across two lines */".to_string(),
            0 | 1 => format!("// function {block}"),
            2 => format!("pub fn f{block}(x: u64) -> u64 {{"),
            3 => format!("    let y = x * {} + {};", block % 7 + 1, block % 13),
            4 => "    if y % 2 == 0 {".to_string(),
            5 => format!("        return y / 2; // even, line {}", n + 1),
            6 => "    }".to_string(),
            7 => format!("    println!(\"f{block}: {{}}\", y);"),
            8 => "    y".to_string(),
            _ => "}".to_string(),
        };
        out.push_str(&line);
        out.push('\n');
        n += 1;
    }
    out
}

/// The sample workspace's key prefix in the data tree: `code/sample/`.
#[must_use]
pub fn prefix() -> String {
    app_key(APP_FOLDER, &format!("{SAMPLE_FOLDER}/"))
}

/// The jobs that write the sample workspace.
#[must_use]
pub fn jobs() -> Vec<FileJob> {
    let root = prefix();
    let files: [(&str, String); 5] = [
        ("src/main.rs", MAIN_RS.to_string()),
        ("src/lib.rs", LIB_RS.to_string()),
        ("Cargo.toml", CARGO_TOML.to_string()),
        ("README.md", README_MD.to_string()),
        ("huge.rs", huge_rs(HUGE_LINES)),
    ];
    files
        .into_iter()
        .map(|(name, text)| FileJob::Put {
            key: format!("{root}{name}"),
            bytes: text.into_bytes(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{buffer::TextBuffer, highlight::Highlighter, highlight::syntax_for};

    #[test]
    fn huge_rs_has_a_hundred_thousand_lines_and_reads_back() {
        let text = huge_rs(HUGE_LINES);
        let b = TextBuffer::from_text(&text);
        assert_eq!(b.line_count(), HUGE_LINES + 1, "the last line is the empty one after the break");
        assert_eq!(b.line(90_002), "pub fn f9000(x: u64) -> u64 {");
        assert_eq!(prefix(), "code/sample/");
        let keys: Vec<String> = jobs()
            .into_iter()
            .map(|j| match j {
                FileJob::Put { key, .. } => key,
                other => panic!("{other:?}"),
            })
            .collect();
        assert!(keys.contains(&"code/sample/huge.rs".to_string()));
    }

    #[test]
    fn the_sample_sources_highlight_as_rust() {
        let b = TextBuffer::from_text(MAIN_RS);
        let mut h = Highlighter::new(syntax_for("main.rs", ""));
        let text_of = |i: usize| b.line(i);
        for line in 0..b.line_count() {
            assert!(h.line_spans(line, &text_of).is_some());
        }
        assert_eq!(h.language(), "Rust");
    }
}
