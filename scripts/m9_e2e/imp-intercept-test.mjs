// imp-intercept-test.mjs <az_indirect_dispatch.ll> [--keep]
//
// Behavioural test for the IAT-import intercepts the transpiler emits into the
// indirect-call dispatcher (see doc/web-iat-import-dispatch.md).
//
// It reads the REAL dispatcher IR the transpiler wrote — every lift leaves it
// at <scratch>/az_indirect_dispatch.ll when AZ_REMILL_KEEP_SCRATCH=1 — extracts
// just the `imp<label>:` blocks, links them into a minimal standalone module,
// and drives each one with a known State. Reading the emitted file rather than
// a copy of it is the point: a test that re-implements what it checks passes
// happily after the emitter drifts.
//
// Why behavioural and not "it compiles": the failure that bit process_heap_alloc
// twice was reading a right-looking argument from the wrong register. That
// compiles perfectly and silently returns nonsense, so every assertion below
// pins which ARGUMENT an intercept reads, not just that it runs.
//
//   node scripts/m9_e2e/imp-intercept-test.mjs \
//       "$TEMP/azul-web-transpiler-<pid>/az_indirect_dispatch.ll"
import fs from 'fs';
import path from 'path';
import os from 'os';
import { execFileSync } from 'child_process';

const LL = process.argv[2];
const KEEP = process.argv.includes('--keep');
if (!LL || !fs.existsSync(LL)) {
    console.error('usage: imp-intercept-test.mjs <az_indirect_dispatch.ll>');
    console.error('  (produced by any lift run with AZ_REMILL_KEEP_SCRATCH=1)');
    process.exit(2);
}
const TOOLS = path.resolve('third_party/remill/dependencies/install/bin');
const tool = n => path.join(TOOLS, n + (process.platform === 'win32' ? '.exe' : ''));

// ---- extract the imp blocks -------------------------------------------------
const src = fs.readFileSync(LL, 'utf8').split('\n');
const blocks = new Map();          // entry label -> block text (may be several BBs)
let cur = null, buf = [];
const flush = () => { if (cur) blocks.set(cur, buf.join('\n')); cur = null; buf = []; };
for (const line of src) {
    const m = /^([A-Za-z_][\w.]*):/.exec(line);
    if (m) {
        // Entry blocks are `imp<hex>`; continuations are `imp_<word>_<hex>` and
        // belong to the entry above them. The emitter uses that underscore form
        // precisely so the two are distinguishable — b/c/d/e/a/f are hex digits,
        // so a bare `impd<hex>` would otherwise read as entry `d<hex>` and the
        // block would be torn off its predecessor (which is how this test first
        // failed: extracted fragments, dominance errors, a valid module blamed).
        const isEntry = /^imp[0-9a-f]+$/.test(m[1]);
        const isCont = m[1].startsWith('imp_');
        if (isEntry) { flush(); cur = m[1].slice(3); buf = [line]; continue; }
        if (!isCont) { flush(); continue; }
    }
    if (cur) buf.push(line);
}
flush();
if (blocks.size === 0) {
    console.error(`no imp<label> blocks in ${LL} — this build routed no IAT imports.`);
    console.error('Check the transpiler log for "IAT import ... routed" lines.');
    process.exit(1);
}
console.log(`extracted ${blocks.size} intercept block(s) from ${path.basename(LL)}`);

// ---- rebuild a standalone module --------------------------------------------
let ir = 'target datalayout = "e-m:e-p:32:32-p10:8:8-p20:8:8-i64:64-n32:64-S128-ni:1:10:20"\n';
ir += 'target triple = "wasm32-unknown-unknown"\n';
ir += 'declare i8 @__remill_read_memory_8(ptr, i64)\n';
ir += 'declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)\n';
ir += 'declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n';
ir += 'declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)\n';
ir += 'define ptr @__az_indirect_dispatch(ptr %state, i64 %pc, ptr %memory) {\nentry:\n';
ir += '  %pcm = and i64 %pc, 4294967295\n  switch i64 %pcm, label %unk [\n';
for (const l of blocks.keys()) ir += `    i64 ${parseInt(l, 16)}, label %imp${l}\n`;
ir += '  ]\n';
for (const b of blocks.values()) ir += b + '\n';
ir += 'unk:\n  ret ptr %memory\n}\n';

const out = fs.mkdtempSync(path.join(os.tmpdir(), 'az-imptest-'));
const p = n => path.join(out, n);
fs.writeFileSync(p('m.ll'), ir);
try {
    execFileSync(tool('opt'), ['-O2', '-S', p('m.ll'), '-o', p('m.opt.ll')], { stdio: 'pipe' });
    execFileSync(tool('llc'), ['-mtriple=wasm32-unknown-unknown', '-filetype=obj', '-O2',
        '-o', p('m.o'), p('m.opt.ll')], { stdio: 'pipe' });
    execFileSync(tool('wasm-ld'), ['--no-entry', '--allow-undefined',
        '--export=__az_indirect_dispatch', '--export-memory',
        '--initial-memory=1048576', '-o', p('m.wasm'), p('m.o')], { stdio: 'pipe' });
} catch (e) {
    console.error('build failed:', String(e.stderr || e.message).slice(0, 800));
    process.exit(1);
}

// ---- drive it ---------------------------------------------------------------
const ARG = [2248, 2264, 2344, 2360];   // RCX, RDX, R8, R9
const RET = 2216;                       // RAX
const CURSOR = 262176;                  // bump cursor, u32
const STATE = 0x50000;                  // clear of the runtime region

let memory;
const dv = () => new DataView(memory.buffer);
const bytes = () => new Uint8Array(memory.buffer);
const setArg = (i, v) => dv().setBigUint64(STATE + ARG[i], BigInt(v), true);
const getRet = () => dv().getBigUint64(STATE + RET, true);
const retI64 = () => BigInt.asIntN(64, getRet());
const setCursor = v => dv().setUint32(CURSOR, v, true);
const getCursor = () => dv().getUint32(CURSOR, true);

const env = {
    __remill_read_memory_8: (m, a) => bytes()[Number(a)],
    // llc lowers the llvm intrinsics to these libcalls; the loader and every
    // probe harness already provide them, so no new env surface is introduced.
    memset: (d, c, n) => { bytes().fill(c & 0xFF, d, d + n); return d; },
    memcpy: (d, s, n) => { bytes().copyWithin(d, s, s + n); return d; },
    memmove: (d, s, n) => { bytes().copyWithin(d, s, s + n); return d; },
};
const { instance } = await WebAssembly.instantiate(fs.readFileSync(p('m.wasm')), { env });
memory = instance.exports.memory;
const labels = [...blocks.keys()].map(l => parseInt(l, 16));
const dispatch = l => instance.exports.__az_indirect_dispatch(STATE, BigInt(l), 0);

let pass = 0, fail = 0;
const check = (name, got, want) => {
    const ok = got === want;
    console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name}: got ${got}, want ${want}`);
    ok ? pass++ : fail++;
};

// Identify each block by behaviour — the label is a runtime address that
// differs in every process, so it cannot be hard-coded.
//
// Probing has to key on OBSERVABLE EFFECTS, not on the return value alone:
// mem{cpy,move,set} return dst, which is neither 0 nor 1, so a return-only
// probe files them next to GetProcessHeap and they end up with a single
// "non-zero" assertion instead of being tested at all.
const PROBE_DST = 0x48000, PROBE_SRC = 0x48100;
const classify = (l) => {
    // 1. Does it move the bump cursor?
    setCursor(0x20000);
    bytes().set([0xC1, 0xC2, 0xC3, 0xC4], PROBE_DST);          // canary
    bytes().set([0x11, 0x22, 0x33, 0x44], PROBE_SRC);
    setArg(0, PROBE_DST); setArg(1, PROBE_SRC); setArg(2, 4); setArg(3, 4);
    dv().setBigUint64(STATE + RET, 0n, true);
    dispatch(l);
    if (getCursor() !== 0x20000) return 'alloc-like';
    // 2. Did it write through dst? Only memset/memmove do.
    if (bytes()[PROBE_DST] !== 0xC1) {
        return bytes()[PROBE_DST] === 0x11 ? 'memmove' : 'memset';
    }
    const r1 = getRet();
    // 3. Does the return depend on the buffers? Only memcmp does; HeapFree and
    //    GetProcessHeap both answer with a constant.
    bytes().set([0x11, 0x22, 0x33, 0x44], PROBE_DST);
    setArg(0, PROBE_DST); setArg(1, PROBE_SRC); setArg(2, 4);
    dispatch(l);
    if (getRet() !== r1) return 'memcmp';
    return Number(r1 & 0xFFFFFFFFn) === 1 ? 'HeapFree' : 'ProcessHeap';
};
const kinds = new Map();
for (const l of labels) kinds.set(l, classify(l));
console.log('block kinds: ' + [...kinds.values()].join(', '));

for (const [l, kind] of kinds) {
    if (kind === 'ProcessHeap') {
        console.log('GetProcessHeap');
        dispatch(l);
        check('returns a non-zero handle', getRet() !== 0n, true);
    } else if (kind === 'HeapFree') {
        console.log('HeapFree(hHeap, flags, ptr)');
        setCursor(0x10000);
        setArg(0, 0xA20000); setArg(1, 0); setArg(2, 0x12340);
        dispatch(l);
        check('returns TRUE', Number(getRet()), 1);
        check('bump never frees: cursor untouched', getCursor(), 0x10000);
    } else if (kind === 'alloc-like') {
        // Distinguish HeapAlloc from HeapReAlloc: realloc copies from ARG[2].
        const src2 = 0x40000;
        bytes().set([1, 2, 3, 4, 5, 6, 7, 8], src2);
        setCursor(0x41000);
        setArg(0, 0xA20000); setArg(1, 0); setArg(2, src2); setArg(3, 8);
        dispatch(l);
        const nb = Number(getRet());
        const copied = bytes()[nb] === 1 && bytes()[nb + 7] === 8;
        if (copied) {
            console.log('HeapReAlloc(hHeap, flags, lpMem, dwBytes) — ptr ARG[2], size ARG[3]');
            check('returns a fresh block', nb, 0x41000);
            check('copies the old payload', bytes()[nb] * 100 + bytes()[nb + 7], 108);
            setCursor(0x42000);
            setArg(2, 0); setArg(3, 16);
            dispatch(l);
            check('null old pointer allocates, does not copy from 0', Number(getRet()), 0x42000);
        } else {
            console.log('HeapAlloc(hHeap, flags, dwBytes) — size is ARG[2], NOT ARG[0]');
            setCursor(0x20000);
            // ARG[0] holds a heap handle: reading it as a size is the exact
            // misclassification that produced zero-byte allocations.
            setArg(0, 0xA20000); setArg(1, 0); setArg(2, 200);
            dispatch(l);
            check('returns the old cursor', Number(getRet()), 0x20000);
            check('advances the cursor by the size', getCursor(), 0x20000 + 200);
            bytes()[0x30000] = 0xAB;
            setCursor(0x30000);
            setArg(2, 16);
            dispatch(l);
            check('zeroes the returned block', bytes()[0x30000], 0);
        }
    } else if (kind === 'memcmp') {
        console.log('memcmp(a, b, n)');
        const A = 0x45000, B = 0x45100;
        const wr = (at, s) => bytes().set([...s].map(c => c.charCodeAt(0)), at);
        wr(A, 'export_path'); wr(B, 'export_path');
        setArg(0, A); setArg(1, B); setArg(2, 11);
        dispatch(l);
        check('equal buffers compare 0', Number(getRet()), 0);
        wr(B, 'exportedXXX');
        setArg(0, A); setArg(1, B); setArg(2, 11);
        dispatch(l);
        check('different buffers compare non-zero', getRet() !== 0n, true);
        // The SIGN matters, not just non-zero: serde's field lookup and
        // BTreeMap ordering both branch on it, and a sign-flipped memcmp
        // silently mis-sorts instead of failing.
        wr(A, 'a'); wr(B, 'b');
        setArg(0, A); setArg(1, B); setArg(2, 1);
        dispatch(l);
        check("'a' vs 'b' is negative", retI64() < 0n, true);
        setArg(0, B); setArg(1, A); setArg(2, 1);
        dispatch(l);
        check("'b' vs 'a' is positive", retI64() > 0n, true);
        // A difference past the compared length must NOT be seen.
        wr(A, 'abcZ'); wr(B, 'abcQ');
        setArg(0, A); setArg(1, B); setArg(2, 3);
        dispatch(l);
        check('stops at n', Number(getRet()), 0);
        setArg(2, 0);
        dispatch(l);
        check('zero length compares equal', Number(getRet()), 0);
    } else if (kind === 'memmove') {
        console.log('memmove/memcpy(dst, src, n)');
        bytes().set([9, 8, 7, 6], 0x46000);
        bytes().fill(0, 0x46100, 0x46110);
        setArg(0, 0x46100); setArg(1, 0x46000); setArg(2, 4);
        dispatch(l);
        check('copies the payload', bytes()[0x46100] * 10 + bytes()[0x46103], 96);
        check('returns dst', Number(getRet()), 0x46100);
        check('does not write past n', bytes()[0x46104], 0);
        // Overlapping forward move must not corrupt — memcpy is routed to the
        // memmove helper precisely so overlap is safe.
        bytes().set([1, 2, 3, 4], 0x46200);
        setArg(0, 0x46202); setArg(1, 0x46200); setArg(2, 4);
        dispatch(l);
        check('overlapping move preserves the source bytes',
            bytes()[0x46202] * 10 + bytes()[0x46205], 14);
    } else {
        console.log('memset(dst, c, n)');
        bytes().fill(0, 0x46300, 0x46310);
        setArg(0, 0x46300); setArg(1, 0x5A); setArg(2, 3);
        dispatch(l);
        check('fills with the byte', bytes()[0x46300], 0x5A);
        check('fills exactly n bytes', bytes()[0x46302] * 1000 + bytes()[0x46303], 0x5A * 1000);
        check('returns dst', Number(getRet()), 0x46300);
        // Only the low byte of the fill value is used.
        setArg(0, 0x46300); setArg(1, 0xFF41); setArg(2, 2);
        dispatch(l);
        check('uses only the low byte of c', bytes()[0x46300], 0x41);
    }
}

if (!KEEP) fs.rmSync(out, { recursive: true, force: true });
console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
