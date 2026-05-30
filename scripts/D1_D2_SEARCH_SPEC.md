# D1 / D2 — guide search restructure (SPEC, ready to implement on a clean channel)

User: IMPLEMENT (twice). Paused mid-task ONLY because the tool Read + Bash-stdout
channel degraded to ~1-line truncation — unsafe to blind-edit the 382-line
azul-search.js + multi-function guide.rs. A session RESTART clears this (seen 2x
today). Fonts (Imbue + Red Hat Display) ARE done + committed (8de83f8a3).

## GOAL
D1. The guide search must NOT be a floating top-right overlay. Put it IN the page
    as a COLUMN next to the guide content (inline mount).
D2. Scope by page:
    - guide OVERVIEW page (guide.html) → keep the full guide (pagefind) search.
    - individual guide pages → use the API search, EXPANDED ON LOAD, seeded from
      that page's frontmatter `default-search-keys`, presented as
      "direct links to the API docs for items mentioned on this page."

## EXACT SEAMS (grep-verified before the channel died)
- doc/src/docgen/guide.rs:
    * line 261-263: individual guide page calls
        get_search_init(PageKind::Guide(&guide.default_search_keys))
    * line 622: an overview/secondary page calls get_search_init(PageKind::Guide(&[]))
    * line 870: another calls get_search_init(PageKind::Guide(&[]))
    * search_script is substituted into the page templates at lines 595, 699, 900.
    * Need to identify which fn is the OVERVIEW (guide.html) vs the per-PAGE
      renderer: `grep -nE '^(pub )?fn ' doc/src/docgen/guide.rs`. The fn that
      renders one markdown guide page (uses guide.default_search_keys, line 261)
      = the PER-PAGE one → switch to the new API-expanded mode. The overview fn
      (renders the guide index / table of contents) keeps PageKind::Guide.
- doc/src/docgen/mod.rs:
    * enum PageKind {Api, Guide(&[String]), Other} at ~831.
    * get_search_init at ~840: builds window.__AZUL_SEARCH_CONFIG__ = {mode, route,
      defaults}. Guide → ("guide","newtab",defaults_json). Api → ("api","stay","[]").
- doc/templates/azul-search.js (~382 lines):
    * mount(host) ~540: `inline = host.hasAttribute('data-azs-inline') ||
      host.classList.contains('azs-mount-inline')`; picks INLINE_TEMPLATE vs
      FLOATING_TEMPLATE. So an INLINE mount needs a host div with
      class="azs-mount-inline" / data-azs-inline IN the page (like the index does
      at #azul-search-mount). With NO such host the script auto-creates the
      floating default mount (`#azul-search-mount-default` from get_search_init).
    * loadConfig() ~53 reads window.__AZUL_SEARCH_CONFIG__ {mode,route,defaults}.
    * INLINE_TEMPLATE/DEFAULTS handling: the script already shows `defaults` when
      the input is empty (per get_search_init docs) — that's the basis for
      "expanded on load".
- doc/templates/main.css: `.page-search { max-width:320px; margin-left:auto; }`
  (the inline content-page search wrapper). `#guide { max-width:700px }` is the
  article column.

## IMPLEMENTATION PLAN
1) mod.rs: add a PageKind variant for the per-page case, e.g.
     `GuideApiLinks(&'a [String])`
   get_search_init arm → mode="api", route="newtab", defaults=keys_json, AND a
   new flag like `expand:true` in __AZUL_SEARCH_CONFIG__ so the panel opens on
   load showing the default keys as API-doc links. (Api index already powers
   linking each key to /api/<version>#<Type>.)
2) guide.rs:
   - PER-PAGE renderer (the one at line 261): switch
       PageKind::Guide(&guide.default_search_keys)
     → PageKind::GuideApiLinks(&guide.default_search_keys)
   - OVERVIEW renderer: keep PageKind::Guide(&[]) (pagefind full-guide search).
   - In the PER-PAGE template, add an inline mount host as a COLUMN: wrap the
     article + a side column, e.g.
        <div class="guide-layout">
          <div id="guide"> …article… </div>
          <aside class="guide-search-col">
            <div class="azs-mount-inline" data-azs-inline></div>
          </aside>
        </div>
     (or float the mount right of #guide). Remove reliance on the floating
     default mount for guide pages.
3) azul-search.js:
   - honor cfg.expand: when true, open the panel on mount and render cfg.defaults
     immediately (as API links) instead of waiting for input/focus.
   - ensure mode="api" + route="newtab" links each default key to the API page.
4) main.css: add `.guide-layout` (grid: 1fr + a ~260px search column, collapses
   to single-column < 1100px) + `.guide-search-col` (sticky top). Keep the
   existing `.page-search` / dark-mode tokens (D3 already done).
5) Build: `cargo run -r -p azul-doc deploy debug`; serve doc/target/deploy:8799;
   verify: individual guide page shows an inline search COLUMN (not floating),
   pre-expanded with the frontmatter keys as API links; guide.html overview still
   has the full pagefind search; dark mode OK.

## VERIFICATION COMMANDS (when channel is healthy)
  grep -n 'GuideApiLinks\|PageKind::Guide' doc/src/docgen/guide.rs doc/src/docgen/mod.rs
  grep -c 'guide-search-col\|guide-layout' doc/target/deploy/guide/*.html
  grep -c 'azs-mount-inline' doc/target/deploy/guide/hello-world/rust.html   # want >=1 (inline)
  # overview keeps pagefind:
  grep -c 'mode: "guide"' doc/target/deploy/guide.html

## STATUS / GIT
- origin/mobile-ios-android last pushed = 5a9199c0f (per the last successful push
  this session); MANY local commits ahead are UNPUSHED, including the whole web
  round-2 + the Imbue/Red Hat Display swap (8de83f8a3). PUSH them on resume:
  `git push` then dispatch `gh workflow run rust.yml -f run_mode=website`.
- A local preview server may be running on :8799 (doc/target/deploy). Kill with
  `pkill -f "http.server 8799"`.
