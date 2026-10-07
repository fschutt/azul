# Built-in textures

`vellum_ink.bin` - vellum (parchment) grain, 128 x 128, seamless.

- Source: a vellum texture image from Wikimedia Commons / Wikipedia, released
  under CC0 1.0 (public domain dedication), added to the repository as
  `assets/vellum_cc0.png` (128 x 128 RGB).
- Stored as its INK: how much darker than the paper each pixel is, in 16
  levels (0 = the paper, 15 = the deepest crease), two pixels per byte, high
  nibble first, row by row - 8192 bytes, no decoder needed.
- `azul_layout::texture` turns it into `BuiltinTexture::Vellum` (opaque
  black-and-white paper) and `BuiltinTexture::VellumOverlay` (black ink at a
  low alpha, to lay over any colour).
