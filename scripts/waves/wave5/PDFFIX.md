<!-- Brief of the PDFFIX agent (2026-10-02, base dfa3e14b8, branch wt/pdffix, merged d2694da61; report
     scripts/PDFFIX_2026_10_02.md). Reconstructed after the /tmp wipe. -->
Task PDFFIX (azul, PR #476). House rules: scripts/waves/wave5/house_rules.md (never compile). Branch `wt/pdffix` from `dfa3e14b8`.
Another agent doing PDF work (printpdf's HTML-to-PDF via azul-layout) reported two azul layout bugs. Root-cause and fix
both, RED first (`layout/tests/<sentence>.rs`, appended to `layout/tests/all.rs`), small commits. NEVER touch
`layout/src/solver3/page_breaks.rs` or `layout/tests/a_padded_table_cell_stays_in_its_row.rs` (another session's,
uncommitted); if a root cause is in page_breaks.rs, write it down precisely in the report instead of editing it.

1. column-count (issue fschutt/azul#481): `<div style="column-count: 2">` with four `<p>` children rendered four stacked
   paragraphs, EACH split into two columns. Implement multi-column layout for a block container (CSS Multi-column 1):
   column width from count/width/gap, children laid out at column width and distributed over N balanced columns, an
   inline formatting context splits between lines (reuse text3's line split), other boxes move whole; RTL; the
   container is as tall as its tallest column. A single IFC root with column-count keeps text3's split.
2. Paged layout: content that overflows a clipped (overflow: hidden), fixed-height box still created extra pages.
   Find where the paged extent is decided; content clipped by an overflow ancestor must not extend it. RED: a
   fixed-height page box with overflow hidden and an abspos block of long text -> one page.
