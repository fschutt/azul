// CDP-driven HTML→PDF renderer — mirrors doc/src/reftest/autodebug.rs ChromeCdp.
//
// Launches ONE persistent headless Chrome with remote debugging, connects over
// the CDP WebSocket (Node 18+/25 has a built-in global WebSocket — no `ws`
// package), and for each URL does Page.navigate -> wait load -> Page.printToPDF.
// This avoids the per-process `--print-to-pdf` hang that deadlocks Chrome
// 148 headless=new. Writes <outdir>/NNNN.pdf per page.
//
// Usage: node docs_pdf_cdp.mjs <chrome> <outdir> <url1> <url2> ...
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const [chromePath, outDir, ...urls] = process.argv.slice(2);
if (!chromePath || !outDir || urls.length === 0) {
  console.error('usage: docs_pdf_cdp.mjs <chrome> <outdir> <url...>');
  process.exit(2);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// 1. launch Chrome with remote debugging (network left ON so web fonts load).
const chrome = spawn(chromePath, [
  '--headless=new', '--disable-gpu', '--no-sandbox',
  // --disable-dev-shm-usage: CI runners give /dev/shm only ~64 MB and Chrome
  // crashes ("Target closed") when it overflows; force it to use /tmp instead.
  '--disable-dev-shm-usage',
  '--remote-debugging-port=0', '--no-first-run', '--disable-extensions',
  '--hide-scrollbars', 'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });

// 2. parse the devtools port from stderr.
const port = await new Promise((res, rej) => {
  let buf = '';
  const t = setTimeout(() => rej(new Error('Chrome did not print a debug port in 20s')), 20000);
  chrome.stderr.on('data', (d) => {
    buf += d.toString();
    const m = buf.match(/ws:\/\/127\.0\.0\.1:(\d+)\//);
    if (m) { clearTimeout(t); res(Number(m[1])); }
  });
});

// 3. find (or create) a page target and connect the CDP WebSocket.
let targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
let page = targets.find((t) => t.type === 'page');
if (!page) page = await (await fetch(`http://127.0.0.1:${port}/json/new`, { method: 'PUT' })).json();
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener('open', r, { once: true }));

let nextId = 0;
const pending = new Map();
let loadResolve = null;
ws.addEventListener('message', (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  if (m.method === 'Page.loadEventFired' && loadResolve) { loadResolve(); loadResolve = null; }
});
const cmd = (method, params = {}) => new Promise((res) => {
  const id = ++nextId;
  pending.set(id, res);
  ws.send(JSON.stringify({ id, method, params }));
});

await cmd('Page.enable');
let ok = 0;
for (let k = 0; k < urls.length; k++) {
  const loaded = new Promise((r) => { loadResolve = r; });
  await cmd('Page.navigate', { url: urls[k] });
  await Promise.race([loaded, sleep(9000)]);   // load event, or 9s cap
  await sleep(450);                            // let web fonts / layout settle
  // printBackground keeps the dark code-block backgrounds; preferCSSPageSize
  // honours the @page rule from the guide's print CSS.
  const r = await cmd('Page.printToPDF', { printBackground: true, preferCSSPageSize: true });
  const data = r.result && r.result.data;
  if (data) {
    writeFileSync(`${outDir}/${String(k).padStart(4, '0')}.pdf`, Buffer.from(data, 'base64'));
    ok++;
    process.stdout.write(`  [${k + 1}/${urls.length}] ${urls[k].replace(/^https?:\/\/[^/]+\//, '')}\n`);
  } else {
    process.stdout.write(`  ! no PDF for ${urls[k]}\n`);
  }
}
try { ws.close(); } catch {}
chrome.kill();
console.log(`rendered ${ok}/${urls.length} pages`);
process.exit(ok > 0 ? 0 : 1);
