//! What a whole UI actually COSTS in memory, as a checked number.
//!
//! `doc/guide/en/architecture.md` rests an architectural argument on a
//! measurement: re-deriving the UI from application state on every interaction
//! is affordable because "the entire DOM with styling in even a large
//! application is only ~500KB - 1MB". That claim is the answer to the standard
//! objection to `UI = f(data)` — that rebuilding is too expensive — so it needs
//! to be a fact the tree checks, not a number someone once saw.
//!
//! Measured on a document far past anything a GUI reaches — the 1 MB, ~50k-node
//! XHTML spec chapter the render bench uses — the answer splits in two, and the
//! split is the interesting part:
//!
//!   * the DOM proper is ~257 bytes/node, so a 5000-node UI (already a dense
//!     desktop window) is ~1.25 MB. The guide's number is right.
//!   * the CSS PROPERTY CACHE is ~1774 bytes/node — SEVEN TIMES the DOM, and
//!     87% of the total, most of it `computed_values`.
//!
//! That matters for how the claim is stated. "The DOM is a megabyte" is true
//! and is the affordability argument; "a StyledDom is a megabyte" is false by
//! an order of magnitude, and the difference is a derived cache, not the tree.
//! A rebuild pays for the cache too, so the honest framing is that re-derivation
//! is cheap in the DOM and bounded by the cascade.
//!
//! The bounds below catch a regression of the ORDER (a `NodeData` that doubles,
//! a cache that starts retaining per-node allocations), not an exact byte count
//! ordinary work would churn.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// The XHTML spec chapter the render benchmark uses: ~1 MB of markup,
    /// tens of thousands of nodes. `None` when run from a checkout without it,
    /// so the test degrades to a skip rather than a failure.
    fn bench_document() -> Option<String> {
        for rel in ["../doc/xhtml1/chapter-8.xht", "doc/xhtml1/chapter-8.xht"] {
            if let Ok(s) = std::fs::read_to_string(PathBuf::from(rel)) {
                return Some(s);
            }
        }
        None
    }

    /// THE TREE ITSELF is a few hundred bytes per node — the measurement
    /// `architecture.md` cites when it answers "isn't rebuilding expensive?".
    ///
    /// If this ever fails, the guide's affordability argument has to be
    /// restated, not the bound relaxed.
    #[test]
    fn the_dom_itself_costs_a_few_hundred_bytes_per_node() {
        let Some(xml) = bench_document() else {
            eprintln!("[skip] doc/xhtml1/chapter-8.xht not present");
            return;
        };
        let styled = crate::xml::parse_xml_to_styled_dom(&xml)
            .expect("the bench document must parse");
        let report = styled.memory_report();
        let nodes = report.node_count;
        let total = report.total_bytes();
        assert!(nodes > 10_000, "expected a large document, got {nodes} nodes");

        let per_node = total / nodes.max(1);
        println!(
            "StyledDom: {nodes} nodes, {total} bytes total, {per_node} bytes/node\n  \
             hierarchy {} | node_data {} | styled_nodes {} | cascade {} | tags {} | \
             non_leaf {} | callbacks {} | css_cache {}",
            report.node_hierarchy_bytes,
            report.node_data_bytes,
            report.styled_nodes_bytes,
            report.cascade_info_bytes,
            report.tag_ids_bytes,
            report.non_leaf_nodes_bytes,
            report.callback_vecs_bytes,
            report.css_property_cache.total_bytes(),
        );
        let c = &report.css_property_cache;
        println!(
            "  css_cache: cascaded {} | css_props {} | computed {} | overridden {} | \
             global {} | compact {} | font_sizes {}",
            c.cascaded_props_bytes,
            c.css_props_bytes,
            c.computed_values_bytes,
            c.user_overridden_bytes,
            c.global_css_props_bytes,
            c.compact_cache_bytes,
            c.resolved_font_sizes_bytes,
        );
        let dom_only = total - c.total_bytes();
        println!(
            "  DOM WITHOUT the style cache: {dom_only} bytes = {} bytes/node",
            dom_only / nodes.max(1),
        );

        // THE TREE, without the derived style cache: this is what the guide's
        // "~500KB - 1MB for a large application" is about, and what a rebuild
        // has to reconstruct structurally. Measured at 257 bytes/node; 512 is
        // an order check, not a target.
        let dom_per_node = dom_only / nodes.max(1);
        assert!(
            dom_per_node < 512,
            "the DOM is {dom_per_node} bytes/node — the re-derivation argument rests on this \
             staying in the hundreds",
        );
    }

    /// THE STYLE CACHE, NOT THE TREE, IS THE EXPENSIVE HALF — recorded so the
    /// ratio is a fact rather than a surprise.
    ///
    /// `computed_values` alone is roughly four times the whole DOM. Anyone
    /// reading "a DOM is about a megabyte" and sizing a budget from it will be
    /// out by an order of magnitude unless they know this, and any future work
    /// on rebuild cost should start here rather than on the tree.
    #[test]
    fn the_style_cache_dominates_the_tree_it_is_derived_from() {
        let Some(xml) = bench_document() else {
            eprintln!("[skip] doc/xhtml1/chapter-8.xht not present");
            return;
        };
        let styled = crate::xml::parse_xml_to_styled_dom(&xml)
            .expect("the bench document must parse");
        let report = styled.memory_report();
        let cache = report.css_property_cache.total_bytes();
        let dom_only = report.total_bytes() - cache;
        println!(
            "cache {cache} vs dom {dom_only} — cache is {:.1}x the tree",
            cache as f64 / dom_only.max(1) as f64,
        );
        assert!(
            cache > dom_only,
            "the style cache used to dominate the DOM ({cache} vs {dom_only}); if that has \
             changed, the module docs above and the guide's memory figures need updating",
        );

        // And the number the guide's claim projects to for a dense real UI.
        let dom_per_node = dom_only / report.node_count.max(1);
        let projected = dom_per_node * 5_000;
        println!(
            "projected 5000-node UI (tree only): {projected} bytes ({:.2} MB)",
            projected as f64 / (1024.0 * 1024.0),
        );
        assert!(
            projected < 4 * 1024 * 1024,
            "a 5000-node UI's TREE projects to {projected} bytes; the guide claims ~1 MB",
        );
    }

    /// HOW MUCH of `computed_values` is answerable from the compact cache?
    ///
    /// `computed_values` is a per-node sorted vec of resolved INHERITABLE
    /// properties, and it is 58% of the whole style cache. 17 of the 37
    /// inheritable property types also have a compact encoding, so for those
    /// the compact cache already holds the answer (it does its own inheritance
    /// — `compact.rs`, "Step 1: Inherit from parent's COMPACT values"). Only
    /// the remaining 20 — the rare ones (`hanging-punctuation`,
    /// `text-combine-upright`, `list-style-position`, `widows`, `orphans` …) —
    /// genuinely need a tall form.
    ///
    /// This counts the split on a real stylesheet, so the decision to write the
    /// `CssPropertyType` -> compact dispatch is made against a number rather
    /// than against the shape of the type list.
    #[test]
    fn how_much_of_computed_values_the_compact_cache_could_answer() {
        let Some(xml) = bench_document() else {
            eprintln!("[skip] doc/xhtml1/chapter-8.xht not present");
            return;
        };
        let styled = crate::xml::parse_xml_to_styled_dom(&xml)
            .expect("the bench document must parse");
        let cache = styled.get_css_property_cache();

        let mut compact_covered = 0usize;
        let mut needs_tall = 0usize;
        let mut by_type: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for per_node in &cache.computed_values {
            for (ty, _) in per_node {
                if ty.has_compact_encoding() {
                    compact_covered += 1;
                } else {
                    needs_tall += 1;
                    *by_type.entry(format!("{ty:?}")).or_default() += 1;
                }
            }
        }
        let total = compact_covered + needs_tall;
        println!(
            "computed_values entries: {total} total | {compact_covered} answerable from the \
             compact cache ({:.1}%) | {needs_tall} genuinely need the tall form",
            100.0 * compact_covered as f64 / total.max(1) as f64,
        );
        println!("  non-compact inherited types actually used: {by_type:?}");

        // Of the compact-covered entries, only some are LOSSLESSLY recoverable:
        // the compact cache stores font-family as a u64 HASH, and pixel-valued
        // properties as a px-or-sentinel encoding that discards em/%/vh metrics.
        // Count the entries a `CssPropertyType -> CssProperty` bridge could
        // actually answer.
        let mut recoverable: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut lossy: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for per_node in &cache.computed_values {
            for (ty, _) in per_node {
                if !ty.has_compact_encoding() {
                    continue;
                }
                let name = format!("{ty:?}");
                // Enum-valued tier-1 properties and the packed colour decode
                // back exactly; everything else loses its metric or its identity.
                let lossless = matches!(
                    name.as_str(),
                    "font-weight" | "font-style" | "text-align" | "visibility" | "white-space"
                        | "direction" | "writing-mode" | "border-collapse" | "color"
                        | "tab-size" | "border-spacing"
                );
                if lossless {
                    *recoverable.entry(name).or_default() += 1;
                } else {
                    *lossy.entry(name).or_default() += 1;
                }
            }
        }
        let rec: usize = recoverable.values().sum();
        let los: usize = lossy.values().sum();
        println!(
            "  of the compact-covered {compact_covered}: {rec} losslessly recoverable ({:.1}% of \
             ALL entries), {los} lossy",
            100.0 * rec as f64 / total.max(1) as f64,
        );
        println!("    recoverable: {recoverable:?}");
        println!("    lossy:       {lossy:?}");

        // WHY the remaining entries are still expensive, and what a transposed
        // store would cost instead. `CssProperty` is a wide enum held BY VALUE
        // in every entry, so an inherited `cursor: pointer` on a container is
        // paid for again in every descendant. Counting DISTINCT values per type
        // sizes the alternative: one copy of each value plus a node-id list.
        use azul_css::props::property::CssProperty;
        let entry_sz = core::mem::size_of::<(
            azul_css::props::property::CssPropertyType,
            azul_core::prop_cache::CssPropertyWithOrigin,
        )>();
        println!(
            "  size_of CssProperty = {} B, per-entry = {entry_sz} B",
            core::mem::size_of::<CssProperty>(),
        );
        let mut distinct: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        let mut entries = 0usize;
        for per_node in &cache.computed_values {
            for (ty, p) in per_node {
                entries += 1;
                distinct
                    .entry(format!("{ty:?}"))
                    .or_default()
                    .insert(format!("{:?}", p.property));
            }
        }
        let distinct_total: usize = distinct.values().map(std::collections::BTreeSet::len).sum();
        let transposed = distinct_total * entry_sz + entries * core::mem::size_of::<u32>();
        let current = entries * entry_sz;
        println!(
            "  {entries} entries over {distinct_total} DISTINCT values: by-value {current} B vs \
             transposed {transposed} B ({:.0}x smaller)",
            current as f64 / transposed.max(1) as f64,
        );
        for (ty, vals) in &distinct {
            println!("    {ty}: {} distinct value(s)", vals.len());
        }
    }
}
