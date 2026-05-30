# Website Round 4 — feedback checklist (ready to execute on a clean channel)

Captured from user feedback after the D1/D2 + Imbue/Red Hat Display work landed.
Held from blind execution ONLY because the tool stdout/Read channel stalled
(recurring this session; a session restart cleared it twice today). Fonts +
D1/D2 ARE committed and verified.

## Items

R4-1. **JS code blocks lost syntax highlighting** after the api.json
  `displayName` "JavaScript" → "JS" rename (round 2, I6).
  → Cause to confirm: the Prism language CLASS on `<pre><code class="language-…">`
    is being derived from `displayName` (now "js"/"JS") instead of the stable
    Prism language id `javascript`. Prism has no `language-js` grammar, so it
    renders unhighlighted.
  → Fix: keep the Prism class = canonical id (`javascript`) regardless of the
    tab `displayName`. Look in doc/src/docgen/apidocs.rs (code-tab rendering)
    and wherever language tabs map name→prism class. Likely a `displayName` vs
    `prismLang`/`id` split is needed, or map "JS"→"javascript" at emit.
  → Verify: `grep -o 'class="language-[a-z]*"' deploy/api/0.2.0.html | sort -u`
    should still contain `language-javascript`; the JS tab must highlight.

R4-2. **Big h1 headings: Playfair Display → Imbue, but BOLDER.** Playfair
  doesn't work for the large eye-catcher. So Imbue is now used for BOTH the big
  headings and the subtitles; Playfair is dropped entirely.
  → main.css: `main h1, h1 { font-family: "Imbue", … }` (was Playfair);
    `main h1 { font-weight: 800; }` (was 700) + keep responsive sizes;
    `.feature-image h2.feature-title` → Imbue, heavier weight (drop the
    text-shadow fake-bold, use a real wght).
  → guide.rs inline css: h1 `font-family:'Imbue'` + `font-weight:800`
    (2 sites: generate_guide_html ~324, generate_guide_mainpage ~625).
  → blog.rs inline css: h1 → Imbue + 800 (2 sites ~148, ~165).
  → Remove Playfair: @font-face in main.css, preload `<link>` in mod.rs (~810),
    deploy.rs copy list (~1881), and `doc/fonts/PlayfairDisplay-*.ttf` (+OFL).
  → Verify deploy: 0 refs to "Playfair" in deploy/main.css and guide/blog html.

R4-3. **Default Imbue weight a little bolder** (subtitles / h2-h6).
  → guide.rs h2 `font-weight: normal` → `500` (or `600`).
  → main.css `main h2, h2, h4, h5, h6` — add `font-weight: 500;`.
  → Keep `font-optical-sizing: auto`.

R4-4 (release box, Image #5) — multiple:
  .1 **"Search API" placeholder text must be BOLD.** azul-search.css
     `.azs-input` (and the inline input) → `font-weight: 700;` incl. its
     `::placeholder`. (Placeholder needs its own `::placeholder { font-weight:700 }`.)
  .2 **Use flexbox for the two columns** of the release box (#latestrelease:
     the right-aligned version block + the links block). Read
     doc/templates/index.template.html #latestrelease and make it
     `display:flex; justify-content: space-between` (links left, version right),
     search bar full width below.
  .3 **Individual guide pages still showed "Search Guide" not "Search API".**
     D2 (PageKind::GuidePage) already fixes this in the latest build — CONFIRM
     in the rebuilt deploy: `grep -c 'Search API' deploy/guide/hello-world/rust.html`
     ≥1 and `grep -c 'Search guide' …rust.html` == 0; overview guide.html keeps
     "Search guide".

R4-5. **Search bar styling (both API + guide search):**
  - NO border-radius (square corners): `.azs-inline-row { border-radius: 0; }`
    (and `.azs-panel-inline { border-radius: 0; }` to match).
  - More contrast + thicker border: bump `--azs-border` to a darker value and
    `.azs-inline-row { border-width: 2px; }`.
  - Fat text: input `font-weight: 700` (ties to R4-4.1).
  - More contrast overall (bg/fg vs page).

R4-6. **Guide search column must not push the h1 down / collision → overlay:**
  - The search column must NOT displace the article h1 downward. (Column layout
    already avoids the old top-band displacement; confirm h1 sits at top.)
  - When the viewport is too narrow for [article | 300px search] side by side,
    the search must switch to an OVERLAY (floating / mobile mode) ON TOP of the
    text — NOT wrap below pushing layout. Implement via media query: at the
    narrow breakpoint set `.guide-search-col { position: fixed; }` overlay
    (anchored bottom for mobile) + `#guide { flex-basis:100% }`.
  - **Fade-out:** overlay a fixed div with a vertical gradient
    `linear-gradient(to bottom, transparent, <page-bg>)` above the bar so the
    text appears to fade into the background under the search bar (contrast).
  - **Bottom padding:** add generous `padding-bottom` to guide pages (#guide)
    so scrolling to the end doesn't jam the last text under the overlaid bar.
  - Because the bar overlays at the bottom in this mode, flip the results panel
    to open UPWARD: `.azs-panel-inline { bottom: calc(100%+4px); top:auto }`
    inside the narrow media query.

## Notes / state
- Fonts (Imbue + Red Hat Display): committed e9d3a3cfd (PUSHED).
- D1/D2 (GuidePage API-links column): committed locally this session (verified:
  cargo check + build green; deploy greps correct). HEAD after that commit holds
  it. NOT yet pushed beyond the font commit — confirm with `git log --oneline`.
- Channel-safe edit method: /tmp Python splice with `assert count==1` per edit,
  atomic (writes only if ALL match). See /tmp/d1d2.py for the pattern.
- Preview: `python3 -m http.server 8799` in doc/target/deploy; pages were opened
  for review. Rebuild with `cargo run -r -p azul-doc deploy debug`.
- Build is green as of the D1/D2 commit; only pre-existing snake_case warnings.
