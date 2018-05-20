//! Constraint building (mostly taken from `limn_layout`)

use cassowary::{Solver, Variable, Constraint};
use cassowary::WeightedRelation::{EQ, GE};
use cassowary::strength::{WEAK, REQUIRED};
use euclid::{Point2D, Size2D};

pub type Size = Size2D<f32>;
pub type Point = Point2D<f32>;

/// A set of cassowary `Variable`s representing the
/// bounding rectangle of a layout.
#[derive(Debug, Copy, Clone)]
pub(crate) struct DisplayRect {
    pub left: Variable,
    pub top: Variable,
    pub right: Variable,
    pub bottom: Variable,
    pub width: Variable,
    pub height: Variable,
}

impl Default for DisplayRect {
    fn default() -> Self {
        Self {
            left: Variable::new(),
            top: Variable::new(),
            right: Variable::new(),
            bottom: Variable::new(),
            width: Variable::new(),
            height: Variable::new(),
        }
    }
}

impl DisplayRect {

    pub fn add_to_solver(&self, solver: &mut Solver) {
        solver.add_edit_variable(self.left, WEAK).unwrap_or_else(|_e| { });
        solver.add_edit_variable(self.top, WEAK).unwrap_or_else(|_e| { });
        solver.add_edit_variable(self.right, WEAK).unwrap_or_else(|_e| { });
        solver.add_edit_variable(self.bottom, WEAK).unwrap_or_else(|_e| { });
        solver.add_edit_variable(self.width, WEAK).unwrap_or_else(|_e| { });
        solver.add_edit_variable(self.height, WEAK).unwrap_or_else(|_e| { });
    }

    pub fn remove_from_solver(&self, solver: &mut Solver) {
        solver.remove_edit_variable(self.left).unwrap_or_else(|_e| { });
        solver.remove_edit_variable(self.top).unwrap_or_else(|_e| { });
        solver.remove_edit_variable(self.right).unwrap_or_else(|_e| { });
        solver.remove_edit_variable(self.bottom).unwrap_or_else(|_e| { });
        solver.remove_edit_variable(self.width).unwrap_or_else(|_e| { });
        solver.remove_edit_variable(self.height).unwrap_or_else(|_e| { });
    }

}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct Strength(pub f64);

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
pub struct Padding(pub f32);

#[derive(Debug, Copy, Clone)]
pub(crate) enum CssConstraint {
    Size((SizeConstraint, Strength)),
    Padding((PaddingConstraint, Strength, Padding))
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum SizeConstraint {
    Width(f32),
    Height(f32),
    MinWidth(f32),
    MinHeight(f32),
    Size(Size),
    MinSize(Size),
    AspectRatio(f32),
    Shrink,
    ShrinkHorizontal,
    ShrinkVertical,
    TopLeft(Point),
    Center(DisplayRect),
    CenterHorizontal(Variable, Variable),
    CenterVertical(Variable, Variable),
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum PaddingConstraint {
    AlignTop(Variable),
    AlignBottom(Variable),
    AlignLeft(Variable),
    AlignRight(Variable),
    AlignAbove(Variable),
    AlignBelow(Variable),
    AlignToLeftOf(Variable),
    AlignToRightOf(Variable),
    Above(Variable),
    Below(Variable),
    ToLeftOf(Variable),
    ToRightOf(Variable),
    BoundLeft(Variable),
    BoundTop(Variable),
    BoundRight(Variable),
    BoundBottom(Variable),
    BoundBy(DisplayRect),
    MatchLayout(DisplayRect),
    MatchWidth(Variable),
    MatchHeight(Variable),
}

impl SizeConstraint {
    pub(crate) fn build(&self, rect: &DisplayRect, strength: f64) -> Vec<Constraint> {
        use self::SizeConstraint::*;

        match *self {
            Width(width) => {
                vec![ rect.width | EQ(strength) | width ]
            },
            Height(height) => {
                vec![ rect.height | EQ(strength) | height ]
            },
            MinWidth(width) => {
                vec![ rect.width | GE(strength) | width ]
            },
            MinHeight(height) => {
                vec![ rect.height | GE(strength) | height ]
            },
            Size(size) => {
                vec![
                    rect.width | EQ(strength) | size.width,
                    rect.height | EQ(strength) | size.height,
                ]
            },
            MinSize(size) => {
                vec![
                    rect.width | GE(strength) | size.width,
                    rect.height | GE(strength) | size.height,
                ]
            },
            AspectRatio(aspect_ratio) => {
                vec![ aspect_ratio * rect.width | EQ(strength) | rect.height ]
            },
            Shrink => {
                vec![
                    rect.width | EQ(strength) | 0.0,
                    rect.height | EQ(strength) | 0.0,
                ]
            },
            ShrinkHorizontal => {
                vec![ rect.width | EQ(strength) | 0.0 ]
            },
            ShrinkVertical => {
                vec![ rect.height | EQ(strength) | 0.0 ]
            },
            TopLeft(point) => {
                vec![
                    rect.left | EQ(strength) | point.x,
                    rect.top | EQ(strength) | point.y,
                ]
            },
            Center(other) => {
                vec![
                    rect.left - other.left | EQ(REQUIRED) | other.right - rect.right,
                    rect.top - other.top | EQ(REQUIRED) | other.bottom - rect.bottom,
                ]
            },
            CenterHorizontal(left, right) => {
                vec![ rect.left - left | EQ(REQUIRED) | right - rect.right ]
            },
            CenterVertical(top, bottom) => {
                vec![ rect.top - top | EQ(REQUIRED) | bottom - rect.bottom ]
            },
        }
    }
}

impl PaddingConstraint {
    pub(crate) fn build(&self, rect: &DisplayRect, strength: f64, padding: f32) -> Vec<Constraint> {
        use self::PaddingConstraint::*;
        match *self {
            AlignTop(top) => {
                vec![ rect.top - top | EQ(strength) | padding ]
            },
            AlignBottom(bottom) => {
                vec![ bottom - rect.bottom | EQ(strength) | padding ]
            },
            AlignLeft(left) => {
                vec![ rect.left - left | EQ(strength) | padding ]
            },
            AlignRight(right) => {
                vec![ right - rect.right | EQ(strength) | padding ]
            },
            AlignAbove(top) => {
                vec![ top - rect.bottom | EQ(strength) | padding ]
            },
            AlignBelow(bottom) => {
                vec![ rect.top - bottom | EQ(strength) | padding ]
            },
            AlignToLeftOf(left) => {
                vec![ left - rect.right | EQ(strength) | padding ]
            },
            AlignToRightOf(right) => {
                vec![ rect.left - right | EQ(strength) | padding ]
            },
            Above(top) => {
                vec![ top - rect.bottom | GE(strength) | padding ]
            },
            Below(bottom) => {
                vec![ rect.top - bottom | GE(strength) | padding ]
            },
            ToLeftOf(left) => {
                vec![ left - rect.right | GE(strength) | padding ]
            },
            ToRightOf(right) => {
                vec![ rect.left - right | GE(strength) | padding ]
            },
            BoundLeft(left) => {
                vec![ rect.left - left | GE(strength) | padding ]
            },
            BoundTop(top) => {
                vec![ rect.top - top | GE(strength) | padding ]
            },
            BoundRight(right) => {
                vec![ right - rect.right | GE(strength) | padding ]
            },
            BoundBottom(bottom) => {
                vec![ bottom - rect.bottom | GE(strength) | padding ]
            },
            BoundBy(other) => {
                vec![
                    rect.left - other.left | GE(strength) | padding,
                    rect.top - other.top | GE(strength) | padding,
                    other.right - rect.right | GE(strength) | padding,
                    other.bottom - rect.bottom | GE(strength) | padding,
                ]
            },
            MatchLayout(other) => {
                vec![
                    rect.left - other.left | EQ(strength) | padding,
                    rect.top - other.top | EQ(strength) | padding,
                    other.right - rect.right | EQ(strength) | padding,
                    other.bottom - rect.bottom | EQ(strength) | padding,
                ]
            },
            MatchWidth(width) => {
                vec![ width - rect.width | EQ(strength) | padding ]
            },
            MatchHeight(height) => {
                vec![ height - rect.height | EQ(strength) | padding ]
            },
        }
    }
}