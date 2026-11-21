//! The vector file `tests/vectors/envelope_v2.json`: an array of
//! `{"name", "valid", "hex", "expect", "inputs"}` rows, one object per line (rule 10 of the W1
//! lanes). A valid row is a known answer: `hex` is the envelope, `expect` the plaintext (hex), and
//! `inputs` says how an independent program rebuilds it. An invalid row is a refused input:
//! `hex` are the bytes, `expect` is the `Debug` text of the error `check_shape` answers.
//!
//! The file is written only under `UPDATE_VECTORS=1` and never when `CI` is set; otherwise a
//! test compares the rows it computes with the file and fails with the first difference.
#![allow(dead_code)]

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub valid: bool,
    pub hex: String,
    pub expect: String,
    pub inputs: String,
}

impl Row {
    pub fn new(name: &str, valid: bool, hex: &str, expect: &str, inputs: &str) -> Row {
        Row {
            name: name.to_string(),
            valid,
            hex: hex.to_string(),
            expect: expect.to_string(),
            inputs: inputs.to_string(),
        }
    }
}

pub fn path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors")
        .join(file)
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            other => panic!("vector files hold printable ASCII only, not {other:?}"),
        }
    }
    out.push('"');
    out
}

/// The text of the file for these rows: one object per line, so a diff shows one vector.
pub fn render(rows: &[Row]) -> String {
    let mut out = String::from("[\n");
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!(
            "  {{\"name\": {}, \"valid\": {}, \"hex\": {}, \"expect\": {}, \"inputs\": {}}}",
            quote(&r.name),
            r.valid,
            quote(&r.hex),
            quote(&r.expect),
            quote(&r.inputs)
        ));
        out.push_str(if i + 1 < rows.len() { ",\n" } else { "\n" });
    }
    out.push_str("]\n");
    out
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(|b| b.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) {
        self.ws();
        assert_eq!(
            self.s.get(self.i),
            Some(&c),
            "vector file: expected {:?} at byte {}",
            c as char,
            self.i
        );
        self.i += 1;
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn string(&mut self) -> String {
        self.eat(b'"');
        let mut out = String::new();
        loop {
            let b = *self.s.get(self.i).expect("an unterminated string");
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).expect("an unterminated escape");
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        other => panic!("vector file: bad escape {:?}", other as char),
                    }
                }
                _ => out.push(b as char),
            }
        }
        out
    }

    fn boolean(&mut self) -> bool {
        self.ws();
        for (word, value) in [("true", true), ("false", false)] {
            if self.s[self.i..].starts_with(word.as_bytes()) {
                self.i += word.len();
                return value;
            }
        }
        panic!("vector file: expected a boolean at byte {}", self.i)
    }

    fn object(&mut self) -> Row {
        self.eat(b'{');
        let mut row = Row::new("", false, "", "", "");
        let mut seen_valid = false;
        loop {
            let key = self.string();
            self.eat(b':');
            match key.as_str() {
                "name" => row.name = self.string(),
                "valid" => {
                    row.valid = self.boolean();
                    seen_valid = true;
                }
                "hex" => row.hex = self.string(),
                "expect" => row.expect = self.string(),
                "inputs" => row.inputs = self.string(),
                other => panic!("vector file: unknown key {other:?}"),
            }
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(b'}');
        assert!(seen_valid, "vector file: a row without `valid`");
        row
    }

    fn rows(&mut self) -> Vec<Row> {
        self.eat(b'[');
        let mut rows = Vec::new();
        if self.peek() == Some(b']') {
            self.i += 1;
            return rows;
        }
        loop {
            rows.push(self.object());
            if self.peek() == Some(b',') {
                self.i += 1;
            } else {
                break;
            }
        }
        self.eat(b']');
        rows
    }
}

pub fn parse(text: &str) -> Vec<Row> {
    let mut p = Parser {
        s: text.as_bytes(),
        i: 0,
    };
    let rows = p.rows();
    p.ws();
    assert_eq!(p.i, text.len(), "vector file: trailing bytes");
    rows
}

/// The rows of the file as they are now (the canonical form is checked).
pub fn read(file: &str) -> Vec<Row> {
    let text = std::fs::read_to_string(path(file)).unwrap_or_else(|e| {
        panic!("{file} is missing or unreadable ({e}): UPDATE_VECTORS=1 writes it, never in CI")
    });
    let rows = parse(&text);
    assert_eq!(
        text,
        render(&rows),
        "{file} is not in the canonical form that UPDATE_VECTORS=1 writes"
    );
    rows
}

/// Compares `computed` with the file, or writes the file under `UPDATE_VECTORS=1` (never when
/// `CI` is set). A difference fails with the first differing row.
pub fn check_or_update(file: &str, computed: &[Row]) {
    if std::env::var_os("UPDATE_VECTORS").is_some_and(|v| v == "1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "UPDATE_VECTORS=1 refuses to write {file} when CI is set"
        );
        std::fs::create_dir_all(path(file).parent().expect("a parent folder"))
            .expect("the vectors folder");
        std::fs::write(path(file), render(computed)).expect("write the vector file");
        return;
    }
    let pinned = read(file);
    for (i, (want, got)) in pinned.iter().zip(computed).enumerate() {
        assert_eq!(
            want, got,
            "{file}: row {i} ({}) differs from what the code computes now (pinned left, computed right)",
            want.name
        );
    }
    assert_eq!(
        pinned.len(),
        computed.len(),
        "{file}: {} pinned rows, the code computes {}",
        pinned.len(),
        computed.len()
    );
}
