//! Generate the "agentic release" bundle: artefacts that let a coding agent
//! (Claude Code, Cursor, etc.) write high-quality azul applications.
//!
//! Three (well, four) files are emitted at the site root:
//!
//! * `llms.txt`      — the emerging llms.txt convention: a concise markdown
//!                     index of the docs with absolute links.
//! * `llms-full.txt` — the full concatenation of every guide page, for
//!                     "dump everything into context" workflows.
//! * `skill.md`      — a Claude Code Agent Skill (with valid frontmatter)
//!                     that teaches an agent how to build azul apps and how
//!                     to use the API reference as a search tool.
//! * `.well-known/azul-skill.md` — a copy of `skill.md` at a discoverable path.
//!
//! All of these are built programmatically from `guide::get_guide_list()` and
//! the `ApiData` accessors so they stay in sync with the rest of the site.

use super::{guide, HTML_ROOT};
use crate::api::ApiData;

/// Logical ordering for the long-form / index pages. Pages not listed here
/// are appended afterwards in their natural guide order, so adding a new
/// guide page never drops it from the bundle.
const PREFERRED_ORDER: &[&str] = &[
    "architecture",
    "dom",
    "hello-world",
    "callbacks",
    "widgets",
    "styling",
    "layout",
    "text-input",
    "images",
    "accessibility",
    "clipboard",
    "file-dialogs",
    "networking",
    "background-tasks",
    "realtime-media",
    "routing",
    "windowing",
    "web-deployment",
    "mobile",
    "headless-rendering",
    "e2e-testing",
    "reference",
];

/// Return guide pages sorted by `PREFERRED_ORDER`, with anything not listed
/// appended afterwards (stable, in `get_guide_list()` order).
fn ordered_guides() -> Vec<guide::Guide> {
    let mut guides = guide::get_guide_list();
    guides.sort_by_key(|g| {
        PREFERRED_ORDER
            .iter()
            .position(|p| *p == g.file_name)
            .unwrap_or(usize::MAX)
    });
    guides
}

/// `llms.txt` — concise markdown index for LLMs.
pub fn generate_llms_txt(api_data: &ApiData) -> String {
    let latest = api_data.get_latest_version_str().unwrap_or("0.0.0");
    let mut out = String::new();

    out.push_str("# azul\n\n");
    out.push_str(
        "> azul is a cross-platform, MIT-licensed GUI framework for building native desktop \
         (and, soon, web) applications. It pairs a retained DOM/CSS UI model with an \
         immediate-mode-style developer experience: a `LayoutCallback` maps your application \
         data to a `Dom`, CSS styles it, and event callbacks mutate the data and request a \
         re-layout. The whole API is exposed over a stable `repr(C)` ABI, so the same concepts \
         are available from Rust (native) plus 10+ generated language bindings \
         (C, C++, Python, and more). It ships headless rendering and a JSON-driven E2E test \
         runner for windowless verification.\n\n",
    );

    out.push_str("## Guide (concept pages, raw markdown)\n\n");
    for g in ordered_guides() {
        // Skip internals/contributor pages from the headline index — they live
        // under the dedicated section below.
        if g.file_name.starts_with("internals/") {
            continue;
        }
        let desc = g
            .description
            .as_deref()
            .map(|d| format!(": {}", d))
            .unwrap_or_default();
        out.push_str(&format!(
            "- [{}]({}/guide/{}.md){}\n",
            g.title, HTML_ROOT, g.file_name, desc
        ));
    }
    out.push('\n');

    out.push_str("## Internals (contributor docs)\n\n");
    for g in ordered_guides() {
        if !g.file_name.starts_with("internals/") {
            continue;
        }
        out.push_str(&format!(
            "- [{}]({}/guide/{}.md)\n",
            g.title, HTML_ROOT, g.file_name
        ));
    }
    out.push('\n');

    out.push_str("## API reference\n\n");
    out.push_str(&format!(
        "- [API reference (HTML)]({}/api.html): full rendered reference for the latest version\n",
        HTML_ROOT
    ));
    out.push_str(&format!(
        "- [Version manifest]({}/api/index.json): JSON `{{ \"latest\": \"{}\", \"versions\": [...] }}`\n",
        HTML_ROOT, latest
    ));
    out.push_str(&format!(
        "- [Per-version reference]({}/api/{}.html)\n",
        HTML_ROOT, latest
    ));
    out.push_str(&format!(
        "- [Search index (JSON)]({}/api/{}.search.json): compact searchable index over the \
         whole API. Each entry has short keys — `k` kind (`m` module, `s` struct, `e` enum, \
         `fp` fnptr, `ev` enum variant, `f` struct field, `fn` method, `cn` constructor), \
         `n` name, `m` module, `p` parent class, `a` anchor, `d` doc text, `s` signature.\n\n",
        HTML_ROOT, latest
    ));

    out.push_str("## Hello world (per language)\n\n");
    for lang in ["rust", "c", "cpp", "python"] {
        out.push_str(&format!(
            "- [{} hello world]({}/guide/hello-world/{}.md)\n",
            lang, HTML_ROOT, lang
        ));
    }
    out.push('\n');

    out.push_str("## Agent skill\n\n");
    out.push_str(&format!(
        "- [azul-gui skill file]({}/skill.md): install once to make a coding agent ready to \
         build azul apps; also at {}/.well-known/azul-skill.md\n",
        HTML_ROOT, HTML_ROOT
    ));
    out.push_str(&format!("- [llms-full.txt]({}/llms-full.txt): every guide page concatenated\n", HTML_ROOT));

    out
}

/// `llms-full.txt` — full concatenation of every guide page.
pub fn generate_llms_full_txt() -> String {
    let mut out = String::new();
    out.push_str("# azul — full documentation\n\n");
    out.push_str(
        "This file concatenates every azul guide page in teaching order. It is meant to be \
         pasted wholesale into an LLM context window. For the structured index see llms.txt.\n\n",
    );

    for g in ordered_guides() {
        out.push_str(&format!("\n\n# {}\n\n", g.title));
        out.push_str(g.content.trim());
        out.push('\n');
    }

    out
}

/// `skill.md` — a Claude Code Agent Skill.
pub fn generate_skill_md(api_data: &ApiData) -> String {
    let latest = api_data.get_latest_version_str().unwrap_or("0.0.0");
    let mut s = String::new();

    s.push_str("---\n");
    s.push_str("name: azul-gui\n");
    s.push_str(
        "description: Build native desktop (and web) GUI applications with the azul framework \
         in Rust, C, C++, Python, and 10+ other language bindings — DOM/CSS UI, callbacks, \
         widgets, headless + E2E testing.\n",
    );
    s.push_str("---\n\n");

    s.push_str("# Building GUI applications with azul\n\n");
    s.push_str(&format!(
        "azul is a cross-platform, MIT-licensed GUI framework for native desktop apps (web \
         support is in progress). Full guide: {HTML_ROOT}/guide.html — API reference: \
         {HTML_ROOT}/api.html (latest version is `{latest}`).\n\n"
    ));

    // --- What azul is + architecture ----------------------------------------
    s.push_str("## What azul is\n\n");
    s.push_str(
        "azul pairs a **retained DOM/CSS UI** with an **immediate-mode-style** developer \
         experience. You never hand-mutate widgets. Instead:\n\n\
         - Your application state lives in a single plain struct, type-erased into a `RefAny` \
           (a refcounted, runtime-checked `Box<dyn Any>`-like handle).\n\
         - A **`LayoutCallback`** maps that data to a `Dom` tree. It runs on startup and again \
           whenever a callback asks for a refresh.\n\
         - **CSS** (inline or stylesheet, with `:hover`, `:focus`, `@media`, and `@os(...)` \
           queries) styles the DOM and drives the layout solver.\n\
         - **Event callbacks** receive the `RefAny`, downcast it to your struct, mutate it, and \
           return an `Update` telling the framework whether to do nothing or rebuild the DOM.\n\n\
         The entire public API is a stable `repr(C)` ABI. Native Rust callbacks are still \
         `extern \"C\"`. The 10+ language bindings are generated from a single `api.json`, so \
         the *concepts* below map one-to-one across C, C++, Python, etc. — only syntax differs.\n\n\
         Depth: see "
    );
    s.push_str(&format!(
        "{HTML_ROOT}/guide/architecture.md, {HTML_ROOT}/guide/dom.md, and \
         {HTML_ROOT}/guide/internals/dom.md.\n\n"
    ));

    // --- Mental model + hello world -----------------------------------------
    s.push_str("## Mental model: a minimal Rust counter app\n\n");
    s.push_str(
        "`App` owns the data + config; `WindowCreateOptions::create(layout_fn)` wires the \
         layout callback; callbacks return `Update`. Minimal, correct against the current API:\n\n",
    );
    s.push_str("```rust\n");
    s.push_str(MINIMAL_RUST_HELLO_WORLD);
    s.push_str("```\n\n");
    s.push_str(&format!(
        "Notes that bite: every callback is `extern \"C\"`; `downcast_ref`/`downcast_mut` are \
         runtime-checked and may fail (return `Update::DoNothing`); `data.clone()` bumps a \
         refcount, it does not deep-copy; `with_css` is the consuming builder form of \
         `set_css`. Full walk-through: {HTML_ROOT}/guide/hello-world/rust.md.\n\n"
    ));

    // --- Feature checklist ---------------------------------------------------
    s.push_str("## Feature set\n\n");
    s.push_str(
        "- DOM construction + CSS layout (flexbox-like solver, `:hover`/`:focus`/`@media`/`@os`)\n\
         - Built-in widgets (Button, TextInput, CheckBox, DropDown, lists, scroll regions, ...)\n\
         - OpenGL custom rendering surfaces\n\
         - Images and SVG\n\
         - Text input + text selection + IME\n\
         - Accessibility (screen-reader tree)\n\
         - Clipboard and native file dialogs\n\
         - Networking + background tasks (threads / timers) for async work\n\
         - Real-time media\n\
         - Routing (swap the layout callback, SPA-style)\n\
         - Headless rendering + JSON-driven E2E testing\n\
         - Web deployment (in progress) and mobile deployment\n\n",
    );

    // --- API reference as a tool --------------------------------------------
    s.push_str("## Using the API reference as a search tool\n\n");
    s.push_str(
        "Treat the published reference as your API search backend via WebFetch:\n\n\
         1. Fetch the version manifest to learn the latest version:\n",
    );
    s.push_str(&format!("   `{HTML_ROOT}/api/index.json` → `{{ \"latest\": \"{latest}\", \"versions\": [...] }}`\n"));
    s.push_str(&format!(
        "2. Fetch the compact search index for that version and query it locally:\n   \
         `{HTML_ROOT}/api/<version>.search.json` (e.g. `{HTML_ROOT}/api/{latest}.search.json`).\n"
    ));
    s.push_str(
        "   Schema — top level `{ \"v\": version, \"e\": [entries] }`. Each entry uses short keys:\n\n\
         | key | meaning |\n\
         |-----|---------|\n\
         | `k` | kind: `m` module, `s` struct, `e` enum, `fp` fnptr, `ev` enum variant, `f` struct field, `fn` method, `cn` constructor |\n\
         | `n` | the entity's own name |\n\
         | `m` | module it lives in |\n\
         | `p` | parent class (for variants/fields/methods/constructors) |\n\
         | `a` | anchor fragment in the api page (no leading `#`) |\n\
         | `d` | plain-text doc body |\n\
         | `s` | signature line (fns/constructors/fields/callbacks) |\n\n\
         To answer \"what methods does `Dom` have?\" filter for `k == \"fn\" && p == \"Dom\"` and \
         read each `s`. To find a type, match `n` against `k in (s, e, fp)`.\n",
    );
    s.push_str(&format!(
        "3. For prose + full rendering, fetch `{HTML_ROOT}/api/<version>.html` and deep-link \
         using the entry `a` anchor (`#<a>`).\n\n"
    ));

    // --- E2E + headless ------------------------------------------------------
    s.push_str("## Verification: headless rendering + E2E testing\n\n");
    s.push_str(
        "Always verify your app builds and behaves before declaring done — and you can do it \
         windowless:\n\n\
         - **Headless render**: run with `AZ_BACKEND=headless` to execute the layout/render \
           pipeline without opening a window (CI-friendly, lets you assert the DOM/layout was \
           produced).\n\
         - **E2E test runner**: set `AZ_E2E=<path-to-json>` (or pass the JSON inline) to drive \
           the app through a scripted sequence of synthetic events and state assertions. The \
           test JSON schema lives in `tests/e2e/*.json` in the repo.\n\
         - Assert application state by checking your data model after the scripted events, and \
           assert the rendered tree via the headless display list.\n\n",
    );
    s.push_str(&format!(
        "Details: {HTML_ROOT}/guide/e2e-testing.md and {HTML_ROOT}/guide/headless-rendering.md.\n\n"
    ));

    // --- Per-language --------------------------------------------------------
    s.push_str("## Other languages\n\n");
    s.push_str(
        "Rust is first-class, but the bindings are all generated from the same `api.json`, so \
         the App / RefAny / LayoutCallback / Dom / CSS / Update model is identical everywhere — \
         only the syntax changes. Start from the per-language hello world:\n\n",
    );
    for lang in ["rust", "c", "cpp", "python"] {
        s.push_str(&format!("- {HTML_ROOT}/guide/hello-world/{lang}.md\n"));
    }
    s.push_str(&format!(
        "\nFor the full document dump (all guides concatenated) fetch {HTML_ROOT}/llms-full.txt; \
         for a structured index fetch {HTML_ROOT}/llms.txt.\n"
    ));

    s
}

/// Minimal, current-API counter hello-world (derived from
/// `doc/guide/en/hello-world/rust.md`). Kept inline so the skill is
/// self-contained even when the agent can't fetch the guide.
const MINIMAL_RUST_HELLO_WORLD: &str = r#"use azul::prelude::*;
use azul::widgets::Button;

// Your application state: a single plain struct.
struct DataModel {
    counter: usize,
}

// Maps DataModel -> Dom. Runs on startup and on Update::RefreshDom.
extern "C"
fn my_layout_func(data: RefAny, _: LayoutCallbackInfo) -> Dom {
    // Runtime-checked downcast back to the concrete struct.
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return Dom::create_body(),
    };

    let label = Dom::create_p_with_text(counter.as_str())
        .with_css("font-size: 50px");

    let mut button = Button::create("Update counter");
    // clone() bumps the RefAny refcount (thread-safe); it is NOT a deep copy.
    button.set_on_click(data.clone(), my_on_click);
    let button = button.dom().with_css("flex-grow: 1");

    Dom::create_body()
        .with_child(label)
        .with_child(button)
}

extern "C"
fn my_on_click(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    data.counter += 1;
    Update::RefreshDom // queue a relayout
}

fn main() {
    let data = DataModel { counter: 0 };
    let app_config = AppConfig::create();
    let window = WindowCreateOptions::create(my_layout_func);
    let app = App::create(RefAny::new(data), app_config);
    app.run(window);
}
"#;
