//! A frame a shell rasters on the CPU (or WebRender draws on the GPU) is put on its window's frame
//! report, pinned in the source of the shells, which a test binary on one host cannot run (the phone
//! shells, and the X11, Wayland and Windows ones: ledger E45; the macOS and headless shells have their
//! own tests).
//!
//! The e2e counters read the report, not the backend: `assert_idle_stable`, `assert_work_bounded`
//! and `assert_state_machines_idle` fail with "NO FRAME was rendered since the last reset" when
//! `frames_since_reset` is 0, and a shell that never called `CpuBackend::record_last_frame_on` after
//! its `render_frame` left it at 0 for the life of the window: `mount_damage_smoke` was red on the
//! Android emulator with hundreds of frames rendered, the same file that passes on the desktop.

const SHELLS: &[(&str, &str)] = &[
    ("android", include_str!("android/mod.rs")),
    ("ios", include_str!("ios/mod.rs")),
    ("x11", include_str!("linux/x11/mod.rs")),
    ("wayland", include_str!("linux/wayland/mod.rs")),
    ("windows", include_str!("windows/mod.rs")),
];

/// The shells whose WebRender frames are put on the window's frame report too (the phone shells' GPU
/// paths are the GPU agents').
const GL_SHELLS: &[(&str, &str)] = &[
    ("x11", include_str!("linux/x11/mod.rs")),
    ("wayland", include_str!("linux/wayland/mod.rs")),
    ("windows", include_str!("windows/mod.rs")),
];

/// The code of `source` without its line comments, so a comment about `render_frame` is not a call.
fn code_of(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Is the text before `at` (whitespace skipped) `owner`: `... cpu_backend` + `.`?
fn follows(code: &str, at: usize, owner: &str) -> bool {
    code[..at].trim_end().ends_with(owner)
}

/// Does this line start a function at the margin of an item (no indentation) or of a method (four spaces)?
fn starts_a_function(line: &str) -> bool {
    let margin = line.len() - line.trim_start_matches(' ').len();
    if margin != 0 && margin != 4 {
        return false;
    }
    ["fn ", "pub fn ", "pub(crate) fn ", "unsafe fn ", "pub unsafe fn "]
        .iter()
        .any(|start| line[margin..].starts_with(start))
}

/// Where the function that holds `from` ends: the start of the next function after it.
fn end_of_the_function(code: &str, from: usize) -> usize {
    let mut offset = from;
    let mut lines = code[from..].split_inclusive('\n');
    if let Some(first) = lines.next() {
        offset += first.len();
    }
    for line in lines {
        if starts_a_function(line) {
            return offset;
        }
        offset += line.len();
    }
    code.len()
}

/// Every `owner.call(` of `source` that no `record(` follows in its function: the line numbers of those calls.
fn unrecorded(source: &str, owner: &str, call: &str, record: &str) -> Vec<usize> {
    let code = code_of(source);
    let mut missing = Vec::new();
    let mut search = 0;
    while let Some(found) = code[search..].find(call) {
        let at = search + found;
        search = at + call.len();
        if !follows(&code, at, owner) {
            continue;
        }
        let end = end_of_the_function(&code, at);
        if !code[at..end].contains(record) {
            missing.push(code[..at].matches('\n').count() + 1);
        }
    }
    missing
}

/// Every `cpu_backend.render_frame(` of `source` that no `record_last_frame_on(` follows in its function:
/// the line numbers of those calls.
fn unrecorded_frames(source: &str) -> Vec<usize> {
    unrecorded(source, "cpu_backend", ".render_frame(", "record_last_frame_on(")
}

/// Every `renderer.render(` (a WebRender frame) of `source` that no `.record_frame(` follows in its
/// function: the line numbers of those calls.
fn unrecorded_gpu_frames(source: &str) -> Vec<usize> {
    unrecorded(source, "renderer", ".render(", ".record_frame(")
}

#[test]
fn the_scan_finds_a_frame_nothing_recorded() {
    let source = "impl W {\n    fn paint(&mut self) {\n        self.cpu_backend\n            .render_frame(a, b);\n    }\n\n    fn other(&mut self) {\n        self.cpu_backend.record_last_frame_on(lw);\n    }\n}\n";
    assert_eq!(unrecorded_frames(source), vec![4], "the call in `paint` has no record after it in its function");
    let recorded = "impl W {\n    fn paint(&mut self) {\n        self.cpu_backend.render_frame(a, b);\n        self.cpu_backend.record_last_frame_on(lw);\n    }\n}\n";
    assert!(unrecorded_frames(recorded).is_empty());
    let commented = "impl W {\n    fn paint(&mut self) {\n        // so the next cpu_backend.render_frame(a) can blit\n    }\n}\n";
    assert!(unrecorded_frames(commented).is_empty(), "a comment is not a call");
}

#[test]
fn every_cpu_frame_a_shell_rasters_is_put_on_the_windows_frame_report() {
    for (shell, source) in SHELLS {
        let calls = code_of(source).matches(".render_frame(").count();
        assert!(calls > 0, "{shell}: no `render_frame(` call found - was the CPU path renamed or moved?");
        let missing = unrecorded_frames(source);
        assert!(
            missing.is_empty(),
            "{shell}: the CPU frame rendered at line(s) {missing:?} is not put on the window's frame report \
             (`self.cpu_backend.record_last_frame_on(layout_window)` after the call): every e2e assertion \
             that needs 'a frame was rendered since the last reset' fails on this shell for the life of the window"
        );
    }
}

#[test]
fn the_scan_finds_a_webrender_frame_nothing_recorded() {
    let source = "impl W {\n    fn present(&mut self) {\n        match renderer.render(size, 0) {\n            Ok(r) => {}\n        }\n    }\n}\n";
    assert_eq!(unrecorded_gpu_frames(source), vec![3]);
    let split = "impl W {\n    fn present(&mut self) {\n        let results = renderer\n            .render(size, 0)?;\n    }\n}\n";
    assert_eq!(unrecorded_gpu_frames(split), vec![4], "a call on its own line after `renderer` is found");
    let recorded = "impl W {\n    fn present(&mut self) {\n        let r = renderer.render(size, 0);\n        lw.record_frame(a, b);\n    }\n}\n";
    assert!(unrecorded_gpu_frames(recorded).is_empty());
    let other = "impl W {\n    fn present(&mut self) {\n        template.render(size, 0);\n    }\n}\n";
    assert!(unrecorded_gpu_frames(other).is_empty(), "only the renderer's render is a frame");
}

#[test]
fn every_webrender_frame_a_desktop_shell_draws_is_put_on_the_windows_frame_report() {
    for (shell, source) in GL_SHELLS {
        let calls = code_of(source).matches(".render(").count();
        assert!(calls > 0, "{shell}: no `.render(` call found - was the GPU path renamed or moved?");
        let missing = unrecorded_gpu_frames(source);
        assert!(
            missing.is_empty(),
            "{shell}: the WebRender frame drawn at line(s) {missing:?} is not put on the window's frame report \
             (`layout_window.record_frame(damage, damage)` after the call, with \
             `FrameDamage::of_gpu_frame`): every e2e assertion that needs 'a frame was rendered since the last \
             reset' fails on this shell for the life of the window"
        );
    }
}
