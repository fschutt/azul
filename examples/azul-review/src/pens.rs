//! The pens of the top bar (stub: the tests come first).

use crate::model::{Semantic, Tool};

#[derive(Debug, Clone, Copy)]
pub struct PenSpec {
    pub semantic: Semantic,
    pub nibs: &'static [Tool],
}

pub static PENS: [PenSpec; 5] = [
    PenSpec { semantic: Semantic::Scope, nibs: &[Tool::Pen] },
    PenSpec { semantic: Semantic::Scope, nibs: &[Tool::Pen] },
    PenSpec { semantic: Semantic::Scope, nibs: &[Tool::Pen] },
    PenSpec { semantic: Semantic::Scope, nibs: &[Tool::Pen] },
    PenSpec { semantic: Semantic::Scope, nibs: &[Tool::Pen] },
];

#[derive(Debug, Clone, PartialEq)]
pub struct Pens {
    selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    Selected,
    Cycled,
    Ignored,
}

impl Pens {
    #[must_use]
    pub fn new() -> Self {
        Self { selected: 3 }
    }
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }
    #[must_use]
    pub fn semantic(&self) -> Semantic {
        Semantic::Praise
    }
    #[must_use]
    pub fn tool(&self) -> Tool {
        Tool::AudioPen
    }
    pub fn select(&mut self, _index: usize) -> bool {
        true
    }
    pub fn press_key(&mut self, _index: usize) -> Pressed {
        Pressed::Ignored
    }
    pub fn double_click(&mut self, _index: usize) {}
    pub fn cycle(&mut self, _backward: bool) {}
}

impl Default for Pens {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use]
pub fn pen_of(_semantic: Semantic) -> Option<usize> {
    None
}

#[cfg(test)]
#[path = "pens_test.rs"]
mod pens_test;
