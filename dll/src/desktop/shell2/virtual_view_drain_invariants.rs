//! The phone shells re-invoke the VirtualViews whose content arrived later BEFORE they raster a CPU frame, pinned in their source (a test
//! binary on one host cannot run them).
//!
//! The GPU path drains the queue (`pending_virtual_view_updates`, filled by `trigger_all_virtual_view_rerender`) inside `generate_frame`; the
//! CPU path has no `generate_frame`, so every shell that rasters on the CPU drains it itself, right before `render_frame` (macOS, X11, Wayland,
//! Windows and the headless shell all do). The Android and the iOS shell did not: AzMaps showed no tile on the CPU renderer of either phone (its
//! controls and its attribution drew, the map did not) while the same app drew them on the GPU path (the iOS Simulator run of 2026-11-28).

const SHELLS: &[(&str, &str)] = &[
    ("android", include_str!("android/mod.rs")),
    ("ios", include_str!("ios/mod.rs")),
];

/// The code of `source` without its line comments, so a comment about the drain is not a call.
fn code_of(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
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

/// The line numbers of every `cpu_backend.render_frame(` of `source` that no `drain_virtual_view_updates(` precedes in its function.
fn undrained_frames(source: &str) -> Vec<usize> {
    let code = code_of(source);
    let mut missing = Vec::new();
    let mut function_start = 0;
    let mut offset = 0;
    for (index, line) in code.split_inclusive('\n').enumerate() {
        if starts_a_function(line) {
            function_start = offset;
        }
        if line.contains(".render_frame(") || line.trim_start().starts_with(".render_frame(") {
            let before = &code[function_start..offset];
            let owner = code[..offset + line.find(".render_frame(").unwrap_or(0)].trim_end().ends_with("cpu_backend");
            if owner && !before.contains("drain_virtual_view_updates(") {
                missing.push(index + 1);
            }
        }
        offset += line.len();
    }
    missing
}

#[test]
fn the_scan_finds_a_frame_no_drain_precedes() {
    let source = "impl W {\n    fn paint(&mut self) {\n        self.common.prepare();\n        self.cpu_backend\n            .render_frame(a, b);\n    }\n}\n";
    assert_eq!(undrained_frames(source), vec![5]);
    let drained = "impl W {\n    fn paint(&mut self) {\n        self.common.drain_virtual_view_updates();\n        self.cpu_backend.render_frame(a, b);\n    }\n}\n";
    assert!(undrained_frames(drained).is_empty());
    let after = "impl W {\n    fn paint(&mut self) {\n        self.cpu_backend.render_frame(a, b);\n        self.common.drain_virtual_view_updates();\n    }\n}\n";
    assert_eq!(undrained_frames(after), vec![3], "a drain AFTER the frame is too late");
    let elsewhere = "impl W {\n    fn other(&mut self) {\n        self.common.drain_virtual_view_updates();\n    }\n\n    fn paint(&mut self) {\n        self.cpu_backend.render_frame(a, b);\n    }\n}\n";
    assert_eq!(undrained_frames(elsewhere), vec![7], "a drain in another function does not count");
    let commented = "impl W {\n    fn paint(&mut self) {\n        // self.common.drain_virtual_view_updates();\n        self.cpu_backend.render_frame(a, b);\n    }\n}\n";
    assert_eq!(undrained_frames(commented), vec![4], "a comment is not a call");
}

#[test]
fn every_cpu_frame_a_phone_shell_rasters_is_preceded_by_a_drain_of_the_virtual_view_updates() {
    for (shell, source) in SHELLS {
        let frames = code_of(source).matches(".render_frame(").count();
        assert!(frames > 0, "{shell}: no `render_frame(` call found - was the CPU path renamed or moved?");
        let missing = undrained_frames(source);
        assert!(
            missing.is_empty(),
            "{shell}: the CPU frame rendered at line(s) {missing:?} is not preceded by `self.common.drain_virtual_view_updates()` in its function: \
             a VirtualView whose content arrives later (a map's tiles) is never re-invoked on this shell's CPU renderer"
        );
    }
}
