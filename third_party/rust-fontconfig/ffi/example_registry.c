/**
 * @file example_registry.c
 * @brief Demonstrates the async registry (background thread) API.
 *
 * This mirrors how the "azul" GUI framework uses rust-fontconfig for
 * fast startup:
 *
 *   App startup (instant):
 *     1. fc_registry_new()    — create registry, returns immediately
 *     2. fc_registry_spawn()  — launch scout + builder threads, returns immediately
 *     → Window appears, user sees content
 *
 *   First layout pass (blocks only for what we need):
 *     3. fc_registry_request_fonts({"Arial","sans-serif"}, {"monospace"})
 *        — blocks until ONLY those font stacks are resolved
 *        — builder threads prioritize these over the other 800+ fonts
 *     4. fc_registry_snapshot() — take a cache snapshot for rendering
 *     → First frame renders with correct fonts
 *
 *   Subsequent frames:
 *     5. Background threads keep parsing remaining fonts
 *     6. If new CSS font-family appears, request_fonts() again
 *        — blocks only if that specific font isn't loaded yet
 *
 * Build:
 *   make
 *   ./example_registry [1|2|3]
 */

#include <stdio.h>
#include <string.h>
#include "rust_fontconfig.h"

/* ── Helpers ─────────────────────────────────────────────────────────────── */

static void print_font_id(const FcFontId* id) {
    char buf[64];
    if (fc_font_id_to_string(id, buf, sizeof(buf)))
        printf("%s", buf);
    else
        printf("(unknown)");
}

static void print_font_path(FcFontRegistry registry, const FcFontId* id) {
    FcFontPath* path = fc_registry_get_font_path(registry, id);
    if (path) {
        printf("%s", path->path);
        if (path->font_index > 0)
            printf(" [index %zu]", path->font_index);
        fc_font_path_free(path);
    } else {
        printf("(memory font)");
    }
}

static void print_render_config(const FcFontRenderConfig* rc) {
    const char* hintstyles[] = {"none", "slight", "medium", "full"};
    const char* rgbas[] = {"unknown", "rgb", "bgr", "vrgb", "vbgr", "none"};
    const char* lcdfilters[] = {"none", "default", "light", "legacy"};

    int any = 0;
    if (rc->antialias >= 0)     { printf("      antialias:     %s\n", rc->antialias ? "true" : "false"); any = 1; }
    if (rc->hinting >= 0)       { printf("      hinting:       %s\n", rc->hinting ? "true" : "false"); any = 1; }
    if (rc->hintstyle >= 0 && rc->hintstyle <= 3)
                                { printf("      hintstyle:     %s\n", hintstyles[rc->hintstyle]); any = 1; }
    if (rc->autohint >= 0)      { printf("      autohint:      %s\n", rc->autohint ? "true" : "false"); any = 1; }
    if (rc->rgba >= 0 && rc->rgba <= 5)
                                { printf("      rgba:          %s\n", rgbas[rc->rgba]); any = 1; }
    if (rc->lcdfilter >= 0 && rc->lcdfilter <= 3)
                                { printf("      lcdfilter:     %s\n", lcdfilters[rc->lcdfilter]); any = 1; }
    if (rc->embeddedbitmap >= 0){ printf("      embeddedbitmap:%s\n", rc->embeddedbitmap ? "true" : "false"); any = 1; }
    if (rc->embolden >= 0)      { printf("      embolden:      %s\n", rc->embolden ? "true" : "false"); any = 1; }
    if (rc->dpi >= 0.0)         { printf("      dpi:           %.1f\n", rc->dpi); any = 1; }
    if (rc->scale >= 0.0)       { printf("      scale:         %.2f\n", rc->scale); any = 1; }
    if (rc->minspace >= 0)      { printf("      minspace:      %s\n", rc->minspace ? "true" : "false"); any = 1; }
    if (!any)                   { printf("      (all defaults — no per-font overrides from fonts.conf)\n"); }
}

static void print_runs(FcFontChain chain, FcFontCache cache,
                        FcFontRegistry registry, const char* text) {
    size_t runs_count = 0;
    FcResolvedFontRun* runs = fc_chain_query_for_text(chain, cache, text, &runs_count);

    printf("    Input:  \"%s\"\n", text);
    printf("    Runs:   %zu\n", runs_count);

    for (size_t i = 0; i < runs_count; i++) {
        printf("      [%3zu..%3zu] \"%s\"", runs[i].start_byte, runs[i].end_byte, runs[i].text);
        if (runs[i].has_font) {
            /* Look up the font's real name via metadata */
            FcFontMetadata* meta = fc_registry_get_metadata(registry, &runs[i].font_id);
            if (meta && meta->full_name)
                printf("  -->  %s", meta->full_name);
            else
                printf("  -->  font ");

            if (runs[i].css_source[0] != '\0')
                printf("  (matched via \"%s\")", runs[i].css_source);

            if (meta) fc_font_metadata_free(meta);
        } else {
            printf("  -->  (no font found)");
        }
        printf("\n");
    }
    printf("\n");

    fc_resolved_runs_free(runs, runs_count);
}

/* ── Demo 1: Azul-style fast startup ─────────────────────────────────── */

static void demo_azul_pattern(void) {
    printf("====================================================================\n");
    printf("  Demo 1: Azul-Style Fast Startup\n");
    printf("====================================================================\n");
    printf("\n");
    printf("  This mirrors how the azul GUI framework uses rust-fontconfig.\n");
    printf("  The key insight: don't wait for all 800+ system fonts to load.\n");
    printf("  Only block for the fonts the current frame actually needs.\n");
    printf("\n");

    /* ── Phase 1: App startup (instant) ─────────────────────────────── */
    printf("--- Phase 1: App Startup (instant) ---\n\n");

    printf("  fc_registry_new() ...\n");
    FcFontRegistry registry = fc_registry_new();
    printf("  Done. (No scanning happened yet.)\n\n");

    printf("  fc_registry_spawn() ...\n");
    fc_registry_spawn(registry);
    printf("  Done. Scout + builder threads now running in background.\n\n");

    /* Check immediate status — scout may or may not be done yet */
    size_t count0 = 0;
    FcFontInfo* f0 = fc_registry_list_fonts(registry, &count0);
    fc_font_info_free(f0, count0);
    printf("  Status right after spawn:\n");
    printf("    Scout complete:  %s\n", fc_registry_is_scan_complete(registry) ? "yes" : "no");
    printf("    Build complete:  %s\n", fc_registry_is_build_complete(registry) ? "no" : "no");
    printf("    Fonts loaded:    %zu  (background threads are still working)\n", count0);
    printf("\n");
    printf("  At this point, the window can appear and show a loading state,\n");
    printf("  skeleton UI, or cached content. No blocking has occurred.\n");
    printf("\n");

    /* ── Phase 2: First layout pass ─────────────────────────────────── */
    printf("--- Phase 2: First Layout Pass (blocks only for needed fonts) ---\n\n");

    printf("  The layout engine needs these CSS font-family stacks:\n");
    printf("    Stack 0: [\"Arial\", \"Helvetica\", \"sans-serif\"]\n");
    printf("    Stack 1: [\"Georgia\", \"Times New Roman\", \"serif\"]\n");
    printf("    Stack 2: [\"Courier New\", \"monospace\"]\n");
    printf("\n");
    printf("  Calling fc_registry_request_fonts() ...\n");
    printf("  This BLOCKS until exactly these fonts are parsed and ready.\n");
    printf("  The builder threads re-prioritize: these stacks become CRITICAL.\n");
    printf("\n");

    const char* stack0[] = {"Arial", "Helvetica", "sans-serif"};
    const char* stack1[] = {"Georgia", "Times New Roman", "serif"};
    const char* stack2[] = {"Courier New", "monospace"};
    const char** stacks[] = {stack0, stack1, stack2};
    size_t counts[] = {3, 3, 2};
    size_t num_chains = 0;

    FcFontChain* chains = fc_registry_request_fonts(
        registry, stacks, counts, 3, &num_chains);

    /* Check status after request_fonts returns */
    size_t count1 = 0;
    FcFontInfo* f1 = fc_registry_list_fonts(registry, &count1);
    fc_font_info_free(f1, count1);

    printf("  request_fonts() returned.\n");
    printf("    Chains resolved:  %zu\n", num_chains);
    printf("    Fonts loaded:     %zu  (NOT all system fonts — just what we need + common)\n", count1);
    printf("    Scout complete:   %s\n", fc_registry_is_scan_complete(registry) ? "yes" : "no");
    printf("    Build complete:   %s\n", fc_registry_is_build_complete(registry) ? "yes" : "no");
    printf("\n");

    /* Take a snapshot for text rendering */
    FcFontCache cache = fc_registry_snapshot(registry);

    /* Show what each chain resolved to */
    printf("  Resolved font chains:\n\n");
    const char* stack_names[] = {
        "sans-serif stack (Arial, Helvetica, sans-serif)",
        "serif stack (Georgia, Times New Roman, serif)",
        "monospace stack (Courier New, monospace)",
    };
    for (size_t i = 0; i < num_chains; i++) {
        printf("    Chain %zu: %s\n", i, stack_names[i]);

        /* Show the CSS fallback groups */
        size_t groups_count = 0;
        FcCssFallbackGroup* groups = fc_chain_get_css_fallbacks(chains[i], &groups_count);
        for (size_t g = 0; g < groups_count; g++) {
            printf("      \"%s\" -> %zu font(s)", groups[g].css_name, groups[g].fonts_count);
            if (groups[g].fonts_count > 0) {
                printf(": ");
                print_font_id(&groups[g].fonts[0].id);
                /* Print font file path */
                printf(" (");
                print_font_path(registry, &groups[g].fonts[0].id);
                printf(")");
            }
            printf("\n");
        }
        fc_css_fallback_groups_free(groups, groups_count);
        printf("\n");
    }

    /* ── Phase 3: Render text with the resolved chains ──────────────── */
    printf("--- Phase 3: Render Text ---\n\n");

    printf("  Using the sans-serif chain for text layout:\n\n");
    print_runs(chains[0], cache, registry, "Hello, World!");
    print_runs(chains[0], cache, registry, "The quick brown fox jumps over the lazy dog.");

    printf("  Using the serif chain:\n\n");
    print_runs(chains[1], cache, registry, "Lorem ipsum dolor sit amet.");

    printf("  Using the monospace chain:\n\n");
    print_runs(chains[2], cache, registry, "fn main() { println!(\"Hello\"); }");

    printf("  Multilingual text (sans-serif chain):\n\n");
    print_runs(chains[0], cache, registry,
               "Hello \xe4\xb8\x96\xe7\x95\x8c");                    /* Hello 世界 */
    print_runs(chains[0], cache, registry,
               "\xd0\x9f\xd1\x80\xd0\xb8\xd0\xb2\xd0\xb5\xd1\x82"  /* Привет */
               " World");
    print_runs(chains[0], cache, registry,
               "caf\xc3\xa9 na\xc3\xafve r\xc3\xa9sum\xc3\xa9");    /* café naïve résumé */

    /* Show render config for Arial */
    printf("  Per-font render config (from fonts.conf on Linux):\n\n");
    FcPattern* query = fc_pattern_new();
    fc_pattern_set_name(query, "Arial");
    FcTraceMsg* trace = NULL;
    size_t trace_count = 0;
    FcFontMatch* match = fc_cache_query(cache, query, &trace, &trace_count);
    if (match) {
        printf("    Arial render config (from fonts.conf on Linux):\n");
        FcFontRenderConfig rc = fc_cache_get_render_config(cache, &match->id);
        print_render_config(&rc);
        fc_font_match_free(match);
    }
    fc_trace_free(trace, trace_count);
    fc_pattern_free(query);
    printf("\n");

    /* ── Phase 4: Background loading continues ──────────────────────── */
    printf("--- Phase 4: Background Status ---\n\n");

    size_t count2 = 0;
    FcFontInfo* f2 = fc_registry_list_fonts(registry, &count2);
    printf("  Fonts now loaded: %zu\n", count2);
    printf("  Build complete:   %s\n", fc_registry_is_build_complete(registry) ? "yes" : "not yet — builders still parsing in background");
    printf("\n");
    printf("  If a new CSS rule appears (e.g. @font-face or a new element with\n");
    printf("  font-family: \"Fira Code\"), we simply call request_fonts() again.\n");
    printf("  It blocks only if that specific font hasn't been parsed yet.\n");
    fc_font_info_free(f2, count2);

    /* Cleanup */
    fc_cache_free(cache);
    for (size_t i = 0; i < num_chains; i++)
        fc_font_chain_free(chains[i]);
    fc_registry_chains_free(chains, num_chains);
    fc_registry_free(registry);

    printf("\n  Registry freed (background threads shut down).\n\n");
}

/* ── Demo 2: Blocking on a second font stack mid-frame ───────────────── */

static void demo_incremental_loading(void) {
    printf("====================================================================\n");
    printf("  Demo 2: Incremental Font Loading (block on demand)\n");
    printf("====================================================================\n");
    printf("\n");
    printf("  Shows what happens when the app discovers it needs MORE fonts\n");
    printf("  after the first layout pass. request_fonts() blocks again, but\n");
    printf("  only for the new fonts — previously loaded fonts are instant.\n");
    printf("\n");

    FcFontRegistry registry = fc_registry_new();
    fc_registry_spawn(registry);

    /* --- First request: basic UI fonts --- */
    printf("  First request: [\"Arial\", \"sans-serif\"]\n");

    const char* stack0[] = {"Arial", "sans-serif"};
    const char** stacks0[] = {stack0};
    size_t counts0[] = {2};
    size_t n0 = 0;
    FcFontChain* chains0 = fc_registry_request_fonts(registry, stacks0, counts0, 1, &n0);

    size_t loaded_after_first = 0;
    FcFontInfo* f = fc_registry_list_fonts(registry, &loaded_after_first);
    fc_font_info_free(f, loaded_after_first);
    printf("    Fonts loaded after first request: %zu\n", loaded_after_first);
    printf("    Build complete: %s\n\n",
           fc_registry_is_build_complete(registry) ? "yes" : "no");

    /* --- Second request: code editor opened, needs monospace --- */
    printf("  User opens code editor. Now we also need monospace fonts.\n");
    printf("  Second request: [\"Menlo\", \"Consolas\", \"Courier New\", \"monospace\"]\n");

    const char* stack1[] = {"Menlo", "Consolas", "Courier New", "monospace"};
    const char** stacks1[] = {stack1};
    size_t counts1[] = {4};
    size_t n1 = 0;
    FcFontChain* chains1 = fc_registry_request_fonts(registry, stacks1, counts1, 1, &n1);

    size_t loaded_after_second = 0;
    f = fc_registry_list_fonts(registry, &loaded_after_second);
    fc_font_info_free(f, loaded_after_second);
    printf("    Fonts loaded after second request: %zu\n", loaded_after_second);
    printf("    Build complete: %s\n\n",
           fc_registry_is_build_complete(registry) ? "yes" : "no");

    /* --- Third request: user pastes Japanese text --- */
    printf("  User pastes Japanese text. We need CJK fonts.\n");
    printf("  Third request: [\"Hiragino Sans\", \"Noto Sans CJK JP\", \"sans-serif\"]\n");

    const char* stack2[] = {"Hiragino Sans", "Noto Sans CJK JP", "sans-serif"};
    const char** stacks2[] = {stack2};
    size_t counts2[] = {3};
    size_t n2 = 0;
    FcFontChain* chains2 = fc_registry_request_fonts(registry, stacks2, counts2, 1, &n2);

    size_t loaded_after_third = 0;
    f = fc_registry_list_fonts(registry, &loaded_after_third);
    fc_font_info_free(f, loaded_after_third);
    printf("    Fonts loaded after third request: %zu\n", loaded_after_third);
    printf("    Build complete: %s\n\n",
           fc_registry_is_build_complete(registry) ? "yes" : "no");

    /* Show text rendering with all three chains */
    FcFontCache cache = fc_registry_snapshot(registry);

    printf("  Rendering with all three chains:\n\n");
    printf("  Sans-serif:\n");
    print_runs(chains0[0], cache, registry, "Quick UI text");
    printf("  Monospace:\n");
    print_runs(chains1[0], cache, registry, "let x = 42;");
    printf("  CJK:\n");
    print_runs(chains2[0], cache, registry,
               "\xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\xe3\x81\xae"  /* 日本語の */
               "\xe3\x83\x86\xe3\x82\xad\xe3\x82\xb9\xe3\x83\x88"); /* テキスト */

    /* Cleanup */
    fc_cache_free(cache);
    for (size_t i = 0; i < n0; i++) fc_font_chain_free(chains0[i]);
    for (size_t i = 0; i < n1; i++) fc_font_chain_free(chains1[i]);
    for (size_t i = 0; i < n2; i++) fc_font_chain_free(chains2[i]);
    fc_registry_chains_free(chains0, n0);
    fc_registry_chains_free(chains1, n1);
    fc_registry_chains_free(chains2, n2);
    fc_registry_free(registry);
    printf("  Done.\n\n");
}

/* ── Demo 3: Old blocking API vs new async API ───────────────────────── */

static void demo_old_vs_new(void) {
    printf("====================================================================\n");
    printf("  Demo 3: Old API (blocking) vs New API (async)\n");
    printf("====================================================================\n\n");

    /* --- Old API --- */
    printf("  OLD API: fc_cache_build()\n");
    printf("    Scans and parses ALL system fonts before returning.\n");
    printf("    Building...\n");
    FcFontCache old_cache = fc_cache_build();

    size_t old_count = 0;
    FcFontInfo* old_fonts = fc_cache_list_fonts(old_cache, &old_count);
    printf("    Loaded %zu fonts. App was BLOCKED until all finished.\n", old_count);
    printf("\n");

    /* Show a few fonts from the old cache */
    printf("    First 5 fonts:\n");
    for (size_t i = 0; i < 5 && i < old_count; i++) {
        printf("      %s", old_fonts[i].name ? old_fonts[i].name : "(unnamed)");
        if (old_fonts[i].family)
            printf(" [%s]", old_fonts[i].family);
        printf("\n");
    }
    fc_font_info_free(old_fonts, old_count);
    fc_cache_free(old_cache);

    printf("\n");

    /* --- New API --- */
    printf("  NEW API: fc_registry_new() + fc_registry_request_fonts()\n");
    printf("    Only blocks for the fonts the current frame needs.\n");
    printf("    Creating and spawning...\n");
    FcFontRegistry registry = fc_registry_new();
    fc_registry_spawn(registry);

    printf("    Requesting [\"Arial\", \"sans-serif\"] ...\n");
    const char* stack[] = {"Arial", "sans-serif"};
    const char** stacks[] = {stack};
    size_t counts[] = {2};
    size_t n = 0;
    FcFontChain* chains = fc_registry_request_fonts(registry, stacks, counts, 1, &n);

    size_t new_count = 0;
    FcFontInfo* new_fonts = fc_registry_list_fonts(registry, &new_count);
    printf("    Loaded %zu of %zu total system fonts.\n", new_count, old_count);
    printf("    Build complete: %s\n", fc_registry_is_build_complete(registry) ? "yes" : "no");
    printf("    The remaining %zu fonts are still loading in the background.\n",
           old_count > new_count ? old_count - new_count : 0);
    printf("    The app can render its first frame NOW.\n");
    fc_font_info_free(new_fonts, new_count);

    for (size_t i = 0; i < n; i++) fc_font_chain_free(chains[i]);
    fc_registry_chains_free(chains, n);
    fc_registry_free(registry);
    printf("\n  Done.\n\n");
}

/* ── Main ────────────────────────────────────────────────────────────── */

int main(int argc, char** argv) {
    const char* demo = (argc > 1) ? argv[1] : "all";

    if (strcmp(demo, "all") == 0 || strcmp(demo, "1") == 0)
        demo_azul_pattern();
    if (strcmp(demo, "all") == 0 || strcmp(demo, "2") == 0)
        demo_incremental_loading();
    if (strcmp(demo, "all") == 0 || strcmp(demo, "3") == 0)
        demo_old_vs_new();

    return 0;
}
