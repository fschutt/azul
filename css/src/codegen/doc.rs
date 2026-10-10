//! A tiny layout model shared by the printers.
//!
//! Printers build a [`Doc`] tree; [`render`] lays it out. The layout rule is
//! deliberately width-independent so the output is predictable (and golden
//! files are reviewable by hand): a [`Doc::List`] is either flat
//! (`open item, item close`) or `broken` (one item per line, indented one
//! level). The printers break a list exactly when the IR node is "tall"
//! (see [`super::lang::is_tall`]): it is a non-empty `Vec` or contains one.

use alloc::{string::String, vec::Vec};

/// A layout tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doc {
    /// Literal text (must not contain a newline unless it is a comment the
    /// printer wants verbatim).
    Text(String),
    /// Concatenation without break points of its own.
    Cat(Vec<Doc>),
    /// A delimited, separated list.
    List {
        open: String,
        items: Vec<Doc>,
        /// Separator in flat mode (`", "`); broken mode uses it trimmed.
        sep: String,
        close: String,
        /// Flat mode pads inside the delimiters: `{ a, b }` vs `(a, b)`.
        pad: bool,
        /// One item per line.
        broken: bool,
        /// Broken mode also puts the separator after the last item.
        trailing: bool,
    },
    /// A method chain (`head.a(..).b(..)`): flat, or `broken` with every
    /// link on a line of its own, one level deeper than the head.
    Chain {
        head: alloc::boxed::Box<Doc>,
        links: Vec<Doc>,
        broken: bool,
    },
}

impl Doc {
    #[must_use]
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    #[must_use]
    pub fn cat(parts: Vec<Self>) -> Self {
        Self::Cat(parts)
    }

    /// `name(args)` - a call whose argument list breaks when `broken`.
    #[must_use]
    pub fn call(name: impl Into<String>, args: Vec<Self>, broken: bool) -> Self {
        Self::Cat(alloc::vec![
            Self::Text(name.into()),
            Self::list("(", args, ", ", ")", false, broken, false),
        ])
    }

    #[must_use]
    pub fn list(
        open: &str,
        items: Vec<Self>,
        sep: &str,
        close: &str,
        pad: bool,
        broken: bool,
        trailing: bool,
    ) -> Self {
        Self::List {
            open: open.into(),
            items,
            sep: sep.into(),
            close: close.into(),
            pad,
            broken,
            trailing,
        }
    }

    /// `recv` followed by the chained call `link` (`.with_child(x)`): a new
    /// chain, or `recv`'s chain one link longer. A chain of two or more
    /// links puts each on a line of its own.
    #[must_use]
    pub fn chained(recv: Self, link: Self) -> Self {
        match recv {
            Self::Chain {
                head, mut links, ..
            } => {
                links.push(link);
                let broken = links.len() >= 2;
                Self::Chain {
                    head,
                    links,
                    broken,
                }
            }
            other => Self::Chain {
                head: alloc::boxed::Box::new(other),
                links: alloc::vec![link],
                broken: false,
            },
        }
    }

    /// `true` if this renders on more than one line (a broken list or
    /// chain, at the top or inside).
    #[must_use]
    pub fn is_multiline(&self) -> bool {
        match self {
            Self::Text(s) => s.contains('\n'),
            Self::Cat(ps) => ps.iter().any(Self::is_multiline),
            Self::List { items, broken, .. } => {
                (*broken && !items.is_empty()) || items.iter().any(Self::is_multiline)
            }
            Self::Chain {
                head,
                links,
                broken,
            } => *broken || head.is_multiline() || links.iter().any(Self::is_multiline),
        }
    }

    /// Render flat (ignores `broken`).
    #[must_use]
    pub fn flat(&self) -> String {
        let mut out = String::new();
        flat_into(self, &mut out);
        out
    }
}

fn flat_into(d: &Doc, out: &mut String) {
    match d {
        Doc::Text(s) => out.push_str(s),
        Doc::Cat(ps) => {
            for p in ps {
                flat_into(p, out);
            }
        }
        Doc::List {
            open,
            items,
            sep,
            close,
            pad,
            ..
        } => {
            out.push_str(open);
            if *pad && !items.is_empty() {
                out.push(' ');
            }
            for (i, it) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(sep);
                }
                flat_into(it, out);
            }
            if *pad && !items.is_empty() {
                out.push(' ');
            }
            out.push_str(close);
        }
        Doc::Chain { head, links, .. } => {
            flat_into(head, out);
            for l in links {
                flat_into(l, out);
            }
        }
    }
}

/// Lay `doc` out, starting at nesting `level` (continuation lines are
/// indented with `indent` repeated `level + 1` times).
#[must_use]
pub fn render(doc: &Doc, indent: &str, level: usize) -> String {
    let mut out = String::new();
    render_into(doc, indent, level, &mut out);
    out
}

fn render_into(d: &Doc, indent: &str, level: usize, out: &mut String) {
    match d {
        Doc::Text(s) => out.push_str(s),
        Doc::Cat(ps) => {
            for p in ps {
                render_into(p, indent, level, out);
            }
        }
        Doc::List {
            open,
            items,
            sep,
            close,
            broken,
            trailing,
            ..
        } => {
            if !*broken || items.is_empty() {
                flat_into(d, out);
                return;
            }
            out.push_str(open);
            let s = sep.trim_end();
            for (i, it) in items.iter().enumerate() {
                out.push('\n');
                for _ in 0..=level {
                    out.push_str(indent);
                }
                render_into(it, indent, level + 1, out);
                if i + 1 < items.len() || *trailing {
                    out.push_str(s);
                }
            }
            out.push('\n');
            for _ in 0..level {
                out.push_str(indent);
            }
            out.push_str(close);
        }
        Doc::Chain {
            head,
            links,
            broken,
        } => {
            render_into(head, indent, level, out);
            for l in links {
                if *broken {
                    out.push('\n');
                    for _ in 0..=level {
                        out.push_str(indent);
                    }
                    render_into(l, indent, level + 1, out);
                } else {
                    render_into(l, indent, level, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn a_broken_list_puts_one_item_per_line_and_flat_children_stay_flat() {
        let inner = Doc::call("f", vec![Doc::text("1"), Doc::text("2")], false);
        let outer = Doc::list("[", vec![inner.clone(), inner], ", ", "]", false, true, true);
        assert_eq!(render(&outer, "    ", 0), "[\n    f(1, 2),\n    f(1, 2),\n]");
        let flat = Doc::list("{", vec![Doc::text("a"), Doc::text("b")], ", ", "}", true, false, false);
        assert_eq!(render(&flat, "    ", 0), "{ a, b }");
    }
}
