// Read the AZ_FN_COVERAGE bitmap out of a live first paint.
//
// AZ_FN_COVERAGE makes each lifted function store 1 into AZ_COV_BASE + idx at
// entry. Static reachability over-reports badly - it is the transitive closure
// of what MIGHT be called - so only the executed set says what has to ship
// eagerly and what can become a lazily fetched chunk.
//
// Usage:
//   node --experimental-websocket read-coverage.js <page-url> <out.json> [waitMs]
//
// Pair the output with `coverage-manifest.tsv` from the same run's scratch dir
// (idx \t export_as \t name). Manifest indices are per-run; a manifest from a
// different run is meaningless here.
const COV_BASE = 0x41000;
const COV_CAP = 0xead0; // through 0x4EAD0, the on_click stack base

const url = process.argv[2] || 'http://127.0.0.1:8801/';
const out = process.argv[3] || 'coverage.json';
const waitMs = parseInt(process.argv[4] || '20000', 10);

async function main() {
  const list = await (await fetch('http://127.0.0.1:9222/json')).json();
  const page = list.find(t => t.type === 'page') || list[0];
  if (!page) throw new Error('no CDP target on :9222');

  const ws = new WebSocket(page.webSocketDebuggerUrl);
  let id = 0;
  const pending = new Map();
  ws.onmessage = ev => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) {
      pending.get(m.id)(m);
      pending.delete(m.id);
    }
  };
  await new Promise(r => (ws.onopen = r));
  const send = (method, params) =>
    new Promise(res => {
      const i = ++id;
      pending.set(i, res);
      ws.send(JSON.stringify({ id: i, method, params: params || {} }));
    });

  await send('Page.enable');
  await send('Runtime.enable');
  await send('Page.navigate', { url });

  // Wait for the loader to finish bootstrapping rather than a fixed sleep:
  // __azProbe is installed at the end of bootstrap.
  const deadline = Date.now() + waitMs;
  let ready = false;
  while (Date.now() < deadline) {
    const r = await send('Runtime.evaluate', {
      expression: '!!(window.__azProbe && window.__azProbe.memory)',
      returnByValue: true,
    });
    if (r.result && r.result.result && r.result.result.value === true) {
      ready = true;
      break;
    }
    await new Promise(r2 => setTimeout(r2, 500));
  }
  if (!ready) {
    console.error('__azProbe never appeared - bootstrap did not complete');
    process.exit(2);
  }

  // Collect indices with a nonzero byte. Return them as a compact string so a
  // large hit set does not blow up the CDP payload as a JSON array of numbers.
  const expr = `(() => {
    const m = window.__azProbe.memory;
    const u8 = new Uint8Array(m.buffer, ${COV_BASE}, ${COV_CAP});
    let hits = [];
    for (let i = 0; i < u8.length; i++) if (u8[i] !== 0) hits.push(i);
    return JSON.stringify({ total: u8.length, hits: hits });
  })()`;
  const r = await send('Runtime.evaluate', { expression: expr, returnByValue: true });
  const val = r.result && r.result.result && r.result.result.value;
  if (!val) {
    console.error('evaluate returned nothing:', JSON.stringify(r).slice(0, 400));
    process.exit(3);
  }
  const parsed = JSON.parse(val);
  require('fs').writeFileSync(out, JSON.stringify(parsed));
  console.log('coverage slots scanned : ' + parsed.total);
  console.log('functions ENTERED      : ' + parsed.hits.length);
  console.log('wrote ' + out);
  ws.close();
}

main().catch(e => {
  console.error(e && e.stack ? e.stack : String(e));
  process.exit(1);
});
