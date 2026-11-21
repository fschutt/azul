//! What the page furniture asks of the font system: which stacks, for which
//! characters. The drawing of it is pinned through `layout_document_paged_v2`
//! (`paged_layout_test.rs`).

use azul_css::AzString;

use super::*;
use crate::solver3::pagination::{OptionPageSetup, PageMargins, PageSequence, PageSetup};

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
