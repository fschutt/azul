//! A paragraph hyphenates again when its language changes.
//!
//! `hyphens: auto` breaks words in the content language: the `lang` of the
//! nearest element that states one. An app that switches a region to another
//! language rebuilds it with another `lang`. Only an attribute changes; the
//! paragraph keeps its own data and every style its text is shaped with. It
//! still has to break its words anew, and in block flow nothing else asks it
//! to: its size cache answers with the lines of the old language.
//!
//! Font-independent, like the other hyphenation tests: no font sets the 20 px
//! word "Advertisement" on one line of a 60 px box, so a hyphenated box is
//! several 30 px lines tall and an unhyphenated one (the word overflows) is
//! one line. `x-none` names no language with hyphenation patterns.

use azul_core::dom::{AttributeType, Dom};

use crate::{
    a_measured_label_flows_its_text_only_at_a_new_width::{
        assert_lays_out_like_a_fresh_window, node_with_class, rebuild, rect,
    },
    a_one_box_slide_does_not_re_lay_out_the_page::window,
};

/// Where the page states its language.
#[derive(Clone, Copy)]
enum StatedOn {
    /// On the region around the word: the word's box inherits it.
    Region,
    /// On the word's own box.
    Word,
}

/// A 60 px box of 30 px lines holding the word.
fn narrow_word() -> Dom {
    Dom::create_div_with_text("Advertisement")
        .with_class("word".into())
        .with_css("width: 60px; font-size: 20px; line-height: 30px;")
}

/// The word and a block after it in one 60 px box: the word sits in an
/// anonymous block of its own.
fn word_beside_a_block() -> Dom {
    narrow_word().with_child(Dom::create_div().with_css("height: 10px;"))
}

/// The page: a region that hyphenates, holding a heading, the word's box
/// `content` makes and a box below it, in language `lang` stated `on` the
/// region or the word's box.
fn page(lang: &str, on: StatedOn, content: fn() -> Dom) -> Dom {
    let in_lang = |dom: Dom| dom.with_attribute(AttributeType::Lang(lang.into()));
    let (region, word) = match on {
        StatedOn::Region => (in_lang(Dom::create_div()), content()),
        StatedOn::Word => (Dom::create_div(), in_lang(content())),
    };
    Dom::create_html().with_child(
        Dom::create_body().with_css("margin: 0px;").with_child(
            region
                .with_css("width: 300px; hyphens: auto;")
                .with_child(Dom::create_div_with_text("Files"))
                .with_child(word)
                .with_child(Dom::create_div().with_css("height: 20px;")),
        ),
    )
}

/// How tall the word's box is in a window opened on the page in `lang`.
fn word_height(lang: &str, on: StatedOn, content: fn() -> Dom) -> f32 {
    let lw = window(page(lang, on, content));
    rect(&lw, node_with_class(&lw, "word")).size.height
}

/// Switches the page from no language to English and back, twice; every
/// page lays out as a window opened on it does.
fn switch_back_and_forth(on: StatedOn, content: fn() -> Dom) {
    let english = word_height("en", on, content);
    let none = word_height("x-none", on, content);
    assert!(
        english > none + 25.0,
        "harness: lang=\"en\" hyphenates the word over several lines ({english} px high, \
         {none} px in no language)"
    );
    let mut lw = window(page("x-none", on, content));
    for lang in ["en", "x-none", "en", "x-none"] {
        rebuild(&mut lw, page(lang, on, content));
        assert_lays_out_like_a_fresh_window(
            &lw,
            page(lang, on, content),
            &format!("the page is in `{lang}`"),
        );
    }
}

/// The region switches language: the paragraph in it breaks its word in
/// English and back into one overflowing line.
#[test]
fn a_paragraph_hyphenates_again_when_its_region_switches_language() {
    switch_back_and_forth(StatedOn::Region, narrow_word);
}

/// The same for a word that shares its box with a block: the anonymous block
/// around the word breaks it anew.
#[test]
fn a_word_beside_a_block_hyphenates_again_when_its_region_switches_language() {
    switch_back_and_forth(StatedOn::Region, word_beside_a_block);
}

/// The paragraph's own `lang` changes: the box holding the lines is the one
/// whose attribute changed.
#[test]
fn a_paragraph_hyphenates_again_when_its_own_language_changes() {
    switch_back_and_forth(StatedOn::Word, narrow_word);
}
