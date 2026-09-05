// CDP screenshot + console/exception capture for any azul web-lift app.
//
// Usage: node --experimental-websocket scripts/m9_e2e/cdp_screenshot.js <url> <out.png> [waitMs]
// Requires a Chromium/Edge on :9222 (--headless=new --remote-debugging-port=9222).
//
// Prints every console line and every uncaught exception (with wasm frames), then
// writes the PNG. Those logs are the point: for a freshly-lifted app they enumerate
// the lifting bugs (each `RuntimeError: ...` names a failing wasm function).
const fs = require('fs');

const CDP = process.env.AZ_CDP || 'http://127.0.0.1:9222';
const URL = process.argv[2] || 'http://127.0.0.1:8800/';
const OUT = process.argv[3] || 'C:/rb/shot.png';
const WAIT = parseInt(process.argv[4] || '12000', 10);

(async () => {
    const tab = await (await fetch(`${CDP}/json/new?${encodeURIComponent(URL)}`, { method: 'PUT' })).json();
    const ws = new WebSocket(tab.webSocketDebuggerUrl);
    await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; setTimeout(rej, 10000); });

    let id = 0;
    const pend = new Map();
    const logs = [];
    const errs = [];
    ws.onmessage = e => {
        const m = JSON.parse(e.data);
        if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); }
        if (m.method === 'Runtime.consoleAPICalled') {
            logs.push((m.params.args || [])
                .map(a => (a.value !== undefined ? String(a.value) : String(a.description || a.type)))
                .join(' '));
        }
        if (m.method === 'Runtime.exceptionThrown') {
            const d = m.params.exceptionDetails;
            const frames = ((d.stackTrace && d.stackTrace.callFrames) || [])
                .map(f => (f.functionName || '<anon>') + (f.url.includes('.wasm') ? '@wasm' : ''))
                .slice(0, 6).join(' <- ');
            errs.push(String((d.exception && d.exception.description) || d.text).split('\n')[0] + '  ||  ' + frames);
        }
    };
    const send = (method, params) => {
        const i = ++id;
        ws.send(JSON.stringify({ id: i, method, params }));
        return new Promise(r => pend.set(i, r));
    };

    await send('Runtime.enable', {});
    await send('Page.enable', {});
    await send('Emulation.setDeviceMetricsOverride',
        { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
    await new Promise(r => setTimeout(r, WAIT));

    console.log('===== CONSOLE (' + logs.length + ') =====');
    logs.slice(-40).forEach(l => console.log('  ' + l.slice(0, 200)));
    console.log('===== EXCEPTIONS (' + errs.length + ') =====');
    // Dedup: a lifting bug usually fires repeatedly from the same frame.
    const seen = new Set();
    errs.forEach(l => { const k = l.slice(0, 120); if (!seen.has(k)) { seen.add(k); console.log('  ' + l.slice(0, 240)); } });

    const shot = await send('Page.captureScreenshot', { format: 'png' });
    const data = shot.result && shot.result.data;
    if (!data) { console.log('SCREENSHOT FAILED'); process.exit(1); }
    fs.writeFileSync(OUT, Buffer.from(data, 'base64'));
    console.log('===== SCREENSHOT: ' + OUT + ' (' + fs.statSync(OUT).size + ' bytes) =====');

    // A quick "is anything actually rendered?" signal.
    const ev = async ex => (await send('Runtime.evaluate', { expression: ex, returnByValue: true })).result?.result?.value;
    console.log('body text length:', await ev('document.body.innerText.length'));
    console.log('DOM nodes:', await ev('document.querySelectorAll("*").length'));
    ws.close();
})().catch(e => { console.log('SHOT-ERR:', e.message || e); process.exit(1); });
