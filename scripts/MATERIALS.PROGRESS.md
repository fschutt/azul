# MATERIALS - builtin(leather|bluejeans|aluminium): micro PBR tiles + a CPU 2.5D shader

Worktree branch `worktree-agent-aafa9ee28a4747fc6` (ff'd to `fix/input-bugs-2026-09-19` @ 786b8cf08).
Scratch (never in the repo): /tmp/materials/ (scripts/, src/, micro/, azm/, compare/). Python:
/opt/homebrew/bin/python3 (numpy, PIL, brotli). The 4K sources stay in ~/Downloads (never committed).

## DONE
- 7a6bf853a fix(texture): commit vellum_ink.bin (`*.bin` is gitignored: builtin(vellum) never reached the repo)
- 062249baa test(RED): a callback image in a background layer is drawn for the layer's rect
- 15aa3a711 fix(engine): background-layer callback images rendered per item + the window's MaterialLight
- (this commit) feat(materials): the `materials` feature - tiles, decoder, shader, caches, CSS names, packer

## NEXT
- Python reference mirroring the Rust shader (macro on the final colour), final comparison sheets, timings
- report

## Findings
- Background layers on non-root boxes are drawn STRETCHED over the background area (one DL Image item,
  `push_background_layer`, no background-size / -repeat / -position); only the canvas (root) background
  tiles. A callback image there was never invoked (fixed in 15aa3a711).
- GL path: `build_webrender_transaction` / `generate_frame` register a frame's images
  (`register_frame_resources`) BEFORE `process_image_callback_updates` -> `prepare_frame_gl`, so a RAW frame a
  callback produces in preparation is registered one frame late (true for image nodes' CPU frames too). The
  background resolver therefore also runs where display lists are made and on a new light.
- aluminium: normal map flat (128,127,255), displacement all 0, specular ~constant (91..102), AO ~constant.
- denim (ambientCG Fabric003) is a 4096 x 7255 scan; its normal / height / roughness are its colour's
  (corr(Y,H) 0.98, corr(Y,R) -0.99): one Y plane + chroma is the whole material.
- leather: roughness ~ Y (corr 0.96).
- At UI scale the maps are noise-like: spatial prediction (MED) gains 0-4 dB on Y / normals, so transform
  coding buys little; entropy coding (brotli) buys 15-25 % over bit packing.
