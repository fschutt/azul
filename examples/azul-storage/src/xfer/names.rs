//! The names a transfer makes: the ones Explorer makes ("a - Copy.txt", "a (2).txt", "New folder
//! (2)"). AzDrive's file operations re-export them; the planner uses them for the name a file
//! gets when its own is taken.

/// `archive.tar.gz` -> (`archive.tar`, `.gz`); `.env` and `README` have no extension.
#[must_use]
pub fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// `stem<suffix>ext` for the first suffix of `suffixes` whose name is free.
fn first_free(
    stem: &str,
    ext: &str,
    suffixes: impl Iterator<Item = String>,
    taken: &mut dyn FnMut(&str) -> bool,
) -> String {
    let mut last = format!("{stem}{ext}");
    for suffix in suffixes {
        let candidate = format!("{stem}{suffix}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
        last = candidate;
    }
    last
}

/// The name Explorer gives a copy pasted into the folder it came from: `a - Copy.txt`, then
/// `a - Copy (2).txt`, ... - `copy` the window's language's word ("Kopie").
#[must_use]
pub fn copy_name(name: &str, copy: &str, taken: &mut dyn FnMut(&str) -> bool) -> String {
    let (stem, ext) = split_extension(name);
    first_free(stem, ext, copy_suffixes(copy), taken)
}

/// [`copy_name`] for a folder (a dot in its name is not an extension).
#[must_use]
pub fn copy_folder_name(name: &str, copy: &str, taken: &mut dyn FnMut(&str) -> bool) -> String {
    first_free(name, "", copy_suffixes(copy), taken)
}

/// ` - Copy`, ` - Copy (2)`, ...
fn copy_suffixes(copy: &str) -> impl Iterator<Item = String> + '_ {
    std::iter::once(format!(" - {copy}"))
        .chain((2..10_000).map(move |n| format!(" - {copy} ({n})")))
}

/// The name "Keep both files" gives the newcomer: `a (2).txt`, `a (3).txt`.
#[must_use]
pub fn keep_both_name(name: &str, taken: &mut dyn FnMut(&str) -> bool) -> String {
    let (stem, ext) = split_extension(name);
    first_free(stem, ext, (2..10_000).map(|n| format!(" ({n})")), taken)
}

/// [`keep_both_name`] for a folder (a dot in its name is not an extension).
#[must_use]
pub fn keep_both_folder_name(name: &str, taken: &mut dyn FnMut(&str) -> bool) -> String {
    first_free(name, "", (2..10_000).map(|n| format!(" ({n})")), taken)
}

/// A new item's name: `name` when free, else `name (2)`, `name (3)`, ...
/// ("New folder", "New Text Document.txt").
#[must_use]
pub fn new_name(name: &str, taken: &mut dyn FnMut(&str) -> bool) -> String {
    if taken(name) {
        keep_both_name(name, taken)
    } else {
        name.to_string()
    }
}
