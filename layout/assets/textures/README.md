# Built-in textures

`vellum_ink.bin` - vellum (parchment) grain, 128 x 128, seamless.

- Source: a vellum texture image from Wikimedia Commons, released under CC0
  1.0 (public domain dedication), 128 x 128 RGB. The source image is not kept
  in the repository: `vellum_ink.bin` is the stored derivative, in the format
  below.
- Stored as its INK: how much darker than the paper each pixel is, in 16
  levels (0 = the paper, 15 = the deepest crease), two pixels per byte, high
  nibble first, row by row - 8192 bytes, no decoder needed.
- `azul_layout::texture` turns it into `BuiltinTexture::Vellum` (opaque
  black-and-white paper) and `BuiltinTexture::VellumOverlay` (black ink at a
  low alpha, to lay over any colour).

## Materials (cargo feature `materials`)

`leather.azm`, `bluejeans.azm`, `aluminium.azm` - the micro tiles behind
`builtin(leather)`, `builtin(bluejeans)`, `builtin(aluminium)`: 128 x 128
texels each, seamless, lit at run time for the rect they fill
(`azul_layout::texture::materials`).

| file            | source (all CC0 1.0)                                                                 | 4K maps used                         |
|-----------------|---------------------------------------------------------------------------------------|--------------------------------------|
| `leather.azm`   | Poly Haven "Brown Leather" by Rob Tuytel, <https://polyhaven.com/a/brown_leather> (0.4 m square) | albedo, normal (GL), roughness, AO   |
| `bluejeans.azm` | ambientCG "Fabric003", <https://ambientcg.com/a/Fabric003> (a 600 dpi scan, 4096 x 7255; licence and author in the set's `.usda`) | colour, normal (GL), roughness       |
| `aluminium.azm` | ShareTextures "Aluminium Texture" by M. Tolga Arslan, <https://www.sharetextures.com/textures/metal/aluminium-texture> (download `aluminium_texture-4K`, files `4K-aluminium_1-*.jpg`) | diffuse (its normal map is flat, its displacement 0, its specular and AO maps constant) |

The 4K sources are NOT in the repository (4-50 MB each). Cut and packed by
`scripts/make_material_tiles.py --src <dir with the three downloads>`:

- a crop at UI scale - 8 (leather), 4 (denim), 2 (aluminium) source pixels per
  texel: the leather's pebbles end up 4-7 px wide at 1x, the twill ~3 px -
  area-averaged to 144 x 144 and made seamless with two minimum-error seam
  cuts across the wrap (the first and last column / row are neighbours in the
  source);
- flattened (its large-scale variation divided out) so the repeat does not
  show; the source's own mottling comes back as a 16 x 16 macro luma map at
  the source's real period (512 / 1024 / 2048 texels);
- packed: the albedo with the ambient occlusion baked in, as YCoCg - Y per
  texel, Co / Cg subsampled - and the normal's x / y, each quantised over
  mean +- 3 sigma; derived at decode time: roughness = a + b Y (correlation
  0.96 leather, -0.99 denim), the denim's normal from the gradient of Y (its
  normal and height maps are its scan's), the aluminium's flat normal and
  constant roughness (0.42). One byte per symbol, brotli 11.

| material  | planes (bits @ subsample)        | raw packed bits | brotli (shipped) |
|-----------|----------------------------------|-----------------|------------------|
| leather   | Y 3, Co/Cg 2 @ 8x8, n.x 3, n.y 2 | 16.5 KB         | 13 679 B         |
| bluejeans | Y 4, Co/Cg 3 @ 4x4               | 9.0 KB          | 8 191 B          |
| aluminium | Y 3                              | 6.1 KB          | 5 139 B          |

Format (`AZM1`, after brotli; `materials::parse_tile`): `"AZM1"`, u16 side,
u8 planes, u8 constants; per plane u8 channel (Y Co Cg nx ny rough ao), u8
bits, u8 subsample, u8 0, f32 lo, f32 hi; per constant u8 id, 3 x u8 0, f32
(ids 0-6 fill a channel, 16 / 17 roughness = a + b Y, 18 normal from Y);
u8 macro side M, 3 x u8 0, f32 period, f32 lo, f32 hi, M x M u8; then each
plane's symbols, one byte each, row by row.
