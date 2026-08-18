#!/usr/bin/env node
// azul "mini puppeteer" — web e2e harness entry point.
//
// Implements scripts/web-e2e-harness-plan.md (P1 + the P2 keyboard subset):
// runs the existing AZ_E2E JSON scenarios (tests/e2e/*.json, format:
// dll/src/desktop/shell2/common/debug_server/full.rs E2eTest) against the web
// backend in a real headless Edge/Chrome over raw-WebSocket CDP.
//
//   node scripts/e2e-web/run.mjs tests/e2e/hello_world_counter.json
//   node scripts/e2e-web/run.mjs tests/e2e --filter counter --update-golden
//
// Choices made where the plan/task left options open (details in README.md):
//   * Directory name: scripts/e2e-web/ (task brief) instead of the plan's
//     scripts/web_e2e/.
//   * PNG decode: pure-JS decoder on node:zlib (lib/png.mjs), NOT the
//     Chrome/OffscreenCanvas route — goldens must be comparable offline.
//   * Tab lifecycle: one fresh tab per SPEC FILE by default (desktop parity —
//     tests inside one file share app state, e.g. the contenteditable suite);
//     --isolate forces a fresh tab per named test.
//   * Golden policy: missing golden ⇒ saved + PASS with "baseline CREATED"
//     (mirrors desktop auto-baseline, full.rs:3878-3888); --update-golden
//     overwrites existing goldens. results.json marks created baselines so CI
//     can reject them.
//   * Settle: __az_settled when the loader serves it; fixed-delay fallback
//     (--settle-fallback-ms, default 500) when it is undefined (servers
//     started before 2026-08-17). Boot settle timeout fails the spec; per-step
//     settle timeout is recorded loudly on the step but does not abort.
//   * Page exceptions (Runtime.exceptionThrown, wasm frames tagged) fail the
//     test they occurred in; exceptions during boot fail the whole spec.
//
// Node 20 needs --experimental-websocket for the global WebSocket; this script
// re-execs itself with the flag automatically when the global is missing.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

// ---- WebSocket availability (node 20: behind --experimental-websocket) -----
if (typeof WebSocket === 'undefined') {
    const self = fileURLToPath(import.meta.url);
    const r = spawnSync(process.execPath, ['--experimental-websocket', self, ...process.argv.slice(2)],
        { stdio: 'inherit' });
    process.exit(r.status === null ? 2 : r.status);
}

const { checkBrowser, newTab, closeTab, Cdp, browserRecipe } = await import('./lib/cdp.mjs');
const { Driver, INPUT_OPS } = await import('./lib/driver.mjs');
const { ASSERT_OPS, runAssert } = await import('./lib/asserts.mjs');

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const sleep = ms => new Promise(r => setTimeout(r, ms));

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

function usage() {
    console.log(`azul web e2e harness — runs AZ_E2E JSON scenarios against the web backend over CDP

usage: node scripts/e2e-web/run.mjs <spec.json | dir-of-specs> [more specs...] [options]

options:
  --url <base>              app server base URL       (default http://127.0.0.1:8800)
  --cdp <base>              CDP HTTP endpoint         (default http://127.0.0.1:9222)
  --golden-dir <dir>        golden PNG tree           (default scripts/e2e-web/golden/)
  --out-dir <dir>           artifacts: screenshots, diffs, console logs, results.json
                                                      (default target/web_e2e/)
  --update-golden           overwrite existing goldens with this run's screenshots
  --filter <regex>          only run tests whose name matches
  --isolate                 fresh tab per named test (default: fresh tab per spec file)
  --settle-cap-ms <ms>      max wait for __az_pending==0 per settle (default 5000)
  --settle-fallback-ms <ms> fixed delay when __az_settled is undefined (default 500)
  --boot-timeout-ms <ms>    max wait for loader bootstrap (default 30000)
  --help

exit codes: 0 all pass, 1 any test failed, 2 infrastructure error (no browser/server/spec)`);
}

function parseArgs(argv) {
    const opts = {
        specs: [],
        url: 'http://127.0.0.1:8800',
        cdp: 'http://127.0.0.1:9222',
        goldenDir: path.join(SCRIPT_DIR, 'golden'),
        outDir: path.join(SCRIPT_DIR, '..', '..', 'target', 'web_e2e'),
        updateGolden: false,
        filter: null,
        isolate: false,
        settleCapMs: 5000,
        settleFallbackMs: 500,
        bootTimeoutMs: 30_000,
    };
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i];
        const next = () => {
            if (i + 1 >= argv.length) { console.error(`missing value for ${a}`); process.exit(2); }
            return argv[++i];
        };
        switch (a) {
            case '--url': opts.url = next(); break;
            case '--cdp': opts.cdp = next(); break;
            case '--golden-dir': opts.goldenDir = path.resolve(next()); break;
            case '--out-dir': opts.outDir = path.resolve(next()); break;
            case '--update-golden': opts.updateGolden = true; break;
            case '--filter': opts.filter = new RegExp(next()); break;
            case '--isolate': opts.isolate = true; break;
            case '--settle-cap-ms': opts.settleCapMs = Number(next()); break;
            case '--settle-fallback-ms': opts.settleFallbackMs = Number(next()); break;
            case '--boot-timeout-ms': opts.bootTimeoutMs = Number(next()); break;
            case '--help': case '-h': usage(); process.exit(0); break;
            default:
                if (a.startsWith('--')) { console.error(`unknown option ${a}`); usage(); process.exit(2); }
                opts.specs.push(a);
        }
    }
    return opts;
}

function discoverSpecs(paths) {
    const out = [];
    for (const p of paths) {
        const abs = path.resolve(p);
        if (!fs.existsSync(abs)) { console.error(`spec path does not exist: ${abs}`); process.exit(2); }
        const st = fs.statSync(abs);
        if (st.isDirectory()) {
            const files = fs.readdirSync(abs).filter(f => f.toLowerCase().endsWith('.json')).sort();
            if (files.length === 0) console.error(`WARNING: no .json specs in ${abs}`);
            for (const f of files) out.push(path.join(abs, f));
        } else {
            out.push(abs);
        }
    }
    return out;
}

function loadSpec(file) {
    let json;
    try {
        json = JSON.parse(fs.readFileSync(file, 'utf8'));
    } catch (e) {
        throw new Error(`cannot parse ${file}: ${e.message}`);
    }
    // Desktop accepts Vec<E2eTest> or a single E2eTest (run.rs:71-125).
    const tests = Array.isArray(json) ? json : [json];
    for (const t of tests) {
        if (!t || typeof t.name !== 'string' || !Array.isArray(t.steps)) {
            throw new Error(`${file}: not AZ_E2E format (each test needs "name" + "steps"; ` +
                `the resize/tick AZ_E2E_TEST memory-rig format is not runnable here)`);
        }
    }
    return tests;
}

const sanitize = s => String(s).replace(/[^\w.-]+/g, '_').slice(0, 120);

// ---------------------------------------------------------------------------
// Per-test execution
// ---------------------------------------------------------------------------

async function runTest(driver, cdp, test, opts, specOut) {
    const t0 = Date.now();
    const testDir = path.join(specOut, sanitize(test.name));
    fs.mkdirSync(testDir, { recursive: true });
    const cfg = test.config || {};
    const stepResults = [];
    const notes = [];
    let failed = false;

    if (test.setup) {
        const dsf = (test.setup.dpi || 96) / 96;
        await driver.setMetrics(test.setup.window_width || 800, test.setup.window_height || 600, dsf);
        if (test.setup.app_state != null) {
            notes.push('setup.app_state SKIPPED: no set_app_state path on web (plan §3.3)');
        }
    }

    for (let i = 0; i < test.steps.length; i++) {
        const step = test.steps[i];
        const op = String(step.op || '');
        const { op: _op, screenshot: wantAfterShot, ...params } = step;
        const stepPrefix = `${String(i).padStart(2, '0')}_${sanitize(op)}`;
        const consoleMark = cdp.console.length;
        const excMark = cdp.exceptions.length;
        const s0 = Date.now();
        const xweb = params['x-web'] || {};

        const rec = { step_index: i, op, status: 'pass', duration_ms: 0, logs: [] };

        if (failed && !cfg.continue_on_failure) {
            rec.status = 'skip';
            rec.error = 'skipped: earlier step failed (continue_on_failure=false)';
            stepResults.push(rec);
            continue;
        }
        if (xweb.skip === true) {
            rec.status = 'skip';
            rec.error = 'skipped via "x-web": {"skip": true}';
            stepResults.push(rec);
            continue;
        }

        try {
            let result;
            if (ASSERT_OPS.has(op)) {
                // Assertion point ⇒ capture a screenshot (artifact always;
                // golden compare only for assert_screenshot).
                let shot = null;
                try { shot = await cdp.screenshot(); } catch (e) { notes.push(`screenshot at step ${i} failed: ${e.message}`); }
                if (shot && op !== 'assert_screenshot') {
                    fs.writeFileSync(path.join(testDir, `${stepPrefix}.png`), shot);
                    rec.screenshot = path.join(testDir, `${stepPrefix}.png`);
                }
                result = await runAssert(op, params, {
                    cdp, shot,
                    goldenDir: opts.goldenDir,
                    updateGolden: opts.updateGolden,
                    artifactDir: testDir,
                    stepPrefix,
                    log: l => console.log(`    ${l}`),
                });
                if (op === 'assert_screenshot' && result.extra && result.extra.actual) {
                    rec.screenshot = result.extra.actual;
                }
            } else {
                result = await driver.runStep(op, params);
                if (INPUT_OPS.has(op)) {
                    const settle = await driver.settle(op);
                    rec.settle = settle;
                    if (settle.mode === 'timeout') {
                        rec.logs.push(`SETTLE TIMEOUT after ${op}: __az_pending stuck > 0 for ${driver.settleCapMs} ms`);
                    }
                }
                if (xweb.settle_ms) await sleep(Number(xweb.settle_ms));
                if (result.wantScreenshot || wantAfterShot === true) {
                    try {
                        const shot = await cdp.screenshot();
                        const p = path.join(testDir, `${stepPrefix}${wantAfterShot ? '_after' : ''}.png`);
                        fs.writeFileSync(p, shot);
                        rec.screenshot = p;
                    } catch (e) { notes.push(`screenshot at step ${i} failed: ${e.message}`); }
                }
            }
            rec.status = result.status;
            if (result.message) rec.error = result.status === 'fail' ? result.message : undefined;
            if (result.message && result.status !== 'fail') rec.logs.push(result.message);
            if (result.actual !== undefined) rec.actual = result.actual;
            if (result.response !== undefined) rec.response = result.response;
            if (result.extra !== undefined) rec.extra = result.extra;
        } catch (e) {
            rec.status = 'fail';
            rec.error = `${op}: ${e.message}`;
        }

        // Page exceptions thrown during this step fail it (wasm frames included).
        const newExc = cdp.exceptions.slice(excMark);
        if (newExc.length > 0) {
            rec.status = 'fail';
            const msg = `page exception(s) during "${op}": ${newExc.map(x => x.text).join(' ;; ')}`;
            rec.error = rec.error ? `${rec.error} || ${msg}` : msg;
        }

        rec.logs.push(...cdp.console.slice(consoleMark).slice(0, 50).map(c => `[${c.kind}] ${c.text.slice(0, 300)}`));
        rec.duration_ms = Date.now() - s0;
        if (rec.status === 'fail') {
            failed = true;
            console.log(`    step ${i} ${op}: FAIL — ${rec.error}`);
        }
        stepResults.push(rec);
        if (cfg.delay_between_steps_ms) await sleep(Number(cfg.delay_between_steps_ms));
    }

    // Final screenshot (desktop E2eTestResult.final_screenshot analogue).
    let finalShot = null;
    try {
        const shot = await cdp.screenshot();
        finalShot = path.join(testDir, 'final.png');
        fs.writeFileSync(finalShot, shot);
    } catch { /* tab may be gone */ }

    const passed = stepResults.filter(s => s.status === 'pass').length;
    const failedN = stepResults.filter(s => s.status === 'fail').length;
    const skipped = stepResults.filter(s => s.status === 'skip').length;
    return {
        name: test.name,
        description: test.description,
        status: failedN > 0 ? 'fail' : 'pass',
        duration_ms: Date.now() - t0,
        step_count: stepResults.length,
        steps_passed: passed,
        steps_failed: failedN,
        steps_skipped: skipped,
        steps: stepResults,
        final_screenshot: finalShot,
        notes: notes.length ? notes : undefined,
    };
}

// ---------------------------------------------------------------------------
// Per-tab execution (one spec file's tests, or one isolated test)
// ---------------------------------------------------------------------------

async function runTestsInTab(specFile, tests, opts, specOut, tag) {
    const tab = await newTab(opts.cdp, 'about:blank');
    const cdp = new Cdp(tab.webSocketDebuggerUrl);
    const results = [];
    try {
        await cdp.connect();
        await cdp.send('Runtime.enable');
        await cdp.send('Page.enable');
        await cdp.send('Log.enable').catch(() => {});

        const driver = new Driver(cdp, {
            settleCapMs: opts.settleCapMs,
            settleFallbackMs: opts.settleFallbackMs,
            log: l => console.log(`    ${l}`),
        });
        const setup = tests[0].setup || {};
        await driver.setMetrics(setup.window_width || 800, setup.window_height || 600, (setup.dpi || 96) / 96);

        await cdp.send('Page.navigate', { url: opts.url });
        const boot = await driver.bootWait(opts.bootTimeoutMs);
        const bootExc = cdp.exceptions.slice();
        if (!boot.ok || bootExc.length > 0) {
            const reason = !boot.ok
                ? `page never became ready: ${boot.reason} (settle timeout / loader dead?)`
                : `uncaught page exception(s) during boot: ${bootExc.map(x => x.text).join(' ;; ')}`;
            console.error(`  BOOT FAILURE for ${tag}: ${reason}`);
            const tail = cdp.console.slice(-15).map(c => `    [${c.kind}] ${c.text.slice(0, 220)}`);
            if (tail.length) { console.error('  console tail:'); tail.forEach(l => console.error(l)); }
            for (const t of tests) {
                results.push({
                    name: t.name, status: 'fail', duration_ms: 0,
                    step_count: t.steps.length, steps_passed: 0,
                    steps_failed: t.steps.length, steps_skipped: 0,
                    steps: [], error: reason,
                });
            }
            return { results, boot, settleMode: boot.settle ? boot.settle.mode : null };
        }
        console.log(`  boot ok in ${boot.ms} ms (settle: ${boot.settle.mode}${boot.settle.mode === 'fallback' ? ` ${opts.settleFallbackMs}ms — loader has no __az_settled (pre-2026-08-17 server)` : ''})`);
        await driver.installHelpers();

        for (const test of tests) {
            if (opts.filter && !opts.filter.test(test.name)) continue;
            console.log(`  test ${test.name} ...`);
            const r = await runTest(driver, cdp, test, opts, specOut);
            console.log(`  test ${test.name} ... ${r.status === 'pass' ? 'ok' : 'FAILED'} (${r.duration_ms} ms)`);
            results.push(r);
        }
        return { results, boot, settleMode: boot.settle.mode };
    } finally {
        // Persist the whole tab's console + exceptions.
        try {
            const lines = [
                ...cdp.console.map(c => `[${new Date(c.ts).toISOString()}] [${c.kind}] ${c.text}`),
                ...(cdp.exceptions.length ? ['', '===== EXCEPTIONS ====='] : []),
                ...cdp.exceptions.map(x => `[${new Date(x.ts).toISOString()}] ${x.text}`),
            ];
            fs.mkdirSync(specOut, { recursive: true });
            fs.appendFileSync(path.join(specOut, 'console.log'), lines.join('\n') + '\n');
        } catch { /* artifacts are best-effort */ }
        cdp.close();
        await closeTab(opts.cdp, tab.id);
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

async function main() {
    const opts = parseArgs(process.argv.slice(2));
    if (opts.specs.length === 0) { usage(); process.exit(2); }
    const specFiles = discoverSpecs(opts.specs);

    // Infra check 1: browser on :9222 — loud recipe if absent.
    const ver = await checkBrowser(opts.cdp);
    if (!ver) {
        console.error(browserRecipe(opts.cdp));
        process.exit(2);
    }
    console.log(`browser: ${ver.Browser || 'unknown'} @ ${opts.cdp}`);

    // Infra check 2: app server. This harness NEVER starts servers itself —
    // cold lifts take minutes to an hour (plan §7 risk 7).
    try {
        const ctl = new AbortController();
        const t = setTimeout(() => ctl.abort(), 5000);
        await fetch(opts.url, { signal: ctl.signal });
        clearTimeout(t);
    } catch (e) {
        console.error(`Nothing answered on ${opts.url} (${e.message}).`);
        console.error('The azul web server is not running. Start the app under test yourself, e.g.:');
        console.error('  AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 REMILL_LIFT_BIN=... ./examples/c/hello-world.exe');
        console.error('(full recipe incl. depth env vars: scripts/m9_e2e/cdp_gate.sh:15-26; wait for "Listening on")');
        process.exit(2);
    }
    console.log(`server:  ${opts.url} answers`);

    fs.mkdirSync(opts.outDir, { recursive: true });
    const startedAt = new Date().toISOString();
    const specsOut = [];
    let anyFail = false;
    let sawFallbackSettle = false;

    for (const file of specFiles) {
        console.log(`spec ${file}`);
        const specBase = sanitize(path.basename(file, '.json'));
        const specOut = path.join(opts.outDir, specBase);
        fs.mkdirSync(specOut, { recursive: true });
        try { fs.rmSync(path.join(specOut, 'console.log')); } catch { /* first run */ }
        let tests;
        try {
            tests = loadSpec(file);
        } catch (e) {
            console.error(`  ${e.message}`);
            specsOut.push({ file, error: e.message, tests: [] });
            anyFail = true;
            continue;
        }
        const selected = opts.filter ? tests.filter(t => opts.filter.test(t.name)) : tests;
        if (selected.length === 0) {
            console.log('  (no tests match --filter)');
            specsOut.push({ file, tests: [] });
            continue;
        }

        const specResult = { file, consoleLog: path.join(specOut, 'console.log'), tests: [] };
        try {
            if (opts.isolate) {
                // Fresh tab per named test — page reload resets wasm app state,
                // so chained suites (contenteditable) will NOT behave as on
                // desktop; use the default per-file mode for those.
                for (const t of selected) {
                    const { results, settleMode } = await runTestsInTab(file, [t], opts, specOut, `${specBase}/${t.name}`);
                    specResult.tests.push(...results);
                    if (settleMode === 'fallback') sawFallbackSettle = true;
                }
            } else {
                const { results, settleMode } = await runTestsInTab(file, selected, opts, specOut, specBase);
                specResult.tests.push(...results);
                if (settleMode === 'fallback') sawFallbackSettle = true;
            }
        } catch (e) {
            console.error(`  spec crashed: ${e.message}`);
            specResult.error = e.message;
            anyFail = true;
        }
        if (specResult.tests.some(t => t.status === 'fail')) anyFail = true;
        specsOut.push(specResult);
    }

    // ---- results.json + summary board -------------------------------------
    const allTests = specsOut.flatMap(s => s.tests);
    const summary = {
        tests: allTests.length,
        passed: allTests.filter(t => t.status === 'pass').length,
        failed: allTests.filter(t => t.status === 'fail').length,
        steps_passed: allTests.reduce((a, t) => a + (t.steps_passed || 0), 0),
        steps_failed: allTests.reduce((a, t) => a + (t.steps_failed || 0), 0),
        steps_skipped: allTests.reduce((a, t) => a + (t.steps_skipped || 0), 0),
        baselines_created: allTests.reduce((a, t) =>
            a + (t.steps || []).filter(s => s.extra && s.extra.baselineCreated).length, 0),
    };
    const resultsPath = path.join(opts.outDir, 'results.json');
    fs.writeFileSync(resultsPath, JSON.stringify({
        startedAt, finishedAt: new Date().toISOString(),
        url: opts.url, cdp: opts.cdp,
        goldenDir: opts.goldenDir, outDir: opts.outDir,
        updateGolden: opts.updateGolden,
        specs: specsOut, summary,
    }, null, 2));

    console.log('');
    console.log('=== web-e2e summary ===');
    for (const t of allTests) {
        const mark = t.status === 'pass' ? 'PASS' : 'FAIL';
        const why = t.status === 'fail'
            ? ' — ' + (t.error || (t.steps.find(s => s.status === 'fail') || {}).error || 'see results.json').slice(0, 160)
            : '';
        console.log(`${mark} ${t.name} (${t.steps_passed}/${t.step_count} steps, ${t.duration_ms} ms` +
            (t.steps_skipped ? `, ${t.steps_skipped} skipped` : '') + `)${why}`);
    }
    console.log(`total: ${summary.tests} tests — ${summary.passed} passed, ${summary.failed} failed; ` +
        `steps ${summary.steps_passed}/${summary.steps_passed + summary.steps_failed + summary.steps_skipped} ` +
        `(${summary.steps_skipped} skipped)` +
        (summary.baselines_created ? `; ${summary.baselines_created} golden baseline(s) CREATED` : ''));
    console.log(`artifacts: ${opts.outDir}`);
    console.log(`results:   ${resultsPath}`);
    if (sawFallbackSettle) {
        console.log('NOTE: fixed-delay settle fallback was used — the server predates the ' +
            '__az_pending/__az_settled loader (restart it after a rebuild for deterministic settling).');
    }
    process.exit(anyFail ? 1 : 0);
}

main().catch(e => {
    console.error(`harness error: ${e.stack || e.message}`);
    process.exit(2);
});
