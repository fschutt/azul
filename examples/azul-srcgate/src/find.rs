//! What a gate looks for in a file's code: calls, identifiers, macros, string literals - each
//! answered as the lines (from 1) where it stands, once per occurrence.
//!
//! The finders read tokens ([`crate::lex`]) of a file's [`crate::code`], so what a comment or a
//! test item says is never found and a line number is the file's. They see what is written, not
//! what it means: `use std::env::var as get; get("X")` is a call of `get`. A gate says what it
//! relies on, and its rule's planted and clean cases ([`crate::Rule`]) show it.

use crate::lex::{lex_spans, Tok};

/// A token and the line (from 1) it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub tok: Tok,
    pub line: usize,
}

/// The tokens of `text` (a file's [`crate::code`]: comments and test items already blanked) with
/// their lines.
#[must_use]
pub fn locate(text: &str) -> Vec<Located> {
    let (toks, _) = lex_spans(text);
    let mut starts = vec![0usize];
    starts.extend(
        text.bytes()
            .enumerate()
            .filter(|(_, b)| *b == b'\n')
            .map(|(i, _)| i + 1),
    );
    toks.into_iter()
        .map(|(tok, span)| Located {
            tok,
            line: starts.partition_point(|&s| s <= span.start),
        })
        .collect()
}

/// Every identifier `name`.
#[must_use]
pub fn idents(t: &[Located], name: &str) -> Vec<usize> {
    t.iter()
        .filter(|x| x.tok.is_ident(name))
        .map(|x| x.line)
        .collect()
}

/// Every string literal `is` accepts (its value, escapes resolved).
#[must_use]
pub fn strings(t: &[Located], is: impl Fn(&str) -> bool) -> Vec<usize> {
    t.iter()
        .filter(|x| matches!(&x.tok, Tok::Str(s) if is(s)))
        .map(|x| x.line)
        .collect()
}

/// Every call of the function `path` (`"env::var"`, `"var"`): its names, as written, followed by
/// `(` - `std::env::var(` and `env::var(` are calls of `env::var`, `var(` alone is not (a
/// one-name path matches the bare call only). A method call (`x.var(`) and a definition
/// (`fn var(`) are none.
#[must_use]
pub fn calls(t: &[Located], path: &str) -> Vec<usize> {
    let names: Vec<&str> = path.split("::").collect();
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if !x.tok.is_ident(names[names.len() - 1])
            || !t.get(i + 1).is_some_and(|n| n.tok.is_punct('('))
        {
            continue;
        }
        // the names before the last one, joined by `::`, right before it
        let mut at = i;
        let mut ok = true;
        for name in names[..names.len() - 1].iter().rev() {
            let colons = at >= 2 && t[at - 1].tok.is_punct(':') && t[at - 2].tok.is_punct(':');
            if !colons || at < 3 || !t[at - 3].tok.is_ident(name) {
                ok = false;
                break;
            }
            at -= 3;
        }
        if !ok {
            continue;
        }
        let before = at.checked_sub(1).map(|k| &t[k].tok);
        let is_call = if names.len() == 1 {
            // not a method, not a path's tail, not a definition
            !before.is_some_and(|b| b.is_punct('.') || b.is_punct(':') || b.is_ident("fn"))
        } else {
            !before.is_some_and(|b| b.is_ident("fn"))
        };
        if is_call {
            found.push(x.line);
        }
    }
    found
}

/// Every method call `.name(` (a turbofish, `.name::<T>(`, too).
#[must_use]
pub fn method_calls(t: &[Located], name: &str) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if !x.tok.is_ident(name) || i == 0 || !t[i - 1].tok.is_punct('.') {
            continue;
        }
        let next = t.get(i + 1).map(|n| &n.tok);
        let turbofish = next.is_some_and(|n| n.is_punct(':'))
            && t.get(i + 2).is_some_and(|n| n.tok.is_punct(':'))
            && t.get(i + 3).is_some_and(|n| n.tok.is_punct('<'));
        if next.is_some_and(|n| n.is_punct('(')) || turbofish {
            found.push(x.line);
        }
    }
    found
}

/// Every use of the macro `name` (`todo!()`, `todo![]`, `todo!{}`, also `std::todo!(`): the name,
/// `!`, and an opening delimiter - `todo != x` is none.
#[must_use]
pub fn macro_calls(t: &[Located], name: &str) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if x.tok.is_ident(name)
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('!'))
            && t.get(i + 2)
                .is_some_and(|n| n.tok.is_punct('(') || n.tok.is_punct('[') || n.tok.is_punct('{'))
        {
            found.push(x.line);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::code;

    fn toks(src: &str) -> Vec<Located> {
        locate(&code(src))
    }

    #[test]
    fn lines_are_the_files_and_comments_and_test_items_are_not_read() {
        let src = "// todo!()\nfn a() {\n    let _ = 1;\n    todo!();\n}\n#[cfg(test)]\nmod t {\n    fn b() { todo!() }\n}\nfn c() { unimplemented!(\"x\") }\n";
        let t = toks(src);
        assert_eq!(macro_calls(&t, "todo"), vec![4]);
        assert_eq!(macro_calls(&t, "unimplemented"), vec![10]);
    }

    #[test]
    fn a_macro_is_the_name_a_bang_and_an_opening_delimiter() {
        let t = toks("fn a() { todo!(); todo![]; todo!{}; let x = todo != 1; std::todo!(\"y\"); }");
        assert_eq!(macro_calls(&t, "todo").len(), 4);
    }

    #[test]
    fn a_call_is_its_path_as_written_and_a_method_or_a_definition_is_none() {
        let t = toks(
            "fn var() {}\nfn a() {\n    std::env::var(\"X\");\n    env::var(\"Y\");\n    var(\"Z\");\n    x.var(\"W\");\n    other::var(\"V\");\n}\n",
        );
        assert_eq!(calls(&t, "env::var"), vec![3, 4]);
        assert_eq!(calls(&t, "var"), vec![5], "the bare call only");
        assert_eq!(calls(&t, "std::env::var"), vec![3]);
        assert_eq!(method_calls(&t, "var"), vec![6]);
    }

    #[test]
    fn methods_are_found_with_a_turbofish_and_idents_and_strings_by_value() {
        let t = toks("fn a() {\n    p.push(x);\n    q.push::<u8>(1);\n    push(2);\n    let AZ_BACKEND = \"AZ_E2E_TEST\";\n}\n");
        assert_eq!(method_calls(&t, "push"), vec![2, 3]);
        assert_eq!(idents(&t, "AZ_BACKEND"), vec![5]);
        assert_eq!(strings(&t, |s| s == "AZ_E2E_TEST"), vec![5]);
        assert!(
            strings(&t, |s| s == "AZ_BACKEND").is_empty(),
            "an ident is no string"
        );
    }
}
