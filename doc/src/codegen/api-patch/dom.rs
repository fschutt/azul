
    
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

    
    impl NodeData {
        pub const fn const_new(node_type: NodeType) -> Self {
            use crate::option::{OptionRefAny, OptionTabIndex};
            Self {
                node_type,
                dataset: OptionRefAny::None,
                ids_and_classes: IdOrClassVec::from_const_slice(&[]),
                callbacks: CallbackDataVec::from_const_slice(&[]),
                inline_css_props: NodeDataInlineCssPropertyVec::from_const_slice(&[]),
                tab_index: OptionTabIndex::None,
                extra: ::core::ptr::null_mut(),
            }
        }

        pub const fn const_body() -> Self {
            Self::const_new(NodeType::Body)
        }

        pub const fn const_div() -> Self {
            Self::const_new(NodeType::Div)
        }

        pub const fn const_text(text: AzString) -> Self {
            Self::const_new(NodeType::Text(text))
        }
    }

    
    impl Dom {

        pub const fn const_new(node_data: NodeData) -> Self {
            Dom {
                root: node_data,
                children: DomVec::from_const_slice(&[]),
                estimated_total_children: 0,
            }
        }

        pub const fn const_body() -> Self {
            Self::const_new(NodeData::const_body())
        }

        pub const fn const_div() -> Self {
            Self::const_new(NodeData::const_div())
        }

        pub const fn const_text(text: AzString) -> Self {
            Self::const_new(NodeData::const_text(text))
        }
    }