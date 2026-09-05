// Assertion evaluators: AZ_E2E assert_* ops against the live browser DOM +
// golden-PNG screenshot compare.
//
// Part of the web e2e harness (scripts/web-e2e-harness-plan.md §4.5 mapping
// table). Desktop reference implementations: full.rs:3340-3931. Defaults are
// kept identical where they exist (assert_layout tolerance 0.5;
// assert_screenshot threshold 2, max_diff_ratio 0.0, auto-baseline when the
// golden is missing — full.rs:3878-3888).

import fs from 'node:fs';
import path from 'node:path';
import { decodePng, pixelDiff, renderDiffPng } from './png.mjs';

export const ASSERT_OPS = new Set([
    'assert_text', 'assert_exists', 'assert_not_exists', 'assert_node_count',
    'assert_layout', 'assert_scroll', 'assert_css', 'assert_app_state',
    'assert_screenshot',
]);

/**
 * Map a scenario `reference` path onto the web golden tree.
 * "layout/tests/reference_images/contenteditable_overflow/01_initial.png"
 *   → <goldenDir>/contenteditable_overflow/01_initial.png
 * (desktop references are cpurender pixels — browser pixels get their own
 * parallel tree keyed by the same relative name; plan §4.5 "web-baseline").
 */
export function goldenPathFor(reference, goldenDir) {
    let rel = String(reference).replace(/\\/g, '/');
    const m = rel.match(/reference_images(?:_web)?\/(.+)$/);
    if (m) rel = m[1];
    else {
        rel = rel.replace(/^[a-zA-Z]:\//, '').replace(/^\/+/, '').replace(/\.\.\//g, '');
    }
    if (!rel.toLowerCase().endsWith('.png')) rel += '.png';
    return path.join(goldenDir, rel);
}

/**
 * Evaluate one assert_* step.
 *
 * @param op      the assert op name
 * @param params  flattened step params (incl. optional "x-web" overrides)
 * @param ctx {
 *   cdp,             — connected Cdp (page helpers installed)
 *   shot,            — PNG Buffer captured at this assertion point (or null)
 *   goldenDir, updateGolden,
 *   artifactDir,     — per-test dir for actual/diff artifacts
 *   stepPrefix,      — e.g. "03_assert_screenshot"
 *   log,
 * }
 * @returns {{status:'pass'|'fail'|'skip', message:string, actual?, extra?}}
 */
export async function runAssert(op, params, ctx) {
    const xweb = params['x-web'] || {};
    switch (op) {
        case 'assert_text': {
            const sel = String(params.selector ?? '');
            const expected = String(params.expected ?? '');
            const r = await ctx.cdp.eval(
                `(function(s){ var el = window.__azE2E.q(s);
                    if (!el) return { found: false };
                    return { found: true, text: (el.textContent || '').trim() };
                })(${JSON.stringify(sel)})`);
            if (!r.found) return { status: 'fail', message: `assert_text: selector "${sel}" matched nothing` };
            const pass = r.text === expected;
            return {
                status: pass ? 'pass' : 'fail',
                message: pass ? `text of "${sel}" == ${JSON.stringify(expected)}`
                    : `assert_text "${sel}": expected ${JSON.stringify(expected)}, got ${JSON.stringify(r.text)}`,
                actual: r.text,
            };
        }
        case 'assert_exists':
        case 'assert_not_exists': {
            const sel = String(params.selector ?? '');
            const n = await ctx.cdp.eval(`window.__azE2E.count(${JSON.stringify(sel)})`);
            const wantExist = op === 'assert_exists';
            const pass = wantExist ? n > 0 : n === 0;
            return {
                status: pass ? 'pass' : 'fail',
                message: pass ? `${op} "${sel}" ok (count=${n})`
                    : `${op} "${sel}": count=${n}`,
                actual: n,
            };
        }
        case 'assert_node_count': {
            const sel = String(params.selector ?? '');
            const expected = Number(params.expected);
            const n = await ctx.cdp.eval(`window.__azE2E.count(${JSON.stringify(sel)})`);
            const pass = n === expected;
            return {
                status: pass ? 'pass' : 'fail',
                message: pass ? `node count of "${sel}" == ${expected}`
                    : `assert_node_count "${sel}": expected ${expected}, got ${n}`,
                actual: n,
            };
        }
        case 'assert_layout': {
            const sel = String(params.selector ?? '');
            const prop = String(params.property ?? '');
            const expected = Number(params.expected);
            const tolerance = params.tolerance != null ? Number(params.tolerance) : 0.5; // desktop default, full.rs:3544+
            if (!['x', 'y', 'width', 'height'].includes(prop)) {
                return { status: 'fail', message: `assert_layout: unknown property "${prop}" (x|y|width|height)` };
            }
            const useWasm = xweb.source === 'wasm';
            const r = await ctx.cdp.eval(
                `(function(s, useWasm){ var H = window.__azE2E;
                    var el = H.q(s);
                    if (!el) return { found: false };
                    var info = H.elInfo(el);
                    var rect = info.rect, src = 'gBCR';
                    if (useWasm && info.azId != null) {
                        var wr = H.wasmRect(info.azId);
                        if (wr) { rect = { x: wr.x, y: wr.y, w: wr.w, h: wr.h }; src = 'wasm-rect'; }
                        else return { found: true, wasmMissing: true };
                    }
                    return { found: true, rect: rect, src: src };
                })(${JSON.stringify(sel)}, ${useWasm})`);
            if (!r.found) return { status: 'fail', message: `assert_layout: selector "${sel}" matched nothing` };
            if (r.wasmMissing) return { status: 'skip', message: `assert_layout "${sel}": x-web source=wasm but rect cache has no valid entry (solve gated?)` };
            const actual = { x: r.rect.x, y: r.rect.y, width: r.rect.w, height: r.rect.h }[prop];
            const pass = Math.abs(actual - expected) <= tolerance;
            return {
                status: pass ? 'pass' : 'fail',
                message: pass
                    ? `${prop} of "${sel}" == ${expected} ±${tolerance} (${r.src})`
                    : `assert_layout "${sel}".${prop}: expected ${expected} ±${tolerance}, got ${actual} (${r.src})`,
                actual,
            };
        }
        case 'assert_scroll': {
            const sel = String(params.selector ?? '');
            const tolerance = params.tolerance != null ? Number(params.tolerance) : 0.5;
            const r = await ctx.cdp.eval(
                `(function(s){ var el = window.__azE2E.q(s);
                    if (!el) return { found: false };
                    return { found: true, x: el.scrollLeft, y: el.scrollTop };
                })(${JSON.stringify(sel)})`);
            if (!r.found) return { status: 'fail', message: `assert_scroll: selector "${sel}" matched nothing` };
            const problems = [];
            if (params.x != null && Math.abs(r.x - Number(params.x)) > tolerance) {
                problems.push(`scrollLeft expected ${params.x}, got ${r.x}`);
            }
            if (params.y != null && Math.abs(r.y - Number(params.y)) > tolerance) {
                problems.push(`scrollTop expected ${params.y}, got ${r.y}`);
            }
            return {
                status: problems.length === 0 ? 'pass' : 'fail',
                message: problems.length === 0
                    ? `scroll of "${sel}" ok (x=${r.x}, y=${r.y})`
                    : `assert_scroll "${sel}": ${problems.join('; ')}`,
                actual: { x: r.x, y: r.y },
            };
        }
        case 'assert_css': {
            const sel = String(params.selector ?? '');
            const prop = String(params.property ?? '');
            const r = await ctx.cdp.eval(
                `window.__azE2E.cssCompare(${JSON.stringify(sel)}, ${JSON.stringify(prop)}, ${JSON.stringify(String(params.expected ?? ''))})`);
            if (r && r.found === false) return { status: 'fail', message: `assert_css: selector "${sel}" matched nothing` };
            if (!r || !r.ok) {
                // Desktop compares Rust Debug strings (full.rs:3650-3652) — un-normalizable
                // value forms can never match browser CSS; SKIP instead of lying (plan §4.5).
                return {
                    status: 'skip',
                    message: `assert_css "${sel}".${prop}: value not normalizable on web (${(r && r.reason) || 'unknown'}); actual=${JSON.stringify(r && r.actual)}`,
                    actual: r && r.actual,
                };
            }
            return {
                status: r.pass ? 'pass' : 'fail',
                message: r.pass ? `css ${prop} of "${sel}" matches`
                    : `assert_css "${sel}".${prop}: expected ${JSON.stringify(params.expected)}, got ${JSON.stringify(r.actual)}`,
                actual: r.actual,
            };
        }
        case 'assert_app_state': {
            return {
                status: 'skip',
                message: 'assert_app_state: no wasm-side state serializer export yet (plan §3.3) — SKIP on web',
            };
        }
        case 'assert_screenshot':
            return assertScreenshot(params, ctx, xweb);
        default:
            return { status: 'skip', message: `unknown assertion "${op}"` };
    }
}

function assertScreenshot(params, ctx, xweb) {
    if (!ctx.shot) return { status: 'fail', message: 'assert_screenshot: no screenshot captured' };
    const threshold = xweb.threshold ?? params.threshold ?? 2;           // desktop default (full.rs)
    const maxDiffRatio = xweb.max_diff_ratio ?? params.max_diff_ratio ?? 0.0;
    const mask = xweb.mask || [];
    const goldenPath = goldenPathFor(params.reference || `${ctx.stepPrefix}.png`, ctx.goldenDir);
    const actualPath = path.join(ctx.artifactDir, `${ctx.stepPrefix}_actual.png`);

    fs.mkdirSync(path.dirname(actualPath), { recursive: true });
    fs.writeFileSync(actualPath, ctx.shot);
    // Honor the scenario's save_actual by REMAPPING its basename into the
    // artifact dir (corpus files point at /tmp — not portable to Windows).
    if (params.save_actual) {
        const base = path.basename(String(params.save_actual).replace(/\\/g, '/'));
        fs.writeFileSync(path.join(ctx.artifactDir, base), ctx.shot);
    }

    const goldenExists = fs.existsSync(goldenPath);
    if (!goldenExists || ctx.updateGolden) {
        fs.mkdirSync(path.dirname(goldenPath), { recursive: true });
        fs.writeFileSync(goldenPath, ctx.shot);
        const what = goldenExists ? 'golden UPDATED' : 'baseline CREATED';
        ctx.log(`assert_screenshot: ${what}: ${goldenPath}`);
        return {
            status: 'pass',
            message: `assert_screenshot: ${what} at ${goldenPath}` +
                (goldenExists ? '' : ' (auto-baseline, matches desktop full.rs:3878-3888; rerun to compare)'),
            extra: { golden: goldenPath, baselineCreated: !goldenExists, goldenUpdated: goldenExists && ctx.updateGolden },
        };
    }

    let golden, actual;
    try {
        golden = decodePng(fs.readFileSync(goldenPath));
        actual = decodePng(ctx.shot);
    } catch (e) {
        return { status: 'fail', message: `assert_screenshot: PNG decode failed: ${e.message}` };
    }
    const diff = pixelDiff(golden, actual, { threshold, mask });
    if (!diff.dimensionsMatch) {
        return {
            status: 'fail',
            message: `assert_screenshot: dimensions differ — golden ${diff.expected.width}x${diff.expected.height}` +
                ` vs actual ${diff.actual.width}x${diff.actual.height} (${goldenPath})`,
            extra: { golden: goldenPath, actual: actualPath },
        };
    }
    const pass = diff.diffRatio <= maxDiffRatio;
    const stats = {
        golden: goldenPath, actual: actualPath,
        threshold, maxDiffRatio,
        diffCount: diff.diffCount, comparedPixels: diff.comparedPixels,
        maskedPixels: diff.maskedPixels, maxDelta: diff.maxDelta,
        diffRatio: Number(diff.diffRatio.toFixed(6)),
    };
    if (!pass) {
        const diffPath = path.join(ctx.artifactDir, `${ctx.stepPrefix}_diff.png`);
        try {
            fs.writeFileSync(diffPath, renderDiffPng(actual, diff, mask));
            stats.diff = diffPath;
        } catch (e) {
            ctx.log(`diff image write failed: ${e.message}`);
        }
        // Also drop the golden next to it for eyeballing.
        const expCopy = path.join(ctx.artifactDir, `${ctx.stepPrefix}_expected.png`);
        try { fs.copyFileSync(goldenPath, expCopy); stats.expected = expCopy; } catch { /* non-fatal */ }
    }
    return {
        status: pass ? 'pass' : 'fail',
        message: pass
            ? `assert_screenshot ok: diff_ratio ${stats.diffRatio} <= ${maxDiffRatio} (maxDelta ${diff.maxDelta})`
            : `assert_screenshot FAILED: diff_ratio ${stats.diffRatio} > ${maxDiffRatio}` +
              ` (${diff.diffCount}/${diff.comparedPixels} px differ > ${threshold}; maxDelta ${diff.maxDelta})`,
        extra: stats,
    };
}
