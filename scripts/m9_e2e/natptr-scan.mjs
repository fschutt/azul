// natptr-scan.mjs — scan a wasm module's DATA SEGMENTS for 8-byte-aligned
// little-endian values that are NATIVE host-image addresses.
//
// Any hit is a pointer the mirror copied but never translated native->synth:
// the lifted guest will wrap it to 32 bits (0x7FFA2Bxx_xxxx -> 0x2Bxxxxxx),
// which lands beyond the 512 MB linear memory and traps OOB on first deref —
// the exact shape of the rustc_entry ctrl-pointer trap.
//
// Usage: node natptr-scan.mjs <file.wasm> <imgBaseHex> <imgEndHex> [max]
import { readFileSync } from 'node:fs';

const [file, baseHex, endHex, maxArg] = process.argv.slice(2);
const buf = readFileSync(file);
const IMG_LO = BigInt('0x' + baseHex), IMG_HI = BigInt('0x' + endHex);
const MAX = parseInt(maxArg || '40', 10);

let p = 8;
function u32() { let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); return r >>> 0; }

let segCount = 0, hits = 0, segsWithHits = 0;
while (p < buf.length) {
    const id = buf[p++]; const size = u32(); const end = p + size;
    if (id === 11) { // data section
        const n = u32();
        for (let i = 0; i < n; i++) {
            const flags = u32();
            let memOff = -1;
            if (flags === 0 || flags === 2) {
                if (flags === 2) u32(); // memidx
                // offset expr: expect i32.const <LEB> end
                if (buf[p] === 0x41) { p++; let r = 0, s = 0, b; do { b = buf[p++]; r |= (b & 0x7f) << s; s += 7; } while (b & 0x80); memOff = r >>> 0; }
                while (buf[p] !== 0x0b) p++;
                p++;
            }
            const len = u32();
            const dstart = p;
            segCount++;
            let segHit = false;
            // Scan qwords aligned to the DESTINATION address (memOff), not the
            // file offset — pointers in guest structs are 8-aligned in memory.
            const align = memOff >= 0 ? ((8 - (memOff % 8)) % 8) : 0;
            for (let o = align; o + 8 <= len; o += 8) {
                const v = buf.readBigUInt64LE(dstart + o);
                if (v >= IMG_LO && v < IMG_HI) {
                    hits++;
                    segHit = true;
                    if (hits <= MAX) {
                        const slot = memOff >= 0 ? '0x' + (memOff + o).toString(16) : `seg${i}+0x${o.toString(16)}`;
                        console.log(`  slot ${slot}  value 0x${v.toString(16)}  (trunc 0x${(v & 0xFFFFFFFFn).toString(16)})`);
                    }
                }
            }
            if (segHit) segsWithHits++;
            p = dstart + len;
        }
    }
    p = end;
}
console.log(`\nsegments=${segCount}  untranslated-native-pointer hits=${hits} (in ${segsWithHits} segments)`);
