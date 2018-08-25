use std::collections::BTreeMap;
use cassowary::{
    Variable, Solver, Constraint,
    strength::*,
};
use glium::glutin::dpi::LogicalSize;
use webrender::api::LayoutPixel;
use euclid::{TypedRect, TypedPoint2D, TypedSize2D};
use {
    id_tree::{NodeId, Arena},
    dom::NodeData,
    cache::{EditVariableCache, DomTreeCache, DomChangeSet},
    constraints::{CssConstraint, RectConstraintVariables},
    traits::Layout,
};

/// Stores the variables of the root width and height (but not the values themselves)
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct WindowSizeConstraints {
    pub(crate) width_var: Variable,
    pub(crate) height_var: Variable,
}

impl WindowSizeConstraints {
    pub fn new() -> Self {
        Self {
            width_var: Variable::new(),
            height_var: Variable::new(),
        }
    }
}

/// Solver for solving the UI of the current window
pub struct UiSolver {
    /// The actual cassowary solver
    solver: Solver,
    /// The size constraints of the root window
    window_constraints: WindowSizeConstraints,
    /// The list of variables that has been added to the solver
    edit_variable_cache: EditVariableCache,
    ///
    solved_values: BTreeMap<Variable, f64>,
    /// The cache of the previous frames DOM tree
    dom_tree_cache: DomTreeCache,
}

impl UiSolver {

    pub(crate) fn new(window_size: &LogicalSize) -> Self {

        let mut solver = Solver::new();
        let window_constraints = WindowSizeConstraints::new();

        solver.add_edit_variable(window_constraints.width_var, STRONG).unwrap();
        solver.add_edit_variable(window_constraints.height_var, STRONG).unwrap();
        solver.suggest_value(window_constraints.width_var, window_size.width as f64).unwrap();
        solver.suggest_value(window_constraints.height_var, window_size.height as f64).unwrap();

        Self {
            solver: solver,
            solved_values: BTreeMap::new(),
            window_constraints: window_constraints,
            edit_variable_cache: EditVariableCache::empty(),
            dom_tree_cache: DomTreeCache::empty(),
        }
    }

    pub(crate) fn update_dom<T: Layout>(&mut self, root: &NodeId, arena: &Arena<NodeData<T>>) -> DomChangeSet {
        let changeset = self.dom_tree_cache.update(*root, arena);
        self.edit_variable_cache.initialize_new_rectangles(&mut self.solver, &changeset);
        self.edit_variable_cache.remove_unused_variables(&mut self.solver);
        changeset
    }

    pub(crate) fn insert_css_constraints_for_rect(&mut self, rect_idx: NodeId, constraints: &[CssConstraint]) {
        use cassowary::strength::*;
        use constraints::{SizeConstraint, Strength, Point};

        let dom_hash = &self.dom_tree_cache.previous_layout.arena[rect_idx];
        let display_rect = &self.edit_variable_cache.map[&dom_hash.data].1;

        println!("display_rect {} - variables: {:?}", rect_idx, display_rect);

        self.solver.add_constraints(&css_constraints_to_cassowary_constraints(display_rect, &[
            CssConstraint::Size(SizeConstraint::TopLeft(Point::new(0.0, 0.0)), Strength(STRONG))
        ])).unwrap();

        self.solver.add_constraints(&css_constraints_to_cassowary_constraints(display_rect, constraints)).unwrap();
    }

    /// Notifies the solver that the window size has changed
    pub(crate) fn update_window_size(&mut self, window_size: &LogicalSize) {
        self.solver.suggest_value(self.window_constraints.width_var, window_size.width).unwrap();
        self.solver.suggest_value(self.window_constraints.height_var, window_size.height).unwrap();
    }

    pub(crate) fn update_layout_cache(&mut self) {
        for (variable, solved_value) in self.solver.fetch_changes() {
            println!("variable {:?} - solved value: {}", variable, solved_value);
            self.solved_values.insert(*variable, *solved_value);
        }
    }

    pub(crate) fn query_bounds_of_rect(&self, rect_id: NodeId) -> TypedRect<f32, LayoutPixel> {

        let display_rect = self.get_rect_constraints(rect_id).unwrap();

        let top = self.solved_values.get(&display_rect.top).and_then(|x| Some(*x)).unwrap_or(0.0);
        let left = self.solved_values.get(&display_rect.left).and_then(|x| Some(*x)).unwrap_or(0.0);

        let right = match self.solved_values.get(&display_rect.right) {
            Some(w) => *w,
            None => self.solved_values[&self.window_constraints.width_var],
        };

        let bottom = match self.solved_values.get(&display_rect.bottom) {
            Some(h) => *h,
            None => self.solved_values[&self.window_constraints.height_var],
        };
/*
        println!("rect id: {} - top: {}, left: {}, right: {}, bottom: {}",
                 rect_id, top, left, right, bottom
        );
*/
        TypedRect::new(TypedPoint2D::new(top as f32, left as f32), TypedSize2D::new((right - left) as f32, (bottom - top) as f32))
    }

    pub(crate) fn get_rect_constraints(&self, rect_id: NodeId) -> Option<RectConstraintVariables> {
        let dom_hash = &self.dom_tree_cache.previous_layout.arena.get(&rect_id)?;
        self.edit_variable_cache.map.get(&dom_hash.data).and_then(|rect| Some(rect.1))
    }

    pub(crate) fn get_window_constraints(&self) -> WindowSizeConstraints {
        self.window_constraints
    }
}

fn css_constraints_to_cassowary_constraints(rect: &RectConstraintVariables, css: &[CssConstraint])
-> Vec<Constraint>
{
    css.iter().flat_map(|constraint|
        match *constraint {
            CssConstraint::Size(constraint, strength) => {
                constraint.build(&rect, strength.0)
            }
            CssConstraint::Padding(constraint, strength, padding) => {
                constraint.build(&rect, strength.0, padding.0)
            }
        }
    ).collect()
}