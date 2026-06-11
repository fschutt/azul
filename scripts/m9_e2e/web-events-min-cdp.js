#!/usr/bin/env node
// S1 minimal verification: web-events-min.bin (click + keydown-broadcast +
// resize-broadcast, ~5 nodes — under the class-B trap). Confirms the two S1
// routing paths reach their wasm callbacks. Reads counters straight from wasm
// memory via __azProbe.mini.AzStartup_peekU32(modelPtr + 4*slot).
const PAGE_URL = process.argv[2] || 'http://127.0.0.1:8800/';
const CDP_HTTP = 'http://127.0.0.1:9222';
const SLOT = { click: 0, keydown: 6, resize: 10 };
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

    const nodes = JSON.parse((await send('Runtime.evaluate', {
        expression: `JSON.stringify([...document.querySelectorAll('[data-az-cb]')].map(el => { const r = el.getBoundingClientRect(); return { ev: el.getAttribute('data-az-ev'), idx: +el.getAttribute('data-az-cb'), x: r.x + r.width/2, y: r.y + r.height/2 }; }))`,
        returnByValue: true })).result.value);
    const byEv = {}; for (const n of nodes) byEv[n.ev] = n;
    console.log('cb nodes:', nodes.map(n => `${n.ev}@az_${n.idx}`).join(' '));

    const counters = async () => JSON.parse((await send('Runtime.evaluate', {
        expression: `JSON.stringify([...Array(16).keys()].map(k => __azProbe.mini.AzStartup_peekU32(__azProbe.modelPtr + 4*k)))`,
        returnByValue: true })).result.value);
    const before = await counters();

    // click (single-target hit-test)
    if (byEv.click) {
        await send('Input.dispatchMouseEvent', { type: 'mousePressed', x: Math.round(byEv.click.x), y: Math.round(byEv.click.y), button: 'left', buttons: 1, clickCount: 1 });
        await send('Input.dispatchMouseEvent', { type: 'mouseReleased', x: Math.round(byEv.click.x), y: Math.round(byEv.click.y), button: 'left', buttons: 0, clickCount: 1 });
        await sleep(120);
    }
    // keydown (broadcast)
    await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65 });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65 });
    await sleep(120);
    // resize (broadcast)
    await send('Emulation.setDeviceMetricsOverride', { width: 900, height: 700, deviceScaleFactor: 1, mobile: false });
    await sleep(150);
    await send('Runtime.evaluate', { expression: `window.dispatchEvent(new Event('resize'))`, returnByValue: true });
    await sleep(250);

    const after = await counters();
    let pass = true;
    console.log('\nslot (before -> after):');
    for (const [name, k] of Object.entries(SLOT)) {
        const ok = after[k] > before[k];
        if (!ok) pass = false;
        console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name.padEnd(8)} [${k}] ${before[k]} -> ${after[k]}`);
    }
    if (!pass) { console.log('\nlast console:'); log.slice(-20).forEach(l => console.log('  ' + l)); }
    try { await fetch(`${CDP_HTTP}/json/close/${tab.id}`); } catch (e) {}
    console.log(pass ? '\nS1 MIN: ALL 3 KINDS DELIVERED ✓' : '\nS1 MIN: MISSING KINDS ✗');
    process.exit(pass ? 0 : 1);
}
main().catch(e => { console.error('driver error:', e); process.exit(2); });
