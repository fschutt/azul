# Website round 2 — review feedback (2026-05-30) — FULL requirement list

Source: user review of the local build (3 screenshots). "Make a list and don't drop
any requirements." Status of each: [ ] todo / [x] done / [~] partial.

## FONTS (4 families now — mixed, not a single swap)
[ ] F1. Playfair Display is NOT actually loading (headings render as serif fallback,
        not Playfair). Diagnose + fix @font-face so Playfair renders.
[ ] F2. RESTORE "Instrument Serif" (I deleted it last commit — recover from git).
        It stays the font for: SUBTITLES / section headings (e.g. guide "Introduction"),
        AND the example-showcase title "Hello World, Goodbye JavaScript".
[ ] F3. Playfair Display is for the BIG headings: hero h1 "Built for beauty and speed",
        and the guide page big h1 (e.g. "Hello World [Rust]") — BUT reduce that big
        guide h1's size (currently very large).
[ ] F4. Body text = Rubik (already done, keep).
[ ] F5. Monospace/code: replace `monospace` with a real font — "whatever RedHat has"
        => Red Hat Mono (self-host, OFL). Apply to all `font-family: monospace` spots
        (main.css has ~8; also code blocks / inline code / install boxes).
   NET font map:
     - Playfair Display : big h1 (hero, guide page title)
     - Instrument Serif : subtitles / section headings (h2/h3) + showcase title
     - Rubik            : body / UI
     - Red Hat Mono     : all monospace / code

## DARK MODE + SEARCH (Image 2 = a guide page in dark mode)
[ ] D1. The floating "Search guide" box (top-right overlay) is out of place. It must be
        IN the page, as a COLUMN next to the guide content (not a floating overlay).
[ ] D2. Guide search scoping:
        - The guide SEARCH (full-text guide index) belongs ONLY on the guide OVERVIEW
          page (guide.html).
        - On INDIVIDUAL guide pages: use the API search instead, AND have it
          EXPANDED ON LOAD showing the items from that page's frontmatter
          (default-search-keys) as "direct links to the API docs for items mentioned
          on this page".
[ ] D3. General dark-mode review of the search component (must look right in dark mode).

## INDEX PAGE (Image 3)
[ ] I1. Hero h1 "Built for beauty and speed." -> Playfair (see F3).
[ ] I2. Showcase title "Hello, World. Goodbye, JavaScript." -> Instrument Serif (see F2).
[ ] I3. The version block "v0.2.0 / 2026-05-29" -> RIGHT-ALIGNED, in a SEPARATE COLUMN
        (currently left-aligned in the latestrelease box). (#latestrelease grid.)
[ ] I4. "Search API" bar redesign:
        a. works in DARK MODE
        b. REMOVE the search (magnifier) icon
        c. REMOVE any box-shadow
        d. move the "/" hotkey hint to the LEFT of the bar (currently right)
[ ] I5. Remove the "more languages…" expander. List ALL 11 shipped languages as tabs
        (python, c, cpp, rust, csharp, java, kotlin, lua, ruby, node, ocaml).
        (mod.rs generate_language_tabs_html: drop primary/overflow split; PRIMARY_LANGUAGES
        = all 11. tab_order already = the 11.)
[ ] I6. api.json: rename language displayName "JavaScript" -> "JS" (node lang, api.json:1556).
        (Leave the showcase line "Goodbye, JavaScript." text as-is — that's prose, not the tab.)
[ ] I7. Lang tab "C" should have the same min-width as "Python"/"Rust" (button min-width
        so single-char labels aren't cramped). (main.css .lang-grid button min-width.)

## NOTES / SEAMS (verified)
- Index hero + version + search mount: templates/index.template.html (the #latestrelease
  div + #azul-search-mount.azs-mount-inline). Hero h1 in <header class="grid-row">.
- Lang tabs: mod.rs generate_language_tabs_html (~line 160-221); PRIMARY_LANGUAGES const +
  overflow "more languages" details (~line 213). CSS .lang-grid / .lang-more in main.css ~499.
- Search: get_search_init + enum PageKind{Guide(keys),Api,Other} in mod.rs ~708-770;
  azul-search.js / azul-search.css in templates/. Guide pages pass PageKind::Guide(keys);
  index passes Other; api pages pass Api. (D2 changes which kind guide *pages* vs the guide
  *overview* use, + an expand-on-load mode seeded from default-search-keys.)
- Fonts live in doc/fonts/ + @font-face in main.css; guide.rs/blog.rs/mod.rs have inline CSS
  naming fonts; deploy.rs copy_static_assets() hardcodes which font files get copied.
- api.json displayName JavaScript @1556; tabOrder lists the 11 langs @ ~7-19.

## EXECUTION ORDER
P1 fonts (F1-F5): restore Instrument Serif, fetch Red Hat Mono + Playfair fix, reassign,
   update deploy.rs copy list + all inline CSS. Rebuild + eyeball.
P2 index (I1-I7): hero/showcase font classes, version column, search-bar redesign,
   drop more-languages (11 tabs), api.json JS rename, C min-width.
P3 search/dark-mode (D1-D3): the column layout + guide-vs-api scoping + expand-on-load.
   (Largest; do last, carefully.)
Rebuild via `cargo run -r -p azul-doc deploy debug`, serve doc/target/deploy on :8799, open.
