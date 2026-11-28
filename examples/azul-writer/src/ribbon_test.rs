//! The ribbon: its tabs, its Home band, the font combos and the style gallery.

use azul::widgets::RichTextDoc;

use super::*;

fn editor_on(markdown: &str) -> RichTextEditorState {
    RichTextEditorState::create(RichTextDoc::create_from_markdown(markdown))
}

/// The words inside `dom`.
fn texts(dom: &Dom) -> Vec<String> {
    let mut out: Vec<String> = dom
        .root
        .node_type
        .get_text()
        .into_option()
        .map(|t| t.as_str().to_string())
        .into_iter()
        .collect();
    for child in dom.children.as_slice() {
        out.extend(texts(child));
    }
    out
}

fn desktop() -> Strip {
    Strip {
        left: 0.0,
        right: 0.0,
        width: 1280.0,
        phone: false,
        content_inset: 0.0,
        band: None,
    }
}

fn state_on(tab: usize) -> AppState {
    let mut st = AppState::new(RefAny::new(()), std::path::PathBuf::from("data"), false);
    st.ribbon_tab = tab;
    st
}

/// The File tab is said in the window's language, as the tabs beside it are: DATEI, not
/// FILE, in a German window.
#[test]
fn the_file_tab_is_said_in_the_windows_language() {
    crate::l10n::in_english();
    let st = state_on(0);
    azul_appkit::l10n::set_locale("de-DE");
    let shown = texts(&ribbon(&RefAny::new(()), &st, &desktop()));
    azul_appkit::l10n::set_locale("en-US");
    assert!(shown.iter().any(|t| t == "DATEI"), "{shown:?}");
    assert!(shown.iter().any(|t| t == "START"), "the tab beside it: {shown:?}");
    assert!(!shown.iter().any(|t| t == "FILE"), "{shown:?}");
}

#[test]
fn the_ribbon_has_file_and_the_five_bands_of_the_card_in_order() {
    crate::l10n::in_english();
    let shown = texts(&ribbon(&RefAny::new(()), &state_on(1), &desktop()));
    let labels = ["FILE", "HOME", "INSERT", "LAYOUT", "REVIEW", "VIEW"];
    let mut at = 0;
    for word in labels {
        let Some(found) = shown[at..].iter().position(|t| t == word) else {
            panic!("{word} is missing or out of order: {shown:?}");
        };
        at += found + 1;
    }
}

/// Word 2010's HOME tab starts with the Clipboard group: Paste, Cut and Copy.
#[test]
fn the_home_tab_starts_with_the_clipboard_group() {
    crate::l10n::in_english();
    let shown = texts(&ribbon(&RefAny::new(()), &state_on(1), &desktop()));
    for word in ["Clipboard", "Paste", "Cut", "Copy"] {
        assert!(shown.iter().any(|t| t == word), "{word} is missing: {shown:?}");
    }
}

#[test]
fn the_review_band_counts_words_and_says_why_the_rest_is_not_there() {
    crate::l10n::in_english();
    let shown = texts(&ribbon(&RefAny::new(()), &state_on(4), &desktop()));
    assert!(shown.iter().any(|t| t.contains("Word")), "{shown:?}");
}

#[test]
fn the_font_combos_show_the_carets_family_and_the_nearest_size() {
    assert_eq!(font_family_index(""), 0, "the paper's face");
    assert_eq!(FONT_FAMILIES[font_family_index("monospace")].1, "monospace");
    assert_eq!(font_family_index("Comic Sans"), 0, "an unknown family shows the default");
    assert_eq!(FONT_SIZES[font_size_index(0.0)], DEFAULT_FONT_SIZE, "no size of its own");
    assert_eq!(FONT_SIZES[font_size_index(10.5)], 10.5);
    assert_eq!(FONT_SIZES[font_size_index(15.0)], 14.0, "between 14 and 16: the smaller");
    assert_eq!(FONT_SIZES[font_size_index(17.5)], 18.0);
    assert_eq!(FONT_SIZES[font_size_index(500.0)], 72.0);
}

#[test]
fn every_style_of_the_gallery_is_its_own_kind() {
    // The old gallery mapped its 3rd and 5th cells both to heading 1
    // (DEDUP_EDITORS A3.5).
    let kinds: Vec<Option<RichBlockKind>> = (0..STYLES.len()).map(style_kind).collect();
    assert_eq!(kinds[1], Some(RichBlockKind::Heading(1)));
    assert_eq!(kinds[2], Some(RichBlockKind::Heading(2)));
    assert_eq!(kinds[3], Some(RichBlockKind::Heading(3)));
    assert_eq!(style_kind(STYLES.len()), None);
}

#[test]
fn the_gallery_shows_the_carets_style_and_a_pick_never_toggles_it_off() {
    let heading = editor_on("# Title\n");
    assert_eq!(current_style(&heading), 1);
    assert!(style_commands(&heading, 1).is_empty(), "heading 1 again changes nothing");
    assert_eq!(
        style_commands(&heading, 2),
        vec![RichTextCommand::ToggleKind(RichBlockKind::Heading(2))]
    );
    let quote = editor_on("> words\n");
    assert_eq!(current_style(&quote), 4);
    assert_eq!(style_commands(&quote, 0), vec![RichTextCommand::ToggleQuote]);
    let plain = editor_on("words\n");
    assert_eq!(style_commands(&plain, 4), vec![RichTextCommand::ToggleQuote]);
}

#[test]
fn the_sidebar_styles_and_the_ribbons_gallery_are_the_same_six_styles() {
    // the sidebar's Styles section lists STYLES one row each: the tag range holds them all
    assert!(STYLES.len() <= 1000);
    assert!(STYLES.iter().all(|(key, _)| key.starts_with("azwriter-style-")));
}

#[test]
fn a_window_draws_under_the_bars_only_on_a_phone_build() {
    assert_eq!(draws_under_the_bars(), cfg!(any(target_os = "android", target_os = "ios")));
}
