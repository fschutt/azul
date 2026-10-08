//! A hundred and twenty nested blocks lay out on a two-megabyte thread.
//!
//! Block layout recurses once per nesting level - `calculate_layout_for_subtree`
//! -> `calculate_layout_for_subtree_fragment` -> `layout_formatting_context` ->
//! `layout_bfc` -> the next level - and in a debug build every local of those
//! functions keeps its own stack slot. `layout_bfc` held both of its layout
//! passes, margin collapsing, floats, fragmentation and the content height in
//! one frame of ~26 KiB; a whole level took ~36 KiB, and a chain of 61 divs
//! overflowed the 2 MiB threads the test harness runs on (`managers::a11y`'s
//! deep-chain test had to move to an 8 MiB thread). A page that deep is not
//! exotic: quoted mail replies nest a few levels per reply.
//!
//! The chain is laid out in a CHILD PROCESS of this test binary: a stack
//! overflow aborts the whole process, and would take every other test of the
//! shared binary with it. Debug builds only - release frames are a few KiB.

#![cfg(debug_assertions)]

use azul_core::{
    dom::Dom, geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const DEPTH: usize = 120;
const STACK: usize = 2 << 20;
/// Set in the child process: lay the chain out instead of spawning a child.
const CHILD: &str = "AZ_DEEP_BLOCK_CHAIN_CHILD";

/// `DEPTH` nested divs with a line of text in the innermost one, styled and
/// laid out the way a window lays out its DOM.
fn lay_out_the_chain() {
    let mut chain = Dom::create_div().with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper("the innermost block"),
    );
    for _ in 0..DEPTH {
        chain = Dom::create_div().with_child(chain);
    }
    let styled = StyledDom::create_from_dom(Dom::create_body().with_child(chain));
    eprintln!("deep chain: styled");
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the chain of divs lays out");
    let result = lw
        .get_layout_result(&azul_core::dom::DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    assert!(
        result.layout_tree.nodes.len() > DEPTH,
        "every div has a box: {} boxes",
        result.layout_tree.nodes.len()
    );
    eprintln!("deep chain: laid out");
}

/// This test's libtest name: relative to the crate root, so drop the leading
/// crate segment of `module_path!()` (`all::`).
fn this_test() -> String {
    let module = module_path!();
    let relative = module.split_once("::").map_or(module, |(_crate, rest)| rest);
    format!("{relative}::a_hundred_and_twenty_nested_blocks_lay_out_on_a_two_megabyte_thread")
}

#[test]
fn a_hundred_and_twenty_nested_blocks_lay_out_on_a_two_megabyte_thread() {
    if std::env::var_os(CHILD).is_some() {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn(lay_out_the_chain)
            .expect("a thread for the chain")
            .join()
            .expect("the chain lays out");
        return;
    }
    let name = this_test();
    let out = std::process::Command::new(std::env::current_exe().expect("this test binary"))
        .arg(&name)
        .arg("--exact")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env(CHILD, "1")
        .output()
        .expect("spawn this test binary");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("running 1 test"),
        "the child ran no test - `--exact {name}` matched nothing.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        out.status.success(),
        "{DEPTH} nested blocks did not lay out on a {} MiB thread{} (exit {:?}).\nstderr:\n{stderr}",
        STACK >> 20,
        if stderr.contains("overflowed its stack") {
            ": the layout overflowed the stack"
        } else {
            ""
        },
        out.status
    );
}
