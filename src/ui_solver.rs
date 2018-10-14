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
use webrender::api::{LayoutRect, LayoutSize, LayoutPoint};
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
    pub(crate) fn query_bounds_of_rect(&self, rect_id: NodeId) -> LayoutRect {

        let display_rect = self.get_rect_constraints(rect_id).unwrap();

        let origin_position = &self.position;

        let top = self.solved_values.get(&display_rect.top).and_then(|x| Some(*x)).unwrap_or(0.0) + origin_position.y;
        let left = self.solved_values.get(&display_rect.left).and_then(|x| Some(*x)).unwrap_or(0.0) + origin_position.x;
        let width = self.solved_values.get(&display_rect.width).and_then(|x| Some(*x)).unwrap_or(0.0);
        let height = self.solved_values.get(&display_rect.height).and_then(|x| Some(*x)).unwrap_or(0.0);

        LayoutRect::new(LayoutPoint::new(left as f32, top as f32), LayoutSize::new(width as f32, height as f32))
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
        /*
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
        */
    }

    layout_constraints
}

// -------------------------------- New layout system, no cassowary ------------------------------- //

const DEFAULT_FLEX_GROW_FACTOR: f32 = 1.0;

#[derive(Debug, Copy, Clone, PartialEq)]
enum WhConstraint {
    /// between min, max
    Between(f32, f32),
    /// Value needs to be exactly X
    EqualTo(f32),
    /// Value can be anything
    Unconstrained,
}

impl WhConstraint {

    /// Returns the minimum value or 0 on `Unconstrained`
    /// (warning: this might not be what you want)
    pub fn min_needed_space(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(min, _) => Some(*min),
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }

    /// Returns the maximum space until the constraint is violated - returns
    /// `None` if the constraint is unbounded
    pub fn max_available_space(&self) -> Option<f32> {
        use self::WhConstraint::*;
        match self {
            Between(_, max) => { Some(*max) },
            EqualTo(exact) => Some(*exact),
            Unconstrained => None,
        }
    }

    /// Returns if this `WhConstraint` is an `EqualTo` constraint
    pub fn is_fixed_constraint(&self) -> bool {
        use self::WhConstraint::*;
        match self {
            EqualTo(_) => true,
            _ => false,
        }
    }
}

macro_rules! determine_preferred {
    ($fn_name:ident, $width:ident, $min_width:ident, $max_width:ident) => (

    /// - `preferred_inner_width` denotes the preferred width of the width or height got from the
    /// from the rectangles content.
    ///
    /// For example, if you have an image, the `preferred_inner_width` is the images width,
    /// if the node type is an text, the `preferred_inner_width` is the text height.
    fn $fn_name(layout: &RectLayout, preferred_inner_width: Option<f32>) -> WhConstraint {

        let mut width = layout.$width.and_then(|w| Some(w.0.to_pixels()));
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

        // We only need to correct the width if the preferred width is in
        // the range between min & max and the width isn't already specified in CSS
        if let Some(preferred_width) = preferred_inner_width {
            if width.is_none() &&
               preferred_width > absolute_min.unwrap_or(0.0) &&
               preferred_width < absolute_max.unwrap_or(f32::MAX)
            {
                width = Some(preferred_width);
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
                    WhConstraint::Between(min_width, max_width)
                } else {
                    // TODO: check sign positive on max_width!
                    WhConstraint::Between(0.0, max_width)
                }
            } else {
                if let Some(min_width) = absolute_min {
                    WhConstraint::Between(min_width, f32::MAX)
                } else {
                    // no width, min_width or max_width
                    WhConstraint::Unconstrained
                }
            }
        }
    })
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

use css_parser::{LayoutMargin, LayoutPadding};

#[derive(Debug, Copy, Clone, PartialEq)]
struct WidthCalculatedRect {
    pub preferred_width: WhConstraint,
    pub margin: LayoutMargin,
    pub padding: LayoutPadding,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl WidthCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_horizontal(&self) -> f32 {
        self.preferred_width.min_needed_space().unwrap_or(0.0) +
        self.margin.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.margin.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.left + padding.right`)
    pub fn get_horizontal_padding(&self) -> f32 {
        self.padding.left.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.right.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn solved_result(&self) -> WidthSolvedResult {
        WidthSolvedResult {
            min_width: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct HeightCalculatedRect {
    pub preferred_height: WhConstraint,
    pub margin: LayoutMargin,
    pub padding: LayoutPadding,
    pub flex_grow_px: f32,
    pub min_inner_size_px: f32,
}

impl HeightCalculatedRect {
    /// Get the flex basis in the horizontal direction - vertical axis has to be calculated differently
    pub fn get_flex_basis_vertical(&self) -> f32 {
        self.preferred_height.min_needed_space().unwrap_or(0.0) +
        self.margin.top.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.margin.bottom.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.top.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.bottom.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
    }

    /// Get the sum of the horizontal padding amount (`padding.top + padding.bottom`)
    pub fn get_vertical_padding(&self) -> f32 {
        self.padding.top.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0) +
        self.padding.bottom.and_then(|px| Some(px.to_pixels())).unwrap_or(0.0)
    }

    /// Called after solver has run: Solved width of rectangle
    pub fn solved_result(&self) -> HeightSolvedResult {
        HeightSolvedResult {
            min_height: self.min_inner_size_px,
            space_added: self.flex_grow_px,
        }
    }
}

// `typed_arena!(WidthCalculatedRect, preferred_width, determine_preferred_width, get_horizontal_padding, get_flex_basis_horizontal)`
macro_rules! typed_arena {(
    $struct_name:ident,
    $preferred_field:ident,
    $determine_preferred_fn:ident,
    $get_padding_fn:ident,
    $get_flex_basis:ident,
    $bubble_fn_name:ident
) => (

impl Arena<$struct_name> {

    /// Fill out the preferred width of all nodes.
    ///
    /// We could operate on the Arena<DisplayRectangle> directly, but that makes testing very
    /// hard since we are only interested in testing or touching the layout. So this makes the
    /// calculation maybe a few microseconds slower, but gives better testing capabilities
    ///
    /// NOTE: Later on, this could maybe be a Arena<&'a RectLayout>.
    #[must_use]
    fn from_rect_layout_arena(arena: &Arena<RectLayout>, widths: Arena<Option<f32>>) -> Self {
        arena.transform(|node, id| {
            $struct_name {
                // TODO: get the initial width of the rect content
                $preferred_field: $determine_preferred_fn(&node, widths[id].data),
                margin: node.margin.unwrap_or_default(),
                padding: node.padding.unwrap_or_default(),
                flex_grow_px: 0.0,
                min_inner_size_px: 0.0,
            }
        })
    }

    /// Bubble the inner sizes to their parents -  on any parent nodes, fill out
    /// the width so that the `preferred_width` can contain the child nodes (if
    /// that doesn't violate the constraints of the parent)
    #[must_use]
    fn $bubble_fn_name(
        &mut self,
        arena: &Arena<RectLayout>)
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
            let children_flex_basis = self.sum_children_flex_basis(*non_leaf_id, arena);

            // Calculate the new flex-basis width
            let parent_width_metrics = self[*non_leaf_id].data;

            // For calculating the inner width, subtract the parents padding
            let parent_padding = self[*non_leaf_id].data.$get_padding_fn();

            // If the children are larger than the parents preferred max-width or smaller
            // than the parents min-width, adjust
            let child_width = match parent_width_metrics.$preferred_field {
                Between(min, max) => {
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

            self[*non_leaf_id].data.min_inner_size_px = child_width;
        }

        // Now, the width of all elements should be filled,
        // but they aren't flex-growed or flex-shrinked yet

        non_leaf_nodes
    }

    /// Go from the root down and flex_grow the children if needed - respects the `width`, `min_width` and `max_width` properties
    /// The layout step doesn't account for the min_width and max_width constraints, so we have to adjust them manually
    fn apply_flex_grow(
        &mut self,
        arena: &Arena<RectLayout>,
        parent_ids_sorted_by_depth: &[(usize, NodeId)],
        root_width: f32)
    {
        /// Does the actual width layout, respects the `width`, `min_width` and `max_width`
        /// properties as well as the `flex_grow` factor. `flex_shrink` currently does nothing.
        fn apply_flex_grow_with_constraints(
            node_id: &NodeId,
            arena: &Arena<RectLayout>,
            width_calculated_arena: &mut Arena<$struct_name>)
        {
            // Function can only be called on parent nodes, not child nodes
            debug_assert!(width_calculated_arena[*node_id].first_child.is_some());

            // The inner space of the parent node, without the padding
            let mut parent_node_inner_width = {
                let parent_node = &width_calculated_arena[*node_id].data;
                parent_node.min_inner_size_px + parent_node.flex_grow_px - parent_node.$get_padding_fn()
            };

            // 1. Set all child elements that have an exact width to that width, record their violations
            //    and add their violation to the leftover horizontal space.
            // let mut horizontal_space_from_fixed_width_items = 0.0;
            let mut horizontal_space_taken_up_by_fixed_width_items = 0.0;

            {
                // Vec<(NodeId, PreferredWidth)>
                let exact_width_childs = node_id
                        .children(width_calculated_arena)
                        .filter_map(|id| if let WhConstraint::EqualTo(exact) = width_calculated_arena[id].data.$preferred_field {
                            Some((id, exact))
                        } else {
                            None
                        })
                        .collect::<Vec<(NodeId, f32)>>();

                for (exact_width_child_id, preferred_width) in exact_width_childs {
                    // horizontal_space_from_fixed_width_items += violation_px;
                    horizontal_space_taken_up_by_fixed_width_items += preferred_width;
                    // so that node.min_inner_size_px + node.flex_grow_px = preferred_width
                    width_calculated_arena[exact_width_child_id].data.flex_grow_px =
                        preferred_width - width_calculated_arena[exact_width_child_id].data.min_inner_size_px;
                }
            }

            // The fixed-width items are now considered solved, so subtract them out of the width of the parent.
            parent_node_inner_width -= horizontal_space_taken_up_by_fixed_width_items;

            // Now we can be sure that if we write #x { width: 500px; } that it will actually be 500px large
            // and not be influenced by flex in any way.

            // 2. Set all items to their minimium width. Record how much space is gained by doing so.
            let mut horizontal_space_taken_up_by_variable_items = 0.0;

            use FastHashSet;

            let mut variable_width_childs = node_id
                .children(width_calculated_arena)
                .filter(|id| !width_calculated_arena[*id].data.$preferred_field.is_fixed_constraint())
                .collect::<FastHashSet<NodeId>>();

            for variable_child_id in &variable_width_childs {
                let min_width = width_calculated_arena[*variable_child_id].data.$preferred_field.min_needed_space().unwrap_or(0.0);
                horizontal_space_taken_up_by_variable_items += min_width;

                // so that node.min_inner_size_px + node.flex_grow_px = min_width
                width_calculated_arena[*variable_child_id].data.flex_grow_px =
                    min_width - width_calculated_arena[*variable_child_id].data.min_inner_size_px;
            }

            // This satisfies the `width` and `min_width` constraints. However, we still need to worry about
            // the `max_width` and unconstrained childs
            //
            // By setting the items to their minimum size, we've gained some space that we now need to distribute
            // according to the flex_grow values
            parent_node_inner_width -= horizontal_space_taken_up_by_variable_items;

            let mut total_horizontal_space_available = parent_node_inner_width;
            let mut max_width_violations = Vec::new();

            loop {

                if total_horizontal_space_available <= 0.0 || variable_width_childs.is_empty() {
                    break;
                }

                // In order to apply flex-grow correctly, we need the sum of
                // the flex-grow factors of all the variable-width children
                //
                // NOTE: variable_width_childs can change its length, have to recalculate every loop!
                let children_combined_flex_grow: f32 = variable_width_childs
                    .iter()
                    .map(|child_id|
                            // Prevent flex-grow and flex-shrink to be less than 1
                            arena[*child_id].data.flex_grow
                                .and_then(|grow| Some(grow.0.max(1.0)))
                                .unwrap_or(DEFAULT_FLEX_GROW_FACTOR))
                    .sum();

                // Grow all variable children by the same amount.
                for variable_child_id in &variable_width_childs {

                    let flex_grow = arena[*variable_child_id].data.flex_grow
                        .and_then(|grow| Some(grow.0.max(1.0)))
                        .unwrap_or(DEFAULT_FLEX_GROW_FACTOR);

                    let added_space_for_one_child = total_horizontal_space_available * (flex_grow / children_combined_flex_grow);

                    let current_width_of_child = width_calculated_arena[*variable_child_id].data.min_inner_size_px +
                                                 width_calculated_arena[*variable_child_id].data.flex_grow_px;

                    if let Some(max_width) = width_calculated_arena[*variable_child_id].data.$preferred_field.max_available_space() {
                        if (current_width_of_child + added_space_for_one_child) > max_width {
                            // so that node.min_inner_size_px + node.flex_grow_px = max_width
                            width_calculated_arena[*variable_child_id].data.flex_grow_px =
                                max_width - width_calculated_arena[*variable_child_id].data.min_inner_size_px;

                            max_width_violations.push(*variable_child_id);
                        } else {
                            // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                            width_calculated_arena[*variable_child_id].data.flex_grow_px =
                                added_space_for_one_child - width_calculated_arena[*variable_child_id].data.min_inner_size_px;
                        }
                    } else {
                        // so that node.min_inner_size_px + node.flex_grow_px = added_space_for_one_child
                        width_calculated_arena[*variable_child_id].data.flex_grow_px =
                            added_space_for_one_child - width_calculated_arena[*variable_child_id].data.min_inner_size_px;
                    }
                }

                // If we haven't violated any max_width constraints, then we have
                // added all remaining widths and thereby successfully solved the layout
                if max_width_violations.is_empty() {
                    break;
                } else {
                    // Nodes that were violated can't grow anymore in the next iteration,
                    // so we remove them from the solution and consider them "solved".
                    // Their amount of violation then gets distributed across the remaining
                    // items in the next iteration.
                    for solved_node_id in max_width_violations.drain(..) {

                        // Since the node now gets removed, it doesn't contribute to the pool anymore
                        total_horizontal_space_available -=
                            width_calculated_arena[solved_node_id].data.min_inner_size_px +
                            width_calculated_arena[solved_node_id].data.flex_grow_px;

                        variable_width_childs.remove(&solved_node_id);
                    }
                }
            }
        }

        debug_assert!(self[NodeId::new(0)].data.flex_grow_px == 0.0);

        // Set the window width on the root node (since there is only one root node, we can
        // calculate the `flex_grow_px` directly)
        //
        // Usually `top_level_flex_basis` is NOT 0.0, rather it's the sum of all widths in the DOM,
        // i.e. the sum of the whole DOM tree
        let top_level_flex_basis = self[NodeId::new(0)].data.min_inner_size_px;
        self[NodeId::new(0)].data.flex_grow_px = root_width - top_level_flex_basis;

        for (_node_depth, parent_id) in parent_ids_sorted_by_depth {
            apply_flex_grow_with_constraints(parent_id, arena, self);
        }
    }

    /// Returns the sum of the flex-basis of the current nodes' children
    fn sum_children_flex_basis(
        &self,
        node_id: NodeId,
        display_arena: &Arena<RectLayout>)
    -> f32
    {
        // Function must be called on a non-leaf node
        debug_assert!(self[node_id].first_child.is_some());

        node_id
            .children(self)
            .filter(|child_node_id| display_arena[*child_node_id].data.position != Some(LayoutPosition::Absolute))
            .map(|child_node_id| self[child_node_id].data.$get_flex_basis())
            .sum()
    }
}

)}

/*
fn apply_cross_axis_stretched(width_calculated_arena: Arena<WidthCalculatedRect>) -> BTree<NodeId, Height> {
    // Function can only be called on parent nodes, not child nodes
    debug_assert!(width_calculated_arena[*node_id].first_child.is_some());
    // We act on a Arena<WidthCalculatedRect> and return an arena of heights that should
    use css_parser::LayoutDirection::*;

    for child_id in node_id.children(width_calculated_arena) {
        if width_calculated_arena[child_id].data.direction == Row | RowReverse {
            // heights of children = this.inner_height
        } else {
            // widths of children = this.inner_width
        }
    }
    // If we are called on the width, apply the height
}
*/

typed_arena!(
    WidthCalculatedRect,
    preferred_width,
    determine_preferred_width,
    get_horizontal_padding,
    get_flex_basis_horizontal,
    bubble_preferred_widths_to_parents
);

typed_arena!(
    HeightCalculatedRect,
    preferred_height,
    determine_preferred_height,
    get_vertical_padding,
    get_flex_basis_vertical,
    bubble_preferred_heights_to_parents);

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct WidthSolvedResult {
    pub min_width: f32,
    pub space_added: f32,
}

impl WidthSolvedResult {
    pub fn total(&self) -> f32 {
        self.min_width + self.space_added
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) struct HeightSolvedResult {
    pub min_height: f32,
    pub space_added: f32,
}

impl HeightSolvedResult {
    pub fn total(&self) -> f32 {
        self.min_height + self.space_added
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SolvedWidthLayout {
    pub solved_widths: Arena<WidthSolvedResult>,
    pub layout_only_arena: Arena<RectLayout>,
}

#[derive(Debug, Clone)]
pub(crate) struct SolvedHeightLayout {
    pub solved_heights: Arena<HeightSolvedResult>,
}

/// Returns the solved widths of the items in a BTree form
pub(crate) fn solve_flex_layout_width<'a>(
    display_rectangles: &Arena<DisplayRectangle<'a>>,
    preferred_widths: Arena<Option<f32>>,
    window_width: f32)
-> SolvedWidthLayout
{
    let layout_only_arena = display_rectangles.transform(|node, _| node.layout);
    let mut width_calculated_arena = Arena::<WidthCalculatedRect>::from_rect_layout_arena(&layout_only_arena, preferred_widths);
    let non_leaf_nodes_sorted_by_depth = width_calculated_arena.bubble_preferred_widths_to_parents(&layout_only_arena);
    width_calculated_arena.apply_flex_grow(&layout_only_arena, &non_leaf_nodes_sorted_by_depth, window_width);
    let solved_widths = width_calculated_arena.transform(|node, _| node.solved_result());
    SolvedWidthLayout { solved_widths , layout_only_arena }
}

/// Returns the solved height of the items in a BTree form
pub(crate) fn solve_flex_layout_height(
    solved_widths: &SolvedWidthLayout,
    preferred_heights: Arena<Option<f32>>,
    window_height: f32)
-> SolvedHeightLayout
{
    let SolvedWidthLayout { layout_only_arena, .. } = solved_widths;
    let mut height_calculated_arena = Arena::<HeightCalculatedRect>::from_rect_layout_arena(&layout_only_arena, preferred_heights);
    let non_leaf_nodes_sorted_by_depth = height_calculated_arena.bubble_preferred_heights_to_parents(&layout_only_arena);
    height_calculated_arena.apply_flex_grow(&layout_only_arena, &non_leaf_nodes_sorted_by_depth, window_height);
    let solved_heights = height_calculated_arena.transform(|node, _| node.solved_result());
    SolvedHeightLayout { solved_heights }
}

/// Traverses from arena[id] to the root, returning the amount of parents, i.e. the depth of the node in the tree.
#[inline]
fn leaf_node_depth<T>(id: &NodeId, arena: &Arena<T>) -> usize {
    let mut counter = 0;
    let mut last_id = *id;

    while let Some(parent) = arena[last_id].parent {
        last_id = parent;
        counter += 1;
    }

    counter
}

/// Returns the nearest common ancestor with a `position: relative` attribute
/// or `None` if there is no ancestor that has `position: relative`. Usually
/// used in conjunction with `position: absolute`
fn get_nearest_positioned_ancestor<'a>(start_node_id: NodeId, arena: &Arena<RectLayout>)
-> Option<NodeId>
{
    let mut current_node = start_node_id;
    while let Some(parent) = arena[current_node].parent() {
        // An element with position: absolute; is positioned relative to the nearest
        // positioned ancestor (instead of positioned relative to the viewport, like fixed).
        //
        // A "positioned" element is one whose position is anything except static.
        if let Some(LayoutPosition::Static) = arena[parent].data.position {
            current_node = parent;
        } else {
            return Some(parent);
        }
    }
    None
}

#[cfg(test)]
mod layout_tests {

    use css_parser::RectLayout;
    use id_tree::{Arena, Node, NodeId};
    use super::*;

    /// Returns a DOM for testing so we don't have to construct it every time.
    /// The DOM structure looks like this:
    ///
    /// ```no_run
    /// 0
    /// '- 1
    ///    '-- 2
    ///    '   '-- 3
    ///    '   '--- 4
    ///    '-- 5
    /// ```
    fn get_testing_dom() -> Arena<()> {
        Arena {
            nodes: vec![
                // 0
                Node {
                    parent: None,
                    previous_sibling: None,
                    next_sibling: None,
                    first_child: Some(NodeId::new(1)),
                    last_child: Some(NodeId::new(1)),
                    data: (),
                },
                // 1
                Node {
                    parent: Some(NodeId::new(0)),
                    previous_sibling: None,
                    next_sibling: Some(NodeId::new(5)),
                    first_child: Some(NodeId::new(2)),
                    last_child: Some(NodeId::new(2)),
                    data: (),
                },
                // 2
                Node {
                    parent: Some(NodeId::new(1)),
                    previous_sibling: None,
                    next_sibling: None,
                    first_child: Some(NodeId::new(3)),
                    last_child: Some(NodeId::new(4)),
                    data: (),
                },
                // 3
                Node {
                    parent: Some(NodeId::new(2)),
                    previous_sibling: None,
                    next_sibling: Some(NodeId::new(4)),
                    first_child: None,
                    last_child: None,
                    data: (),
                },
                // 4
                Node {
                    parent: Some(NodeId::new(2)),
                    previous_sibling: Some(NodeId::new(3)),
                    next_sibling: None,
                    first_child: None,
                    last_child: None,
                    data: (),
                },
                // 5
                Node {
                    parent: Some(NodeId::new(1)),
                    previous_sibling: Some(NodeId::new(2)),
                    next_sibling: None,
                    first_child: None,
                    last_child: None,
                    data: (),
                },
            ]
        }
    }

    /// Returns the same arena, but pre-fills nodes at [(NodeId, RectLayout)]
    /// with the layout rect
    fn get_display_rectangle_arena(constraints: &[(usize, RectLayout)]) -> Arena<RectLayout> {
        let arena = get_testing_dom();
        let mut arena = arena.transform(|_, _| RectLayout::default());
        for (id, rect) in constraints {
            arena[NodeId::new(*id)].data = *rect;
        }
        arena
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
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Unconstrained);

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(500.0))),
            min_width: None,
            max_width: None,
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(500.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(500.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
            max_width: None,
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(600.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(10000.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: None,
            min_width: Some(LayoutMinWidth(PixelValue::px(600.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Between(600.0, 800.0));

        let layout = RectLayout {
            width: None,
            min_width: None,
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::Between(0.0, 800.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(1000.0))),
            min_width: None,
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(1200.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(1000.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(800.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(800.0));

        let layout = RectLayout {
            width: Some(LayoutWidth(PixelValue::px(1200.0))),
            min_width: Some(LayoutMinWidth(PixelValue::px(1000.0))),
            max_width: Some(LayoutMaxWidth(PixelValue::px(400.0))),
            .. Default::default()
        };
        assert_eq!(determine_preferred_width(&layout, None), WhConstraint::EqualTo(400.0));
    }

    /// Tests that the nodes get filled correctly
    #[test]
    fn test_fill_out_preferred_width() {

        use css_parser::*;

        let display_rectangles = get_display_rectangle_arena(&[
            (1, RectLayout {
                max_width: Some(LayoutMaxWidth(PixelValue::px(200.0))),
                padding: Some(LayoutPadding { left: Some(PixelValue::px(20.0)), right: Some(PixelValue::px(20.0)), .. Default::default() }),
                .. Default::default()
            })
        ]);

        let preferred_widths = display_rectangles.transform(|_, _| None);
        let mut width_filled_out = Arena::<WidthCalculatedRect>::from_rect_layout_arena(&display_rectangles, preferred_widths);

        // Test some basic stuff - test that `get_flex_basis` works

        // Nodes 0, 2, 3, 4 and 5 have no basis
        assert_eq!(width_filled_out[NodeId::new(0)].data.get_flex_basis_horizontal(), 0.0);

        // Node 1 has a padding on left and right of 20, so a flex-basis of 40.0
        assert_eq!(width_filled_out[NodeId::new(1)].data.get_flex_basis_horizontal(), 40.0);
        assert_eq!(width_filled_out[NodeId::new(1)].data.get_horizontal_padding(), 40.0);

        assert_eq!(width_filled_out[NodeId::new(2)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(3)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(4)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(5)].data.get_flex_basis_horizontal(), 0.0);

        assert_eq!(width_filled_out[NodeId::new(0)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(1)].data.preferred_width, WhConstraint::Between(0.0, 200.0));
        assert_eq!(width_filled_out[NodeId::new(2)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(3)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(4)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(5)].data.preferred_width, WhConstraint::Unconstrained);

        // Test the flex-basis sum
        assert_eq!(width_filled_out.sum_children_flex_basis(NodeId::new(2), &display_rectangles), 0.0);
        assert_eq!(width_filled_out.sum_children_flex_basis(NodeId::new(1), &display_rectangles), 0.0);
        assert_eq!(width_filled_out.sum_children_flex_basis(NodeId::new(0), &display_rectangles), 40.0);

        // -- Section 2: Test that size-bubbling works:
        //
        // Size-bubbling should take the 40px padding and "bubble" it towards the
        let non_leaf_nodes_sorted_by_depth = width_filled_out.bubble_preferred_widths_to_parents(&display_rectangles);

        // ID 5 has no child, so it's not returned, same as 3 and 4
        assert_eq!(non_leaf_nodes_sorted_by_depth, vec![
            (0, NodeId::new(0)),
            (1, NodeId::new(1)),
            (2, NodeId::new(2)),
        ]);

        // This step shouldn't have touched the flex_grow_px
        for node_id in width_filled_out.linear_iter() {
            assert_eq!(width_filled_out[node_id].data.flex_grow_px, 0.0);
        }

        // This step should not modify the `preferred_width`
        assert_eq!(width_filled_out[NodeId::new(0)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(1)].data.preferred_width, WhConstraint::Between(0.0, 200.0));
        assert_eq!(width_filled_out[NodeId::new(2)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(3)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(4)].data.preferred_width, WhConstraint::Unconstrained);
        assert_eq!(width_filled_out[NodeId::new(5)].data.preferred_width, WhConstraint::Unconstrained);

        // The padding of the Node 1 should have bubbled up to be the minimum width of Node 0
        assert_eq!(width_filled_out[NodeId::new(0)].data.min_inner_size_px, 40.0);
        assert_eq!(width_filled_out[NodeId::new(1)].data.get_flex_basis_horizontal(), 40.0);
        assert_eq!(width_filled_out[NodeId::new(1)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(2)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(2)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(3)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(3)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(4)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(4)].data.min_inner_size_px, 0.0);
        assert_eq!(width_filled_out[NodeId::new(5)].data.get_flex_basis_horizontal(), 0.0);
        assert_eq!(width_filled_out[NodeId::new(5)].data.min_inner_size_px, 0.0);

        // -- Section 3: Test if growing the sizes works

        let window_width = 754.0; // pixel

        // - window_width: 754px
        // 0                -- [] - expecting width to stretch to 754 px
        // '- 1             -- [max-width: 200px; padding: 20px] - expecting width to stretch to 200 px
        //    '-- 2         -- [] - expecting width to stretch to 160px
        //    '   '-- 3     -- [] - expecting width to stretch to 80px (half of 160)
        //    '   '--- 4    -- [] - expecting width to stretch to 80px (half of 160)
        //    '-- 5         -- [] - expecting width to stretch to 554px (754 - 200px max-width of earlier sibling)

        width_filled_out.apply_flex_grow(&display_rectangles, &non_leaf_nodes_sorted_by_depth, window_width);

        assert_eq!(width_filled_out[NodeId::new(0)].data.solved_result(), WidthSolvedResult {
            min_width: 40.0,
            space_added: window_width - 40.0,
        });
        assert_eq!(width_filled_out[NodeId::new(1)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 200.0,
        });
        assert_eq!(width_filled_out[NodeId::new(2)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 160.0,
        });
        assert_eq!(width_filled_out[NodeId::new(3)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 80.0,
        });
        assert_eq!(width_filled_out[NodeId::new(4)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: 80.0,
        });
        assert_eq!(width_filled_out[NodeId::new(5)].data.solved_result(), WidthSolvedResult {
            min_width: 0.0,
            space_added: window_width - 200.0,
        });
    }

    /// Tests that the node-depth calculation works correctly
    #[test]
    fn test_leaf_node_depth() {

        let arena = get_testing_dom();

        // 0                -- depth 0
        // '- 1             -- depth 1
        //    '-- 2         -- depth 2
        //    '   '-- 3     -- depth 3
        //    '   '--- 4    -- depth 3
        //    '-- 5         -- depth 2

        assert_eq!(leaf_node_depth(&NodeId::new(0), &arena), 0);
        assert_eq!(leaf_node_depth(&NodeId::new(1), &arena), 1);
        assert_eq!(leaf_node_depth(&NodeId::new(2), &arena), 2);
        assert_eq!(leaf_node_depth(&NodeId::new(3), &arena), 3);
        assert_eq!(leaf_node_depth(&NodeId::new(4), &arena), 3);
        assert_eq!(leaf_node_depth(&NodeId::new(5), &arena), 2);
    }

    // Old test, remove later
    #[test]
    fn test_new_ui_solver_has_root_constraints() {
        let mut solver = DomSolver::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(400.0, 600.0));
        assert!(solver.solver.suggest_value(solver.root_constraints.width_var, 400.0).is_ok());
        assert!(solver.solver.suggest_value(solver.root_constraints.width_var, 600.0).is_ok());
    }
}