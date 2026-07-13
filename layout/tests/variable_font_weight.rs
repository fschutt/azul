#![cfg(feature = "text_layout")]

use std::sync::Arc;

use allsorts::{binary::read::ReadScope, font_data::FontData, tables::FontTableProvider, tag};
use azul_core::{
    dom::{Dom, IdOrClass, NodeId},
    styled_dom::StyledDom,
};
use azul_css::props::basic::{FontRef, PhysicalSize};
use azul_layout::{
    font_ref_to_parsed_font,
    font_traits::ParsedFontTrait,
    solver3::getters::get_style_properties,
    text3::{
        cache::{BidiDirection, FontManager, StyleProperties},
        default::PathLoader,
        script::{Language, Script},
    },
};
use rust_fontconfig::{FontBytes, FontId};

const VARIABLE_TTF: &[u8] = include_bytes!("../../doc/fonts/RedHatDisplay-VariableFont_wght.ttf");

fn load_variable_font() -> FontRef {
    let bytes = Arc::new(FontBytes::Owned(Arc::from(VARIABLE_TTF)));
    PathLoader::new()
        .load_font_shared(bytes, 0)
        .expect("Red Hat Display variable font must parse")
}

fn style_at_weight(weight: f32) -> StyleProperties {
    StyleProperties {
        font_size_px: 24.0,
        font_variations: vec![(*b"wght", weight)],
        ..StyleProperties::default()
    }
}

#[test]
fn computed_css_weight_populates_wght_coordinate() {
    let mut dom =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("weighted".into())].into());
    let (css, errors) = azul_css::parser2::new_from_str(".weighted { font-weight: 800; }");
    assert!(errors.is_empty(), "CSS should parse: {errors:?}");
    let styled_dom = StyledDom::create(&mut dom, css);

    let style = get_style_properties(
        &styled_dom,
        NodeId::ZERO,
        None,
        PhysicalSize::new(800.0, 600.0),
    );

    assert_eq!(style.font_variations, vec![(*b"wght", 800.0)]);
}

#[test]
fn shaping_uses_and_reuses_static_weight_instances() {
    let font = load_variable_font();
    let default_style = StyleProperties::default();
    let light_style = style_at_weight(300.0);
    let heavy_style = style_at_weight(900.0);

    let default = font
        .shape_text(
            "Variable",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &default_style,
        )
        .expect("default instance should shape");
    let light = font
        .shape_text(
            "Variable",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &light_style,
        )
        .expect("light instance should shape");
    let heavy = font
        .shape_text(
            "Variable",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &heavy_style,
        )
        .expect("heavy instance should shape");
    let light_again = font
        .shape_text(
            "Variable",
            Script::Latin,
            Language::EnglishUS,
            BidiDirection::Ltr,
            &light_style,
        )
        .expect("cached light instance should shape");

    let base_hash = font_ref_to_parsed_font(&font).get_hash();
    let default_hash = default[0].font_hash;
    let light_hash = light[0].font_hash;
    let heavy_hash = heavy[0].font_hash;
    assert_ne!(
        base_hash, default_hash,
        "even the default coordinates need an embeddable static resource"
    );
    assert_ne!(
        light_hash, heavy_hash,
        "weights need distinct font resources"
    );
    assert_eq!(
        light_hash, light_again[0].font_hash,
        "same tuple must hit the cache"
    );

    let manager: FontManager<FontRef> =
        FontManager::new(rust_fontconfig::FcFontCache::default()).unwrap();
    manager.insert_font(FontId::new(), font);

    let default_font = manager
        .get_font_by_hash(default_hash)
        .expect("manager should resolve the cached default instance");
    let light_font = manager
        .get_font_by_hash(light_hash)
        .expect("manager should resolve the cached light instance");
    let heavy_font = manager
        .get_font_by_hash(heavy_hash)
        .expect("manager should resolve the cached heavy instance");
    let default_parsed = font_ref_to_parsed_font(&default_font);
    let light_parsed = font_ref_to_parsed_font(&light_font);
    let heavy_parsed = font_ref_to_parsed_font(&heavy_font);

    assert_eq!(light_parsed.pdf_font_metrics.us_weight_class, 300);
    assert_eq!(heavy_parsed.pdf_font_metrics.us_weight_class, 900);

    for parsed in [default_parsed, light_parsed, heavy_parsed] {
        let bytes = parsed
            .source_bytes_for_subset()
            .expect("static instance bytes must remain available");
        let font_data = ReadScope::new(bytes.as_slice())
            .read::<FontData<'_>>()
            .unwrap();
        let provider = font_data.table_provider(0).unwrap();
        assert!(!provider.has_table(tag::FVAR));
        assert!(!provider.has_table(tag::GVAR));
        assert!(provider.has_table(tag::GLYF));
    }
}
