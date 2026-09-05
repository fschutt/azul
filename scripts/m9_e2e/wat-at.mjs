// wat-at.mjs — decode the wasm instruction at a MODULE-RELATIVE byte offset.
//
// V8 reports traps as `wasm-function[N]:0xOFFSET`, where OFFSET is the byte
// offset into the whole module. There is no name section in azul-mini (it is
// --strip-all'd) and no wasm-objdump on this box, so this decodes just enough
// of the code section to answer the only question that matters for an OOB:
// WHICH memory opcode trapped, and what static offset immediate does it carry?
//
// A huge/garbage `offset=` immediate means the base pointer is fine but the
// field offset is wrong; a sane `offset=` means the BASE was garbage.
//
// Usage: node wat-at.mjs <file.wasm> <hexOffset> [contextBytes]
import { readFileSync } from 'node:fs';

const [file, offStr, ctxStr] = process.argv.slice(2);
const buf = readFileSync(file);
const target = parseInt(offStr, 16);
const CTX = parseInt(ctxStr || '48', 10);

// --- minimal LEB readers -------------------------------------------------
let p = 0;
function u32() { let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; }

// Locate the code section (id 10) so we can bound the search.
p = 8;
let codeStart = -1, codeEnd = -1;
while (p < buf.length) {
    const id = buf[p++]; const size = u32(); const start = p;
    if (id === 10) { codeStart = start; codeEnd = start + size; break; }
    p = start + size;
}
if (codeStart < 0) { console.log('no code section'); process.exit(1); }
console.log(`code section: 0x${codeStart.toString(16)} .. 0x${codeEnd.toString(16)}`);
if (target < codeStart || target >= codeEnd) console.log('WARNING: target outside code section');

// Memory opcodes: name + whether it takes align/offset immediates.
const MEMOPS = {
    0x28: 'i32.load', 0x29: 'i64.load', 0x2a: 'f32.load', 0x2b: 'f64.load',
    0x2c: 'i32.load8_s', 0x2d: 'i32.load8_u', 0x2e: 'i32.load16_s', 0x2f: 'i32.load16_u',
    0x30: 'i64.load8_s', 0x31: 'i64.load8_u', 0x32: 'i64.load16_s', 0x33: 'i64.load16_u',
    0x34: 'i64.load32_s', 0x35: 'i64.load32_u',
    0x36: 'i32.store', 0x37: 'i64.store', 0x38: 'f32.store', 0x39: 'f64.store',
    0x3a: 'i32.store8', 0x3b: 'i32.store16', 0x3c: 'i64.store8', 0x3d: 'i64.store16',
    0x3e: 'i64.store32',
};

const op = buf[target];
console.log(`\nbyte @0x${target.toString(16)} = 0x${op.toString(16)}  ${MEMOPS[op] ? '→ ' + MEMOPS[op] : '(not a plain load/store opcode)'}`);
if (MEMOPS[op]) {
    p = target + 1;
    const align = u32();
    const offset = u32();
    console.log(`  align=2^${align}  offset=${offset} (0x${offset.toString(16)})`);
    console.log(offset > 0x10000
        ? '  ⇒ LARGE static offset: the STRUCT FIELD offset is wrong (base may be fine)'
        : '  ⇒ small static offset: the BASE ADDRESS on the stack was garbage');
}

// Raw context so the preceding address computation is visible.
const lo = Math.max(0, target - CTX), hi = Math.min(buf.length, target + CTX);
let out = '';
for (let i = lo; i < hi; i++) {
    if (i === target) out += '[';
    out += buf[i].toString(16).padStart(2, '0');
    out += (i === target) ? '] ' : ' ';
}
console.log(`\ncontext 0x${lo.toString(16)}..0x${hi.toString(16)}:\n${out}`);
