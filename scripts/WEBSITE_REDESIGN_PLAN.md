# Website redesign + restructure plan (erp-site design → azul, blue/gold)

Status: PLAN (ready to execute on a clean tool channel — see CHANNEL note at end).
Author pass 2026-05-30. Source design: /Users/fschutt/Development/erp-site (READ-ONLY,
never modify it). Scope clarified by user: "copy the erp-site design, fonts, CSS — the
site design already works (main page, blog, etc.) — but keep the azul blue/gold colors."

================================================================================
## A. WHAT TO COPY FROM erp-site (design only)
================================================================================
erp-site is a tiny Python-generated static site. Relevant files:
- erp-site/template.html      — page skeleton: <head> Google-Fonts link + styles.css,
                                 body slots {{NAV}}{{CONTENT}}{{CONTACT_GLASS}}{{FOOTER}}{{SCRIPTS}}.
- erp-site/assets/styles.css  — 940-line editorial design system (the thing to port).
- erp-site/config.json        — brand/nav/gradient metadata (reference only).
- erp-site/generate.py        — its generator (reference only; azul has its own Rust one).

### Fonts (erp uses Google Fonts CDN):
  - DISPLAY / headings: 'Playfair Display' (weights 400,600,700,800,900)
  - BODY / UI:          'Rubik' (400,500,600,700)
  (erp also preloads Crimson Pro + IM Fell English but does NOT use them — skip.)
  DECISION NEEDED: self-host (download woff2 into doc/fonts/ + @font-face, matches azul's
  existing self-hosted InstrumentSerif/SourceSerifPro + the whole azul independence theme)
  vs CDN link (what erp does, simplest). RECOMMEND self-host for consistency + offline/PDF.

### Design tokens (erp :root) — PORT THESE, recolored to azul:
erp original:
  --color-text:#1a1a1a  --color-text-light:#4a4a4a  --color-bg:#fafaf9
  --color-accent:#8b3a62 (magenta)  --color-accent-hover:#6b2847  --color-border:#e5e5e3
  hero gradient: linear-gradient(135deg,#3d2456 0%,#8b3a62 50%,#c2185b 100%)

================================================================================
## B. AZUL BLUE/GOLD PALETTE (verified from doc/templates/main.css)
================================================================================
Azul's existing brand colors (hex frequency-ranked from main.css):
  BLUE deep:     #000428 (near-black blue, gradient start)
  BLUE primary:  #004e92 (the main azul blue, 6 uses)
  BLUE bright:   #0084ff (link/interactive blue, 4 uses)
  BLUE alt:      #0446bf, #1d6fdb, #1a2436
  GOLD:          #facb26 (azul gold, 2 uses)  + #e5c07b (muted gold accent)
  neutrals:      #fafafa bg, #1b1e25 dark panels, #d8d8d8 borders

### RECOLOR MAP (erp token -> azul value):
  --color-accent:        #8b3a62 -> #004e92   (azul primary blue)
  --color-accent-hover:  #6b2847 -> #000428   (azul deep blue)
  --color-gold:          (new)   -> #facb26   (azul gold — CTA highlights, card accents, stars)
  --color-text:          #1a1a1a (keep)
  --color-text-light:    #4a4a4a (keep)
  --color-bg:            #fafaf9 -> #fafafa (azul's near-white; or keep erp cream — minor)
  --color-border:        #e5e5e3 -> #e6e6e6
  hero gradient:         linear-gradient(135deg,#000428 0%,#004e92 55%,#0084ff 100%)
                         (azul's signature blue gradient; gold used as the accent ON it)
  feature-card.has-gradient: same azul blue gradient; gold for the "→" bullets / headings accent.
  fonts:                 --font-serif/--font-display: 'Playfair Display', Georgia, serif
                         --font-sans: 'Rubik', -apple-system, ... (port erp's stacks verbatim)

================================================================================
## C. erp design-system components to port (from styles.css)
================================================================================
All verbatim except colors (use the azul vars above):
  .container (max-width:1200px; padding:0 24px)
  .navbar (FIXED, centered, glass: rgba(255,255,255,.8)+backdrop-filter blur(20px) saturate(80%),
           border-radius:12px, box-shadow 0 8px 32px rgba(0,0,0,.1)); .nav-brand (Playfair 22px);
           .nav-links (gap 32px); .lang-switcher
  .hero (padding 180px 0 100px; .has-gradient -> white text); .hero .container (2-col grid 1fr 1fr,
         gap 64px); .hero h1 (Playfair 56px/900, -0.03em); .hero-subtitle; .cta-buttons;
         .btn / .btn-primary / .btn-secondary / .btn-hero-primary / .btn-hero-secondary
  sections: .features-section/.faq-section/.cta-section/.text-section (padding 80px 0, white,
         border-bottom 1px border)
  .features-grid (auto-fit minmax(280px,1fr) gap 64px) + .grid-2x2 (repeat(2,1fr) gap 48px)
  .feature-card (padding 32px; .has-gradient -> azul gradient, radius 12px, white, hover translateY(-4px));
         h3 Playfair 28px/600; p Rubik 17px/500; ul li with "→" before (recolor → gold)
  .faq-* (accordion; .faq-question Playfair 20px/600, ::after "+"/"−"; .faq-answer max-height toggle)
  footer (padding 48px 0, white, centered, text-light 14px)
  @media (max-width:768px): hero 1-col, h1 40px, grid 1-col, nav-links hidden
  (lines ~431-940 of erp styles.css = blog/about/feature-detail page styles — port if/when
   azul gets those pages; not needed for the landing page.)

================================================================================
## D. NEW LANDING PAGE (azul.rs/  ->  later azlin.io/)
================================================================================
Build a NEW root index in the erp style, azul-colored:
  - glass NAV: brand "Azul" (later "Azlin") + links (UI, Workspace, OS, Docs, Blog) + (lang later)
  - HERO (.has-gradient azul blue): h1 e.g. "One framework. Every platform."
    subtitle about native Rust UI; CTAs: "Get started" (gold btn) + "Docs".
  - PRODUCT CARDS (.features-grid.grid-2x2 — 3 cards, the azul-gradient checkerboard look):
      * UI         "Azul UI"        — the GUI framework (REAL: links to /ui). desc + bullets.
      * Workspace  "Azul Workspace" — FAKE/"coming soon" badge. metapackage of apps.
      * OS         "Azul OS"        — FAKE/"coming soon" badge. the desktop OS vision.
  - small FAQ (optional) + footer.
  These map to the future `brew install azlin/ws` / `os/*` products in PACKAGE_DISTRIBUTION_PLAN.md.

### Route move: current docs landing (today azul.rs/index.html) -> azul.rs/ui
  - The current generated index (the azul-doc "home"/index.template.html output) becomes /ui/.
  - The NEW erp-style page becomes the root /.
  - Keep all existing doc routes (/guide, /api, /release, /reftest, …) unchanged.

================================================================================
## E. GENERATOR INTEGRATION (azul is Rust-generated, NOT Python like erp)
================================================================================
Azul site is generated by doc/src (the azul-doc crate). Key seams (MAP EXACTLY on restart):
  - doc/src/docgen/mod.rs — generates index.html (grep: index.template.html / generate_index /
    PageKind / the home page). This is where the root index is emitted.
  - doc/templates/index.template.html (11.5 KB) + index.section.template.html — current home layout.
  - doc/templates/main.css (21 KB) — current site CSS (has the azul blue/gold already).
  - doc/src/docgen/get_common_head_tags / get_sidebar / get_prism_script — shared head/chrome.
  - doc/src/dllgen/deploy.rs copy_static_assets() — copies templates/*.css/js + fonts/* into the
    deploy dir at build time (THIS is where a new landing.css + Playfair/Rubik fonts get wired).
PLAN:
  1. Add Playfair+Rubik fonts to doc/fonts/ (self-host) + @font-face in a new landing.css
     (or fold into main.css). copy_static_assets() already copies doc/fonts/* — extend its
     hardcoded font list (it currently names InstrumentSerif/SourceSerifPro files explicitly).
  2. Add a landing.css with the ported+recolored erp design system.
  3. New fn in docgen (e.g. generate_landing_html) emitting the erp-style root page using
     landing.css; write it to deploy root index.html.
  4. Re-point the current home generator output to /ui/index.html (a one-route move).
  5. Update internal links that point at "/" as "the docs home" to "/ui" where appropriate
     (sidebar logo, etc.) — audit get_sidebar() + header <a href='https://azul.rs/'>.

================================================================================
## F. FILE-TREE RESTRUCTURE (organize deploy output; do NOT change content)
================================================================================
User: "restructure the website artifacts a bit, just move them around so it's more organized."
Targets (the generated deploy/ tree + doc/templates/ source):
  - doc/templates/ mixes CSS (main.css, azul-search.css), JS (azul-search.js, azul-review.js,
    prism_code_highlighter.js), SVG (logo.svg, fleur-de-lis.svg), HTML templates. Consider
    doc/templates/{css,js,img,html}/ subdirs — BUT copy_static_assets() + include_str! paths
    are hardcoded; every move needs a matching path update in deploy.rs/docgen. Low risk if done
    with grep-confirmed path edits + cargo check.
  - Deploy output: keep /release, /api, /guide; add /ui (docs home), / (new landing). The
    registry mirrors (/apt,/rpm,/arch,/alpine,/maven,/pypi,/npm,/nuget,/gems,/homebrew-azul.git,
    /dl) already land at root — leave them.
  CAUTION: this is the riskiest part (hardcoded include_str!/copy paths). Do it LAST, one move +
  cargo check at a time. Defer if time-boxed.

================================================================================
## G. SCRIPTS/ CLEANUP (separate ask, also pending)
================================================================================
scripts/ is bloated with ~68 dated session-log markdown files (~2.2 MB): HANDOFF_*, SESSION_*,
M<n>_*_PROMPT, STATUS_REPORT_*, OVERNIGHT_*, *_AUDIT_2026_*, NEXT_*_PROMPT, etc.
LOAD-BEARING (never delete — referenced by rust.yml): analyze_coverage.py, build_registry_mirrors.sh,
build-android.sh, build-ios.sh, check_dep_justifications.py, coverage.sh,
dependency-justifications.toml, docs_pdf_cdp.mjs, docs_pdf_book.mjs, docs_pdf_merge.py,
docs_to_pdf.sh, e2e_language_matrix.sh, screenshot_single.sh (+ build_remill.sh, build_all.sh, etc.).
KEEP the real plans: PACKAGE_DISTRIBUTION_PLAN.md, this file.
A GUARDED audit script is ready at /tmp/scripts_cleanup_audit.sh: it lists only scratch-pattern
.md files that grep-confirms are referenced NOWHERE outside scripts/*.md, and prints a KEPT list
for any that ARE referenced. PROCEDURE on clean channel:
  1. bash /tmp/scripts_cleanup_audit.sh  (review /tmp/cleanup_candidates.txt + /tmp/cleanup_kept.txt)
  2. git rm the confirmed-unreferenced candidates (so it's reversible), commit
     "chore(scripts): remove dated session-log/handoff scratch notes".
  3. cargo check / a website build still pass (they don't depend on the md).
Do NOT delete anything in scripts/research/ or the architecture/design docs without a second look.

================================================================================
## H. CHANNEL NOTE (why this is a plan, not done yet)
================================================================================
The Bash + Read tool channel degraded mid-task this turn (outputs truncating to "..."; the same
corruption seen earlier that a session RESTART clears). Per the established rule, destructive ops
(deleting scripts) and large blind edits were STOPPED. The Write tool stayed reliable, so this
plan + PACKAGE_DISTRIBUTION_PLAN.md are committed durably. EXECUTE the above on a fresh channel.

State at pause: all registry/CI/docker/pdf work from earlier this turn is committed + pushed
(origin/mobile-ios-android) and a run_mode=deploy run (26690070273) is in flight. Nothing here is
half-applied — items A-G are unstarted (design code not written yet); only the PLANS are on disk.
