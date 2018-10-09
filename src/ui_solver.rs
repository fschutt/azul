use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
    f32,
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
    css_parser::{LayoutPosition, RectLayout},
    cache::{EditVariableCache, DomTreeCache, DomChangeSet},
    traits::Layout,
    dom::NodeData,
    display_list::DisplayRectangle,
};

/// Reserve the 0th DOM ID for the windows root DOM
pub(crate) const TOP_LEVEL_DOM_ID: DomId = DomId(0);
/// Since we reserved the root DOM ID at 0, we have to start at 1, not at 0
static LAST_DOM_ID: AtomicUsize = AtomicUsize::new(1);

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
    position: LogicalPosition,
    size: LogicalSize,
}

impl DomSolver {

    pub(crate) fn new(position: LogicalPosition, size: LogicalSize) -> Self {
        let mut solver = Solver::new();
        let root_constraints = RootSizeConstraints::new(&mut solver, size);
        Self {
            solver,
            added_constraints: BTreeMap::new(),
            solved_values: BTreeMap::new(),
            root_constraints,
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

    pub(crate) fn insert_css_constraints(&mut self, constraints: Vec<Constraint>) {
        // TODO: Solver currently locks up here when inserting 5000 constraints
        self.solver.add_constraints(&constraints).unwrap();
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

    /// Queries the bounds of the rectangle at the ID.
    ///
    /// **NOTE**: Automatically applies the `self.position` offset to the rectangle,
    /// so that the resulting rectangle can be directly pushed into the display list!
    pub(crate) fn query_bounds_of_rect(&self, rect_id: NodeId) -> TypedRect<f32, LayoutPixel> {

        let display_rect = self.get_rect_constraints(rect_id).unwrap();

        let origin_position = &self.position;

        let top = self.solved_values.get(&display_rect.top).and_then(|x| Some(*x)).unwrap_or(0.0) + origin_position.y;
        let left = self.solved_values.get(&display_rect.left).and_then(|x| Some(*x)).unwrap_or(0.0) + origin_position.x;
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

    /// TODO: Make this an iterator, so we can avoid the unnecessary collections!
    pub(crate) fn create_layout_constraints<'a, T: Layout>(
        &self,
        rect_id: NodeId,
        display_rectangles: &Arena<DisplayRectangle<'a>>,
        dom: &Arena<NodeData<T>>)
    -> Vec<Constraint>
    {
        create_layout_constraints(&self, rect_id, display_rectangles, dom)
    }

    /// For tracking which constraints are actually in the solver, we need to track what
    /// the added constraints are
    pub(crate) fn push_added_constraints(&mut self, rect_id: NodeId, constraints: Vec<Constraint>) {
        self.added_constraints.entry(rect_id).or_insert_with(|| Vec::new()).extend(constraints);
    }

    /// Clears all the constraints, but **not the edit variables**!
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

    /*
    let preferred_width = determine_preferred_width(&rect.layout);
    let preferred_height = determine_preferred_height(&rect.layout);
    */

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

#[derive(Debug, Copy, Clone, PartialEq)]
enum WhConstraint {
    /// between min, max, Prefer::Max | Prefer::Min
    Between(f32, f32, WhPrefer),
    /// Value needs to be exactly X
    EqualTo(f32),
    /// Value can be anything
    Unconstrained,
}

impl Default for WhConstraint {
    fn default() -> Self {
        WhConstraint::Unconstrained
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum WhPrefer {
    Max,
    Min,
}

impl WhConstraint {

    /// Returns the actual value of the constraint
    pub fn actual_value(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(min, max, prefer) => match prefer { WhPrefer::Min => Some(*min), WhPrefer::Max => Some(*max) },
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }

    /// Returns the minimum value or 0 on `Unconstrained`
    /// (warning: this might not be what you want)
    pub fn min_needed_space(&self) -> f32 {
        self.actual_value().unwrap_or(0.0)
    }

    /// Returns the maximum space until the constraint is violated - returns
    /// `None` if the constraint is unbounded
    pub fn max_available_space(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(_, max, _) => { Some(*max) },
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }
}

macro_rules! determine_preferred {
    ($fn_name:ident, $width:ident, $min_width:ident, $max_width:ident) => (
    fn $fn_name(layout: &RectLayout) -> WhConstraint {

        let width = layout.$width.and_then(|w| Some(w.0.to_pixels()));
        let min_width = layout.$min_width.and_then(|w| Some(w.0.to_pixels()));
        let max_width = layout.$max_width.and_then(|w| Some(w.0.to_pixels()));

        // TODO: correct for width / height less than 0 - "negative" width is impossible!

        let (absolute_min, absolute_max) = {
            if let (Some(min), Some(max)) = (min_width, max_width) {
                if min_width < max_width {
                    (Some(min), Some(max))
                } else {
                    // min-width > max_width: max_width wins
                    (Some(max), Some(max))
                }
            } else {
                (min_width, max_width)
            }
        };

        if let Some(width) = width {
            if let Some(max_width) = absolute_max {
                if let Some(min_width) = absolute_min {
                    if min_width < width && width < max_width {
                        // normal: min_width < width < max_width
                        WhConstraint::EqualTo(width)
                    } else if width > max_width {
                        WhConstraint::EqualTo(max_width)
                    } else if width < min_width {
                        WhConstraint::EqualTo(min_width)
                    } else {
                        WhConstraint::Unconstrained /* unreachable */
                    }
                } else {
                    // width & max_width
                    WhConstraint::EqualTo(width.min(max_width))
                }
            } else if let Some(min_width) = absolute_min {
                // no max width, only width & min_width
                WhConstraint::EqualTo(width.max(min_width))
            } else {
                // no min-width or max-width
                WhConstraint::EqualTo(width)
            }
        } else {
            // no width, only min_width and max_width
            if let Some(max_width) = absolute_max {
                if let Some(min_width) = absolute_min {
                    WhConstraint::Between(min_width, max_width, WhPrefer::Min)
                } else {
                    // TODO: check sign positive on max_width!
                    WhConstraint::Between(0.0, max_width, WhPrefer::Min)
                }
            } else {
                if let Some(min_width) = absolute_min {
                    WhConstraint::Between(min_width, f32::MAX, WhPrefer::Min)
                } else {
                    // no width, min_width or max_width
                    WhConstraint::Unconstrained
                }
            }
        }
    })
}

use css_parser::{LayoutMargin, LayoutPadding};

#[derive(Debug, Copy, Clone)]
struct WidthCalculatedRect {
    pub preferred_width: WhConstraint,
    pub preferred_height: WhConstraint,
    pub margin: LayoutMargin,
    pub padding: LayoutPadding,
    pub flex_grow_px: f32,
}

impl WidthCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis(&self) -> FlexBasisHorizontal {
        FlexBasisHorizontal {
            min_width: self.preferred_width.min_needed_space(),
            self_margin_left: self.margin.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0),
            self_margin_right: self.margin.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0),
            self_padding_left: self.padding.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0),
            self_padding_right: self.padding.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0),
        }
    }

    // Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self) -> f32 {
          self.padding.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
        + self.padding.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
    }
}

#[derive(Debug, Copy, Clone)]
struct FlexBasisHorizontal {
    pub min_width: f32,
    pub self_margin_left: f32,
    pub self_margin_right: f32,
    pub self_padding_right: f32,
    pub self_padding_left: f32,
}

impl FlexBasisHorizontal {
    /// Total flex basis in the horizontal direction (sum of the components)
    pub fn total(&self) -> f32 {
        self.min_width +
        self.self_margin_left +
        self.self_margin_right +
        self.self_padding_left +
        self.self_padding_right
    }
}

/// Returns the sum of the flex-basis of the current nodes' children
fn sum_children_flex_basis<'a>(
    node_id: NodeId,
    arena: &Arena<WidthCalculatedRect>,
    display_arena: &Arena<DisplayRectangle<'a>>)
-> f32
{
    let mut current_min_width = 0.0;

    // Sum up the flex-basis width of the nodes children
    for child_node_id in node_id.children(arena) {
        if display_arena[child_node_id].data.layout.position == Some(LayoutPosition::Absolute) {
            current_min_width += arena[child_node_id].data.get_flex_basis().total();
        }
    }

    current_min_width
}

/// Fill out the preferred width of all nodes
fn fill_out_preferred_width<'a>(arena: &Arena<DisplayRectangle<'a>>)
-> Arena<WidthCalculatedRect>
{
    arena.transform(|node, _| {
        WidthCalculatedRect {
            preferred_width: determine_preferred_width(&node.layout),
            preferred_height: determine_preferred_height(&node.layout),
            margin: node.layout.margin.unwrap_or_default(),
            padding: node.layout.padding.unwrap_or_default(),
            flex_grow_px: 0.0,
        }
    })
}

/// On any parent nodes, fill out the width so that the `preferred_width` can contain the
/// child nodes (if that doesn't violate the constraints of the parent)
#[must_use]
fn bubble_preferred_widths_to_parents<'a>(
    arena: &Arena<DisplayRectangle<'a>>,
    leaf_nodes_populated: &mut Arena<WidthCalculatedRect>)
-> Vec<(usize, NodeId)>
{
    // This is going to be a bit slow, but we essentially need to "bubble" the sizes from the leaf
    // nodes to the parent nodes. So first we collect the IDs of all non-leaf nodes and then
    // sort them by their depth.

    // This is so that we can substitute the flex-basis sizes from the inside out
    // since the outer flex-basis depends on the inner flex-basis, so we have to calculate the inner-most sizes first.

    let mut non_leaf_nodes: Vec<(usize, NodeId)> =
        arena.nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| if node.first_child.is_some() { Some(idx) } else { None })
        .map(|non_leaf_id| {
            let non_leaf_id = NodeId::new(non_leaf_id);
            (leaf_node_depth(&non_leaf_id, &arena), non_leaf_id)
        })
        .collect();

    // Sort the non-leaf nodes by their depth
    non_leaf_nodes.sort_by(|a, b| a.0.cmp(&b.0));

    // Reverse, since we want to go from the inside out (depth 5 needs to be filled out first)
    //
    // Set the preferred_width of the parent nodes
    for (_node_depth, non_leaf_id) in non_leaf_nodes.iter().rev() {

        use self::WhConstraint::*;

        // Sum of the direct childrens flex-basis = the parents preferred width
        let children_flex_basis = sum_children_flex_basis(*non_leaf_id, leaf_nodes_populated, arena);

        // Calculate the new flex-basis width
        let parent_width_metrics = leaf_nodes_populated[*non_leaf_id].data;

        // For calculating the inner width, subtract the parents padding
        let parent_padding = leaf_nodes_populated[*non_leaf_id].data.get_horizontal_padding();

        // If the children are larger than the parents preferred max-width or smaller
        // than the parents min-width, adjust
        let child_width = match parent_width_metrics.preferred_width {
            Between(min, max, _) => {
                if children_flex_basis > (max - parent_padding)  {
                    max
                } else if children_flex_basis < (min + parent_padding) {
                    min
                } else {
                    children_flex_basis
                }
            },
            EqualTo(exact) => exact - parent_padding,
            Unconstrained => children_flex_basis,
        };

        leaf_nodes_populated[*non_leaf_id].data.preferred_width = EqualTo(child_width);
    }

    // Now, the width of all elements should be filled,
    // but they aren't flex-growed or flex-shrinked yet

    non_leaf_nodes
}

const DEFAULT_FLEX_GROW_FACTOR: f32 = 1.0;
const DEFAULT_FLEX_SHRINK_FACTOR: f32 = 1.0;

fn flex_grow_children_width<'a>(
    id: NodeId,
    arena: &Arena<DisplayRectangle<'a>>,
    leaf_nodes_populated: &mut Arena<WidthCalculatedRect>,
    overflow: f32)
{
    // Assert that this function gets called on a node that has children
    debug_assert!(arena[id].first_child.is_some());
    // Overflow must be negative, otherwise flex-shrink doesn't apply
    debug_assert!(!overflow.is_sign_positive());

    let children_flex_grow_factor: f32 = id.children(arena).map(|child_id| {
        arena[child_id].data.layout.flex_grow.and_then(|grow| Some(grow.0)).unwrap_or(DEFAULT_FLEX_GROW_FACTOR)
    }).sum();

    // borrowing problem - can't borrow leaf_nodes_populated although it would be safe
    let mut children_to_change = Vec::new();

    for child_id in id.children(leaf_nodes_populated) {
        let flex_grow = arena[child_id].data.layout.flex_grow.and_then(|grow| Some(grow.0)).unwrap_or(DEFAULT_FLEX_GROW_FACTOR);
        let flex_grow_px = overflow * (flex_grow / children_flex_grow_factor);
        children_to_change.push((child_id, flex_grow_px));
    }

    for (child_id, flex_grow_px) in children_to_change {
        leaf_nodes_populated[child_id].data.flex_grow_px = flex_grow_px;
    }
}

fn flex_shrink_children_width<'a>(
    id: NodeId,
    arena: &Arena<DisplayRectangle<'a>>,
    leaf_nodes_populated: &mut Arena<WidthCalculatedRect>,
    overflow: f32)
{

    // Assert that this function gets called on a node that has children
    debug_assert!(arena[id].first_child.is_some());
    // Overflow must be negative, otherwise flex-shrink doesn't apply
    debug_assert!(overflow.is_sign_positive());

    let children_combined_flex_basis: f32 = id.children(arena)
        .map(|child_id| leaf_nodes_populated[child_id].data.get_flex_basis().total())
        .sum();

    // borrowing problem
    let mut children_to_change = Vec::new();

    for child_id in id.children(leaf_nodes_populated) {
        let flex_shrink = arena[child_id].data.layout.flex_shrink
            .and_then(|shrink| Some(shrink.0))
            .unwrap_or(DEFAULT_FLEX_SHRINK_FACTOR);

        let flex_basis = leaf_nodes_populated[child_id].data.get_flex_basis().total(); // can be 0
        let flex_shrink_px = overflow * ((flex_shrink * flex_basis) / children_combined_flex_basis);
        children_to_change.push((child_id, flex_shrink_px));
    }

    for (child_id, flex_shrink_px) in children_to_change {
        leaf_nodes_populated[child_id].data.flex_grow_px = flex_shrink_px;
    }
}

// flex-grow or flex-shrink one parent nodes children
fn apply_flex_grow_or_shrink_to_parent_node<'a>(
    id: NodeId,
    arena: &Arena<DisplayRectangle<'a>>,
    leaf_nodes_populated: &mut Arena<WidthCalculatedRect>)
{
    // Assert that this function gets called on a node that has children
    debug_assert!(arena[id].first_child.is_some());

    // Overflow = The inner value of the self item - must be unwrapped first - meaning on the self item,
    // the item must be `EqualTo`!
    let mut self_inner_width = if let WhConstraint::EqualTo(exact) = leaf_nodes_populated[id].data.preferred_width {
        exact
    } else {
        panic!("EqualTo wasn't set on the parent, no way to resolve the childs width");
    };

    // Since we go from outer-to-inner now, the flex_grow_px has to be respected
    self_inner_width += leaf_nodes_populated[id].data.flex_grow_px;

    // Re-calculate the necessary width of the children items
    let children_combined_flex_basis: f32 = id.children(arena)
        .map(|child_id| leaf_nodes_populated[child_id].data.get_flex_basis().total())
        .sum();

    let overflow: f32 = self_inner_width - children_combined_flex_basis;
    if overflow == 0.0 { return; }
    if overflow.is_sign_positive() {
        // Space is available, grow children of this node
        flex_grow_children_width(id, arena, leaf_nodes_populated, overflow);
    } else {
        // Space has to be removed, shrink children of this node
        flex_shrink_children_width(id, arena, leaf_nodes_populated, overflow);
    }
}

fn apply_flex_grow_or_shrink<'a>(
    arena: &Arena<DisplayRectangle<'a>>,
    leaf_nodes_populated: &mut Arena<WidthCalculatedRect>,
    parent_ids_sorted_by_depths: &[(usize, NodeId)],
    window_width: f32)
{
    // If the root node isn't constrained (which is likely the case),
    // set it to equal the window's width
    leaf_nodes_populated[NodeId::new(0)].data.preferred_width = WhConstraint::EqualTo(window_width);
    leaf_nodes_populated[NodeId::new(0)].data.flex_grow_px = 0.0;
    leaf_nodes_populated[NodeId::new(0)].data.margin = LayoutMargin::default();
    leaf_nodes_populated[NodeId::new(0)].data.padding = LayoutPadding::default();

    apply_flex_grow_or_shrink_to_parent_node(NodeId::new(0), arena, leaf_nodes_populated);

    for (_node_depth, parent_id) in parent_ids_sorted_by_depths {
        apply_flex_grow_or_shrink_to_parent_node(*parent_id, arena, leaf_nodes_populated);
    }
}

pub(crate) struct WidthSolvedResult {
    pub width: f32,
    pub space_added: f32,
}

impl WidthSolvedResult {
    pub fn total(&self) -> f32 {
        self.width + self.space_added
    }
}

/// Returns the solved widths of the items in a BTree form
pub(crate) fn solve_flex_layout_width<'a>(
    display_rectangles: &Arena<DisplayRectangle<'a>>,
    window_width: f32)
-> BTreeMap<NodeId, WidthSolvedResult>
{
    // Create the window widths from the arena
    let mut width_calculated_arena = fill_out_preferred_width(display_rectangles);

    // Bubble the inner sizes to their parents
    let non_leaf_nodes_sorted_by_depth = bubble_preferred_widths_to_parents(&display_rectangles, &mut width_calculated_arena);

    // Go from the root down and stretch or shrink the children if they overflow
    apply_flex_grow_or_shrink(&display_rectangles, &mut width_calculated_arena, &non_leaf_nodes_sorted_by_depth, window_width);

    // Calculate the final size and return the solution
    let mut width_btree = BTreeMap::new();

    for node_id in width_calculated_arena.linear_iter() {
        // TODO: Put this in a function and incorporate the flex-start / flex-end / stretch, etc.
        let total_adjusted_width =
        width_btree.insert(node_id, WidthSolvedResult {
            width: width_calculated_arena[node_id].data.preferred_width.min_needed_space(),
            space_added: width_calculated_arena[node_id].data.flex_grow_px,
        });
    }

    width_btree
}


/// Traverses from arena[id] to the root, returning the amount of parents, i.e. the depth of the node in the tree.
fn leaf_node_depth<T>(id: &NodeId, arena: &Arena<T>) -> usize {
    let mut counter = 0;
    let mut last_id = *id;

    while let Some(parent) = arena[last_id].parent {
        last_id = parent;
        counter += 1;
    }

    counter
}


/// Returns the preferred width, given [width, min_width, max_width] inside a RectLayout
/// or `None` if the height can't be determined from the node alone.
///
// fn determine_preferred_width(layout: &RectLayout) -> Option<f32>
determine_preferred!(determine_preferred_width, width, min_width, max_width);

/// Returns the preferred height, given [height, min_height, max_height] inside a RectLayout
// or `None` if the height can't be determined from the node alone.
///
// fn determine_preferred_height(layout: &RectLayout) -> Option<f32>
determine_preferred!(determine_preferred_height, height, min_height, max_height);


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

#[test]
fn test_determine_preferred_width() {
    use css_parser::{LayoutMinWidth, LayoutMaxWidth, PixelValue, LayoutWidth};

    let layout = RectLayout {
        width: None,
        min_width: None,
        max_width: None,
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::Unconstrained);

    let layout = RectLayout {
        width: Some(LayoutWidth(PixelValue::px(500.0))),
        min_width: None,
        max_width: None,
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::EqualTo(500.0));

    let layout = RectLayout {
        width: Some(LayoutWidth(PixelValue::px(500.0))),
        min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
        max_width: None,
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::EqualTo(600.0));

    let layout = RectLayout {
        width: Some(LayoutWidth(PixelValue::px(10000.0))),
        min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
        max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::EqualTo(800.0));

    let layout = RectLayout {
        width: None,
        min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
        max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::Between(600.0, 800.0, WhPrefer::Min));

    let layout = RectLayout {
        width: None,
        min_width: None,
        max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::Between(0.0, 800.0, WhPrefer::Min));

    let layout = RectLayout {
        width: Some(LayoutWidth(PixelValue::px(1000.0))),
        min_width: None,
        max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::EqualTo(800.0));

    let layout = RectLayout {
        width: Some(LayoutWidth(PixelValue::px(1200.0))),
        min_width: Some(LayoutMinWidth(PixelValue::px(1000.0))),
        max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::EqualTo(800.0));

    let layout = RectLayout {
        width: Some(LayoutWidth(PixelValue::px(1200.0))),
        min_width: Some(LayoutMinWidth(PixelValue::px(1000.0))),
        max_width: Some(LayoutMaxWidth(PixelValue::px(400.0))),
        .. Default::default()
    };
    assert_eq!(determine_preferred_width(&layout), WhConstraint::EqualTo(400.0));
}

#[test]
fn test_new_ui_solver_has_root_constraints() {
    let mut solver = DomSolver::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(400.0, 600.0));
    assert!(solver.solver.suggest_value(solver.root_constraints.width_var, 400.0).is_ok());
    assert!(solver.solver.suggest_value(solver.root_constraints.width_var, 600.0).is_ok());
}