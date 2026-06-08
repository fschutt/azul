/// Reproduction for the menubar "View" -> "V" clip bug.
///
/// Structure mirrors layout/src/widgets/menubar.rs:
///   bar  : display:flex; flex-direction:row; align-items:stretch; height:26px; width:100%
///   item : display:flex; flex-direction:row; align-items:center; padding:0 10px
///   text : "View"
///
/// Bug (task #11): a flex item whose single child is text gets its main-axis
/// (width) constrained to the cross-axis (height = 26px), clipping the text.
use azul_core::dom::{Dom, DomId};
use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::resources::RendererResources;
use azul_layout::font::loading::build_font_cache;
use azul_layout::font_traits::{FontManager, TextLayoutCache};
use azul_layout::paged::FragmentationContext;
use azul_layout::solver3::paged_layout::layout_document_paged_with_config;
use azul_layout::solver3::pagination::FakePageConfig;
use azul_layout::text3::default::PathLoader;
use azul_layout::xml::DomXmlExt;
use azul_layout::Solver3LayoutCache;
use std::collections::{BTreeMap, HashMap};

fn fresh_cache() -> Solver3LayoutCache {
    Solver3LayoutCache {
        tree: None,
        calculated_positions: Vec::new(),
        viewport: None,
        scroll_ids: HashMap::new(),
        scroll_id_to_node_id: HashMap::new(),
        counters: HashMap::new(),
        float_cache: HashMap::new(),
        cache_map: Default::default(),
        previous_positions: Vec::new(),
        cached_display_list: None,
        prev_dom_ptr: 0,
        prev_viewport: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::zero(),
        },
    }
}

/// Reproduce via the REAL widget: build_menubar_dom() + with_css scoping +
/// system: namespace. This is the actual path the app uses.
#[test]
fn test_real_menubar_widget_not_clipped() {
    use azul_core::menu::{Menu, MenuItem, MenuItemVec, StringMenuItem};
    use azul_core::styled_dom::StyledDom;
    use azul_core::dom::Dom;

    let item = |label: &str| MenuItem::String(StringMenuItem::create(label.into()));
    let menu = Menu::create(MenuItemVec::from_vec(vec![
        item("File"), item("Edit"), item("View"),
    ]));

    let bar = azul_layout::widgets::menubar::build_menubar_dom(&menu);
    // Wrap like a window root: column flex body holding the bar.
    let root = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 100%; height: 100%;")
        .with_child(bar);

    let styled_dom = StyledDom::create_from_dom(root);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");
    let mut layout_cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();

    let content_size = LogicalSize::new(800.0, 600.0);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect { origin: LogicalPosition::zero(), size: content_size };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let page_config = FakePageConfig::new();

    layout_document_paged_with_config(
        &mut layout_cache, &mut text_cache, fragmentation_context, &styled_dom, viewport,
        &mut font_manager, &BTreeMap::new(), &mut debug_messages, None, &renderer_resources,
        azul_core::resources::IdNamespace(0), DomId::ROOT_ID, font_loader, page_config,
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    ).expect("Layout should succeed");

    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");
    println!("\n=== REAL WIDGET TREE ===");
    for i in 0..20 {
        if let Some(node) = tree.get(i) {
            let size = node.used_size.unwrap_or_default();
            let intrinsic = tree.warm(i).and_then(|w| w.intrinsic_sizes).unwrap_or_default();
            println!(
                "Node {}: fc={:?} size=({:.1},{:.1}) intrinsic_min_w={:.1} max_w={:.1}",
                i, node.formatting_context, size.width, size.height,
                intrinsic.min_content_width, intrinsic.max_content_width
            );
        }
    }
    println!("\n=== REAL WIDGET TAFFY TRACE ===");
    if let Some(msgs) = debug_messages.as_ref() {
        for m in msgs.iter() {
            let s = format!("{:?}", m.message);
            if s.contains("TAFFY OUTPUT") || s.contains("TAFFY CHILD RESULT") || s.contains("set_unrounded") {
                println!("{}", s);
            }
        }
    }

    // The 3 item flex nodes should be wider than the 26px cross-axis height.
    let mut item_widths = Vec::new();
    for i in 0..20 {
        if let Some(node) = tree.get(i) {
            if matches!(node.formatting_context, azul_core::dom::FormattingContext::Flex) {
                let w = node.used_size.unwrap_or_default().width;
                if w > 0.0 && w < 400.0 { item_widths.push((i, w)); }
            }
        }
    }
    println!("\nReal widget item flex widths: {:?}", item_widths);
    for (idx, w) in &item_widths {
        assert!(*w > 30.0, "real menubar item node {} clipped to {} (bug #11)", idx, w);
    }
}

/// FAITHFUL to the real app: drive `solver3::layout_document` (the interactive /
/// headless entry point — NOT the paged wrapper) with a real `Some(system_style)`
/// whose platform matches `Platform::current()`, after preloading fonts exactly
/// like the app does. This tells us whether the *app* clips "View", independent of
/// the paged-path None bug.
#[test]
fn test_app_path_menubar_not_clipped() {
    use azul_core::menu::{Menu, MenuItem, MenuItemVec, StringMenuItem};
    use azul_core::styled_dom::StyledDom;
    use azul_core::dom::Dom;
    use azul_layout::solver3::getters::{
        collect_and_resolve_font_chains_with_registration, collect_font_ids_from_chains,
        compute_fonts_to_load, load_fonts_from_disk,
    };

    let item = |label: &str| MenuItem::String(StringMenuItem::create(label.into()));
    let menu = Menu::create(MenuItemVec::from_vec(vec![
        item("File"), item("Edit"), item("View"),
    ]));
    let bar = azul_layout::widgets::menubar::build_menubar_dom(&menu);
    let root = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 100%; height: 100%;")
        .with_child(bar);
    let styled_dom = StyledDom::create_from_dom(root);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("font manager");
    let mut layout_cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();
    let content_size = LogicalSize::new(800.0, 600.0);
    let viewport = LogicalRect { origin: LogicalPosition::zero(), size: content_size };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };

    // --- preload fonts exactly like paged_layout.rs / the app does ---
    let platform = azul_css::system::Platform::current();
    let fc_cache_clone = font_manager.fc_cache.clone();
    let chains = collect_and_resolve_font_chains_with_registration(
        &styled_dom, &fc_cache_clone, &mut font_manager, &platform,
    );
    let required = collect_font_ids_from_chains(&chains);
    let already = font_manager.get_loaded_font_ids();
    let to_load = compute_fonts_to_load(&required, &already);
    if !to_load.is_empty() {
        let r = load_fonts_from_disk(&to_load, &font_manager.fc_cache, &font_loader);
        font_manager.insert_fonts(r.loaded);
    }
    font_manager.set_font_chain_cache(chains.into_fontconfig_chains());

    // The app uses discover_system_style(); on Linux that is a Platform::Linux
    // SystemStyle. default_for_platform() gives the same platform class, so the
    // measurement font chain matches the loaded (Platform::current) chain.
    let system_style = std::sync::Arc::new(azul_css::system::SystemStyle::default_for_platform());

    let display_list = azul_layout::solver3::layout_document(
        &mut layout_cache,
        &mut text_cache,
        &styled_dom,
        viewport,
        &font_manager,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        true,
        Vec::new(),
        None,
        &azul_core::resources::ImageCache::default(),
        Some(system_style),
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
    )
    .expect("layout_document should succeed");

    // Inspect the display list: each menubar label must emit one glyph per char.
    // "File"/"Edit"/"View" = 4 glyphs each (View regressed to 1 before the fix).
    use azul_layout::solver3::display_list::DisplayListItem;
    let mut label_glyph_counts: Vec<usize> = Vec::new();
    for item in display_list.items.iter() {
        if let DisplayListItem::Text { glyphs, source_node_index, .. } = item {
            println!("DL Text: {} glyph(s) src_node={:?}", glyphs.len(), source_node_index);
            label_glyph_counts.push(glyphs.len());
        }
    }
    assert_eq!(label_glyph_counts, vec![4, 4, 4],
        "menubar labels File/Edit/View must each emit 4 glyphs, got {:?}", label_glyph_counts);

    let tree = layout_cache.tree.as_ref().expect("tree");
    println!("\n=== APP-PATH TREE (system_style=Some, Linux) ===");
    for i in 0..12 {
        if let Some(node) = tree.get(i) {
            let size = node.used_size.unwrap_or_default();
            let intr = tree.warm(i).and_then(|w| w.intrinsic_sizes).unwrap_or_default();
            println!("Node {}: fc={:?} size=({:.1},{:.1}) intr_min_w={:.1} max_w={:.1}",
                i, node.formatting_context, size.width, size.height,
                intr.min_content_width, intr.max_content_width);
        }
    }
    let mut item_widths = Vec::new();
    for i in 0..12 {
        if let Some(node) = tree.get(i) {
            if matches!(node.formatting_context, azul_core::dom::FormattingContext::Flex) {
                let w = node.used_size.unwrap_or_default().width;
                if w > 0.0 && w < 400.0 { item_widths.push((i, w)); }
            }
        }
    }
    println!("App-path item flex widths: {:?}", item_widths);
    for (idx, w) in &item_widths {
        assert!(*w > 30.0, "APP PATH: menubar item node {} clipped to {} — bug is in the app, not just paged", idx, w);
    }
    assert_eq!(item_widths.len(), 3, "expected 3 items, got {:?}", item_widths);
}

/// Probe many words through the REAL menubar widget + app layout path, dump the
/// per-word glyph count in the display list. Isolates whether the truncation is
/// word-specific or structural (e.g. cache-key collision).
#[test]
fn test_probe_words_glyph_counts() {
    use azul_core::menu::{Menu, MenuItem, MenuItemVec, StringMenuItem};
    use azul_core::styled_dom::StyledDom;
    use azul_core::dom::Dom;
    use azul_layout::solver3::getters::{
        collect_and_resolve_font_chains_with_registration, collect_font_ids_from_chains,
        compute_fonts_to_load, load_fonts_from_disk,
    };
    use azul_layout::solver3::display_list::DisplayListItem;

    let words = ["File", "Edit", "View", "View", "Wiew", "Xiew", "Viww", "Vie", "Vi", "Help", "AAAA", "Open"];
    let items: Vec<MenuItem> = words.iter()
        .map(|w| MenuItem::String(StringMenuItem::create((*w).into())))
        .collect();
    let menu = Menu::create(MenuItemVec::from_vec(items));
    let bar = azul_layout::widgets::menubar::build_menubar_dom(&menu);
    let root = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 100%; height: 100%;")
        .with_child(bar);
    let styled_dom = StyledDom::create_from_dom(root);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("fm");
    let mut layout_cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();
    let viewport = LogicalRect { origin: LogicalPosition::zero(), size: LogicalSize::new(1200.0, 600.0) };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let platform = azul_css::system::Platform::current();
    let fcc = font_manager.fc_cache.clone();
    let chains = collect_and_resolve_font_chains_with_registration(&styled_dom, &fcc, &mut font_manager, &platform);
    let required = collect_font_ids_from_chains(&chains);
    let already = font_manager.get_loaded_font_ids();
    let to_load = compute_fonts_to_load(&required, &already);
    if !to_load.is_empty() {
        let r = load_fonts_from_disk(&to_load, &font_manager.fc_cache, &font_loader);
        font_manager.insert_fonts(r.loaded);
    }
    font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
    let system_style = std::sync::Arc::new(azul_css::system::SystemStyle::default_for_platform());

    let display_list = azul_layout::solver3::layout_document(
        &mut layout_cache, &mut text_cache, &styled_dom, viewport, &font_manager,
        &BTreeMap::new(), &BTreeMap::new(), &mut debug_messages, None, &renderer_resources,
        azul_core::resources::IdNamespace(0), DomId::ROOT_ID, true, Vec::new(), None,
        &azul_core::resources::ImageCache::default(), Some(system_style),
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
    ).expect("layout");

    // Map each text node to its word via tree order; dump glyph counts.
    println!("\n=== PROBE WORD GLYPH COUNTS ===");
    let mut text_runs: Vec<(usize, usize)> = Vec::new(); // (src_node, glyph_count)
    for item in display_list.items.iter() {
        if let DisplayListItem::Text { glyphs, source_node_index, .. } = item {
            if let Some(n) = source_node_index {
                text_runs.push((*n, glyphs.len()));
            }
        }
    }
    text_runs.sort();
    for (i, (node, gc)) in text_runs.iter().enumerate() {
        let word = words.get(i).copied().unwrap_or("?");
        let flag = if *gc != word.chars().count() { "  <-- TRUNCATED" } else { "" };
        println!("word {:>5?} (node {}): {} glyphs (expected {}){}", word, node, gc, word.chars().count(), flag);
    }
    // Regression: every word must emit one glyph per character (no kerning-induced
    // truncation). Before the fix, positive-kerning words (View, Wiew, AAAA, …)
    // collapsed to a single glyph because max-content omitted kerning.
    for (i, (node, gc)) in text_runs.iter().enumerate() {
        let word = words.get(i).copied().unwrap_or("?");
        assert_eq!(
            *gc, word.chars().count(),
            "word {:?} (node {}) emitted {} glyphs, expected {} — kerning/max-content truncation regressed (#11)",
            word, node, gc, word.chars().count()
        );
    }
}

#[test]
fn test_menubar_item_text_not_clipped() {
    let html = r#"
    <html>
        <head>
            <style>
                .bar {
                    display: flex;
                    flex-direction: row;
                    align-items: stretch;
                    width: 100%;
                    height: 26px;
                }
                .item {
                    display: flex;
                    flex-direction: row;
                    align-items: center;
                    padding-left: 10px;
                    padding-right: 10px;
                }
            </style>
        </head>
        <body>
            <div class="bar">
                <div class="item">File</div>
                <div class="item">Edit</div>
                <div class="item">View</div>
            </div>
        </body>
    </html>
    "#;

    let styled_dom = Dom::from_xml_string(html);

    let fc_cache = build_font_cache();
    let mut font_manager = FontManager::new(fc_cache).expect("Failed to create font manager");
    let mut layout_cache = fresh_cache();
    let mut text_cache = TextLayoutCache::new();

    let content_size = LogicalSize::new(800.0, 600.0);
    let fragmentation_context = FragmentationContext::new_paged(content_size);
    let viewport = LogicalRect {
        origin: LogicalPosition::zero(),
        size: content_size,
    };
    let renderer_resources = RendererResources::default();
    let mut debug_messages = Some(Vec::new());
    let loader = PathLoader::new();
    let font_loader = |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
        loader.load_font_shared(bytes, index)
    };
    let page_config = FakePageConfig::new();

    layout_document_paged_with_config(
        &mut layout_cache,
        &mut text_cache,
        fragmentation_context,
        &styled_dom,
        viewport,
        &mut font_manager,
        &BTreeMap::new(),
        &mut debug_messages,
        None,
        &renderer_resources,
        azul_core::resources::IdNamespace(0),
        DomId::ROOT_ID,
        font_loader,
        page_config,
        &azul_core::resources::ImageCache::default(),
        azul_core::task::GetSystemTimeCallback { cb: azul_core::task::get_system_time_libstd },
        false,
    )
    .expect("Layout should succeed");

    let tree = layout_cache.tree.as_ref().expect("Layout tree should exist");

    // Dump the whole tree for orientation.
    println!("\n=== TREE ===");
    for i in 0..16 {
        if let Some(node) = tree.get(i) {
            let size = node.used_size.unwrap_or_default();
            let intrinsic = tree.warm(i).and_then(|w| w.intrinsic_sizes).unwrap_or_default();
            println!(
                "Node {}: fc={:?} size=({:.1},{:.1}) intrinsic_min_w={:.1} max_w={:.1}",
                i, node.formatting_context, size.width, size.height,
                intrinsic.min_content_width, intrinsic.max_content_width
            );
        }
    }

    // Dump taffy debug trace.
    println!("\n=== TAFFY TRACE ===");
    if let Some(msgs) = debug_messages.as_ref() {
        for m in msgs.iter() {
            let s = format!("{:?}", m.message);
            if s.contains("TAFFY") {
                println!("{}", s);
            }
        }
    }

    // Find each .item node: a Flex node whose used width should hold "View"+padding.
    // The 3 items are siblings; locate them by being Flex with a text descendant.
    let mut item_widths = Vec::new();
    for i in 0..16 {
        if let Some(node) = tree.get(i) {
            if matches!(node.formatting_context, azul_core::dom::FormattingContext::Flex) {
                let w = node.used_size.unwrap_or_default().width;
                // bar is full width (~800); items are the narrow ones
                if w > 0.0 && w < 400.0 {
                    item_widths.push((i, w));
                }
            }
        }
    }
    println!("\nItem flex widths: {:?}", item_widths);

    // Each item holds a 4-char word + 20px padding. Must be well above the 26px
    // cross-axis height (the bug clamps it to ~26).
    for (idx, w) in &item_widths {
        assert!(
            *w > 30.0,
            "menubar item node {} width {} is clipped to ~cross-axis-height (bug #11); expected >30",
            idx, w
        );
    }
    assert_eq!(item_widths.len(), 3, "expected 3 menubar items, got {:?}", item_widths);
}
