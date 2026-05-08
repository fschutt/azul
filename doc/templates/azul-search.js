// azul-search.js — quick search panel for the Azul docs.
//
// The module has two layers:
//
//   AzulSearch.search(index, query)      // pure, returns ranked Result[]
//   AzulSearch.mount({source, ...})      // renders a panel into the DOM
//   AzulSearch.attach({source, ...})     // mount() into a container we create
//
// `source` is one of:
//   { type: 'api-index', url: '/api/<v>.search.json' }
//   { type: 'pagefind',  url: '/pagefind/' }   // adapter stub, not active yet
//
// The headless `search()` function takes an already-loaded index object and
// is decoupled from any DOM. Tests/embedders can use it directly.
//
// Anchor convention matches what `apidocs.rs` emits: `#m.<module>`,
// `#st.<Class>`, `#<Class>.<member>`. When we run *on* the api page we
// emit relative `#anchor` hrefs; everywhere else we resolve against the
// configured `apiPageUrl`.

(function () {
  'use strict';

  // ---------- kind metadata ---------------------------------------------

  // Short kind code -> { label, badgeClass, weight }. Weight tweaks ranking
  // when otherwise tied: we'd rather surface a struct than one of its
  // 30 enum variants when the query also matches the parent type.
  var KIND = {
    m:  { label: 'mod',     cls: 'k-m',  weight: 1.0 },
    s:  { label: 'struct',  cls: 'k-s',  weight: 1.0 },
    e:  { label: 'enum',    cls: 'k-e',  weight: 1.0 },
    fp: { label: 'fnptr',   cls: 'k-fp', weight: 0.95 },
    fn: { label: 'fn',      cls: 'k-fn', weight: 0.9 },
    cn: { label: 'new',     cls: 'k-cn', weight: 0.9 },
    ev: { label: 'variant', cls: 'k-ev', weight: 0.7 },
    f:  { label: 'field',   cls: 'k-f',  weight: 0.7 },
    g:  { label: 'guide',   cls: 'k-g',  weight: 1.0 },
  };

  // ---------- pure search ------------------------------------------------

  // Tokenise: lowercase, split on whitespace, drop empty tokens. We keep
  // dotted/underscored words intact so e.g. "fn_args" stays one token.
  function tokenise(q) {
    return (q || '').toLowerCase().split(/\s+/).filter(Boolean);
  }

  // CamelCase initials match: query "wcr" hits "WindowCreateRequest".
  function initialsMatch(name, q) {
    var parts = name.split(/(?=[A-Z])|_/).filter(Boolean);
    if (parts.length < 2) return false;
    var initials = parts.map(function (p) { return p[0].toLowerCase(); }).join('');
    return initials.indexOf(q) === 0;
  }

  function scoreEntry(entry, tokens, fullQuery) {
    var name = entry.n.toLowerCase();
    var parent = entry.p ? entry.p.toLowerCase() : '';
    var module = entry.m ? entry.m.toLowerCase() : '';
    var doc = entry.d ? entry.d.toLowerCase() : '';
    var sig = entry.s ? entry.s.toLowerCase() : '';

    var score = 0;
    var allInName = true;
    var anyInBody = false;

    for (var i = 0; i < tokens.length; i++) {
      var t = tokens[i];
      var inName = false;

      if (name === t) { score += 1000; inName = true; }
      else if (name.indexOf(t) === 0) { score += 400; inName = true; }
      else if (name.indexOf(t) >= 0) { score += 200; inName = true; }
      else if (initialsMatch(entry.n, t)) { score += 350; inName = true; }

      if (parent.indexOf(t) >= 0) score += 80;
      if (module.indexOf(t) >= 0) score += 60;
      if (sig.indexOf(t) >= 0) score += 40;

      if (doc.indexOf(t) >= 0) {
        // Cap occurrence bonuses so a 4 KB doc doesn't dominate.
        var occ = 0, idx = -1;
        while ((idx = doc.indexOf(t, idx + 1)) !== -1 && occ < 6) occ++;
        score += 20 + occ * 4;
        anyInBody = true;
      } else if (!inName && parent.indexOf(t) < 0 && module.indexOf(t) < 0
                 && sig.indexOf(t) < 0) {
        // Token didn't match anywhere — disqualify (AND semantics).
        return -1;
      }

      if (!inName) allInName = false;
    }

    // Prefer hits where the whole query is consecutive in the name.
    if (fullQuery && name.indexOf(fullQuery) >= 0) score += 150;
    if (allInName) score += 100;
    // Slight nudge by kind weight.
    var kw = (KIND[entry.k] && KIND[entry.k].weight) || 1;
    return Math.round(score * kw);
  }

  function search(index, query, opts) {
    opts = opts || {};
    var limit = opts.limit || 50;
    var tokens = tokenise(query);
    if (tokens.length === 0) return [];
    var full = tokens.join(' ');
    var entries = (index && index.e) || [];
    var hits = [];
    for (var i = 0; i < entries.length; i++) {
      var s = scoreEntry(entries[i], tokens, full);
      if (s > 0) hits.push({ entry: entries[i], score: s });
    }
    hits.sort(function (a, b) {
      if (b.score !== a.score) return b.score - a.score;
      return a.entry.n.length - b.entry.n.length;
    });
    return hits.slice(0, limit);
  }

  // ---------- HTML helpers ----------------------------------------------

  function escapeHtml(s) {
    return String(s)
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
  }

  // Highlight every occurrence of any token in `text`. Case-insensitive.
  function highlight(text, tokens) {
    if (!text) return '';
    if (!tokens || tokens.length === 0) return escapeHtml(text);
    var pattern = tokens.map(function (t) {
      return t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    }).join('|');
    var re = new RegExp('(' + pattern + ')', 'gi');
    var parts = text.split(re);
    var out = '';
    for (var i = 0; i < parts.length; i++) {
      if (i % 2 === 1) out += '<mark>' + escapeHtml(parts[i]) + '</mark>';
      else out += escapeHtml(parts[i]);
    }
    return out;
  }

  // Pick the most relevant ~160-char snippet of `doc` to show.
  function snippet(doc, tokens, max) {
    max = max || 160;
    if (!doc) return '';
    if (doc.length <= max) return doc;
    var lower = doc.toLowerCase();
    var bestIdx = -1;
    for (var i = 0; i < tokens.length; i++) {
      var pos = lower.indexOf(tokens[i]);
      if (pos >= 0 && (bestIdx < 0 || pos < bestIdx)) bestIdx = pos;
    }
    if (bestIdx < 0) return doc.slice(0, max).trim() + '…';
    var start = Math.max(0, bestIdx - 40);
    var end = Math.min(doc.length, start + max);
    var prefix = start > 0 ? '…' : '';
    var suffix = end < doc.length ? '…' : '';
    return prefix + doc.slice(start, end).trim() + suffix;
  }

  // ---------- source adapters -------------------------------------------

  // Adapter: { load() -> Promise<{search(query) -> Promise<Result[]>}> }
  function apiIndexAdapter(url) {
    return {
      load: function () {
        return fetch(url, { credentials: 'omit' })
          .then(function (r) {
            if (!r.ok) throw new Error('HTTP ' + r.status + ' loading ' + url);
            return r.json();
          })
          .then(function (index) {
            return {
              type: 'api-index',
              version: index.v,
              apiPageUrl: '/api/' + index.v + '.html',
              search: function (q, opts) {
                return Promise.resolve(search(index, q, opts));
              },
              count: (index.e || []).length,
            };
          });
      }
    };
  }

  // Auto-discover: read /api/index.json (manifest produced by docgen) to
  // pick the latest version, then load that version's search.json.
  function apiDefaultAdapter() {
    var manifestUrl = '/api/index.json';
    return {
      load: function () {
        return fetch(manifestUrl, { credentials: 'omit' })
          .then(function (r) {
            if (!r.ok) throw new Error('HTTP ' + r.status + ' loading ' + manifestUrl);
            return r.json();
          })
          .then(function (manifest) {
            var v = manifest.latest;
            if (!v) throw new Error('Manifest has no "latest" field');
            return apiIndexAdapter('/api/' + v + '.search.json').load();
          });
      }
    };
  }

  // Pagefind adapter. Dynamic-imports `<url>pagefind.js` (the runtime
  // emitted by the pagefind CLI). Falls back to api-default if the
  // pagefind bundle isn't reachable, so a guide page still has working
  // search even when pagefind wasn't installed at deploy time.
  function pagefindAdapter(url) {
    url = url || '/pagefind/';
    if (url.charAt(url.length - 1) !== '/') url += '/';
    return {
      load: function () {
        return import(/* webpackIgnore: true */ url + 'pagefind.js')
          .then(function (pf) {
            // Initialise once. Older pagefind builds expose `init`; newer
            // ones initialise lazily inside `search`.
            var ready = pf.init ? Promise.resolve(pf.init()) : Promise.resolve();
            return ready.then(function () { return pf; });
          })
          .then(function (pf) {
            return {
              type: 'pagefind',
              // Each pagefind result expands into one entry per sub-result
              // (heading anchor) when available, falling back to the page
              // itself. Pagefind's excerpt is pre-marked HTML; we forward
              // it via `_html` so renderResults skips re-highlighting.
              search: function (q) {
                if (!q) return Promise.resolve([]);
                return pf.search(q).then(function (res) {
                  var results = res.results.slice(0, 30);
                  return Promise.all(results.map(function (r) {
                    return r.data();
                  })).then(function (datas) {
                    var out = [];
                    for (var i = 0; i < datas.length; i++) {
                      var d = datas[i];
                      var pageTitle = (d.meta && d.meta.title) || d.url;
                      var subs = d.sub_results || [];
                      // Score descending by pagefind's order; we cap at 50.
                      var baseScore = 1000 - i;
                      if (subs.length === 0) {
                        out.push(makePagefindEntry(pageTitle, null,
                          d.url, d.excerpt, baseScore));
                      } else {
                        for (var j = 0; j < subs.length; j++) {
                          var s = subs[j];
                          out.push(makePagefindEntry(s.title || pageTitle,
                            pageTitle, s.url || d.url, s.excerpt, baseScore - j * 0.01));
                        }
                      }
                    }
                    return out.slice(0, 50);
                  });
                });
              },
              count: 0,
            };
          })
          .catch(function (err) {
            if (window.console) {
              console.warn('AzulSearch: pagefind unavailable, falling back to api-default', err);
            }
            return apiDefaultAdapter().load();
          });
      }
    };
  }

  function makePagefindEntry(name, parent, url, excerptHtml, score) {
    return {
      score: score,
      entry: {
        k: 'g',
        n: name,
        p: parent || null,
        a: url, // already starts with `/` — resolveHref passes through
        d: excerptHtml ? stripTags(excerptHtml) : '',
        _html: excerptHtml || null, // pre-marked snippet from pagefind
      },
    };
  }

  function stripTags(html) {
    return String(html).replace(/<[^>]+>/g, '');
  }

  function makeAdapter(source) {
    if (!source || !source.type) throw new Error('AzulSearch: source.type required');
    if (source.type === 'api-default') return apiDefaultAdapter();
    if (source.type === 'api-index') return apiIndexAdapter(source.url);
    if (source.type === 'pagefind') return pagefindAdapter(source.url);
    throw new Error('AzulSearch: unknown source.type ' + source.type);
  }

  // ---------- DOM mount --------------------------------------------------

  // Build the HTML scaffold. Inserted into `host`.
  var TEMPLATE = ''
    + '<div class="azul-search" data-state="closed">'
    +   '<button class="azs-toggle" type="button" aria-label="Open search">'
    +     '<span class="azs-toggle-icon" aria-hidden="true">⌕</span>'
    +     '<span class="azs-toggle-label">Search API</span>'
    +     '<kbd class="azs-toggle-key">/</kbd>'
    +   '</button>'
    +   '<div class="azs-panel" role="dialog" aria-label="Search">'
    +     '<div class="azs-input-row">'
    +       '<input class="azs-input" type="search" autocomplete="off" '
    +         'spellcheck="false" placeholder="Search…" aria-label="Search query" />'
    +       '<button class="azs-close" type="button" aria-label="Close">×</button>'
    +     '</div>'
    +     '<div class="azs-meta" aria-live="polite"></div>'
    +     '<ul class="azs-results" role="listbox"></ul>'
    +   '</div>'
    + '</div>';

  function isMobile() {
    return window.matchMedia('(max-width: 1099px)').matches;
  }

  // Resolve an entry's anchor to a fully qualified href. On the api page
  // we want bare fragments so the browser does the in-page jump.
  function resolveHref(entry, ctx) {
    if (!entry || !entry.a) return '#';
    if (entry.a.indexOf('http') === 0 || entry.a.indexOf('/') === 0) {
      return entry.a; // Pagefind passes absolute URLs through
    }
    if (ctx.onApiPage) return '#' + entry.a;
    return ctx.apiPageUrl + '#' + entry.a;
  }

  function renderResults(ulEl, results, tokens, ctx, metaEl) {
    if (results.length === 0) {
      ulEl.innerHTML = '';
      metaEl.textContent = tokens.length === 0
        ? (ctx.entryCount ? ctx.entryCount + ' entries' : '')
        : 'No matches';
      return;
    }
    var html = '';
    for (var i = 0; i < results.length; i++) {
      var r = results[i];
      var e = r.entry;
      var kindMeta = KIND[e.k] || { label: e.k, cls: '' };
      var displayName = e.p ? (e.p + '::' + e.n) : e.n;
      var loc = e.m ? ('mod ' + e.m) : '';
      // Pagefind hands us a pre-marked HTML excerpt; trust it verbatim
      // (it's well-formed and already escaped by pagefind). Otherwise
      // build a snippet from plain doc text and run our own highlight().
      var snipHtml = e._html
        ? e._html
        : (e.d ? highlight(snippet(e.d, tokens), tokens) : '');
      var sigLine = e.s ? ('<code class="azs-sig">' + escapeHtml(e.s) + '</code>') : '';
      html += '<li class="azs-result" data-idx="' + i + '">'
        +   '<a href="' + escapeHtml(resolveHref(e, ctx)) + '">'
        +     '<span class="azs-kind ' + kindMeta.cls + '">' + kindMeta.label + '</span>'
        +     '<span class="azs-name">' + highlight(displayName, tokens) + '</span>'
        +     (loc ? '<span class="azs-loc">' + escapeHtml(loc) + '</span>' : '')
        +     (sigLine ? '<div>' + sigLine + '</div>' : '')
        +     (snipHtml ? '<div class="azs-snippet">' + snipHtml + '</div>' : '')
        +   '</a>'
        + '</li>';
    }
    ulEl.innerHTML = html;
    metaEl.textContent = results.length + (results.length === 50 ? '+ matches' : ' matches');
  }

  function mount(opts) {
    if (!opts || !opts.source) throw new Error('AzulSearch.mount: source required');

    var hostSel = opts.mount || opts.el;
    var host = typeof hostSel === 'string' ? document.querySelector(hostSel) : hostSel;
    if (!host) throw new Error('AzulSearch.mount: mount element not found');

    host.innerHTML = TEMPLATE;
    var root = host.querySelector('.azul-search');
    var panel = host.querySelector('.azs-panel');
    var input = host.querySelector('.azs-input');
    var resultsEl = host.querySelector('.azs-results');
    var metaEl = host.querySelector('.azs-meta');
    var toggleBtn = host.querySelector('.azs-toggle');
    var closeBtn = host.querySelector('.azs-close');

    var ctx = {
      apiPageUrl: opts.apiPageUrl || '',
      onApiPage: !!opts.onApiPage,
      entryCount: 0,
    };

    var adapter = makeAdapter(opts.source);
    var loaded = null;
    var loading = null;
    var lastQuery = '';
    var debounceTimer = 0;
    var selectedIdx = -1;

    function ensureLoaded() {
      if (loaded) return Promise.resolve(loaded);
      if (loading) return loading;
      metaEl.textContent = 'Loading index…';
      loading = adapter.load().then(function (l) {
        loaded = l;
        ctx.entryCount = l.count || 0;
        // Adapter may know the canonical api page URL (auto-discovery
        // path); use it if the caller didn't override.
        if (l.apiPageUrl && !opts.apiPageUrl) ctx.apiPageUrl = l.apiPageUrl;
        metaEl.textContent = (l.count ? l.count + ' entries' : 'Ready');
        return l;
      }).catch(function (err) {
        metaEl.textContent = 'Failed to load index';
        if (window.console) console.error(err);
        loading = null;
        throw err;
      });
      return loading;
    }

    function runQuery() {
      var q = input.value;
      lastQuery = q;
      if (!q) {
        renderResults(resultsEl, [], [], ctx, metaEl);
        selectedIdx = -1;
        return;
      }
      ensureLoaded().then(function (l) {
        return l.search(q, { limit: 50 }).then(function (results) {
          if (input.value !== q) return; // outdated
          renderResults(resultsEl, results, tokenise(q), ctx, metaEl);
          selectedIdx = results.length > 0 ? 0 : -1;
          updateSelection();
        });
      }, function () { /* error already shown in metaEl */ });
    }

    function updateSelection() {
      var items = resultsEl.querySelectorAll('.azs-result');
      for (var i = 0; i < items.length; i++) {
        if (i === selectedIdx) items[i].classList.add('selected');
        else items[i].classList.remove('selected');
      }
      var sel = items[selectedIdx];
      if (sel) sel.scrollIntoView({ block: 'nearest' });
    }

    function open() {
      root.dataset.state = 'open';
      // Defer focus so iOS Safari doesn't suppress the soft keyboard.
      setTimeout(function () { input.focus(); input.select(); }, 0);
      ensureLoaded().then(prefetchTarget, function () { /* ignore */ });
    }

    // Prefetch the page that results will navigate to. The api page is
    // ~1.4 MB so a cold click feels slow; queueing a background fetch as
    // soon as the user opens the panel masks the latency entirely. Only
    // fires once and only when the adapter knows the target.
    var prefetched = false;
    function prefetchTarget() {
      if (prefetched) return;
      var target = ctx.apiPageUrl;
      if (!target) return;
      prefetched = true;
      // Skip if we're already on that page.
      try {
        var here = window.location.pathname;
        var t = target.split('#')[0];
        if (here === t || here.endsWith(t)) return;
      } catch (e) { /* ignore */ }
      var link = document.createElement('link');
      link.rel = 'prefetch';
      link.href = target;
      link.as = 'document';
      document.head.appendChild(link);
    }
    function close() {
      root.dataset.state = 'closed';
      input.blur();
    }

    toggleBtn.addEventListener('click', function () {
      if (root.dataset.state === 'open') close();
      else open();
    });
    closeBtn.addEventListener('click', close);

    input.addEventListener('input', function () {
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(runQuery, 70);
    });

    input.addEventListener('keydown', function (ev) {
      var items = resultsEl.querySelectorAll('.azs-result');
      if (ev.key === 'ArrowDown') {
        ev.preventDefault();
        if (items.length === 0) return;
        selectedIdx = Math.min(items.length - 1, selectedIdx + 1);
        updateSelection();
      } else if (ev.key === 'ArrowUp') {
        ev.preventDefault();
        if (items.length === 0) return;
        selectedIdx = Math.max(0, selectedIdx - 1);
        updateSelection();
      } else if (ev.key === 'Enter') {
        var link = items[selectedIdx] && items[selectedIdx].querySelector('a');
        if (link) {
          ev.preventDefault();
          link.click();
          close();
        }
      } else if (ev.key === 'Escape') {
        ev.preventDefault();
        if (input.value) { input.value = ''; runQuery(); }
        else close();
      }
    });

    // Click-outside on mobile (full-screen overlay): close when tapping
    // backdrop. Desktop panel doesn't have a backdrop.
    panel.addEventListener('click', function (ev) {
      if (ev.target === panel) close();
    });

    // Global hotkeys.
    var hotkey = opts.hotkey !== false;
    function onGlobalKey(ev) {
      if (!hotkey) return;
      if (ev.target && /^(INPUT|TEXTAREA|SELECT)$/.test(ev.target.tagName)) return;
      if (ev.target && ev.target.isContentEditable) return;
      if (ev.key === '/' && !ev.ctrlKey && !ev.metaKey && !ev.altKey) {
        ev.preventDefault();
        open();
      } else if ((ev.key === 'k' || ev.key === 'K') && (ev.ctrlKey || ev.metaKey)) {
        ev.preventDefault();
        open();
      }
    }
    document.addEventListener('keydown', onGlobalKey);

    return {
      open: open,
      close: close,
      destroy: function () {
        document.removeEventListener('keydown', onGlobalKey);
        host.innerHTML = '';
      },
      search: function (q) { input.value = q; runQuery(); },
    };
  }

  // Auto-attach: create our own container appended to <body> if the page
  // didn't provide one.
  function attach(opts) {
    opts = opts || {};
    var host = document.createElement('div');
    host.className = 'azul-search-host';
    document.body.appendChild(host);
    return mount(Object.assign({}, opts, { mount: host }));
  }

  // ---------- public API -------------------------------------------------

  window.AzulSearch = {
    search: search,
    mount: mount,
    attach: attach,
  };
})();
