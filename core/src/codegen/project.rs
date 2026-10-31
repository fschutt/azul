//! The project API: ONE function family that combines the two generators -
//! the DOM ([`super::dom`]: markup and component instances → render
//! functions and apps) and the CSS (`azul_css::codegen`'s `emit_css`: a
//! stylesheet → named styles) - on top of the component API: a component
//! library is one file of render functions plus the library's registration
//! (`register_<library>_library`, where the printer can spell it).
//!
//! - [`project_files`]: a whole project for one language ("Export > Code",
//!   the zip): the app (build file, the module with the app's markup and
//!   every component it calls, a `main`), one file per component library,
//!   the app's stylesheet as named styles, a README.
//! - [`fragment_code`]: markup as one render function or an app project
//!   (AzBuilder's "Subtree → code").
//! - [`library_code`]: components as one file with their registration
//!   ("Component → code").
//! - [`html_code`]: pasted HTML / XHTML as code ("HTML → DOM (code)").
//!
//! Every function answers an unknown language with the list of the known
//! ones, and a language whose printer does not build a DOM with its reason
//! (a warning; the printed module says why, item by item).

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write as _;

use azul_css::codegen::{
    ir::{Ident, Module},
    CodegenBackend, GeneratedFile,
};

use super::{
    backend,
    dom::{
        default_fn_name, lower_component_library, lower_components_app,
        lower_components_fragment, markup_parts, Components,
    },
    dom_warning, render_fn_name,
};
use crate::xml::{ComponentDef, XmlNodeChild};

/// What an export answers: the code a dialog shows first and its file name,
/// every file when the answer is a project (else empty), and warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeExport {
    /// The language id (`rust`, `c`, `cpp`, `python`, ...).
    pub language: String,
    /// The file name of `code` (for a download).
    pub file_name: String,
    /// The code shown first.
    pub code: String,
    /// Every file, when the answer is a project (an app, or code with its
    /// styles); else empty.
    pub files: Vec<GeneratedFile>,
    pub warnings: Vec<String>,
}

/// What a DOM export produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeMode {
    /// One render function (and the component functions it calls).
    Function,
    /// A complete program that shows the markup in a window.
    App,
}

impl CodeMode {
    /// `function` (default) or `app`.
    ///
    /// # Errors
    /// Anything else.
    pub fn parse(s: Option<&str>) -> Result<Self, String> {
        match s.unwrap_or("function") {
            "function" | "fn" => Ok(Self::Function),
            "app" | "program" => Ok(Self::App),
            other => Err(format!("unknown mode {other:?}; use function or app")),
        }
    }
}

/// The markup a project's app shows.
#[derive(Debug, Clone, PartialEq)]
pub enum AppMarkup {
    /// A fragment or a `<body>` with the CSS it uses (the builder's document,
    /// a subtree).
    Fragment {
        nodes: Vec<XmlNodeChild>,
        css: String,
    },
    /// A whole page: `<html>` with `<head><style>` and a `<body>` (the live
    /// page, a pasted document); every `<style>` block is its stylesheet.
    Page(Vec<XmlNodeChild>),
}

impl AppMarkup {
    /// The app's markup and stylesheet.
    fn parts(&self) -> Result<(Vec<XmlNodeChild>, String), String> {
        match self {
            Self::Fragment { nodes, css } => Ok((nodes.clone(), css.clone())),
            Self::Page(nodes) => {
                let (roots, css, _) = markup_parts(nodes)?;
                Ok((roots, css))
            }
        }
    }
}

/// What [`project_files`] writes.
pub struct ProjectSpec<'a> {
    /// The window title.
    pub title: &'a str,
    pub app: AppMarkup,
    /// The components the app and the libraries use (map + templates).
    pub components: &'a Components<'a>,
    /// The component libraries written as files of their own (their names
    /// in `components.map`).
    pub libraries: Vec<String>,
}

/// `nodes` (markup; `css` its stylesheet) as code: a render function
/// `fn_name` or an app titled `title`, component instances as calls of their
/// functions.
///
/// # Errors
/// An unknown language.
pub fn fragment_code(
    nodes: &[XmlNodeChild],
    css: &str,
    language: &str,
    mode: CodeMode,
    fn_name: &str,
    title: &str,
    components: &Components<'_>,
) -> Result<CodeExport, String> {
    let b = backend(language)?;
    let warnings: Vec<String> = dom_warning(&*b).into_iter().collect();
    Ok(match mode {
        CodeMode::Function => {
            let m = lower_components_fragment(nodes, css, fn_name, Vec::new(), components);
            CodeExport {
                language: b.lang().to_string(),
                file_name: format!("{}.{}", Ident::from_text(fn_name).snake(), b.extension()),
                code: b.emit_module(&m),
                files: Vec::new(),
                warnings,
            }
        }
        CodeMode::App => {
            let m = lower_components_app(nodes, css, title, components);
            project_answer(&*b, app_files(&*b, &m), warnings)
        }
    })
}

/// Pasted HTML / XHTML as code ("HTML → DOM (code)"): a document (`<html>`,
/// a root `<body>` / `<head>`) is its `<body>`, a fragment is itself; every
/// `<style>` block is the stylesheet (each node's `style` attribute its
/// own). `fn_name` (default: `render_ui` for a document, else named after
/// the single root like a subtree export). `with_styles`: the stylesheet
/// also as named styles in a file of its own (the CSS generator).
///
/// # Errors
/// An unknown language; a document without a body.
pub fn html_code(
    root_nodes: &[XmlNodeChild],
    language: &str,
    mode: CodeMode,
    fn_name: Option<&str>,
    with_styles: bool,
    components: &Components<'_>,
) -> Result<CodeExport, String> {
    let b = backend(language)?;
    let (roots, css, is_page) = markup_parts(root_nodes)?;
    let name = fn_name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map_or_else(
            || {
                if is_page {
                    render_fn_name("ui")
                } else {
                    default_fn_name(&roots)
                }
            },
            str::to_string,
        );
    let mut out = fragment_code(&roots, &css, b.lang(), mode, &name, "Azul app", components)?;
    if with_styles {
        if let Some(styles) = styles_file(&*b, &css, mode == CodeMode::App) {
            if out.files.is_empty() {
                out.files.push(GeneratedFile {
                    path: out.file_name.clone(),
                    contents: out.code.clone(),
                });
            } else if b.lang() == "rust" {
                declare_rust_module(&mut out.files, "styles");
            }
            out.files.push(styles);
        }
    }
    Ok(out)
}

/// Components `defs` of library `library` as one source file: each
/// component's render function once (with the functions of the components
/// it uses) and the library's registration where the printer spells it.
///
/// # Errors
/// An unknown language.
pub fn library_code(
    library: &str,
    defs: &[&ComponentDef],
    language: &str,
    components: &Components<'_>,
) -> Result<CodeExport, String> {
    let b = backend(language)?;
    let mut warnings: Vec<String> = dom_warning(&*b).into_iter().collect();
    let version = components
        .map
        .libraries
        .iter()
        .find(|l| l.name.as_str() == library)
        .map_or_else(|| "0.1.0".to_string(), |l| l.version.as_str().to_string());
    let m = lower_component_library(library, &version, defs, components, &mut warnings);
    Ok(CodeExport {
        language: b.lang().to_string(),
        file_name: format!("{}.{}", Ident::from_text(library).snake(), b.extension()),
        code: b.emit_module(&m),
        files: Vec::new(),
        warnings,
    })
}

/// Everything a project for `language` holds: the app (build file, the
/// module with the app's markup and every component it calls, a `main`),
/// each library of `spec.libraries` as a file of its own (Rust:
/// `src/components/<lib>.rs` + `mod.rs`; else `components/<lib>.<ext>`), the
/// app's stylesheet as named styles (Rust: `src/styles.rs`; else
/// `styles.<ext>`) and a README with the build steps. The warnings: a
/// language without DOM export, a component without a template, a library
/// that does not exist.
///
/// # Errors
/// An unknown language; a page without a body.
pub fn project_files(
    language: &str,
    spec: &ProjectSpec<'_>,
) -> Result<(Vec<GeneratedFile>, Vec<String>), String> {
    let b = backend(language)?;
    let mut warnings: Vec<String> = dom_warning(&*b).into_iter().collect();
    let (nodes, css) = spec.app.parts()?;
    let m = lower_components_app(&nodes, &css, spec.title, spec.components);
    let mut files = app_files(&*b, &m);

    let mut component_paths: Vec<String> = Vec::new();
    let mut rust_mods: Vec<String> = Vec::new();
    for name in &spec.libraries {
        let Some(lib) = spec
            .components
            .map
            .libraries
            .iter()
            .find(|l| l.name.as_str() == name.as_str())
        else {
            warnings.push(format!("component library '{name}' not found: left out"));
            continue;
        };
        let defs: Vec<&ComponentDef> = lib.components.iter().collect();
        if defs.is_empty() {
            continue;
        }
        let code = library_code(name, &defs, b.lang(), spec.components)?;
        for w in code.warnings {
            if !warnings.contains(&w) {
                warnings.push(w);
            }
        }
        let sn = Ident::from_text(name).snake();
        let path = if b.lang() == "rust" {
            rust_mods.push(sn.clone());
            format!("src/components/{sn}.rs")
        } else {
            format!("components/{sn}.{}", b.extension())
        };
        component_paths.push(path.clone());
        files.push(GeneratedFile {
            path,
            contents: code.code,
        });
    }
    if !rust_mods.is_empty() {
        let mut m = String::new();
        for sn in &rust_mods {
            let _ = writeln!(m, "pub mod {sn};");
        }
        files.push(GeneratedFile {
            path: "src/components/mod.rs".to_string(),
            contents: m,
        });
        declare_rust_module(&mut files, "components");
    }

    let mut styles_path = None;
    if let Some(styles) = styles_file(&*b, &css, true) {
        if b.lang() == "rust" {
            declare_rust_module(&mut files, "styles");
        }
        styles_path = Some(styles.path.clone());
        files.push(styles);
    }

    let listed: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    files.push(GeneratedFile {
        path: "README.md".to_string(),
        contents: readme(&*b, &listed, &component_paths, styles_path.as_deref()),
    });
    Ok((files, warnings))
}

/// The files of an app module ([`Module::app`]): the printer's project
/// (build file, the module, a `main` that opens the window). A printer that
/// does not do DOM export has no app to write: its module alone (which says
/// why, item by item), not the CSS harness `emit_project_files` would make.
#[must_use]
pub fn app_files(b: &dyn CodegenBackend, m: &Module) -> Vec<GeneratedFile> {
    if b.exports_dom() {
        b.emit_project_files(m)
    } else {
        alloc::vec![GeneratedFile {
            path: format!("ui.{}", b.extension()),
            contents: b.emit_module(m),
        }]
    }
}

/// An answer that is a project: the entry point shown first.
fn project_answer(
    b: &dyn CodegenBackend,
    files: Vec<GeneratedFile>,
    warnings: Vec<String>,
) -> CodeExport {
    let first = main_file(&files).cloned().unwrap_or(GeneratedFile {
        path: String::new(),
        contents: String::new(),
    });
    CodeExport {
        language: b.lang().to_string(),
        file_name: first.path.rsplit('/').next().unwrap_or("").to_string(),
        code: first.contents,
        files,
        warnings,
    }
}

/// The project file a dialog shows first: the entry point.
fn main_file(files: &[GeneratedFile]) -> Option<&GeneratedFile> {
    files
        .iter()
        .find(|f| {
            let name = f.path.rsplit('/').next().unwrap_or(&f.path);
            name.starts_with("main.") || name.starts_with("Main.") || name.starts_with("app.")
        })
        .or_else(|| files.first())
}

/// `css` as named styles (`emit_css`) in the file a project keeps them in:
/// Rust `src/styles.rs` in a project, else `styles.<ext>`. `None` for a
/// stylesheet without rules.
fn styles_file(b: &dyn CodegenBackend, css: &str, in_project: bool) -> Option<GeneratedFile> {
    if css.trim().is_empty() {
        return None;
    }
    let (parsed, _warnings) = azul_css::parser2::new_from_str(css);
    if parsed.rules.as_ref().is_empty() {
        return None;
    }
    let path = if in_project && b.lang() == "rust" {
        "src/styles.rs".to_string()
    } else {
        format!("styles.{}", b.extension())
    };
    Some(GeneratedFile {
        path,
        contents: b.emit_css(&parsed),
    })
}

/// Declare module `name` in a Rust project's `src/main.rs` (its items may
/// be unused by the app: `#[allow(dead_code)]`).
fn declare_rust_module(files: &mut [GeneratedFile], name: &str) {
    if let Some(main) = files.iter_mut().find(|f| f.path == "src/main.rs") {
        let _ = write!(main.contents, "\n#[allow(dead_code)]\nmod {name};\n");
    }
}

fn readme(
    b: &dyn CodegenBackend,
    files: &[String],
    component_paths: &[String],
    styles_path: Option<&str>,
) -> String {
    let build = match b.lang() {
        "rust" => "cargo run --release\n# the bindings link against libazul: export \
                   AZ_LINK_PATH=<the directory holding libazul>"
            .to_string(),
        "c" | "cpp" => {
            "make AZUL_INCLUDE=<azul>/target/codegen AZUL_LIB=<azul>/target/release\n./app"
                .to_string()
        }
        "python" => "python3 main.py   # needs the azul extension module next to it".to_string(),
        // The printer's project files carry their build steps as header comments.
        _ if b.exports_dom() => format!(
            "# see the comments at the top of the {} build file and main file above",
            b.display_name()
        ),
        _ => format!(
            "# see the {} files above; the {} printer does not write a runnable app yet",
            b.display_name(),
            b.display_name()
        ),
    };
    let comps = if component_paths.is_empty() {
        String::new()
    } else {
        format!(
            "\nThe component libraries are in {}: each `render_*` function builds one \
             component; where the language can spell it, `register_<library>_library` \
             registers them for XML / AzBuilder.\n",
            component_paths
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let styles = styles_path.map_or_else(String::new, |p| {
        format!(
            "\nThe app's stylesheet as named styles (one function per selector, conditions as \
             `apply_if`) is in `{p}`; the app itself carries each node's CSS inline.\n"
        )
    });
    format!(
        "# AzBuilder export ({})\n\nFiles: {}.\n{comps}{styles}\n## Build and run\n\n```sh\n{build}\n```\n\n\
         `<azul>` is an azul checkout with the bindings generated (`cargo run --release -p \
         azul-doc -- codegen all`) and libazul built (`cargo build --release -p azul-dll \
         --features build-dll`).\n",
        b.display_name(),
        files
            .iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(", "),
    )
}
