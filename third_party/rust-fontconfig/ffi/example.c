#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rust_fontconfig.h"

// Function to display program usage
void print_usage(const char* program_name) {
    printf("Rust FontConfig Example\n");
    printf("=======================\n\n");
    printf("A tool to search and display information about fonts on your system.\n\n");
    printf("USAGE:\n");
    printf("  %s [COMMAND] [ARGUMENTS]\n\n", program_name);
    printf("COMMANDS:\n");
    printf("  list              - List all available fonts\n");
    printf("  search <name>     - Search for a specific font and display detailed information\n");
    printf("  resolve <text>    - Demonstrate two-step font resolution for text (new in 1.2.0)\n");
    printf("  demo              - Run multilingual demo (Chinese, Arabic, Cyrillic)\n");
    printf("  help              - Display this help message\n\n");
    printf("EXAMPLES:\n");
    printf("  %s list\n", program_name);
    printf("  %s search Arial\n", program_name);
    printf("  %s search \"Times New Roman\"\n", program_name);
    printf("  %s resolve \"Hello World!\"\n", program_name);
    printf("  %s demo\n", program_name);
}

// Function to read entire file into memory
unsigned char* read_file(const char* path, size_t* size_out) {
    // Skip memory: prefix for in-memory fonts
    if (strncmp(path, "memory:", 7) == 0) {
        fprintf(stderr, "Cannot read in-memory font file directly\n");
        return NULL;
    }

    FILE* file = fopen(path, "rb");
    if (!file) {
        fprintf(stderr, "Failed to open file: %s\n", path);
        return NULL;
    }

    // Get file size
    fseek(file, 0, SEEK_END);
    long size = ftell(file);
    fseek(file, 0, SEEK_SET);

    if (size <= 0) {
        fclose(file);
        return NULL;
    }

    // Allocate buffer and read file
    unsigned char* buffer = (unsigned char*)malloc(size);
    if (!buffer) {
        fclose(file);
        return NULL;
    }

    size_t bytes_read = fread(buffer, 1, size, file);
    fclose(file);

    if (bytes_read != (size_t)size) {
        free(buffer);
        return NULL;
    }

    *size_out = (size_t)size;
    return buffer;
}

// Function to display font metadata
void print_font_metadata(const FcFontMetadata* metadata) {
    if (!metadata) {
        printf("No metadata available\n");
        return;
    }
    
    printf("Font Metadata:\n");
    printf("  Full Name: %s\n", metadata->full_name ? metadata->full_name : "Unknown");
    printf("  Family: %s\n", metadata->font_family ? metadata->font_family : "Unknown");
    printf("  Subfamily: %s\n", metadata->font_subfamily ? metadata->font_subfamily : "Unknown");
    printf("  PostScript Name: %s\n", metadata->postscript_name ? metadata->postscript_name : "Unknown");
    
    if (metadata->copyright)
        printf("  Copyright: %s\n", metadata->copyright);
    
    if (metadata->version)
        printf("  Version: %s\n", metadata->version);
    
    if (metadata->designer)
        printf("  Designer: %s\n", metadata->designer);
    
    if (metadata->manufacturer)
        printf("  Manufacturer: %s\n", metadata->manufacturer);
    
    if (metadata->license)
        printf("  License: %s\n", metadata->license);
}

// Function to list all fonts in the cache
void list_fonts(FcFontCache cache) {
    size_t count = 0;
    FcFontInfo* fonts = fc_cache_list_fonts(cache, &count);
    if (!fonts) {
        printf("No fonts found in cache\n");
        return;
    }

    printf("Found %zu fonts:\n", count);
    for (size_t i = 0; i < count; i++) {
        char id_str[40];
        if (fc_font_id_to_string(&fonts[i].id, id_str, sizeof(id_str))) {
            printf("%3zu. ID: %s\n", i+1, id_str);
            printf("     Name: %s\n", fonts[i].name ? fonts[i].name : "Unknown");
            printf("     Family: %s\n", fonts[i].family ? fonts[i].family : "Unknown");
        } else {
            printf("%3zu. ID: <error converting ID>\n", i+1);
        }
        
        if (i < count - 1) {
            printf("\n");
        }
    }

    fc_font_info_free(fonts, count);
}

// Function to search for a font and display its details
int search_and_display_font(FcFontCache cache, const char* font_name) {
    printf("Searching for font: %s\n", font_name);
    
    FcPattern* pattern = fc_pattern_new();
    if (!pattern) {
        fprintf(stderr, "Failed to create pattern\n");
        return 1;
    }
    
    fc_pattern_set_name(pattern, font_name);
    
    FcTraceMsg* trace = NULL;
    size_t trace_count = 0;
    FcFontMatch* match = fc_cache_query(cache, pattern, &trace, &trace_count);
    
    if (!match) {
        printf("No font found matching '%s'\n", font_name);
        fc_pattern_free(pattern);
        if (trace) fc_trace_free(trace, trace_count);
        return 1;
    }
    
    printf("\n--- Font Match for '%s' ---\n\n", font_name);
    
    char id_str[40];
    if (fc_font_id_to_string(&match->id, id_str, sizeof(id_str))) {
        printf("Font ID: %s\n", id_str);
    } else {
        printf("Font ID: <error converting ID>\n");
    }
    
    printf("Unicode ranges: %zu\n", match->unicode_ranges_count);
    for (size_t i = 0; i < match->unicode_ranges_count && i < 5; i++) {
        printf("  Range %zu: U+%04X - U+%04X\n", i, 
               match->unicode_ranges[i].start, 
               match->unicode_ranges[i].end);
    }
    
    if (match->unicode_ranges_count > 5) {
        printf("  ... and %zu more ranges\n", match->unicode_ranges_count - 5);
    }
    
    // Get the font path
    FcFontPath* font_path = fc_cache_get_font_path(cache, &match->id);
    if (font_path) {
        printf("\nFont path: %s (index: %zu)\n", font_path->path, font_path->font_index);
        
        // Get and print font metadata
        FcFontMetadata* metadata = fc_cache_get_font_metadata(cache, &match->id);
        if (metadata) {
            printf("\n");
            print_font_metadata(metadata);
            fc_font_metadata_free(metadata);
        }
        
        // Only try to load the file if it's not an in-memory font
        if (strncmp(font_path->path, "memory:", 7) != 0) {
            // Load font file into memory
            size_t font_size = 0;
            unsigned char* font_data = read_file(font_path->path, &font_size);
            
            if (font_data) {
                printf("\nLoaded font data: %zu bytes\n", font_size);
                
                // Create memory font
                FcFont* memory_font = fc_font_new(font_data, font_size, font_path->font_index, "memory-font");
                if (memory_font) {
                    printf("Created in-memory font\n");
                    
                    // Add memory font to cache
                    FcPattern* mem_pattern = fc_pattern_new();
                    if (mem_pattern) {
                        char memory_name[256];
                        snprintf(memory_name, sizeof(memory_name), "Memory-%s", font_name);
                        fc_pattern_set_name(mem_pattern, memory_name);
                        
                        fc_cache_add_memory_fonts(cache, mem_pattern, memory_font, 1);
                        printf("Added memory font to cache with name: %s\n", memory_name);
                        
                        fc_pattern_free(mem_pattern);
                    }
                    
                    fc_font_free(memory_font);
                }
                
                free(font_data);
            }
        }
        
        fc_font_path_free(font_path);
    } else {
        printf("\nWARNING: Failed to get font path\n");
    }
    
    // Display fallback fonts
    if (match->fallbacks_count > 0) {
        printf("\nFallback fonts: %zu\n", match->fallbacks_count);
        for (size_t i = 0; i < match->fallbacks_count && i < 3; i++) {
            FcFontMatchNoFallback* fallback = &match->fallbacks[i];
            if (fc_font_id_to_string(&fallback->id, id_str, sizeof(id_str))) {
                printf("  Fallback %zu: %s (%zu ranges)\n", i, id_str, 
                       fallback->unicode_ranges_count);
            }
        }
        
        if (match->fallbacks_count > 3) {
            printf("  ... and %zu more fallbacks\n", match->fallbacks_count - 3);
        }
    }
    
    // Cleanup
    fc_font_match_free(match);
    fc_pattern_free(pattern);
    if (trace) fc_trace_free(trace, trace_count);
    
    return 0;
}

// Function to display detailed font information by name
int display_detailed_font_info(FcFontCache cache, const char* font_name) {
    printf("Searching for font: %s\n", font_name);
    
    FcPattern* pattern = fc_pattern_new();
    if (!pattern) {
        fprintf(stderr, "Failed to create pattern\n");
        return 1;
    }
    
    fc_pattern_set_name(pattern, font_name);
    
    FcTraceMsg* trace = NULL;
    size_t trace_count = 0;
    FcFontMatch* match = fc_cache_query(cache, pattern, &trace, &trace_count);
    
    if (!match) {
        printf("No font found matching '%s'\n", font_name);
        fc_pattern_free(pattern);
        if (trace) fc_trace_free(trace, trace_count);
        return 1;
    }
    
    char id_str[40];
    fc_font_id_to_string(&match->id, id_str, sizeof(id_str));
    
    printf("\n=== Detailed Information for '%s' ===\n\n", font_name);
    printf("Font ID: %s\n\n", id_str);
    
    // Get and print font metadata
    FcFontMetadata* metadata = fc_cache_get_font_metadata(cache, &match->id);
    if (metadata) {
        printf("METADATA:\n");
        printf("  Full Name: %s\n", metadata->full_name ? metadata->full_name : "Unknown");
        printf("  Family: %s\n", metadata->font_family ? metadata->font_family : "Unknown");
        printf("  Subfamily: %s\n", metadata->font_subfamily ? metadata->font_subfamily : "Unknown");
        printf("  PostScript Name: %s\n", metadata->postscript_name ? metadata->postscript_name : "Unknown");
        
        if (metadata->copyright)
            printf("  Copyright: %s\n", metadata->copyright);
        
        if (metadata->version)
            printf("  Version: %s\n", metadata->version);
        
        if (metadata->designer)
            printf("  Designer: %s\n", metadata->designer);
        
        if (metadata->manufacturer)
            printf("  Manufacturer: %s\n", metadata->manufacturer);
        
        fc_font_metadata_free(metadata);
    }
    
    // Get the font path
    FcFontPath* font_path = fc_cache_get_font_path(cache, &match->id);
    if (font_path) {
        printf("\nFILE INFORMATION:\n");
        printf("  Path: %s\n", font_path->path);
        printf("  Font Index: %zu\n", font_path->font_index);
        fc_font_path_free(font_path);
    } else {
        printf("\nWARNING: Failed to get font path\n");
    }
    
    printf("\nUNICODE COVERAGE:\n");
    for (size_t i = 0; i < match->unicode_ranges_count && i < 10; i++) {
        printf("  Range %zu: U+%04X - U+%04X\n", i, 
               match->unicode_ranges[i].start, 
               match->unicode_ranges[i].end);
    }
    
    if (match->unicode_ranges_count > 10) {
        printf("  ... and %zu more ranges\n", match->unicode_ranges_count - 10);
    }
    
    // Fallback fonts
    if (match->fallbacks_count > 0) {
        printf("\nFALLBACK FONTS:\n");
        for (size_t i = 0; i < match->fallbacks_count && i < 5; i++) {
            FcFontMatchNoFallback* fallback = &match->fallbacks[i];
            char fb_id_str[40];
            
            if (fc_font_id_to_string(&fallback->id, fb_id_str, sizeof(fb_id_str))) {
                printf("  Fallback %zu: %s (%zu ranges)\n", i, fb_id_str, 
                       fallback->unicode_ranges_count);
            }
        }
        
        if (match->fallbacks_count > 5) {
            printf("  ... and %zu more fallbacks\n", match->fallbacks_count - 5);
        }
    }
    
    // Cleanup
    fc_font_match_free(match);
    fc_pattern_free(pattern);
    if (trace) fc_trace_free(trace, trace_count);
    
    return 0;
}

// Function to demonstrate the new two-step font resolution API (1.2.0)
int resolve_text_with_chain(FcFontCache cache, const char* text) {
    printf("=== Two-Step Font Resolution Demo (1.2.0 API) ===\n\n");
    printf("Text: \"%s\"\n\n", text);
    
    // Step 1: Create a font chain from CSS font families
    // This is equivalent to CSS: font-family: "Noto Sans", "Arial", sans-serif;
    const char* families[] = {"Noto Sans", "Arial", "Helvetica", "sans-serif"};
    size_t families_count = sizeof(families) / sizeof(families[0]);
    
    printf("Step 1: Resolving font chain for CSS font-family stack:\n");
    for (size_t i = 0; i < families_count; i++) {
        printf("  %zu. %s\n", i + 1, families[i]);
    }
    printf("\n");
    
    FcTraceMsg* trace = NULL;
    size_t trace_count = 0;
    
    FcFontChain chain = fc_resolve_font_chain(
        cache,
        families,
        families_count,
        FC_WEIGHT_NORMAL,
        FC_MATCH_FALSE,  // italic
        FC_MATCH_FALSE,  // oblique
        &trace,
        &trace_count
    );
    
    if (!chain) {
        printf("ERROR: Failed to resolve font chain\n");
        if (trace) fc_trace_free(trace, trace_count);
        return 1;
    }
    
    printf("Font chain resolved successfully!\n\n");
    
    // Optionally, show what's in the chain
    size_t groups_count = 0;
    FcCssFallbackGroup* groups = fc_chain_get_css_fallbacks(chain, &groups_count);
    if (groups && groups_count > 0) {
        printf("CSS Fallback Groups in chain: %zu\n", groups_count);
        for (size_t i = 0; i < groups_count && i < 5; i++) {
            printf("  %zu. \"%s\" -> %zu font(s)\n", 
                   i + 1, groups[i].css_name, groups[i].fonts_count);
        }
        if (groups_count > 5) {
            printf("  ... and %zu more groups\n", groups_count - 5);
        }
        printf("\n");
        fc_css_fallback_groups_free(groups, groups_count);
    }
    
    // Step 2: Query fonts for the text
    printf("Step 2: Querying fonts for text...\n\n");
    
    size_t runs_count = 0;
    FcResolvedFontRun* runs = fc_chain_query_for_text(chain, cache, text, &runs_count);
    
    if (!runs || runs_count == 0) {
        printf("No font runs returned (text may be empty or no fonts found)\n");
        fc_font_chain_free(chain);
        if (trace) fc_trace_free(trace, trace_count);
        return 0;
    }
    
    printf("Font Runs: %zu\n", runs_count);
    printf("----------------------------------------\n");
    
    for (size_t i = 0; i < runs_count; i++) {
        FcResolvedFontRun* run = &runs[i];
        
        printf("Run %zu:\n", i + 1);
        printf("  Text: \"%s\"\n", run->text);
        printf("  Bytes: %zu - %zu\n", run->start_byte, run->end_byte);
        printf("  CSS Source: %s\n", run->css_source);
        
        if (run->has_font) {
            char id_str[40];
            if (fc_font_id_to_string(&run->font_id, id_str, sizeof(id_str))) {
                printf("  Font ID: %s\n", id_str);
            }
            
            // Get more info about this font
            FcFontMetadata* metadata = fc_cache_get_font_metadata(cache, &run->font_id);
            if (metadata) {
                printf("  Font: %s\n", metadata->full_name ? metadata->full_name : "Unknown");
                fc_font_metadata_free(metadata);
            }
        } else {
            printf("  Font: NONE (no font can render this text)\n");
        }
        
        printf("\n");
    }
    
    // Cleanup
    fc_resolved_runs_free(runs, runs_count);
    fc_font_chain_free(chain);
    if (trace) fc_trace_free(trace, trace_count);
    
    printf("=== Demo Complete ===\n");
    return 0;
}

// Demonstrate multilingual text resolution with Chinese, Arabic, and mixed scripts
int demo_multilingual_resolution(FcFontCache cache) {
    printf("=== Multilingual Font Resolution Demo ===\n\n");
    printf("This demo shows how rust-fontconfig resolves fonts for text\n");
    printf("containing multiple scripts (Latin, Chinese, Arabic, Cyrillic).\n\n");
    
    // Test strings with different scripts
    const char* test_strings[] = {
        "Hello World!",                          // Latin only
        "你好世界",                               // Chinese only  
        "مرحبا بالعالم",                          // Arabic only (right-to-left)
        "Привет мир",                            // Cyrillic only
        "Hello 你好 مرحبا Привет",                // Mixed: Latin + Chinese + Arabic + Cyrillic
        "Price: $99.99 / 价格：¥699 / السعر: 50﷼", // Mixed with numbers and currencies
    };
    const char* test_descriptions[] = {
        "Latin only",
        "Chinese only",
        "Arabic only (RTL)",
        "Cyrillic only",
        "Mixed scripts",
        "Mixed with numbers/currencies",
    };
    size_t num_tests = sizeof(test_strings) / sizeof(test_strings[0]);
    
    // Use a realistic CSS font stack that would be used for multilingual content
    const char* families[] = {
        "Noto Sans",           // Good Unicode coverage
        "Arial Unicode MS",    // Fallback with wide coverage
        "Helvetica",           // Common Latin font
        "PingFang SC",         // Chinese (Simplified)
        "Hiragino Sans GB",    // Chinese fallback
        "Arial",               // Latin fallback
        "sans-serif"           // System default
    };
    size_t families_count = sizeof(families) / sizeof(families[0]);
    
    printf("CSS Font Stack:\n");
    printf("  font-family: ");
    for (size_t i = 0; i < families_count; i++) {
        printf("\"%s\"%s", families[i], i < families_count - 1 ? ", " : ";\n");
    }
    printf("\n");
    
    // Resolve the font chain once (this is cached internally)
    FcTraceMsg* trace = NULL;
    size_t trace_count = 0;
    
    FcFontChain chain = fc_resolve_font_chain(
        cache, families, families_count,
        FC_WEIGHT_NORMAL, FC_MATCH_FALSE, FC_MATCH_FALSE,
        &trace, &trace_count
    );
    
    if (!chain) {
        printf("ERROR: Failed to resolve font chain\n");
        if (trace) fc_trace_free(trace, trace_count);
        return 1;
    }
    
    // Show resolved chain info
    size_t groups_count = 0;
    FcCssFallbackGroup* groups = fc_chain_get_css_fallbacks(chain, &groups_count);
    if (groups && groups_count > 0) {
        printf("Resolved %zu font groups in chain\n\n", groups_count);
        fc_css_fallback_groups_free(groups, groups_count);
    }
    
    // Free initial trace, we'll get new traces per query if needed
    if (trace) {
        fc_trace_free(trace, trace_count);
        trace = NULL;
        trace_count = 0;
    }
    
    // Process each test string
    printf("========================================\n");
    for (size_t t = 0; t < num_tests; t++) {
        printf("\nTest %zu: %s\n", t + 1, test_descriptions[t]);
        printf("Input: \"%s\"\n", test_strings[t]);
        printf("----------------------------------------\n");
        
        size_t runs_count = 0;
        FcResolvedFontRun* runs = fc_chain_query_for_text(chain, cache, test_strings[t], &runs_count);
        
        if (!runs || runs_count == 0) {
            printf("  No fonts found for this text\n");
            continue;
        }
        
        printf("  Resolved into %zu run(s):\n", runs_count);
        
        for (size_t i = 0; i < runs_count; i++) {
            FcResolvedFontRun* run = &runs[i];
            
            printf("\n  Run %zu: \"%s\"\n", i + 1, run->text);
            printf("    Byte range: %zu-%zu\n", run->start_byte, run->end_byte);
            
            if (run->has_font) {
                printf("    CSS source: %s\n", run->css_source);
                
                FcFontMetadata* metadata = fc_cache_get_font_metadata(cache, &run->font_id);
                if (metadata) {
                    printf("    Font: %s\n", metadata->full_name ? metadata->full_name : "Unknown");
                    if (metadata->font_family) {
                        printf("    Family: %s\n", metadata->font_family);
                    }
                    fc_font_metadata_free(metadata);
                }
                
                // Get font path
                FcFontPath* path = fc_cache_get_font_path(cache, &run->font_id);
                if (path) {
                    printf("    Path: %s\n", path->path);
                    fc_font_path_free(path);
                }
            } else {
                printf("    Font: NONE (no font supports these characters)\n");
            }
        }
        
        fc_resolved_runs_free(runs, runs_count);
        printf("\n========================================\n");
    }
    
    fc_font_chain_free(chain);
    
    printf("\n=== Demo Complete ===\n");
    printf("\nNote: The actual fonts used depend on what's installed on your system.\n");
    printf("Different operating systems will resolve to different fonts.\n");
    
    return 0;
}

int main(int argc, char** argv) {
    // Default to showing help if no arguments provided
    if (argc == 1) {
        print_usage(argv[0]);
        return 0;
    }
    
    // Build the font cache
    FcFontCache cache = fc_cache_build();
    if (!cache) {
        fprintf(stderr, "Failed to build font cache\n");
        return 1;
    }
    
    int result = 0;
    
    // Parse command line arguments
    if (strcmp(argv[1], "list") == 0) {
        // List all fonts
        list_fonts(cache);
    } else if (strcmp(argv[1], "search") == 0) {
        if (argc < 3) {
            fprintf(stderr, "Error: 'search' command requires a font name argument\n\n");
            print_usage(argv[0]);
            result = 1;
        } else {
            // Search for specific font with detailed info
            result = display_detailed_font_info(cache, argv[2]);
        }
    } else if (strcmp(argv[1], "resolve") == 0) {
        if (argc < 3) {
            fprintf(stderr, "Error: 'resolve' command requires a text argument\n\n");
            print_usage(argv[0]);
            result = 1;
        } else {
            // Demonstrate two-step font resolution
            result = resolve_text_with_chain(cache, argv[2]);
        }
    } else if (strcmp(argv[1], "demo") == 0) {
        // Run multilingual demonstration
        result = demo_multilingual_resolution(cache);
    } else if (strcmp(argv[1], "help") == 0 || strcmp(argv[1], "--help") == 0 || 
               strcmp(argv[1], "-h") == 0) {
        print_usage(argv[0]);
    } else {
        // For backward compatibility: treat unknown command as a font name to search
        result = search_and_display_font(cache, argv[1]);
    }
    
    fc_cache_free(cache);
    return result;
}
