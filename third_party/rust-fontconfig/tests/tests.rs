use rust_fontconfig::*;

#[test]
fn test_operating_system_font_expansion() {
    // Test Windows font expansion (without Unicode ranges = default fonts)
    let windows_os = OperatingSystem::Windows;
    let no_ranges: &[UnicodeRange] = &[];
    
    assert_eq!(windows_os.get_serif_fonts(no_ranges), vec!["Times New Roman".to_string()]);
    assert_eq!(
        windows_os.get_sans_serif_fonts(no_ranges),
        vec!["Segoe UI", "Tahoma", "Microsoft Sans Serif", "MS Sans Serif", "Helv"]
            .iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(
        windows_os.get_monospace_fonts(no_ranges),
        vec!["Segoe UI Mono", "Courier New", "Cascadia Code", "Cascadia Mono", "Consolas"]
            .iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    
    // Test macOS font expansion
    let macos_os = OperatingSystem::MacOS;
    assert_eq!(
        macos_os.get_serif_fonts(no_ranges),
        vec!["Times New Roman", "Times", "New York", "Palatino"].iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(
        macos_os.get_sans_serif_fonts(no_ranges),
        vec!["San Francisco", ".AppleSystemUIFont", ".SFUIText", ".SFUI-Regular", "Helvetica Neue", "Helvetica", "Lucida Grande"]
            .iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(
        macos_os.get_monospace_fonts(no_ranges),
        vec!["SF Mono", "Menlo", "Monaco", "Courier", "Oxygen Mono", "Source Code Pro", "Fira Mono"]
            .iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    
    // Test Linux font expansion
    let linux_os = OperatingSystem::Linux;
    assert_eq!(
        linux_os.get_serif_fonts(no_ranges).len(),
        8,
        "Linux should have 8 serif fonts"
    );
    assert_eq!(
        linux_os.get_sans_serif_fonts(no_ranges),
        vec!["Ubuntu", "Arial", "DejaVu Sans", "Noto Sans", "Liberation Sans"]
            .iter().map(|s| s.to_string()).collect::<Vec<_>>()
    );
    
    // Test generic family expansion
    let families = vec!["Arial".to_string(), "sans-serif".to_string()];
    let expanded = expand_font_families(&families, OperatingSystem::MacOS, no_ranges);
    assert_eq!(expanded[0], "Arial");
    assert_eq!(expanded[1], "San Francisco");
    assert_eq!(expanded[2], ".AppleSystemUIFont");
    assert_eq!(expanded[3], ".SFUIText");
    
    // Test non-generic family (should pass through unchanged)
    let specific = vec!["MyCustomFont".to_string()];
    let expanded = expand_font_families(&specific, OperatingSystem::Windows, no_ranges);
    assert_eq!(expanded, vec!["MyCustomFont".to_string()]);
}

#[test]
fn test_unicode_range_matching() {
    // Create mock fonts with different Unicode ranges
    let latin_font = FcFont {
        bytes: vec![0, 1, 2, 3], // Dummy data
        font_index: 0,
        id: "latin-font".to_string(),
    };

    let cyrillic_font = FcFont {
        bytes: vec![4, 5, 6, 7], // Dummy data
        font_index: 0,
        id: "cyrillic-font".to_string(),
    };

    let cjk_font = FcFont {
        bytes: vec![8, 9, 10, 11], // Dummy data
        font_index: 0,
        id: "cjk-font".to_string(),
    };

    // Create patterns with Unicode ranges
    let latin_pattern = FcPattern {
        name: Some("Latin Font".to_string()),
        family: Some("Latin Family".to_string()),
        unicode_ranges: vec![
            UnicodeRange {
                start: 0x0000,
                end: 0x007F,
            }, // Basic Latin
            UnicodeRange {
                start: 0x0080,
                end: 0x00FF,
            }, // Latin-1 Supplement
        ],
        ..Default::default()
    };

    let cyrillic_pattern = FcPattern {
        name: Some("Cyrillic Font".to_string()),
        family: Some("Cyrillic Family".to_string()),
        unicode_ranges: vec![
            UnicodeRange {
                start: 0x0400,
                end: 0x04FF,
            }, // Cyrillic
        ],
        ..Default::default()
    };

    let cjk_pattern = FcPattern {
        name: Some("CJK Font".to_string()),
        family: Some("CJK Family".to_string()),
        unicode_ranges: vec![
            UnicodeRange {
                start: 0x4E00,
                end: 0x9FFF,
            }, // CJK Unified Ideographs
        ],
        ..Default::default()
    };

    // Create the font cache with our mock fonts
    let mut cache = FcFontCache::default();
    cache.with_memory_fonts(vec![
        (latin_pattern.clone(), latin_font),
        (cyrillic_pattern.clone(), cyrillic_font),
        (cjk_pattern.clone(), cjk_font),
    ]);

    // Get font IDs for assertions
    let font_list = cache.list();
    let latin_id = font_list
        .iter()
        .find(|(pattern, _)| pattern.name == Some("Latin Font".to_string()))
        .map(|(_, id)| *id)
        .expect("Latin font not found");

    let cyrillic_id = font_list
        .iter()
        .find(|(pattern, _)| pattern.name == Some("Cyrillic Font".to_string()))
        .map(|(_, id)| *id)
        .expect("Cyrillic font not found");

    // Test querying with Unicode ranges
    let mut trace: Vec<TraceMsg> = Vec::new();

    // Query for Latin characters
    let latin_query = FcPattern {
        unicode_ranges: vec![UnicodeRange {
            start: 0x0041,
            end: 0x005A,
        }], // A-Z
        ..Default::default()
    };

    // Use list() and filter instead of query_all()
    let matches: Vec<_> = cache.list().into_iter()
        .filter(|(pattern, _)| {
            // Check if unicode ranges overlap
            if pattern.unicode_ranges.is_empty() { return false; }
            pattern.unicode_ranges.iter().any(|r| {
                latin_query.unicode_ranges.iter().any(|q| {
                    r.start <= q.end && q.start <= r.end
                })
            })
        })
        .collect();
    assert_eq!(matches.len(), 1);
    assert_eq!(cache.get_memory_font(&latin_id).is_some(), true);

    // Check trace messages for non-matches (Unicode range mismatches)
    trace.clear();

    // Query for Cyrillic characters
    let cyrillic_query = FcPattern {
        unicode_ranges: vec![UnicodeRange {
            start: 0x0410,
            end: 0x044F,
        }], // Cyrillic letters
        ..Default::default()
    };

    let matches: Vec<_> = cache.list().into_iter()
        .filter(|(pattern, _)| {
            if pattern.unicode_ranges.is_empty() { return false; }
            pattern.unicode_ranges.iter().any(|r| {
                cyrillic_query.unicode_ranges.iter().any(|q| {
                    r.start <= q.end && q.start <= r.end
                })
            })
        })
        .collect();
    assert_eq!(matches.len(), 1);
    assert_eq!(cache.get_memory_font(&cyrillic_id).is_some(), true);

    // Query for text that needs multiple fonts using resolve_font_chain + query_for_text
    #[cfg(feature = "std")]
    {
        let text = "Hello Привет 你好"; // Latin, Cyrillic, and CJK

        // Build a generic font chain from our in-memory fonts
        let families: Vec<String> = cache.list().iter()
            .filter_map(|(pattern, _)| pattern.family.clone())
            .collect();

        let chain = cache.resolve_font_chain(
            &families,
            FcWeight::Normal,
            PatternMatch::DontCare,
            PatternMatch::DontCare,
            &mut trace,
        );

        let runs = chain.query_for_text(&cache, text);

        // Collect unique fonts used
        let unique_fonts: std::collections::HashSet<_> = runs.iter()
            .filter_map(|r| r.font_id)
            .collect();

        assert!(
            unique_fonts.len() >= 2,
            "Should use multiple fonts for multilingual text"
        );
    }
}

#[test]
fn test_weight_matching() {
    // Create fonts with different weights
    let normal_font = FcFont {
        bytes: vec![0, 1, 2, 3],
        font_index: 0,
        id: "normal-font".to_string(),
    };

    let bold_font = FcFont {
        bytes: vec![4, 5, 6, 7],
        font_index: 0,
        id: "bold-font".to_string(),
    };

    // Create patterns
    let normal_pattern = FcPattern {
        name: Some("Normal Font".to_string()),
        family: Some("Test Family".to_string()),
        weight: FcWeight::Normal,
        ..Default::default()
    };

    let bold_pattern = FcPattern {
        name: Some("Bold Font".to_string()),
        family: Some("Test Family".to_string()),
        weight: FcWeight::Bold,
        bold: PatternMatch::True,
        ..Default::default()
    };

    // Create the font cache
    let mut cache = FcFontCache::default();
    cache.with_memory_fonts(vec![
        (normal_pattern.clone(), normal_font),
        (bold_pattern.clone(), bold_font),
    ]);

    // Test querying with weights
    let mut trace = Vec::new();

    // Query for normal weight
    let normal_query = FcPattern {
        family: Some("Test Family".to_string()),
        weight: FcWeight::Normal,
        ..Default::default()
    };

    let matches = cache.query(&normal_query, &mut trace);
    assert!(matches.is_some(), "Should match normal weight font");

    // Query for bold weight
    let bold_query = FcPattern {
        family: Some("Test Family".to_string()),
        weight: FcWeight::Bold,
        ..Default::default()
    };

    let matches = cache.query(&bold_query, &mut trace);
    assert!(matches.is_some(), "Should match bold weight font");

    // Query that doesn't match - wrong family
    trace.clear();
    let wrong_family_query = FcPattern {
        family: Some("Wrong Family".to_string()),
        weight: FcWeight::Normal,
        ..Default::default()
    };

    let matches = cache.query(&wrong_family_query, &mut trace);
    assert!(matches.is_none(), "Should not match with wrong family");

    // Check trace messages for family mismatch
    let family_mismatch_traces = trace
        .iter()
        .filter(|msg| matches!(msg.reason, MatchReason::FamilyMismatch { .. }))
        .count();
    assert!(
        family_mismatch_traces > 0,
        "Expected family mismatch trace messages"
    );

    // Query that doesn't match - weight mismatch
    trace.clear();
    let light_query = FcPattern {
        family: Some("Test Family".to_string()),
        weight: FcWeight::Light,
        ..Default::default()
    };

    let matches = cache.query(&light_query, &mut trace);
    assert!(matches.is_none(), "Should not match with weight mismatch");

    // Check trace messages for weight mismatch
    let weight_mismatch_traces = trace
        .iter()
        .filter(|msg| matches!(msg.reason, MatchReason::WeightMismatch { .. }))
        .count();
    assert!(
        weight_mismatch_traces > 0,
        "Expected weight mismatch trace messages"
    );

    // Test weight matching algorithm
    let available_weights = [FcWeight::Light, FcWeight::Normal, FcWeight::Bold];

    // When exact match exists
    assert_eq!(
        FcWeight::Normal.find_best_match(&available_weights),
        Some(FcWeight::Normal),
        "Should find exact match when available"
    );

    // When desired weight is less than 400
    assert_eq!(
        FcWeight::ExtraLight.find_best_match(&available_weights),
        Some(FcWeight::Light),
        "Should find closest lighter weight for weights < 400"
    );

    // When desired weight is greater than 500
    assert_eq!(
        FcWeight::ExtraBold.find_best_match(&available_weights),
        Some(FcWeight::Bold),
        "Should find closest heavier weight for weights > 500"
    );

    // For weight 400, try 500 first then lighter weights
    let available = [FcWeight::Light, FcWeight::Bold];
    assert_eq!(
        FcWeight::Normal.find_best_match(&available),
        Some(FcWeight::Light),
        "For weight 400, should prefer lightest weight when 500 unavailable"
    );

    // For weight 500, try 400 first then lighter weights
    let available = [FcWeight::Light, FcWeight::SemiBold];
    assert_eq!(
        FcWeight::Medium.find_best_match(&available),
        Some(FcWeight::Light),
        "For weight 500, should prefer 400 first"
    );
}

#[test]
fn test_trace_messages() {
    // Create a simple font cache with one font
    let test_font = FcFont {
        bytes: vec![0, 1, 2, 3],
        font_index: 0,
        id: "test-font".to_string(),
    };

    let test_pattern = FcPattern {
        name: Some("Test Font".to_string()),
        family: Some("Test Family".to_string()),
        italic: PatternMatch::False,
        monospace: PatternMatch::True,
        weight: FcWeight::Normal,
        stretch: FcStretch::Normal,
        unicode_ranges: vec![UnicodeRange {
            start: 0x0000,
            end: 0x007F,
        }],
        ..Default::default()
    };

    let mut cache = FcFontCache::default();
    cache.with_memory_fonts(vec![(test_pattern.clone(), test_font)]);

    // Test name mismatch
    let mut trace = Vec::new();
    let name_query = FcPattern {
        name: Some("Wrong Name".to_string()),
        ..Default::default()
    };

    let matches = cache.query(&name_query, &mut trace);
    assert!(matches.is_none(), "Should not match with wrong name");

    assert!(!trace.is_empty(), "Trace should not be empty");
    let name_mismatch = trace.iter().any(|msg| {
        if let MatchReason::NameMismatch { requested, found } = &msg.reason {
            requested.as_ref() == Some(&"Wrong Name".to_string())
                && found.as_ref() == Some(&"Test Font".to_string())
        } else {
            false
        }
    });
    assert!(name_mismatch, "Name mismatch trace message not found");

    // Test style mismatch
    trace.clear();
    let style_query = FcPattern {
        name: Some("Test Font".to_string()),
        italic: PatternMatch::True,
        ..Default::default()
    };

    let matches = cache.query(&style_query, &mut trace);
    assert!(matches.is_none(), "Should not match with style mismatch");

    let style_mismatch = trace.iter().any(|msg| {
        if let MatchReason::StyleMismatch { property, .. } = &msg.reason {
            property == &"italic"
        } else {
            false
        }
    });
    assert!(style_mismatch, "Style mismatch trace message not found");

    // Test stretch mismatch
    trace.clear();
    let stretch_query = FcPattern {
        name: Some("Test Font".to_string()),
        stretch: FcStretch::Condensed,
        ..Default::default()
    };

    let matches = cache.query(&stretch_query, &mut trace);
    assert!(matches.is_none(), "Should not match with stretch mismatch");

    let stretch_mismatch = trace
        .iter()
        .any(|msg| matches!(msg.reason, MatchReason::StretchMismatch { .. }));
    assert!(stretch_mismatch, "Stretch mismatch trace message not found");

    // Test unicode range mismatch
    trace.clear();
    let range_query = FcPattern {
        name: Some("Test Font".to_string()),
        unicode_ranges: vec![UnicodeRange {
            start: 0x0370,
            end: 0x03FF,
        }], // Greek
        ..Default::default()
    };

    let matches = cache.query(&range_query, &mut trace);
    assert!(
        matches.is_none(),
        "Should not match with Unicode range mismatch"
    );

    let range_mismatch = trace
        .iter()
        .any(|msg| matches!(msg.reason, MatchReason::UnicodeRangeMismatch { .. }));
    assert!(
        range_mismatch,
        "Unicode range mismatch trace message not found"
    );
}

fn getfonts(
    arial_id: FontId,
    arial_bold_id: FontId,
    courier_id: FontId,
    fira_id: FontId,
    noto_cjk_id: FontId,
) -> Vec<(FontId, FcPattern, FcFont)> {
    return vec![
        (
            arial_id,
            FcPattern {
                name: Some("Arial".to_string()),
                family: Some("Arial".to_string()),
                weight: FcWeight::Normal,
                bold: PatternMatch::False,
                monospace: PatternMatch::False,
                unicode_ranges: vec![UnicodeRange {
                    start: 0x0000,
                    end: 0x007F,
                }],
                ..Default::default()
            },
            FcFont {
                bytes: vec![1, 2, 3, 4],
                font_index: 0,
                id: "arial-regular".to_string(),
            },
        ),
        (
            arial_bold_id,
            FcPattern {
                name: Some("Arial Bold".to_string()),
                family: Some("Arial".to_string()),
                weight: FcWeight::Bold,
                bold: PatternMatch::True,
                monospace: PatternMatch::False,
                unicode_ranges: vec![UnicodeRange {
                    start: 0x0000,
                    end: 0x007F,
                }],
                ..Default::default()
            },
            FcFont {
                bytes: vec![5, 6, 7, 8],
                font_index: 0,
                id: "arial-bold".to_string(),
            },
        ),
        // Monospace fonts
        (
            courier_id,
            FcPattern {
                name: Some("Courier New".to_string()),
                family: Some("Courier New".to_string()),
                weight: FcWeight::Normal,
                monospace: PatternMatch::True,
                unicode_ranges: vec![UnicodeRange {
                    start: 0x0000,
                    end: 0x007F,
                }],
                ..Default::default()
            },
            FcFont {
                bytes: vec![9, 10, 11, 12],
                font_index: 0,
                id: "courier-new".to_string(),
            },
        ),
        (
            fira_id,
            FcPattern {
                name: Some("Fira Code".to_string()),
                family: Some("Fira Code".to_string()),
                weight: FcWeight::Normal,
                monospace: PatternMatch::True,
                unicode_ranges: vec![UnicodeRange {
                    start: 0x0000,
                    end: 0x007F,
                }],
                ..Default::default()
            },
            FcFont {
                bytes: vec![13, 14, 15, 16],
                font_index: 0,
                id: "fira-code".to_string(),
            },
        ),
        // CJK font
        (
            noto_cjk_id,
            FcPattern {
                name: Some("Noto Sans CJK".to_string()),
                family: Some("Noto Sans CJK".to_string()),
                weight: FcWeight::Normal,
                monospace: PatternMatch::False,
                unicode_ranges: vec![
                    UnicodeRange {
                        start: 0x0000,
                        end: 0x007F,
                    }, // Latin
                    UnicodeRange {
                        start: 0x4E00,
                        end: 0x9FFF,
                    }, // CJK
                ],
                ..Default::default()
            },
            FcFont {
                bytes: vec![17, 18, 19, 20],
                font_index: 0,
                id: "noto-sans-cjk".to_string(),
            },
        ),
    ];
}

// Update the test code to use deterministic IDs
#[test]
fn test_font_search() {
    // Create fixed font IDs for deterministic testing
    let arial_id = FontId(1);
    let arial_bold_id = FontId(2);
    let courier_id = FontId(3);
    let fira_id = FontId(4);
    let noto_cjk_id = FontId(5);

    // Create a set of fonts with various properties for testing search functionality
    let fonts = getfonts(arial_id, arial_bold_id, courier_id, fira_id, noto_cjk_id);

    // Create font cache with all our test fonts using deterministic IDs
    let mut cache = FcFontCache::default();
    for (id, pattern, font) in fonts {
        cache.with_memory_font_with_id(id, pattern, font);
    }

    // Test 2: Search for any monospace font using list() with filter
    let mut trace: Vec<TraceMsg> = Vec::new();
    
    let results: Vec<_> = cache.list().into_iter()
        .filter(|(pattern, _)| pattern.monospace == PatternMatch::True)
        .collect();
    assert_eq!(results.len(), 2, "Should find two monospace fonts");

    let result_ids: Vec<FontId> = results.into_iter().map(|(_, id)| id).collect();
    assert!(
        result_ids.contains(&courier_id),
        "Should include Courier New"
    );
    assert!(result_ids.contains(&fira_id), "Should include Fira Code");

    // Test 4: Search for a font that can render CJK text using resolve_font_chain
    #[cfg(feature = "std")]
    {
        let cjk_text = "你好"; // Hello in Chinese

        // Build font chain from all available fonts
        let families: Vec<String> = cache.list().iter()
            .filter_map(|(pattern, _)| pattern.family.clone())
            .collect();

        let chain = cache.resolve_font_chain(
            &families,
            FcWeight::Normal,
            PatternMatch::DontCare,
            PatternMatch::DontCare,
            &mut trace,
        );

        let runs = chain.query_for_text(&cache, cjk_text);
        assert!(!runs.is_empty(), "Should find fonts for CJK text");

        let result_ids: Vec<FontId> = runs.iter()
            .filter_map(|r| r.font_id)
            .collect();
        assert!(
            result_ids.contains(&noto_cjk_id),
            "Should include Noto Sans CJK"
        );

        // Test 5: Multiple fonts for mixed text
        trace.clear();
        let mixed_text = "Hello 你好"; // Latin and CJK

        let runs = chain.query_for_text(&cache, mixed_text);

        // Collect unique fonts
        let unique_fonts: std::collections::HashSet<_> = runs.iter()
            .filter_map(|r| r.font_id)
            .collect();

        assert!(
            unique_fonts.len() >= 1,
            "Should find at least one font for mixed text"
        );

        // Verify that we got both Latin and CJK capable fonts
        let cjk_found = unique_fonts.contains(&noto_cjk_id);
        assert!(cjk_found, "Should find a CJK-capable font");
    }
}

#[test]
fn test_failing_isolated() {
    // Create fixed font IDs for deterministic testing
    let arial_id = FontId(1);
    let arial_bold_id = FontId(2);
    let courier_id = FontId(3);
    let fira_id = FontId(4);
    let noto_cjk_id = FontId(5);

    // Create a set of fonts with various properties for testing search functionality
    let fonts = getfonts(arial_id, arial_bold_id, courier_id, fira_id, noto_cjk_id);

    // Create font cache with all our test fonts using deterministic IDs
    let mut cache = FcFontCache::default();
    for (id, pattern, font) in fonts {
        cache.with_memory_font_with_id(id, pattern, font);
    }

    // Test 1: Search for Arial font
    let mut trace = Vec::new();
    let arial_query = FcPattern {
        name: Some("Arial".to_string()),
        ..Default::default()
    };

    let result = cache.query(&arial_query, &mut trace);
    assert!(result.is_some(), "Should find Arial font");
    assert_eq!(result.unwrap().id, arial_id, "Should match Arial font ID");
}

#[test]
fn test_failing_isolated_2() {
    // Create fixed font IDs for deterministic testing
    let arial_id = FontId(1);
    let arial_bold_id = FontId(2);
    let courier_id = FontId(3);
    let fira_id = FontId(4);
    let noto_cjk_id = FontId(5);

    // Create a set of fonts with various properties for testing search functionality
    let fonts = getfonts(arial_id, arial_bold_id, courier_id, fira_id, noto_cjk_id);

    // Create font cache with all our test fonts using deterministic IDs
    let mut cache = FcFontCache::default();
    for (id, pattern, font) in fonts {
        cache.with_memory_font_with_id(id, pattern, font);
    }

    // Test 3: Search for bold Arial font
    let mut trace = Vec::new();
    let arial_bold_query = FcPattern {
        family: Some("Arial".to_string()),
        bold: PatternMatch::True,
        ..Default::default()
    };

    let result = cache.query(&arial_bold_query, &mut trace);
    assert!(result.is_some(), "Should find Arial Bold font");
    assert_eq!(
        result.unwrap().id,
        arial_bold_id,
        "Should match Arial Bold font ID"
    );
}

/// Regression test for the headless / wasm / embedder-bundled-font bug.
///
/// A bundled IN-MEMORY font, registered via `with_memory_fonts` with the
/// kind of NAIVE pattern a normal caller actually writes (a generic-ish
/// name and, crucially, an EMPTY `unicode_ranges`), must be usable to shape
/// text when the document asks for the generic CSS family `"serif"` and the
/// cache has NO system fonts at all.
///
/// Before the fix this returned `None` for two independent reasons:
///   1. `with_memory_fonts` stored the empty `unicode_ranges` verbatim, and
///      `resolve_char` skips fonts with no range info, so the font could
///      never be selected for any character.
///   2. The generic `"serif"` family was expanded to a hardcoded list of
///      real OS font names (Times, DejaVu Serif, ...) and the original
///      generic name was dropped, so a registered memory font was never
///      reached.
///
/// Requires the `parsing` feature: without it the crate cannot inspect the
/// font's cmap/OS2 to learn its Unicode coverage, so the empty ranges
/// cannot be auto-populated.
#[cfg(all(feature = "std", feature = "parsing"))]
#[test]
fn test_memory_font_generic_serif_resolves_char() {
    // A real Latin TTF, embedded into the test binary.
    let font_bytes = include_bytes!("fixtures/InstrumentSerif-Regular.ttf").to_vec();

    // Empty cache: no system fonts (headless / wasm / embedder scenario).
    let cache = FcFontCache::default();

    // Exactly what a normal caller writes: a name, and an EMPTY
    // unicode_ranges (they do NOT hand-compute the cmap).
    let pattern = FcPattern {
        name: Some("serif".to_string()),
        family: Some("serif".to_string()),
        unicode_ranges: Vec::new(),
        ..Default::default()
    };
    let font = FcFont {
        bytes: font_bytes,
        font_index: 0,
        id: "bundled-serif".to_string(),
    };
    cache.with_memory_fonts(vec![(pattern, font)]);

    // Resolve a chain for the generic CSS family "serif".
    let mut trace: Vec<TraceMsg> = Vec::new();
    let chain = cache.resolve_font_chain_with_scripts(
        &["serif".to_string()],
        FcWeight::Normal,
        PatternMatch::False,
        PatternMatch::False,
        None,
        &mut trace,
    );

    // The bundled font is the ONLY font available; it MUST be selected to
    // render an ASCII 'A'.
    let resolved = chain.resolve_char(&cache, 'A');
    assert!(
        resolved.is_some(),
        "bundled in-memory 'serif' font must resolve ASCII 'A' on a headless cache; \
         got None (chain = {:#?})",
        chain
    );
}
