#!/usr/bin/env node
// browser_test.mjs [--chrome PATH] [-- SANDBOX COMMAND...]: the sandbox's pages in a real browser
// (headless Chrome or Chromium over the DevTools protocol; Node's own fetch, WebSocket and http, no
// packages). It starts the sandbox (the nearest target/debug/azul-pay-sandbox, or the command after
// `--`, which is given `--port 0 --webhook-url ...`) on a free loopback port with its webhooks going
// to a receiver here, creates sessions the way an app's backend does, and pays the way a payer does:
//
//   card-paid        a test card typed into the split-screen page, Pay, back on the success page,
//                    the signed checkout.session.completed received
//   card-declined    4000 0000 0000 0002: the page stays and shows Stripe's message
//   card-invalid     a wrong Luhn digit: the page's own check stops it before any request
//   card-3ds         4000 0025 0000 3155: the bank's step, Complete, back on the success page
//   real-card        a real-looking card number is refused with the sandbox's message
//   fields-bridge    the popover's fields page: ready / brand / complete over /_bridge, then the
//                    app's #cmd=confirm, a 3-D Secure step inside the page, the result
//   sepa-mandate     the GoCardless-shaped flow: holder and IBAN, the mandate, the bank's Confirm,
//                    the signed batch with billing_requests.fulfilled
//   looks-like-test  every page's title starts with [TEST], the ribbon and the watermark show, a
//                    request naming another host is refused
//
// One PASS / FAIL line per flow; the exit code is the number of failures. No profile of the
// user's is touched (a fresh temporary one) and nothing leaves this computer.

import { spawn } from 'node:child_process';
import { createHmac, timingSafeEqual } from 'node:crypto';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { createServer, request as httpRequest } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const dashes = process.argv.indexOf('--');
const own = dashes > 0 ? process.argv.slice(2, dashes) : process.argv.slice(2);
const arg = (name, fallback) => {
  const i = own.indexOf(name);
  return i >= 0 && own[i + 1] ? own[i + 1] : fallback;
};
// the binary cargo built: in CARGO_TARGET_DIR, else in the nearest target/ above this crate
const built = () => {
  if (process.env.CARGO_TARGET_DIR) return join(process.env.CARGO_TARGET_DIR, 'debug', 'azul-pay-sandbox');
  for (let d = here; d !== dirname(d); d = dirname(d)) {
    const at = join(d, 'target', 'debug', 'azul-pay-sandbox');
    if (existsSync(at)) return at;
  }
  return join(here, 'target', 'debug', 'azul-pay-sandbox');
};
const SANDBOX = dashes > 0 ? process.argv.slice(dashes + 1) : [built()];
const CHROME = arg('--chrome', process.env.CHROME || [
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  '/usr/bin/google-chrome', '/usr/bin/chromium', '/usr/bin/chromium-browser',
].find((p) => existsSync(p)));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// polls `fn` until it gives something truthy; a throw counts as "not yet" (a page that is between
// two documents cannot be asked), and the last one is named when the time is up
const until = async (what, fn, ms = 10000) => {
  const end = Date.now() + ms;
  let last = '';
  for (;;) {
    try {
      const v = await fn();
      if (v) return v;
    } catch (e) {
      last = `: ${e && e.message || e}`;
    }
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}${last}`);
    await sleep(100);
  }
};
// the sandbox and Chrome; on the way out each is stopped and waited for, then Chrome's temporary
// profile goes (it writes into it until it has exited)
const children = [];
let profile = '';
const running = (c) => c.exitCode === null && c.signalCode === null;
const stop = async (code) => {
  for (const c of children.filter(running)) {
    const gone = new Promise((r) => c.once('exit', r));
    c.kill();
    await Promise.race([gone, sleep(5000)]);
    if (running(c)) c.kill('SIGKILL');
  }
  if (profile) rmSync(profile, { recursive: true, force: true, maxRetries: 3 });
  process.exit(code);
};
process.on('uncaughtException', (e) => {
  console.error(`browser_test: ${e && e.message || e}`);
  stop(1);
});
process.on('exit', () => children.filter(running).forEach((c) => c.kill()));

// ---- the app's side: the webhooks the sandbox posts (kept with their headers), the return pages ----
const received = [];
const receiver = createServer((req, res) => {
  const chunks = [];
  req.on('data', (c) => chunks.push(c));
  req.on('end', () => {
    if (req.method === 'POST' && req.url.startsWith('/hooks/')) {
      received.push({ url: req.url, headers: req.headers, body: Buffer.concat(chunks) });
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end('{"ok":true}');
    } else if (req.method === 'GET' && req.url.startsWith('/return/')) {
      res.writeHead(200, { 'content-type': 'text/html' });
      res.end('<!doctype html><title>back in the app</title><p>returned</p>');
    } else {
      res.writeHead(404);
      res.end();
    }
  });
});
await new Promise((r) => receiver.listen(0, '127.0.0.1', r));
const APP = `http://127.0.0.1:${receiver.address().port}`;

// ---- the sandbox ----
if (dashes < 0 && !existsSync(SANDBOX[0])) throw new Error(`no sandbox at ${SANDBOX[0]} (cargo build -p azul-pay-sandbox, or -- COMMAND...)`);
const sandbox = spawn(SANDBOX[0], [...SANDBOX.slice(1), '--port', '0', '--webhook-url', `${APP}/hooks/{provider}`], { stdio: ['ignore', 'pipe', 'inherit'] });
children.push(sandbox);
sandbox.on('error', (e) => { throw new Error(`the sandbox did not start: ${e.message}`); });
let BASE = '';
sandbox.stdout.on('data', (d) => {
  const m = String(d).match(/on (http:\/\/127\.0\.0\.1:\d+)/);
  if (m && !BASE) BASE = m[1];
});
sandbox.on('exit', (code) => {
  if (!BASE) throw new Error(`the sandbox stopped (exit ${code}) before it listened`);
});
await until('the sandbox to start', () => BASE, 15000);
const keys = await (await fetch(`${BASE}/_dev/keys`)).json();
const STRIPE = keys.providers['fake-stripe'];
const GC = keys.providers['fake-gocardless'];

// ---- Chrome over the DevTools protocol ----
if (!CHROME) throw new Error('no Chrome or Chromium found (--chrome PATH or CHROME=...)');
profile = mkdtempSync(join(tmpdir(), 'pay-sandbox-chrome-'));
const chrome = spawn(CHROME, [
  '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`,
  '--no-first-run', '--no-default-browser-check', '--disable-background-networking',
  '--disable-component-update', '--disable-sync', '--disable-default-apps', '--disable-extensions',
  '--disable-client-side-phishing-detection', '--safebrowsing-disable-auto-update',
  '--metrics-recording-only', '--disable-features=Translate,OptimizationHints,MediaRouter',
  'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
children.push(chrome);
let wsUrl = '';
chrome.stderr.on('data', (d) => {
  const m = String(d).match(/DevTools listening on (ws:\/\/\S+)/);
  if (m) wsUrl = m[1];
});
await until('Chrome to listen', () => wsUrl, 20000);

class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.id = 0;
    this.waiting = new Map();
    this.listeners = [];
    this.ws.onmessage = (m) => {
      const msg = JSON.parse(m.data);
      if (msg.id && this.waiting.has(msg.id)) {
        const { ok, fail } = this.waiting.get(msg.id);
        this.waiting.delete(msg.id);
        msg.error ? fail(new Error(JSON.stringify(msg.error))) : ok(msg.result);
      } else if (msg.method) {
        this.listeners.forEach((l) => l(msg));
      }
    };
  }
  open() { return new Promise((r, e) => { this.ws.onopen = r; this.ws.onerror = e; }); }
  // the next event `method` of the page `sessionId`
  once(method, sessionId) {
    return new Promise((r) => {
      const l = (msg) => {
        if (msg.method === method && msg.sessionId === sessionId) {
          this.listeners = this.listeners.filter((x) => x !== l);
          r(msg.params);
        }
      };
      this.listeners.push(l);
    });
  }
  send(method, params = {}, sessionId) {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params, sessionId }));
    return new Promise((ok, fail) => this.waiting.set(id, { ok, fail }));
  }
}
const cdp = new Cdp(wsUrl);
await cdp.open();
const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
const page = (method, params) => cdp.send(method, params, sessionId);
await page('Page.enable');
await page('Runtime.enable');

const evaluate = async (expression) => {
  const r = await page('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(`${expression}: ${JSON.stringify(r.exceptionDetails)}`);
  return r.result.value;
};
const url = () => evaluate('location.href');
// opens `to` as a new document and waits for its load event
const goto = async (to) => {
  const loaded = cdp.once('Page.loadEventFired', sessionId);
  const nav = await page('Page.navigate', { url: to });
  if (nav.errorText) throw new Error(`${to}: ${nav.errorText}`);
  await Promise.race([loaded, sleep(10000).then(() => { throw new Error(`${to} did not load`); })]);
};
// types into an input the way a payer does: focus, the value, an input event per character
const type = (selector, text) => evaluate(`(() => { const el = document.querySelector(${JSON.stringify(selector)});
  el.focus(); el.value = ''; for (const ch of ${JSON.stringify(text)}) { el.value += ch; el.dispatchEvent(new Event('input', {bubbles: true})); }
  return el.value; })()`);
const click = (selector) => evaluate(`(() => { document.querySelector(${JSON.stringify(selector)}).click(); return true; })()`);
const text = () => evaluate('document.body.innerText');
const waitForUrl = (prefix) => until(`a page at ${prefix}`, async () => (await url()).startsWith(prefix) && (await evaluate('document.readyState')) === 'complete');

// ---- the backend's part: sessions and billing requests on the sandbox's APIs ----
const form = (pairs) => Object.entries(pairs).map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&');
let made = 0;
const session = async (method, uiMode) => {
  const ref = `browser-card-${++made}`;
  const body = {
    mode: 'payment', ui_mode: uiMode, client_reference_id: ref, 'metadata[provider_ref]': ref,
    'line_items[0][quantity]': '1', 'line_items[0][price_data][currency]': 'eur',
    'line_items[0][price_data][unit_amount]': '5999', 'line_items[0][price_data][product_data][name]': 'Test product',
    'payment_method_types[0]': method, 'payment_intent_data[metadata][provider_ref]': ref,
    expires_at: String(Math.floor(Date.now() / 1000) + 3600),
  };
  if (uiMode === 'elements') body.return_url = `${APP}/return/ok`;
  else { body.success_url = `${APP}/return/ok`; body.cancel_url = `${APP}/return/cancel`; }
  const r = await fetch(`${STRIPE.api}/v1/checkout/sessions`, {
    method: 'POST',
    headers: { authorization: `Bearer ${STRIPE.secret_key}`, 'content-type': 'application/x-www-form-urlencoded', 'idempotency-key': ref },
    body: form(body),
  });
  if (r.status !== 200) throw new Error(`session: ${r.status} ${await r.text()}`);
  return r.json();
};
const flow = async () => {
  const headers = { authorization: `Bearer ${GC.secret_key}`, 'gocardless-version': '2015-07-06', 'content-type': 'application/json' };
  const ref = `browser-sepa-${++made}`;
  const br = await (await fetch(`${GC.api}/billing_requests`, { method: 'POST', headers: { ...headers, 'idempotency-key': ref }, body: JSON.stringify({ billing_requests: {
    mandate_request: { scheme: 'sepa_core', currency: 'EUR', metadata: { provider_ref: ref } },
    subscription_request: { amount: 5999, currency: 'EUR', interval_unit: 'monthly', interval: 1, count: 1, name: 'Test product', metadata: { provider_ref: ref } },
    metadata: { provider_ref: ref } } }) })).json();
  const f = await (await fetch(`${GC.api}/billing_request_flows`, { method: 'POST', headers, body: JSON.stringify({ billing_request_flows: {
    redirect_uri: `${APP}/return/ok`, exit_uri: `${APP}/return/cancel`, links: { billing_request: br.billing_requests.id } } }) })).json();
  return f.billing_request_flows.authorisation_url;
};
const session_of = async (id) => (await fetch(`${STRIPE.api}/v1/checkout/sessions/${id}`, { headers: { authorization: `Bearer ${STRIPE.secret_key}` } })).json();
const stripeEvents = (since) => received.slice(since).filter((r) => r.url === '/hooks/fake-stripe').map((r) => {
  const sig = String(r.headers['stripe-signature'] || '');
  const t = (sig.match(/t=(\d+)/) || [])[1];
  const v1 = (sig.match(/v1=([0-9a-f]+)/) || [])[1] || '';
  const want = createHmac('sha256', STRIPE.webhook_secret).update(`${t}.`).update(r.body).digest('hex');
  if (want.length !== v1.length || !timingSafeEqual(Buffer.from(want), Buffer.from(v1))) throw new Error('a Stripe-shaped webhook whose signature does not verify');
  return JSON.parse(r.body).type;
});
const switchOn = (name) => fetch(`${BASE}/_dev/switch`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ name }) });
const bridge = async () => (await (await fetch(`${BASE}/_dev/state`)).json()).bridge;

// ---- the flows ----
const results = [];
const check = async (name, fn) => {
  try {
    const detail = await fn();
    results.push(true);
    console.log(`PASS  ${name}${detail ? `  (${detail})` : ''}`);
  } catch (e) {
    results.push(false);
    console.log(`FAIL  ${name}\n      ${String(e && e.message || e).slice(0, 600)}`);
  }
};
const payCard = async (number) => {
  await type('input[name=card_number]', number);
  await type('input[name=exp]', '1234');
  await type('input[name=cvc]', '123');
  await click('form.card button');
};

await check('card-paid', async () => {
  const s = await session('card', 'hosted_page');
  const before = received.length;
  await goto(s.url);
  await payCard('4242424242424242');
  await waitForUrl(`${APP}/return/ok`);
  const types = await until('the signed webhooks', () => { const t = stripeEvents(before); return t.includes('checkout.session.completed') && t; });
  const now = await session_of(s.id);
  if (now.status !== 'complete' || now.payment_status !== 'paid') throw new Error(`the session is ${now.status} / ${now.payment_status}`);
  return types.join(', ');
});

await check('card-declined', async () => {
  const s = await session('card', 'hosted_page');
  await goto(s.url);
  await payCard('4000000000000002');
  await until('the decline message', async () => (await text()).includes('Your card was declined.'));
  if (!(await url()).startsWith(s.url)) throw new Error('left the page');
  if ((await session_of(s.id)).status !== 'open') throw new Error('the session closed');
  return 'stays open with Stripe\'s message';
});

await check('card-invalid', async () => {
  const s = await session('card', 'hosted_page');
  await goto(s.url);
  await payCard('4242424242424243');
  await until('the page\'s own check', async () => (await text()).includes('Your card number is invalid.'));
  if ((await session_of(s.id)).payment_intent !== null) throw new Error('the payment was tried');
  return 'stopped before any request';
});

await check('real-card', async () => {
  const s = await session('card', 'hosted_page');
  await goto(s.url);
  await payCard('4111111111111111');
  await until('the refusal', async () => (await text()).includes('This sandbox only accepts test card numbers'));
  if (await evaluate('document.querySelector("input[name=card_number]").value') !== '') throw new Error('the number came back in the page');
  if ((await session_of(s.id)).payment_intent !== null) throw new Error('a payment was made');
  const state = await (await fetch(`${BASE}/_dev/state`)).text();
  if (state.includes('4111111111111111')) throw new Error('the number is in the sandbox\'s state');
  return 'refused, not echoed, not kept';
});

await check('card-3ds', async () => {
  const s = await session('card', 'hosted_page');
  await goto(s.url);
  await payCard('4000002500003155');
  await waitForUrl(`${BASE}/stripe/3ds/`);
  if (!(await text()).includes('Test bank')) throw new Error('no bank step');
  await click('button[value=complete]');
  await waitForUrl(`${APP}/return/ok`);
  if ((await session_of(s.id)).payment_status !== 'paid') throw new Error('not paid after the bank step');
  return 'completed at the bank step';
});

await check('fields-bridge', async () => {
  const s = await session('card', 'elements');
  const fragment = `pk=${encodeURIComponent(STRIPE.publishable_key)}&cs=${encodeURIComponent(s.client_secret)}&locale=en`;
  await goto(`${STRIPE.fields_page}#${fragment}`);
  await until('ready over the bridge', async () => (await bridge()).includes('ready'));
  await type('input[name=card_number]', '4242424242424242');
  await type('input[name=exp]', '1234');
  await type('input[name=cvc]', '123');
  await until('brand and complete over the bridge', async () => { const b = await bridge(); return b.includes('brand') && b.includes('complete'); });
  await switchOn('three_ds_next');
  await evaluate(`location.hash = ${JSON.stringify(`${fragment}&cmd=confirm&name=Test+Payer&n=1`)}`);
  await until('the 3-D Secure step in the page', async () => evaluate('!document.getElementById("sca").hidden'));
  const answers = (b) => b.filter((m) => m === 'result').length;
  await until('the step announced over the bridge', async () => { const b = await bridge(); return answers(b) === 1 && b.includes('height'); });
  await click('#sca button[value=complete]');
  await until('the second result over the bridge', async () => answers(await bridge()) === 2);
  if ((await session_of(s.id)).payment_status !== 'paid') throw new Error('not paid after the step');
  return (await bridge()).join(', ');
});

await check('sepa-mandate', async () => {
  const before = received.length;
  const at = await flow();
  await goto(at);
  await type('input[name=holder]', 'Test Holder');
  await type('input[name=iban]', 'DE89370400440532013000');
  await click('form.iban button');
  await until('the mandate page', async () => (await text()).includes('Set up the mandate'));
  await click('form button');
  await until('the bank step', async () => (await text()).includes('Test bank'));
  await click('button[value=confirm]');
  await waitForUrl(`${APP}/return/ok`);
  const batch = await until('the signed batch', () => received.slice(before).find((r) => r.url === '/hooks/fake-gocardless'
    && JSON.parse(r.body).events.some((e) => e.resource_type === 'billing_requests' && e.action === 'fulfilled')));
  const want = createHmac('sha256', GC.webhook_secret).update(batch.body).digest('hex');
  if (want !== batch.headers['webhook-signature']) throw new Error('a GoCardless-shaped batch whose signature does not verify');
  return 'fulfilled, signed';
});

await check('looks-like-test', async () => {
  const s = await session('card', 'hosted_page');
  for (const at of [`${BASE}/`, s.url, STRIPE.fields_page]) {
    await goto(at);
    const title = await evaluate('document.title');
    const body = await text();
    if (!title.startsWith('[TEST]')) throw new Error(`${at}: title ${title}`);
    if (!body.includes('TEST - local fake') || !body.includes('TEST - NOT A REAL PAYMENT PAGE')) throw new Error(`${at}: no ribbon or watermark`);
  }
  const port = Number(new URL(BASE).port);
  const status = await new Promise((ok, fail) => {
    const r = httpRequest({ host: '127.0.0.1', port, path: '/health', headers: { host: 'evil.example' } }, (res) => { res.resume(); ok(res.statusCode); });
    r.on('error', fail);
    r.end();
  });
  if (status !== 403) throw new Error(`another Host answered ${status}`);
  return 'titles, ribbon, watermark, Host refused';
});

// ---- the local methods: one page per flow, documented test data only ----
const localSession = async (method, currency, extra = {}) => {
  const ref = `browser-local-${++made}`;
  const body = {
    mode: 'payment', ui_mode: 'hosted_page', client_reference_id: ref, 'metadata[provider_ref]': ref,
    'line_items[0][quantity]': '1', 'line_items[0][price_data][currency]': currency,
    'line_items[0][price_data][unit_amount]': '5999', 'line_items[0][price_data][product_data][name]': 'Test product',
    'payment_method_types[0]': method, 'payment_intent_data[metadata][provider_ref]': ref,
    expires_at: String(Math.floor(Date.now() / 1000) + 3600),
    success_url: `${APP}/return/ok`, cancel_url: `${APP}/return/cancel`, ...extra,
  };
  const r = await fetch(`${STRIPE.api}/v1/checkout/sessions`, {
    method: 'POST',
    headers: { authorization: `Bearer ${STRIPE.secret_key}`, 'content-type': 'application/x-www-form-urlencoded', 'idempotency-key': ref },
    body: form(body),
  });
  if (r.status !== 200) throw new Error(`session: ${r.status} ${await r.text()}`);
  return r.json();
};

await check('ideal-test-bank', async () => {
  const s = await localSession('ideal', 'eur');
  const before = received.length;
  await goto(s.url);
  const body = await text();
  if (!body.includes('Test Bank A')) throw new Error('no test bank list');
  await click('button[value=authorize]');
  await waitForUrl(`${APP}/return/ok`);
  const types = await until('the signed webhooks', () => { const t = stripeEvents(before); return t.includes('payment_intent.succeeded') && t; });
  return types.join(', ');
});

await check('blik-code-then-phone', async () => {
  const s = await localSession('blik', 'eur', { 'adaptive_pricing[enabled]': 'true' });
  const before = received.length;
  await goto(s.url);
  if (!(await text()).includes('fake rate')) throw new Error('the converted price is not marked as a fake rate');
  await type('input[name=code]', '123456');
  await click('form button');
  await waitForUrl(`${BASE}/stripe/phone/`);
  await click('button[value=authorize]');
  const types = await until('the signed webhooks', () => { const t = stripeEvents(before); return t.includes('payment_intent.succeeded') && t; });
  return types.join(', ');
});

await check('twint-qr-is-loopback-and-the-page-follows-the-phone', async () => {
  const s = await localSession('twint', 'chf');
  await goto(s.url);
  const code = await evaluate("(document.querySelector('.qr code') || {}).textContent || ''");
  if (!code.startsWith(`${BASE}/stripe/phone/`)) throw new Error(`the QR names ${code}`);
  // the payer's phone, standing in: the desktop page polls and goes on by itself
  const r = await fetch(code, { method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: form({ action: 'authorize' }) });
  if (r.status !== 200) throw new Error(`the fake phone answered ${r.status}`);
  await waitForUrl(`${APP}/return/ok`);
  return 'approved on the phone, the page followed';
});

await check('multibanco-voucher', async () => {
  const s = await localSession('multibanco', 'eur');
  const before = received.length;
  await goto(s.url);
  await type('input[name=email]', 'succeed_immediately@example.com');
  await click('form button');
  await until('the voucher', async () => (await text()).includes('Reference'));
  const types = await until('the timeline', () => { const t = stripeEvents(before); return t.includes('checkout.session.async_payment_succeeded') && t; }, 20000);
  return types.join(', ');
});

await check('undocumented-test-data-refused', async () => {
  const s = await localSession('blik', 'pln');
  await goto(s.url);
  await type('input[name=code]', '654321');
  await click('form button');
  await until('the refusal', async () => (await text()).includes('only accepts the documented test data'));
  if ((await text()).includes('654321')) throw new Error('the typed code is echoed');
  return 'refused, not echoed';
});

// ---- done ----
cdp.ws.close();
receiver.close();
const failed = results.filter((ok) => !ok).length;
console.log(`${results.length - failed} passed, ${failed} failed`);
await stop(failed);
