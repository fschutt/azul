    use azul_css::props::basic::FontRef;

    use super::*;

    /// The invariant `memory_families` exists to uphold: it is an INDEX into
    /// `fc_cache`, so every `FontId` it hands to chain resolution must still be
    /// loadable from the cache that is live *right now*. A dangling id does not
    /// fail loudly — it resolves, then fails to load, and the text silently
    /// re-measures with the fallback font's metrics.
    fn assert_index_is_live(m: &FontManager<FontRef>) {
        for (family, faces) in &m.memory_families {
            for f in faces {
                assert!(
                    m.fc_cache.is_memory_font(&f.font_match.id),
                    "`{family}` is indexed with {:?}, which the live fc_cache cannot load",
                    f.font_match.id
                );
            }
        }
    }

    /// A `FontRef` that [`crate::font_ref_to_parsed_font`] is actually allowed
    /// to reborrow.
    ///
    /// This test used to fabricate its probe faces as
    /// `FontRef::new(core::ptr::addr_of!(A).cast(), noop)` over a one-byte
    /// `static A: u8`. Every call it makes on such a handle —
    /// `register_embedded_font`, `resolve_font_by_hash` and the bare
    /// `get_hash()` in the assertions — routes through
    /// `<FontRef as ParsedFontTrait>::get_hash` (`text3/default.rs`), which is
    /// `crate::font_ref_to_parsed_font(self).hash`, i.e.
    /// `unsafe { &*ptr.cast::<ParsedFont>() }` on a 1-byte-aligned, 1-byte-long
    /// address. That is undefined behaviour, and `debug_assertions` builds turn
    /// it into a `misaligned pointer dereference` *non-unwinding* panic that
    /// aborts the whole test binary — the dev-profile CI job died here, and
    /// only there, because release builds elide the alignment check.
    ///
    /// `parsed_font_to_font_ref` is the constructor whose contract
    /// `font_ref_to_parsed_font` names ("must have been created by
    /// `parsed_font_to_font_ref`"), so a handle minted here is genuinely
    /// backed by a heap `ParsedFont` and the reborrow is sound.
    fn probe_font_ref(bytes: &[u8]) -> FontRef {
        let parsed = crate::font::parsed::ParsedFont::from_bytes(bytes, 0, &mut Vec::new())
            .expect("the built-in mock fonts must parse");
        crate::parsed_font_to_font_ref(parsed)
    }

    /// REGRESSION: `clone_shared` used to FORK `embedded_fonts` rather than
    /// share it, so a face registered in one manager was invisible to every
    /// other one cloned from it.
    ///
    /// That is a silent-tofu generator, because the manager that REGISTERS an
    /// embedded face and the one that later SHAPES with it are routinely
    /// different objects - a child window, a tray icon, an off-screen render.
    /// When they disagree the shaper falls back to a system face with no glyph
    /// at the icon's private-use codepoint and draws `.notdef`, while every
    /// step in between reports success.
    ///
    /// BOTH directions are asserted on purpose: `Arc::clone` is the only
    /// implementation that gives you parent->child AND child->parent, so a
    /// one-directional test would still pass against a copy-on-clone.
    #[test]
    fn embedded_fonts_are_shared_between_cloned_managers_in_both_directions() {
        let parent: FontManager<FontRef> =
            FontManager::new(FcFontCache::default()).expect("FontManager::new must not fail");
        let child = parent.clone_shared();

        // TWO DIFFERENT faces on purpose. `ParsedFont::hash` hashes the font
        // bytes, so two parses of the same file collide - and with one shared
        // hash the assertions below would also hold against the copy-on-clone
        // fork this test exists to catch (each manager would hold the very hash
        // it is asked for). The `assert_ne!` pins that non-vacuity.
        let from_parent = probe_font_ref(crate::text3::mock_fonts::MOCK_MONO_TTF);
        let from_child = probe_font_ref(crate::text3::mock_fonts::MOCK_WIDE_TTF);
        let parent_hash = from_parent.get_hash();
        let child_hash = from_child.get_hash();
        assert_ne!(
            parent_hash, child_hash,
            "the two probe faces must be distinguishable, or a forked `embedded_fonts` would \
             satisfy both directions below"
        );

        parent.register_embedded_font(&from_parent);
        child.register_embedded_font(&from_child);

        assert_eq!(
            child
                .resolve_font_by_hash(parent_hash)
                .map(|f| f.get_hash()),
            Some(parent_hash),
            "a face registered on the PARENT must be visible to a clone"
        );
        assert_eq!(
            parent
                .resolve_font_by_hash(child_hash)
                .map(|f| f.get_hash()),
            Some(child_hash),
            "a face registered on a CLONE must be visible to the parent"
        );
    }

    #[test]
    fn swapping_the_fc_cache_does_not_strand_a_dead_memory_font_id() {
        let mut m: FontManager<FontRef> =
            FontManager::new(FcFontCache::default()).expect("FontManager::new must not fail");
        let norm = rust_fontconfig::utils::normalize_family_name("Azul Mock Mono");

        let before = m
            .memory_families
            .get(&norm)
            .cloned()
            .expect("the built-in mock fonts are registered by every constructor");
        assert_eq!(before.len(), 1);
        assert_index_is_live(&m);

        // Exactly what the DLL does at the top of EVERY `regenerate_layout`.
        for swap in 1..=3 {
            m.replace_fc_cache(FcFontCache::default());
            assert_index_is_live(&m);
            let faces = m
                .memory_families
                .get(&norm)
                .expect("the mock fonts are re-registered into the new cache");
            assert_eq!(
                faces.len(),
                1,
                "swap {swap} appended a face instead of replacing it: the index grows by one dead \
                 face per cache swap and `pick_memory_face` keeps returning the first (dead) one"
            );
        }

        // Non-vacuity: the fresh caches really were empty, so the face WAS
        // re-minted under a new id — the old id is exactly the one that used to
        // be stranded at the head of the list.
        let after = &m.memory_families[&norm];
        assert_ne!(
            after[0].font_match.id, before[0].font_match.id,
            "a fresh FcFontCache cannot already contain the mock font"
        );
        assert!(
            !m.fc_cache.is_memory_font(&before[0].font_match.id),
            "the pre-swap id must be dead — otherwise this test proves nothing"
        );
    }
