/// Minimal test case to verify Taffy's align-items: stretch behavior
/// Tests if Taffy correctly stretches flex items to container height

#[test]
fn test_taffy_align_items_stretch() {
    use taffy::prelude::*;
    
    let mut taffy: TaffyTree<()> = TaffyTree::new();
    
    // Create 3 flex items with flex-grow but NO explicit height
    // Item 1: flex-grow: 1
    let item1 = taffy
        .new_leaf(Style {
            flex_grow: 1.0,
            size: Size {
                width: auto(),
                height: auto(),  // Should stretch to 100px
            },
            ..Default::default()
        })
        .unwrap();
    
    // Item 2: flex-grow: 2
    let item2 = taffy
        .new_leaf(Style {
            flex_grow: 2.0,
            size: Size {
                width: auto(),
                height: auto(),  // Should stretch to 100px
            },
            ..Default::default()
        })
        .unwrap();
    
    // Item 3: flex-grow: 3
    let item3 = taffy
        .new_leaf(Style {
            flex_grow: 3.0,
            size: Size {
                width: auto(),
                height: auto(),  // Should stretch to 100px
            },
            ..Default::default()
        })
        .unwrap();
    
    // Create flex container: 600px × 100px, flex-direction: row, align-items: stretch
    let container = taffy
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(AlignItems::Stretch),
                size: Size {
                    width: length(600.0),
                    height: length(100.0),
                },
                ..Default::default()
            },
            &[item1, item2, item3],
        )
        .unwrap();
    
    // Compute layout
    taffy
        .compute_layout(
            container,
            Size {
                width: AvailableSpace::Definite(600.0),
                height: AvailableSpace::Definite(100.0),
            },
        )
        .unwrap();
    
    // Check container layout
    let container_layout = taffy.layout(container).unwrap();
    println!("Container: size={:?}, location={:?}", container_layout.size, container_layout.location);
    assert_eq!(container_layout.size.width, 600.0, "Container width should be 600px");
    assert_eq!(container_layout.size.height, 100.0, "Container height should be 100px");
    
    // Check item 1 layout (flex-grow: 1, should get 1/6 = 100px width)
    let item1_layout = taffy.layout(item1).unwrap();
    println!("Item 1: size={:?}, location={:?}", item1_layout.size, item1_layout.location);
    assert_eq!(item1_layout.size.width, 100.0, "Item 1 width should be 100px (1/6 of 600px)");
    assert_eq!(item1_layout.size.height, 100.0, "Item 1 height should be STRETCHED to 100px");
    assert_eq!(item1_layout.location.x, 0.0, "Item 1 x should be 0");
    assert_eq!(item1_layout.location.y, 0.0, "Item 1 y should be 0 (top-aligned when stretched)");
    
    // Check item 2 layout (flex-grow: 2, should get 2/6 = 200px width)
    let item2_layout = taffy.layout(item2).unwrap();
    println!("Item 2: size={:?}, location={:?}", item2_layout.size, item2_layout.location);
    assert_eq!(item2_layout.size.width, 200.0, "Item 2 width should be 200px (2/6 of 600px)");
    assert_eq!(item2_layout.size.height, 100.0, "Item 2 height should be STRETCHED to 100px");
    assert_eq!(item2_layout.location.x, 100.0, "Item 2 x should be 100");
    assert_eq!(item2_layout.location.y, 0.0, "Item 2 y should be 0 (top-aligned when stretched)");
    
    // Check item 3 layout (flex-grow: 3, should get 3/6 = 300px width)
    let item3_layout = taffy.layout(item3).unwrap();
    println!("Item 3: size={:?}, location={:?}", item3_layout.size, item3_layout.location);
    assert_eq!(item3_layout.size.width, 300.0, "Item 3 width should be 300px (3/6 of 600px)");
    assert_eq!(item3_layout.size.height, 100.0, "Item 3 height should be STRETCHED to 100px");
    assert_eq!(item3_layout.location.x, 300.0, "Item 3 x should be 300");
    assert_eq!(item3_layout.location.y, 0.0, "Item 3 y should be 0 (top-aligned when stretched)");
}

#[derive(Debug, Copy, Clone)]
struct IntrinsicSizeContext {
    width: f32,
    height: f32,
}

#[test]
fn test_taffy_align_items_stretch_with_intrinsic_size() {
    use taffy::prelude::*;
    
    let mut taffy: TaffyTree<IntrinsicSizeContext> = TaffyTree::new();
    
    // Create flex item with intrinsic size (measure function)
    let item_with_intrinsic = taffy
        .new_leaf_with_context(
            Style {
                flex_grow: 1.0,
                size: Size {
                    width: auto(),
                    height: auto(),
                },
                ..Default::default()
            },
            IntrinsicSizeContext {
                width: 20.0,
                height: 15.0,
            },
        )
        .unwrap();
    
    // Create flex container
    let container = taffy
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: Some(AlignItems::Stretch),
                size: Size {
                    width: length(600.0),
                    height: length(100.0),
                },
                ..Default::default()
            },
            &[item_with_intrinsic],
        )
        .unwrap();
    
    // Compute layout with custom measure function
    taffy
        .compute_layout_with_measure(
            container,
            Size {
                width: AvailableSpace::Definite(600.0),
                height: AvailableSpace::Definite(100.0),
            },
            |known_dimensions, _available_space, _node_id, context, _style| {
                println!("Measure called: known_dimensions={:?}", known_dimensions);
                
                if let Some(ctx) = context {
                    Size {
                        width: known_dimensions.width.unwrap_or(ctx.width),
                        height: known_dimensions.height.unwrap_or(ctx.height),
                    }
                } else {
                    Size {
                        width: known_dimensions.width.unwrap_or(0.0),
                        height: known_dimensions.height.unwrap_or(0.0),
                    }
                }
            },
        )
        .unwrap();
    
    // Check if item is stretched despite having intrinsic size
    let item_layout = taffy.layout(item_with_intrinsic).unwrap();
    println!("Item with intrinsic: size={:?}, location={:?}", item_layout.size, item_layout.location);
    
    // Key question: Does Taffy call measure with known_dimensions.height = Some(100.0)?
    // And does the final layout have height = 100.0?
    assert_eq!(item_layout.size.width, 600.0, "Item width should fill container");
    assert_eq!(
        item_layout.size.height, 100.0,
        "Item height should be STRETCHED to 100px, not intrinsic 15px"
    );
    assert_eq!(item_layout.location.y, 0.0, "Item should be top-aligned when stretched");
}
