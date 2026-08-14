// wfunc.mjs — map a wasm function INDEX to its export name (and vice versa).
// Recreates the lost %TEMP%/wexport.mjs (untracked, cleared over the month gap).
// Usage:  node wfunc.mjs <file.wasm> <idx> [idx...]     → name for each index
//         node wfunc.mjs <file.wasm> --grep <substr>    → indices whose name matches
// Func index space = [imported funcs ..] then [defined funcs ..]; exports carry the
// absolute index. azul-mini.wasm is --strip-all'd (no name section) but EXPORT survives.
import { readFileSync } from 'node:fs';
const [file, ...args] = process.argv.slice(2);
const buf = readFileSync(file);
let p = 8; // skip magic+version
function u32() { let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; }
function name() { const n = u32(); const s = buf.toString('utf8', p, p + n); p += n; return s; }
let importFuncs = 0;
const exportsByIdx = new Map();
while (p < buf.length) {
  const id = buf[p++]; const size = u32(); const end = p + size;
  if (id === 2) { // import
    const count = u32();
    for (let i = 0; i < count; i++) { name(); name(); const kind = buf[p++]; if (kind === 0) { u32(); importFuncs++; } else if (kind === 1) { p++; const l = buf[p++]; if (l) u32(); u32(); } else if (kind === 2) { p++; const l = buf[p++]; if (l) u32(); u32(); } else if (kind === 3) { p++; buf[p++]; } }
  } else if (id === 7) { // export
    const count = u32();
    for (let i = 0; i < count; i++) { const nm = name(); const kind = buf[p++]; const idx = u32(); if (kind === 0) exportsByIdx.set(idx, nm); }
  }
  p = end;
}
if (args[0] === '--grep') {
  const sub = args[1];
  for (const [idx, nm] of [...exportsByIdx].sort((a, b) => a[0] - b[0])) if (nm.includes(sub)) console.log(idx + '\t' + nm);
} else {
  for (const a of args) {
    const idx = parseInt(a, 10);
    if (idx < importFuncs) { console.log(`func[${idx}] = IMPORT (index < ${importFuncs} imported funcs)`); continue; }
    const nm = exportsByIdx.get(idx);
    console.log(`func[${idx}] = ` + (nm || `(defined, not exported; ${importFuncs} imports)`));
  }
}
