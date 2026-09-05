// extradata.mjs — rebuild the `--extra_data` the transpiler passes to remill for one
// function, so a lift can be reproduced locally (see relift-one.sh for why).
//
// Mirrors transpiler_remill.rs's build_extra_data: every RIP-relative target the
// function references gets a window mirrored at its SYNTH address. We use the
// LEA_MIRROR_WINDOW (128 B) for all of them, which is a superset of the per-operand
// memory_size the transpiler uses for non-lea operands.
//
// Usage: node extradata.mjs <dll> <asmfile> <outfile>
import { readFileSync, writeFileSync } from 'node:fs';

const [dll, asmFile, outFile] = process.argv.slice(2);
const buf = readFileSync(dll);
const asm = readFileSync(asmFile, 'utf8');

// PE section map (from llvm-readobj --sections). file = RVA - (VA - PointerToRawData).
const SECTIONS = [
    { lo: 0x1000, hi: 0x1000 + 0x1478a9a, delta: 0xc00 },      // .text
    { lo: 0x147a000, hi: 0x147a000 + 0xa067d0, delta: 0x1000 }, // .rdata
];
const SYNTH_BIAS = 0x10f000;
const WIN = parseInt(process.env.AZ_WIN || "128", 10);

const targets = new Set();
for (const m of asm.matchAll(/#\s+0x([0-9a-f]+)/g)) targets.add(parseInt(m[1], 16));

const regions = [];
for (const vma of [...targets].sort((a, b) => a - b)) {
    const rva = vma - 0x180000000;
    const sec = SECTIONS.find(s => rva >= s.lo && rva < s.hi);
    if (!sec) continue;
    const off = rva - sec.delta;
    if (off < 0 || off + WIN > buf.length) continue;
    regions.push((rva + SYNTH_BIAS).toString(16) + ':' + buf.slice(off, off + WIN).toString('hex'));
}
writeFileSync(outFile, regions.join(';'));
console.error(`riprel targets=${targets.size} regions=${regions.length}`);
