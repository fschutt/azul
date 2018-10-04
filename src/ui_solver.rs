use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};
use cassowary::{
    Variable, Solver, Constraint,
    strength::*,
};
use glium::glutin::dpi::{LogicalSize, LogicalPosition};
use webrender::api::LayoutPixel;
use euclid::{TypedRect, TypedPoint2D, TypedSize2D};
use {
    id_tree::{NodeId, Arena},
    ui_description::UiDescription,
    css_parser::LayoutPosition,
    cache::{EditVariableCache, DomTreeCache, DomChangeSet},
    traits::Layout,
    dom::NodeData,
    display_list::DisplayRectangle,
};

const LAST_DOM_ID: AtomicUsize = AtomicUsize::new(0);

/// Counter for uniquely identifying a DOM solver -
/// one DOM solver carries all the variables for one DOM, so that
/// two DOMs don't accidentally interact with each other.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct DomId(usize);

/// Creates a new, unique DOM ID
pub(crate) fn new_dom_id() -> DomId {
    DomId(LAST_DOM_ID.fetch_add(1, Ordering::SeqCst))
}

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
pub(crate) struct UiSolver {
    /// Several DOM structures can have multiple, nested DOMs (like iframes)
    /// that do not interact with each other at all. While the DOM
    dom_trees: BTreeMap<DomId, DomSolver>,
}

pub(crate) struct DomSolver {
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
    /// Position of the DOM on screen. For the root dom, this will be (0, 0)
    pub position: LogicalPosition,
    pub size: LogicalSize,
}

impl DomSolver {
    pub(crate) fn new(solver: &mut Solver, size: LogicalSize, position: LogicalPosition) -> Self {
        Self {
            solver: Solver::new(),
            added_constraints: BTreeMap::new(),
            solved_values: BTreeMap::new(),
            root_constraints: RootSizeConstraints::new(&mut solver, size),
            edit_variable_cache: EditVariableCache::empty(),
            dom_tree_cache: DomTreeCache::empty(),
            position, size,
        }
    }

    pub(crate) fn update_dom<T: Layout>(&mut self, ui_descr: &UiDescription<T>) -> DomChangeSet {
        let root =  &ui_descr.ui_descr_root;
        let arena = &*(ui_descr.ui_descr_arena.borrow());
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

    pub(crate) fn create_layout_constraints<'a, T: Layout>(
        &self,
        rect_id: NodeId,
        display_rectangles: &Arena<DisplayRectangle<'a>>,
        dom: &Arena<NodeData<T>>)
    -> Vec<Constraint>
    {
        create_layout_constraints(&self, rect_id, display_rectangles, dom)
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

impl UiSolver {

    pub(crate) fn new() -> Self {
        Self {
            dom_trees: BTreeMap::new(),
        }
    }

    pub(crate) fn insert_dom(&mut self, id: DomId, solver: DomSolver) {
        self.dom_trees.insert(id, solver);
    }

    pub(crate) fn remove_dom(&mut self, id: &DomId) {
        self.dom_trees.remove(id);
    }

    pub(crate) fn get_dom_ref(&self, id: &DomId) -> Option<&DomSolver> {
        self.dom_trees.get(id)
    }

    pub(crate) fn get_dom_mut(&mut self, id: &DomId) -> Option<&mut DomSolver> {
        self.dom_trees.get_mut(id)
    }
}


// Returns the constraints for one rectangle
fn create_layout_constraints<'a, T: Layout>(
    ui_solver: &DomSolver,
    rect_id: NodeId,
    display_rectangles: &Arena<DisplayRectangle<'a>>,
    dom: &Arena<NodeData<T>>)
-> Vec<Constraint>
{
    use cassowary::{
        WeightedRelation::{EQ, GE, LE},
    };
    use ui_solver::RectConstraintVariables;
    use std::f64;
    use css_parser::LayoutDirection::*;

    const WEAK: f64 = 3.0;
    const MEDIUM: f64 = 30.0;
    const STRONG: f64 = 300.0;
    const REQUIRED: f64 = f64::MAX;

    let rect = &display_rectangles[rect_id].data;
    let self_rect = ui_solver.get_rect_constraints(rect_id).unwrap();

    let dom_node = &dom[rect_id];

    let mut layout_constraints = Vec::new();

    let window_constraints = ui_solver.get_window_constraints();

    // Insert the max height and width constraints
    //
    // min-width and max-width are stronger than width because
    // the width has to be between min and max width

    // min-width, width, max-width
    if let Some(min_width) = rect.layout.min_width {
        layout_constraints.push(self_rect.width | GE(REQUIRED) | min_width.0.to_pixels());
    }
    if let Some(width) = rect.layout.width {
        layout_constraints.push(self_rect.width | EQ(STRONG) | width.0.to_pixels());
    } else {
        if let Some(parent) = dom_node.parent {
            // If the parent has a flex-direction: row, divide the
            // preferred width by the number of children
            let parent_rect = ui_solver.get_rect_constraints(parent).unwrap();
            let parent_direction = &display_rectangles[parent].data.layout.direction.unwrap_or_default();
            match parent_direction {
                Row | RowReverse => {
                    let num_children = parent.children(dom).count();
                    layout_constraints.push(self_rect.width | EQ(STRONG) | parent_rect.width / (num_children as f32));
                    layout_constraints.push(self_rect.width | EQ(WEAK) | parent_rect.width);
                },
                Column | ColumnReverse => {
                    layout_constraints.push(self_rect.width | EQ(STRONG) | parent_rect.width);
                }
            }
        } else {
            layout_constraints.push(self_rect.width | EQ(REQUIRED) | window_constraints.width_var);
        }
    }
    if let Some(max_width) = rect.layout.max_width {
        layout_constraints.push(self_rect.width | LE(REQUIRED) | max_width.0.to_pixels());
    }

    // min-height, height, max-height
    if let Some(min_height) = rect.layout.min_height {
        layout_constraints.push(self_rect.height | GE(REQUIRED) | min_height.0.to_pixels());
    }
    if let Some(height) = rect.layout.height {
        layout_constraints.push(self_rect.height | EQ(STRONG) | height.0.to_pixels());
    } else {
        if let Some(parent) = dom_node.parent {
            // If the parent has a flex-direction: column, divide the
            // preferred height by the number of children
            let parent_rect = ui_solver.get_rect_constraints(parent).unwrap();
            let parent_direction = &display_rectangles[parent].data.layout.direction.unwrap_or_default();
            match parent_direction {
                Row | RowReverse => {
                    layout_constraints.push(self_rect.height | EQ(STRONG) | parent_rect.height);
                },
                Column | ColumnReverse => {
                    let num_children = parent.children(dom).count();
                    layout_constraints.push(self_rect.height | EQ(STRONG) | parent_rect.height / (num_children as f32));
                    layout_constraints.push(self_rect.height | EQ(WEAK) | parent_rect.height);
                }
            }
        } else {
            layout_constraints.push(self_rect.height | EQ(REQUIRED) | window_constraints.height_var);
        }
    }
    if let Some(max_height) = rect.layout.max_height {
        layout_constraints.push(self_rect.height | LE(REQUIRED) | max_height.0.to_pixels());
    }

    // root node: start at (0, 0)
    if dom_node.parent.is_none() {
        layout_constraints.push(self_rect.top | EQ(REQUIRED) | 0.0);
        layout_constraints.push(self_rect.left | EQ(REQUIRED) | 0.0);
    }

    // Node has children: Push the constraints for `flex-direction`
    if dom_node.first_child.is_some() {

        let direction = rect.layout.direction.unwrap_or_default();

        let mut next_child_id = dom_node.first_child;
        let mut previous_child: Option<RectConstraintVariables> = None;

        // Iterate through children
        while let Some(child_id) = next_child_id {

            let child = &display_rectangles[child_id].data;
            let child_rect = ui_solver.get_rect_constraints(child_id).unwrap();

            let should_respect_relative_positioning = child.layout.position == Some(LayoutPosition::Relative);

            let (relative_top, relative_left, relative_right, relative_bottom) = if should_respect_relative_positioning {(
                child.layout.top.and_then(|top| Some(top.0.to_pixels())).unwrap_or(0.0),
                child.layout.left.and_then(|left| Some(left.0.to_pixels())).unwrap_or(0.0),
                child.layout.right.and_then(|right| Some(right.0.to_pixels())).unwrap_or(0.0),
                child.layout.right.and_then(|bottom| Some(bottom.0.to_pixels())).unwrap_or(0.0),
            )} else {
                (0.0, 0.0, 0.0, 0.0)
            };

            match direction {
                Row => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.left | EQ(MEDIUM) | self_rect.left + relative_left),
                        Some(prev) => layout_constraints.push(child_rect.left | EQ(MEDIUM) | (prev.left + prev.width) + relative_left),
                    }
                    layout_constraints.push(child_rect.top | EQ(MEDIUM) | self_rect.top);
                },
                RowReverse => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.left | EQ(MEDIUM) | (self_rect.left  + relative_left + (self_rect.width - child_rect.width))),
                        Some(prev) => layout_constraints.push((child_rect.left + child_rect.width) | EQ(MEDIUM) | prev.left + relative_left),
                    }
                    layout_constraints.push(child_rect.top | EQ(MEDIUM) | self_rect.top);
                },
                Column => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.top | EQ(MEDIUM) | self_rect.top),
                        Some(prev) => layout_constraints.push(child_rect.top | EQ(MEDIUM) | (prev.top + prev.height)),
                    }
                    layout_constraints.push(child_rect.left | EQ(MEDIUM) | self_rect.left + relative_left);
                },
                ColumnReverse => {
                    match previous_child {
                        None => layout_constraints.push(child_rect.top | EQ(MEDIUM) | (self_rect.top + (self_rect.height - child_rect.height))),
                        Some(prev) => layout_constraints.push((child_rect.top + child_rect.height) | EQ(MEDIUM) | prev.top),
                    }
                    layout_constraints.push(child_rect.left | EQ(MEDIUM) | self_rect.left + relative_left);
                },
            }

            previous_child = Some(child_rect);
            next_child_id = dom[child_id].next_sibling;
        }
    }

    // Handle position: absolute
    if let Some(LayoutPosition::Absolute) = rect.layout.position {

        let top = rect.layout.top.and_then(|top| Some(top.0.to_pixels())).unwrap_or(0.0);
        let left = rect.layout.left.and_then(|left| Some(left.0.to_pixels())).unwrap_or(0.0);
        let right = rect.layout.right.and_then(|right| Some(right.0.to_pixels())).unwrap_or(0.0);
        let bottom = rect.layout.right.and_then(|bottom| Some(bottom.0.to_pixels())).unwrap_or(0.0);

        match get_nearest_positioned_ancestor(rect_id, display_rectangles) {
            None => {
                // window is the nearest positioned ancestor
                // TODO: hacky magic that relies on having one root element
                let window_id = ui_solver.get_rect_constraints(NodeId::new(0)).unwrap();
                layout_constraints.push(self_rect.top | EQ(REQUIRED) | window_id.top + top);
                layout_constraints.push(self_rect.left | EQ(REQUIRED) | window_id.left + left);
            },
            Some(nearest_positioned) => {
                let nearest_positioned = ui_solver.get_rect_constraints(nearest_positioned).unwrap();
                layout_constraints.push(self_rect.top | GE(STRONG) | nearest_positioned.top + top);
                layout_constraints.push(self_rect.left | GE(STRONG) | nearest_positioned.left + left);
            }
        }
    }

    layout_constraints
}

/// Returns the nearest common ancestor with a `position: relative` attribute
/// or `None` if there is no ancestor that has `position: relative`. Usually
/// used in conjunction with `position: absolute`
fn get_nearest_positioned_ancestor<'a>(start_node_id: NodeId, arena: &Arena<DisplayRectangle<'a>>)
-> Option<NodeId>
{
    let mut current_node = start_node_id;
    while let Some(parent) = arena[current_node].parent() {
        // An element with position: absolute; is positioned relative to the nearest
        // positioned ancestor (instead of positioned relative to the viewport, like fixed).
        //
        // A "positioned" element is one whose position is anything except static.
        if let Some(LayoutPosition::Static) = arena[parent].data.layout.position {
            current_node = parent;
        } else {
            return Some(parent);
        }
    }
    None
}