    /// Inject scroll bar DIVs with relevant event handlers into the DOM
    ///
    /// This function essentially takes a DOM and inserts a wrapper DIV
    /// on every parent. First, all scrollbars are set to "display:none;"
    /// with a special library-internal marker that indicates that this
    /// DIV is a scrollbar. Then later on in the layout code, the items
    /// are set to "display: flex / block" as necessary, because
    /// this way scrollbars aren't treated as "special" objects (the event
    /// handling for scrollbars are just regular callback handlers).
    pub fn inject_scroll_bars(&mut self) {
        use azul_css::parser2::CssApiWrapper;

        // allocate 14 nodes for every node
        //
        // 0: root component
        // 1: |- vertical container (flex-direction: column-reverse, flex-grow: 1)
        // 2:    |- horizontal scrollbar (height: 15px, flex-direction: row)
        // 3:    |  |- left thumb
        // 4:    |  |- middle content
        // 5:    |  |   |- thumb track
        // 6:    |  |- right thumb
        // 7:    |- content container (flex-direction: row-reverse, flex-grow: 1)
        // 8:       |- vertical scrollbar (width: 15px, flex-direction: column)
        // 9:       |   |- top thumb
        // 10:      |   |- middle content
        // 11:      |   |    |- thumb track
        // 12:      |   |- bottom thumb
        // 13:      |- content container (flex-direction: row, flex-grow: 1)
        // 14:          |- self.root
        //                  |- ... self.children

        let dom_to_inject = Dom::div()
        // .with_class("__azul-native-scroll-root-component".into())
        .with_inline_style("display:flex; flex-grow:1; flex-direction:column;".into())
        .with_children(vec![

            Dom::div()
            // .with_class("__azul-native-scroll-vertical-container".into())
            .with_inline_style("display:flex; flex-grow:1; flex-direction:column-reverse;".into())
            .with_children(vec![

                Dom::div()
                // .with_class("__azul-native-scroll-horizontal-scrollbar".into())
                .with_inline_style("display:flex; flex-grow:1; flex-direction:row; height:15px; background:grey;".into())
                .with_children(vec![
                    Dom::div(),
                    // .with_class("__azul-native-scroll-horizontal-scrollbar-track-left".into()),
                    Dom::div()
                    // .with_class("__azul-native-scroll-horizontal-scrollbar-track-middle".into())
                    .with_children(vec![
                        Dom::div()
                        // .with_class("__azul-native-scroll-horizontal-scrollbar-track-thumb".into())
                    ].into()),
                    Dom::div()
                    // .with_class("__azul-native-scroll-horizontal-scrollbar-track-right".into()),
                ].into()),

                Dom::div()
                // .with_class("__azul-native-scroll-content-container-1".into())
                .with_inline_style("display:flex; flex-grow:1; flex-direction:row-reverse;".into())
                .with_children(vec![

                    Dom::div()
                    // .with_class("__azul-native-scroll-vertical-scrollbar".into())
                    .with_inline_style("display:flex; flex-grow:1; flex-direction:column; width:15px; background:grey;".into())
                    .with_children(vec![
                       Dom::div(),
                       // .with_class("__azul-native-scroll-vertical-scrollbar-track-top".into()),
                       Dom::div()
                       // .with_class("__azul-native-scroll-vertical-scrollbar-track-middle".into())
                       .with_children(vec![
                           Dom::div()
                           // .with_class("__azul-native-scroll-vertical-scrollbar-track-thumb".into())
                       ].into()),
                       Dom::div()
                       // .with_class("__azul-native-scroll-vertical-scrollbar-track-bottom".into()),
                    ].into()),

                    Dom::div()
                    // .with_class("__azul-native-scroll-content-container-1".into())
                    .with_inline_style("display:flex; flex-grow:1; flex-direction:column;".into())
                    .with_children(vec![
                        Dom::div() // <- this div is where the new children will be injected into
                    ].into())
                ].into())
            ].into())
        ].into())
        .style(CssApiWrapper::empty());

        // allocate new nodes
        let nodes_to_allocate =
            self.node_data.len() + (self.non_leaf_nodes.len() * dom_to_inject.node_data.len());

        // pre-allocate a new DOM tree with self.nodes.len() * dom_to_inject.nodes.len() nodes

        let mut new_styled_dom = StyledDom {
            root: self.root,
            node_hierarchy: vec![NodeHierarchyItem::zeroed(); nodes_to_allocate].into(),
            node_data: vec![NodeData::default(); nodes_to_allocate].into(),
            styled_nodes: vec![StyledNode::default(); nodes_to_allocate].into(),
            cascade_info: vec![CascadeInfo::default(); nodes_to_allocate].into(),
            nodes_with_window_callbacks: self.nodes_with_window_callbacks.clone(),
            nodes_with_not_callbacks: self.nodes_with_not_callbacks.clone(),
            nodes_with_datasets: self.nodes_with_datasets.clone(),
            tag_ids_to_node_ids: self.tag_ids_to_node_ids.clone(),
            non_leaf_nodes: self.non_leaf_nodes.clone(),
            css_property_cache: self.css_property_cache.clone(),
            dom_id: self.dom_id,
        };

        // inject self.root as the nth node
        let inject_as_id = 0;

        #[cfg(feature = "std")]
        {
            println!(
                "inject scroll bars:\r\n{}",
                dom_to_inject.get_html_string("", "", true)
            );
        }

        // *self = new_styled_dom;
    }
