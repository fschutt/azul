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
        let deficit_before = cache.tally.font_deficit;
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
            cache.tally.font_deficit > deficit_before,
            "a run drawn without the face that covers it is short of its font"
        );
    }

    /// The mock font manager, the chains of its mono family and a paragraph of that family:
    /// what the tests below shape.
    #[allow(clippy::type_complexity)] // a test fixture: the pieces a shaping call takes
    fn paragraph() -> (
        FontManager<FontRef>,
        HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        Vec<InlineContent>,
        FontChainKey,
    ) {
        let fm: FontManager<FontRef> =
            FontManager::new(FcFontCache::default()).expect("a font manager");
        let selectors = vec![FontSelector {
            family: "Azul Mock Mono".to_string(),
            ..FontSelector::default()
        }];
        let key = FontChainKey::from_selectors(&selectors);
        let chain = resolve_chain_on_miss(&key, &fm.fc_cache);
        let mut chains = HashMap::new();
        chains.insert(key.clone(), chain);
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
        (fm, chains, content, key)
    }

    /// The shaping that came up short of a font is counted by the cache that shaped it, and
    /// drained once: another cache beside it has none of it. It used to be one process-wide
    /// counter, drained by whichever layout pass came first.
    #[test]
    fn a_shaping_deficit_is_counted_by_the_cache_that_shaped_and_drained_once() {
        let (fm, chains, content, _) = paragraph();
        let constraints = UnifiedConstraints::default();
        let (mut short, mut other) = (TextShapingCache::new(), TextShapingCache::new());
        // No face is loaded: the run shapes short of its font.
        short
            .shape_content(
                &content,
                &[],
                &constraints,
                &chains,
                &fm.fc_cache,
                &fm.get_loaded_fonts(),
                &mut None,
            )
            .expect("the run shapes");
        assert!(
            short.take_font_shape_deficit() > 0,
            "a run shaped without its face is short of its font"
        );
        assert_eq!(short.take_font_shape_deficit(), 0, "drained: counted once");
        assert_eq!(other.take_font_shape_deficit(), 0, "no other cache's text is in it");
    }

    /// A face registered while the app runs changes the face a family resolves to, which the
    /// key of a shaped group holds nothing else of: the generation of the registered fonts is
    /// part of the key, so text shaped before a registration is shaped again after it, and
    /// asking again at the same generation shapes nothing.
    /// Mutation: leave `fonts_generation` out of the key and the third count is the second.
    #[test]
    fn text_shaped_before_a_registration_is_shaped_again_after_it() {
        let (fm, chains, content, key) = paragraph();
        let mut resolved = crate::solver3::getters::ResolvedFontChains::default();
        resolved
            .chains
            .insert(FontChainKeyOrRef::Chain(key), chains.values().next().cloned().expect("a chain"));
        let loader = crate::text3::default::PathLoader::new();
        let failed = fm.load_missing_for_chains(&resolved, |bytes, index| {
            loader.load_font_shared(bytes, index)
        });
        assert!(failed.is_empty(), "premise: the mock face loads: {failed:?}");
        let loaded = fm.get_loaded_fonts();
        let constraints = UnifiedConstraints::default();
        let mut cache = TextShapingCache::new();
        let shape = |cache: &mut TextShapingCache| {
            cache
                .shape_content(
                    &content,
                    &[],
                    &constraints,
                    &chains,
                    &fm.fc_cache,
                    &loaded,
                    &mut None,
                )
                .expect("the run shapes");
            cache.tally.groups_shaped
        };
        let first = shape(&mut cache);
        assert_eq!(first, 1, "one group shaped");
        assert_eq!(shape(&mut cache), first, "the same text at the same generation is a hit");

        let bytes = &crate::text3::ui_fonts::bundled_ui_fonts()[0].bytes;
        assert!(fm.registered_fonts.register_font("Azul Registration Face", bytes));
        cache.set_fonts_generation(fm.registered_fonts.generation());
        assert_eq!(shape(&mut cache), first + 1, "after a registration it is shaped again");
        assert_eq!(shape(&mut cache), first + 1, "and kept for the new generation");
    }
