#!/usr/bin/env node
// S1 input-event coverage test (2026-06-11). Drives examples/c/web-events.bin
// through headless Chrome via CDP: synthesizes real input for every wired
// event kind, then reads the per-kind counters straight out of wasm linear
// memory (window.__azProbe.mini.AzStartup_peekU32).
//
// Usage: node scripts/m9_e2e/web-events-cdp.js [page_url]
// Requires: Chrome headless on :9222, the web server (web-events.bin) on :8800.
//
// PASS = every exercised slot incremented; FAIL lists the slots that stayed 0.

const PAGE_URL = process.argv[2] || 'http://127.0.0.1:8800/';
const CDP_HTTP = 'http://127.0.0.1:9222';

const SLOT = {
    click: 0, mousedown: 1, mousemove: 3, keydown: 6,
    resize: 10, scroll: 11, mouseenter: 12, mouseleave: 13, contextmenu: 14,
};

function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }

async function main() {
    const res = await fetch(`${CDP_HTTP}/json/new?about:blank`, { method: 'PUT' });
    const tab = await res.json();
    const ws = new WebSocket(tab.webSocketDebuggerUrl);
    let nextId = 1;
    const pending = new Map();
    const consoleLines = [];

    const send = (method, params = {}) => new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { resolve, reject, method });
        ws.send(JSON.stringify({ id, method, params }));
    });

    await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
    ws.onmessage = (ev) => {
        const msg = JSON.parse(ev.data);
        if (msg.id && pending.has(msg.id)) {
            const p = pending.get(msg.id); pending.delete(msg.id);
            if (msg.error) p.reject(new Error(p.method + ': ' + JSON.stringify(msg.error)));
            else p.resolve(msg.result);
            return;
        }
        if (msg.method === 'Runtime.consoleAPICalled') {
            const args = (msg.params.args || []).map(a =>
                a.value !== undefined ? String(a.value) : (a.description || a.type)).join(' ');
            consoleLines.push(`[${msg.params.type}] ${args}`);
        } else if (msg.method === 'Runtime.exceptionThrown') {
            const d = msg.params.exceptionDetails;
            consoleLines.push(`[EXCEPTION] ${(d.exception && d.exception.description || d.text).split('\n')[0]}`);
        }
    };

    await send('Runtime.enable');
    await send('Page.enable');
    await send('Page.navigate', { url: PAGE_URL });

    // Wait for bootstrap (azState != 0 via the probe).
    let ready = false;
    for (let i = 0; i < 60; i++) {
        await sleep(500);
        const r = await send('Runtime.evaluate', {
            expression: 'window.__azProbe && __azProbe.state ? 1 : 0',
            returnByValue: true,
        });
        if (r.result.value === 1) { ready = true; break; }
    }
    if (!ready) {
        console.error('FAIL: bootstrap never completed (azState=0).');
        consoleLines.slice(-25).forEach(l => console.error('  ' + l));
        process.exit(1);
    }

    // Collect the cb-bearing nodes + their geometry from the live DOM.
    const nodesR = await send('Runtime.evaluate', {
        expression: `JSON.stringify([...document.querySelectorAll('[data-az-cb]')].map(el => {
            const r = el.getBoundingClientRect();
            return { ev: el.getAttribute('data-az-ev'), idx: +el.getAttribute('data-az-cb'),
                     x: r.x + r.width / 2, y: r.y + r.height / 2, w: r.width, h: r.height };
        }))`,
        returnByValue: true,
    });
    const nodes = JSON.parse(nodesR.result.value);
    const byEv = {};
    for (const n of nodes) byEv[n.ev] = n;
    console.log('cb nodes:', nodes.map(n => `${n.ev}@az_${n.idx}(${n.x | 0},${n.y | 0})`).join(' '));

    const counters = async () => {
        const r = await send('Runtime.evaluate', {
            expression: `JSON.stringify([...Array(16).keys()].map(k =>
                __azProbe.mini.AzStartup_peekU32(__azProbe.modelPtr + 4 * k)))`,
            returnByValue: true,
        });
        return JSON.parse(r.result.value);
    };
    const before = await counters();

    const mouse = (type, x, y, opts = {}) => send('Input.dispatchMouseEvent', {
        type, x: Math.round(x), y: Math.round(y), button: opts.button || 'none',
        buttons: opts.buttons === undefined ? 0 : opts.buttons,
        clickCount: opts.clickCount || 0, deltaX: opts.deltaX || 0, deltaY: opts.deltaY || 0,
    });

    // 1. click + mousedown (left press/release on each target).
    for (const ev of ['click', 'mousedown']) {
        const n = byEv[ev];
        if (!n) continue;
        await mouse('mousePressed', n.x, n.y, { button: 'left', buttons: 1, clickCount: 1 });
        await mouse('mouseReleased', n.x, n.y, { button: 'left', buttons: 0, clickCount: 1 });
        await sleep(80);
    }

    // 2. mousemove over its div (rAF-throttled — give it a frame).
    if (byEv.mousemove) {
        await mouse('mouseMoved', byEv.mousemove.x, byEv.mousemove.y);
        await sleep(120);
        await mouse('mouseMoved', byEv.mousemove.x + 4, byEv.mousemove.y);
        await sleep(120);
    }

    // 3. wheel over the scroll div.
    if (byEv.scroll) {
        await mouse('mouseMoved', byEv.scroll.x, byEv.scroll.y);
        await sleep(60);
        await mouse('mouseWheel', byEv.scroll.x, byEv.scroll.y, { deltaY: 60 });
        await sleep(80);
    }

    // 4. enter/leave: cross into the enter-div, then into the leave-div,
    //    then out to a corner — boundary crossings fire enter+leave.
    if (byEv.mouseenter && byEv.mouseleave) {
        await mouse('mouseMoved', byEv.mouseenter.x, byEv.mouseenter.y);
        await sleep(60);
        await mouse('mouseMoved', byEv.mouseleave.x, byEv.mouseleave.y);
        await sleep(60);
        await mouse('mouseMoved', 2, 2);
        await sleep(60);
    }

    // 5. contextmenu (right press/release).
    if (byEv.contextmenu) {
        const n = byEv.contextmenu;
        await mouse('mousePressed', n.x, n.y, { button: 'right', buttons: 2, clickCount: 1 });
        await mouse('mouseReleased', n.x, n.y, { button: 'right', buttons: 0, clickCount: 1 });
        await sleep(80);
    }

    // 6. keydown (broadcast — no focus needed).
    await send('Input.dispatchKeyEvent', {
        type: 'keyDown', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65,
    });
    await send('Input.dispatchKeyEvent', {
        type: 'keyUp', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65,
    });
    await sleep(80);

    // 7. resize via device-metrics override; explicitly dispatch the
    //    window resize event too (headless does not always emit it).
    await send('Emulation.setDeviceMetricsOverride', {
        width: 900, height: 700, deviceScaleFactor: 1, mobile: false,
    });
    await sleep(150);
    await send('Runtime.evaluate', {
        expression: `window.dispatchEvent(new Event('resize'))`,
        returnByValue: true,
    });
    await sleep(250);

    const after = await counters();
    const expected = Object.entries(SLOT);
    let pass = true;
    console.log('\nslot results (before -> after):');
    for (const [name, k] of expected) {
        const ok = after[k] > before[k];
        if (!ok) pass = false;
        console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name.padEnd(12)} [${k}] ${before[k]} -> ${after[k]}`);
    }
    if (!pass) {
        console.log('\nlast console lines:');
        consoleLines.slice(-30).forEach(l => console.log('  ' + l));
    }
    try { await fetch(`${CDP_HTTP}/json/close/${tab.id}`); } catch (e) {}
    console.log(pass ? '\nALL EVENT KINDS DELIVERED ✓' : '\nSOME EVENT KINDS MISSING ✗');
    process.exit(pass ? 0 : 1);
}

main().catch(e => { console.error('driver error:', e); process.exit(2); });
