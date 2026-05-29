/* azul-review.js — TEMPORARY in-page documentation review tool.
 *
 * Injected on every azul.rs page (see docgen::get_common_head_tags). Lets the
 * maintainer select text in the guide / API docs, attach a comment to that
 * selection, and persist it to the browser's IndexedDB. An "Export all" button
 * dumps every comment across every page as one JSON file to hand to an AI for
 * guided fixes.
 *
 * This is a review-phase tool and is meant to be REMOVED in a later release
 * (drop the <script> tag in get_common_head_tags + this file). It stores
 * nothing server-side and makes no network calls.
 *
 * Storage model (IndexedDB db "azul-review", store "comments", keyPath "id"):
 *   { id, page, title, quote, prefix, suffix, comment, ts }
 *   - page   : location.pathname (stable across azul.rs/<path> vs .../<path>.html)
 *   - quote  : the exact selected text
 *   - prefix/suffix : ~40 chars of surrounding text, so a human/AI can locate
 *                     the quote even if it appears more than once on the page
 */
(function () {
  'use strict';
  if (window.__azulReviewLoaded) return;
  window.__azulReviewLoaded = true;

  var DB_NAME = 'azul-review';
  var STORE = 'comments';
  var CTX = 40; // chars of context captured on each side of the selection

  // ---- IndexedDB (promise wrappers) ------------------------------------
  function openDb() {
    return new Promise(function (resolve, reject) {
      var req = indexedDB.open(DB_NAME, 1);
      req.onupgradeneeded = function () {
        var db = req.result;
        if (!db.objectStoreNames.contains(STORE)) {
          var os = db.createObjectStore(STORE, { keyPath: 'id', autoIncrement: true });
          os.createIndex('page', 'page', { unique: false });
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
    + '.azr-min{cursor:pointer;color:#aaa;}';
  var styleEl = document.createElement('style');
  styleEl.textContent = css;
  document.head.appendChild(styleEl);

  // ---- Panel -----------------------------------------------------------
  var panel = document.createElement('div');
  panel.className = 'azr-panel';
  panel.innerHTML =
    '<h4><span>📝 Doc review (<span class="azr-count">0</span>)</span>'
    + '<span class="azr-min" title="minimize">—</span></h4>'
    + '<div class="azr-list"></div>'
    + '<div class="azr-bar">'
    + '<button class="azr-export">Export all</button>'
    + '<button class="azr-clear">Clear page</button>'
    + '</div>';
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

  function refresh() {
    getAll().then(function (all) {
      countEl.textContent = all.length;
      var mine = all.filter(function (r) { return r.page === page(); });
      listEl.innerHTML = '';
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

  // ---- Selection → comment popup --------------------------------------
  var pop = null;
  function closePop() { if (pop) { pop.remove(); pop = null; } }
  document.addEventListener('mousedown', function (e) {
    if (pop && !pop.contains(e.target)) closePop();
  });

  document.addEventListener('mouseup', function (e) {
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
    getAll().then(function (all) {
      var byPage = {};
      all.forEach(function (r) { (byPage[r.page] = byPage[r.page] || []).push(r); });
      var out = {
        exported_at: new Date().toISOString(),
        site: location.origin,
        total: all.length,
        pages: byPage
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
    if (!confirm('Delete all comments for THIS page?')) return;
    getAll().then(function (all) {
      var mine = all.filter(function (r) { return r.page === page(); });
      Promise.all(mine.map(function (r) { return deleteComment(r.id); })).then(refresh);
    });
  });

  refresh();
})();
