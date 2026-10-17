//! Style sheets from outside the app - a book's, a mail's, a feed article's - read rule by rule
//! so an app can keep what is safe and fits it, by its own policy.
//!
//! This is the MECHANICS only (comments, rules, at-rules, blocks, declarations, the values
//! that name something to fetch or run); what to keep is the caller's: AzReader keeps a
//! book's layout and drops its colours and fonts, a mail reader keeps a mail's look but
//! scopes it to the paper. Plain Rust, no libazul: `cargo test -p azul-appkit`.

/// One item of a style sheet, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssItem<'a> {
    /// `selectors { declarations }`: the selector list as written (trimmed) and the
    /// declarations ([`parse_declarations`]).
    Rule {
        selectors: &'a str,
        declarations: Vec<(String, String)>,
    },
    /// `@name condition { body }` (`@media`, `@supports`, `@font-face`, `@page`, ...): the
    /// name in lower case without its `@`, the condition (trimmed) and the block's inside.
    AtBlock {
        name: String,
        condition: &'a str,
        body: &'a str,
    },
    /// `@name ...;` (`@import`, `@charset`, `@namespace`): the name in lower case.
    AtStatement { name: String },
}

/// `css` without its comments (`/* ... */`, an unclosed one to the end) and without the
/// `<!--` / `-->` an HTML `<style>` may wrap it in.
#[must_use]
pub fn strip_comments(css: &str) -> String {
    let mut plain = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        plain.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        rest = match after.find("*/") {
            Some(end) => &after[end + 2..],
            None => "",
        };
    }
    plain.push_str(rest);
    plain.replace("<!--", " ").replace("-->", " ")
}

/// The inside of the block that starts at `s` (its `{`) and what follows its matching `}`; an
/// unclosed block runs to the end.
#[must_use]
pub fn block_body(s: &str) -> (&str, &str) {
    let mut depth = 0_usize;
    for (at, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return (&s[1..at], &s[at + 1..]);
                }
            }
            _ => {}
        }
    }
    (s.get(1..).unwrap_or(""), "")
}

/// The items of a sheet (comments already stripped - [`strip_comments`]), in order, each
/// handed to `visit`. A statement at-rule ends at its `;`, a block at-rule at its block; text
/// after the last block that opens none is dropped.
pub fn for_each_item<'a>(css: &'a str, visit: &mut dyn FnMut(CssItem<'a>)) {
    let mut rest = css.trim_start();
    while !rest.is_empty() {
        let brace = rest.find('{');
        if let Some(at_rule) = rest.strip_prefix('@') {
            let name_len = at_rule
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .unwrap_or(at_rule.len());
            let name = at_rule[..name_len].to_ascii_lowercase();
            match (rest.find(';'), brace) {
                (Some(semi), None) => {
                    visit(CssItem::AtStatement { name });
                    rest = rest[semi + 1..].trim_start();
                }
                (Some(semi), Some(b)) if semi < b => {
                    visit(CssItem::AtStatement { name });
                    rest = rest[semi + 1..].trim_start();
                }
                (_, Some(b)) => {
                    let condition = rest[1 + name_len..b].trim();
                    let (body, after) = block_body(&rest[b..]);
                    visit(CssItem::AtBlock {
                        name,
                        condition,
                        body,
                    });
                    rest = after.trim_start();
                }
                (None, None) => break,
            }
            continue;
        }
        let Some(b) = brace else {
            break;
        };
        let selectors = rest[..b].trim();
        let (body, after) = block_body(&rest[b..]);
        visit(CssItem::Rule {
            selectors,
            declarations: parse_declarations(body),
        });
        rest = after.trim_start();
    }
}

/// The declarations of a block or a `style` attribute: `(property, value)` with the property
/// in lower case, the value trimmed, `!important` dropped; a declaration without a `:` or
/// with an empty value is left out. Semicolons inside quotes or parentheses
/// (`content: ";"`, `url(a;b)`) do not end a declaration.
#[must_use]
pub fn parse_declarations(block: &str) -> Vec<(String, String)> {
    // Split at the semicolons outside quotes and parentheses.
    let mut parts: Vec<&str> = Vec::new();
    let mut quote: Option<char> = None;
    let mut parens = 0_usize;
    let mut start = 0;
    for (at, c) in block.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => parens += 1,
            (None, ')') => parens = parens.saturating_sub(1),
            (None, ';') if parens == 0 => {
                parts.push(&block[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&block[start..]);
    parts
        .into_iter()
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let mut value = value.trim().to_string();
            if let Some(at) = value.to_ascii_lowercase().find("!important") {
                value.truncate(at);
                value = value.trim().to_string();
            }
            (!property.is_empty() && !value.is_empty()).then_some((property, value))
        })
        .collect()
}

/// Whether a value names nothing to fetch or run and cannot reach out of its declaration: no
/// `url(`, `expression`, `javascript:`, `@import`, `behavior`, `-moz-binding`, `image-set`,
/// escapes, markup, comments, `;`, braces or control characters.
#[must_use]
pub fn safe_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ![
        "url(",
        "url (",
        "expression",
        "javascript:",
        "@import",
        "\\",
        "<",
        ">",
        "/*",
        "behavior",
        "-moz-binding",
        "image-set",
        ";",
        "{",
        "}",
    ]
    .iter()
    .any(|bad| lower.contains(bad))
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(css: &str) -> Vec<CssItem<'_>> {
        let mut out = Vec::new();
        for_each_item(css, &mut |item| out.push(item));
        out
    }

    #[test]
    fn comments_and_the_html_comment_wrapping_go() {
        assert_eq!(strip_comments("a/* x */b/* open"), "ab");
        assert_eq!(strip_comments("<!-- p { } -->").trim(), "p { }");
    }

    #[test]
    fn a_block_ends_at_its_matching_brace() {
        assert_eq!(block_body("{ a { b } c } rest"), (" a { b } c ", " rest"));
        assert_eq!(block_body("{ open"), (" open", ""));
    }

    #[test]
    fn a_sheet_is_read_as_rules_and_at_rules_in_order() {
        let css = "@charset \"utf-8\"; @import url(x.css);\n\
                   h1, h2 { font-weight: bold; margin: 0 }\n\
                   @media screen and (min-width: 2px) { p { color: red } }\n\
                   @font-face { font-family: X; src: url(x.ttf) }\n\
                   p.note{text-indent:0 !important;;color:}";
        let got = items(css);
        assert_eq!(got.len(), 6, "{got:?}");
        assert_eq!(
            got[0],
            CssItem::AtStatement {
                name: "charset".into()
            }
        );
        assert_eq!(
            got[1],
            CssItem::AtStatement {
                name: "import".into()
            }
        );
        assert_eq!(
            got[2],
            CssItem::Rule {
                selectors: "h1, h2",
                declarations: vec![
                    ("font-weight".into(), "bold".into()),
                    ("margin".into(), "0".into())
                ],
            }
        );
        match &got[3] {
            CssItem::AtBlock {
                name,
                condition,
                body,
            } => {
                assert_eq!(name, "media");
                assert_eq!(*condition, "screen and (min-width: 2px)");
                assert_eq!(body.trim(), "p { color: red }");
            }
            other => panic!("not the media block: {other:?}"),
        }
        assert!(matches!(&got[4], CssItem::AtBlock { name, .. } if name == "font-face"));
        assert_eq!(
            got[5],
            CssItem::Rule {
                selectors: "p.note",
                declarations: vec![("text-indent".into(), "0".into())],
            },
            "!important dropped, an empty declaration and an empty value left out"
        );
    }

    #[test]
    fn a_semicolon_in_quotes_or_parentheses_does_not_end_a_declaration() {
        assert_eq!(
            parse_declarations("content: \"a;b\"; Font-Family: 'x;y', serif; width: calc(1px;2px)"),
            vec![
                ("content".into(), "\"a;b\"".into()),
                ("font-family".into(), "'x;y', serif".into()),
                ("width".into(), "calc(1px;2px)".into()),
            ]
        );
    }

    #[test]
    fn a_value_that_names_something_to_fetch_or_run_is_unsafe() {
        assert!(safe_value("1.5em"));
        assert!(safe_value("bold"));
        assert!(!safe_value("url(x.png)"));
        assert!(!safe_value("URL (x.png)"));
        assert!(!safe_value("expression(alert(1))"));
        assert!(!safe_value("red; color: blue"));
        assert!(!safe_value("\\41"));
        assert!(!safe_value("a\u{7}b"));
    }
}
