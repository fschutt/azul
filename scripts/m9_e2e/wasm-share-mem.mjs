// wasm-share-mem.mjs — patch a wasm module's memory (internal AND imported) to be
// `shared`, so its backing buffer is a SharedArrayBuffer observable across worker
// threads. Used by marker-probe.mjs to read the lifted solver's progress markers
// LIVE while the synchronous solve hangs in a Worker.
//
// A pure BINARY patch (not a wabt wasm→wat→wasm round-trip): the lifted mini.wasm
// is ~31 MB and wabt OOMs on it. We only need to flip the memory limits flags to
// `shared` (0x03 = has_max | shared) and ensure a `maximum` is present — a tiny,
// surgical edit of the Memory section (id 5, internal mem) and the Import section
// (id 2, imported mem). The `shared` flag is independent of import/export, and a
// shared memory keeps its SharedArrayBuffer stable across memory.grow (a non-shared
// grow detaches/reallocates the ArrayBuffer → a cross-thread view goes stale).

function readU32Leb(buf, p) {
  let r = 0, s = 0, b;
  do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80);
  return [r >>> 0, p];
}
function writeU32Leb(v) {
  const a = []; v >>>= 0;
  do { let b = v & 0x7f; v >>>= 7; if (v) b |= 0x80; a.push(b); } while (v);
  return Buffer.from(a);
}

// Rewrite a memory_type (flags + limits) at `p` to `shared` with a maximum.
// Returns { bytes: Buffer(new memtype), consumed: oldByteLen }.
function patchMemType(buf, p, maxPages) {
  const start = p;
  let flags; [flags, p] = readU32Leb(buf, p);
  let min; [min, p] = readU32Leb(buf, p);
  let oldMax = 0;
  if (flags & 0x01) { [oldMax, p] = readU32Leb(buf, p); }
  const consumed = p - start;
  const newMax = Math.max(maxPages, min, oldMax);
  // 0x03 = has_max | shared
  const bytes = Buffer.concat([writeU32Leb(0x03), writeU32Leb(min), writeU32Leb(newMax)]);
  return { bytes, consumed, min, newMax, wasShared: (flags & 0x02) !== 0 };
}

function patchMemorySection(buf, start, end, maxPages) {
  let p = start, cnt; [cnt, p] = readU32Leb(buf, p);
  const out = [writeU32Leb(cnt)];
  let patched = 0;
  for (let i = 0; i < cnt; i++) {
    const m = patchMemType(buf, p, maxPages);
    out.push(m.bytes); p += m.consumed; patched++;
  }
  if (p < end) out.push(buf.slice(p, end)); // trailing (shouldn't exist)
  return { content: Buffer.concat(out), patched };
}

function patchImportSection(buf, start, end, maxPages) {
  let p = start, cnt; [cnt, p] = readU32Leb(buf, p);
  const out = [writeU32Leb(cnt)];
  let patched = 0;
  for (let i = 0; i < cnt; i++) {
    const impStart = p;
    let mlen; [mlen, p] = readU32Leb(buf, p); p += mlen;          // module name
    let flen; [flen, p] = readU32Leb(buf, p); p += flen;          // field name
    const kind = buf[p++];
    if (kind === 0) { let t; [t, p] = readU32Leb(buf, p); out.push(buf.slice(impStart, p)); }
    else if (kind === 1) { // table: reftype byte + limits
      p++; let lf; [lf, p] = readU32Leb(buf, p); let mn; [mn, p] = readU32Leb(buf, p);
      if (lf & 1) { let mx; [mx, p] = readU32Leb(buf, p); }
      out.push(buf.slice(impStart, p));
    }
    else if (kind === 2) { // memory: memtype → make shared
      const m = patchMemType(buf, p, maxPages);
      out.push(buf.slice(impStart, p), m.bytes); // names+kind byte, then new memtype
      p += m.consumed; patched++;
    }
    else if (kind === 3) { p += 2; out.push(buf.slice(impStart, p)); } // global: valtype + mut
    else { throw new Error('patchImportSection: unknown import kind ' + kind); }
  }
  if (p < end) out.push(buf.slice(p, end));
  return { content: Buffer.concat(out), patched };
}

export function patchSharedMemory(bytes, { maxPages = 16384 } = {}) {
  const buf = Buffer.from(bytes);
  if (buf.length < 8 || buf.readUInt32LE(0) !== 0x6d736100) throw new Error('not a wasm module');
  const pieces = [buf.slice(0, 8)];
  let p = 8, count = 0;
  while (p < buf.length) {
    const id = buf[p];
    let sp = p + 1, secSize; [secSize, sp] = readU32Leb(buf, sp);
    const contentStart = sp, contentEnd = sp + secSize;
    if (id === 5) {
      const r = patchMemorySection(buf, contentStart, contentEnd, maxPages);
      pieces.push(Buffer.from([5]), writeU32Leb(r.content.length), r.content);
      count += r.patched;
    } else if (id === 2) {
      const r = patchImportSection(buf, contentStart, contentEnd, maxPages);
      pieces.push(Buffer.from([2]), writeU32Leb(r.content.length), r.content);
      count += r.patched;
    } else {
      pieces.push(buf.slice(p, contentEnd));
    }
    p = contentEnd;
  }
  if (count === 0) throw new Error('patchSharedMemory: no memory (internal or imported) found');
  return { bytes: Buffer.concat(pieces), count };
}

// --- self-test: round-trips small hand-built modules and asserts the patched
// memory instantiates as shared (buffer is a SharedArrayBuffer). ---
if (process.argv[1] && process.argv[1].replace(/\\/g, '/').endsWith('wasm-share-mem.mjs')) {
  // (module (memory (export "memory") 2))  — internal, min only
  const internalMinOnly = Buffer.from([
    0x00,0x61,0x73,0x6d, 0x01,0x00,0x00,0x00,
    0x05,0x03, 0x01, 0x00,0x02,                          // memory sec: 1 mem, flags0 min2
    0x07,0x0a, 0x01, 0x06,0x6d,0x65,0x6d,0x6f,0x72,0x79, 0x02,0x00, // export "memory" mem0
  ]);
  // (module (import "env" "memory" (memory 1)))  — imported, min only
  const importedMinOnly = Buffer.from([
    0x00,0x61,0x73,0x6d, 0x01,0x00,0x00,0x00,
    0x02,0x0f, 0x01, 0x03,0x65,0x6e,0x76, 0x06,0x6d,0x65,0x6d,0x6f,0x72,0x79, 0x02, 0x00,0x01, // import env.memory mem flags0 min1 (content=15B)
  ]);
  for (const [label, src, imported] of [
    ['internal-min-only', internalMinOnly, false],
    ['imported-min-only', importedMinOnly, true],
  ]) {
    try {
      const { bytes, count } = patchSharedMemory(src, { maxPages: 64 });
      let ok, detail;
      if (imported) {
        const mem = new WebAssembly.Memory({ initial: 1, maximum: 64, shared: true });
        await WebAssembly.instantiate(bytes, { env: { memory: mem } });
        ok = mem.buffer instanceof SharedArrayBuffer; detail = 'import accepted shared mem';
      } else {
        const { instance } = await WebAssembly.instantiate(bytes, {});
        ok = instance.exports.memory.buffer instanceof SharedArrayBuffer;
        detail = 'exports.memory.buffer is SAB=' + ok;
      }
      console.log(`[${label}] patched count=${count} -> ${ok ? 'OK' : 'FAIL'} ${detail}`);
    } catch (e) { console.log(`[${label}] ERROR ${e.message}`); }
  }
}
