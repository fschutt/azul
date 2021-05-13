    use crate::option::OptionImageMask;
    use crate::option::OptionTabIndex;
    use crate::option::OptionRefAny;
    use crate::callbacks::IFrameCallback;
    use crate::callbacks::IFrameCallbackType;
    use crate::callbacks::RefAny;
    use crate::image::ImageRef;
    use crate::vec::DomVec;
    use crate::vec::IdOrClassVec;
    use crate::vec::CallbackDataVec;
    use crate::vec::NodeDataInlineCssPropertyVec;

    impl Dom {

        /// Creates an empty DOM with a give `NodeType`. Note: This is a `const fn` and
        /// doesn't allocate, it only allocates once you add at least one child node.
        #[inline]
        pub const fn const_new(node_type: NodeType) -> Self {
            const DEFAULT_VEC: DomVec = DomVec::from_const_slice(&[]);
            Self {
                root: NodeData::new(node_type),
                children: DEFAULT_VEC,
                total_children: 0,
            }
        }

        #[inline(always)]
        pub const fn const_div() -> Self { Self::new(NodeType::Div) }
        #[inline(always)]
        pub const fn const_body() -> Self { Self::new(NodeType::Body) }
        #[inline(always)]
        pub const fn const_br() -> Self { Self::new(NodeType::Br) }
    }

    impl NodeData {

        /// Creates a new `NodeData` instance from a given `NodeType`
        #[inline]
        pub const fn const_new(node_type: NodeType) -> Self {
            Self {
                node_type,
                dataset: OptionRefAny::None,
                ids_and_classes: IdOrClassVec::from_const_slice(&[]),
                callbacks: CallbackDataVec::from_const_slice(&[]),
                inline_css_props: NodeDataInlineCssPropertyVec::from_const_slice(&[]),
                clip_mask: OptionImageMask::None,
                tab_index: OptionTabIndex::None,
            }
        }

        /// Shorthand for `NodeData::new(NodeType::Body)`.
        #[inline(always)]
        pub const fn const_body() -> Self {
            Self::new(NodeType::Body)
        }

        /// Shorthand for `NodeData::new(NodeType::Div)`.
        #[inline(always)]
        pub const fn const_div() -> Self {
            Self::new(NodeType::Div)
        }

        /// Shorthand for `NodeData::new(NodeType::Br)`.
        #[inline(always)]
        pub const fn const_br() -> Self {
            Self::new(NodeType::Br)
        }

        // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
        // in the future (which is why the fields are all private).

        #[inline(always)]
        pub const fn const_get_node_type(&self) -> &NodeType { &self.node_type }
        #[inline(always)]
        pub const fn const_get_dataset(&self) -> &OptionRefAny { &self.dataset }
        #[inline(always)]
        pub const fn const_get_ids_and_classes(&self) -> &IdOrClassVec { &self.ids_and_classes }
        #[inline(always)]
        pub const fn const_get_callbacks(&self) -> &CallbackDataVec { &self.callbacks }
        #[inline(always)]
        pub const fn const_get_inline_css_props(&self) -> &NodeDataInlineCssPropertyVec { &self.inline_css_props }
        #[inline(always)]
        pub const fn const_get_clip_mask(&self) -> &OptionImageMask { &self.clip_mask }
        #[inline(always)]
        pub const fn const_get_tab_index(&self) -> OptionTabIndex { self.tab_index }
    }

    impl Default for Dom {
        fn default() -> Self {
            Dom::div()
        }
    }

    impl Default for NodeData {
        fn default() -> Self {
            NodeData::new(NodeType::Div)
        }
    }

    impl Default for TabIndex {
        fn default() -> Self {
            TabIndex::Auto
        }
    }

    impl core::iter::FromIterator<Dom> for Dom {
        fn from_iter<I: IntoIterator<Item=Dom>>(iter: I) -> Self {
            use crate::vec::DomVec;
            let mut total_children = 0;
            let children = iter.into_iter().map(|c| {
                total_children += c.total_children + 1;
                c
            }).collect::<DomVec>();

            Dom {
                root: NodeData::div(),
                children,
                total_children,
            }
        }
    }

    impl core::iter::FromIterator<NodeData> for Dom {
        fn from_iter<I: IntoIterator<Item=NodeData>>(iter: I) -> Self {
            use crate::vec::DomVec;
            let children = iter.into_iter().map(|c| Dom {
                root: c,
                children: DomVec::from_const_slice(&[]),
                total_children: 0
            }).collect::<DomVec>();
            let total_children = children.len();

            Dom {
                root: NodeData::div(),
                children: children,
                total_children,
            }
        }
    }

    impl core::iter::FromIterator<NodeType> for Dom {
        fn from_iter<I: core::iter::IntoIterator<Item=NodeType>>(iter: I) -> Self {
            iter.into_iter().map(|i| {
                let mut nd = NodeData::default();
                nd.node_type = i;
                nd
            }).collect()
        }
    }

    impl From<On> for AzEventFilter {
        fn from(on: On) -> AzEventFilter {
            on.into_event_filter()
        }
    }