use azul_core::events::ProcessEventResult;
use azul_css::{
    props::basic::pixel::{OptionPixelValue, PixelValue},
    system::SafeAreaInsets,
};

use super::*;

fn px(v: f32) -> OptionPixelValue {
    OptionPixelValue::Some(PixelValue::px(v))
}

/// What `layout_subviews` did before: it rebuilt the whole struct and wrote `None` into the
/// keyboard field, because "a titlebar/desktop surface never has an on-screen keyboard over it".
/// Kept here so the test says in numbers what was wrong.
fn the_shell_before_the_fix(top: f64, left: f64, bottom: f64, right: f64) -> SafeAreaInsets {
    SafeAreaInsets {
        top: edge_inset(top),
        bottom: edge_inset(bottom),
        left: edge_inset(left),
        right: edge_inset(right),
        keyboard: OptionPixelValue::None,
    }
}

/// The keyboard's inset is written by the keyboard notification and read by the app; a layout pass
/// (UIKit runs one at every `setNeedsLayout`, and the keyboard coming up causes several) must not
/// erase it.
#[test]
fn a_layout_pass_keeps_the_keyboard_inset_the_keyboard_notification_wrote() {
    let with_keyboard = SafeAreaInsets {
        keyboard: px(291.0),
        ..SafeAreaInsets::default()
    };

    let old = the_shell_before_the_fix(59.0, 0.0, 34.0, 0.0);
    assert_eq!(
        old.keyboard,
        OptionPixelValue::None,
        "the old pass wiped the keyboard inset (this is the bug)"
    );

    let new = safe_area_after_layout(&with_keyboard, 59.0, 0.0, 34.0, 0.0);
    assert_eq!(new.keyboard, px(291.0), "the keyboard's inset survives the pass");
    assert_eq!(new.top, px(59.0), "the notch / Dynamic Island");
    assert_eq!(new.bottom, px(34.0), "the home indicator");
    assert_eq!(new.left, OptionPixelValue::None);
    assert_eq!(new.right, OptionPixelValue::None);
}

/// Landscape-left and landscape-right have the same bounds and opposite insets (the notch side),
/// so "the size did not change" cannot decide whether the layout is stale: the insets themselves
/// are compared.
#[test]
fn turning_a_phone_from_one_landscape_to_the_other_changes_the_insets_and_nothing_else() {
    let none = SafeAreaInsets::default();
    let left_notch = safe_area_after_layout(&none, 0.0, 59.0, 21.0, 0.0);
    let right_notch = safe_area_after_layout(&left_notch, 0.0, 0.0, 21.0, 59.0);
    assert_ne!(
        left_notch, right_notch,
        "the notch moved to the other side: the layout has to be asked again"
    );
    let same_again = safe_area_after_layout(&right_notch, 0.0, 0.0, 21.0, 59.0);
    assert_eq!(
        right_notch, same_again,
        "the same numbers twice are not a change: no relayout for a layout pass that moved nothing"
    );
}

/// UIKit answers 0.0 (and sub-point fractions on scaled displays) where there is nothing to keep
/// out of, and a number that is not one must not become an inset.
#[test]
fn an_inset_of_half_a_point_or_less_is_no_inset() {
    assert_eq!(edge_inset(0.0), OptionPixelValue::None);
    assert_eq!(edge_inset(0.5), OptionPixelValue::None);
    assert_eq!(edge_inset(-3.0), OptionPixelValue::None);
    assert_eq!(edge_inset(f64::NAN), OptionPixelValue::None);
    assert_eq!(edge_inset(f64::INFINITY), OptionPixelValue::None);
    assert_eq!(edge_inset(0.75), px(0.75));
    assert_eq!(edge_inset(47.0), px(47.0));
}

/// A phone in portrait, 852 points high; the keyboard's end frame comes in as the view's own
/// coordinates after the shell converted it.
#[test]
fn the_keyboard_covers_the_part_of_the_view_below_its_top_edge() {
    assert_eq!(keyboard_cover(852.0, 561.0, 852.0), 291.0, "a keyboard up");
    assert_eq!(
        keyboard_cover(852.0, 852.0, 852.0),
        0.0,
        "its top at the view's bottom edge covers nothing"
    );
    assert_eq!(
        keyboard_cover(852.0, 900.0, 852.0),
        0.0,
        "a DISMISSED keyboard is reported as a frame below the view, which is negative overlap"
    );
    assert_eq!(
        keyboard_cover(852.0, -40.0, 852.0),
        852.0,
        "a keyboard taller than the view covers the view, not more"
    );
}

/// A garbage frame (a notification without a value, an uninitialised rect) must read as "no
/// keyboard", never as NaN or a negative inset that lays the whole page out wrong - and
/// `f64::clamp` panics on a NaN bound, which the old arithmetic could reach.
#[test]
fn a_keyboard_frame_that_is_not_a_number_covers_nothing() {
    assert_eq!(keyboard_cover(f64::NAN, 100.0, 852.0), 0.0);
    assert_eq!(keyboard_cover(852.0, f64::NAN, 852.0), 0.0);
    assert_eq!(keyboard_cover(852.0, 100.0, f64::NAN), 0.0);
    assert_eq!(keyboard_cover(852.0, 100.0, -5.0), 0.0);
}

/// `None`, not `Some(0)`: the field's docs distinguish "no keyboard" from "a keyboard covering
/// nothing".
#[test]
fn a_keyboard_that_covers_nothing_is_no_inset() {
    assert_eq!(keyboard_inset(0.0), OptionPixelValue::None);
    assert_eq!(keyboard_inset(0.4), OptionPixelValue::None);
    assert_eq!(keyboard_inset(291.0), px(291.0));
}

/// A name the runtime already has (a second copy of the library in the process) is not an error:
/// the next free one is used, in order, and the plain name stays the first choice.
#[test]
fn a_class_name_that_is_taken_gives_way_to_the_next_free_one() {
    assert_eq!(class_name_attempt("AzulView", 0), "AzulView");
    assert_eq!(class_name_attempt("AzulView", 1), "AzulView_1");
    assert_eq!(class_name_attempt("AzulView", 2), "AzulView_2");

    let taken = ["AppDelegate", "AppDelegate_1"];
    let free = (0..8)
        .map(|n| class_name_attempt("AppDelegate", n))
        .find(|name| !taken.contains(&name.as_str()));
    assert_eq!(free.as_deref(), Some("AppDelegate_2"));
}

/// The unified log cuts a message at about 1 KiB; a panic's backtrace has to arrive whole, so it
/// is handed over in pieces that fit, cut on character boundaries (a multi-byte character cut in
/// the middle would be a panic inside the panic hook).
#[test]
fn a_long_message_reaches_the_system_log_in_pieces_that_fit_and_lose_nothing() {
    let text = "first line\n\nsecond line is a little longer\nthird";
    assert_eq!(
        log_pieces(text, 800),
        ["first line", "second line is a little longer", "third"],
        "lines stay whole when they fit; empty lines are dropped"
    );

    let long = "x".repeat(2000);
    let pieces = log_pieces(&long, 800);
    assert_eq!(pieces.len(), 3);
    assert!(pieces.iter().all(|p| p.len() <= 800 && !p.is_empty()));
    assert_eq!(pieces.concat(), long, "nothing lost");

    // A line made of 3-byte characters, cut at a limit that is not a multiple of 3.
    let wide = "\u{20AC}".repeat(500); // the euro sign, 3 bytes each
    let pieces = log_pieces(&wide, 800);
    assert!(pieces.iter().all(|p| p.len() <= 800));
    assert_eq!(pieces.concat(), wide, "nothing lost, no character cut in two");

    // A limit smaller than one character still makes progress.
    let pieces = log_pieces("\u{1F600}\u{1F600}", 1);
    assert_eq!(pieces.concat(), "\u{1F600}\u{1F600}");

    assert!(log_pieces("", 800).is_empty());
}

/// What `settle` and the end of `handle_touch` did before: any result but `DoNothing` became a
/// rebuild of the DOM.
fn the_step_before_the_fix(result: ProcessEventResult) -> FrameStep {
    if matches!(result, ProcessEventResult::DoNothing) {
        FrameStep::Nothing
    } else {
        FrameStep::Regenerate
    }
}

/// A finger dragging a list produces `ShouldReRenderCurrentWindow` at the touch rate; a typed or
/// dictated string produces `ShouldIncrementalRelayout`. Neither may re-run the app's `layout()`.
#[test]
fn a_scroll_or_a_text_edit_is_not_a_rebuild_of_the_dom() {
    use ProcessEventResult as R;

    assert_eq!(
        the_step_before_the_fix(R::ShouldReRenderCurrentWindow),
        FrameStep::Regenerate,
        "the old shell rebuilt the DOM for a repaint (this is the bug)"
    );
    assert_eq!(
        the_step_before_the_fix(R::ShouldIncrementalRelayout),
        FrameStep::Regenerate,
        "and for a text edit, which the app's unchanged model then overwrote"
    );

    assert_eq!(frame_step(R::DoNothing), FrameStep::Nothing);
    assert_eq!(frame_step(R::ShouldReRenderCurrentWindow), FrameStep::Repaint);
    assert_eq!(
        frame_step(R::ShouldUpdateDisplayListCurrentWindow),
        FrameStep::Repaint
    );
    assert_eq!(frame_step(R::UpdateHitTesterAndProcessAgain), FrameStep::Repaint);
    assert_eq!(
        frame_step(R::ShouldIncrementalRelayout),
        FrameStep::IncrementalRelayout
    );
    assert_eq!(
        frame_step(R::ShouldRegenerateDomCurrentWindow),
        FrameStep::Regenerate
    );
    assert_eq!(
        frame_step(R::ShouldRegenerateDomAllWindows),
        FrameStep::Regenerate
    );
}

/// The ladder is monotonic: a heavier result never calls for a lighter step than a lighter one.
#[test]
fn a_heavier_result_never_asks_for_less_than_a_lighter_one() {
    use ProcessEventResult as R;
    let ladder = [
        R::DoNothing,
        R::ShouldReRenderCurrentWindow,
        R::ShouldUpdateDisplayListCurrentWindow,
        R::UpdateHitTesterAndProcessAgain,
        R::ShouldIncrementalRelayout,
        R::ShouldRegenerateDomCurrentWindow,
        R::ShouldRegenerateDomAllWindows,
    ];
    let weight = |step: FrameStep| match step {
        FrameStep::Nothing => 0,
        FrameStep::Repaint => 1,
        FrameStep::IncrementalRelayout => 2,
        FrameStep::Regenerate => 3,
    };
    for pair in ladder.windows(2) {
        assert!(
            weight(frame_step(pair[0])) <= weight(frame_step(pair[1])),
            "{:?} -> {:?} but {:?} -> {:?}",
            pair[0],
            frame_step(pair[0]),
            pair[1],
            frame_step(pair[1])
        );
    }
}

/// The picker asked `UTType` for `png`, `pdf` and `data` with `performSelector:`: those are
/// global constants in Objective-C, not class methods (checked against the runtime: `UTType`
/// answers `typeWithIdentifier:`, `typeWithFilenameExtension:` and `typeWithMIMEType:`, and
/// nothing named `png`). The descriptors map onto the documented identifiers instead.
#[test]
fn a_file_filter_descriptor_becomes_a_uti_query_the_runtime_answers() {
    use UtiQuery::{Extension, Identifier, MimeType};

    assert_eq!(uti_query_for_descriptor("*.png"), Some(Identifier("public.png")));
    assert_eq!(uti_query_for_descriptor(".PNG"), Some(Identifier("public.png")));
    assert_eq!(uti_query_for_descriptor("jpg"), Some(Identifier("public.jpeg")));
    assert_eq!(uti_query_for_descriptor("jpeg"), Some(Identifier("public.jpeg")));
    assert_eq!(uti_query_for_descriptor("pdf"), Some(Identifier("com.adobe.pdf")));
    assert_eq!(uti_query_for_descriptor("mp4"), Some(Identifier("public.mpeg-4")));
    assert_eq!(uti_query_for_descriptor("svg"), Some(Identifier("public.svg-image")));
    assert_eq!(
        uti_query_for_descriptor("  *.Markdown "),
        Some(Extension("markdown".to_string())),
        "an extension without a well-known type is asked for by its extension"
    );
    assert_eq!(
        uti_query_for_descriptor("image/png"),
        Some(MimeType("image/png".to_string()))
    );
    assert_eq!(uti_query_for_descriptor(""), None);
    assert_eq!(uti_query_for_descriptor("*."), None);
    assert_eq!(UTI_DATA, "public.data");
    assert_eq!(UTI_FOLDER, "public.folder");
}

/// No identifier of the table may be one a selector could be built from: the old code registered
/// `"png\0"` as a selector (a NUL inside the name, which `CString` refuses: a panic).
#[test]
fn no_uti_identifier_carries_a_nul_or_a_space() {
    for ext in [
        "png", "jpg", "pdf", "gif", "txt", "json", "html", "xml", "zip", "mov", "mp4", "mp3",
        "wav", "svg",
    ] {
        let Some(UtiQuery::Identifier(id)) = uti_query_for_descriptor(ext) else {
            panic!("{ext} must be a well-known type");
        };
        assert!(!id.contains('\0') && !id.contains(' ') && id.contains('.'), "{id}");
    }
}

/// iPad multitasking tells EVERY visible app when any app's keyboard comes or goes; only the app
/// that raised it has a keyboard over its layout.
#[test]
fn another_apps_keyboard_does_not_inset_this_apps_layout() {
    assert!(keyboard_event_is_ours(Some(true)));
    assert!(!keyboard_event_is_ours(Some(false)));
    assert!(
        keyboard_event_is_ours(None),
        "no key (an older system) is the old behaviour: the keyboard is ours"
    );
}

/// What the old arithmetic did: `text[..base.min(len)]` with whatever offset UIKit sent.
fn the_slice_before_the_fix(text: &str, base: usize) -> std::thread::Result<(usize, usize)> {
    std::panic::catch_unwind(|| {
        let start = text[..base.min(text.len())]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i);
        (start, base)
    })
}

/// A position UIKit hands back after the text changed can sit inside a multi-byte character; the
/// slice there is a panic (an abort in a callback).
#[test]
fn extending_a_position_that_sits_inside_a_character_does_not_panic_and_never_inverts() {
    let text = "a\u{fc}\u{20ac}\u{1f600}b"; // 1 + 2 + 3 + 4 + 1 bytes: boundaries 0 1 3 6 10 11
    // byte 2 is inside the u-umlaut (bytes 1..3)
    assert!(
        the_slice_before_the_fix(text, 2).is_err(),
        "the old slice panicked at a byte inside a character (this is the bug)"
    );

    // Towards the start: from inside the u-umlaut the position snaps down to its start (1), and
    // the one character before that is 'a'.
    assert_eq!(one_character_around(text, 2, true), (0, 1));
    // Towards the end: from inside the u-umlaut the position snaps to 1 and the character after
    // it is the u-umlaut itself.
    assert_eq!(one_character_around(text, 2, false), (1, 3));
    // Whole characters, every width.
    assert_eq!(one_character_around(text, 3, true), (1, 3), "u-umlaut");
    assert_eq!(one_character_around(text, 6, true), (3, 6), "euro sign");
    assert_eq!(one_character_around(text, 10, true), (6, 10), "emoji");
    assert_eq!(one_character_around(text, 6, false), (6, 10), "emoji after the euro sign");
    // The ends: empty, never inverted, and an offset past the end is the end.
    assert_eq!(one_character_around(text, 0, true), (0, 0));
    assert_eq!(one_character_around(text, text.len(), false), (text.len(), text.len()));
    assert_eq!(
        one_character_around(text, text.len() + 50, false),
        (text.len(), text.len())
    );
    assert_eq!(one_character_around("", 7, true), (0, 0));
}

/// 'a', u-umlaut (1 unit, 2 bytes), euro sign (1 unit, 3 bytes), an emoji (2 units, 4 bytes), 'b':
/// six UTF-16 units, boundaries at 0 1 2 3 5 6.
const MIXED: &str = "a\u{fc}\u{20ac}\u{1f600}b";

/// What the shell did before: positions in BYTES, a step of one "character" being one byte, the
/// result snapped back to a character boundary. Past the umlaut and the euro sign the byte steps
/// (1 -> 2, 3 -> 4, 4 -> 5) land inside a character and snap back.
fn the_byte_step_before_the_fix(text: &str, base_byte: usize) -> usize {
    let target = base_byte + 1;
    let mut offset = target.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// UIKit walks a document one unit at a time (`positionFromPosition:offset:1`, then the tokenizer
/// builds words from that): the walk has to visit every character boundary exactly once and then
/// answer nil at the end.
#[test]
fn stepping_through_a_document_with_wide_characters_visits_every_boundary_once() {
    assert_eq!(utf16_len(MIXED), 6);

    let mut stuck = 1usize; // after 'a'
    for _ in 0..3 {
        stuck = the_byte_step_before_the_fix(MIXED, stuck);
    }
    assert!(
        stuck < MIXED.len(),
        "the byte model could not leave the umlaut (this is the bug): it stalls inside the text"
    );

    let mut at = 0usize;
    let mut forward = vec![at];
    while let Some(next) = position_from_offset(MIXED, at, 1) {
        forward.push(next);
        at = next;
    }
    assert_eq!(forward, [0, 1, 2, 3, 5, 6], "every boundary once, the emoji's two units in one step");
    assert_eq!(position_from_offset(MIXED, 6, 1), None, "past the end is nil");

    let mut at = 6usize;
    let mut backward = vec![at];
    while let Some(next) = position_from_offset(MIXED, at, -1) {
        backward.push(next);
        at = next;
    }
    assert_eq!(backward, [6, 5, 3, 2, 1, 0], "and back again");
    assert_eq!(position_from_offset(MIXED, 0, -1), None, "before the start is nil");
}

#[test]
fn an_offset_of_several_units_lands_on_a_boundary_in_the_direction_of_travel() {
    // 2 -> +2 = 4, inside the emoji (units 3..5): forward moves on to its end.
    assert_eq!(position_from_offset(MIXED, 2, 2), Some(5));
    // 5 -> -1 = 4, inside the emoji: backward moves to its start.
    assert_eq!(position_from_offset(MIXED, 5, -1), Some(3));
    // Zero is where it started (on a boundary).
    assert_eq!(position_from_offset(MIXED, 3, 0), Some(3));
    // The far ends and the overflow cases.
    assert_eq!(position_from_offset(MIXED, 0, 6), Some(6));
    assert_eq!(position_from_offset(MIXED, 0, 7), None);
    assert_eq!(position_from_offset(MIXED, 6, i64::MIN), None);
    assert_eq!(position_from_offset(MIXED, usize::MAX, 1), None);
    assert_eq!(position_from_offset("", 0, 0), Some(0));
    assert_eq!(position_from_offset("", 0, 1), None);
}

#[test]
fn a_unit_offset_inside_a_surrogate_pair_snaps_by_direction() {
    assert_eq!(snap_utf16_offset(MIXED, 4, true), 5);
    assert_eq!(snap_utf16_offset(MIXED, 4, false), 3);
    assert_eq!(snap_utf16_offset(MIXED, 3, true), 3, "already a boundary");
    assert_eq!(snap_utf16_offset(MIXED, 99, true), 6, "past the end is the end");
}

/// What the touch handler did before: any landing finger became primary when there was no primary.
fn the_primary_before_the_fix(current: Option<u64>, began: &[u64]) -> Option<u64> {
    if current.is_none() {
        began.first().copied()
    } else {
        current
    }
}

/// Two fingers down, the first (primary) lifts and the second stays; a third lands. The mouse
/// must not jump to the third finger.
#[test]
fn a_finger_landing_while_another_is_still_down_is_never_primary() {
    const B: u64 = 0xB;
    const C: u64 = 0xC;
    // The state after the primary lifted: no primary, one finger (B) still down; C lands.
    assert_eq!(
        the_primary_before_the_fix(None, &[C]),
        Some(C),
        "the old handler gave the mouse to the third finger (this is the bug)"
    );
    assert_eq!(primary_after_began(None, 1, &[C]), None, "the mouse stays where it was left");

    // The first finger of a group is primary, and a later one never takes over.
    assert_eq!(primary_after_began(None, 0, &[B]), Some(B));
    assert_eq!(primary_after_began(Some(B), 1, &[C]), Some(B));
    // Two fingers landing in one event: the first of them.
    assert_eq!(primary_after_began(None, 0, &[B, C]), Some(B));
    // Once every finger has lifted, the next one starts a new group.
    assert_eq!(primary_after_began(None, 0, &[C]), Some(C));
    // Nothing landed: nothing to decide.
    assert_eq!(primary_after_began(None, 0, &[]), None);
}
