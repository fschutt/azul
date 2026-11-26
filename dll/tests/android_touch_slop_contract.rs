//! The Android shell applies the touch slop (`android_core::slop`) to the primary contact, pinned in source.
//!
//! The pure rule (a finger is a tap candidate until it has moved 8 dp from where it landed; the move that crosses the slop
//! makes it a pan for good; the lift of a pan is no click; a mouse and a stylus have no slop) is tested in
//! `android_core/slop_test.rs`. The shell is compiled for `target_os = "android"` only, so what it does with the answers is
//! read here:
//!
//! * a move inside the slop moves neither the pointer (the click is where the finger LANDED) nor the pan;
//! * the move that crosses it starts the pan with the displacement beyond the slop (no jump);
//! * the lift of a pan reaches the mouse as the end of an aborted gesture (no click);
//! * a mouse, a stylus and an eraser are told to the rule as contacts it does not apply to.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_touch_slop_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

/// The code of the Android shell: comments dropped, whitespace removed (rustfmt breaks the calls over lines).
fn shell() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../dll/src/desktop/shell2/android/mod.rs");
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    src.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[test]
fn a_contact_is_told_to_the_slop_and_a_mouse_or_a_stylus_has_none() {
    let code = shell();
    assert!(code.contains("window.tap.down(applies,"), "the primary contact's landing is told to the slop rule");
    assert!(
        code.contains("TouchToolType::Finger|azul_core::window::TouchToolType::Unknown"),
        "the slop applies to a finger and to a contact the platform did not name, not to a mouse, a stylus or an eraser"
    );
    assert!(code.contains("window.tap.moved("), "every move of the primary is asked of the rule");
    assert!(code.contains("window.tap.cancel()"), "a cancelled gesture leaves nothing behind");
}

/// Inside the slop the pointer stays where the finger landed and nothing pans; the move that crosses it starts the pan with
/// the displacement beyond the slop.
///
/// Mutation: the pointer moved and the pan fed at every move (the state of 0f3e1eb093: "ANY Move delta above 0.01 px
/// scrolls at once, there is NO TOUCH SLOP").
#[test]
fn a_move_inside_the_slop_moves_neither_the_pointer_nor_the_pan() {
    let code = shell();
    assert!(
        code.contains("if!matches!(slop_move,Some(SlopMove::Hold)){"),
        "the pointer follows the finger only once it is past the slop"
    );
    assert!(
        code.contains("Some(_)ifmatches!(slop_move,Some(SlopMove::Hold))=>{(None,"),
        "the pan is not fed inside the slop"
    );
    assert!(
        code.contains("Some(SlopMove::Start{delta})=>Some(delta)"),
        "the first pan delta is the displacement beyond the slop"
    );
}

/// A pan clicks nothing when it lifts: the mouse sees the end of an aborted gesture (the hit test is empty and the pointer
/// leaves), as it does for `ACTION_CANCEL`.
///
/// Mutation: the lift of a pan handed to the mouse as an `Up` (a drag that ended on the node it began on activated it).
#[test]
fn the_lift_of_a_pan_is_an_aborted_gesture_to_the_mouse() {
    let code = shell();
    assert!(
        code.contains("if matches!(window.tap.up(),SlopLift::Pan){mouse_action=MotionAction::Cancel;}".replace(' ', "").as_str()),
        "the lift of a pan must reach the mouse as a cancel"
    );
    assert!(code.contains("matchmouse_action{"), "the mouse state follows `mouse_action`, not the raw action");
    assert!(
        code.contains("mouse_effect(phase_of(mouse_action),primary_lifting)"),
        "the hit test follows `mouse_action` too (a cancel hits nothing)"
    );
}
