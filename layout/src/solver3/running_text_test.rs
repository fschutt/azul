//! What the page furniture asks of the font system: which stacks, for which
//! characters; what it says when it cannot draw; and what a setter makes of
//! text it is given directly. The drawing of a whole document is pinned through
//! `layout_document_paged_v2` (`paged_layout_test.rs`).

use azul_css::{props::basic::FontRef, AzString};
use rust_fontconfig::FcFontCache;

use super::*;
use crate::{
    solver3::pagination::{OptionPageSetup, PageMargins, PageSequence, PageSetup},
    text3::{
        cache::{ShapedCluster, ShapedItem},
        default::PathLoader,
    },
};

fn style(family: Option<&str>, size: f32) -> HeaderFooterConfig {
    let config = HeaderFooterConfig {
        font_size: size,
        ..HeaderFooterConfig::with_page_numbers()
    };
    match family {
        Some(family) => config.with_font_family(family),
        None => config,
    }
}

fn chars_of(content: &MarginBoxContent) -> String {
    let mut chars = BTreeSet::new();
    collect_chars(content, &mut chars);
    chars.into_iter().collect()
}

/// A blank family is no family: it is the document's default font, as unset.
#[test]
fn a_blank_family_is_the_default_font_as_none_is() {
    let unset = FaceKey::of(&style(None, 10.0));
    assert_eq!(unset.family, None);
    assert_eq!(FaceKey::of(&style(Some(""), 10.0)), unset);
    assert_eq!(FaceKey::of(&style(Some(" \t "), 10.0)), unset);
    assert_eq!(
        FaceKey::of(&style(Some("  Georgia, serif "), 10.0)).family.as_deref(),
        Some("Georgia, serif")
    );
}

/// The same family at two sizes are two stacks: the optical size of a variable
/// face is the size, and the chain's key carries it.
#[test]
fn a_family_at_two_sizes_is_two_faces() {
    let a = FaceKey::of(&style(Some("serif"), 10.0));
    let b = FaceKey::of(&style(Some("serif"), 12.0));
    assert_ne!(a, b);
    assert!((a.size_px() - 10.0).abs() < f32::EPSILON);
    assert!((b.size_px() - 12.0).abs() < f32::EPSILON);
}

/// The characters a chain must cover: what the text says and what its page
/// numbers can say, in the letters of the number format.
#[test]
fn the_characters_of_a_header_and_its_page_numbers_are_collected() {
    let text = chars_of(&MarginBoxContent::text("Bericht Größe"));
    assert!(text.contains('ö') && text.contains('ß'), "{text}");

    let decimal = chars_of(&MarginBoxContent::PageCounter);
    assert!(decimal.contains('0') && decimal.contains('9') && decimal.contains('?'), "{decimal}");

    let greek = chars_of(&MarginBoxContent::PageCounterFormatted(CounterFormat::LowerGreek));
    assert!(greek.contains('α') && greek.contains('ω'), "every Greek letter: {greek}");

    let roman = chars_of(&MarginBoxContent::PageCounterFormatted(CounterFormat::UpperRoman));
    for numeral in ['I', 'V', 'X', 'L', 'C', 'D', 'M'] {
        assert!(roman.contains(numeral), "{numeral} in {roman}");
    }

    let combined = chars_of(&MarginBoxContent::combined(vec![
        MarginBoxContent::text("Seite "),
        MarginBoxContent::PageCounter,
        MarginBoxContent::text(" von "),
        MarginBoxContent::PagesCounter,
    ]));
    assert!(combined.contains('S') && combined.contains('v') && combined.contains('5'), "{combined}");
}

/// What a callback says is not known until it is called, and it is called
/// once per page: never to find out which characters to cover.
#[test]
fn a_callback_is_not_called_to_collect_characters() {
    extern "C" fn never_called(
        _data: &mut azul_core::refany::RefAny,
        _info: crate::solver3::pagination::PageInfo,
    ) -> AzString {
        panic!("a callback ran while the fonts were being prepared");
    }
    let content = MarginBoxContent::custom(never_called, azul_core::refany::RefAny::new(0_u8));
    assert_eq!(chars_of(&content), "");
}

/// A setup that shows neither a header nor a footer asks for no font; one that
/// shows either asks for the font of its family, and every setup of a page
/// sequence counts: the first page's, an override's, the parity ones.
#[test]
fn every_setup_of_a_page_sequence_asks_for_its_family() {
    let setup = |header_footer: HeaderFooterConfig| PageSetup {
        header_footer,
        page_size: azul_core::geom::LogicalSize::new(600.0, 400.0),
        margins: PageMargins::default(),
    };
    let mut sequence = PageSequence::uniform(setup(style(Some("Default Family"), 10.0)));
    sequence.first_page = OptionPageSetup::Some(setup(style(Some("First Family"), 10.0)));
    sequence.even_pages = OptionPageSetup::Some(setup(style(Some("Even Family"), 11.0)));
    sequence.set_override(7, setup(style(Some("Override Family"), 10.0)));
    sequence.odd_pages = OptionPageSetup::Some(setup(HeaderFooterConfig::default()));

    let mut slicer = SlicerConfig::simple(100.0);
    slicer.page_sequence = Some(sequence);
    let wanted = wanted_faces(&slicer);
    let families: Vec<(Option<&str>, f32)> = wanted
        .keys()
        .map(|key| (key.family.as_deref(), key.size_px()))
        .collect();
    assert_eq!(
        families,
        [
            (Some("Default Family"), 10.0),
            (Some("Even Family"), 11.0),
            (Some("First Family"), 10.0),
            (Some("Override Family"), 10.0),
        ],
        "the odd pages' setup shows nothing and asks for nothing"
    );
    // A uniform config reads the slicer's own.
    let mut uniform = SlicerConfig::simple(100.0);
    uniform.header_footer = style(Some("Only Family"), 9.0);
    assert_eq!(wanted_faces(&uniform).len(), 1);
    assert!(wanted_faces(&SlicerConfig::simple(100.0)).is_empty(), "no header, no footer");
}

const VIEWPORT: LogicalSize = LogicalSize {
    width: 600.0,
    height: 400.0,
};

/// A font manager that holds the faces azul bundles (the mock fonts) and no
/// system font.
fn font_manager() -> FontManager<FontRef> {
    FontManager::<FontRef>::new(FcFontCache::default()).expect("a font manager")
}

/// The slicer of a document whose pages carry the header and footer of `style`.
fn slicer_of(style: &HeaderFooterConfig) -> SlicerConfig {
    let mut slicer = SlicerConfig::simple(100.0);
    slicer.header_footer = style.clone();
    slicer
}

/// The band of a header: 600px wide, 30px high.
fn band() -> LogicalRect {
    LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 30.0))
}

/// The [`RunningText`] of a document whose pages carry the header and footer of
/// `prepared_for`: the fonts of that setup are resolved and loaded in
/// `font_manager`, and what is said on the way goes to `messages`.
fn running_text<'a>(
    prepared_for: &HeaderFooterConfig,
    font_manager: &'a mut FontManager<FontRef>,
    messages: &'a mut Option<Vec<LayoutDebugMessage>>,
) -> RunningText<'a, FontRef> {
    let loader = PathLoader::new();
    RunningText::new(
        &slicer_of(prepared_for),
        &StyledDom::default(),
        VIEWPORT,
        font_manager,
        &|bytes, index| loader.load_font_shared(bytes, index),
        messages,
    )
}

/// What a layout was told about the page furniture: the findings among the
/// messages it collected (the text layout writes its own trace into the same
/// list).
fn told(messages: &Option<Vec<LayoutDebugMessage>>) -> Vec<String> {
    messages
        .iter()
        .flatten()
        .map(|message| message.message.as_str())
        .filter(|message| message.starts_with("[azul][paginate]"))
        .map(str::to_owned)
        .collect()
}

/// Holds the shared diagnostics ring for one test and starts it empty: a test
/// that makes the page furniture report, or reads what the process was told.
fn quiet_diagnostics() -> std::sync::MutexGuard<'static, ()> {
    let lock = azul_core::diagnostics::test_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    azul_core::diagnostics::clear();
    lock
}

/// `layout` with the glyphs of the clusters `losing` picks stripped of their face
/// (hash 0), as the glyphs of a face that never resolved are.
fn without_a_face(layout: &UnifiedLayout, losing: impl Fn(&ShapedCluster) -> bool) -> UnifiedLayout {
    let mut layout = layout.clone();
    for positioned in &mut layout.items {
        if let ShapedItem::Cluster(cluster) = &mut positioned.item {
            if losing(cluster) {
                for glyph in &mut cluster.glyphs {
                    glyph.font_hash = 0;
                }
            }
        }
    }
    layout
}

/// Blank text is nothing to draw and nothing to say: shaped, it is an empty
/// layout, and an empty layout would be taken for a family without a font.
#[test]
fn blank_text_is_neither_drawn_nor_reported() {
    let config = style(Some("Azul Mock Wide"), 10.0);
    let mut manager = font_manager();
    let mut messages = Some(Vec::new());
    let mut running = running_text(&config, &mut manager, &mut messages);

    for blank in [" ", "   ", "\t\n", "\u{a0}"] {
        assert!(
            running
                .set_text(blank, band(), &config, TextAlignment::Center)
                .is_empty(),
            "{blank:?}"
        );
    }
    assert!(
        !running
            .set_text("Report", band(), &config, TextAlignment::Center)
            .is_empty(),
        "the same setter draws text"
    );
    drop(running);

    assert_eq!(told(&messages), Vec::<String>::new());
}

/// A size that is no size - zero, negative, not a number, infinite - draws
/// nothing and asks for no face: at infinity every coordinate of the text would
/// be NaN.
#[test]
fn a_size_that_is_no_size_draws_nothing_and_asks_for_no_face() {
    for size in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -2.0] {
        let config = style(Some("Azul Mock Wide"), size);
        assert!(
            wanted_faces(&slicer_of(&config)).is_empty(),
            "size {size}: no face is wanted"
        );

        let mut manager = font_manager();
        let mut messages = Some(Vec::new());
        let mut running = running_text(&config, &mut manager, &mut messages);
        assert!(
            running
                .set_text("Report", band(), &config, TextAlignment::Center)
                .is_empty(),
            "size {size}"
        );
        drop(running);
        assert_eq!(told(&messages), Vec::<String>::new(), "size {size}");
    }
}

/// The slicer, not this module, picks the setup of each page. A setup it picks
/// that `wanted_faces` did not see has no face: that is said (once), and not a
/// header that silently vanishes.
#[test]
fn a_style_no_face_was_prepared_for_is_said_once() {
    let _lock = quiet_diagnostics();
    let prepared = style(Some("Azul Mock Wide"), 10.0);
    let asked = style(Some("Azul Mock Mono"), 10.0);
    let mut manager = font_manager();
    let mut messages = Some(Vec::new());
    let mut running = running_text(&prepared, &mut manager, &mut messages);

    assert!(
        !running
            .set_text("Report", band(), &prepared, TextAlignment::Center)
            .is_empty(),
        "the prepared style is drawn"
    );
    for _ in 0..2 {
        assert!(running
            .set_text("Report", band(), &asked, TextAlignment::Center)
            .is_empty());
    }
    drop(running);

    let said = told(&messages);
    assert_eq!(said.len(), 1, "said once: {said:?}");
    assert!(
        said[0].contains("no font was resolved")
            && said[0].contains("Azul Mock Mono")
            && said[0].contains("\"Report\""),
        "{said:?}"
    );
}

/// Text part of which has no face to be drawn with is drawn in part, and the
/// layout says it: it names the family and the text, and how much of it is gone.
#[test]
fn text_part_of_which_has_no_face_is_drawn_in_part_and_said() {
    let _lock = quiet_diagnostics();
    let config = style(Some("Azul Mock Wide"), 10.0);
    let mut manager = font_manager();
    let mut messages = Some(Vec::new());
    let mut running = running_text(&config, &mut manager, &mut messages);
    let stack = running.faces[0].1.clone();
    let shaped = running
        .shape("Report 12", &stack, &config, 600.0, TextAlignment::Center)
        .expect("the text is shaped");
    // The digits lose their face: their glyphs carry no hash.
    let layout = without_a_face(&shaped, |cluster| {
        let text = cluster.text();
        !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
    });

    let items = running.finish(
        Arc::new(layout),
        &stack,
        &config,
        band(),
        "Report 12",
        "Azul Mock Wide",
    );
    drop(running);

    let runs = items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::Text { .. }))
        .count();
    assert_eq!(runs, 1, "the letters are drawn, the digits are not");
    assert!(
        items
            .iter()
            .any(|item| matches!(item, DisplayListItem::TextLayout { .. })),
        "and the shaped layout is there for a PDF writer"
    );
    let said = told(&messages);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("part of the page text")
            && said[0].contains("\"Report 12\"")
            && said[0].contains("Azul Mock Wide")
            && said[0].contains("1 of its 2 runs"),
        "{said:?}"
    );
}

/// Text none of which has a face draws nothing, and says so as the whole of it
/// (not as part).
#[test]
fn text_none_of_which_has_a_face_is_said_as_not_drawn() {
    let _lock = quiet_diagnostics();
    let config = style(Some("Azul Mock Wide"), 10.0);
    let mut manager = font_manager();
    let mut messages = Some(Vec::new());
    let mut running = running_text(&config, &mut manager, &mut messages);
    let stack = running.faces[0].1.clone();
    let shaped = running
        .shape("Report 12", &stack, &config, 600.0, TextAlignment::Center)
        .expect("the text is shaped");
    let layout = without_a_face(&shaped, |_| true);

    let items = running.finish(
        Arc::new(layout),
        &stack,
        &config,
        band(),
        "Report 12",
        "Azul Mock Wide",
    );
    drop(running);

    assert!(items.is_empty());
    let said = told(&messages);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].contains("was not drawn: no face of Azul Mock Wide")
            && !said[0].contains("part of the page text"),
        "{said:?}"
    );
}

/// A finding is told to a layout once per kind and detail, and to the process
/// once per kind: the process's table of once-only findings is bounded, and a
/// service that lays out documents in arbitrary families must not fill it with
/// their names.
#[test]
fn a_finding_is_told_to_the_layout_per_detail_and_to_the_process_per_kind() {
    let _lock = quiet_diagnostics();
    let mut first = Some(Vec::new());
    {
        let mut reporter = Reporter {
            sink: &mut first,
            said: BTreeSet::new(),
        };
        reporter.warn(Finding::UnresolvedFamilies, "Alpha", "[azul][paginate] alpha is unresolved".to_owned());
        reporter.warn(Finding::UnresolvedFamilies, "Beta", "[azul][paginate] beta is unresolved".to_owned());
        // The next page runs into the same finding: it is not said again.
        reporter.warn(
            Finding::UnresolvedFamilies,
            "Alpha",
            "[azul][paginate] alpha is unresolved, once more".to_owned(),
        );
        // Another kind of finding about the same detail is another finding.
        reporter.warn(Finding::NoGlyphs, "Alpha", "[azul][paginate] alpha has no glyphs".to_owned());
    }
    // The next layout is told Alpha again, whatever the process was told before.
    let mut second = Some(Vec::new());
    {
        let mut reporter = Reporter {
            sink: &mut second,
            said: BTreeSet::new(),
        };
        reporter.warn(Finding::UnresolvedFamilies, "Alpha", "[azul][paginate] alpha is unresolved".to_owned());
    }

    assert_eq!(
        told(&first),
        [
            "[azul][paginate] alpha is unresolved",
            "[azul][paginate] beta is unresolved",
            "[azul][paginate] alpha has no glyphs"
        ]
    );
    assert_eq!(told(&second), ["[azul][paginate] alpha is unresolved"]);
    let lines = azul_core::diagnostics::recorded();
    let in_the_process = |needle: &str| lines.iter().filter(|line| line.contains(needle)).count();
    assert_eq!(in_the_process("is unresolved"), 1, "{lines:?}");
    assert_eq!(in_the_process("has no glyphs"), 1, "{lines:?}");
}

/// A list of families is the fallback of its stack only where none of the names
/// it holds is a font: `"Georgia, Foo, serif"` is fine where Georgia is installed.
#[test]
fn only_a_list_every_name_of_which_is_missing_is_set_in_the_fallback() {
    let names = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
    let missing = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect::<BTreeSet<_>>();

    // Georgia matched, Foo did not (the legacy resolver names Foo as missing).
    assert!(families_without_a_font(
        &names(&["Georgia", "Foo", "serif", "sans-serif", "monospace"]),
        &missing(&["Foo"])
    )
    .is_empty());
    // The first choice is the one that is missing.
    assert!(families_without_a_font(
        &names(&["Foo", "Georgia", "serif", "sans-serif", "monospace"]),
        &missing(&["Foo"])
    )
    .is_empty());
    // A name behind a generic is not judged by the registry's resolver: it is not missing.
    assert!(families_without_a_font(
        &names(&["Foo", "serif", "Bar", "sans-serif", "monospace"]),
        &missing(&["Foo"])
    )
    .is_empty());
    // Nothing the list names exists: the whole list is lost, and the generics are not named.
    assert_eq!(
        families_without_a_font(
            &names(&["Foo", "Bar", "serif", "sans-serif", "monospace"]),
            &missing(&["Foo", "Bar"])
        ),
        ["Foo", "Bar"]
    );
    assert_eq!(
        families_without_a_font(&names(&["Foo", "serif"]), &missing(&["Foo"])),
        ["Foo"]
    );
    // A list of generics has nothing to be missing.
    assert!(families_without_a_font(
        &names(&["serif", "sans-serif", "monospace"]),
        &BTreeSet::new()
    )
    .is_empty());
}
