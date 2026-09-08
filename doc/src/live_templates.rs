//! Templates and stylesheets are `include_str!`-compiled into the binary so a
//! production deploy is self-contained and reproducible from one artifact.
//! While iterating on the site's texts that is the wrong trade: every edit to
//! `doc/templates/index.template.html` cost a five-minute rebuild of azul-doc
//! before a 35-second deploy could show it.
//!
//! In a `deploy debug` run the deploy switches this module to *live* mode,
//! and every site that used to read the compiled-in copy asks [`get`]
//! instead, which returns the current file under `doc/templates/` when it
//! can be read and the compiled-in copy otherwise. Production deploys never
//! enable live mode, so CI output is unaffected.

use std::{
    borrow::Cow,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

static LIVE: AtomicBool = AtomicBool::new(false);

/// `doc/templates`, resolved at compile time from the crate manifest.
const TEMPLATE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/templates");

/// Switch to reading templates from disk (debug deploys only).
pub fn enable() {
    LIVE.store(true, Ordering::Relaxed);
}

/// True once [`enable`] has run.
pub fn is_live() -> bool {
    LIVE.load(Ordering::Relaxed)
}

/// The template `name` (a file name under `doc/templates/`): the on-disk
/// file in live mode, the compiled-in `compiled` copy otherwise. A read error
/// in live mode is reported once per call and falls back to `compiled`, so a
/// renamed or missing file degrades to the compiled behaviour instead of
/// aborting the deploy.
pub fn get(name: &str, compiled: &'static str) -> Cow<'static, str> {
    if is_live() {
        let path = Path::new(TEMPLATE_DIR).join(name);
        match std::fs::read_to_string(&path) {
            Ok(s) => return Cow::Owned(s),
            Err(e) => eprintln!(
                "[live-templates] cannot read {}: {e} - using the compiled-in copy",
                path.display()
            ),
        }
    }
    Cow::Borrowed(compiled)
}

/// Concatenation of several templates (the page-family stylesheets are
/// served as one `<style>` block), each resolved through [`get`].
pub fn join(parts: &[(&str, &'static str)]) -> String {
    let mut out = String::new();
    for (name, compiled) in parts {
        out.push_str(&get(name, compiled));
    }
    out
}
