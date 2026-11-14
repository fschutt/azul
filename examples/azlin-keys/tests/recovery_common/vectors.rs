//! The vector files `tests/vectors/{recovery_wrap,shamir_kat,contacts}.json`: an array of
//! `{"name", "valid", "hex", "expect", "inputs"}` rows, one object per line (rule 10 of the W1
//! lanes). They are written by the INDEPENDENT program of the lane (`s15d_ref.py`: Crockford,
//! GF(256) by log/exp tables, BLAKE3, X25519, XChaCha20-Poly1305, Ed25519, CBOR, Argon2id and
//! ML-KEM by OpenSSL or from FIPS 203), never by this crate: the tests here only READ them and
//! compare what the crate computes with the file, so a change of the crate's output is a failing
//! test, not a rewritten vector.
//!
//! A valid row is a known answer: `hex` is the bytes (an envelope, shares, the bytes of a text),
//! `expect` what the code must give. An invalid row is a refused input: `expect` is the word of
//! the error. `inputs` says how an independent program rebuilds the row.
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

pub fn path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors")
        .join(file)
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
        let mut row = Row {
            name: String::new(),
            valid: false,
            hex: String::new(),
            expect: String::new(),
            inputs: String::new(),
        };
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

/// The canonical text of the rows: one object per line, so a diff shows one vector.
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

/// The rows of the file; the canonical form is checked, and every name is unique.
pub fn read(file: &str) -> Vec<Row> {
    let text = std::fs::read_to_string(path(file)).unwrap_or_else(|e| {
        panic!("{file} is missing or unreadable ({e}): run the independent program of the lane")
    });
    let rows = parse(&text);
    assert_eq!(
        text,
        render(&rows),
        "{file} is not in the canonical form of the vector files"
    );
    let mut names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), rows.len(), "{file}: a name is used twice");
    rows
}

/// The row with this name.
pub fn row<'a>(rows: &'a [Row], name: &str) -> &'a Row {
    rows.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no vector named {name}"))
}
