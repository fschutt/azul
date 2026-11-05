//! Gate: in the files that read bytes other people wrote, a size is reserved only from something
//! the reader knows (ARCH28 RF50 / AR5b-5, Z-17).
//!
//! `panic = "abort"` is the shipped profile, so a size, count or offset read from a header, an
//! index entry or a chunk line that reaches `with_capacity`, `reserve`, `resize` or `vec![_; n]`
//! before it is compared with anything is a remote abort. The ways to be safe are written next to
//! the call: a literal or a constant, a `.len()` of something the reader holds, a `.min(..)`, a
//! `Declared::prealloc()`, the crate's own `reserve(..)` helper. What the rule cannot read as one
//! of those is an allowance below with the reason the number is bounded anyway - the check that
//! stands in front of it. The counts only go down.
//!
//! The rule reads the argument's tokens, not types: `Vec::with_capacity(count)` is unbounded to
//! it even when `count` was checked three lines up; that is what the allowance says. The files it
//! reads are `PARSERS`: the shipped code of the formats that read bytes of a bucket, a header, an
//! index or an answer. Test code is not read.

use azul_srcgate::{files, hits, self_test, workspace, Allow, Budget, Located, Orphans, Root, Rule};

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 30,
    },
    Root {
        dir: "examples/azlin-compress/src",
        min_files: 10,
    },
];

/// The files that read untrusted bytes.
const PARSERS: &[&str] = &[
    "examples/azul-storage/src/http1.rs",
    "examples/azul-storage/src/s3.rs",
    "examples/azul-storage/src/sharing.rs",
    "examples/azul-storage/src/packing.rs",
    "examples/azul-storage/src/encrypted.rs",
    "examples/azul-storage/src/crypto/azl1.rs",
    "examples/azul-storage/src/crypto/keys.rs",
    "examples/azul-storage/src/crypto/device.rs",
    "examples/azul-storage/src/crypto/drops.rs",
    "examples/azul-storage/src/crypto/share.rs",
    "examples/azul-storage/src/crypto/contacts.rs",
    "examples/azul-storage/src/crypto/mod.rs",
    "examples/azul-storage/src/meta/pack.rs",
    "examples/azul-storage/src/meta/pointer.rs",
    "examples/azlin-compress/src/transform/records.rs",
    "examples/azlin-compress/src/transform/mod.rs",
];

const ALLOC: &str = "alloc-from-an-unchecked-size";

/// Identifiers that bound a size: the length of something held, a cap, a first reservation, the
/// crate's own helper that caps one.
const BOUNDS: &[&str] = &[
    "len", "min", "prealloc", "reserve", "capacity", "PREALLOC", "RESERVE_CAP",
];

/// Primitive types and keywords that appear in a constant expression (`HEADER_LEN as usize`).
const CONSTANT_WORDS: &[&str] = &[
    "as", "usize", "u8", "u16", "u32", "u64", "u128", "i32", "i64", "isize", "size_of",
];

/// Whether an identifier is a number or a constant by its spelling (`0u8`, `HEADER_LEN`).
fn constant_like(word: &str) -> bool {
    let number = word.chars().next().is_some_and(|c| c.is_ascii_digit());
    let constant = word
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && word.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    number || constant || CONSTANT_WORDS.contains(&word)
}

/// Whether the tokens of a size expression are bounded by something the reader holds.
fn bounded(expression: &[Located]) -> bool {
    let words: Vec<&str> = expression
        .iter()
        .filter_map(|x| x.tok.ident())
        .collect();
    words.iter().any(|w| BOUNDS.contains(w)) || words.iter().all(|w| constant_like(w))
}

/// The index just past the closer that matches the opener at `open` (`(` or `[`), or the end.
fn close_of(t: &[Located], open: usize) -> usize {
    let mut depth = 0usize;
    for (i, x) in t.iter().enumerate().skip(open) {
        if let azul_srcgate::Tok::Punct(c) = &x.tok {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
        }
    }
    t.len()
}

/// The end of the first argument that starts at `from` and runs to `close`: the first top-level
/// comma, else `close`.
fn first_argument_end(t: &[Located], from: usize, close: usize) -> usize {
    let mut depth = 0usize;
    for (i, x) in t.iter().enumerate().take(close).skip(from) {
        if let azul_srcgate::Tok::Punct(c) = &x.tok {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => return i,
                _ => {}
            }
        }
    }
    close
}

/// `with_capacity(n)`, `reserve(n)`, `reserve_exact(n)`, `try_reserve(n)`, `resize(n, v)` and
/// `vec![v; n]` whose size `n` is not bounded.
fn unchecked_allocations(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let Some(name) = x.tok.ident() else { continue };
        let previous = i.checked_sub(1).map(|k| &t[k].tok);
        if matches!(name, "with_capacity" | "reserve" | "reserve_exact" | "try_reserve" | "resize")
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('('))
            && !previous.is_some_and(|p| p.is_ident("fn"))
        {
            let close = close_of(t, i + 1);
            let end = first_argument_end(t, i + 2, close);
            if !bounded(&t[i + 2..end.min(t.len())]) {
                found.push(x.line);
            }
        } else if name == "vec"
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('!'))
            && t.get(i + 2).is_some_and(|n| n.tok.is_punct('['))
        {
            let close = close_of(t, i + 2);
            // the size is what follows the top-level `;`
            let mut depth = 0usize;
            let mut semi = None;
            for (k, y) in t.iter().enumerate().take(close).skip(i + 2) {
                if let azul_srcgate::Tok::Punct(c) = &y.tok {
                    match c {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' => depth = depth.saturating_sub(1),
                        ';' if depth == 1 => {
                            semi = Some(k);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            if let Some(semi) = semi {
                if !bounded(&t[semi + 1..close.min(t.len())]) {
                    found.push(x.line);
                }
            }
        }
    }
    found
}

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        ALLOC,
        unchecked_allocations,
        &[
            "let v: Vec<u8> = Vec::with_capacity(declared);",
            "let v = vec![0u8; size];",
            "buf.reserve(n);",
            "buf.resize(total, 0);",
            "let v = Vec::with_capacity(header.count + 1);",
        ],
        &[
            "let v: Vec<u8> = Vec::with_capacity(8);",
            "let v: Vec<u8> = Vec::with_capacity(HEADER_LEN + 4);",
            "let v = Vec::with_capacity(HEADER_LEN as usize);",
            "let v = Vec::with_capacity(items.len() * 2);",
            "let v = Vec::with_capacity(n.min(1 << 20));",
            "let v = Vec::with_capacity(Declared::new(n).prealloc());",
            "let v = Vec::with_capacity(reserve(len));",
            "let v = vec![0u8; HEADER_LEN];",
            "let v = vec![None; items.len()];",
            "out.try_reserve(chunk.len())?;",
            "buf.resize(BLOCK, 0);",
            "fn reserve(len: u64) -> usize { 0 }",
        ],
    )]
}

/// Where an unchecked-looking size stands, and the check that stands in front of it. TO SHRINK.
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azul-storage/src/crypto/azl1.rs",
        ALLOC,
        1,
        "the writer's pending segment: segment_size is the option this process writes with, and a \
         header's is checked by check_segment_size (1 KiB to 16 MiB) before an object is read",
    ),
    Allow::exactly(
        "examples/azul-storage/src/packing.rs",
        ALLOC,
        1,
        "seal_pack: body_len is the layout of the members THIS process is packing (PACK_MAX), not \
         a header's word",
    ),
    Allow::exactly(
        "examples/azul-storage/src/encrypted.rs",
        ALLOC,
        1,
        "write_from_many: `count` is `large.len()` of the batch the caller handed in, two lines up",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/pack.rs",
        ALLOC,
        2,
        "PackIndex::open ties `count` and `objects` to the index's real length with `expected` \
         BEFORE it reserves: the pattern this gate asks for",
    ),
    Allow::exactly(
        "examples/azlin-compress/src/transform/records.rs",
        ALLOC,
        2,
        "write_records: the sum of the pieces this process holds; the PNG record's chunk count is \
         compared with MAX_IDAT_CHUNKS on the lines above",
    ),
];

fn parser_files() -> Vec<azul_srcgate::Code> {
    let all = files(&workspace(), ROOTS, Orphans::Ignore).unwrap_or_else(|e| panic!("{e:#?}"));
    all.into_iter()
        .filter(|f| PARSERS.contains(&f.rel.as_str()))
        .collect()
}

#[test]
fn a_size_reserved_in_a_parser_is_bounded_by_something_the_reader_holds() {
    self_test(&rules());
    let read = parser_files();
    assert_eq!(
        read.len(),
        PARSERS.len(),
        "a parser file was not read: {:?}",
        PARSERS
            .iter()
            .filter(|p| !read.iter().any(|f| f.rel == **p))
            .collect::<Vec<_>>()
    );
    let found = hits(&read, &rules());
    let wrong = Budget(ALLOWED).judge(&found);
    assert!(wrong.is_empty(), "{} finding(s):\n{}", wrong.len(), wrong.join("\n"));
}

/// The allowances name files the gate reads.
#[test]
fn every_allowance_names_a_parser_file() {
    for a in ALLOWED {
        assert!(PARSERS.contains(&a.file), "{} is allowed ({}) and is no parser file", a.file, a.why);
    }
}

/// The rule on code written out: bounded sizes pass, a size from a variable does not, a `vec!`
/// with a comma inside its element is read by its `;`.
#[test]
fn the_rule_reads_the_size_of_each_form() {
    let code = |src: &str| azul_srcgate::locate(&azul_srcgate::code(src));
    let t = code(
        "fn f() {\n let a = vec![(1, 2); n];\n let b = vec![(1, 2); items.len()];\n let c = Vec::with_capacity(a, b);\n let d = x.resize(m, 0);\n}\n",
    );
    assert_eq!(unchecked_allocations(&t), vec![2, 4, 5]);
}
