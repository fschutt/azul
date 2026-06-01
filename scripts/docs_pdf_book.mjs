// Plan the documentation "book" for PDF rendering.
//
// Reads the guide source frontmatter (title / guide_order / audience), buckets
// every page into the three trees the website uses (see guide.rs classify_tree)
// — Getting Started, Advanced, Contributor — orders each, generates a cover
// page + a per-book title+index(tree) page into <deploy>/_pdf/, and prints the
// final ordered list of URLs (one per line) to stdout for the CDP renderer:
//   cover, book1-index, book1 pages…, book2-index, book2 pages…, book3-index, …
//
// Usage: node docs_pdf_book.mjs <deployDir> <baseUrl> <guideSrcDir> [version]
import { readdirSync, readFileSync, writeFileSync, mkdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const [deployDir, baseUrl, guideSrc, version = '0.2.0'] = process.argv.slice(2);

// --- collect guide pages + frontmatter ------------------------------------
function walk(dir, base = '') {
  const out = [];
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    const rel = base ? `${base}/${e}` : e;
    if (statSync(p).isDirectory()) out.push(...walk(p, rel));
    else if (e.endsWith('.md')) out.push({ path: p, fileName: rel.slice(0, -3) });
  }
  return out;
}
function frontmatter(src) {
  const m = src.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  const fm = {};
  if (m) for (const line of m[1].split(/\r?\n/)) {
    const i = line.indexOf(':');
    if (i > 0) fm[line.slice(0, i).trim()] = line.slice(i + 1).trim();
  }
  return fm;
}
const pages = walk(guideSrc).map((p) => {
  const fm = frontmatter(readFileSync(p.path, 'utf8'));
  const order = fm.guide_order != null && fm.guide_order !== '' ? parseInt(fm.guide_order, 10) : null;
  return {
    fileName: p.fileName,
    title: fm.title || p.fileName,
    order: Number.isNaN(order) ? null : order,
    audience: fm.audience || null,
  };
});

// --- bucket into the three trees (mirror guide.rs classify_tree) ----------
function tree(g) {
  if (g.audience === 'contributor') return 2;
  if (g.order != null && g.order >= 200) return 1;       // advanced
  if (g.order == null) return g.fileName.startsWith('internals/') ? 2 : 1;
  return 0;                                              // getting-started (10–199)
}
const books = [
  { key: 'getting-started', num: 'Book I', title: 'Getting Started', sub: 'Build your first azul applications — the essentials.', pages: [] },
  { key: 'advanced', num: 'Book II', title: 'Advanced', sub: 'Deeper features: media, networking, deployment, performance.', pages: [] },
  { key: 'contributor', num: 'Book III', title: 'Contributor Guide', sub: 'Internals — how azul works under the hood.', pages: [] },
];
for (const g of pages) books[tree(g)].pages.push(g);
const cmp = (a, b) => (a.order ?? 1e9) - (b.order ?? 1e9) || a.fileName.localeCompare(b.fileName);
for (const b of books) b.pages.sort(cmp);

// --- front-matter HTML ----------------------------------------------------
const esc = (s) => String(s).replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c]));
const CSS = `
  @page { margin: 1.6cm; }
  html,body { font-family: 'Instrument Serif', Georgia, serif; color: #1a1a1a; }
  .cover { display:flex; flex-direction:column; justify-content:center; align-items:center;
           text-align:center; min-height: 24cm; }
  .cover h1 { font-size: 4em; margin: 0 0 .2em; letter-spacing:.02em; }
  .cover .sub { font-size: 1.6em; color:#444; }
  .cover .meta { margin-top: 3em; color:#888; font-size:1em; font-family: -apple-system, sans-serif; }
  .booktitle { min-height: 22cm; display:flex; flex-direction:column; justify-content:center; }
  .booktitle .num { font-family:-apple-system,sans-serif; letter-spacing:.3em; text-transform:uppercase;
                    color:#b00; font-size:.9em; }
  .booktitle h1 { font-size: 3em; margin:.1em 0 .3em; }
  .booktitle .sub { color:#555; font-size:1.3em; max-width: 18cm; }
  /* Each book's Contents starts on its own page (the .booktitle above fills the
     title page; without this the TOC overflows onto the bottom of it). */
  .toc { break-before: page; page-break-before: always; margin-top: 0; font-family: -apple-system, 'Segoe UI', sans-serif; font-size: 11pt; }
  .toc h2 { font-family:'Instrument Serif',serif; font-size:1.6em; border-bottom:1px solid #ddd; padding-bottom:.2em; }
  .toc ul { list-style:none; padding-left:0; }
  .toc li { margin:.28em 0; }
  .toc .d1 { padding-left: 1.4em; color:#333; }
  .toc .d2 { padding-left: 2.8em; color:#666; font-size:.95em; }
  .toc a { color: inherit; text-decoration:none; }
`;
const fontLink = `<link href="https://fonts.googleapis.com/css2?family=Instrument+Serif&display=swap" rel="stylesheet">`;
function pageHtml(title, body) {
  return `<!DOCTYPE html><html lang=en><head><meta charset=utf-8><title>${esc(title)}</title>${fontLink}<style>${CSS}</style></head><body>${body}</body></html>`;
}
mkdirSync(join(deployDir, '_pdf'), { recursive: true });
const today = new Date().toISOString().slice(0, 10);

writeFileSync(join(deployDir, '_pdf', 'cover.html'), pageHtml('Azul Documentation',
  `<div class=cover><h1>Azul</h1><div class=sub>GUI Framework — Documentation</div>
   <div class=meta>Native desktop &amp; web UI for Rust and 10+ language bindings<br>
   Version ${esc(version)} · ${today}</div></div>`));

// `urls` (one rendered NNNN.pdf each) and `outline` stay index-aligned: the CDP
// renderer writes NNNN.pdf in URL order, so the merge step can attach a PDF
// bookmark to the first physical page of each source PDF. level 0 = top-level
// (cover + each book title), level 1 = a page nested under its book.
const urls = [`${baseUrl}/_pdf/cover.html`];
const outline = [{ title: 'Azul Documentation', level: 0 }];
books.forEach((b, i) => {
  const items = b.pages.map((g) => {
    const depth = (g.fileName.match(/\//g) || []).length;       // 0 = top level
    return `<li class="d${depth}"><a href="${baseUrl}/ui/guide/${g.fileName}.html">${esc(g.title)}</a></li>`;
  }).join('\n');
  writeFileSync(join(deployDir, '_pdf', `book-${i}.html`), pageHtml(b.title,
    `<div class=booktitle><div class=num>${b.num}</div><h1>${esc(b.title)}</h1><div class=sub>${esc(b.sub)}</div></div>
     <div class=toc><h2>Contents</h2><ul>${items}</ul></div>`));
  urls.push(`${baseUrl}/_pdf/book-${i}.html`);
  outline.push({ title: `${b.num}: ${b.title}`, level: 0 });
  for (const g of b.pages) {
    urls.push(`${baseUrl}/ui/guide/${g.fileName}.html`);
    outline.push({ title: g.title, level: 1 });
  }
});

// Outline manifest for the merge step's pypdf bookmark builder (index-aligned to
// the sorted NNNN.pdf files == this url order).
writeFileSync(join(deployDir, '_pdf', 'outline.json'), JSON.stringify(outline));

process.stderr.write(`books: ${books.map((b) => `${b.key}=${b.pages.length}`).join(' ')} · ${urls.length} pages total\n`);
process.stdout.write(urls.join('\n') + '\n');
