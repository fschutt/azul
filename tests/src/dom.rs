#[cfg(test)]
use azul_core::dom::*;
use azul_core::id_tree::NodeDataContainer;
#[cfg(test)]
use azul_css::AzString;

// Fix for test_compact_dom_conversion
#[test]
fn test_compact_dom_conversion() {
    use azul_css::StringVec;

    let dom: Dom = Dom::new_body()
        .with_child(
            Dom::new_div()
                .with_ids_and_classes(vec![IdOrClass::Class("class1".to_string().into())].into()),
        )
        .with_child(
            Dom::new_div()
                .with_ids_and_classes(vec![IdOrClass::Class("class1".to_string().into())].into())
                .with_child(Dom::new_div().with_ids_and_classes(
                    vec![IdOrClass::Id("child_2".to_string().into())].into(),
                )),
        )
        .with_child(
            Dom::new_div()
                .with_ids_and_classes(vec![IdOrClass::Class("class1".to_string().into())].into()),
        );

    let c0 = vec![IdOrClass::Class("class1".to_string().into())];
    let c1 = vec![IdOrClass::Class("class1".to_string().into())];
    let c2 = vec![IdOrClass::Id("child_2".to_string().into())];
    let c3 = vec![IdOrClass::Class("class1".to_string().into())];

    let expected_dom: CompactDom = CompactDom {
        root: NodeId::ZERO,
        node_hierarchy: NodeHierarchy {
            internal: vec![
                Node /* 0 */ {
                parent: None,
                previous_sibling: None,
                next_sibling: None,
                last_child: Some(NodeId::new(4)),
            },
                Node /* 1 */ {
                parent: Some(NodeId::new(0)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(2)),
                last_child: None,
            },
                Node /* 2 */ {
                parent: Some(NodeId::new(0)),
                previous_sibling: Some(NodeId::new(1)),
                next_sibling: Some(NodeId::new(4)),
                last_child: Some(NodeId::new(3)),
            },
                Node /* 3 */ {
                parent: Some(NodeId::new(2)),
                previous_sibling: None,
                next_sibling: None,
                last_child: None,
            },
                Node /* 4 */ {
                parent: Some(NodeId::new(0)),
                previous_sibling: Some(NodeId::new(2)),
                next_sibling: None,
                last_child: None,
            },
            ],
        },
        node_data: NodeDataContainer {
            internal: vec![
                /* 0 */ NodeData::new_body(),
                /* 1 */ NodeData::new_div().with_ids_and_classes(c0.into()),
                /* 2 */ NodeData::new_div().with_ids_and_classes(c1.into()),
                /* 3 */ NodeData::new_div().with_ids_and_classes(c2.into()),
                /* 4 */ NodeData::new_div().with_ids_and_classes(c3.into()),
            ],
        },
    };

    let got_dom = convert_dom_into_compact_dom(dom);
    if got_dom != expected_dom {
        panic!(
            "{}",
            format!(
                "expected compact dom: ----\r\n{:#?}\r\n\r\ngot compact dom: ----\r\n{:#?}\r\n",
                expected_dom, got_dom
            )
        );
    }
}

// Fix for test_dom_sibling_1
#[test]
fn test_dom_sibling_1() {
    use azul_css::StringVec;

    let dom: Dom = Dom::new_div()
        .with_child(
            Dom::new_div()
                .with_ids_and_classes(vec![IdOrClass::Id("sibling-1".to_string().into())].into())
                .with_child(Dom::new_div().with_ids_and_classes(
                    vec![IdOrClass::Id("sibling-1-child-1".to_string().into())].into(),
                )),
        )
        .with_child(
            Dom::new_div()
                .with_ids_and_classes(vec![IdOrClass::Id("sibling-2".to_string().into())].into())
                .with_child(Dom::new_div().with_ids_and_classes(
                    vec![IdOrClass::Id("sibling-2-child-1".to_string().into())].into(),
                )),
        );

    let dom = convert_dom_into_compact_dom(dom);

    let node_data = &dom.node_data;
    let node_hierarchy = &dom.node_hierarchy;

    assert_eq!(NodeId::new(0), dom.root);

    let v: Vec<AzString> = vec!["sibling-1".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(
        v,
        node_data.as_ref()[node_hierarchy.as_ref()[dom.root]
            .get_first_child(dom.root)
            .expect("root has no first child")]
        .get_ids_and_classes()
        .as_slice()
        .into_iter()
        .map(|s| match s {
            IdOrClass::Class(s) => s.clone_self(),
            IdOrClass::Id(s) => s.clone_self(),
        })
        .collect::<Vec<_>>()
        .into()
    );

    let v: Vec<AzString> = vec!["sibling-2".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(
        v,
        node_data.as_ref()[node_hierarchy.as_ref()[node_hierarchy.as_ref()[dom.root]
            .get_first_child(dom.root)
            .expect("root has no first child")]
        .next_sibling
        .expect("root has no second sibling")]
        .get_ids_and_classes()
        .as_slice()
        .into_iter()
        .map(|s| match s {
            IdOrClass::Class(s) => s.clone_self(),
            IdOrClass::Id(s) => s.clone_self(),
        })
        .collect::<Vec<_>>()
        .into()
    );

    let v: Vec<AzString> = vec!["sibling-1-child-1".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(
        v,
        node_data.as_ref()[node_hierarchy.as_ref()[node_hierarchy.as_ref()[dom.root]
            .get_first_child(dom.root)
            .expect("root has no first child")]
        .get_first_child(
            node_hierarchy.as_ref()[dom.root]
                .get_first_child(dom.root)
                .expect("root has no first child")
        )
        .expect("first child has no first child")]
        .get_ids_and_classes()
        .as_slice()
        .into_iter()
        .map(|s| match s {
            IdOrClass::Class(s) => s.clone_self(),
            IdOrClass::Id(s) => s.clone_self(),
        })
        .collect::<Vec<_>>()
        .into()
    );

    let v: Vec<AzString> = vec!["sibling-2-child-1".to_string().into()];
    let v: StringVec = v.into();
    assert_eq!(
        v,
        node_data.as_ref()[node_hierarchy.as_ref()[node_hierarchy.as_ref()[node_hierarchy
            .as_ref()[dom.root]
            .get_first_child(dom.root)
            .expect("root has no first child")]
        .next_sibling
        .expect("first child has no second sibling")]
        .get_first_child(
            node_hierarchy.as_ref()[node_hierarchy.as_ref()[dom.root]
                .get_first_child(dom.root)
                .expect("root has no first child")]
            .next_sibling
            .expect("first child has no second sibling")
        )
        .expect("second sibling has no first child")]
        .get_ids_and_classes()
        .as_slice()
        .into_iter()
        .map(|s| match s {
            IdOrClass::Class(s) => s.clone_self(),
            IdOrClass::Id(s) => s.clone_self(),
        })
        .collect::<Vec<_>>()
        .into()
    );
}

#[test]
fn test_dom_from_iter_1() {
    use azul_core::id_tree::Node;

    let dom = Dom::new_body().with_children(
        (0..5)
            .map(|e| Dom::text(format!("{}", e + 1)))
            .collect::<Vec<_>>()
            .into(),
    );

    let dom = convert_dom_into_compact_dom(dom);

    let node_hierarchy = &dom.node_hierarchy;
    let node_data = &dom.node_data;
    let node_hierarchy = node_hierarchy.as_ref();
    let node_data = node_data.as_ref();

    // We need to have 6 nodes:
    //
    // root                 NodeId(0)
    //   |-> 1              NodeId(1)
    //   |-> 2              NodeId(2)
    //   |-> 3              NodeId(3)
    //   |-> 4              NodeId(4)
    //   '-> 5              NodeId(5)

    assert_eq!(node_hierarchy.len(), 6);

    // Check root node
    assert_eq!(
        node_hierarchy.get(NodeId::new(0)),
        Some(&Node {
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            last_child: Some(NodeId::new(5)),
        })
    );
    assert_eq!(
        node_data.get(NodeId::new(0)),
        Some(&NodeData::new(NodeType::Div))
    );

    assert_eq!(
        node_hierarchy.get(NodeId::new(node_hierarchy.len() - 1)),
        Some(&Node {
            parent: Some(NodeId::new(0)),
            previous_sibling: Some(NodeId::new(4)),
            next_sibling: None,
            last_child: None,
        })
    );

    assert_eq!(
        node_data.get(NodeId::new(node_data.len() - 1)),
        Some(&NodeData::text("5"))
    );
}
