#!/usr/bin/env node
// S2 verification: web-setcss-min.bin — on_click calls
// AzCallbackInfo_setCssProperty(width:300px) on the hit node. Asserts the
// change rides a CallbackChange -> SET_INLINE_STYLE TLV -> JS setProperty,
// i.e. the clicked element's inline style gains width:300px.
const PAGE_URL = process.argv[2] || 'http://127.0.0.1:8800/';
const CDP_HTTP = 'http://127.0.0.1:9222';
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function main() {
    const tab = await (await fetch(`${CDP_HTTP}/json/new?about:blank`, { method: 'PUT' })).json();
    const ws = new WebSocket(tab.webSocketDebuggerUrl);
    let nextId = 1; const pending = new Map(); const log = [];
    const send = (m, p = {}) => new Promise((res, rej) => { const id = nextId++; pending.set(id, { res, rej }); ws.send(JSON.stringify({ id, method: m, params: p })); });
    await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
    ws.onmessage = ev => {
        const msg = JSON.parse(ev.data);
        if (msg.id && pending.has(msg.id)) { const p = pending.get(msg.id); pending.delete(msg.id); msg.error ? p.rej(new Error(JSON.stringify(msg.error))) : p.res(msg.result); return; }
        if (msg.method === 'Runtime.consoleAPICalled') log.push('[' + msg.params.type + '] ' + (msg.params.args || []).map(a => a.value !== undefined ? a.value : a.description).join(' '));
        else if (msg.method === 'Runtime.exceptionThrown') { const d = msg.params.exceptionDetails; log.push('[EXC] ' + ((d.exception && d.exception.description) || d.text).split('\n')[0]); }
    };
    await send('Runtime.enable'); await send('Page.enable');
    await send('Page.navigate', { url: PAGE_URL });

    let ready = false;
    for (let i = 0; i < 60; i++) {
        await sleep(500);
        const r = await send('Runtime.evaluate', { expression: 'window.__azProbe && __azProbe.state ? 1 : 0', returnByValue: true });
        if (r.result.value === 1) { ready = true; break; }
    }
    if (!ready) { console.error('FAIL: no bootstrap'); log.slice(-20).forEach(l => console.error('  ' + l)); process.exit(1); }

    const rawTarget = (await send('Runtime.evaluate', {
        expression: `(() => { const el = document.querySelector('[data-az-cb]'); if (!el) return 'null'; const r = el.getBoundingClientRect(); return JSON.stringify({ idx: +el.getAttribute('data-az-cb'), x: r.x + r.width/2, y: r.y + r.height/2, styleWidth: el.style.width || '' }); })()`,
        returnByValue: true })).result.value;
    if (rawTarget === 'null') { console.error('FAIL: no cb node'); process.exit(1); }
    const t = JSON.parse(rawTarget);
    console.log('target az_' + t.idx + ' style.width BEFORE = "' + t.styleWidth + '"');

    await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: Math.round(t.x), y: Math.round(t.y), button: 'left', buttons: 1, clickCount: 1 });
    await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: Math.round(t.x), y: Math.round(t.y), button: 'left', buttons: 0, clickCount: 1 });
    await sleep(250);

    const after = (await send('Runtime.evaluate', {
        expression: `(document.getElementById('az_${t.idx}') || {}).style ? document.getElementById('az_${t.idx}').style.width : '?'`,
        returnByValue: true })).result.value;
    console.log('target style.width AFTER = "' + after + '"');
    const pass = after === '300px';
    if (!pass) { console.log('last console:'); log.slice(-20).forEach(l => console.log('  ' + l)); }
    try { await fetch(`${CDP_HTTP}/json/close/${tab.id}`); } catch (e) {}
    console.log(pass ? '\nS2: set_css_property ROUND-TRIPPED ✓ (width -> 300px)' : '\nS2: CSS change did NOT apply ✗');
    process.exit(pass ? 0 : 1);
}
main().catch(e => { console.error('driver error:', e); process.exit(2); });
