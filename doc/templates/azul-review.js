/* azul-review.js — TEMPORARY in-page documentation review tool.
 *
 * Injected on every azul.rs page (see docgen::get_common_head_tags) but HIDDEN
 * by default, so normal visitors of the live site never see it. Activate with a
 * secret gesture (type "azrev", or load a URL with #azrev); the state persists
 * in localStorage so a multi-page review survives link clicks. Toggle it off
 * (header ✕, or type "azrev" again) to hide everything and preview the final
 * shipped product.
 *
 * Two review modes, both persisted to IndexedDB and merged into one "Export all"
 * JSON to hand an AI:
 *   1. COMMENT  — select text → popup → note on that quoted selection.
 *   2. EDIT (WYSIWYG) — toggle "Edit" to make the prose blocks contenteditable;
 *      type directly into the live page. Each changed block is recorded as
 *      original → edited (+ heading context), upserted instantly, so the direct
 *      edits can be transferred to the backing .md files.
 *
 * Review-phase only — meant to be REMOVED later (drop the <script> tag in
 * get_common_head_tags + this file). Stores nothing server-side, no network.
 *
 * Storage (IndexedDB db "azul-review", v2):
 *   store "comments" (keyPath id): { id, page, title, quote, prefix, suffix, comment, ts }
 *   store "edits"    (keyPath key): { key, page, title, url, tag, heading, orig, edited, ts }
 *     - page : location.pathname (stable across azul.rs/<path> vs .../<path>.html)
 *     - key  : page + '#' + block-index (so re-editing a block upserts in place)
 *     - orig/edited : block text before/after the WYSIWYG change
 */
(function () {
  'use strict';
  if (window.__azulReviewLoaded) return;
  window.__azulReviewLoaded = true;

  // The tool is HIDDEN by default so normal visitors of the live azul.rs site
  // never see it. Activate it with a secret no-normal-user-knows gesture:
  //   • type the sequence  a z r e v  (quickly, when not in a text field), or
  //   • load any page with  #azrev  in the URL, or
  //   • it auto-restores if you activated it earlier this browser (localStorage).
  // Toggling it OFF hides the panel, all comment marks and edit outlines, and
  // exits edit mode — i.e. simulates the final shipped product. State persists
  // across page navigations so a multi-page review survives clicking links.
  var ACTIVE_KEY = 'azr-active';
  var SECRET = 'azrev';
  var active = false;

  var DB_NAME = 'azul-review';
  var STORE = 'comments';
  var EDITS = 'edits';        // WYSIWYG direct-edit records (one per edited block)
  var CTX = 40; // chars of context captured on each side of the selection

  // ---- IndexedDB (promise wrappers) ------------------------------------
  function openDb() {
    return new Promise(function (resolve, reject) {
      // v2 adds the `edits` store (keyPath `key` = stable per-block id, so an
      // edit to the same block upserts in place instead of piling up rows).
      var req = indexedDB.open(DB_NAME, 2);
      req.onupgradeneeded = function (ev) {
        var db = req.result;
        if (!db.objectStoreNames.contains(STORE)) {
          var os = db.createObjectStore(STORE, { keyPath: 'id', autoIncrement: true });
          os.createIndex('page', 'page', { unique: false });
        }
        if (!db.objectStoreNames.contains(EDITS)) {
          var es = db.createObjectStore(EDITS, { keyPath: 'key' });
          es.createIndex('page', 'page', { unique: false });
        }
      };
      req.onsuccess = function () { resolve(req.result); };
      req.onerror = function () { reject(req.error); };
    });
  }
  function tx(mode, fn) {
    return openDb().then(function (db) {
      return new Promise(function (resolve, reject) {
        var t = db.transaction(STORE, mode);
        var os = t.objectStore(STORE);
        var out = fn(os);
        t.oncomplete = function () { resolve(out && out.result !== undefined ? out.result : out); };
        t.onerror = function () { reject(t.error); };
      });
    });
  }
  function addComment(rec) { return tx('readwrite', function (os) { return os.add(rec); }); }
  function deleteComment(id) { return tx('readwrite', function (os) { return os.delete(id); }); }
  // Read every stored comment (resolves the getAll request explicitly).
  function getAll() {
    return openDb().then(function (db) {
      return new Promise(function (resolve, reject) {
        var req = db.transaction(STORE, 'readonly').objectStore(STORE).getAll();
        req.onsuccess = function () { resolve(req.result || []); };
        req.onerror = function () { reject(req.error); };
      });
    });
  }

  // ---- edits store (upsert by stable block key) ------------------------
  function putEdit(rec) {
    return openDb().then(function (db) {
      return new Promise(function (resolve, reject) {
        var t = db.transaction(EDITS, 'readwrite');
        t.objectStore(EDITS).put(rec);
        t.oncomplete = function () { resolve(); };
        t.onerror = function () { reject(t.error); };
      });
    });
  }
  function deleteEdit(key) {
    return openDb().then(function (db) {
      return new Promise(function (resolve, reject) {
        var t = db.transaction(EDITS, 'readwrite');
        t.objectStore(EDITS).delete(key);
        t.oncomplete = function () { resolve(); };
        t.onerror = function () { reject(t.error); };
      });
    });
  }
  function getAllEdits() {
    return openDb().then(function (db) {
      return new Promise(function (resolve, reject) {
        var req = db.transaction(EDITS, 'readonly').objectStore(EDITS).getAll();
        req.onsuccess = function () { resolve(req.result || []); };
        req.onerror = function () { reject(req.error); };
      });
    });
  }

  // ---- Styles ----------------------------------------------------------
  var css = ''
    + '.azr-hl{background:#fff3a3;outline:1px solid #e0c200;cursor:pointer;}'
    + '.azr-panel{position:fixed;right:14px;bottom:14px;z-index:2147483600;'
    + 'font:13px/1.4 -apple-system,Segoe UI,Roboto,sans-serif;background:#1e1e24;'
    + 'color:#eee;border:1px solid #444;border-radius:8px;box-shadow:0 4px 18px rgba(0,0,0,.4);'
    + 'width:300px;max-height:60vh;display:flex;flex-direction:column;overflow:hidden;}'
    + '.azr-panel h4{margin:0;padding:8px 10px;background:#2b2b34;font-size:13px;'
    + 'display:flex;justify-content:space-between;align-items:center;}'
    + '.azr-panel .azr-list{overflow:auto;padding:4px 0;flex:1;}'
    + '.azr-item{padding:6px 10px;border-bottom:1px solid #333;cursor:pointer;}'
    + '.azr-item:hover{background:#2b2b34;}'
    + '.azr-item .q{color:#9ad19a;font-style:italic;}'
    + '.azr-item .c{color:#ddd;}'
    + '.azr-item .x{float:right;color:#e07a7a;cursor:pointer;padding-left:6px;}'
    + '.azr-bar{padding:6px 10px;background:#2b2b34;display:flex;gap:6px;border-top:1px solid #444;}'
    + '.azr-bar button{flex:1;cursor:pointer;background:#3a3a45;color:#eee;border:1px solid #555;'
    + 'border-radius:4px;padding:4px;font-size:12px;}'
    + '.azr-bar button:hover{background:#474755;}'
    + '.azr-pop{position:absolute;z-index:2147483601;background:#1e1e24;color:#eee;'
    + 'border:1px solid #555;border-radius:6px;padding:8px;width:260px;'
    + 'box-shadow:0 4px 18px rgba(0,0,0,.4);}'
    + '.azr-pop textarea{width:100%;box-sizing:border-box;height:64px;background:#15151a;'
    + 'color:#eee;border:1px solid #555;border-radius:4px;resize:vertical;font:13px sans-serif;}'
    + '.azr-pop .row{display:flex;gap:6px;margin-top:6px;}'
    + '.azr-pop button{flex:1;cursor:pointer;background:#3a3a45;color:#eee;border:1px solid #555;'
    + 'border-radius:4px;padding:4px;}'
    + '.azr-min{cursor:pointer;color:#aaa;}'
    // WYSIWYG edit mode: subtle outline on editable blocks, stronger on changed.
    + '.azr-editing [contenteditable="true"]{outline:1px dashed rgba(90,160,255,.5);outline-offset:2px;}'
    + '.azr-editing [contenteditable="true"]:focus{outline:2px solid #5aa0ff;background:rgba(90,160,255,.06);}'
    + '.azr-edited{outline:2px solid #46c46a !important;background:rgba(70,196,106,.08) !important;}'
    + '.azr-toggle.azr-on{background:#2a6;border-color:#3c8;color:#fff;}'
    + '.azr-item .e{color:#7fc4ff;}'
    + '.azr-item .arrow{color:#888;}';
  var styleEl = document.createElement('style');
  styleEl.textContent = css;
  document.head.appendChild(styleEl);

  // ---- Panel -----------------------------------------------------------
  var panel = document.createElement('div');
  panel.className = 'azr-panel';
  panel.innerHTML =
    '<h4><span>📝 Doc review (<span class="azr-count">0</span>)</span>'
    + '<span><span class="azr-min" title="minimize">—</span>'
    + '<span class="azr-hide" title="hide — simulate final site (type azrev to bring back)" style="cursor:pointer;color:#aaa;padding-left:10px;">✕</span></span></h4>'
    + '<div class="azr-list"></div>'
    + '<div class="azr-bar">'
    + '<button class="azr-toggle" title="Toggle WYSIWYG edit mode: edit the page text directly; changes are recorded per block">✏️ Edit: off</button>'
    + '</div>'
    + '<div class="azr-bar">'
    + '<button class="azr-export">Export all</button>'
    + '<button class="azr-clear">Clear page</button>'
    + '</div>';
  panel.style.display = 'none'; // hidden until activated by the secret gesture
  document.body.appendChild(panel);
  var listEl = panel.querySelector('.azr-list');
  var countEl = panel.querySelector('.azr-count');
  var bodyHidden = false;
  panel.querySelector('.azr-min').addEventListener('click', function () {
    bodyHidden = !bodyHidden;
    listEl.style.display = bodyHidden ? 'none' : '';
    panel.querySelector('.azr-bar').style.display = bodyHidden ? 'none' : '';
  });

  function page() { return location.pathname.replace(/\.html$/, ''); }

  var editMode = false;

  function refresh() {
    Promise.all([getAll(), getAllEdits()]).then(function (res) {
      var all = res[0], edits = res[1];
      countEl.textContent = (all.length + edits.length);
      var mine = all.filter(function (r) { return r.page === page(); });
      var myEdits = edits.filter(function (r) { return r.page === page(); });
      listEl.innerHTML = '';
      // Direct edits first (the WYSIWYG changes), then comments.
      myEdits.sort(function (a, b) { return a.ts - b.ts; });
      myEdits.forEach(function (r) {
        var d = document.createElement('div');
        d.className = 'azr-item';
        d.innerHTML = '<span class="x" title="discard edit">✕</span>'
          + '<div class="e">✏️ <span class="o"></span> <span class="arrow">→</span> <span class="n"></span></div>';
        d.querySelector('.o').textContent = '“' + trunc(r.orig, 40) + '”';
        d.querySelector('.n').textContent = '“' + trunc(r.edited, 40) + '”';
        d.addEventListener('click', function () { scrollToQuote(r.orig); });
        d.querySelector('.x').addEventListener('click', function (e) {
          e.stopPropagation();
          // discard: restore the live block text + delete the record
          var el = document.querySelector('[data-azr-key="' + cssEsc(r.key) + '"]');
          if (el) { el.textContent = r.orig; el.classList.remove('azr-edited'); }
          deleteEdit(r.key).then(refresh);
        });
        listEl.appendChild(d);
      });
      mine.sort(function (a, b) { return a.ts - b.ts; });
      mine.forEach(function (r) {
        var d = document.createElement('div');
        d.className = 'azr-item';
        d.innerHTML = '<span class="x" title="delete">✕</span>'
          + '<div class="q"></div><div class="c"></div>';
        d.querySelector('.q').textContent = '“' + trunc(r.quote, 70) + '”';
        d.querySelector('.c').textContent = r.comment;
        d.addEventListener('click', function () { scrollToQuote(r.quote); });
        d.querySelector('.x').addEventListener('click', function (e) {
          e.stopPropagation();
          deleteComment(r.id).then(refresh);
        });
        listEl.appendChild(d);
      });
    });
  }
  function trunc(s, n) { return s.length > n ? s.slice(0, n) + '…' : s; }
  function cssEsc(s) { return String(s).replace(/["\\]/g, '\\$&'); }

  // ---- WYSIWYG edit mode ----------------------------------------------
  // Toggling makes the prose blocks in the main content directly editable; each
  // changed block is recorded (original text → edited text + heading context)
  // and upserted to IndexedDB instantly, so direct edits to the "live product"
  // can later be transferred to the backing .md files. Code blocks, nav and the
  // review panel are left alone.
  var EDITABLE = 'p,li,h1,h2,h3,h4,h5,h6,blockquote,td,th,dd,dt,figcaption';

  function contentRoot() {
    return document.querySelector('main, article, .md-content, .content, #content, .guide, .body-content')
      || document.body;
  }
  function editableBlocks() {
    var root = contentRoot();
    return Array.prototype.filter.call(root.querySelectorAll(EDITABLE), function (el) {
      if (panel.contains(el)) return false;
      if (el.closest('pre, code, nav, header, footer, .azr-panel, .azr-pop')) return false;
      // skip blocks that merely wrap other editable blocks (edit the leaf)
      if (el.querySelector(EDITABLE)) return false;
      return el.textContent.trim().length > 0;
    });
  }
  // A stable per-block key: page + heading context + index among same-text blocks.
  function blockKey(el, idx) { return page() + '#' + idx; }
  function headingContext(el) {
    var h = el;
    while (h) {
      var prev = h.previousElementSibling;
      while (prev) {
        if (/^H[1-6]$/.test(prev.tagName)) return prev.textContent.trim();
        prev = prev.previousElementSibling;
      }
      h = h.parentElement;
      if (h === document.body) break;
    }
    return '';
  }

  var debounceTimers = {};
  function recordEdit(el) {
    var key = el.getAttribute('data-azr-key');
    var orig = el.getAttribute('data-azr-orig');
    var edited = el.textContent;
    clearTimeout(debounceTimers[key]);
    debounceTimers[key] = setTimeout(function () {
      if (edited === orig) {
        el.classList.remove('azr-edited');
        deleteEdit(key).then(refresh);
        return;
      }
      el.classList.add('azr-edited');
      putEdit({
        key: key,
        page: page(),
        title: document.title,
        url: location.href,
        tag: el.tagName.toLowerCase(),
        heading: headingContext(el),
        orig: orig,
        edited: edited,
        ts: Date.now()
      }).then(refresh);
    }, 400);
  }

  function enableEdit() {
    var blocks = editableBlocks();
    blocks.forEach(function (el, i) {
      var key = blockKey(el, i);
      el.setAttribute('data-azr-key', key);
      if (el.getAttribute('data-azr-orig') === null) {
        el.setAttribute('data-azr-orig', el.textContent);
      }
      el.setAttribute('contenteditable', 'true');
      el.setAttribute('spellcheck', 'false');
      if (!el.__azrBound) {
        el.addEventListener('input', function () { recordEdit(el); });
        el.__azrBound = true;
      }
    });
    // Re-flag blocks that already have a saved edit (e.g. after reload).
    getAllEdits().then(function (edits) {
      var byKey = {};
      edits.forEach(function (e) { byKey[e.key] = e; });
      blocks.forEach(function (el) {
        var rec = byKey[el.getAttribute('data-azr-key')];
        if (rec) { el.textContent = rec.edited; el.classList.add('azr-edited'); }
      });
    });
    document.body.classList.add('azr-editing');
  }
  function disableEdit() {
    editableBlocks().forEach(function (el) {
      el.removeAttribute('contenteditable');
    });
    document.body.classList.remove('azr-editing');
  }

  var toggleBtn = panel.querySelector('.azr-toggle');
  toggleBtn.addEventListener('click', function () {
    editMode = !editMode;
    toggleBtn.textContent = editMode ? '✏️ Edit: ON' : '✏️ Edit: off';
    toggleBtn.classList.toggle('azr-on', editMode);
    if (editMode) { enableEdit(); } else { disableEdit(); }
  });

  // ---- Selection → comment popup --------------------------------------
  var pop = null;
  function closePop() { if (pop) { pop.remove(); pop = null; } }
  document.addEventListener('mousedown', function (e) {
    if (pop && !pop.contains(e.target)) closePop();
  });

  document.addEventListener('mouseup', function (e) {
    if (!active) return;  // tool dormant until activated
    if (editMode) return; // in WYSIWYG mode a selection is for editing, not commenting
    if (pop && pop.contains(e.target)) return;
    if (panel.contains(e.target)) return;
    var sel = window.getSelection();
    var text = sel && sel.toString().trim();
    if (!text || text.length < 2) return;
    var range = sel.getRangeAt(0);
    var ctx = surroundingContext(range);
    showPopup(e.pageX, e.pageY, text, ctx, range);
  });

  function surroundingContext(range) {
    var prefix = '', suffix = '';
    try {
      var pre = range.cloneRange();
      pre.collapse(true);
      pre.setStart(range.startContainer.ownerDocument.body, 0);
      prefix = pre.toString().slice(-CTX);
    } catch (e) {}
    try {
      var node = range.endContainer;
      var rest = (node.textContent || '').slice(range.endOffset);
      suffix = rest.slice(0, CTX);
    } catch (e) {}
    return { prefix: prefix, suffix: suffix };
  }

  function showPopup(x, y, quote, ctx, range) {
    closePop();
    pop = document.createElement('div');
    pop.className = 'azr-pop';
    pop.style.left = Math.min(x, window.scrollX + window.innerWidth - 280) + 'px';
    pop.style.top = (y + 8) + 'px';
    pop.innerHTML = '<textarea placeholder="Comment on the selection…"></textarea>'
      + '<div class="row"><button class="azr-save">Save</button>'
      + '<button class="azr-cancel">Cancel</button></div>';
    document.body.appendChild(pop);
    var ta = pop.querySelector('textarea');
    ta.focus();
    pop.querySelector('.azr-cancel').addEventListener('click', closePop);
    function save() {
      var comment = ta.value.trim();
      if (!comment) { closePop(); return; }
      highlight(range);
      addComment({
        page: page(),
        title: document.title,
        url: location.href,
        quote: quote,
        prefix: ctx.prefix,
        suffix: ctx.suffix,
        comment: comment,
        ts: Date.now()
      }).then(function () { closePop(); refresh(); });
    }
    pop.querySelector('.azr-save').addEventListener('click', save);
    ta.addEventListener('keydown', function (ev) {
      if (ev.key === 'Enter' && (ev.metaKey || ev.ctrlKey)) save();
      if (ev.key === 'Escape') closePop();
    });
  }

  function highlight(range) {
    try {
      var mark = document.createElement('mark');
      mark.className = 'azr-hl';
      range.surroundContents(mark);
    } catch (e) {
      // Range spans element boundaries — skip the inline highlight, the
      // comment is still saved and listed in the panel.
    }
  }

  function scrollToQuote(quote) {
    var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, null);
    var n;
    while ((n = walker.nextNode())) {
      if (n.nodeValue && n.nodeValue.indexOf(quote.slice(0, 40)) !== -1) {
        var el = n.parentElement;
        if (el) {
          el.scrollIntoView({ behavior: 'smooth', block: 'center' });
          var old = el.style.transition;
          el.style.transition = 'background .3s';
          el.style.background = '#fff3a3';
          setTimeout(function () { el.style.background = ''; el.style.transition = old; }, 1200);
        }
        return;
      }
    }
  }

  // ---- Export / clear --------------------------------------------------
  panel.querySelector('.azr-export').addEventListener('click', function () {
    Promise.all([getAll(), getAllEdits()]).then(function (res) {
      var comments = res[0], edits = res[1];
      var pages = {};
      function bucket(p) {
        return pages[p] || (pages[p] = { comments: [], edits: [] });
      }
      comments.forEach(function (r) { bucket(r.page).comments.push(r); });
      edits.forEach(function (r) { bucket(r.page).edits.push(r); });
      var out = {
        exported_at: new Date().toISOString(),
        site: location.origin,
        total_comments: comments.length,
        total_edits: edits.length,
        note: 'edits[] are direct WYSIWYG changes (orig → edited) to apply to the backing .md; comments[] are notes on a quoted selection.',
        pages: pages
      };
      var blob = new Blob([JSON.stringify(out, null, 2)], { type: 'application/json' });
      var a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = 'azul-review-export-' + new Date().toISOString().slice(0, 10) + '.json';
      a.click();
      URL.revokeObjectURL(a.href);
    });
  });
  panel.querySelector('.azr-clear').addEventListener('click', function () {
    if (!confirm('Delete all comments AND edits for THIS page?')) return;
    Promise.all([getAll(), getAllEdits()]).then(function (res) {
      var mine = res[0].filter(function (r) { return r.page === page(); });
      var myEdits = res[1].filter(function (r) { return r.page === page(); });
      // Restore any live edited blocks to their original text.
      myEdits.forEach(function (r) {
        var el = document.querySelector('[data-azr-key="' + cssEsc(r.key) + '"]');
        if (el) { el.textContent = r.orig; el.classList.remove('azr-edited'); }
      });
      Promise.all(
        mine.map(function (r) { return deleteComment(r.id); })
          .concat(myEdits.map(function (r) { return deleteEdit(r.key); }))
      ).then(refresh);
    });
  });

  // ---- activation (secret gesture / URL hash / persisted state) --------
  function activate() {
    if (active) return;
    active = true;
    panel.style.display = '';
    try { localStorage.setItem(ACTIVE_KEY, '1'); } catch (e) {}
    refresh();
  }
  function deactivate() {
    if (editMode) {
      editMode = false;
      toggleBtn.textContent = '✏️ Edit: off';
      toggleBtn.classList.remove('azr-on');
      disableEdit();
    }
    closePop();
    active = false;
    panel.style.display = 'none';
    try { localStorage.removeItem(ACTIVE_KEY); } catch (e) {}
  }
  function toggleActive() { if (active) deactivate(); else activate(); }

  panel.querySelector('.azr-hide').addEventListener('click', deactivate);

  // Secret typed sequence (a-z-r-e-v) — ignored while typing in a field/edit.
  var typed = '';
  var typedTimer = null;
  document.addEventListener('keydown', function (e) {
    var t = e.target;
    var inField = t && (t.isContentEditable ||
      /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName || ''));
    if (inField) return;
    if (e.key && e.key.length === 1) {
      typed = (typed + e.key.toLowerCase()).slice(-SECRET.length);
      clearTimeout(typedTimer);
      typedTimer = setTimeout(function () { typed = ''; }, 1500);
      if (typed === SECRET) { typed = ''; toggleActive(); }
    }
  });

  // Bookmarkable activation on the live site: any URL containing #azrev.
  function checkHash() {
    if ((location.hash || '').toLowerCase().indexOf('azrev') !== -1) activate();
  }
  window.addEventListener('hashchange', checkHash);

  // Restore a prior session so a multi-page review survives link clicks.
  var wasActive = false;
  try { wasActive = localStorage.getItem(ACTIVE_KEY) === '1'; } catch (e) {}
  checkHash();
  if (wasActive) activate();
})();
