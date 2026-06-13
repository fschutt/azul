// CDP click gate for the Windows web-lift bring-up.
// Drives a REAL headless Chrome through the production loader.js path:
//   navigate → read counter → click the button → read counter again.
// PASS iff the visible counter incremented (the lifted on_click cb ran
// in wasm and the SetText TLV patch was applied by loader.js).
//
// Usage: node --experimental-websocket scripts/cdp_click_hw.js
// Requires: Chrome on :9222 (--remote-debugging-port=9222 --headless=new),
//           the azul web server on :8800.
const CDP = 'http://127.0.0.1:9222';
const URL = 'http://127.0.0.1:8800/';
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function main() {
  const tab = await (await fetch(`${CDP}/json/new?${encodeURIComponent(URL)}`, { method: 'PUT' })).json();
  const ws = new WebSocket(tab.webSocketDebuggerUrl);
  let id = 1; const pend = new Map();
  const send = (m, p = {}) => new Promise((res, rej) => { const i = id++; pend.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method: m, params: p })); });
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  const logs = [];
  ws.onmessage = ev => {
    const m = JSON.parse(ev.data);
    if (m.id && pend.has(m.id)) { const p = pend.get(m.id); pend.delete(m.id); m.error ? p.rej(new Error(JSON.stringify(m.error))) : p.res(m.result); }
    else if (m.method === 'Runtime.consoleAPICalled') logs.push((m.params.args || []).map(a => a.value).join(' '));
  };
  await send('Runtime.enable'); await send('Page.enable');
  await send('Page.navigate', { url: URL });
  await sleep(6000); // give the loader time to fetch+instantiate the wasm shards

  const readCounter = async () => {
    const r = await send('Runtime.evaluate', {
      expression: `(document.querySelector('#az_1')||{}).textContent || (document.body.innerText.match(/\\b\\d+\\b/)||[''])[0]`,
      returnByValue: true,
    });
    return r.result.value;
  };
  const before = await readCounter();
  // Click the button (or the counter region) via a real synthesized mouse event.
  await send('Runtime.evaluate', {
    expression: `(()=>{const b=[...document.querySelectorAll('button,[data-az-cb],div')].find(e=>/increase|counter|\\d/i.test(e.textContent));
      const r=(b||document.body).getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2};})()`,
    returnByValue: true,
  }).then(async pt => {
    const { x, y } = pt.result.value || { x: 50, y: 50 };
    for (const type of ['mousePressed', 'mouseReleased']) {
      await send('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1, buttons: 1 });
    }
  });
  await sleep(1500);
  const after = await readCounter();

  console.log(`counter before="${before}" after="${after}"`);
  if (logs.length) console.log('page console:', logs.slice(-8).join(' | '));
  await fetch(`${CDP}/json/close/${tab.id}`).catch(() => {});
  const bn = parseInt(before), an = parseInt(after);
  if (Number.isFinite(an) && Number.isFinite(bn) && an === bn + 1) {
    console.log('PASS: counter incremented via lifted wasm cb in a real browser');
    process.exit(0);
  }
  console.error('FAIL: counter did not increment as expected');
  process.exit(1);
}
main().catch(e => { console.error('ERROR:', e.message); process.exit(2); });
