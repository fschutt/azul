//! "Reduce motion" at the cascade: while the window's context asks for reduced motion, the three
//! properties that declare an animation (`animation`, `-azul-animation-in`, `-azul-animation-out`)
//! read as not declared, so nothing the engine animates for a declaration runs; every other
//! property is read as before.

use azul_css::{
    accessibility::{AccessibilityPreferences, SettingState},
    dynamic_selector::{BoolCondition, DynamicSelectorContext},
    props::property::CssPropertyType,
    OptionF32,
};

use crate::{dom::Dom, id::NodeId, styled_dom::StyledDom};

fn styled() -> StyledDom {
    let mut dom = Dom::create_body().with_children(vec![Dom::create_div()].into());
    let css = azul_css::parser2::new_from_str(
        "div { animation: all 1s linear; width: 100px; }",
    )
    .0;
    StyledDom::create(&mut dom, css)
}

/// Whether the node 1 (the div) declares `ty` for the cascade, both ways of asking.
fn reads(sd: &StyledDom, ty: CssPropertyType) -> (bool, bool) {
    let id = NodeId::new(1);
    let node = &sd.node_data.as_container()[id];
    let state = sd.get_styled_node_state(&id);
    let cache = sd.get_css_property_cache();
    (
        cache.get_property(node, &id, &state, &ty).is_some(),
        cache.get_property_if_declared(node, &id, &state, &ty).is_some(),
    )
}

fn context(reduced: SettingState) -> DynamicSelectorContext {
    DynamicSelectorContext {
        preferences: AccessibilityPreferences {
            reduced_motion: reduced,
            ..AccessibilityPreferences::default()
        },
        ..DynamicSelectorContext::default()
    }
}

#[test]
fn a_declared_animation_is_read_until_the_user_asks_for_reduced_motion() {
    let mut sd = styled();
    assert_eq!(reads(&sd, CssPropertyType::Animation), (true, true), "no window: the declaration is read");

    sd.set_dynamic_selector_context(context(SettingState::Off));
    assert_eq!(reads(&sd, CssPropertyType::Animation), (true, true), "motion is wanted");

    sd.set_dynamic_selector_context(context(SettingState::On));
    assert_eq!(
        reads(&sd, CssPropertyType::Animation),
        (false, false),
        "reduced motion: the node declares no animation"
    );
    assert!(sd.get_css_property_cache().motion_is_reduced());

    sd.set_dynamic_selector_context(context(SettingState::Unknown));
    assert_eq!(
        reads(&sd, CssPropertyType::Animation),
        (true, true),
        "an option the platform did not answer is not a request"
    );
}

#[test]
fn every_other_property_is_read_as_before() {
    let mut sd = styled();
    sd.set_dynamic_selector_context(context(SettingState::On));
    assert_eq!(reads(&sd, CssPropertyType::Width), (true, true), "width is not an animation");
}

#[test]
fn the_animation_properties_are_the_three_that_declare_one() {
    for ty in [
        CssPropertyType::Animation,
        CssPropertyType::AnimationIn,
        CssPropertyType::AnimationOut,
    ] {
        assert!(ty.is_animation_declaration(), "{ty:?}");
    }
    for ty in [CssPropertyType::Width, CssPropertyType::Opacity, CssPropertyType::Transform] {
        assert!(!ty.is_animation_declaration(), "{ty:?}");
    }
}

#[test]
fn the_system_animation_speed_of_zero_and_the_older_condition_ask_for_it_too() {
    let mut sd = styled();
    let mut preferences = AccessibilityPreferences::default();
    preferences.animation_scale = OptionF32::Some(0.0);
    sd.set_dynamic_selector_context(DynamicSelectorContext {
        preferences,
        ..DynamicSelectorContext::default()
    });
    assert_eq!(reads(&sd, CssPropertyType::Animation), (false, false), "Android's remove animations");

    let mut sd = styled();
    sd.set_dynamic_selector_context(DynamicSelectorContext {
        prefers_reduced_motion: BoolCondition::True,
        ..DynamicSelectorContext::default()
    });
    assert_eq!(reads(&sd, CssPropertyType::Animation), (false, false), "the mock of a test");
}
