//! Gate: in the files that read bytes other people wrote, a size is reserved only from something
//! the reader knows (ARCH28 RF50 / AR5b-5, Z-17).
//!
//! `panic = "abort"` is the shipped profile, so a size, count or offset read from a header, an
//! index entry or a chunk line that reaches `with_capacity`, `reserve`, `resize`, `resize_with`,
//! `vec![_; n]`, `.repeat(n)`, `repeat(..).take(n).collect()` or `(0..n).map(..).collect()` before
//! it is compared with anything is a remote abort. The ways to be safe are written next to the
//! call, and each is a CALL or a CONSTANT, never a variable that happens to be named like one: a
//! literal or a constant, a `.len()` / `.capacity()` of something the reader holds, a `.min(..)`, a
//! `Declared::prealloc()`, the crate's own free `reserve(..)` helper (which is itself no
//! allocation site: it is the cap). What the rule cannot read as one of those is an allowance below
//! with the reason the number is bounded anyway - the check that stands in front of it. The counts
//! only go down.
//!
//! The first version of this rule took ANY identifier named `len`, `min`, `capacity` or `reserve`
//! as a bound, so `Vec::with_capacity(usize::try_from(len).unwrap_or(0))` - the line X-AR4-37 was
//! about - passed, and it counted the arguments of the `reserve(..)` helper as allocations: it had
//! never been green (X-DELTA-AZDRIVE-KEYS-16). The counts below were made again with this rule's
//! own scanner.
//!
//! The rule reads the argument's tokens, not types: `Vec::with_capacity(count)` is unbounded to
//! it even when `count` was checked three lines up; that is what the allowance says. The files it
//! reads are `PARSERS`: the shipped code of the formats that read bytes of a bucket, a header, an
//! index, an answer or a compressed stream. Test code is not read.

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
    "examples/azul-storage/src/crypto/shamir.rs",
    "examples/azul-storage/src/meta/pack.rs",
    "examples/azul-storage/src/meta/pointer.rs",
    "examples/azul-storage/src/meta/wal.rs",
    "examples/azul-storage/src/meta/repo.rs",
    "examples/azul-storage/src/meta/index.rs",
    "examples/azul-storage/src/meta/tree.rs",
    "examples/azul-storage/src/meta/objects.rs",
    "examples/azul-storage/src/meta/cache.rs",
    "examples/azul-storage/src/multipart.rs",
    "examples/azul-storage/src/ops.rs",
    "examples/azul-storage/src/oauth.rs",
    "examples/azul-storage/src/tables.rs",
    "examples/azul-storage/src/transport.rs",
    "examples/azul-storage/src/xml.rs",
    "examples/azlin-compress/src/container.rs",
    "examples/azlin-compress/src/segment.rs",
    "examples/azlin-compress/src/member.rs",
    "examples/azlin-compress/src/transform/records.rs",
    "examples/azlin-compress/src/transform/mod.rs",
    "examples/azlin-compress/src/transform/deflate.rs",
    "examples/azlin-compress/src/transform/jpeg.rs",
    "examples/azlin-compress/src/transform/lepton.rs",
    "examples/azlin-compress/src/transform/scan.rs",
];

const ALLOC: &str = "alloc-from-an-unchecked-size";

/// Methods that bound a size when they are CALLED (`x.len()`, `n.min(..)`, `v.capacity()`,
/// `Declared::new(n).prealloc()`, `std::cmp::min(..)`): a variable that is named `len` bounds
/// nothing.
const BOUNDING_CALLS: &[&str] = &["len", "min", "prealloc", "capacity"];

/// Constants that are the cap itself.
const BOUNDING_CONSTANTS: &[&str] = &["PREALLOC", "RESERVE_CAP"];

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

fn punct_at(t: &[Located], i: usize, c: char) -> bool {
    t.get(i).is_some_and(|x| x.tok.is_punct(c))
}

/// Whether the tokens of a size expression are bounded by something the reader holds: a call that
/// bounds ([`BOUNDING_CALLS`] after a `.` or a path, or the free `reserve(..)` helper), a cap
/// constant, or nothing but constants. One bounding term bounds the expression (the rule reads
/// tokens, not types).
fn bounded(expression: &[Located]) -> bool {
    for (i, x) in expression.iter().enumerate() {
        let Some(word) = x.tok.ident() else { continue };
        let call = punct_at(expression, i + 1, '(');
        let after_member =
            i > 0 && (punct_at(expression, i - 1, '.') || punct_at(expression, i - 1, ':'));
        let after_dot = i > 0 && punct_at(expression, i - 1, '.');
        if (call && after_member && BOUNDING_CALLS.contains(&word))
            || (call && !after_dot && word == "reserve")
            || BOUNDING_CONSTANTS.contains(&word)
        {
            return true;
        }
    }
    expression
        .iter()
        .filter_map(|x| x.tok.ident())
        .all(constant_like)
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

/// The index of the first `collect` call after `from` in the same statement (up to the next
/// top-level `;`), if any.
fn collects_after(t: &[Located], from: usize) -> bool {
    let mut depth = 0usize;
    for (k, y) in t.iter().enumerate().skip(from) {
        match &y.tok {
            azul_srcgate::Tok::Punct('(' | '[' | '{') => depth += 1,
            azul_srcgate::Tok::Punct(')' | ']' | '}') => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            azul_srcgate::Tok::Punct(';') if depth == 0 => return false,
            azul_srcgate::Tok::Ident(name)
                if name == "collect" && k > 0 && punct_at(t, k - 1, '.') =>
            {
                return true
            }
            _ => {}
        }
    }
    false
}

/// `with_capacity(n)`, `.reserve(n)`, `reserve_exact(n)`, `try_reserve(n)`, `resize(n, v)`,
/// `resize_with(n, f)`, `vec![v; n]`, `.repeat(n)`, `repeat(..).take(n)` and `(0..n)` that are
/// collected, whose size `n` is not bounded. The free function `reserve(..)` is the crate's cap and
/// no site.
fn unchecked_allocations(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let Some(name) = x.tok.ident() else { continue };
        let previous = i.checked_sub(1).map(|k| &t[k].tok);
        let call = punct_at(t, i + 1, '(');
        let after_dot = previous.is_some_and(|p| p.is_punct('.'));
        let after_fn = previous.is_some_and(|p| p.is_ident("fn"));
        if matches!(
            name,
            "with_capacity"
                | "reserve"
                | "reserve_exact"
                | "try_reserve"
                | "resize"
                | "resize_with"
        ) && call
            && !after_fn
            && (name != "reserve" || after_dot)
        {
            let close = close_of(t, i + 1);
            let end = first_argument_end(t, i + 2, close);
            if !bounded(&t[i + 2..end.min(t.len())]) {
                found.push(x.line);
            }
        } else if name == "repeat" && call && after_dot {
            let close = close_of(t, i + 1);
            if !bounded(&t[i + 2..close.min(t.len())]) {
                found.push(x.line);
            }
        } else if name == "take" && call && after_dot {
            // `repeat(..).take(n).collect()`: n is the size of what is built.
            let from = i.saturating_sub(40);
            let chain_start = t[from..i]
                .iter()
                .rposition(|y| y.tok.is_punct(';') || y.tok.is_punct('{'))
                .map_or(from, |k| from + k + 1);
            let repeats = t[chain_start..i].iter().enumerate().any(|(k, y)| {
                (y.tok.is_ident("repeat") || y.tok.is_ident("repeat_with"))
                    && punct_at(t, chain_start + k + 1, '(')
            });
            let close = close_of(t, i + 1);
            if repeats && collects_after(t, close + 1) && !bounded(&t[i + 2..close.min(t.len())]) {
                found.push(x.line);
            }
        } else if name == "vec" && punct_at(t, i + 1, '!') && punct_at(t, i + 2, '[') {
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
        } else if name == "0"
            && punct_at(t, i + 1, '.')
            && punct_at(t, i + 2, '.')
            && i > 0
            && punct_at(t, i - 1, '(')
        {
            // `(0..n)` built into a collection: n is the size.
            let close = close_of(t, i - 1);
            let start = if punct_at(t, i + 3, '=') {
                i + 4
            } else {
                i + 3
            };
            if punct_at(t, close + 1, '.')
                && collects_after(t, close + 1)
                && !bounded(&t[start.min(close)..close.min(t.len())])
            {
                found.push(x.line);
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
            // the line X-AR4-37 was about: a VARIABLE named `len` bounds nothing
            "let v = Vec::with_capacity(usize::try_from(len).unwrap_or(0));",
            "let v = Vec::with_capacity(len);",
            "buf.resize_with(n, Default::default);",
            "let v = data.repeat(times);",
            "let v: Vec<u8> = std::iter::repeat(0u8).take(n).collect();",
            "let v: Vec<u64> = (0..count).map(|i| i * 2).collect();",
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
            // the crate's own free helper is the cap, not a site
            "let room = reserve(end - start + 1);",
            "let v: Vec<u8> = std::iter::repeat(0u8).take(16).collect();",
            "for _ in std::iter::repeat(0u8).take(n) { step(); }",
            "let v: Vec<u64> = (0..items.len()).map(|i| i * 2).collect();",
            "let total: u64 = (0..count).map(|i| i * 2).sum();",
            "for i in (0..count).rev() { step(i); }",
            "let v = data.repeat(2);",
            "let v = Vec::with_capacity(cap.min(PREALLOC));",
        ],
    )]
}

/// Where an unchecked-looking size stands, and the check that stands in front of it. TO SHRINK.
/// The numbers are what this rule's own scanner finds (made again for X-DELTA-AZDRIVE-KEYS-16:
/// the first version took any variable called `len` for a bound and counted the arguments of the
/// `reserve(..)` helper, and had never been green: 4 against 1 in azl1.rs, 1 against 2 in
/// records.rs, none for crypto/mod.rs).
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azul-storage/src/crypto/azl1.rs",
        ALLOC,
        1,
        "the writer's pending segment: segment_size is the option this process writes with, and a \
         header's is checked by check_segment_size (1 KiB to 16 MiB) before an object is read",
    ),
    Allow::exactly(
        "examples/azul-storage/src/crypto/mod.rs",
        ALLOC,
        1,
        "read_wiped_from: `room` is `more.min(expected).min(WIPED_ROOM_MAX)` plus the prefix, the \
         line above",
    ),
    Allow::exactly(
        "examples/azul-storage/src/crypto/shamir.rs",
        ALLOC,
        1,
        "combine: `len` is the length of the first share this process holds, and every other \
         share is compared with it on the lines below before the secret is made",
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
        "write_large: `count` is `large.len()` of the batch the caller handed in, two lines up",
    ),
    Allow::exactly(
        "examples/azul-storage/src/ops.rs",
        ALLOC,
        1,
        "the parallel batch: `count` is `items.len()` of the batch the caller handed in",
    ),
    Allow::exactly(
        "examples/azul-storage/src/multipart.rs",
        ALLOC,
        3,
        "in_parallel: `count` is the number of jobs the caller handed in (two sites); read_at: \
         `len` is the length of a part this process cut (PART_SIZE), not a word of the bucket",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/cache.rs",
        ALLOC,
        1,
        "rows: the columns of a row of this device's own cache database (a query this process \
         wrote), not bytes of a bucket",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/pack.rs",
        ALLOC,
        2,
        "PackIndex::open ties `count` and `objects` to the index's real length with `expected` \
         BEFORE it reserves: the pattern this gate asks for",
    ),
    Allow::exactly(
        "examples/azlin-compress/src/container.rs",
        ALLOC,
        2,
        "the encoder's pending segment: options.segment_size is checked by check_segment_size at \
         the start of an encode; the reader's segment: data_len is compared with plain_len on the \
         lines above, and plain_len with the header's segment size (check_segment_size, at most \
         16 MiB)",
    ),
    Allow::exactly(
        "examples/azlin-compress/src/segment.rs",
        ALLOC,
        1,
        "decode_segment's brotli arm: `expected` is at most DECODED_MAX (16 MiB), refused at the \
         top of the function before any codec is entered (X-DELTA-AZDRIVE-KEYS-14)",
    ),
    Allow::exactly(
        "examples/azlin-compress/src/transform/lepton.rs",
        ALLOC,
        1,
        "decode: `jpeg_len` is held against the cap of the decode (check_len with \
         limits.max_decoded_len) by decode_records before it calls here",
    ),
    Allow::exactly(
        "examples/azlin-compress/src/transform/records.rs",
        ALLOC,
        2,
        "write_records: `len` is the sum of the pieces this process holds; the PNG record's chunk \
         `count` is compared with MAX_IDAT_CHUNKS on the lines above",
    ),
    Allow::exactly(
        "examples/azlin-compress/src/transform/scan.rs",
        ALLOC,
        1,
        "parse_frame: `components` is one to four, compared on the lines above",
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
