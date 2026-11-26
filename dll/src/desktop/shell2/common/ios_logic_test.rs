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
