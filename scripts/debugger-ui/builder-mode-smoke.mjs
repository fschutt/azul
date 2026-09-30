// Headless UI test for the debugger page's light / dark mode (debugger.css
// tokens, `app.mode` in debugger.js, the palette in debugger-dnd.js).
//
//     node scripts/debugger-ui/builder-mode-smoke.mjs [--shots] [--chrome <path>] [--keep]
//
// Serves the REAL debugger page with a MOCK of the debug server (lib/smoke.mjs),
// starts a headless Chrome of its own, emulates the desktop's
// `prefers-color-scheme` and drives the Auto / Light / Dark toggle. It checks:
//
//   * Auto follows the desktop: a light page on a light desktop, a dark one on a
//     dark desktop, both readable (text / background contrast >= 4.5);
//   * Light and Dark pin the page whatever the desktop is, and the choice
//     survives a reload (localStorage);
//   * the palette asks for its thumbnails in the page's mode
//     (`get_component_thumbnail` `dark`), again when the mode changes, and shows
//     them on a backdrop of that mode;
//   * the panels that read the page's older token names (`--bg`, `--fg`, ...)
//     get a colour, not a transparent background.
//
// `--shots` also writes the page in both modes to
// scripts/debugger-ui/screenshots/light-dark-<mode>.png (the whole page) and
// light-dark-<mode>-palette.png (the palette).
//
// Needs node >= 21 (global WebSocket) and a Chrome / Chromium / Edge binary.
// Exit code 0 = all checks passed.

import fs from 'node:fs';
import path from 'node:path';
import { encodePng } from '../e2e-web/lib/png.mjs';
import { ROOT, args, builderMock, checks, clone, openPage, serveDebugger, startChrome, stopChrome, waitFor } from './lib/smoke.mjs';

const SHOTS_DIR = path.join(ROOT, 'scripts/debugger-ui/screenshots');

/** A 70 x 28 stand-in for a native thumbnail: a button on the mode's backdrop. */
function thumbnail(dark) {
    const w = 70, h = 28;
    const rgba = Buffer.alloc(w * h * 4);
    const back = dark ? [30, 30, 30] : [255, 255, 255];
    const face = dark ? [10, 132, 255] : [0, 122, 204];
    for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
            const inFace = x >= 6 && x < w - 6 && y >= 5 && y < h - 5;
            const c = inFace ? face : back;
            rgba.set([c[0], c[1], c[2], 255], (y * w + x) * 4);
        }
    }
    return 'data:image/png;base64,' + encodePng(w, h, rgba).toString('base64');
}
const THUMB = { light: thumbnail(false), dark: thumbnail(true) };

// ── the mock debug server ────────────────────────────────────────────────

const registry = { libraries: [
    { name: 'builtin', modifiable: false, components: [
        { tag: 'div', display_name: 'Div', data_model: [] },
        { tag: 'p', display_name: 'Paragraph', data_model: [] },
        { tag: 'button', display_name: 'Button', data_model: [] },
    ] },
    { name: 'user', modifiable: true, components: [
        { tag: 'card', display_name: 'Card', data_model: [
            { name: 'text', field_type: 'String', default: 'Title', required: false, description: '' },
            { name: 'wide', field_type: 'bool', default: 'false', required: false, description: '' },
        ] },
    ] },
] };

const builder = builderMock(registry);
builder.ops({ op: 'builder_insert', parent: 0, component: 'p', attrs: { text: 'Hello' } });
builder.ops({ op: 'builder_insert', parent: 0, library: 'user', component: 'card', attrs: { text: 'Card' } });
const sent = [];

function handle(msg) {
    sent.push(msg);
    switch (msg.op) {
        case 'get_state': return { logical_width: 400, logical_height: 300, hidpi_factor: 1 };
        case 'get_component_registry': return clone(registry);
        case 'get_libraries':
            return { libraries: registry.libraries.map((l) => ({
                name: l.name, modifiable: !!l.modifiable, component_count: l.components.length })) };
        case 'get_component_thumbnail':
            return { library: msg.library, name: msg.name, key: '0', data: msg.dark ? THUMB.dark : THUMB.light,
                empty: false, width: 70, height: 28, cached: false, no_visual: null };
        case 'get_app_state': return {};
        default: {
            const v = builder.ops(msg);
            return v === undefined ? null : v;
        }
    }
}

const thumbsSince = (i) => sent.slice(i).filter((m) => m.op === 'get_component_thumbnail');

const HELPERS = `
window.__t = {
  rgb(css) { const m = String(css).match(/rgba?\\(([^)]+)\\)/); return m ? m[1].split(',').slice(0, 3).map(Number) : null; },
  lum(c) { const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); },
  contrast(a, b) { const x = this.lum(a), y = this.lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); },
  look() {
    const body = getComputedStyle(document.body);
    const bg = this.rgb(body.backgroundColor), fg = this.rgb(body.color);
    const thumb = document.querySelector('.azb-thumb');
    const side = document.querySelector('.panel-col');
    return { bg, fg, contrast: Math.round(this.contrast(bg, fg) * 10) / 10, dark: this.lum(bg) < 0.2,
      scheme: getComputedStyle(document.documentElement).colorScheme,
      sidebar: side ? this.rgb(getComputedStyle(side).backgroundColor) : null,
      thumbBg: thumb ? this.rgb(getComputedStyle(thumb).backgroundColor) : null,
      appDark: app.mode.isDark(), choice: app.mode.choice,
      checked: [...document.querySelectorAll('[data-mode-choice]')].filter((b) => b.getAttribute('aria-checked') === 'true').map((b) => b.dataset.modeChoice) };
  },
  pick(choice) { document.querySelector('[data-mode-choice="' + choice + '"]').click(); return true; },
  thumbsShown() { return [...document.querySelectorAll('.azb-thumb img')].map((i) => i.src); },
  /** The background a panel written against an older token name gets. */
  aliasBackground(name) {
    const probe = document.createElement('div');
    probe.style.background = 'var(--' + name + ')';
    document.body.appendChild(probe);
    const bg = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return bg;
  },
};
true`;

async function desktop(cdp, mode) {
    await cdp.send('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: mode }] });
}

async function shoot(cdp, name, clipExpr) {
    const params = { format: 'png' };
    if (clipExpr) params.clip = Object.assign({ scale: 2 }, await cdp.eval(clipExpr));
    const r = await cdp.send('Page.captureScreenshot', params, 20_000);
    fs.mkdirSync(SHOTS_DIR, { recursive: true });
    const file = path.join(SHOTS_DIR, `light-dark-${name}.png`);
    fs.writeFileSync(file, Buffer.from(r.data, 'base64'));
    return path.relative(ROOT, file);
}

const PALETTE_CLIP = `(() => { const r = document.getElementById('inspector-component-palette').getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: Math.min(r.height, 260) }; })()`;

async function main() {
    const { check, finish } = checks();
    const server = await serveDebugger(handle);
    const chrome = await startChrome();
    const url = `http://127.0.0.1:${server.address().port}/`;
    let cdp;
    const ready = `typeof app !== 'undefined' && !!app.mode && !!window.azDnd
        && azDnd.state.mode === 'document' && document.querySelectorAll('.azb-thumb img').length >= 4`;
    try {
        cdp = await openPage(chrome, 'about:blank');
        await desktop(cdp, 'light');
        await cdp.send('Page.navigate', { url });
        const up = await waitFor(cdp, ready, 10000);
        check('the page opens with its palette thumbnails', up,
            await cdp.eval(`({ app: typeof app, cards: document.querySelectorAll('.azb-card').length })`).catch((e) => String(e)));
        if (!up) throw new Error('page not ready');
        await cdp.eval(HELPERS);

        // ── Auto on a light desktop ──
        let look = await cdp.eval('__t.look()');
        check('a fresh page follows the desktop: Auto is the checked choice', look.choice === 'auto'
            && JSON.stringify(look.checked) === '["auto"]', look);
        check('on a light desktop the page is light, and readable (contrast >= 4.5)',
            !look.dark && !look.appDark && look.scheme === 'light' && look.contrast >= 4.5, look);
        check('the palette asked for light thumbnails (dark: false)',
            thumbsSince(0).length >= 4 && thumbsSince(0).every((m) => m.dark === false), thumbsSince(0));
        check('...and shows them on a light backdrop', look.thumbBg && __lum(look.thumbBg) > 0.8, look.thumbBg);
        const aliases = await cdp.eval(`['bg', 'bg-alt', 'bg-darker', 'bg-secondary', 'hover']
            .map((n) => [n, __t.aliasBackground(n)])`);
        check('the older token names are defined: a panel using --bg / --bg-alt / ... is not see-through',
            aliases.every(([, bg]) => bg !== 'rgba(0, 0, 0, 0)'), aliases);
        if (args.includes('--shots')) {
            console.log('     screenshot: ' + await shoot(cdp, 'light'));
            console.log('     screenshot: ' + await shoot(cdp, 'light-palette', PALETTE_CLIP));
        }

        // ── Auto on a dark desktop ──
        let mark = sent.length;
        await desktop(cdp, 'dark');
        await waitFor(cdp, `__t.look().dark && __t.thumbsShown().length >= 4
            && __t.thumbsShown().every((s) => s === ${JSON.stringify(THUMB.dark)})`, 5000);
        look = await cdp.eval('__t.look()');
        check('when the desktop turns dark, Auto turns the page dark, readable too',
            look.dark && look.appDark && look.scheme === 'dark' && look.contrast >= 4.5, look);
        check('the palette asks for its thumbnails again, in dark mode (dark: true)',
            thumbsSince(mark).length >= 4 && thumbsSince(mark).every((m) => m.dark === true), thumbsSince(mark));
        check('...shows the dark pictures, on a dark backdrop',
            await cdp.eval(`__t.thumbsShown().every((s) => s === ${JSON.stringify(THUMB.dark)})`)
            && __lum(look.thumbBg) < 0.05, look.thumbBg);
        if (args.includes('--shots')) {
            console.log('     screenshot: ' + await shoot(cdp, 'dark'));
            console.log('     screenshot: ' + await shoot(cdp, 'dark-palette', PALETTE_CLIP));
        }

        // ── Light pinned on a dark desktop ──
        mark = sent.length;
        await cdp.eval(`__t.pick('light')`);
        await waitFor(cdp, `!__t.look().dark && __t.thumbsShown().length >= 4
            && __t.thumbsShown().every((s) => s === ${JSON.stringify(THUMB.light)})`, 5000);
        look = await cdp.eval('__t.look()');
        check('Light pins the page light on a dark desktop', !look.dark && !look.appDark
            && look.scheme === 'light' && JSON.stringify(look.checked) === '["light"]', look);
        check('...the palette follows (dark: false)',
            thumbsSince(mark).length >= 4 && thumbsSince(mark).every((m) => m.dark === false), thumbsSince(mark));
        check('...and the choice is remembered',
            await cdp.eval(`localStorage.getItem('azul_debugger_mode')`) === 'light');

        await cdp.send('Page.reload', {});
        await waitFor(cdp, ready, 10000);
        await cdp.eval(HELPERS);
        look = await cdp.eval('__t.look()');
        check('after a reload the page is still pinned light on the dark desktop',
            !look.dark && look.choice === 'light' && JSON.stringify(look.checked) === '["light"]', look);

        // ── Dark pinned on a light desktop ──
        await desktop(cdp, 'light');
        mark = sent.length;
        await cdp.eval(`__t.pick('dark')`);
        await waitFor(cdp, `__t.look().dark && __t.thumbsShown().length >= 4
            && __t.thumbsShown().every((s) => s === ${JSON.stringify(THUMB.dark)})`, 5000);
        look = await cdp.eval('__t.look()');
        check('Dark pins the page dark on a light desktop',
            look.dark && look.appDark && look.scheme === 'dark' && JSON.stringify(look.checked) === '["dark"]', look);
        check('...the palette follows (dark: true)',
            thumbsSince(mark).length >= 4 && thumbsSince(mark).every((m) => m.dark === true), thumbsSince(mark));
        mark = sent.length;
        await desktop(cdp, 'dark');
        await new Promise((r) => setTimeout(r, 200));
        await desktop(cdp, 'light');
        await new Promise((r) => setTimeout(r, 200));
        look = await cdp.eval('__t.look()');
        check('a pinned page ignores the desktop flipping, and asks for nothing', look.dark
            && thumbsSince(mark).length === 0, { look, asked: thumbsSince(mark).length });

        // ── back to Auto ──
        await cdp.eval(`__t.pick('auto')`);
        await waitFor(cdp, `!__t.look().dark`, 5000);
        look = await cdp.eval('__t.look()');
        check('Auto follows the (light) desktop again', !look.dark && look.choice === 'auto'
            && await cdp.eval(`localStorage.getItem('azul_debugger_mode')`) === 'auto', look);

        check('no page exceptions', cdp.exceptions.length === 0, cdp.exceptions);
        const errors = cdp.console.filter((c) => c.kind === 'error');
        check('no console errors', errors.length === 0, errors);
    } finally {
        if (cdp) cdp.close();
        await stopChrome(chrome);
        server.close();
    }
    finish();
}

/** Relative luminance of an [r, g, b] triple (node side). */
function __lum(c) {
    const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
}

main().catch((e) => { console.error(e); process.exit(2); });
