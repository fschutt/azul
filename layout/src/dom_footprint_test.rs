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

    /// THE CACHE IS NOW SMALLER THAN THE TREE — the inversion, pinned.
    ///
    /// It used to be 7x the tree, and `computed_values` alone was four times
    /// the whole DOM. After dropping the unreachable entries, transposing the
    /// inherited store and sharing identical runs in `css_props` /
    /// `cascaded_props`, what is left is dominated by the COMPACT CACHE — the
    /// irreducible bitfield form the hot layout loop reads, which is the one
    /// part that should be big.
    ///
    /// If this ever fails the other way, a per-node duplicate has crept back in.
    #[test]
    fn the_style_cache_no_longer_dominates_the_tree_it_is_derived_from() {
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
            cache < dom_only,
            "the style cache ({cache} B) has grown past the tree it derives from ({dom_only} B) \
             — a per-node duplicate has come back; see `FlatVecVec::dedup_runs` and \
             `InheritedValues`",
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

    /// The TRANSPOSED store, measured: how many (node, property) pairs it
    /// represents, how few distinct values that actually is, and what it costs.
    ///
    /// Inherited values are shared down a subtree by definition, so the
    /// per-node shape paid for a 136-byte `CssProperty` again in every
    /// descendant. `cursor` was the extreme case: 29 391 entries carrying ONE
    /// value. This pins the compression ratio so a regression that reintroduces
    /// per-node copies is visible as a number rather than as a memory graph.
    #[test]
    fn the_inherited_store_holds_a_handful_of_values_for_tens_of_thousands_of_nodes() {
        let Some(xml) = bench_document() else {
            eprintln!("[skip] doc/xhtml1/chapter-8.xht not present");
            return;
        };
        let styled = crate::xml::parse_xml_to_styled_dom(&xml)
            .expect("the bench document must parse");
        let cache = styled.get_css_property_cache();
        let store = &cache.computed_values;

        let entries = store.entry_count();
        let buckets = store.bucket_count();
        let bytes = store.heap_bytes();
        let by_value_bytes = entries
            * core::mem::size_of::<(
                azul_css::props::property::CssPropertyType,
                azul_core::prop_cache::CssPropertyWithOrigin,
            )>();
        println!(
            "inherited store: {entries} (node, property) pairs over {buckets} distinct values\n               transposed {bytes} B vs per-node {by_value_bytes} B ({:.0}x smaller)",
            by_value_bytes as f64 / bytes.max(1) as f64,
        );

        assert!(entries > 10_000, "expected a large document, got {entries} pairs");
        // The whole point: values are SHARED, so the bucket count stays tiny
        // even as the node count grows. 100 is an order check.
        assert!(
            buckets < 100,
            "{buckets} distinct inherited values — the store is no longer sharing them",
        );
        assert!(
            bytes < by_value_bytes / 4,
            "transposed {bytes} B vs per-node {by_value_bytes} B — the compression is gone",
        );
    }
}
