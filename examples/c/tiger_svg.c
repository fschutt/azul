// Tiger SVG Renderer - C API Example
// Demonstrates: load SVG file → parse → render to RawImage → encode PNG → save to disk
//
// Build:
//   cc -o tiger_svg tiger_svg.c -I../../target/codegen -L../../target/release -lazul -Wl,-rpath,../../target/release
// Run:
//   DYLD_LIBRARY_PATH=../../target/release ./tiger_svg

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char** argv) {
    const char* svg_path = "../../examples/assets/svg/tiger.svg";
    if (argc > 1) svg_path = argv[1];

    // Read SVG file
    FILE* f = fopen(svg_path, "rb");
    if (!f) {
        fprintf(stderr, "ERROR: Cannot open %s\n", svg_path);
        return 1;
    }
    fseek(f, 0, SEEK_END);
    long file_size = ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t* svg_bytes = (uint8_t*)malloc(file_size);
    fread(svg_bytes, 1, file_size, f);
    fclose(f);

    printf("Read %ld bytes from %s\n", file_size, svg_path);

    // Parse SVG
    AzU8VecRef svg_ref = { .ptr = svg_bytes, .len = (size_t)file_size };
    AzSvgParseOptions opts = AzSvgParseOptions_default();
    AzResultParsedSvgSvgParseError result = AzParsedSvg_fromBytes(svg_ref, opts);

    const AzParsedSvg* parsed;
    if (!AzResultParsedSvgSvgParseError_matchRefOk(&result, &parsed)) {
        fprintf(stderr, "ERROR: Failed to parse SVG\n");
        free(svg_bytes);
        return 1;
    }
    printf("SVG parsed successfully\n");

    // Render to RawImage (900x900)
    AzSvgRenderOptions render_opts = AzSvgRenderOptions_default();
    AzLayoutSize target_size = { .width = 900.0f, .height = 900.0f };
    render_opts.target_size.Some = (AzOptionLayoutSizeVariant_Some){
        .tag = AzOptionLayoutSize_Tag_Some,
        .payload = target_size,
    };

    AzOptionRawImage raw_opt = AzParsedSvg_render(parsed, render_opts);
    if (raw_opt.Some.tag != AzOptionRawImage_Tag_Some) {
        fprintf(stderr, "ERROR: SVG render returned None\n");
        AzResultParsedSvgSvgParseError_delete(&result);
        free(svg_bytes);
        return 1;
    }

    AzRawImage raw = raw_opt.Some.payload;
    printf("Rendered to %zux%zu RGBA image\n", raw.width, raw.height);

    // Encode to PNG
    AzResultU8VecEncodeImageError png_result = AzRawImage_encodePng(&raw);
    const AzU8Vec* png_data;
    if (!AzResultU8VecEncodeImageError_matchRefOk(&png_result, &png_data)) {
        fprintf(stderr, "ERROR: PNG encoding failed\n");
        AzRawImage_delete(&raw);
        AzResultParsedSvgSvgParseError_delete(&result);
        free(svg_bytes);
        return 1;
    }

    // Save PNG
    const char* out_path = "/tmp/azul_tiger_c_api.png";
    FILE* out = fopen(out_path, "wb");
    if (out) {
        fwrite(png_data->ptr, 1, png_data->len, out);
        fclose(out);
        printf("Wrote %zu bytes to %s\n", png_data->len, out_path);
    } else {
        fprintf(stderr, "ERROR: Cannot write %s\n", out_path);
    }

    // Open the image
#ifdef __APPLE__
    char cmd[256];
    snprintf(cmd, sizeof(cmd), "open %s", out_path);
    system(cmd);
#elif __linux__
    char cmd[256];
    snprintf(cmd, sizeof(cmd), "xdg-open %s", out_path);
    system(cmd);
#endif

    // Cleanup
    AzResultU8VecEncodeImageError_delete(&png_result);
    AzRawImage_delete(&raw);
    AzResultParsedSvgSvgParseError_delete(&result);
    free(svg_bytes);

    printf("Done!\n");
    return 0;
}
