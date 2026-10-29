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
//!
//! A rebuild can also find its whole DOM unchanged and keep the one it
//! retained, cascade and layout included. [`NodeField`] lists every field of
//! a node with what reads it and what such a rebuild does with it: compares
//! it, or carries the fresh build's value onto the retained DOM, or both.
//! The pre-cascade fingerprints, the payloads they carry and the comparison
//! of two styled DOMs are derived from that list.

use core::hash::{Hash, Hasher};

use azul_css::AzString;

use crate::{
    dom::{AttributeType, NodeData, NodeDataExt, NodeType, SvgNodeData, TabIndex},
    hash::DefaultHasher,
    refany::{RefAny, RefAnyHolder},
    resources::DecodedImage,
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

/// The hashes of what the layout reads of a node beyond its type, style and flags.
///
/// There is one per reach that keeps one: its attributes by their rows
/// ([`LayoutAttribute`]) and its SVG data. A reach the node has no input of
/// hashes to 0.
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

/// What of a node itself makes it a target of the pointer and the keyboard.
///
/// Beside its style (a `:hover` or `cursor` rule, an `overflow` that
/// scrolls), that is a handler of an event that is not the window's, a tab
/// stop or a context menu. Such a node gets a hit-test tag
/// (`CssPropertyCache::generate_tag_ids` reads these), and the display list
/// paints a hit-test area for it, so the fingerprints compare whether a node
/// is one ([`crate::diff::NodeDataFingerprint::hit_test_target`]).
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
                .or_else(|| focus_handler.then_some(TabIndex::Auto)),
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

/// What a rebuild that finds its DOM unchanged does with a field of a node.
///
/// Such a rebuild keeps the DOM the window retained, with its cascade and its
/// layout: before the cascade when the fingerprints of the two builds did not
/// move (`crate::diff::fingerprint_dom`), after it when the two styled DOMs
/// compare alike (`crate::styled_dom::is_layout_equivalent`). A field is
/// compared there, carried onto the retained DOM, or both; one that is
/// neither would keep its change from the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retained {
    /// Compared: a change of it styles and lays out the new DOM.
    Compared,
    /// Compared as the node's own style (the style tier of the fingerprints).
    Style,
    /// Its shape is compared, and its value, which every build makes anew (a
    /// handler's data, a menu's handlers), is carried onto the retained DOM.
    ComparedAndCarried,
    /// Carried onto the retained DOM, never compared: what reads it reads it
    /// live, and nothing the window keeps holds it.
    Carried,
    /// Derived by the engine from the other fields: the retained DOM's own
    /// value is the right one.
    Derived,
}

impl Retained {
    /// Whether a change of the field is compared.
    #[must_use]
    pub const fn compares(self) -> bool {
        matches!(self, Self::Compared | Self::Style | Self::ComparedAndCarried)
    }

    /// Whether the fresh build's value is carried onto the retained DOM.
    #[must_use]
    pub const fn carries(self) -> bool {
        matches!(self, Self::ComparedAndCarried | Self::Carried)
    }
}

/// What reads a field of a node, and so what a change of it moves.
#[allow(clippy::struct_excessive_bools)] // one flag per kind of reader
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Effects {
    /// The cascade: which style rules apply and what they compute.
    pub cascade: bool,
    /// How the node is laid out.
    pub layout: bool,
    /// How it is painted.
    pub paint: bool,
    /// Whether it is a hit-test target ([`HitTestInputs`]).
    pub hit_target: bool,
    /// What the window keeps beside its layout: the accessibility tree, the
    /// keys the next rebuild is matched by.
    pub kept: bool,
    /// What reads it where it is used, from the DOM as it is then: an event's
    /// dispatch, a right-click, a lookup by marker.
    pub live: bool,
}

impl Effects {
    /// Nothing reads it.
    pub const NONE: Self = Self {
        cascade: false,
        layout: false,
        paint: false,
        hit_target: false,
        kept: false,
        live: false,
    };

    /// Whether something the window keeps from one build to the next reads
    /// it: such a field has to be compared.
    #[must_use]
    pub const fn reach_what_is_kept(self) -> bool {
        self.cascade || self.layout || self.paint || self.hit_target || self.kept
    }
}

/// A field of a node's data, one row of the table rebuilds are compared by.
///
/// The fields are those of [`NodeData`] and its rarely used fields,
/// [`NodeDataExt`]; every comparison of two builds of a DOM reads the table
/// (`crate::diff::fingerprint_dom`, the payloads the pre-cascade skip carries,
/// `crate::styled_dom::is_layout_equivalent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeField {
    /// What the node is: a text and its characters, an image, a `div`.
    NodeType,
    /// Its event handlers.
    Callbacks,
    /// Its own style.
    Style,
    /// Its tab index, `contenteditable` and the anonymous-box bit.
    Flags,
    /// What assistive technology is told about it.
    Accessibility,
    /// The Fluent arguments its localizable text is formatted with.
    FluentArgs,
    /// Its attributes, ids and classes among them.
    Attributes,
    /// A virtual view's callback and data.
    VirtualView,
    /// The widget state it carries.
    Dataset,
    /// An SVG node's geometry or `viewBox`, a clip mask.
    SvgData,
    /// The menu bar shown at the top of its rect.
    MenuBar,
    /// The menu a right-click on it opens.
    ContextMenu,
    /// The key the next rebuild is matched by.
    Key,
    /// A marker its node is looked up by, minted anew by every build.
    Marker,
    /// What merges its dataset with the one of the build before.
    DatasetMergeCallback,
    /// The component that rendered it.
    ComponentOrigin,
    /// Its presence animations, by name.
    AnimationCallbacks,
    /// The Fluent key its text was translated from.
    L10nKey,
    /// Whether it is drawn as part of its parent.
    PartOfParent,
}

impl NodeField {
    /// Every row of the table.
    pub const ALL: [Self; 19] = [
        Self::NodeType,
        Self::Callbacks,
        Self::Style,
        Self::Flags,
        Self::Accessibility,
        Self::FluentArgs,
        Self::Attributes,
        Self::VirtualView,
        Self::Dataset,
        Self::SvgData,
        Self::MenuBar,
        Self::ContextMenu,
        Self::Key,
        Self::Marker,
        Self::DatasetMergeCallback,
        Self::ComponentOrigin,
        Self::AnimationCallbacks,
        Self::L10nKey,
        Self::PartOfParent,
    ];

    /// The field's name in [`NodeData`] or [`NodeDataExt`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NodeType => "node_type",
            Self::Callbacks => "callbacks",
            Self::Style => "style",
            Self::Flags => "flags",
            Self::Accessibility => "accessibility",
            Self::FluentArgs => "fluent_args",
            Self::Attributes => "attributes",
            Self::VirtualView => "virtual_view",
            Self::Dataset => "dataset",
            Self::SvgData => "svg_data",
            Self::MenuBar => "menu_bar",
            Self::ContextMenu => "context_menu",
            Self::Key => "key",
            Self::Marker => "marker",
            Self::DatasetMergeCallback => "dataset_merge_callback",
            Self::ComponentOrigin => "component_origin",
            Self::AnimationCallbacks => "animation_callbacks",
            Self::L10nKey => "l10n_key",
            Self::PartOfParent => "part_of_parent",
        }
    }

    /// What a rebuild that finds its DOM unchanged does with the field.
    #[must_use]
    pub const fn retained(self) -> Retained {
        match self {
            // The type and the handlers' events are compared; an image's
            // callback, the handlers and a menu's handlers are made anew by
            // every build.
            Self::NodeType | Self::Callbacks | Self::MenuBar | Self::ContextMenu => {
                Retained::ComparedAndCarried
            }
            Self::Style => Retained::Style,
            Self::Flags
            | Self::Accessibility
            | Self::FluentArgs
            | Self::Attributes
            | Self::SvgData
            | Self::Key
            | Self::PartOfParent => Retained::Compared,
            Self::VirtualView
            | Self::Dataset
            | Self::Marker
            | Self::DatasetMergeCallback
            | Self::ComponentOrigin
            | Self::AnimationCallbacks => Retained::Carried,
            Self::L10nKey => Retained::Derived,
        }
    }

    /// What reads the field.
    #[must_use]
    pub const fn effects(self) -> Effects {
        let none = Effects::NONE;
        match self {
            Self::NodeType => Effects {
                cascade: true,
                layout: true,
                paint: true,
                hit_target: true,
                live: true,
                ..none
            },
            Self::Callbacks | Self::ContextMenu => Effects {
                hit_target: true,
                live: true,
                ..none
            },
            Self::Style => Effects {
                cascade: true,
                layout: true,
                paint: true,
                hit_target: true,
                ..none
            },
            Self::Flags => Effects {
                layout: true,
                hit_target: true,
                live: true,
                ..none
            },
            Self::Accessibility | Self::Key => Effects { kept: true, ..none },
            Self::FluentArgs | Self::SvgData => Effects {
                layout: true,
                paint: true,
                ..none
            },
            Self::Attributes => Effects {
                cascade: true,
                layout: true,
                paint: true,
                ..none
            },
            Self::PartOfParent => Effects {
                cascade: true,
                ..none
            },
            Self::VirtualView
            | Self::Dataset
            | Self::MenuBar
            | Self::Marker
            | Self::DatasetMergeCallback
            | Self::ComponentOrigin
            | Self::AnimationCallbacks
            | Self::L10nKey => Effects { live: true, ..none },
        }
    }

    /// Hashes what the structure tier of the fingerprints compares of the
    /// field of `node`: nothing for a field it does not compare, nor for the
    /// style, which is the style tier's.
    pub fn hash_shape<H: Hasher>(self, node: &NodeData, h: &mut H) {
        match self {
            Self::NodeType => hash_node_type_shape(node, h),
            Self::Callbacks => {
                // The events only: the handlers are carried.
                node.callbacks.as_ref().len().hash(h);
                for cb in node.callbacks.as_ref() {
                    cb.event.hash(h);
                }
            }
            Self::Flags => {
                node.is_contenteditable().hash(h);
                node.flags.hash(h);
            }
            Self::Accessibility => node.accessibility.hash(h),
            Self::FluentArgs => hash_fluent_args(node, h),
            Self::Attributes => {
                // Ids and classes as worn, in order, and every other one.
                for attr in node.attributes().as_ref() {
                    match attr {
                        AttributeType::Id(s) => {
                            1u8.hash(h);
                            s.hash(h);
                        }
                        AttributeType::Class(s) => {
                            2u8.hash(h);
                            s.hash(h);
                        }
                        other => {
                            3u8.hash(h);
                            other.hash(h);
                        }
                    }
                }
            }
            Self::SvgData => node.get_svg_data().hash(h),
            // Whether there is one: its items' handlers are carried.
            Self::MenuBar => node.get_menu_bar().is_some().hash(h),
            Self::ContextMenu => node.get_context_menu().is_some().hash(h),
            Self::Key => node.get_key().hash(h),
            Self::PartOfParent => node.is_part_of_parent().hash(h),
            Self::Style
            | Self::VirtualView
            | Self::Dataset
            | Self::Marker
            | Self::DatasetMergeCallback
            | Self::ComponentOrigin
            | Self::AnimationCallbacks
            | Self::L10nKey => {}
        }
    }

    /// Whether `old` and `new` compare alike in the field, as a styled DOM is
    /// compared with the one it would replace (always, for a field that is
    /// not compared).
    #[must_use]
    pub fn reads_alike(self, old: &NodeData, new: &NodeData) -> bool {
        match self {
            Self::NodeType => node_types_alike(old, new),
            Self::Callbacks => old
                .callbacks
                .as_ref()
                .iter()
                .map(|cb| &cb.event)
                .eq(new.callbacks.as_ref().iter().map(|cb| &cb.event)),
            Self::Style => old.style == new.style,
            Self::Flags => old.flags == new.flags,
            Self::Accessibility => old.accessibility == new.accessibility,
            Self::FluentArgs => old.fluent_args == new.fluent_args,
            Self::Attributes => old.attributes().as_ref() == new.attributes().as_ref(),
            Self::SvgData => old.get_svg_data() == new.get_svg_data(),
            Self::MenuBar => old.get_menu_bar().is_some() == new.get_menu_bar().is_some(),
            Self::ContextMenu => {
                old.get_context_menu().is_some() == new.get_context_menu().is_some()
            }
            Self::Key => old.get_key() == new.get_key(),
            Self::PartOfParent => old.is_part_of_parent() == new.is_part_of_parent(),
            Self::VirtualView
            | Self::Dataset
            | Self::Marker
            | Self::DatasetMergeCallback
            | Self::ComponentOrigin
            | Self::AnimationCallbacks
            | Self::L10nKey => true,
        }
    }

    /// Whether `node` holds a value of the field to carry (never, for a field
    /// that is not carried).
    #[must_use]
    pub fn holds_value(self, node: &NodeData) -> bool {
        let extra = node.extra_fields();
        match self {
            Self::NodeType => image_callback(node).is_some(),
            Self::Callbacks => !node.callbacks.as_ref().is_empty(),
            Self::VirtualView => extra.is_some_and(|e| e.virtual_view.is_some()),
            Self::Dataset => extra.is_some_and(|e| e.dataset.is_some()),
            Self::MenuBar => extra.is_some_and(|e| e.menu_bar.is_some()),
            Self::ContextMenu => extra.is_some_and(|e| e.context_menu.is_some()),
            Self::Marker => extra.is_some_and(|e| e.marker.is_some()),
            Self::DatasetMergeCallback => {
                extra.is_some_and(|e| e.dataset_merge_callback.is_some())
            }
            Self::ComponentOrigin => extra.is_some_and(|e| e.component_origin.is_some()),
            Self::AnimationCallbacks => {
                extra.is_some_and(|e| !e.animation_callbacks.is_empty())
            }
            Self::Style
            | Self::Flags
            | Self::Accessibility
            | Self::FluentArgs
            | Self::Attributes
            | Self::SvgData
            | Self::Key
            | Self::L10nKey
            | Self::PartOfParent => false,
        }
    }

    /// Puts `from`'s value of the field onto `to`, when `from` holds one to
    /// carry ([`Self::holds_value`]).
    pub fn carry(self, from: &NodeData, to: &mut NodeData) {
        if !self.holds_value(from) {
            return;
        }
        match self {
            Self::NodeType => {
                if let Some(callback) = image_callback(from) {
                    to.node_type = NodeType::Image(azul_css::css::BoxOrStatic::heap(
                        crate::resources::ImageRef::callback(
                            callback.callback.clone(),
                            callback.refany.clone(),
                        ),
                    ));
                }
            }
            Self::Callbacks => to.callbacks = from.callbacks.clone(),
            Self::VirtualView => carry_extra(from, to, |fresh, held| {
                held.virtual_view.clone_from(&fresh.virtual_view);
            }),
            Self::Dataset => carry_extra(from, to, |fresh, held| {
                held.dataset.clone_from(&fresh.dataset);
            }),
            Self::MenuBar => carry_extra(from, to, |fresh, held| {
                held.menu_bar.clone_from(&fresh.menu_bar);
            }),
            Self::ContextMenu => carry_extra(from, to, |fresh, held| {
                held.context_menu.clone_from(&fresh.context_menu);
            }),
            Self::Marker => carry_extra(from, to, |fresh, held| {
                held.marker.clone_from(&fresh.marker);
            }),
            Self::DatasetMergeCallback => carry_extra(from, to, |fresh, held| {
                held.dataset_merge_callback
                    .clone_from(&fresh.dataset_merge_callback);
            }),
            Self::ComponentOrigin => carry_extra(from, to, |fresh, held| {
                held.component_origin.clone_from(&fresh.component_origin);
            }),
            Self::AnimationCallbacks => carry_extra(from, to, |fresh, held| {
                held.animation_callbacks
                    .clone_from(&fresh.animation_callbacks);
            }),
            Self::Style
            | Self::Flags
            | Self::Accessibility
            | Self::FluentArgs
            | Self::Attributes
            | Self::SvgData
            | Self::Key
            | Self::L10nKey
            | Self::PartOfParent => {}
        }
    }

    /// Hands every `RefAny` the field of `node` holds to `visit`: the data of
    /// its handlers, of an image's callback and of a view, its dataset, its
    /// menus' items' data, its presence animations' data. The rest holds
    /// none, or only a binding's own callable (a merge callback's), which is
    /// never a node's data.
    pub fn for_each_refany(self, node: &mut NodeData, visit: &mut dyn FnMut(&mut RefAny)) {
        match self {
            Self::NodeType => {
                // An image's callback is shared by the clones of its image:
                // a new data makes a new image.
                let Some(callback) = image_callback(node) else {
                    return;
                };
                let (function, mut data) = (callback.callback.clone(), callback.refany.clone());
                let before = data.sharing_info.ptr;
                visit(&mut data);
                if data.sharing_info.ptr != before {
                    node.node_type = NodeType::Image(azul_css::css::BoxOrStatic::heap(
                        crate::resources::ImageRef::callback(function, data),
                    ));
                }
            }
            Self::Callbacks => {
                for callback in node.callbacks.as_mut() {
                    callback.for_each_refany(visit);
                }
            }
            Self::VirtualView => {
                if let Some(view) = node
                    .existing_extra_fields_mut()
                    .and_then(|extra| extra.virtual_view.as_mut())
                {
                    view.for_each_refany(visit);
                }
            }
            Self::Dataset => {
                if let Some(dataset) = node
                    .existing_extra_fields_mut()
                    .and_then(|extra| extra.dataset.as_mut())
                {
                    visit(dataset);
                }
            }
            Self::MenuBar => {
                if let Some(menu) = node
                    .existing_extra_fields_mut()
                    .and_then(|extra| extra.menu_bar.as_mut())
                {
                    menu.for_each_refany(visit);
                }
            }
            Self::ContextMenu => {
                if let Some(menu) = node
                    .existing_extra_fields_mut()
                    .and_then(|extra| extra.context_menu.as_mut())
                {
                    menu.for_each_refany(visit);
                }
            }
            Self::AnimationCallbacks => {
                if let Some(extra) = node.existing_extra_fields_mut() {
                    for function in &mut extra.animation_callbacks {
                        function.for_each_refany(visit);
                    }
                }
            }
            Self::Style
            | Self::Flags
            | Self::Accessibility
            | Self::FluentArgs
            | Self::Attributes
            | Self::SvgData
            | Self::Key
            | Self::Marker
            | Self::DatasetMergeCallback
            | Self::ComponentOrigin
            | Self::L10nKey
            | Self::PartOfParent => {}
        }
    }

    /// A node holding `node`'s values to carry and nothing else, `None` when
    /// it holds none: what the pre-cascade skip keeps of a fresh build's node.
    #[must_use]
    pub fn values_to_carry(node: &NodeData) -> Option<NodeData> {
        let carried = Self::ALL
            .into_iter()
            .filter(|field| field.retained().carries());
        if !carried.clone().any(|field| field.holds_value(node)) {
            return None;
        }
        let mut values = NodeData::create_div();
        for field in carried {
            field.carry(node, &mut values);
        }
        Some(values)
    }
}

/// The image callback of `node`, if it is an image drawn by one.
fn image_callback(node: &NodeData) -> Option<&crate::callbacks::CoreImageCallback> {
    match node.get_node_type() {
        NodeType::Image(image) => match image.get_data() {
            DecodedImage::Callback(callback) => Some(callback),
            _ => None,
        },
        _ => None,
    }
}

/// Copies a rarely used field of `from` onto `to` with `copy`.
fn carry_extra(
    from: &NodeData,
    to: &mut NodeData,
    copy: impl FnOnce(&NodeDataExt, &mut NodeDataExt),
) {
    if let Some(fresh) = from.extra_fields() {
        copy(fresh, to.extra_fields_mut());
    }
}

/// The shape of a node's type: an image callback by its function and the
/// type of its data (both builds make the data anew; the callback itself is
/// carried), any other image by its identity, everything else by its value,
/// and a text's being localizable (it renders its key's translation).
fn hash_node_type_shape<H: Hasher>(node: &NodeData, h: &mut H) {
    match node.get_node_type() {
        NodeType::Image(image) => match image.get_data() {
            DecodedImage::Callback(callback) => {
                0xB0DE_CA11_u32.hash(h);
                callback.callback.cb.hash(h);
                callback.refany.get_type_id().hash(h);
            }
            _ => node.get_node_type().hash(h),
        },
        NodeType::Text(text) => {
            node.get_node_type().hash(h);
            text.as_ref().is_localizable().hash(h);
        }
        other => other.hash(h),
    }
}

/// Whether two nodes' types compare alike: [`hash_node_type_shape`]'s rule.
fn node_types_alike(old: &NodeData, new: &NodeData) -> bool {
    match (old.get_node_type(), new.get_node_type()) {
        (NodeType::Image(old_image), NodeType::Image(new_image)) => {
            match (old_image.get_data(), new_image.get_data()) {
                (DecodedImage::Callback(old_cb), DecodedImage::Callback(new_cb)) => {
                    old_cb.callback.cb == new_cb.callback.cb
                        && old_cb.refany.get_type_id() == new_cb.refany.get_type_id()
                }
                _ => old_image == new_image,
            }
        }
        (NodeType::Text(old_text), NodeType::Text(new_text)) => {
            old_text == new_text
                && old_text.as_ref().is_localizable() == new_text.as_ref().is_localizable()
        }
        (old_type, new_type) => old_type == new_type,
    }
}

/// The Fluent arguments of `node`, which its localizable text is formatted
/// with: each by its key and its typed value.
fn hash_fluent_args<H: Hasher>(node: &NodeData, h: &mut H) {
    let Some(args) = node.fluent_args.as_deref() else {
        return;
    };
    0xF1u8.hash(h);
    args.as_slice().len().hash(h);
    for arg in args.as_slice() {
        arg.key.hash(h);
        match &arg.value {
            crate::dom::FluentArg::String(s) => {
                0u8.hash(h);
                s.hash(h);
            }
            crate::dom::FluentArg::I32(i) => {
                1u8.hash(h);
                i.hash(h);
            }
            crate::dom::FluentArg::F32(f) => {
                2u8.hash(h);
                f.to_bits().hash(h);
            }
        }
    }
}
