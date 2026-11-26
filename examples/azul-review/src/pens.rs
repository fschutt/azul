//! The pens of the top bar: five meanings, each a pen of its own with the nibs it can have. A click on a pen takes it,
//! a double click on it (or its key pressed again) takes the next nib; each pen keeps the nib it was left with.
//!
//! The data is the list `PENS` (the keys 1 to 5 are the places in it): what a pen means and which nibs it has are
//! said there and nowhere else. The marker is the broad highlighter that always means "scope", so only the scope pen
//! has it; the others write with a pen or with an audio pen (a pen that records the voice as it draws).

use crate::model::{Semantic, Tool};

/// One pen: the meaning it writes and the nibs it has (the first is what it starts with).
#[derive(Debug, Clone, Copy)]
pub struct PenSpec {
    pub semantic: Semantic,
    pub nibs: &'static [Tool],
}

/// The pens, in the order of their keys (1 to 5).
pub static PENS: [PenSpec; 5] = [
    PenSpec { semantic: Semantic::Scope, nibs: &[Tool::Marker, Tool::Pen, Tool::AudioPen] },
    PenSpec { semantic: Semantic::Issue, nibs: &[Tool::Pen, Tool::AudioPen] },
    PenSpec { semantic: Semantic::Question, nibs: &[Tool::Pen, Tool::AudioPen] },
    PenSpec { semantic: Semantic::Duplicate, nibs: &[Tool::Pen, Tool::AudioPen] },
    PenSpec { semantic: Semantic::Praise, nibs: &[Tool::Pen, Tool::AudioPen] },
];

/// The place of the pen of a meaning (`None`: no pen writes it).
#[must_use]
pub fn pen_of(semantic: Semantic) -> Option<usize> {
    PENS.iter().position(|p| p.semantic == semantic)
}

/// What a key of a pen did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// Another pen was taken.
    Selected,
    /// The pen in hand has its next nib.
    Cycled,
    /// No such pen.
    Ignored,
}

/// Which pen is in hand and which nib each pen has.
#[derive(Debug, Clone, PartialEq)]
pub struct Pens {
    selected: usize,
    /// For each pen, the place in its `nibs`.
    nib: [usize; PENS.len()],
}

impl Default for Pens {
    fn default() -> Self {
        Self::new()
    }
}

impl Pens {
    /// The scope pen in hand with the marker, as AzReview has always begun.
    #[must_use]
    pub fn new() -> Self {
        Self { selected: 0, nib: [0; PENS.len()] }
    }

    /// The place of the pen in hand.
    #[must_use]
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// What the pen in hand writes.
    #[must_use]
    pub fn semantic(&self) -> Semantic {
        PENS[self.selected].semantic
    }

    /// The nib of the pen in hand.
    #[must_use]
    pub fn tool(&self) -> Tool {
        self.nib_of(self.selected).unwrap_or(Tool::Pen)
    }

    /// The nib pen `index` has (`None`: no such pen).
    #[must_use]
    pub fn nib_of(&self, index: usize) -> Option<Tool> {
        let pen = PENS.get(index)?;
        pen.nibs.get(self.nib[index]).copied()
    }

    /// Takes pen `index`; whether that changed what is in hand.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= PENS.len() || index == self.selected {
            return false;
        }
        self.selected = index;
        true
    }

    /// The key of pen `index`: the pen in hand has its next nib, any other pen is taken.
    pub fn press_key(&mut self, index: usize) -> Pressed {
        if index >= PENS.len() {
            return Pressed::Ignored;
        }
        if index == self.selected {
            self.cycle(false);
            Pressed::Cycled
        } else {
            self.select(index);
            Pressed::Selected
        }
    }

    /// A double click on pen `index`: it is taken and has its next nib.
    pub fn double_click(&mut self, index: usize) {
        if index >= PENS.len() {
            return;
        }
        self.select(index);
        self.cycle(false);
    }

    /// The pen in hand has its next (or its previous) nib, round.
    pub fn cycle(&mut self, backward: bool) {
        let count = PENS[self.selected].nibs.len();
        let at = &mut self.nib[self.selected];
        *at = if backward { (*at + count - 1) % count } else { (*at + 1) % count };
    }
}

#[cfg(test)]
#[path = "pens_test.rs"]
mod pens_test;
