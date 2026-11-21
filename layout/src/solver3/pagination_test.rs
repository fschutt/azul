//! The font family of the header, the footer and the page numbers: the
//! additive part of `FakePageConfig` and `HeaderFooterConfig`.

use super::*;

/// No family is the default: the text is set in the document's default font.
#[test]
fn a_config_has_no_font_family_until_one_is_set() {
    assert!(FakePageConfig::new().font_family.is_none());
    assert!(FakePageConfig::default().font_family.is_none());
    assert!(HeaderFooterConfig::default().font_family.is_none());
    assert!(HeaderFooterConfig::with_page_numbers().font_family.is_none());
    assert!(FakePageConfig::new()
        .to_header_footer_config()
        .font_family
        .is_none());
}

/// The builder takes a CSS `font-family` list as it is written, and the
/// internal config the slicer reads carries it.
#[test]
fn with_font_family_sets_the_css_list_and_crosses_the_bridge() {
    let config = FakePageConfig::new()
        .with_header_text("Report")
        .with_footer_page_numbers()
        .with_font_family("Georgia, 'Times New Roman', serif");
    assert_eq!(
        config.font_family.as_ref().map(AzString::as_str),
        Some("Georgia, 'Times New Roman', serif")
    );
    let internal = config.to_header_footer_config();
    assert_eq!(
        internal.font_family.as_ref().map(AzString::as_str),
        Some("Georgia, 'Times New Roman', serif")
    );
    let direct = HeaderFooterConfig::with_page_numbers().with_font_family("serif");
    assert_eq!(direct.font_family.as_ref().map(AzString::as_str), Some("serif"));
}

/// Setting the family touches nothing else: a config built the way printpdf
/// builds one (the builders, then a field) is what it was.
#[test]
fn the_font_family_leaves_every_other_field_as_it_was() {
    let plain = FakePageConfig::new()
        .with_header_text("Report")
        .with_footer_page_numbers()
        .skip_first_page(true);
    let with_family = plain.clone().with_font_family("serif");
    let mut equal = with_family.clone();
    equal.font_family = OptionString::None;
    assert_eq!(equal, plain);
    assert_ne!(with_family, plain, "the family is part of what a config is");
    assert_eq!(
        with_family.to_header_footer_config().header_height,
        plain.to_header_footer_config().header_height,
        "the room the header takes is not the family's business"
    );
}
