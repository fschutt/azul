// Minimal pure-JS PNG decode / encode / pixel-diff. No npm dependencies —
// inflate/deflate come from node:zlib.
//
// Part of the web e2e harness (scripts/web-e2e-harness-plan.md §4.1 "lib/png.js").
// Chosen option (plan left decoder choice open): a ~200-LOC native decoder here
// instead of round-tripping pixels through Chrome via OffscreenCanvas — golden
// files live on disk, so decoding must not require a live browser.
//
// Supports what Chrome's Page.captureScreenshot and our own encoder emit:
// 8-bit depth, non-interlaced, color types 0 (gray), 2 (RGB), 3 (palette),
// 4 (gray+alpha), 6 (RGBA). Everything else raises a clear error.
//
// The diff semantics are a port of layout/src/cpurender/pixmap.rs:424-513:
// a pixel "differs" iff ANY channel's |delta| exceeds `threshold`; the compare
// passes iff dimensions match and diff_ratio <= max_diff_ratio.

import zlib from 'node:zlib';

const SIG = [137, 80, 78, 71, 13, 10, 26, 10];

/** @returns {{width:number, height:number, rgba:Uint8Array}} */
export function decodePng(buf) {
    if (buf.length < 8 || SIG.some((b, i) => buf[i] !== b)) {
        throw new Error('not a PNG (bad signature)');
    }
    let pos = 8;
    let width = 0, height = 0, bitDepth = 0, colorType = 0, interlace = 0;
    let palette = null, trns = null;
    const idat = [];
    while (pos + 8 <= buf.length) {
        const len = buf.readUInt32BE(pos);
        const type = buf.toString('latin1', pos + 4, pos + 8);
        const data = buf.subarray(pos + 8, pos + 8 + len);
        pos += 8 + len + 4; // skip CRC
        if (type === 'IHDR') {
            width = data.readUInt32BE(0);
            height = data.readUInt32BE(4);
            bitDepth = data[8];
            colorType = data[9];
            interlace = data[12];
        } else if (type === 'PLTE') {
            palette = data;
        } else if (type === 'tRNS') {
            trns = data;
        } else if (type === 'IDAT') {
            idat.push(data);
        } else if (type === 'IEND') {
            break;
        }
    }
    if (!width || !height) throw new Error('PNG: missing/empty IHDR');
    if (bitDepth !== 8) throw new Error(`PNG: unsupported bit depth ${bitDepth} (only 8)`);
    if (interlace !== 0) throw new Error('PNG: interlaced images not supported');
    const chPer = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[colorType];
    if (!chPer) throw new Error(`PNG: unsupported color type ${colorType}`);
    if (colorType === 3 && !palette) throw new Error('PNG: palette image without PLTE');

    const raw = zlib.inflateSync(Buffer.concat(idat));
    const stride = width * chPer;
    if (raw.length < (stride + 1) * height) throw new Error('PNG: truncated pixel data');

    // Un-filter scanlines in place (filters 0-4, bpp = chPer at 8-bit depth).
    const px = new Uint8Array(stride * height);
    for (let y = 0; y < height; y++) {
        const filter = raw[y * (stride + 1)];
        const inRow = (y * (stride + 1)) + 1;
        const outRow = y * stride;
        for (let x = 0; x < stride; x++) {
            const cur = raw[inRow + x];
            const a = x >= chPer ? px[outRow + x - chPer] : 0;             // left
            const b = y > 0 ? px[outRow - stride + x] : 0;                 // up
            const c = (x >= chPer && y > 0) ? px[outRow - stride + x - chPer] : 0; // up-left
            let v;
            switch (filter) {
                case 0: v = cur; break;
                case 1: v = cur + a; break;
                case 2: v = cur + b; break;
                case 3: v = cur + ((a + b) >> 1); break;
                case 4: {
                    const p = a + b - c;
                    const pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
                    v = cur + ((pa <= pb && pa <= pc) ? a : (pb <= pc ? b : c));
                    break;
                }
                default: throw new Error(`PNG: unknown filter ${filter} on row ${y}`);
            }
            px[outRow + x] = v & 0xff;
        }
    }

    // Expand to RGBA.
    const rgba = new Uint8Array(width * height * 4);
    const n = width * height;
    for (let i = 0; i < n; i++) {
        const s = i * chPer, d = i * 4;
        switch (colorType) {
            case 0: rgba[d] = rgba[d + 1] = rgba[d + 2] = px[s]; rgba[d + 3] = 255; break;
            case 2: rgba[d] = px[s]; rgba[d + 1] = px[s + 1]; rgba[d + 2] = px[s + 2]; rgba[d + 3] = 255; break;
            case 3: {
                const idx = px[s], p = idx * 3;
                rgba[d] = palette[p]; rgba[d + 1] = palette[p + 1]; rgba[d + 2] = palette[p + 2];
                rgba[d + 3] = (trns && idx < trns.length) ? trns[idx] : 255;
                break;
            }
            case 4: rgba[d] = rgba[d + 1] = rgba[d + 2] = px[s]; rgba[d + 3] = px[s + 1]; break;
            case 6: rgba[d] = px[s]; rgba[d + 1] = px[s + 1]; rgba[d + 2] = px[s + 2]; rgba[d + 3] = px[s + 3]; break;
        }
    }
    return { width, height, rgba };
}

// ---- encoder (8-bit RGBA, filter 0) ----------------------------------------

let CRC_TABLE = null;
function crc32(...bufs) {
    if (!CRC_TABLE) {
        CRC_TABLE = new Int32Array(256);
        for (let i = 0; i < 256; i++) {
            let c = i;
            for (let k = 0; k < 8; k++) c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
            CRC_TABLE[i] = c;
        }
    }
    let crc = -1;
    for (const buf of bufs) {
        for (let i = 0; i < buf.length; i++) crc = CRC_TABLE[(crc ^ buf[i]) & 0xff] ^ (crc >>> 8);
    }
    return (crc ^ -1) >>> 0;
}

function chunk(type, data) {
    const t = Buffer.from(type, 'latin1');
    const head = Buffer.alloc(4);
    head.writeUInt32BE(data.length, 0);
    const tail = Buffer.alloc(4);
    tail.writeUInt32BE(crc32(t, data), 0);
    return Buffer.concat([head, t, data, tail]);
}

/** @returns {Buffer} a PNG file (8-bit RGBA, non-interlaced, filter 0) */
export function encodePng(width, height, rgba) {
    if (rgba.length !== width * height * 4) throw new Error('encodePng: rgba length mismatch');
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(width, 0);
    ihdr.writeUInt32BE(height, 4);
    ihdr[8] = 8;  // bit depth
    ihdr[9] = 6;  // color type RGBA
    // [10..12] = compression 0, filter 0, interlace 0
    const stride = width * 4;
    const raw = Buffer.alloc((stride + 1) * height);
    for (let y = 0; y < height; y++) {
        raw[y * (stride + 1)] = 0; // filter: none
        raw.set(rgba.subarray(y * stride, (y + 1) * stride), y * (stride + 1) + 1);
    }
    return Buffer.concat([
        Buffer.from(SIG),
        chunk('IHDR', ihdr),
        chunk('IDAT', zlib.deflateSync(raw)),
        chunk('IEND', Buffer.alloc(0)),
    ]);
}

// ---- pixel diff ------------------------------------------------------------

/**
 * Port of cpurender pixel_diff (pixmap.rs:424-513) + our mask extension.
 *
 * @param a  {{width,height,rgba}} expected (golden)
 * @param b  {{width,height,rgba}} actual
 * @param opts {threshold?: number, mask?: [x,y,w,h][]}
 *   threshold — per-channel delta above which a pixel counts as different
 *   mask      — rectangles (CSS px) excluded from comparison entirely
 * @returns {{dimensionsMatch, width, height, diffCount, maskedPixels,
 *            comparedPixels, totalPixels, maxDelta, diffRatio, diffMap}}
 */
export function pixelDiff(a, b, { threshold = 2, mask = [] } = {}) {
    if (a.width !== b.width || a.height !== b.height) {
        return {
            dimensionsMatch: false,
            width: 0, height: 0,
            expected: { width: a.width, height: a.height },
            actual: { width: b.width, height: b.height },
            diffCount: 0, maskedPixels: 0, comparedPixels: 0,
            totalPixels: 0, maxDelta: 0, diffRatio: 1, diffMap: null,
        };
    }
    const { width, height } = a;
    const total = width * height;
    const masked = new Uint8Array(total);
    for (const m of mask || []) {
        const [mx, my, mw, mh] = m.map(Number);
        const x0 = Math.max(0, Math.floor(mx)), y0 = Math.max(0, Math.floor(my));
        const x1 = Math.min(width, Math.ceil(mx + mw)), y1 = Math.min(height, Math.ceil(my + mh));
        for (let y = y0; y < y1; y++) {
            masked.fill(1, y * width + x0, y * width + x1);
        }
    }
    const diffMap = new Uint8Array(total);
    let diffCount = 0, maskedPixels = 0, maxDelta = 0;
    for (let i = 0; i < total; i++) {
        if (masked[i]) { maskedPixels++; continue; }
        const o = i * 4;
        let d0 = Math.abs(a.rgba[o] - b.rgba[o]);
        const d1 = Math.abs(a.rgba[o + 1] - b.rgba[o + 1]);
        const d2 = Math.abs(a.rgba[o + 2] - b.rgba[o + 2]);
        const d3 = Math.abs(a.rgba[o + 3] - b.rgba[o + 3]);
        if (d1 > d0) d0 = d1;
        if (d2 > d0) d0 = d2;
        if (d3 > d0) d0 = d3;
        if (d0 > maxDelta) maxDelta = d0;
        if (d0 > threshold) { diffCount++; diffMap[i] = 1; }
    }
    const compared = total - maskedPixels;
    return {
        dimensionsMatch: true,
        width, height,
        diffCount, maskedPixels, comparedPixels: compared, totalPixels: total,
        maxDelta,
        diffRatio: compared > 0 ? diffCount / compared : 0,
        diffMap,
    };
}

/**
 * Render a diff heatmap: dimmed grayscale of the actual image, differing
 * pixels solid red, masked pixels tinted blue. Returns a PNG Buffer.
 */
export function renderDiffPng(actual, diff, mask = []) {
    const { width, height } = actual;
    const total = width * height;
    const masked = new Uint8Array(total);
    for (const m of mask || []) {
        const [mx, my, mw, mh] = m.map(Number);
        const x0 = Math.max(0, Math.floor(mx)), y0 = Math.max(0, Math.floor(my));
        const x1 = Math.min(width, Math.ceil(mx + mw)), y1 = Math.min(height, Math.ceil(my + mh));
        for (let y = y0; y < y1; y++) masked.fill(1, y * width + x0, y * width + x1);
    }
    const out = new Uint8Array(total * 4);
    for (let i = 0; i < total; i++) {
        const o = i * 4;
        const lum = (actual.rgba[o] * 3 + actual.rgba[o + 1] * 6 + actual.rgba[o + 2]) / 10;
        const dim = Math.round(lum * 0.35 + 40);
        if (diff.diffMap && diff.diffMap[i]) {
            out[o] = 255; out[o + 1] = 0; out[o + 2] = 0; out[o + 3] = 255;
        } else if (masked[i]) {
            out[o] = Math.round(dim * 0.5); out[o + 1] = Math.round(dim * 0.5);
            out[o + 2] = Math.min(255, dim + 80); out[o + 3] = 255;
        } else {
            out[o] = dim; out[o + 1] = dim; out[o + 2] = dim; out[o + 3] = 255;
        }
    }
    return encodePng(width, height, out);
}
