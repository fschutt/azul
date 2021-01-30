    use crate::option::OptionImageMask;
    use crate::option::OptionTabIndex;
    use crate::option::OptionRefAny;
    use crate::callbacks::IFrameCallback;
    use crate::callbacks::IFrameCallbackType;
    use crate::callbacks::GlCallback;
    use crate::callbacks::GlCallbackType;
    use crate::callbacks::RefAny;
    use crate::resources::ImageId;
    use crate::resources::FontId;
    use crate::vec::DomVec;
    use crate::vec::IdOrClassVec;
    use crate::vec::CallbackDataVec;
    use crate::vec::NodeDataInlineCssPropertyVec;
    use crate::css::Css;
    use crate::style::StyledDom;

    impl Dom {

        /// Creates an empty DOM with a give `NodeType`. Note: This is a `const fn` and
        /// doesn't allocate, it only allocates once you add at least one child node.
        #[inline]
        pub const fn new(node_type: NodeType) -> Self {
            const DEFAULT_VEC: DomVec = DomVec::from_const_slice(&[]);
            Self {
                root: NodeData::new(node_type),
                children: DEFAULT_VEC,
                estimated_total_children: 0,
            }
        }

        #[inline(always)]
        pub const fn div() -> Self { Self::new(NodeType::Div) }
        #[inline(always)]
        pub const fn body() -> Self { Self::new(NodeType::Body) }
        #[inline(always)]
        pub const fn br() -> Self { Self::new(NodeType::Br) }
        #[inline(always)]
        pub fn label<S: Into<AzString>>(value: S) -> Self { Self::new(NodeType::Label(value.into())) }
        #[inline(always)]
        pub const fn image(image: ImageId) -> Self { Self::new(NodeType::Image(image)) }
        #[inline(always)]
        #[cfg(feature = "opengl")]
        pub fn gl_texture(data: RefAny, callback: GlCallbackType) -> Self { Self::new(NodeType::GlTexture(GlTextureNode { callback: GlCallback { cb: callback }, data })) }
        #[inline(always)]
        pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self { Self::new(NodeType::IFrame(IFrameNode { callback: IFrameCallback { cb: callback }, data })) }
        /// Shorthand for `Dom::default()`.
        #[inline(always)]
        pub const fn const_default() -> Self { Self::div() }

        #[inline(always)]
        pub fn with_dataset(mut self, data: RefAny) -> Self { self.set_dataset(data); self }
        #[inline(always)]
        pub fn with_ids_and_classes(mut self, ids: IdOrClassVec) -> Self { self.set_ids_and_classes(ids); self }
        #[inline(always)]
        pub fn with_inline_css_props(mut self, properties: NodeDataInlineCssPropertyVec) -> Self { self.set_inline_css_props(properties); self }
        #[inline(always)]
        pub fn with_callbacks(mut self, callbacks: CallbackDataVec) -> Self { self.set_callbacks(callbacks); self }
        #[inline(always)]
        pub fn with_children(mut self, children: DomVec) -> Self { self.set_children(children); self }
        #[inline(always)]
        pub fn with_clip_mask(mut self, clip_mask: OptionImageMask) -> Self { self.set_clip_mask(clip_mask); self }
        #[inline(always)]
        pub fn with_tab_index(mut self, tab_index: OptionTabIndex) -> Self { self.set_tab_index(tab_index); self }

        #[inline(always)]
        pub fn set_dataset(&mut self, data: RefAny) { self.root.set_dataset(Some(data).into()); }
        #[inline(always)]
        pub fn set_ids_and_classes(&mut self, ids: IdOrClassVec) { self.root.set_ids_and_classes(ids); }
        #[inline(always)]
        pub fn set_inline_css_props(&mut self, properties: NodeDataInlineCssPropertyVec) { self.root.set_inline_css_props(properties); }
        #[inline(always)]
        pub fn set_callbacks(&mut self, callbacks: CallbackDataVec) { self.root.set_callbacks(callbacks); }
        #[inline(always)]
        pub fn set_children(&mut self, children: DomVec) {
            self.estimated_total_children = 0;
            for c in children.iter() {
                self.estimated_total_children += c.estimated_total_children + 1;
            }
            self.children = children;
        }
        #[inline(always)]
        pub fn set_clip_mask(&mut self, clip_mask: OptionImageMask) { self.root.set_clip_mask(clip_mask); }
        #[inline(always)]
        pub fn set_tab_index(&mut self, tab_index: OptionTabIndex) { self.root.set_tab_index(tab_index); }
        #[inline(always)]
        pub fn style(self, css: Css) -> StyledDom { StyledDom::new(self, css) }
    }

    impl NodeData {

        /// Creates a new `NodeData` instance from a given `NodeType`
        #[inline]
        pub const fn new(node_type: NodeType) -> Self {
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
        pub const fn body() -> Self {
            Self::new(NodeType::Body)
        }

        /// Shorthand for `NodeData::new(NodeType::Div)`.
        #[inline(always)]
        pub const fn div() -> Self {
            Self::new(NodeType::Div)
        }

        /// Shorthand for `NodeData::new(NodeType::Br)`.
        #[inline(always)]
        pub const fn br() -> Self {
            Self::new(NodeType::Br)
        }

        /// Shorthand for `NodeData::default()`.
        #[inline(always)]
        pub const fn const_default() -> Self {
            Self::div()
        }

        /// Shorthand for `NodeData::new(NodeType::Label(value.into()))`
        #[inline(always)]
        pub fn label<S: Into<AzString>>(value: S) -> Self {
            Self::new(NodeType::Label(value.into()))
        }

        /// Shorthand for `NodeData::new(NodeType::Image(image_id))`
        #[inline(always)]
        pub fn image(image: ImageId) -> Self {
            Self::new(NodeType::Image(image))
        }

        #[inline(always)]
        #[cfg(feature = "opengl")]
        pub fn gl_texture(data: RefAny, callback: GlCallbackType) -> Self {
            Self::new(NodeType::GlTexture(GlTextureNode { callback: GlCallback { cb: callback }, data }))
        }

        #[inline(always)]
        pub fn iframe(data: RefAny, callback: IFrameCallbackType) -> Self {
            Self::new(NodeType::IFrame(IFrameNode { callback: IFrameCallback { cb: callback }, data }))
        }

        // NOTE: Getters are used here in order to allow changing the memory allocator for the NodeData
        // in the future (which is why the fields are all private).

        #[inline(always)]
        pub const fn get_node_type(&self) -> &NodeType { &self.node_type }
        #[inline(always)]
        pub const fn get_dataset(&self) -> &OptionRefAny { &self.dataset }
        #[inline(always)]
        pub const fn get_ids_and_classes(&self) -> &IdOrClassVec { &self.ids_and_classes }
        #[inline(always)]
        pub const fn get_callbacks(&self) -> &CallbackDataVec { &self.callbacks }
        #[inline(always)]
        pub const fn get_inline_css_props(&self) -> &NodeDataInlineCssPropertyVec { &self.inline_css_props }
        #[inline(always)]
        pub const fn get_clip_mask(&self) -> &OptionImageMask { &self.clip_mask }
        #[inline(always)]
        pub const fn get_tab_index(&self) -> OptionTabIndex { self.tab_index }

        #[inline(always)]
        pub fn set_node_type(&mut self, node_type: NodeType) { self.node_type = node_type; }
        #[inline(always)]
        pub fn set_dataset(&mut self, data: OptionRefAny) { self.dataset = data; }
        #[inline(always)]
        pub fn set_ids_and_classes(&mut self, ids_and_classes: IdOrClassVec) { self.ids_and_classes = ids_and_classes; }
        #[inline(always)]
        pub fn set_callbacks(&mut self, callbacks: CallbackDataVec) { self.callbacks = callbacks; }
        #[inline(always)]
        pub fn set_inline_css_props(&mut self, inline_css_props: NodeDataInlineCssPropertyVec) { self.inline_css_props = inline_css_props; }
        #[inline(always)]
        pub fn set_clip_mask(&mut self, clip_mask: OptionImageMask) { self.clip_mask = clip_mask; }
        #[inline(always)]
        pub fn set_tab_index(&mut self, tab_index: OptionTabIndex) { self.tab_index = tab_index; }

        #[inline(always)]
        pub fn with_node_type(self, node_type: NodeType) -> Self { Self { node_type, .. self } }
        #[inline(always)]
        pub fn with_dataset(self, data: OptionRefAny) -> Self { Self { dataset: data, .. self } }
        #[inline(always)]
        pub fn with_ids_and_classes(self, ids_and_classes: IdOrClassVec) -> Self { Self { ids_and_classes, .. self } }
        #[inline(always)]
        pub fn with_callbacks(self, callbacks: CallbackDataVec) -> Self { Self { callbacks, .. self } }
        #[inline(always)]
        pub fn with_inline_css_props(self, inline_css_props: NodeDataInlineCssPropertyVec) -> Self { Self { inline_css_props, .. self } }
        #[inline(always)]
        pub fn with_clip_mask(self, clip_mask: OptionImageMask) -> Self { Self { clip_mask, .. self } }
        #[inline(always)]
        pub fn with_tab_index(self, tab_index: OptionTabIndex) -> Self { Self { tab_index, .. self } }

    }

    impl Default for Dom {
        fn default() -> Self {
            Dom::const_default()
        }
    }

    impl Default for NodeData {
        fn default() -> Self {
            NodeData::const_default()
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
            let mut estimated_total_children = 0;
            let children = iter.into_iter().map(|c| {
                estimated_total_children += c.estimated_total_children + 1;
                c
            }).collect::<DomVec>();

            Dom {
                root: NodeData::div(),
                children,
                estimated_total_children,
            }
        }
    }

    impl core::iter::FromIterator<NodeData> for Dom {
        fn from_iter<I: IntoIterator<Item=NodeData>>(iter: I) -> Self {
            use crate::vec::DomVec;
            let children = iter.into_iter().map(|c| Dom {
                root: c,
                children: DomVec::from_const_slice(&[]),
                estimated_total_children: 0
            }).collect::<DomVec>();
            let estimated_total_children = children.len();

            Dom {
                root: NodeData::div(),
                children: children,
                estimated_total_children,
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