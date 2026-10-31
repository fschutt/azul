#!/usr/bin/env python3
"""Cut the built-in MATERIAL tiles (`builtin(leather)`, `builtin(bluejeans)`, `builtin(aluminium)`,
`azul_layout::texture::materials`, cargo feature `materials`) out of their CC0 4K PBR sources.

    python3 scripts/make_material_tiles.py [--src ~/Downloads] [--out layout/assets/textures] [--vectors]

Needs numpy, Pillow and brotli. The 4K sources are NOT in the repository (4-50 MB each); their origin
and licence are in layout/assets/textures/README.md, next to what this writes.

Per material:

1. a crop at UI scale (`f` source pixels per micro texel: the leather's pebbles end up 4-7 px wide at
   1x, the denim's twill ~3 px) of (N + o) * f source pixels, area-averaged to (N + o)^2 texels;
2. made seamless with two minimum-error seam cuts (image quilting) across the wrap: the left o columns
   take the right overlap left of the cheapest vertical path, then the same for the rows - so the
   tile's first and last column (row) are neighbours in the source;
3. FLATTENED: its large-scale variation (a gaussian of N/10 texels, wrapped) divided out of the albedo
   and subtracted from the normals / roughness, so the repeat does not show as a blotch grid; the
   source's own large-scale mottling comes back as a 16 x 16 MACRO luma map laid over the tile at the
   source's real period (the whole 4K tile, in micro texels);
4. PACKED: the albedo (AO baked into it) as YCoCg - Y per texel, Co/Cg subsampled - and the normal's
   x / y, each uniformly quantised over mean +- 3 sigma to a few bits. What the shader needs and the
   tile does not store is DERIVED: roughness = a + b * Y (correlation 0.96 for the leather, -0.99 for
   the denim), the denim's normals from the gradient of Y (its normal and height maps are its colour
   scan's), the aluminium's flat normal and constant roughness;
5. one byte per symbol, all planes, brotli quality 11 (`.azm`, decoded once per process by
   `materials::decode_tile`; the format is documented there).
"""
import argparse
import os
import struct
import sys

import numpy as np
from PIL import Image

try:
    import brotli
except ImportError:  # pragma: no cover
    sys.exit('needs the brotli module (pip install brotli)')

Image.MAX_IMAGE_PIXELS = None

MAGIC = b'AZM1'
CH_Y, CH_CO, CH_CG, CH_NX, CH_NY, CH_R, CH_AO = range(7)
CONST_ROUGH_A, CONST_ROUGH_B, CONST_NORMAL_FROM_Y = 16, 17, 18

# name -> source files (relative to --src), crop, scale, packing
MATERIALS = {
    'leather': dict(
        files=dict(albedo='brown_leather/brown_leather_albedo_4k.jpg',
                   normal='brown_leather/brown_leather_nor_gl_4k.jpg',
                   rough='brown_leather/brown_leather_rough_4k.jpg',
                   ao='brown_leather/brown_leather_ao_4k.jpg'),
        N=128, f=8, crop=(600, 600), flatten=10,
        planes={CH_Y: (3, 1), CH_CO: (2, 8), CH_CG: (2, 8), CH_NX: (3, 1), CH_NY: (2, 1)},
        rough_from_y=True, normals_from_y=False),
    'bluejeans': dict(
        files=dict(albedo='Fabric003_4K-JPG/Fabric003_4K_Color.jpg',
                   normal='Fabric003_4K-JPG/Fabric003_4K_NormalGL.jpg',
                   rough='Fabric003_4K-JPG/Fabric003_4K_Roughness.jpg'),
        N=128, f=4, crop=(1400, 2600), flatten=16,
        planes={CH_Y: (4, 1), CH_CO: (3, 4), CH_CG: (3, 4)},
        rough_from_y=True, normals_from_y=True),
    'aluminium': dict(
        files=dict(albedo='aluminium_texture-4K/4K-aluminium_1-diffuse.jpg',
                   normal='aluminium_texture-4K/4K-aluminium_1-normal.jpg',
                   ao='aluminium_texture-4K/4K-aluminium_1-ao.jpg'),
        N=128, f=2, crop=(1000, 1000), flatten=10,
        planes={CH_Y: (3, 1)},
        # the specular map is constant (91..102 of 255): no roughness map, a satin constant
        const_rough=0.42, rough_from_y=False, normals_from_y=False),
}


# -- colour ---------------------------------------------------------------------------------------

def srgb_to_lin(c):
    c = np.asarray(c, dtype=np.float32)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4).astype(np.float32)


def lin_to_srgb(c):
    c = np.clip(c, 0, 1)
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1 / 2.4) - 0.055).astype(np.float32)


# -- 1. crop + area downsample --------------------------------------------------------------------

def read_region(path, box):
    im = Image.open(path)
    w, h = im.size
    a = np.asarray(im)
    x0, y0, x1, y1 = box
    return a[np.ix_(np.arange(y0, y1) % h, np.arange(x0, x1) % w)]


def box_down(ch, out_w, out_h):
    im = Image.fromarray(ch.astype(np.float32), mode='F')
    return np.asarray(im.resize((out_w, out_h), Image.BOX), dtype=np.float32)


def load_maps(src, spec, box, size):
    files = spec['files']

    def channel(path, c=None):
        a = read_region(os.path.join(src, path), box).astype(np.float32) / 255
        if a.ndim == 3:
            a = a[..., c if c is not None else 0]
        return a

    m = {}
    alb = read_region(os.path.join(src, files['albedo']), box).astype(np.float32) / 255
    if alb.ndim == 2:
        alb = np.repeat(alb[..., None], 3, axis=2)
    lin = srgb_to_lin(alb)
    m['albedo_lin'] = np.stack([box_down(lin[..., c], size, size) for c in range(3)], -1)
    nrm = read_region(os.path.join(src, files['normal']), box).astype(np.float32) / 255 * 2 - 1
    nx, ny, nz = (box_down(nrm[..., c], size, size) for c in range(3))
    ln = np.sqrt(nx * nx + ny * ny + nz * nz)
    m['nx'], m['ny'] = nx / ln, ny / ln
    m['rough'] = box_down(channel(files['rough']), size, size) if 'rough' in files else \
        np.full((size, size), spec.get('const_rough', 0.5), dtype=np.float32)
    m['ao'] = box_down(channel(files['ao']), size, size) if 'ao' in files else \
        np.ones((size, size), dtype=np.float32)
    return m


# -- 2. seamless: minimum-error seam cuts across the wrap -----------------------------------------

def min_cut_path(err):
    h, w = err.shape
    cost = err.copy()
    back = np.zeros((h, w), dtype=np.int64)
    for y in range(1, h):
        prev = cost[y - 1]
        cand = np.stack([np.r_[np.inf, prev[:-1]], prev, np.r_[prev[1:], np.inf]])
        k = np.argmin(cand, axis=0)
        cost[y] += cand[k, np.arange(w)]
        back[y] = np.arange(w) + k - 1
    path = np.zeros(h, dtype=np.int64)
    path[-1] = int(np.argmin(cost[-1, 1:-1])) + 1
    for y in range(h - 1, 0, -1):
        path[y - 1] = back[y, path[y]]
    return np.clip(path, 1, w - 1)


def cut_columns(m, n, o):
    lum = m['albedo_lin'] @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    err = 0
    for arr, wgt in ((lum, 4.0), (m['nx'], 1.0), (m['ny'], 1.0), (m['rough'], 0.5)):
        err = err + wgt * (arr[:, 0:o] - arr[:, n:n + o]) ** 2
    take_right = np.arange(o)[None, :] < min_cut_path(err)[:, None]
    out = {}
    for k, arr in m.items():
        sel = take_right if arr.ndim == 2 else take_right[..., None]
        out[k] = np.concatenate([np.where(sel, arr[:, n:n + o], arr[:, 0:o]), arr[:, o:n]], axis=1)
    return out


def transposed(m):
    return {k: (np.transpose(v, (1, 0, 2)) if v.ndim == 3 else v.T) for k, v in m.items()}


def seamless(m, n, o):
    return transposed(cut_columns(transposed(cut_columns(m, n, o)), n, o))


# -- 3. flatten + macro ---------------------------------------------------------------------------

def blur_wrap(a, sigma):
    h, w = a.shape
    fy = np.fft.fftfreq(h)[:, None]
    fx = np.fft.fftfreq(w)[None, :]
    g = np.exp(-2 * (np.pi ** 2) * (sigma ** 2) * (fx ** 2 + fy ** 2))
    return np.fft.ifft2(np.fft.fft2(a) * g).real.astype(np.float32)


def flatten(m, sigma):
    out = dict(m)
    lin = m['albedo_lin']
    out['albedo_lin'] = np.stack(
        [lin[..., c] / np.maximum(blur_wrap(lin[..., c], sigma), 1e-4) * lin[..., c].mean() for c in range(3)], -1)
    for k in ('nx', 'ny'):
        out[k] = m[k] - blur_wrap(m[k], sigma)
    if m['rough'].std() > 1e-4:
        out['rough'] = np.clip(m['rough'] - blur_wrap(m['rough'], sigma) + m['rough'].mean(), 0, 1)
    if m['ao'].std() > 1e-4:
        out['ao'] = np.clip(m['ao'] / np.maximum(blur_wrap(m['ao'], sigma), 1e-4) * m['ao'].mean(), 0, 1)
    return out


def macro_map(src, spec, M=16):
    """the whole source tile's luma, area-averaged to M x M (its top square for a non-square scan),
    as a multiplier of mean 1."""
    im = Image.open(os.path.join(src, spec['files']['albedo']))
    w, h = im.size
    im.draft(im.mode, (w // 8, h // 8))
    side = min(im.size)
    im = im.convert('RGB').crop((0, 0, side, side))
    a = srgb_to_lin(np.asarray(im).astype(np.float32) / 255)
    y = a @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    small = box_down(y, M, M)
    return small / small.mean(), w / spec['f']


# -- 4./5. pack -----------------------------------------------------------------------------------

def to_ycocg(rgb):
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    return 0.25 * r + 0.5 * g + 0.25 * b, 0.5 * (r - b), -0.25 * r + 0.5 * g - 0.25 * b


def subsample(a, s):
    h, w = a.shape
    return a.reshape(h // s, s, w // s, s).mean(axis=(1, 3)) if s > 1 else a


def quantise(a, bits):
    m, s = float(a.mean()), float(a.std())
    lo = max(float(a.min()), m - 3 * s)
    hi = min(float(a.max()), m + 3 * s)
    levels = (1 << bits) - 1
    if hi - lo < 1e-6:
        return None, lo, lo
    q = np.clip(np.round((a - lo) / (hi - lo) * levels), 0, levels).astype(np.uint8)
    return q, np.float32(lo), np.float32(hi)


def pack(m, spec, macro, period):
    n = m['nx'].shape[0]
    # AO baked into the albedo's ambient share (the shader then reads AO = 1)
    lin = m['albedo_lin'] * (0.45 + 0.55 * m['ao'])[..., None]
    y, co, cg = to_ycocg(lin_to_srgb(lin))
    chans = {CH_Y: y, CH_CO: co, CH_CG: cg, CH_NX: m['nx'], CH_NY: m['ny'], CH_R: m['rough']}
    planes, data, consts = [], [], []
    for ch, (bits, sub) in spec['planes'].items():
        q, lo, hi = quantise(subsample(chans[ch], sub), bits)
        if q is None:
            consts.append((ch, float(lo)))
            continue
        planes.append((ch, bits, sub, lo, hi))
        data.append(q.tobytes())
    for ch in (CH_CO, CH_CG, CH_NX, CH_NY):
        if ch not in spec['planes'] and not (spec['normals_from_y'] and ch in (CH_NX, CH_NY)):
            consts.append((ch, float(chans[ch].mean())))
    if spec['rough_from_y']:
        a_, b_ = np.linalg.lstsq(np.stack([np.ones(y.size), y.ravel()], 1), m['rough'].ravel(), rcond=None)[0]
        consts += [(CONST_ROUGH_A, float(a_)), (CONST_ROUGH_B, float(b_))]
    else:
        consts.append((CH_R, float(spec.get('const_rough', m['rough'].mean()))))
    if spec['normals_from_y']:
        dx = (np.roll(y, -1, 1) - np.roll(y, 1, 1)) / 2
        dy = (np.roll(y, -1, 0) - np.roll(y, 1, 0)) / 2
        g = np.concatenate([-dx.ravel(), dy.ravel()])
        nn = np.concatenate([m['nx'].ravel(), m['ny'].ravel()])
        consts.append((CONST_NORMAL_FROM_Y, float((g * nn).sum() / (g * g).sum())))
    mq, mlo, mhi = quantise_macro(macro)
    head = MAGIC + struct.pack('<HBB', n, len(planes), len(consts))
    for ch, bits, sub, lo, hi in planes:
        head += struct.pack('<BBBxff', ch, bits, sub, lo, hi)
    for cid, v in consts:
        head += struct.pack('<Bxxxf', cid, v)
    head += struct.pack('<Bxxxfff', macro.shape[0], period, mlo, mhi) + mq.tobytes()
    raw = head + b''.join(data)
    return raw, brotli.compress(raw, quality=11, lgwin=22)


def quantise_macro(macro):
    lo, hi = float(macro.min()), float(macro.max())
    q = np.clip(np.round((macro - lo) / max(hi - lo, 1e-6) * 255), 0, 255).astype(np.uint8)
    return q, lo, hi


# -- the decoder (the Rust one's twin: `materials::decode_tile`) ----------------------------------

def bilinear_up(a, s, n):
    """subsampled grid -> n x n, bilinear with wrap, texel centres."""
    if s == 1:
        return a
    m = a.shape[0]
    c = (np.arange(n) + 0.5) / s - 0.5
    i0 = np.floor(c).astype(np.int64)
    t = (c - i0).astype(np.float32)
    i0 %= m
    i1 = (i0 + 1) % m
    rows = a[i0] * (1 - t)[:, None] + a[i1] * t[:, None]
    return rows[:, i0] * (1 - t)[None, :] + rows[:, i1] * t[None, :]


def decode(raw):
    assert raw[:4] == MAGIC
    n, pc, cc = struct.unpack_from('<HBB', raw, 4)
    off = 8
    planes = []
    for _ in range(pc):
        planes.append(struct.unpack_from('<BBBxff', raw, off))
        off += 12
    consts = {}
    for _ in range(cc):
        cid, v = struct.unpack_from('<Bxxxf', raw, off)
        consts[cid] = np.float32(v)
        off += 8
    mm, period, mlo, mhi = struct.unpack_from('<Bxxxfff', raw, off)
    off += 16
    macro = (mlo + np.frombuffer(raw, np.uint8, mm * mm, off).astype(np.float32) / 255 * (mhi - mlo)).reshape(mm, mm)
    off += mm * mm
    default = {CH_Y: 0.5, CH_CO: 0.0, CH_CG: 0.0, CH_NX: 0.0, CH_NY: 0.0, CH_R: 0.5, CH_AO: 1.0}
    ch = {k: np.full((n, n), consts.get(k, v), np.float32) for k, v in default.items()}
    for cid, bits, sub, lo, hi in planes:
        m = n // sub
        q = np.frombuffer(raw, np.uint8, m * m, off).reshape(m, m).astype(np.float32)
        off += m * m
        v = (np.float32(lo) + q / np.float32((1 << bits) - 1) * (np.float32(hi) - np.float32(lo))).astype(np.float32)
        ch[cid] = bilinear_up(v, sub, n).astype(np.float32)
    if CONST_ROUGH_A in consts:
        ch[CH_R] = np.clip(consts[CONST_ROUGH_A] + consts[CONST_ROUGH_B] * ch[CH_Y], 0, 1)
    if CONST_NORMAL_FROM_Y in consts:
        k = consts[CONST_NORMAL_FROM_Y]
        y = ch[CH_Y]
        nx = -k * (np.roll(y, -1, 1) - np.roll(y, 1, 1)) / 2
        ny = k * (np.roll(y, -1, 0) - np.roll(y, 1, 0)) / 2
        ln = np.sqrt(nx * nx + ny * ny + 1)
        ch[CH_NX], ch[CH_NY] = nx / ln, ny / ln
    t = ch[CH_Y] - ch[CH_CG]
    rgb = np.clip(np.stack([t + ch[CH_CO], ch[CH_Y] + ch[CH_CG], t - ch[CH_CO]], -1), 0, 1)
    return dict(n=n, albedo=rgb, nxy=np.stack([ch[CH_NX], ch[CH_NY]], -1), rough=ch[CH_R], ao=ch[CH_AO],
                macro=macro, period=period)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--src', default=os.path.expanduser('~/Downloads'))
    ap.add_argument('--out', default=os.path.join(os.path.dirname(__file__), '..', 'layout', 'assets', 'textures'))
    ap.add_argument('--only', default='')
    ap.add_argument('--vectors', action='store_true', help='print decoded texels for the Rust tests')
    args = ap.parse_args()
    for name, spec in MATERIALS.items():
        if args.only and name != args.only:
            continue
        n, f, o = spec['N'], spec['f'], max(8, spec['N'] // 8)
        x0, y0 = spec['crop']
        size = (n + o) * f
        m = load_maps(args.src, spec, (x0, y0, x0 + size, y0 + size), n + o)
        m = flatten(seamless(m, n, o), n / spec['flatten'])
        macro, period = macro_map(args.src, spec)
        raw, packed = pack(m, spec, macro, period)
        path = os.path.join(args.out, f'{name}.azm')
        with open(path, 'wb') as fh:
            fh.write(packed)
        print(f'{name}: {n}x{n}, {len(raw)} B raw, {len(packed)} B brotli -> {path}')
        if args.vectors:
            d = decode(raw)
            srgb8 = np.round(d['albedo'] * 255).astype(int)
            for (x, y) in ((0, 0), (17, 5), (n - 1, n - 1), (64, 100)):
                print(f'   texel ({x},{y}) albedo {tuple(srgb8[y, x])} nxy ({d["nxy"][y, x, 0]:+.4f},{d["nxy"][y, x, 1]:+.4f}) '
                      f'rough {d["rough"][y, x]:.4f}')
            a = d['albedo']
            wrap_c = np.abs(a[:, 0] - a[:, -1]).mean()
            in_c = np.abs(a[:, 1:] - a[:, :-1]).mean()
            wrap_r = np.abs(a[0] - a[-1]).mean()
            in_r = np.abs(a[1:] - a[:-1]).mean()
            print(f'   seam: columns {wrap_c:.4f} vs {in_c:.4f} inside; rows {wrap_r:.4f} vs {in_r:.4f}')
            print(f'   mean albedo {tuple(np.round(a.reshape(-1, 3).mean(0) * 255, 1))}')


if __name__ == '__main__':
    main()
