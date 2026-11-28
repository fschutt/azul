//! Source-text invariants of what the shells ask the window to rebuild (E76, 2026-11-28).
//!
//! `request_regeneration(RelayoutReason::RefreshDom)` runs the APP's whole `layout()` and a whole solve. The QA's cold-start
//! lab printed one of those a few milliseconds after the first picture of every start of AzWidgets, with the same picture:
//! 0.3 to 0.9 s of the engine's time on the emulator. A shell asks for a rebuild only for a reason: a callback answered
//! `Update::RefreshDom`, or state the DOM was built from changed (the language, the mode, a size that crosses a breakpoint).
//! A system event that merely says "wake up", "start polling" or "look again" is not one. The shells a test binary on one
//! host cannot run (X11 and Wayland need a display, iOS needs UIKit) are pinned here in their source, function by function;
//! the shared event pass is pinned by the behaviour tests of the headless shell
//! (`headless/tests/a_start_lays_the_page_out_once_and_no_unasked_event_lays_it_out_again.rs`).

const X11_MOD_RS: &str = include_str!("linux/x11/mod.rs");
const WAYLAND_MOD_RS: &str = include_str!("linux/wayland/mod.rs");

/// The body of the first fn declared by `decl`, from its opening brace to the matching closing one.
fn body<'a>(source: &'a str, decl: &str) -> &'a str {
    let start = source
        .find(decl)
        .unwrap_or_else(|| panic!("`{decl}` not found - was it renamed?"));
    let open = start + source[start..].find('{').expect("a fn body");
    let mut depth = 0_usize;
    for (i, c) in source[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=open + i];
                }
            }
            _ => {}
        }
    }
    &source[open..]
}

/// The code of `source` without its line comments: what a ban on a call has to look at, or a comment that explains the ban
/// would trip it.
fn code_of(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A first thread of the window (a map's tile fetch, a camera's capture, started by the AfterMount of the page's widgets)
/// is polled from the next turn of the loop on, which polls on a 16 ms tick while `layout_window.threads` is not empty.
/// The X11 and Wayland shells used to mark the DOM for regeneration "to start checking": a whole `layout()` and solve for the
/// first thread of every start that has one, right after the first picture.
///
/// Mutation: the request back in `start_thread_poll_timer` or `add_threads` of either shell.
#[test]
fn the_linux_shells_start_polling_threads_without_rebuilding_the_dom() {
    for (shell, source) in [("X11", X11_MOD_RS), ("Wayland", WAYLAND_MOD_RS)] {
        for decl in ["fn start_thread_poll_timer", "fn add_threads"] {
            let code = code_of(body(source, decl));
            assert!(
                !code.contains("request_regeneration"),
                "{shell}: `{decl}` asks for a new DOM: the first thread of an app's life lays the page out a second time"
            );
        }
    }
}

/// The loops that make that true: while a thread exists the poll wakes every 16 ms and the writebacks are drained on every
/// wake. Without them the pinned silence above would leave a thread's result unseen until some other event.
#[test]
fn the_linux_loops_poll_on_a_tick_while_a_thread_exists() {
    for (shell, source) in [("X11", X11_MOD_RS), ("Wayland", WAYLAND_MOD_RS)] {
        assert!(
            source.contains("!lw.threads.is_empty()"),
            "{shell}: the loop no longer asks whether a thread exists - `start_thread_poll_timer` may not be silent then"
        );
        assert!(
            source.contains("{ 16 }") || source.contains("else if has_threads"),
            "{shell}: the loop no longer polls on a tick while a thread exists"
        );
    }
}
