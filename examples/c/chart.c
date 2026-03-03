// Chart with SVG Clip Masks - C
// Demonstrates using R8 image masks to clip DOM elements into chart shapes.
// cc -o chart chart.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
// DYLD_LIBRARY_PATH=../../target/release ./chart

#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

// Helper to create AzString from C string
static AzString az_str(const char* s) {
    return AzString_copyFromBytes((const uint8_t*)s, 0, strlen(s));
}

// Minimal app state (required by AZ_REFLECT)
typedef struct { uint8_t _unused; } ChartState;
void ChartState_destructor(void* s) { (void)s; }
AZ_REFLECT(ChartState, ChartState_destructor);

// Bar chart data
#define NUM_BARS 6
static const float bar_values[NUM_BARS] = { 0.7f, 0.45f, 0.9f, 0.3f, 0.6f, 0.85f };
static const char* bar_labels[NUM_BARS] = { "Jan", "Feb", "Mar", "Apr", "May", "Jun" };
static const char* bar_colors[NUM_BARS] = {
    "background: linear-gradient(to top, #ff6b6b, #ee5a24);",
    "background: linear-gradient(to top, #feca57, #ff9f43);",
    "background: linear-gradient(to top, #48dbfb, #0abde3);",
    "background: linear-gradient(to top, #ff9ff3, #f368e0);",
    "background: linear-gradient(to top, #54a0ff, #2e86de);",
    "background: linear-gradient(to top, #1dd1a1, #10ac84);",
};

// Create an R8 mask image with a rounded rectangle for a single bar.
// The mask is white (255) where the bar is, black (0) elsewhere.
static AzImageRef create_bar_mask(float bar_height_pct, int width, int height) {
    int pixel_count = width * height;
    uint8_t* pixels = (uint8_t*)calloc(pixel_count, 1); // All black (clipped)

    // Bar occupies the bottom bar_height_pct of the mask
    int bar_top = (int)((1.0f - bar_height_pct) * height);
    int radius = 8; // rounded top corners

    for (int y = bar_top; y < height; y++) {
        for (int x = 0; x < width; x++) {
            // Check if inside rounded rect (only top corners are rounded)
            int inside = 1;
            if (y < bar_top + radius) {
                // Top-left corner
                if (x < radius) {
                    int dx = radius - x;
                    int dy = radius - (y - bar_top);
                    if (dx * dx + dy * dy > radius * radius) inside = 0;
                }
                // Top-right corner
                if (x >= width - radius) {
                    int dx = x - (width - radius - 1);
                    int dy = radius - (y - bar_top);
                    if (dx * dx + dy * dy > radius * radius) inside = 0;
                }
            }
            if (inside) {
                pixels[y * width + x] = 255; // White = visible
            }
        }
    }

    AzU8Vec pixel_vec = AzU8Vec_copyFromBytes(pixels, 0, pixel_count);
    free(pixels);

    AzRawImage raw = {
        .pixels = { .U8 = { .tag = AzRawImageData_Tag_U8, .payload = pixel_vec } },
        .width = (size_t)width,
        .height = (size_t)height,
        .premultiplied_alpha = false,
        .data_format = AzRawImageFormat_R8,
        .tag = AzU8Vec_create(),
    };

    AzOptionImageRef opt = AzImageRef_newRawimage(raw);
    if (opt.Some.tag == AzOptionImageRef_Tag_Some) {
        return opt.Some.payload;
    }

    // Fallback: null image
    static uint8_t dummy = 0;
    AzU8VecRef empty = { .ptr = &dummy, .len = 0 };
    return AzImageRef_nullImage(width, height, AzRawImageFormat_R8, empty);
}

// Create a pie chart mask (circle with a wedge for a given percentage)
static AzImageRef create_pie_mask(float start_pct, float end_pct, int size) {
    int pixel_count = size * size;
    uint8_t* pixels = (uint8_t*)calloc(pixel_count, 1);

    float cx = size / 2.0f;
    float cy = size / 2.0f;
    float r = (size / 2.0f) - 2.0f; // slight inset

    float start_angle = start_pct * 2.0f * 3.14159265f - 3.14159265f / 2.0f;
    float end_angle = end_pct * 2.0f * 3.14159265f - 3.14159265f / 2.0f;

    for (int y = 0; y < size; y++) {
        for (int x = 0; x < size; x++) {
            float dx = x - cx;
            float dy = y - cy;
            float dist = sqrtf(dx * dx + dy * dy);
            if (dist > r) continue;

            float angle = atan2f(dy, dx);
            // Normalize angle check
            int in_wedge = 0;
            if (start_angle <= end_angle) {
                in_wedge = (angle >= start_angle && angle <= end_angle);
            } else {
                in_wedge = (angle >= start_angle || angle <= end_angle);
            }

            if (in_wedge) {
                pixels[y * size + x] = 255;
            }
        }
    }

    AzU8Vec pixel_vec = AzU8Vec_copyFromBytes(pixels, 0, pixel_count);
    free(pixels);

    AzRawImage raw = {
        .pixels = { .U8 = { .tag = AzRawImageData_Tag_U8, .payload = pixel_vec } },
        .width = (size_t)size,
        .height = (size_t)size,
        .premultiplied_alpha = false,
        .data_format = AzRawImageFormat_R8,
        .tag = AzU8Vec_create(),
    };

    AzOptionImageRef opt = AzImageRef_newRawimage(raw);
    if (opt.Some.tag == AzOptionImageRef_Tag_Some) {
        return opt.Some.payload;
    }

    static uint8_t dummy = 0;
    AzU8VecRef empty = { .ptr = &dummy, .len = 0 };
    return AzImageRef_nullImage(size, size, AzRawImageFormat_R8, empty);
}

// Layout callback
AzDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
    AzDom body = AzDom_createBody();
    AzDom_setInlineStyle(&body, az_str(
        "flex-direction: column;"
        "padding: 20px;"
        "gap: 20px;"
        "background: #1a1a2e;"
        "font-family: sans-serif;"
    ));

    // === Title ===
    AzDom title = AzDom_createText(az_str("Chart Demo - SVG Clip Masks"));
    AzDom_setInlineStyle(&title, az_str(
        "font-size: 24px;"
        "color: white;"
        "text-align: center;"
        "margin-bottom: 10px;"
    ));
    AzDom_addChild(&body, title);

    // === BAR CHART SECTION ===
    AzDom bar_section = AzDom_createDiv();
    AzDom_setInlineStyle(&bar_section, az_str(
        "flex-direction: column;"
        "padding: 15px;"
        "background: #16213e;"
        "border-radius: 12px;"
    ));

    AzDom bar_title = AzDom_createText(az_str("Monthly Revenue (Bar Chart with Clip Masks)"));
    AzDom_setInlineStyle(&bar_title, az_str(
        "font-size: 16px;"
        "color: #eee;"
        "margin-bottom: 15px;"
    ));
    AzDom_addChild(&bar_section, bar_title);

    // Bar container
    AzDom bar_container = AzDom_createDiv();
    AzDom_setInlineStyle(&bar_container, az_str(
        "flex-direction: row;"
        "align-items: flex-end;"
        "height: 250px;"
        "gap: 12px;"
        "padding: 10px;"
    ));

    int mask_w = 60;
    int mask_h = 250;

    for (int i = 0; i < NUM_BARS; i++) {
        // Each bar is a div with:
        // 1. A gradient background (the "fill")
        // 2. An R8 clip mask shaped as a rounded bar

        AzDom bar_col = AzDom_createDiv();
        AzDom_setInlineStyle(&bar_col, az_str(
            "flex-direction: column;"
            "align-items: center;"
            "flex-grow: 1;"
        ));

        // The clipped gradient div
        AzDom bar = AzDom_createDiv();

        // Build inline style with gradient
        char style_buf[512];
        snprintf(style_buf, sizeof(style_buf),
            "width: %dpx;"
            "height: %dpx;"
            "%s",
            mask_w, mask_h, bar_colors[i]
        );
        AzDom_setInlineStyle(&bar, az_str(style_buf));

        // Create the clip mask for this bar
        AzImageRef mask = create_bar_mask(bar_values[i], mask_w, mask_h);
        AzImageMask image_mask = {
            .image = mask,
            .rect = {
                .origin = { .x = 0.0f, .y = 0.0f },
                .size = { .width = (float)mask_w, .height = (float)mask_h },
            },
            .repeat = false,
        };
        bar = AzDom_withClipMask(bar, image_mask);
        AzDom_addChild(&bar_col, bar);

        // Label below bar
        AzDom label = AzDom_createText(az_str(bar_labels[i]));
        AzDom_setInlineStyle(&label, az_str(
            "color: #aaa;"
            "font-size: 12px;"
            "margin-top: 8px;"
        ));
        AzDom_addChild(&bar_col, label);

        // Value label
        char val_buf[32];
        snprintf(val_buf, sizeof(val_buf), "%d%%", (int)(bar_values[i] * 100));
        AzDom val_label = AzDom_createText(az_str(val_buf));
        AzDom_setInlineStyle(&val_label, az_str(
            "color: white;"
            "font-size: 11px;"
            "margin-top: 2px;"
        ));
        AzDom_addChild(&bar_col, val_label);

        AzDom_addChild(&bar_container, bar_col);
    }

    AzDom_addChild(&bar_section, bar_container);
    AzDom_addChild(&body, bar_section);

    // === PIE CHART SECTION ===
    AzDom pie_section = AzDom_createDiv();
    AzDom_setInlineStyle(&pie_section, az_str(
        "flex-direction: column;"
        "align-items: center;"
        "padding: 15px;"
        "background: #16213e;"
        "border-radius: 12px;"
    ));

    AzDom pie_title = AzDom_createText(az_str("Distribution (Pie Chart with Clip Masks)"));
    AzDom_setInlineStyle(&pie_title, az_str(
        "font-size: 16px;"
        "color: #eee;"
        "margin-bottom: 15px;"
    ));
    AzDom_addChild(&pie_section, pie_title);

    // Pie chart container (overlapping divs)
    AzDom pie_container = AzDom_createDiv();
    AzDom_setInlineStyle(&pie_container, az_str(
        "width: 200px;"
        "height: 200px;"
        "position: relative;"
    ));

    int pie_size = 200;
    float pie_values[] = { 0.35f, 0.25f, 0.20f, 0.12f, 0.08f };
    const char* pie_gradient_styles[] = {
        "background: #ff6b6b; position: absolute; top: 0; left: 0;",
        "background: #feca57; position: absolute; top: 0; left: 0;",
        "background: #48dbfb; position: absolute; top: 0; left: 0;",
        "background: #ff9ff3; position: absolute; top: 0; left: 0;",
        "background: #1dd1a1; position: absolute; top: 0; left: 0;",
    };
    int num_slices = 5;
    float cumulative = 0.0f;

    for (int i = 0; i < num_slices; i++) {
        AzDom slice = AzDom_createDiv();

        char slice_style[256];
        snprintf(slice_style, sizeof(slice_style),
            "width: %dpx; height: %dpx; %s",
            pie_size, pie_size, pie_gradient_styles[i]
        );
        AzDom_setInlineStyle(&slice, az_str(slice_style));

        float start = cumulative;
        cumulative += pie_values[i];
        float end = cumulative;

        AzImageRef pie_mask = create_pie_mask(start, end, pie_size);
        AzImageMask pie_image_mask = {
            .image = pie_mask,
            .rect = {
                .origin = { .x = 0.0f, .y = 0.0f },
                .size = { .width = (float)pie_size, .height = (float)pie_size },
            },
            .repeat = false,
        };
        slice = AzDom_withClipMask(slice, pie_image_mask);
        AzDom_addChild(&pie_container, slice);
    }

    AzDom_addChild(&pie_section, pie_container);
    AzDom_addChild(&body, pie_section);

    return AzDom_style(body, AzCss_empty());
}

int main(void) {
    printf("Chart Demo - SVG Clip Masks\n");
    printf("Uses R8 image masks to clip gradient divs into chart shapes.\n\n");

    ChartState state = { ._unused = 0 };
    AzRefAny data = ChartState_upcast(state);
    AzAppConfig config = AzAppConfig_create();
    AzApp app = AzApp_create(data, config);

    AzWindowCreateOptions window = AzWindowCreateOptions_create((AzLayoutCallbackType)layout);
    window.window_state.title = az_str("Chart Demo - Clip Masks");
    window.window_state.size.dimensions.width = 700.0f;
    window.window_state.size.dimensions.height = 700.0f;

    AzApp_run(&app, window);

    return 0;
}
