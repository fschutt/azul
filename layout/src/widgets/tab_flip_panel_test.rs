//! The flip of a tab panel as a widget: when the panel flips at all, what it carries (the five
//! callbacks, the dataset, the two hint chips) and what a rebuild keeps.
//!
//! The rules of the gestures themselves are `tab_flip_test.rs`.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    callbacks::Update,
    dom::{Dom, IdOrClass, IdOrClass::Class, NodeType},
    refany::RefAny,
};
use azul_css::{AzString, StringVec};

use super::*;
use crate::widgets::tab_flip::TabFlip;

fn labels(n: usize) -> StringVec {
    StringVec::from_vec((0..n).map(|i| AzString::from(format!("tab {i}"))).collect())
}

extern "C" fn on_click(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
    Update::DoNothing
}

fn header(n: usize, active: usize) -> TabHeader {
    TabHeader::create(labels(n))
        .with_active_tab(active)
        .with_on_click(RefAny::new(0usize), on_click as TabOnClickCallbackType)
}

fn classes_of(node: &Dom) -> Vec<String> {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .filter_map(|c| match c {
            Class(s) => Some(s.as_str().to_string()),
            IdOrClass::Id(_) => None,
        })
        .collect()
}

fn chip_text(chip: &Dom) -> String {
    match chip.children.as_ref()[0].children.as_ref()[0]
        .root
        .get_node_type()
    {
        NodeType::Text(text) => text.as_ref().as_str().to_string(),
        other => panic!("a chip holds a label, got {other:?}"),
    }
}

#[test]
fn a_bar_flips_at_the_edges_unless_told_otherwise() {
    assert_eq!(TabHeader::default().flip, TabFlip::AtEdges);
    assert_eq!(TabHeader::create(labels(3)).flip, TabFlip::AtEdges);
    assert_eq!(
        TabHeader::create(labels(3)).with_flip(TabFlip::Off).flip,
        TabFlip::Off
    );
    assert_eq!(
        TabHeader::create(labels(3)).with_flip(TabFlip::Always).flip,
        TabFlip::Always
    );
}

#[test]
fn a_panel_that_was_not_told_its_bar_does_not_flip() {
    let dom = TabContent::new(Dom::create_div()).dom();
    assert_eq!(dom.children.as_ref().len(), 1, "the group, nothing else");
    assert!(dom.root.get_callbacks().as_ref().is_empty());
    assert_eq!(TabContent::default().flip, TabFlip::Off);
}

#[test]
fn a_panel_flips_only_with_a_bar_that_has_somebody_to_tell_and_a_row_to_flip_in() {
    // No on_click: nobody to tell.
    let silent = TabHeader::create(labels(3));
    assert_eq!(
        TabContent::new(Dom::create_div())
            .with_flip_of(&silent)
            .dom()
            .children
            .as_ref()
            .len(),
        1
    );
    // One tab: nothing to flip to.
    let single = header(1, 0);
    assert_eq!(
        TabContent::new(Dom::create_div())
            .with_flip_of(&single)
            .dom()
            .children
            .as_ref()
            .len(),
        1
    );
    // Off.
    let off = header(3, 1).with_flip(TabFlip::Off);
    let dom = TabContent::new(Dom::create_div()).with_flip_of(&off).dom();
    assert_eq!(dom.children.as_ref().len(), 1);
    assert!(dom.root.get_callbacks().as_ref().is_empty());
}

#[test]
fn a_flipping_panel_takes_the_touches_and_the_wheel_and_has_a_dataset() {
    let bar = header(3, 1);
    let dom = TabContent::new(Dom::create_div()).with_flip_of(&bar).dom();
    // Touch start, move, end, cancel, and the wheel.
    assert_eq!(dom.root.get_callbacks().as_ref().len(), 5);
    assert!(
        dom.root.get_dataset().is_some(),
        "the gesture's state is the panel's dataset"
    );
}

#[test]
fn the_panel_has_the_chips_of_the_neighbours_it_has() {
    let chips = |active: usize| -> Vec<(bool, String)> {
        let bar = header(3, active);
        let dom = TabContent::new(Dom::create_div()).with_flip_of(&bar).dom();
        dom.children.as_ref()[1..]
            .iter()
            .map(|chip| {
                (
                    classes_of(chip).contains(&FLIP_NEXT_CLASS.to_string()),
                    chip_text(chip),
                )
            })
            .collect()
    };
    assert_eq!(chips(0), vec![(true, "Next: tab 1".to_string())]);
    assert_eq!(
        chips(1),
        vec![
            (true, "Next: tab 2".to_string()),
            (false, "Previous: tab 0".to_string())
        ]
    );
    assert_eq!(chips(2), vec![(false, "Previous: tab 1".to_string())]);
}

#[test]
fn the_content_group_comes_first_so_the_pull_finds_it_by_class() {
    let bar = header(3, 1);
    let dom = TabContent::new(Dom::create_div()).with_flip_of(&bar).dom();
    assert!(
        classes_of(&dom.children.as_ref()[0]).contains(&"__azul-native-tabs-content".to_string())
    );
}

#[test]
fn a_rebuild_keeps_the_gesture_and_the_epoch_and_takes_the_new_tab_and_names() {
    let make = |tab: usize, names: &[&str], epoch: u64| {
        RefAny::new(FlipPanelData {
            flip: TabFlip::AtEdges,
            tab,
            names: names.iter().map(|n| (*n).to_string()).collect(),
            on_flip: FlipTell::Tab(OptionTabOnClick::None),
            gesture: None,
            wheel: crate::widgets::tab_flip::WheelPush::default(),
            epoch,
            origin: None,
        })
    };
    let old = make(1, &["a", "b", "c"], 7);
    let new = make(2, &["a", "b", "c", "d"], 0);
    let mut kept = merge_flip_panel(new, old);
    let data = kept
        .downcast_ref::<FlipPanelData>()
        .expect("the old allocation");
    assert_eq!(data.epoch, 7, "a fling in flight reads this");
    assert_eq!(data.tab, 2);
    assert_eq!(data.names.len(), 4);
}

#[test]
fn the_claims_class_is_the_one_the_guide_names() {
    assert_eq!(CLAIMS_GESTURE_CLASS, "__azul-claims-gesture");
}
