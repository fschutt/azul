    use azul_css::props::basic::FontRef;

    use super::*;

    fn glyph_count(flow: &FlowLayout) -> usize {
        flow.fragment_layouts
            .values()
            .flat_map(|layout| layout.items.iter())
            .map(|positioned| match &positioned.item {
                ShapedItem::Cluster(c) => c.glyphs.len(),
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn draws_once_its_font_is_loaded() {
        let fm: FontManager<FontRef> =
            FontManager::new(FcFontCache::default()).expect("a font manager");
        let selectors = vec![FontSelector {
            family: "Azul Mock Mono".to_string(),
            ..FontSelector::default()
        }];
        let key = FontChainKey::from_selectors(&selectors);
        let chain = resolve_chain_on_miss(&key, &fm.fc_cache);
        let mut chain_cache = HashMap::new();
        chain_cache.insert(key.clone(), chain.clone());
        let style = Arc::new(StyleProperties {
            font_stack: FontStack::Stack(selectors),
            font_size_px: 20.0,
            ..StyleProperties::default()
        });
        let content = vec![InlineContent::Text(StyledRun {
            text: Arc::from("HELLO"),
            style,
            logical_start_byte: 0,
            source_node_id: None,
        })];
        let fragments = vec![LayoutFragment {
            id: "main".to_string(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::Definite(400.0),
                ..UnifiedConstraints::default()
            },
        }];
        let mut cache = TextShapingCache::new();

        let before = cache
            .layout_flow(
                &content,
                &[],
                &fragments,
                &chain_cache,
                &fm.fc_cache,
                &fm.get_loaded_fonts(),
                &mut None,
            )
            .expect("the run lays out");
        assert_eq!(
            glyph_count(&before),
            0,
            "premise: no face is loaded yet, nothing to draw with"
        );

        let mut resolved = crate::solver3::getters::ResolvedFontChains::default();
        resolved.chains.insert(FontChainKeyOrRef::Chain(key), chain);
        let loader = crate::text3::default::PathLoader::new();
        let failed = fm.load_missing_for_chains(&resolved, |bytes, index| {
            loader.load_font_shared(bytes, index)
        });
        assert!(
            failed.is_empty(),
            "premise: the mock face loads: {failed:?}"
        );

        let after = cache
            .layout_flow(
                &content,
                &[],
                &fragments,
                &chain_cache,
                &fm.fc_cache,
                &fm.get_loaded_fonts(),
                &mut None,
            )
            .expect("the run lays out");
        assert_eq!(
            glyph_count(&after),
            5,
            "the same run laid out again once its face is loaded draws its five glyphs"
        );
    }

    /// A char whose covering face is NOT loaded - its chain resolved after
    /// the layout loaded its faces, a key the pre-pass never saw (a run at a
    /// new optical size, SYSUI8) - is drawn by the chain's next face that is
    /// loaded, and the shaping counts as short of its font, so it is not
    /// cached and a pass with the face loaded shapes it again. It shaped to
    /// nothing.
    #[test]
    fn a_char_whose_covering_face_is_not_loaded_is_drawn_by_a_loaded_face() {
        let fm: FontManager<FontRef> =
            FontManager::new(FcFontCache::default()).expect("a font manager");
        let selectors = vec![FontSelector {
            family: "Azul Mock Mono".to_string(),
            ..FontSelector::default()
        }];
        let key = FontChainKey::from_selectors(&selectors);
        let chain = resolve_chain_on_miss(&key, &fm.fc_cache);
        let mut resolved = crate::solver3::getters::ResolvedFontChains::default();
        resolved
            .chains
            .insert(FontChainKeyOrRef::Chain(key.clone()), chain.clone());
        let loader = crate::text3::default::PathLoader::new();
        let failed = fm.load_missing_for_chains(&resolved, |bytes, index| {
            loader.load_font_shared(bytes, index)
        });
        assert!(
            failed.is_empty(),
            "premise: the mock face loads: {failed:?}"
        );

        // A face nobody loaded, covering everything, ahead of the mock face.
        let mut ahead = chain;
        let group = ahead
            .css_fallbacks
            .iter_mut()
            .find(|group| !group.fonts.is_empty())
            .expect("premise: the mock family matched");
        group.fonts.insert(
            0,
            rust_fontconfig::FontMatch {
                id: FontId::new(),
                unicode_ranges: vec![UnicodeRange {
                    start: 0,
                    end: 0x0010_FFFF,
                }],
                fallbacks: Vec::new(),
            },
        );
        let mut chain_cache = HashMap::new();
        chain_cache.insert(key, ahead);

        let style = Arc::new(StyleProperties {
            font_stack: FontStack::Stack(selectors),
            font_size_px: 20.0,
            ..StyleProperties::default()
        });
        let content = vec![InlineContent::Text(StyledRun {
            text: Arc::from("HELLO"),
            style,
            logical_start_byte: 0,
            source_node_id: None,
        })];
        let fragments = vec![LayoutFragment {
            id: "main".to_string(),
            constraints: UnifiedConstraints {
                available_width: AvailableSpace::Definite(400.0),
                ..UnifiedConstraints::default()
            },
        }];
        let mut cache = TextShapingCache::new();
        let deficit_before = thread_font_shape_deficit();
        let flow = cache
            .layout_flow(
                &content,
                &[],
                &fragments,
                &chain_cache,
                &fm.fc_cache,
                &fm.get_loaded_fonts(),
                &mut None,
            )
            .expect("the run lays out");
        assert_eq!(
            glyph_count(&flow),
            5,
            "the run is drawn by the loaded face behind the unloaded one"
        );
        assert!(
            thread_font_shape_deficit() > deficit_before,
            "a run drawn without the face that covers it is short of its font"
        );
    }
