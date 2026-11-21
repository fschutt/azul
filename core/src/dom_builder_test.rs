#[allow(unused_imports)]
pub use super::*;

/// The flat builder against the tree `Dom`: the same UI built both ways is the same tree,
/// the same fingerprints, the same arenas and the same styled DOM; building it allocates
/// for the arenas only and its fingerprints are read, not hashed. (Audit 2026-11-21, task
/// dom-builder.)
#[cfg(all(test, feature = "std"))]
mod audit_dom_builder_tests {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    use azul_css::{
        css::{rule_priority, CssDeclaration, CssPath, CssRuleBlock},
        dynamic_selector::{DynamicSelector, PseudoStateType},
        props::{
            basic::pixel::PixelValue,
            layout::{
                display::LayoutDisplay,
                position::{LayoutLeft, LayoutPosition, LayoutTop},
            },
            property::CssProperty,
        },
        OptionString,
    };

    use super::*;
    use crate::{
        callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
        diff::{fingerprint_dom, same_tree_changes, texts_alone_changed},
        dom::{AttributeNameValue, SmallAriaInfo},
        events::HoverEventFilter,
        id::NodeId,
        resources::{ImageRef, RawImageFormat},
        styled_dom::{convert_dom_into_compact_dom, StyledDom},
    };

    // ---- counting allocator (this test binary only) ----------------------------------

    std::thread_local! {
        /// How often this thread asked the allocator for memory (a growth counts).
        static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
    }

    struct Counting;

    // SAFETY: every method forwards to `System` with the caller's arguments unchanged;
    // the counter is bookkeeping only (const-initialized, so reading it never allocates).
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
            unsafe { System.alloc(layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static COUNTING: Counting = Counting;

    /// What `f` answers and how often it allocated on this thread.
    fn allocations<R>(f: impl FnOnce() -> R) -> (R, u64) {
        let before = ALLOCATIONS.with(Cell::get);
        let out = f();
        (out, ALLOCATIONS.with(Cell::get) - before)
    }

    // ---- a UI described once, built both ways -----------------------------------------

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        fn one_in(&mut self, n: u64) -> bool {
            self.below(n) == 0
        }
    }

    #[derive(Clone)]
    enum Kind {
        Element(NodeType),
        /// A text through the text constructor (the builder's shared string).
        Text(String),
        /// A text given as a whole `NodeType`.
        TextByValue(String),
        /// A translation key.
        LocalizedText(String),
        Image,
        VirtualView,
    }

    #[derive(Clone)]
    struct Spec {
        kind: Kind,
        classes: Vec<String>,
        ids: Vec<String>,
        attrs: Vec<AttributeType>,
        props: Vec<CssPropertyWithConditions>,
        callbacks: Vec<EventFilter>,
        key: Option<String>,
        tab_index: Option<TabIndex>,
        contenteditable: bool,
        dataset: bool,
        accessibility: Option<String>,
        marker: Option<String>,
        sheets: Vec<Css>,
        /// Whether the builder takes the node whole BEFORE its classes (the setters then
        /// write a `NodeData`) or after its handlers.
        whole_first: bool,
        children: Vec<Spec>,
    }

    /// What both builds share: the same image, the same data.
    struct Shared {
        image: ImageRef,
        data: RefAny,
    }

    extern "C" fn handler() {}

    extern "C" fn view_cb(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
        unreachable!("never invoked by these tests")
    }

    fn px(n: u64) -> PixelValue {
        PixelValue::const_px(isize::try_from(n).unwrap_or(0))
    }

    fn sheet(n: u64, priority: u8) -> Css {
        Css::new(vec![CssRuleBlock {
            path: CssPath {
                selectors: Vec::new().into(),
            },
            declarations: vec![CssDeclaration::Static(CssProperty::const_top(LayoutTop {
                inner: px(n),
            }))]
            .into(),
            conditions: Vec::new().into(),
            priority,
        }])
    }

    fn prop(rng: &mut Rng) -> CssPropertyWithConditions {
        let property = match rng.below(4) {
            0 => CssProperty::const_left(LayoutLeft {
                inner: px(rng.below(500)),
            }),
            1 => CssProperty::const_top(LayoutTop {
                inner: px(rng.below(500)),
            }),
            2 => CssProperty::const_position(LayoutPosition::Absolute),
            _ => CssProperty::const_display(LayoutDisplay::Block),
        };
        if rng.one_in(4) {
            CssPropertyWithConditions {
                property,
                apply_if: vec![DynamicSelector::PseudoState(PseudoStateType::Hover)].into(),
            }
        } else {
            CssPropertyWithConditions::simple(property)
        }
    }

    fn attr(rng: &mut Rng) -> AttributeType {
        match rng.below(5) {
            0 => AttributeType::Href("https://example.org".into()),
            // A text the input rule reads (and leaves as it is).
            1 => AttributeType::AriaLabel(format!("label {}", rng.below(9)).into()),
            2 => AttributeType::Title("a title".into()),
            3 => AttributeType::Data(AttributeNameValue {
                attr_name: "data-n".into(),
                value: format!("{}", rng.below(9)).into(),
            }),
            _ => AttributeType::Hidden,
        }
    }

    fn spec(rng: &mut Rng, depth: u32, budget: &mut u32) -> Spec {
        *budget = budget.saturating_sub(1);
        let leaf = depth >= 5 || *budget == 0;
        let kind = match rng.below(if leaf { 12 } else { 7 }) {
            0 | 1 => Kind::Element(NodeType::Div),
            2 => Kind::Element(NodeType::Span),
            3 => Kind::Element(NodeType::P),
            4 => Kind::Element(NodeType::Input),
            5 => Kind::Element(NodeType::Button),
            6 => Kind::Element(NodeType::Br),
            7 | 8 => Kind::Text(match rng.below(4) {
                0 => String::new(),
                n => format!("text {n} {}", rng.below(1000)),
            }),
            9 => Kind::TextByValue(format!("by value {}", rng.below(1000))),
            10 => Kind::LocalizedText(format!("key-{}", rng.below(9))),
            _ if rng.one_in(2) => Kind::Image,
            _ => Kind::VirtualView,
        };
        let mut out = plain(kind);
        out.whole_first = rng.one_in(2);
        for _ in 0..rng.below(3) {
            out.classes.push(format!("c{}", rng.below(5)));
        }
        if rng.one_in(4) {
            out.ids.push(format!("id{}", rng.below(50)));
        }
        if rng.one_in(4) {
            for _ in 0..=rng.below(2) {
                out.attrs.push(attr(rng));
            }
        }
        for _ in 0..rng.below(4) {
            out.props.push(prop(rng));
        }
        if rng.one_in(3) {
            for _ in 0..=rng.below(3) {
                out.callbacks.push(EventFilter::Hover(match rng.below(3) {
                    0 => HoverEventFilter::MouseUp,
                    1 => HoverEventFilter::MouseEnter,
                    _ => HoverEventFilter::MouseLeave,
                }));
            }
        }
        if rng.one_in(3) {
            out.key = Some(format!("key {}", rng.below(10_000)));
        }
        if rng.one_in(6) {
            out.tab_index = Some(match rng.below(3) {
                0 => TabIndex::Auto,
                1 => TabIndex::OverrideInParent(u32::try_from(rng.below(9)).unwrap_or(0)),
                _ => TabIndex::NoKeyboardFocus,
            });
        }
        out.contenteditable = rng.one_in(9);
        out.dataset = rng.one_in(6);
        if rng.one_in(8) {
            out.accessibility = Some(format!("name {}", rng.below(9)));
        }
        if rng.one_in(8) {
            out.marker = Some(format!("marker {}", rng.below(9)));
        }
        if rng.one_in(6) {
            for _ in 0..=rng.below(2) {
                let priority = if rng.one_in(2) {
                    rule_priority::AUTHOR
                } else {
                    rule_priority::INLINE
                };
                out.sheets.push(sheet(rng.below(300), priority));
            }
        }
        if !leaf {
            for _ in 0..rng.below(5) {
                if *budget == 0 {
                    break;
                }
                out.children.push(spec(rng, depth + 1, budget));
            }
        }
        out
    }

    /// A node of `kind` with nothing of its own.
    fn plain(kind: Kind) -> Spec {
        Spec {
            kind,
            classes: Vec::new(),
            ids: Vec::new(),
            attrs: Vec::new(),
            props: Vec::new(),
            callbacks: Vec::new(),
            key: None,
            tab_index: None,
            contenteditable: false,
            dataset: false,
            accessibility: None,
            marker: None,
            sheets: Vec::new(),
            whole_first: false,
            children: Vec::new(),
        }
    }

    /// A random UI of at most `budget` nodes under a `div`.
    fn ui(seed: u64, budget: u32) -> Spec {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut budget = budget;
        let mut root = spec(&mut rng, 0, &mut budget);
        // The root is an element with children, so the builder's result is carried flat.
        root.kind = Kind::Element(NodeType::Div);
        while root.children.len() < 2 {
            let mut one = 3;
            root.children.push(spec(&mut rng, 4, &mut one));
        }
        root
    }

    fn view_node(shared: &Shared) -> NodeData {
        NodeData::create_virtual_view(
            shared.data.clone(),
            VirtualViewCallback {
                cb: view_cb,
                ctx: OptionRefAny::None,
            },
        )
        .with_virtual_view_version(7)
    }

    /// The UI through `Dom`'s own methods.
    fn through_dom(spec: &Spec, shared: &Shared) -> Dom {
        let mut dom = match &spec.kind {
            Kind::Element(node_type) => Dom::create_node(node_type.clone()),
            Kind::Text(text) => {
                Dom::create_text_do_not_use_without_block_level_wrapper(text.as_str())
            }
            Kind::TextByValue(text) => Dom::create_node(NodeType::Text(BoxOrStatic::heap(
                AzString::from_string(text.clone()),
            ))),
            Kind::LocalizedText(key) => {
                Dom::create_text_do_not_use_without_block_level_wrapper(AzString::tr(key))
            }
            Kind::Image => {
                Dom::create_node(NodeType::Image(BoxOrStatic::heap(shared.image.clone())))
            }
            Kind::VirtualView => Dom::create_from_data(view_node(shared)),
        };
        for class in &spec.classes {
            dom.add_class(AzString::from_string(class.clone()));
        }
        for id in &spec.ids {
            dom = dom.with_id(AzString::from_string(id.clone()));
        }
        for attr in &spec.attrs {
            dom = dom.with_attribute(attr.clone());
        }
        for prop in &spec.props {
            dom.add_css_property(prop.clone());
        }
        for event in &spec.callbacks {
            dom.add_callback(*event, shared.data.clone(), handler as usize);
        }
        if let Some(key) = &spec.key {
            dom.set_key(key.as_str());
        }
        if let Some(tab_index) = spec.tab_index {
            dom.set_tab_index(tab_index);
        }
        if spec.contenteditable {
            dom.set_contenteditable(true);
        }
        if spec.dataset {
            dom.set_dataset(OptionRefAny::Some(shared.data.clone()));
        }
        if let Some(name) = &spec.accessibility {
            dom.root
                .set_accessibility_info(SmallAriaInfo::label(name.as_str()).to_full_info());
        }
        if let Some(marker) = &spec.marker {
            dom.set_marker(OptionString::Some(AzString::from_string(marker.clone())));
        }
        for css in &spec.sheets {
            dom.add_component_css(css.clone());
        }
        for child in &spec.children {
            dom.add_child(through_dom(child, shared));
        }
        dom
    }

    /// What only a whole `NodeData` says, written onto the builder's open node.
    fn write_whole(spec: &Spec, b: &mut DomBuilder) {
        if spec.accessibility.is_none() && spec.marker.is_none() {
            return;
        }
        let node = b.node_data_mut().expect("the node is open");
        if let Some(name) = &spec.accessibility {
            node.set_accessibility_info(SmallAriaInfo::label(name.as_str()).to_full_info());
        }
        if let Some(marker) = &spec.marker {
            node.set_marker(OptionString::Some(AzString::from_string(marker.clone())));
        }
    }

    /// The UI through the builder.
    fn through_builder(spec: &Spec, shared: &Shared, b: &mut DomBuilder) {
        match &spec.kind {
            Kind::Element(node_type) => b.open(node_type.clone()),
            Kind::Text(text) => b.open_text_str(text),
            Kind::TextByValue(text) => b.open(NodeType::Text(BoxOrStatic::heap(
                AzString::from_string(text.clone()),
            ))),
            Kind::LocalizedText(key) => b.open_text(AzString::tr(key)),
            Kind::Image => b.open(NodeType::Image(BoxOrStatic::heap(shared.image.clone()))),
            Kind::VirtualView => b.open_node(view_node(shared)),
        }
        if spec.whole_first {
            write_whole(spec, b);
        }
        for class in &spec.classes {
            b.add_class_str(class);
        }
        for id in &spec.ids {
            b.add_id(AzString::from_string(id.clone()));
        }
        for attr in &spec.attrs {
            b.add_attribute(attr.clone());
        }
        for prop in &spec.props {
            b.add_css_property(prop.clone());
        }
        for event in &spec.callbacks {
            b.add_callback(*event, shared.data.clone(), handler as usize);
        }
        if let Some(key) = &spec.key {
            b.set_key(key.as_str());
        }
        if let Some(tab_index) = spec.tab_index {
            b.set_tab_index(tab_index);
        }
        if spec.contenteditable {
            b.set_contenteditable(true);
        }
        if spec.dataset {
            b.set_dataset(OptionRefAny::Some(shared.data.clone()));
        }
        if !spec.whole_first {
            write_whole(spec, b);
        }
        for css in &spec.sheets {
            b.add_component_css(css.clone());
        }
        for child in &spec.children {
            through_builder(child, shared, b);
        }
        b.close();
    }

    fn shared() -> Shared {
        Shared {
            image: ImageRef::null_image(4, 4, RawImageFormat::RGBA8, Vec::new()),
            data: RefAny::new(7u32),
        }
    }

    /// `(the builder's Dom, the tree Dom)` of one UI.
    fn both(spec: &Spec, shared: &Shared) -> (Dom, Dom) {
        let mut b = DomBuilder::new();
        through_builder(spec, shared, &mut b);
        (b.finish(), through_dom(spec, shared))
    }

    fn node_count(spec: &Spec) -> usize {
        1 + spec.children.iter().map(node_count).sum::<usize>()
    }

    // ---- equivalence -----------------------------------------------------------------

    /// The same UI built both ways is the same tree (every node, every sheet, every
    /// count), has the same fingerprints and carried values, and flattens to the same
    /// arenas.
    ///
    /// Mutations this fails on: the child count left out of `compact_shape_hash`; the
    /// key hashed before the attributes; a class hashed with the tag of an id; the
    /// conditions of an inline property left out of `compact_style_hash`; `next_sibling`
    /// not written in `push_node`; `last_child` written as the last descendant; a
    /// dataset not carried in `values_to_carry`; `node_of_parts` setting the key before
    /// the attributes box exists is fine, dropping the flags is not.
    #[test]
    fn a_ui_built_both_ways_is_the_same_tree_with_the_same_fingerprints() {
        let shared = shared();
        let mut carried = 0;
        for seed in 1..=60u64 {
            let spec = ui(seed, 12 + u32::try_from(seed).unwrap_or(0) * 2);
            let (flat, tree) = both(&spec, &shared);
            assert!(flat.is_flat(), "seed {seed}: harness: the root has children");
            assert_eq!(flat.node_count(), node_count(&spec), "seed {seed}: node count");
            carried += 1;

            // The fingerprints, read from the builder's tiers.
            let (flat_fp, flat_values) = fingerprint_built_dom(&flat);
            let (tree_fp, tree_values) = fingerprint_dom(&tree);
            assert_eq!(flat_fp.subtree_len, tree_fp.subtree_len, "seed {seed}: subtree_len");
            assert_eq!(flat_fp.structure, tree_fp.structure, "seed {seed}: structure");
            assert_eq!(flat_fp.style, tree_fp.style, "seed {seed}: style");
            assert_eq!(flat_fp.text_shapes, tree_fp.text_shapes, "seed {seed}: text shapes");
            assert_eq!(flat_fp.sheets, tree_fp.sheets, "seed {seed}: sheets");
            assert_eq!(flat_fp, tree_fp, "seed {seed}: the folds");
            assert!(
                flat_values.nodes == tree_values.nodes,
                "seed {seed}: the carried values"
            );

            // The arenas, against the flatten of the tree.
            let fast = flat.clone().into_fast_dom().ok().expect("a flat Dom");
            let compact = convert_dom_into_compact_dom(tree.clone());
            assert!(
                fast.node_data.as_ref() == &compact.node_data.internal[..],
                "seed {seed}: node data"
            );
            let hierarchy: Vec<NodeHierarchyItem> = compact
                .node_hierarchy
                .internal
                .iter()
                .map(|node| (*node).into())
                .collect();
            assert_eq!(fast.node_hierarchy.as_ref(), &hierarchy[..], "seed {seed}: hierarchy");

            // Node by node, copied out of the arenas.
            for (index, node) in compact.node_data.internal.iter().enumerate() {
                assert!(
                    flat.flat_node_data_at(index).as_ref() == Some(node),
                    "seed {seed}: node {index} copied out of the arenas"
                );
            }

            // The tree itself.
            let mut expanded = flat;
            expanded.expand_flat();
            assert!(!expanded.is_flat());
            assert!(expanded == tree, "seed {seed}: the expanded tree");
        }
        assert_eq!(carried, 60);
    }

    fn assert_styled_alike(flat: &StyledDom, tree: &StyledDom, case: &str) {
        assert!(flat.root == tree.root, "{case}: root");
        assert!(
            flat.node_hierarchy.as_ref() == tree.node_hierarchy.as_ref(),
            "{case}: hierarchy"
        );
        assert!(flat.node_data.as_ref() == tree.node_data.as_ref(), "{case}: node data");
        assert!(
            flat.styled_nodes.as_ref() == tree.styled_nodes.as_ref(),
            "{case}: node states"
        );
        assert!(
            flat.cascade_info.as_ref() == tree.cascade_info.as_ref(),
            "{case}: cascade info"
        );
        assert!(
            flat.nodes_with_window_callbacks.as_ref() == tree.nodes_with_window_callbacks.as_ref(),
            "{case}: window callbacks"
        );
        assert!(
            flat.nodes_with_datasets.as_ref() == tree.nodes_with_datasets.as_ref(),
            "{case}: datasets"
        );
        assert!(
            flat.focusable_nodes.as_ref() == tree.focusable_nodes.as_ref(),
            "{case}: focusable nodes"
        );
        assert!(
            flat.nodes_with_markers.as_ref() == tree.nodes_with_markers.as_ref(),
            "{case}: markers"
        );
        assert!(
            flat.tag_ids_to_node_ids.as_ref() == tree.tag_ids_to_node_ids.as_ref(),
            "{case}: tag ids"
        );
        assert!(
            flat.non_leaf_nodes.as_ref() == tree.non_leaf_nodes.as_ref(),
            "{case}: parents by depth"
        );
        // Every property every node resolves to, in the state it is in.
        for (index, node) in tree.node_data.as_ref().iter().enumerate() {
            let id = NodeId::new(index);
            let state = &tree.styled_nodes.as_ref()[index].styled_node_state;
            assert_eq!(
                flat.get_css_property_cache()
                    .get_computed_css_style_string(node, &id, state),
                tree.get_css_property_cache()
                    .get_computed_css_style_string(node, &id, state),
                "{case}: the style of node {index}"
            );
        }
    }

    /// The same UI built both ways is the same styled DOM: the flat one styled from its
    /// arenas (no tree in between), the tree flattened as ever. And a flat `Dom` styled
    /// through the tree path (expanded first) is that styled DOM too.
    ///
    /// Mutations this fails on: the sheets of `into_fast_dom` left in document order
    /// (an inner node's sheet after its ancestor's); the root's sheets not handed to
    /// `into_fast_dom`; a sheet's owner off by one.
    #[test]
    fn a_ui_built_both_ways_is_the_same_styled_dom() {
        let shared = shared();
        for seed in 1..=24u64 {
            let spec = ui(seed, 40);
            let (flat, tree) = both(&spec, &shared);
            let from_tree = StyledDom::create_from_dom(tree);
            let from_arenas = StyledDom::create_from_dom(flat.clone());
            assert_styled_alike(&from_arenas, &from_tree, &format!("seed {seed}, arenas"));

            let mut expanded = flat;
            expanded.expand_flat();
            let from_expanded = StyledDom::create_from_dom(expanded);
            assert_styled_alike(&from_expanded, &from_tree, &format!("seed {seed}, expanded"));
        }
    }

    /// A rebuild that switches from the tree form to the flat one (or back) and builds the
    /// same UI is unchanged to the early diff; one that changed a text names that text.
    ///
    /// Mutation this fails on: the text view of `hash_node` hashing the range instead of
    /// the characters; the text shape made with the characters in it.
    #[test]
    fn a_rebuild_from_one_form_to_the_other_is_unchanged_to_the_early_diff() {
        let shared = shared();
        // div [ span.row (key a) [ "one" ], "two" ]
        let before = Spec {
            children: vec![
                Spec {
                    classes: vec!["row".to_string()],
                    key: Some("a".to_string()),
                    children: vec![plain(Kind::Text("one".to_string()))],
                    ..plain(Kind::Element(NodeType::Span))
                },
                plain(Kind::Text("two".to_string())),
            ],
            ..plain(Kind::Element(NodeType::Div))
        };
        let (flat, tree) = both(&before, &shared);
        let (tree_fp, _) = fingerprint_dom(&tree);
        let (flat_fp, _) = fingerprint_built_dom(&flat);
        assert_eq!(tree_fp.structure_root, flat_fp.structure_root);
        assert_eq!(tree_fp.style_root, flat_fp.style_root);
        assert_eq!(same_tree_changes(&tree_fp, &flat_fp), None, "tree -> flat");
        assert_eq!(same_tree_changes(&flat_fp, &tree_fp), None, "flat -> tree");
        assert_eq!(texts_alone_changed(&tree_fp, &flat_fp), None);

        let mut after = before.clone();
        after.children[1].kind = Kind::Text("three".to_string());
        let (flat_after, tree_after) = both(&after, &shared);
        let (flat_after_fp, _) = fingerprint_built_dom(&flat_after);
        let (tree_after_fp, _) = fingerprint_dom(&tree_after);
        assert_eq!(flat_after_fp, tree_after_fp, "harness: the changed UI reads alike too");
        let changed = texts_alone_changed(&tree_fp, &flat_after_fp)
            .expect("a text changed and nothing else");
        assert_eq!(changed.len(), 1);
        let index = changed[0];
        assert_eq!(index, node_count(&before) - 1, "the last node is the changed text");
        assert_eq!(
            flat_after.flat_node_data_at(index),
            Some(NodeData::create_text_do_not_use_without_block_level_wrapper("three")),
            "the changed node, copied out of the arenas"
        );
    }

    // ---- the carrying Dom ------------------------------------------------------------

    /// A `Dom` that carries its tree flat becomes that tree wherever a tree is needed, and
    /// its root and sheets are the `Dom`'s own.
    ///
    /// Mutations this fails on: `add_child` / `set_children` / `collect` /
    /// `fixup_children_estimated` without `expand_flat`; `fingerprints_with_root` reading
    /// the builder's copy of the root.
    #[test]
    fn a_flat_dom_becomes_its_tree_wherever_a_tree_is_needed() {
        let shared = shared();
        let spec = ui(11, 30);
        let (flat, tree) = both(&spec, &shared);

        // As a child.
        let parent_of_flat = Dom::create_div().with_child(flat.clone());
        let parent_of_tree = Dom::create_div().with_child(tree.clone());
        assert!(parent_of_flat == parent_of_tree, "with_child");
        assert!(
            Dom::create_div().with_children(vec![flat.clone()].into()) == parent_of_tree,
            "with_children"
        );
        let collected: Dom = vec![flat.clone()].into_iter().collect();
        let collected_tree: Dom = vec![tree.clone()].into_iter().collect();
        assert!(collected == collected_tree, "collect");

        // Taking a child.
        let mut flat_with_child = flat.clone();
        flat_with_child.add_child(Dom::create_div());
        let mut tree_with_child = tree.clone();
        tree_with_child.add_child(Dom::create_div());
        assert!(flat_with_child == tree_with_child, "add_child onto a flat Dom");

        // Counted.
        let mut counted = flat.clone();
        assert_eq!(counted.fixup_children_estimated(), node_count(&spec) - 1);
        assert!(counted == tree, "fixup_children_estimated");

        // Its root is its own: a class written onto the carrying `Dom`, a sheet attached
        // to it, read by the fingerprints as on the tree.
        let mut flat_changed = flat.clone();
        flat_changed.add_class("late".into());
        flat_changed.add_component_css(sheet(5, rule_priority::AUTHOR));
        assert!(flat_changed.is_flat(), "the root's own setters keep the tree flat");
        let mut tree_changed = tree.clone();
        tree_changed.add_class("late".into());
        tree_changed.add_component_css(sheet(5, rule_priority::AUTHOR));
        assert_eq!(
            fingerprint_built_dom(&flat_changed).0,
            fingerprint_dom(&tree_changed).0,
            "the root as the Dom holds it"
        );
        assert_ne!(
            fingerprint_built_dom(&flat_changed).0.structure[0],
            fingerprint_built_dom(&flat).0.structure[0],
            "harness: the class moved the root's tier"
        );
        let mut expanded = flat_changed;
        expanded.expand_flat();
        assert!(expanded == tree_changed, "expanded with its changed root");

        // An empty builder and a single node are plain `Dom`s.
        assert!(DomBuilder::new().finish() == Dom::default());
        let mut one = DomBuilder::new();
        one.open(NodeType::P);
        one.add_class_str("only");
        let one = one.finish();
        assert!(!one.is_flat());
        assert!(one == Dom::create_node(NodeType::P).with_class("only".into()));

        // A tree handed to the builder is that tree's nodes, held whole.
        let mut around = DomBuilder::new();
        around.open(NodeType::Body);
        around.add_dom(tree.clone());
        around.add_dom(flat.clone());
        let mut around = around.finish();
        around.expand_flat();
        assert!(
            around
                == Dom::create_node(NodeType::Body)
                    .with_child(tree.clone())
                    .with_child(tree),
            "add_dom"
        );
    }

    // ---- counts ----------------------------------------------------------------------

    fn left(i: usize) -> CssPropertyWithConditions {
        CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft {
            inner: PixelValue::const_px(isize::try_from(i % 900).unwrap_or(0)),
        }))
    }

    fn top(i: usize) -> CssPropertyWithConditions {
        CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop {
            inner: PixelValue::const_px(isize::try_from(i % 700).unwrap_or(0)),
        }))
    }

    /// The label of the map editor: a keyed `span` with a class, `left` / `top` of its
    /// own and a text; `handlers` handlers each. `texts[i]` and `keys[i]` are its text
    /// and key.
    fn labels_through_dom(
        texts: &[String],
        keys: &[String],
        handlers: usize,
        data: &RefAny,
    ) -> Dom {
        let mut children = Vec::with_capacity(texts.len());
        for (i, (text, key)) in texts.iter().zip(keys.iter()).enumerate() {
            let mut span = Dom::create_span_with_text(text.as_str());
            span.set_key(key.as_str());
            span.add_class("label".into());
            span.add_css_property(left(i));
            span.add_css_property(top(i));
            for _ in 0..handlers {
                span.add_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    handler as usize,
                );
            }
            children.push(span);
        }
        Dom::create_div().with_children(children.into())
    }

    fn labels_through_builder(
        texts: &[String],
        keys: &[String],
        handlers: usize,
        data: &RefAny,
    ) -> Dom {
        let mut b = DomBuilder::with_capacity(2 * texts.len() + 1);
        b.open(NodeType::Div);
        for (i, (text, key)) in texts.iter().zip(keys.iter()).enumerate() {
            b.open(NodeType::Span);
            b.set_key(key.as_str());
            b.add_class_str("label");
            b.add_css_property(left(i));
            b.add_css_property(top(i));
            for _ in 0..handlers {
                b.add_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    handler as usize,
                );
            }
            b.add_text_str(text);
            b.close();
        }
        b.finish()
    }

    fn label_texts(n: usize) -> (Vec<String>, Vec<String>) {
        (
            (0..n).map(|i| format!("Label {i}")).collect(),
            (0..n).map(|i| format!("k{i}")).collect(),
        )
    }

    /// Building n labels through the builder allocates for the growth of its arenas
    /// and nothing per label; through `Dom` it allocates about ten times per label (the
    /// span's rarely-used box, its class string, its attribute list twice, its rule list
    /// and one declaration list per property, its child list, the text's box and
    /// string), eleven with handlers.
    ///
    /// Mutations this fails on: a class kept as an owned string per node; an inline
    /// property kept as a rule with its own declaration list; a `NodeData` made per node
    /// to hash it; the carried values made while building.
    #[test]
    fn building_labels_through_the_builder_allocates_for_the_arenas_only() {
        let data = RefAny::new(7u32);
        for handlers in [0usize, 3] {
            let mut flat_counts = Vec::new();
            for n in [1000usize, 4000] {
                let (texts, keys) = label_texts(n);
                let (flat, flat_allocations) =
                    allocations(|| labels_through_builder(&texts, &keys, handlers, &data));
                let (tree, tree_allocations) =
                    allocations(|| labels_through_dom(&texts, &keys, handlers, &data));
                assert_eq!(
                    fingerprint_built_dom(&flat).0,
                    fingerprint_dom(&tree).0,
                    "harness: both build the same {n} labels"
                );
                assert!(
                    flat_allocations <= 120,
                    "{n} labels, {handlers} handlers: the builder allocated \
                     {flat_allocations} times"
                );
                assert!(
                    tree_allocations >= 8 * n as u64,
                    "{n} labels, {handlers} handlers: `Dom` allocated {tree_allocations} \
                     times (about ten per label were counted by reading)"
                );
                flat_counts.push(flat_allocations);
            }
            assert!(
                flat_counts[1] <= flat_counts[0] + 40,
                "four times the labels: a few more growths, not four times the \
                 allocations ({flat_counts:?})"
            );
        }
    }

    /// The fingerprints of a builder-made tree are read: one node is hashed when they are
    /// asked for (the root, which the `Dom` holds), whatever the size of the tree; every
    /// other node was hashed once, as it was closed.
    ///
    /// Mutation this fails on: `fingerprint_built_dom` expanding the tree and walking it.
    #[test]
    fn the_fingerprints_of_a_builder_made_tree_hash_its_root_only() {
        let data = RefAny::new(7u32);
        for n in [100usize, 2000] {
            let (texts, keys) = label_texts(n);
            let closed_before = NODES_HASHED_AT_CLOSE.with(Cell::get);
            let flat = labels_through_builder(&texts, &keys, 1, &data);
            assert_eq!(
                NODES_HASHED_AT_CLOSE.with(Cell::get) - closed_before,
                2 * n as u64,
                "every node below the root is hashed once, as it is closed"
            );
            let asked_before = NODES_HASHED_AT_FINGERPRINT.with(Cell::get);
            let closed_before = NODES_HASHED_AT_CLOSE.with(Cell::get);
            let (fingerprints, values) = fingerprint_built_dom(&flat);
            assert_eq!(fingerprints.structure.len(), 2 * n + 1);
            assert_eq!(values.nodes.len(), n, "one carried value per label with a handler");
            assert_eq!(NODES_HASHED_AT_FINGERPRINT.with(Cell::get) - asked_before, 1);
            assert_eq!(NODES_HASHED_AT_CLOSE.with(Cell::get) - closed_before, 0);
            assert!(flat.is_flat(), "asking for the fingerprints leaves the tree flat");
        }
    }
}
