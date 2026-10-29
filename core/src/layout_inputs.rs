//! What the layout reads of a node, and what a change of each part moves.
//!
//! A rebuilt DOM is compared with the one before it node by node, through
//! the fingerprints of [`crate::diff::NodeDataFingerprint`], and the layout
//! keeps whatever that comparison calls unchanged: the reconcile clones the
//! node, its caches answer, the cached display list is served. So every part
//! of a node the layout reads has to be part of the comparison. A node's
//! type, its own style and its flags are fingerprinted each on its own; the
//! rest is here: its attributes, which the layout reads only through the
//! rows of [`LayoutAttribute`], and the data an SVG node carries.
//! [`LayoutInputHashes`] hashes them by what a change of them moves, and the
//! fingerprints and the field-by-field comparison both read them so.

use core::hash::{Hash, Hasher};

use azul_css::AzString;

use crate::{
    dom::{AttributeType, NodeData, NodeType, SvgNodeData, TabIndex},
    hash::DefaultHasher,
};

/// What a change of something the layout reads of a node moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutReach {
    /// Nothing the layout keeps: it is read where it is used, from the DOM as
    /// it is then (a click, a key press, a focus change).
    Live,
    /// Which style rules apply (`id`, `class`): the node and its descendants
    /// may compute other styles, which the cascade's diff names.
    Selectors,
    /// The node's own box, laid out and painted from it: the node is laid
    /// out again, and a part of a table with its table.
    OwnBox,
    /// The lines of the node and of the boxes below it, which are broken in
    /// it (`lang`): the reconcile compares the line styles below the node.
    Lines,
    /// The boxes below the node, which read it from the nearest ancestor
    /// that states it: every box below the node is laid out again.
    Descendants,
}

/// An attribute the layout reads: one row of its table.
///
/// The layout reads a node's attributes only through a row
/// ([`NodeData::layout_attributes`]), and the fingerprints hash each row's
/// attributes by its [`LayoutReach`], so whatever the layout reads, a change
/// of it is seen. An attribute that is no row is read by nothing that lays
/// out or paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutAttribute {
    /// `id`: selects style rules.
    Id,
    /// `class`: selects style rules.
    Class,
    /// `lang`: the content language the lines below it hyphenate in.
    Lang,
    /// `contenteditable` as an attribute: whether an edit may change the
    /// node, asked when one happens.
    ContentEditable,
    /// A cell's `colspan`, a column's `span`: how many columns of its
    /// table's grid it takes.
    ColSpan,
    /// A cell's `rowspan`: how many rows of its table's grid it takes.
    RowSpan,
    /// The legacy `align` of a block or a part of a table: `center` centres
    /// the blocks below it whose text is centred.
    Align,
    /// SVG `x`: where a text's first baseline or an image starts.
    SvgX,
    /// SVG `y`.
    SvgY,
    /// SVG `width` of an image.
    SvgWidth,
    /// SVG `height` of an image.
    SvgHeight,
    /// SVG `dx` of a `<tspan>`: the shift before its first character.
    SvgDx,
    /// SVG `transform`: maps the user space of the node and of everything in
    /// it into its parent's.
    SvgTransform,
    /// SVG `fill-rule`: what a shape's fill covers.
    SvgFillRule,
    /// SVG `stroke-linecap`, which a shape takes from the groups around it.
    SvgStrokeLinecap,
    /// SVG `stroke-linejoin`, inherited as the cap is.
    SvgStrokeLinejoin,
    /// SVG `stroke-miterlimit`, inherited as the cap is.
    SvgStrokeMiterlimit,
    /// SVG `stroke-dasharray`, inherited as the cap is.
    SvgStrokeDasharray,
    /// SVG `stroke-dashoffset`, inherited as the cap is.
    SvgStrokeDashoffset,
}

impl LayoutAttribute {
    /// Every row of the table.
    pub const ALL: [Self; 19] = [
        Self::Id,
        Self::Class,
        Self::Lang,
        Self::ContentEditable,
        Self::ColSpan,
        Self::RowSpan,
        Self::Align,
        Self::SvgX,
        Self::SvgY,
        Self::SvgWidth,
        Self::SvgHeight,
        Self::SvgDx,
        Self::SvgTransform,
        Self::SvgFillRule,
        Self::SvgStrokeLinecap,
        Self::SvgStrokeLinejoin,
        Self::SvgStrokeMiterlimit,
        Self::SvgStrokeDasharray,
        Self::SvgStrokeDashoffset,
    ];

    /// The attribute's name, as HTML and SVG spell it ([`AttributeType::name`]).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Class => "class",
            Self::Lang => "lang",
            Self::ContentEditable => "contenteditable",
            Self::ColSpan => "colspan",
            Self::RowSpan => "rowspan",
            Self::Align => "align",
            Self::SvgX => "x",
            Self::SvgY => "y",
            Self::SvgWidth => "width",
            Self::SvgHeight => "height",
            Self::SvgDx => "dx",
            Self::SvgTransform => "transform",
            Self::SvgFillRule => "fill-rule",
            Self::SvgStrokeLinecap => "stroke-linecap",
            Self::SvgStrokeLinejoin => "stroke-linejoin",
            Self::SvgStrokeMiterlimit => "stroke-miterlimit",
            Self::SvgStrokeDasharray => "stroke-dasharray",
            Self::SvgStrokeDashoffset => "stroke-dashoffset",
        }
    }

    /// What a change of the attribute moves.
    #[must_use]
    pub const fn reach(self) -> LayoutReach {
        match self {
            Self::Id | Self::Class => LayoutReach::Selectors,
            Self::Lang => LayoutReach::Lines,
            Self::ContentEditable => LayoutReach::Live,
            Self::ColSpan
            | Self::RowSpan
            | Self::SvgX
            | Self::SvgY
            | Self::SvgWidth
            | Self::SvgHeight
            | Self::SvgDx
            | Self::SvgFillRule => LayoutReach::OwnBox,
            Self::Align
            | Self::SvgTransform
            | Self::SvgStrokeLinecap
            | Self::SvgStrokeLinejoin
            | Self::SvgStrokeMiterlimit
            | Self::SvgStrokeDasharray
            | Self::SvgStrokeDashoffset => LayoutReach::Descendants,
        }
    }

    /// Whether a node carries the attribute by its name (an SVG attribute,
    /// the legacy `align`: [`AttributeType::Custom`]) rather than as a
    /// variant of its own.
    const fn is_named(self) -> bool {
        !matches!(
            self,
            Self::Id
                | Self::Class
                | Self::Lang
                | Self::ContentEditable
                | Self::ColSpan
                | Self::RowSpan
        )
    }

    /// The row `attribute` is, `None` for an attribute the layout does not
    /// read.
    #[must_use]
    pub fn of(attribute: &AttributeType) -> Option<Self> {
        match attribute {
            AttributeType::Id(_) => Some(Self::Id),
            AttributeType::Class(_) => Some(Self::Class),
            AttributeType::Lang(_) => Some(Self::Lang),
            AttributeType::ContentEditable(_) => Some(Self::ContentEditable),
            AttributeType::ColSpan(_) => Some(Self::ColSpan),
            AttributeType::RowSpan(_) => Some(Self::RowSpan),
            // Names compare without case, as HTML's do.
            AttributeType::Custom(named) | AttributeType::Data(named) => Self::ALL
                .into_iter()
                .filter(|row| row.is_named())
                .find(|row| row.name().eq_ignore_ascii_case(named.attr_name.as_str())),
            _ => None,
        }
    }
}

impl NodeData {
    /// The node's attributes of the row `row`, in the order the node holds
    /// them: how the layout reads an attribute ([`LayoutAttribute`]).
    pub fn layout_attributes(
        &self,
        row: LayoutAttribute,
    ) -> impl Iterator<Item = &AttributeType> + '_ {
        self.attributes()
            .as_ref()
            .iter()
            .filter(move |attribute| LayoutAttribute::of(attribute) == Some(row))
    }

    /// The text of the node's first attribute of the row `row`, `None` when
    /// it has none.
    #[must_use]
    pub fn layout_attribute(&self, row: LayoutAttribute) -> Option<AzString> {
        self.layout_attributes(row).next().map(AttributeType::value)
    }
}

/// What a change of an SVG node's data moves: a `viewBox` maps everything in
/// its `<svg>`, a shape's geometry and a clip mask are the node's own. The
/// layout reads no other kind.
const fn svg_reach(data: &SvgNodeData) -> LayoutReach {
    match data {
        SvgNodeData::ViewBox { .. } => LayoutReach::Descendants,
        SvgNodeData::Path(_) | SvgNodeData::ImageClipMask(_) => LayoutReach::OwnBox,
        SvgNodeData::Circle { .. }
        | SvgNodeData::Rect { .. }
        | SvgNodeData::Ellipse { .. }
        | SvgNodeData::Line { .. }
        | SvgNodeData::PointsList { .. }
        | SvgNodeData::LinearGradient { .. }
        | SvgNodeData::RadialGradient { .. }
        | SvgNodeData::GradientStop { .. }
        | SvgNodeData::Use { .. }
        | SvgNodeData::SvgImageData { .. } => LayoutReach::Live,
    }
}

/// The attributes of `node` whose rows have the reach `reach`, in order.
fn attributes_of(
    node: &NodeData,
    reach: LayoutReach,
) -> impl Iterator<Item = &AttributeType> + '_ {
    node.attributes().as_ref().iter().filter(move |attribute| {
        LayoutAttribute::of(attribute).map(LayoutAttribute::reach) == Some(reach)
    })
}

/// The SVG data of `node`, if a change of it has the reach `reach`.
fn svg_data_of(node: &NodeData, reach: LayoutReach) -> Option<&SvgNodeData> {
    node.get_svg_data().filter(|data| svg_reach(data) == reach)
}

/// Whether the layout reads `old` and `new` alike for every input of the
/// reach `reach`: the attributes of its rows, in order, and the SVG data.
#[must_use]
pub fn reads_alike(old: &NodeData, new: &NodeData, reach: LayoutReach) -> bool {
    attributes_of(old, reach).eq(attributes_of(new, reach))
        && svg_data_of(old, reach) == svg_data_of(new, reach)
}

/// The hashes of what the layout reads of a node besides its type, its own
/// style and its flags, one per reach that keeps one: its attributes by
/// their rows ([`LayoutAttribute`]) and its SVG data. A reach the node has
/// no input of hashes to 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LayoutInputHashes {
    /// [`LayoutReach::Selectors`]: the node's ids and classes, as worn.
    pub selectors: u64,
    /// [`LayoutReach::OwnBox`]: a cell's span, an SVG text's position, a
    /// shape's geometry.
    pub own_box: u64,
    /// [`LayoutReach::Lines`]: the content language the node states.
    pub lines: u64,
    /// [`LayoutReach::Descendants`]: the legacy `align`, an SVG group's
    /// `transform` and stroke, an `<svg>`'s `viewBox`.
    pub descendants: u64,
}

impl LayoutInputHashes {
    /// The hashes of `node`'s inputs.
    #[must_use]
    pub fn of(node: &NodeData) -> Self {
        let mut hashers = Hashers::default();
        for attribute in node.attributes().as_ref() {
            if let Some(row) = LayoutAttribute::of(attribute) {
                hashers.add(row.reach(), attribute);
            }
        }
        if let Some(data) = node.get_svg_data() {
            hashers.add(svg_reach(data), data);
        }
        Self {
            selectors: finish(hashers.selectors.as_ref()),
            own_box: finish(hashers.own_box.as_ref()),
            lines: finish(hashers.lines.as_ref()),
            descendants: finish(hashers.descendants.as_ref()),
        }
    }
}

/// One hasher per reach that keeps a hash, made when its first input comes.
#[derive(Default)]
struct Hashers {
    selectors: Option<DefaultHasher>,
    own_box: Option<DefaultHasher>,
    lines: Option<DefaultHasher>,
    descendants: Option<DefaultHasher>,
}

impl Hashers {
    /// Hashes `input` into the hasher of `reach` (none for what nothing keeps).
    fn add<T: Hash + ?Sized>(&mut self, reach: LayoutReach, input: &T) {
        let slot = match reach {
            LayoutReach::Live => return,
            LayoutReach::Selectors => &mut self.selectors,
            LayoutReach::OwnBox => &mut self.own_box,
            LayoutReach::Lines => &mut self.lines,
            LayoutReach::Descendants => &mut self.descendants,
        };
        input.hash(slot.get_or_insert_with(DefaultHasher::new));
    }
}

/// The hash `hasher` holds, 0 for none.
fn finish(hasher: Option<&DefaultHasher>) -> u64 {
    hasher.map_or(0, Hasher::finish)
}

/// What of a node itself makes it a target of the pointer and the keyboard,
/// beside its style (a `:hover` or `cursor` rule, an `overflow` that scrolls):
/// a handler of an event that is not the window's, a tab stop, a context
/// menu. Such a node gets a hit-test tag (`CssPropertyCache::generate_tag_ids`
/// reads these), and the display list paints a hit-test area for it, so the
/// fingerprints compare whether a node is one
/// ([`crate::diff::NodeDataFingerprint::hit_test_target`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HitTestInputs {
    /// The node's tab stop: its own tab index, else the one a focus handler
    /// or a webview gives it.
    pub tab_stop: Option<TabIndex>,
    /// It handles an event that is not the window's.
    pub listens: bool,
    /// It opens a context menu.
    pub context_menu: bool,
}

impl HitTestInputs {
    /// The inputs of `node`.
    #[must_use]
    pub fn of(node: &NodeData) -> Self {
        let callbacks = node.get_callbacks();
        // A webview takes the pointer and the keyboard as an iframe does.
        let focus_handler = matches!(node.get_node_type(), NodeType::WebView(_))
            || callbacks.iter().any(|cb| cb.event.is_focus_callback());
        Self {
            tab_stop: node
                .get_tab_index()
                .or(focus_handler.then_some(TabIndex::Auto)),
            listens: callbacks.iter().any(|cb| !cb.event.is_window_callback()),
            context_menu: node.has_context_menu(),
        }
    }

    /// Whether these alone make the node a hit-test target.
    #[must_use]
    pub const fn make_a_target(&self) -> bool {
        self.tab_stop.is_some() || self.listens || self.context_menu
    }
}
