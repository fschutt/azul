//! The five pens of the top bar: each a meaning with its own nibs; a click selects, a double click or the same key again
//! cycles the nib.

use super::*;
use crate::model::{Semantic, Tool};

#[test]
fn the_pens_are_the_five_meanings_in_order_and_the_scope_pen_alone_has_the_marker() {
    let meanings: Vec<Semantic> = PENS.iter().map(|p| p.semantic).collect();
    assert_eq!(meanings, Semantic::ALL.to_vec());
    assert_eq!(PENS[0].nibs, &[Tool::Marker, Tool::Pen, Tool::AudioPen]);
    for pen in &PENS[1..] {
        assert_eq!(pen.nibs, &[Tool::Pen, Tool::AudioPen], "{:?}", pen.semantic);
    }
}

#[test]
fn a_new_set_of_pens_has_the_scope_pen_in_hand_with_the_marker_as_before() {
    let pens = Pens::new();
    assert_eq!(pens.selected(), 0);
    assert_eq!(pens.semantic(), Semantic::Scope);
    assert_eq!(pens.tool(), Tool::Marker);
}

#[test]
fn selecting_a_pen_changes_the_meaning_and_not_the_nib_of_any_other_pen() {
    let mut pens = Pens::new();
    assert!(pens.select(1));
    assert_eq!(pens.semantic(), Semantic::Issue);
    assert_eq!(pens.tool(), Tool::Pen, "the issue pen starts with its pen nib");
    assert!(!pens.select(1), "already in hand: nothing changed");
    assert!(pens.select(0));
    assert_eq!(pens.tool(), Tool::Marker, "the scope pen is as it was left");
}

#[test]
fn each_pen_keeps_the_nib_it_was_left_with() {
    let mut pens = Pens::new();
    pens.select(2);
    pens.cycle(false);
    assert_eq!(pens.tool(), Tool::AudioPen);
    pens.select(0);
    pens.cycle(false);
    assert_eq!(pens.tool(), Tool::Pen);
    pens.select(2);
    assert_eq!(pens.tool(), Tool::AudioPen, "the question pen still has the audio nib");
    pens.select(0);
    assert_eq!(pens.tool(), Tool::Pen, "and the scope pen the pen nib");
}

#[test]
fn the_key_of_the_pen_in_hand_cycles_its_nib_and_the_key_of_another_pen_takes_that_one() {
    let mut pens = Pens::new();
    assert_eq!(pens.press_key(0), Pressed::Cycled);
    assert_eq!(pens.tool(), Tool::Pen);
    assert_eq!(pens.press_key(3), Pressed::Selected);
    assert_eq!(pens.semantic(), Semantic::Duplicate);
    assert_eq!(pens.tool(), Tool::Pen, "a pen taken by its key is not cycled by it");
    assert_eq!(pens.press_key(3), Pressed::Cycled);
    assert_eq!(pens.tool(), Tool::AudioPen);
    assert_eq!(pens.press_key(9), Pressed::Ignored, "no sixth pen");
}

#[test]
fn a_double_click_takes_the_pen_and_cycles_its_nib() {
    let mut pens = Pens::new();
    pens.double_click(4);
    assert_eq!(pens.semantic(), Semantic::Praise);
    assert_eq!(pens.tool(), Tool::AudioPen);
    pens.double_click(4);
    assert_eq!(pens.tool(), Tool::Pen, "two nibs: back to the first");
    // on the pen in hand it is the same
    pens.double_click(4);
    assert_eq!(pens.tool(), Tool::AudioPen);
}

#[test]
fn the_nibs_cycle_round_in_both_directions() {
    let mut pens = Pens::new();
    let forward: Vec<Tool> = (0..4).map(|_| { pens.cycle(false); pens.tool() }).collect();
    assert_eq!(forward, vec![Tool::Pen, Tool::AudioPen, Tool::Marker, Tool::Pen]);
    let mut pens = Pens::new();
    let backward: Vec<Tool> = (0..4).map(|_| { pens.cycle(true); pens.tool() }).collect();
    assert_eq!(backward, vec![Tool::AudioPen, Tool::Pen, Tool::Marker, Tool::AudioPen]);
}

#[test]
fn a_pen_out_of_range_is_ignored() {
    let mut pens = Pens::new();
    assert!(!pens.select(5));
    pens.double_click(7);
    assert_eq!(pens.selected(), 0);
    assert_eq!(pens.tool(), Tool::Marker);
}

#[test]
fn the_pen_of_a_meaning_is_found_by_the_meaning() {
    assert_eq!(pen_of(Semantic::Scope), Some(0));
    assert_eq!(pen_of(Semantic::Praise), Some(4));
}
