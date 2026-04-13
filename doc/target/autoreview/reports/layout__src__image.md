# Review: layout/src/image.rs

## Summary
- Lines: 346
- Public functions: 9 (decode_raw_image_from_any_bytes, encode_bmp, encode_tga, encode_tiff, encode_gif, encode_pnm, encode_png, encode_jpeg, plus conditional stubs)
- Public structs/enums: 2 (DecodeImageError, EncodeImageError)
- Findings: 0 high, 0 medium, 1 low

## Findings

### [LOW] Encode macro repetition for PNG/JPEG vs other formats
- **Location**: `layout/src/image.rs:277-345` vs `271-275`
- **Details**: `encode_png` and `encode_jpeg` are hand-written rather than using the `encode_func!` macro because they need custom encoder options (`PngEncoder::new_with_quality`, `JpegEncoder::new_with_quality`). This is a reasonable design choice but leads to ~60 lines of duplicated encode-write-error-handle boilerplate. Not a bug, but a minor refactoring opportunity.
- **Recommendation**: Consider a helper closure or inner function to reduce the shared boilerplate (create cursor, get pixels, handle error, wrap result), passing only the encoder creation as a closure. Low priority.

## System Documentation
- System identified: Image encode/decode pipeline (part of the resource/asset system)
- Existing doc: none specific to image encoding/decoding in `doc/guide/`
- Doc needed: A guide page for the image pipeline would be useful but low priority — the module is small and self-contained. Could be a section in a broader "assets and resources" guide.
