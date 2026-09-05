// Dump console output and exceptions from a page load, for diagnosing a
// bootstrap that never completes.
//
// Usage: node --experimental-websocket cdp-console.js <url> [waitMs]
const url = process.argv[2] || 'http://127.0.0.1:8801/';
const waitMs = parseInt(process.argv[3] || '25000', 10);

function argToStr(a) {
  if (a == null) return String(a);
  if (a.value !== undefined) return String(a.value);
  if (a.description) return a.description;
  return a.type || '?';
}

async function main() {
  const list = await (await fetch('http://127.0.0.1:9222/json')).json();
  const page = list.find(t => t.type === 'page') || list[0];
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  let id = 0;
  const pending = new Map();
  const lines = [];
  ws.onmessage = ev => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) {
      pending.get(m.id)(m);
      pending.delete(m.id);
      return;
    }
    if (m.method === 'Runtime.consoleAPICalled') {
      lines.push(m.params.type.toUpperCase() + ': ' +
                 (m.params.args || []).map(argToStr).join(' '));
    } else if (m.method === 'Runtime.exceptionThrown') {
      const d = m.params.exceptionDetails || {};
      lines.push('EXCEPTION: ' + (d.text || '') + ' ' +
                 ((d.exception && (d.exception.description || d.exception.value)) || ''));
    }
  };
  await new Promise(r => (ws.onopen = r));
  const send = (method, params) =>
    new Promise(res => {
      const i = ++id;
      pending.set(i, res);
      ws.send(JSON.stringify({ id: i, method, params: params || {} }));
    });

  await send('Runtime.enable');
  await send('Page.enable');
  await send('Page.navigate', { url });
  await new Promise(r => setTimeout(r, waitMs));

  const probe = await send('Runtime.evaluate', {
    expression: 'typeof window.__azProbe',
    returnByValue: true,
  });
  console.log('__azProbe: ' + (probe.result && probe.result.result &&
                               probe.result.result.value));
  console.log('--- console (' + lines.length + ' lines) ---');
  for (const l of lines) console.log(l.slice(0, 220));
  ws.close();
}

main().catch(e => {
  console.error(e && e.stack ? e.stack : String(e));
  process.exit(1);
});
