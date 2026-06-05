# Refactor: node-scoped inline CSS via `Root(n)` selector (fixes #47)

Status: PLAN (no code yet). Owner: cron loop. Branch: `mobile-ios-android`.

## 1. Problem (#47)

`Dom::set_css("background: red")` on a **non-root** node paints the **whole window**.

Root cause (empirically confirmed, firing 54–55): `set_css(s)` → `Css::parse_inline(s)`
wraps the string in `* { … }` and pushes it onto that node's `.css` vec. In
`CssPropertyCache::restyle` (`core/src/prop_cache.rs:1030`) a rule whose selector is
exactly one `Global` (`*`) is treated as **global-only**: its declarations are pushed
into `self.global_css_props` (`prop_cache.rs:1077`) and applied to **every node**
(`build_compact_cache_with_inheritance`) — skipping per-node matching as an optimization.
So a non-root node's `* { … }` leaks to the entire tree. The two cascade entry points
both flatten every node's `.css` into one global stylesheet and drop the owner node id:
`create_from_dom` → `collect_css_from_dom` (`styled_dom.rs`), and `create_from_fast_dom`
(`styled_dom.rs:963`, literal `// TODO: respect node_id scoping`).

## 2. Goal

`set_css` declarations apply to **the owning node only**, then **normal cascade +
inheritance** (the HTML `style=""` model). A selector string in `set_css` scopes to the
owner's **subtree**. No leakage. Cleanly prepare (NOT implement) parallel per-subtree
cascading.

## 3. Current architecture (where matching + cascading happen)

- **Attachment.** `Dom { root: NodeData, children, css: CssVec }`. `set_css`/`with_css`
  push `parse_inline(s)` onto `Dom.css`. Existing type already models the concept:
  `CssWithNodeId { node_id: usize /* 1-based, 0 = global */, css }` (`dom.rs:3317`) — used
  by `FastDom`.
- **Flatten.** `convert_dom_into_compact_dom` (`styled_dom.rs:2005`) walks the tree in
  pre-order assigning flat `NodeId`s; `node_data[id] = dom.root.copy_special()` (css is a
  `Dom` field, not `NodeData`, so it is NOT carried — it's collected+stripped separately).
- **Cascade.** `restyle` (`prop_cache.rs:1030`): `css.sort_by_specificity()`, then split
  rules into **global-only** (`[Global]`) → `global_css_props` (all nodes) vs **specific**
  → per-node `matches_html_element`. Inheritance applied in
  `build_compact_cache_with_inheritance`.
- **Matching.** `matches_html_element` (`style.rs:51`) walks selector groups
  **right-to-left**; `match_single_selector` (`style.rs:442`) matches
  `Global/Type/Class/Id/PseudoSelector/Attribute` against `node_data` + `CascadeInfo` —
  it has **no node identity** today.

## 4. Design

### 4.1 New selector variant
`CssPathSelector::Root(usize, usize)` in `css/src/css.rs` — the owning node's **subtree
range** `[start, end]` (inclusive flat NodeIds; `end = start + estimated_total_children`,
since the flat arena lays subtrees out contiguously). Matches a node iff
`start <= node <= end`. A LEAF owner has `end == start` (matches only itself). Distinct
from `Global` (genuine `*`); UA/system `*` stays `Global`.

### 4.2 Scoping = `push_front(Root(n))` onto every inline rule's selector path
A CSS block is matched against a `Vec<CssPathSelector>` (`CssPath.selectors`). "Inline css"
= everything in a node's `.css` (all via `parse_inline`, so each rule's path already starts
with the wrapper `Global`). At **flatten**, once rules are final AND the flat_id `n` is
known, **prepend `Root(n)` to the front** of each rule's `selectors` (rebuild the vec with
`Root(n)` first). Declarations + `@os`/`@media`/`@lang` conditions are untouched.

There is no combinator between `Root(n)` and the original leading `Global`, so they form a
**compound** (the same element must match both); `Global` is always true, so the leftmost
group reduces to "the element is node n". Worked cases (owner flat id `n`):
- `[Global]` (bare decl)             → `[Root(n), Global]`                       = node n
- `[Global, Children, Class(foo)]`   → `[Root(n), Global, Children, Class(foo)]` = `.foo` under n
- `[Global, Children, Global]` (`*`) → `[Root(n), Global, Children, Global]`     = n's descendants

INVARIANT (the whole point): a user CANNOT escape the scope. `with_css("* { color:red }")`
parses to `* { * { color:red } }` → `[Root(n), Global, Children, Global]` = **n's
descendants only**, never the whole tree. `with_css("color:red")` → `[Root(n), Global]` =
**node n only**. So inline css can never mutate global style. `@os`/`@media` conditions are
on each `CssRuleBlock` and ride along untouched (`color:red; @os windows { color:blue }` →
two rules, both `push_front(Root(n))`, the second keeping its `@os windows` condition).

`parse_inline` is UNCHANGED — scoping is purely a core/flatten concern; the owner rides
**inside** the `Root(n)` selector, so it survives `restyle`'s specificity sort with no
extra plumbing (no `CssRuleBlock` field, no `restyle` owner param).

### 4.3 Matching
Thread the **current walk node's `NodeId`** into `selector_group_matches` /
`match_single_selector`. New arm: `Root(n) => current_node.index() == n` (pure identity
check; the `Global` it compounds with is always true). The existing right-to-left group
walk then yields:
- leftmost compound `[Root(n), Global]` matched against the target → target == n
  (bare decls land on the owner);
- with a `Children` combinator, matched against an ancestor → "descendant of n"
  (selector strings / user `*` scope to the subtree).
No other matcher change. `match_single_selector` currently takes `node_data` + `CascadeInfo`
(no identity), so the only new plumbing is passing the current `NodeId` down.

### 4.4 restyle
No owner param needed. `Root(n)` is not `[Global]`, so Root rules automatically fall out
of the global-only fast path into the per-node `specific_rules` matcher. Genuine UA/system
`* {}` rules (applied via `apply_ua_css`, not via node `.css`) stay `Global` → still
`global_css_props`. The only change: pass the candidate/walk node id down to the matcher.
(Optimization, LATER: a `[Root(n)]` rule matches exactly one node, so it can be applied
directly to node n instead of scanned against all nodes — note it, don't build it yet.)

### 4.5 Owner capture in both paths
- `create_from_fast_dom`: stop flattening `CssWithNodeIdVec`; use its `node_id` as the
  `Root(n)` owner.
- recursive `create_from_dom`: capture `(flat_id, css)` while flattening
  (`convert_dom_into_compact_dom`) instead of `collect_css_from_dom`'s owner-less concat,
  producing the same `CssWithNodeIdVec`. Unifies both paths.

## 5. Parallelization — PREPARE, DO NOT IMPLEMENT (per request)
With author CSS node-scoped, a node M is matched only by: `Root(a)` rules of its ancestors
`a` (incl. M), genuine UA `*` (read-only global), and author class/id rules. Consequences
to preserve as a seam (no threads/rayon now):
- Per-node matching stays a **pure read** (writes only M's own `css_props`) → already
  embarrassingly parallel (m×n).
- Keep css grouped by owner (`CssWithNodeId`) so a subtree's rule set is identifiable.
- `build_compact_cache_with_inheritance` is a parent→child walk; sibling subtrees are
  independent → fan-out point. Leave a comment marking it.
- **Subtree slicing (the future fan-out).** The flat arena lays each subtree out
  **contiguously**: node `k`'s subtree is `[k, k + subtree_len(k)]`, and
  `estimated_total_children` / `subtree_len` already give the length. Because every author
  rule is now `Root(owner)`-tagged, the rule set for a subtree `[a, b)` is exactly the
  rules with `owner ∈ [a, b)`. So the tree partitions into disjoint subtrees
  (`0..n, n..o, o..p, …`), each cascaded independently from its rule slice + the boundary
  inherited context. The `Root(n)` scoping + contiguous layout are *precisely* what make
  this slicing possible — that is what this refactor sets up (no scheduler yet).

## 6. Unit-test strategy (fast — no app rebuild)
- `cargo test -p azul-css`: `parse_inline`/scope helper turns `*` paths into `Root(n)`;
  `Display` + specificity round-trip; conditions preserved.
- `cargo test -p azul-core`:
  - matcher: build a tiny hierarchy; assert `Root(n)` matches only node n, and
    `Root(n) *` matches exactly n's subtree, nothing outside.
  - **end-to-end cascade** (the #47 regression test): `Dom` body(white) with two sibling
    divs `set_css("background:red")` / `set_css("background:blue")`; `create_from_dom`;
    assert computed background per node — A red, B blue, body white, **no leak**; and a
    nested child of A inherits `color` but NOT `background`.
- App build + headless red/blue render: ONCE at the very end as confirmation.

## 7. Implementation order (each step compiles + unit-tests in isolation)
1. `azul-css`: add `Root(usize)` + `Display` + specificity (0, like `Global`) + fix
   exhaustive matches (`css.rs`, `parser2.rs`, `codegen/rust.rs`). `cargo build -p azul-css`.
2. `azul-css`: scope helper `scope_rule_path(&mut CssPath, flat_id)` + unit tests.
   (Lives in css since it only manipulates `CssPath`.) `cargo test -p azul-css`.
3. `azul-core` `style.rs`: thread node id; `Root(n)` arm + matcher unit tests.
   `cargo test -p azul-core`.
4. `azul-core` `styled_dom.rs`: capture owner in both paths; apply `scope_rule_path` at
   flatten; drop the global concat. End-to-end cascade unit test. `cargo test -p azul-core`.
5. `azul-dll` build + headless red/blue render — final confirmation only.

## 8. Risks / open questions
- **Specificity of `Root`**: treat as 0 (scope marker). `set_css` rules already carry
  `rule_priority::INLINE` (highest), so they win regardless; the class/id parts of a
  scoped selector still count normally.
- **`copy_special` strips css**: confirm the flatten can still see `Dom.css` at the point
  we capture owners (it can — we capture before/at the walk, not from `NodeData`).
- **Anonymous nodes** (tables) get flat ids too; `Root(n)` on an author node won't target
  them — fine.
- **api.json/codegen**: `CssPathSelector` is FFI-exposed; adding `Root(CssScopeRange)`
  (a new `#[repr(C)]` struct) regenerates bindings. Acceptable (we own the crates).

## 9. Status
- [x] **Step 1** — `azul-css`: `CssPathSelector::Root(CssScopeRange{start,end})` +
  `CssScopeRange::contains` + `Display` + codegen arm. `cargo build -p azul-css` ✓.
- [x] **Step 2** — `azul-css`: `CssPath::push_front_scope` + `root_scope_tests` (3 tests).
  `cargo test -p azul-css root_scope` ✓.
- [x] **Step 3** — `azul-core` `style.rs`: candidate `NodeId` threaded into
  `selector_group_matches`/`match_single_selector`; `Root(range)` range-test arm.
- [x] **Step 4** — `azul-core` `styled_dom.rs`: `scope_inline_css` (pre-order, matches
  flatten ids) push_front-s `Root([id, id+estimated_total_children])` onto every inline
  rule in `create_from_dom`. End-to-end test `core/tests/css_scope_47.rs` ✓ (red/blue
  siblings: each own bg, differ, **body none = no leak**). All ~476 core tests pass.
- [~] **Step 5** — headless render (firing 56): CONFIRMS the leak is gone — the window is
  WHITE, not the all-red of firing 53. BUT the two test divs render with no visible box:
  bare-`set_css` LAYOUT props (width/height) don't size the node. Hard data from the test:
  `css_props[divA] = ["background"]` only (paint), NO width/height; `css_props[body] = []`
  (no leak ✓). So restyle routes layout-hot props differently from paint props, and they
  don't land for a `set_css`-matched rule.
- [ ] **FOLLOW-UP A (HIGH — verify regression vs pre-existing)** — bare-`set_css`
  width/height don't reach `css_props`/layout. `parse_inline` keeps them (test added);
  background scopes fine. Trace restyle's layout-hot pipeline (the "Tier 1/2/2b direct
  typed getters" the `get_property` comment names, prop_cache.rs:1934): does it match rules
  with a SEPARATE matcher that does NOT handle the new `Root` selector? If so, the `Root`
  push_front broke layout-prop application (a regression to fix — likely add the `Root` arm
  there too). If bare-`set_css` layout props never worked (examples size via classes /
  content), it's pre-existing. The #47 LEAK fix is correct + verified independent of this.
- [ ] **FOLLOW-UP B** — FastDom/XML path (`create_from_fast_dom`) still merges css globally;
  apply the same `Root(range)` scoping (range from the owner node_id's subtree).

Committed: `58bcce130` (fix+tests), `6f5df0569` (plan), `438d8d46a` (parse_inline test).
