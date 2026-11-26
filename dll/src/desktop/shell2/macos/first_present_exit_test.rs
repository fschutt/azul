//! `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER` ends a macOS app after its first frame. The check used to stand at the end of
//! `render_and_present_in_draw_rect`, which the CPU view's frame path leaves earlier (the startup timeline's harness
//! found the CPU window still running after its first present, 2026-11-26), so a CPU window never exited. The
//! check is at the first present as well. A window cannot be made in a unit test, so the source says it.

const SOURCE: &str = include_str!("mod.rs");

/// The text of the function that starts with `signature`, up to the closing brace at its own indentation.
fn function_text(signature: &str) -> &'static str {
    let start = SOURCE
        .find(signature)
        .unwrap_or_else(|| panic!("`{signature}` is not in the macOS shell"));
    let rest = &SOURCE[start..];
    let line_start = SOURCE[..start].rfind('\n').map_or(0, |at| at + 1);
    let line = &SOURCE[line_start..];
    let indent = &line[..line.len() - line.trim_start().len()];
    let indent = indent.split('\n').next().unwrap_or("");
    let closing = format!("\n{indent}}}\n");
    let end = rest.find(&closing).expect("the function ends at a closing brace of its own indentation");
    &rest[..end]
}

#[test]
fn the_first_present_exits_when_asked() {
    let reveal = function_text("fn reveal_after_first_frame(");
    assert!(
        reveal.contains("exit_after_first_frame_if_asked()"),
        "the reveal of the first frame must run the exit check: a CPU window leaves the frame function before its end"
    );
    let mark_at = reveal.find("self.mark_first_present(").expect("the reveal marks the first present");
    let exit_at = reveal.find("exit_after_first_frame_if_asked()").expect("checked above");
    assert!(mark_at < exit_at, "the timeline's table is printed before the process ends");
}

#[test]
fn one_function_ends_the_process() {
    let check = function_text("fn exit_after_first_frame_if_asked(");
    assert!(check.contains("exit_after_first_frame"), "it reads the setting");
    assert!(check.contains("std::process::exit(0)"), "and ends the process with success");
    assert_eq!(
        SOURCE.matches("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting").count(),
        1,
        "the exit lives in one place; the two callers use it"
    );
}
