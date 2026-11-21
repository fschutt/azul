//! A crate's module tree as the compiler walks it from its root: every `mod name;` that is not
//! a test item, followed to its file (`#[path]` too), recursively; inline `mod name { … }` stay
//! in their file. Platform and feature cfgs count as compiled: a gate reads the code of every
//! platform, not the host's.

use std::path::{Path, PathBuf};

use crate::lex::{attribute_end, lex, without_tests, Tok};

/// What a walk found: the files, and what it could not follow.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    /// The files, the root first, each module after its parent.
    pub files: Vec<PathBuf>,
    /// A `mod name;` without a file, a file that cannot be read: the gate that relies on the tree
    /// fails with these (a module it cannot read is code no gate vouches for).
    pub problems: Vec<String>,
}

/// The files a build compiles from the crate root `root` (`src/lib.rs`, `src/main.rs`); with
/// `tests` also those only a test build compiles (`#[cfg(test)] mod x;`).
#[must_use]
pub fn module_files(root: &Path, tests: bool) -> Tree {
    let mut out = Tree::default();
    walk(root, true, tests, &mut out);
    out
}

fn walk(file: &Path, owns_dir: bool, tests: bool, out: &mut Tree) {
    let src = match std::fs::read_to_string(file) {
        Ok(src) => src,
        Err(e) => {
            out.problems.push(format!("{}: {e}", file.display()));
            return;
        }
    };
    out.files.push(file.to_path_buf());
    let all = lex(&src);
    // the declarations followed: a test build's too when `tests`
    let decl_toks = if tests { all } else { without_tests(&all) };
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    // where `mod name;` looks: beside a mod.rs / lib.rs / main.rs, else in the file's own directory
    let children_dir = if owns_dir {
        dir.clone()
    } else {
        dir.join(
            file.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
        )
    };
    // `mod name;` declarations at the file's level or inside inline modules (`mod a { mod b; }`
    // looks for `a/b.rs`); the braces of any other item hide what is inside them.
    let mut decls: Vec<(PathBuf, String, Option<String>)> = Vec::new();
    let mut i = 0;
    let mut path_attr: Option<String> = None;
    // one entry per open brace: the inline module it opens, else nothing
    let mut open: Vec<Option<String>> = Vec::new();
    while i < decl_toks.len() {
        if let Some(end) = attribute_end(&decl_toks, i) {
            let a = &decl_toks[i..end];
            if a.get(2).is_some_and(|t| t.is_ident("path")) {
                path_attr = a.iter().find_map(|t| match t {
                    Tok::Str(s) => Some(s.clone()),
                    _ => None,
                });
            }
            i = end;
            continue;
        }
        let in_modules = open.iter().all(Option::is_some);
        if in_modules && decl_toks[i].is_ident("mod") {
            if let (Some(Tok::Ident(name)), Some(after)) =
                (decl_toks.get(i + 1), decl_toks.get(i + 2))
            {
                if after.is_punct(';') {
                    let mut at = children_dir.clone();
                    at.extend(open.iter().flatten());
                    decls.push((at, name.clone(), path_attr.take()));
                    i += 3;
                    continue;
                }
            }
        }
        match &decl_toks[i] {
            Tok::Punct('{') => {
                let before = |back: usize| i.checked_sub(back).and_then(|k| decl_toks.get(k));
                let module = match (before(2), before(1)) {
                    (Some(m), Some(Tok::Ident(name))) if m.is_ident("mod") => Some(name.clone()),
                    _ => None,
                };
                open.push(module);
                path_attr = None;
            }
            Tok::Punct('}') => {
                open.pop();
            }
            Tok::Punct(';') => path_attr = None,
            _ => {}
        }
        i += 1;
    }
    for (at, name, attr) in decls {
        match attr {
            Some(p) => {
                let f = dir.join(p);
                let owns = f.file_name().is_some_and(|n| n == "mod.rs");
                walk(&f, owns, tests, out);
            }
            None => {
                let flat = at.join(format!("{name}.rs"));
                if flat.exists() {
                    walk(&flat, false, tests, out);
                } else {
                    walk(&at.join(&name).join("mod.rs"), true, tests, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crate on disk: (path below its root, text).
    fn crate_of(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("azul-srcgate-tree-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, text) in files {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(full, text).unwrap();
        }
        root
    }

    fn names(tree: &Tree, root: &Path) -> Vec<String> {
        tree.files
            .iter()
            .map(|f| {
                f.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn the_tree_follows_modules_beside_and_below_their_parent_and_path_attributes() {
        let root = crate_of(
            "follow",
            &[
                ("src/lib.rs", "mod a;\nmod b;\n#[path = \"odd/c.rs\"]\nmod c;\nmod inline { mod child; fn f() { } }\nfn g() { mod not_followed; }\n#[cfg(test)]\nmod tests;\n"),
                ("src/a.rs", "mod deep;\n"),
                ("src/a/deep.rs", ""),
                ("src/b/mod.rs", "mod inner;\n"),
                ("src/b/inner.rs", ""),
                ("src/odd/c.rs", ""),
                ("src/inline/child.rs", ""),
                ("src/tests.rs", "mod only_in_tests;\n"),
                ("src/tests/only_in_tests.rs", ""),
            ],
        );
        let shipped = module_files(&root.join("src/lib.rs"), false);
        assert!(shipped.problems.is_empty(), "{:?}", shipped.problems);
        assert_eq!(
            names(&shipped, &root),
            [
                "src/lib.rs",
                "src/a.rs",
                "src/a/deep.rs",
                "src/b/mod.rs",
                "src/b/inner.rs",
                "src/odd/c.rs",
                "src/inline/child.rs",
            ],
            "an inline module's `mod child;` is a file below it, one in a function is none, a test module is not shipped"
        );
        let built = module_files(&root.join("src/lib.rs"), true);
        assert!(built.problems.is_empty(), "{:?}", built.problems);
        let built = names(&built, &root);
        assert!(built.contains(&"src/tests.rs".to_string()));
        assert!(built.contains(&"src/tests/only_in_tests.rs".to_string()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_module_without_a_file_is_a_problem_not_a_panic() {
        let root = crate_of("missing", &[("src/lib.rs", "mod gone;\n")]);
        let tree = module_files(&root.join("src/lib.rs"), false);
        assert_eq!(tree.files.len(), 1);
        assert_eq!(tree.problems.len(), 1, "{:?}", tree.problems);
        assert!(tree.problems[0].contains("mod.rs"), "{:?}", tree.problems);
        let _ = std::fs::remove_dir_all(root);
    }
}
