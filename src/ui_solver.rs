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
    traits::Layout,
};

/// A set of cassowary `Variable`s representing the
/// bounding rectangle of a layout.
#[derive(Debug, Copy, Clone)]
pub(crate) struct RectConstraintVariables {
    pub left: Variable,
    pub top: Variable,
    pub width: Variable,
    pub height: Variable,
}

impl Default for RectConstraintVariables {
    fn default() -> Self {
        Self {
            left: Variable::new(),
            top: Variable::new(),
            width: Variable::new(),
            height: Variable::new(),
        }
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_constraints_file() {

}

/// Stores the variables of the root width and height (but not the values themselves)
///
/// Note that the position will always be (0, 0). The layout solver doesn't
/// know where it is on the screen, it only knows the size, but not the position.
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct RootSizeConstraints {
    pub(crate) width_var: Variable,
    pub(crate) height_var: Variable,
}

impl RootSizeConstraints {
    pub fn new(solver: &mut Solver, root_size: LogicalSize) -> Self {

        let width_var = Variable::new();
        let height_var = Variable::new();

        solver.add_edit_variable(width_var, STRONG).unwrap();
        solver.add_edit_variable(height_var, STRONG).unwrap();
        solver.suggest_value(width_var, root_size.width as f64).unwrap();
        solver.suggest_value(height_var, root_size.height as f64).unwrap();

        Self {
            width_var,
            height_var,
        }
    }
}

/// Solver for solving the UI of the current window
pub struct UiSolver {
    /// The actual cassowary solver
    solver: Solver,
    /// In order to remove constraints, we need to store them somewhere
    /// and then remove them from the cassowary solver when they aren't necessary
    /// anymore. This is a pretty hard problem, which is why we need `DomChangeSet`
    /// to get a list of removed NodeIds
    added_constraints: BTreeMap<NodeId, Vec<Constraint>>,
    /// The size constraints of the root of the UI.
    /// Usually this will be the window width and height, but it can also be a
    /// Sub-DOM rectangle (for iframe rendering).
    root_constraints: RootSizeConstraints,
    /// The list of variables that has been added to the solver
    edit_variable_cache: EditVariableCache,
    /// A cache of solved layout variables, updated by the solver on every frame
    solved_values: BTreeMap<Variable, f64>,
    /// The cache of the previous frames DOM tree
    dom_tree_cache: DomTreeCache,
}

impl UiSolver {

    pub(crate) fn new(window_size: LogicalSize) -> Self {

        let mut solver = Solver::new();

        let root_constraints = RootSizeConstraints::new(&mut solver, window_size);

        Self {
            solver: solver,
            added_constraints: BTreeMap::new(),
            solved_values: BTreeMap::new(),
            root_constraints,
            edit_variable_cache: EditVariableCache::empty(),
            dom_tree_cache: DomTreeCache::empty(),
        }
    }

    pub(crate) fn update_dom<T: Layout>(&mut self, root: &NodeId, arena: &Arena<NodeData<T>>) -> DomChangeSet {
        let changeset = self.dom_tree_cache.update(*root, arena);
        self.edit_variable_cache.initialize_new_rectangles(&changeset);
        self.edit_variable_cache.remove_unused_variables();
        changeset
    }

    pub(crate) fn insert_css_constraints_for_rect(&mut self, constraints: &[Constraint]) {
        self.solver.add_constraints(constraints).unwrap();
    }

    /// Notifies the solver that the window size has changed
    pub(crate) fn update_window_size(&mut self, window_size: &LogicalSize) {
        self.solver.suggest_value(self.root_constraints.width_var, window_size.width).unwrap();
        self.solver.suggest_value(self.root_constraints.height_var, window_size.height).unwrap();
    }

    pub(crate) fn update_layout_cache(&mut self) {
        for (variable, solved_value) in self.solver.fetch_changes() {
            self.solved_values.insert(*variable, *solved_value);
        }
    }

    pub(crate) fn query_bounds_of_rect(&self, rect_id: NodeId) -> TypedRect<f32, LayoutPixel> {

        let display_rect = self.get_rect_constraints(rect_id).unwrap();

        let top = self.solved_values.get(&display_rect.top).and_then(|x| Some(*x)).unwrap_or(0.0);
        let left = self.solved_values.get(&display_rect.left).and_then(|x| Some(*x)).unwrap_or(0.0);
        let width = self.solved_values.get(&display_rect.width).and_then(|x| Some(*x)).unwrap_or(0.0);
        let height = self.solved_values.get(&display_rect.height).and_then(|x| Some(*x)).unwrap_or(0.0);

        TypedRect::new(TypedPoint2D::new(left as f32, top as f32), TypedSize2D::new(width as f32, height as f32))
    }

    // TODO: This should use the root, not the
    pub(crate) fn get_root_rect_constraints(&self) -> Option<RectConstraintVariables> {
        self.get_rect_constraints(NodeId::new(0))
    }

    pub(crate) fn get_rect_constraints(&self, rect_id: NodeId) -> Option<RectConstraintVariables> {
        let dom_hash = &self.dom_tree_cache.previous_layout.arena.get(&rect_id)?;
        self.edit_variable_cache.map.get(&dom_hash.data).and_then(|rect| Some(rect.1))
    }

    pub(crate) fn push_added_constraints(&mut self, rect_id: NodeId, constraints: Vec<Constraint>) {
        self.added_constraints.entry(rect_id).or_insert_with(|| Vec::new()).extend(constraints);
    }

    pub(crate) fn clear_all_constraints(&mut self) {
        for entry in self.added_constraints.values() {
            for constraint in entry {
                self.solver.remove_constraint(constraint).unwrap();
            }
        }
        self.added_constraints = BTreeMap::new();
    }

    pub(crate) fn get_window_constraints(&self) -> RootSizeConstraints {
        self.root_constraints
    }
}