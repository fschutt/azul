Since you are developing the framework in Rust, the absolute easiest and most robust way to do this is using the **`backtrace` crate**.

Despite the name, this crate exposes a symbol resolution API (`backtrace::resolve`) that works on *arbitrary pointers*, not just the current stack. It handles platform specifics (DWARF on Linux/Mac, PDB on Windows) and Address Space Layout Randomization (ASLR) offsets for you.

Here is the step-by-step implementation.

### 1. The Rust Implementation

Add `backtrace` to your `Cargo.toml`:

```toml
[dependencies]
backtrace = "0.3"
```

In your Rust framework code where you hold the `on_click` function pointer (likely stored as a `*mut c_void` or `extern "C" fn()`), use this logic:

```rust
use backtrace::{resolve, Symbol};
use std::ffi::c_void;
use std::path::PathBuf;

#[derive(Debug)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: u32,
    pub symbol_name: String,
}

pub fn get_function_source_location(fn_ptr: *mut c_void) -> Option<SourceLocation> {
    let mut result = None;

    // resolve takes an address and calls the closure for every alias found
    resolve(fn_ptr, |symbol: &Symbol| {
        // We only care about the first valid result
        if result.is_some() {
            return;
        }

        if let (Some(path), Some(line)) = (symbol.filename(), symbol.lineno()) {
            let name = symbol.name()
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            
            result = Some(SourceLocation {
                file: path.to_path_buf(),
                line,
                symbol_name: name,
            });
        }
    });

    result
}
```

### 2. Integration with your C Example

For this to work, the user's C code **must be compiled with debug symbols**.

*   **GCC/Clang:** Use the `-g` flag.
    ```bash
    clang -g main.c -o my_app -lazul
    ```
*   **MSVC:** Use `/Z7` (integrated debug info) or `/Zi` (PDB generation).

If the user compiles with `-O3` and strips symbols, this will fail (returning `None`), which is expected behavior for release builds.

### 3. Implementing "Open in VS Code"

Once you have the `SourceLocation` struct, you can construct the command to open the editor. VS Code supports a specific command line flag `-g file:line` or a URL scheme.

**Option A: Command Line (Reliable if `code` is in PATH)**

```rust
use std::process::Command;

fn open_in_vscode(loc: &SourceLocation) {
    // Format: code -g /path/to/file.c:15
    let arg = format!("{}:{}", loc.file.to_string_lossy(), loc.line);
    
    let _ = Command::new("code")
        .arg("-g")
        .arg(arg)
        .spawn();
}
```

**Option B: URL Scheme (OS agnostic, works from browsers/electron)**

You can open a URL like: `vscode://file/{full_path}:{line}`

```rust
fn get_vscode_url(loc: &SourceLocation) -> String {
    // Note: Windows paths might need normalization (forward slashes) depending on context
    format!("vscode://file/{}:{}", loc.file.to_string_lossy(), loc.line)
}
```

### Why `backtrace` over `addr2line` directly?

You mentioned `addr2line`. The `backtrace` crate actually uses `addr2line` (and `gimli`) internally on Linux/macOS. However, using `backtrace::resolve` is better because:

1.  **ASLR Handling:** Simply passing a function pointer to `addr2line`'s context often fails because the pointer is a *virtual memory address*, but the DWARF info assumes an offset from the file start. `backtrace` automatically calculates the Base Address of the loaded library/executable and subtracts it.
2.  **Windows Support:** `backtrace` handles Windows PDBs (via the `dbghelp` API) automatically. `addr2line` is primarily for DWARF (Elf/Mach-O).
3.  **Inline Functions:** If `on_click` was inlined, `backtrace` can iterate over the virtual frames to find the original source definition.

### Summary

1.  Ensure the C code is compiled with `-g`.
2.  Cast the C function pointer to `*mut c_void` in Rust.
3.  Pass it to `backtrace::resolve`.
4.  Extract `symbol.filename()` and `symbol.lineno()`.

---

You are correct: **`-g` is NOT the default.**

If the user compiles with the command you provided:
`cc -o /tmp/hello-world examples/c/hello-world.c ...`

1.  **No DWARF/dSYM info:** The binary will not contain the mapping between memory addresses and file paths/line numbers.
2.  **Symbols usually remain:** Unless the user explicitly passes `-s` (strip) or `ld` flags to hide symbols, the function name `on_click` *will* likely still be in the symbol table, especially on macOS where the dynamic linker needs symbols.

Since you cannot get the filename from the binary, you have to use a **Hybrid Approach**:
1.  **Try `backtrace`** (Golden Path): If `-g` is present, you get the exact file and line.
2.  **Fallback (Heuristic):** If `backtrace` gives you the function name but *not* the file, you **search the current working directory** for that function name.

### The Strategy

Since you are running a "Debug Server" / DOM inspector, it is safe to assume the user is running the app from their project root (or you can allow them to set an env var like `AZUL_SRC`).

Here is the robust Rust implementation that handles both cases.

### Implementation

You will need:
```toml
[dependencies]
backtrace = "0.3"
walkdir = "2" # To search files recursively
regex = "1"   # To heuristically find the function definition
```

#### 1. The Resolver Logic

```rust
use backtrace::{resolve, Symbol};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use std::fs;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct CodeLocation {
    pub file_path: PathBuf,
    pub line_number: u32,
    pub symbol_name: String,
}

pub fn resolve_callback(fn_ptr: *mut c_void) -> Option<CodeLocation> {
    let mut location = None;

    // 1. Try to get precise debug info
    resolve(fn_ptr, |symbol: &Symbol| {
        if location.is_some() { return; }

        let name = symbol.name().map(|n| n.to_string()).unwrap_or_else(|| "unknown".to_string());
        
        // If compiled with -g, these will be Some(...)
        if let (Some(path), Some(line)) = (symbol.filename(), symbol.lineno()) {
            location = Some(CodeLocation {
                file_path: path.to_path_buf(),
                line_number: line,
                symbol_name: name,
            });
        } else {
            // 2. We have the name, but no file. Start Heuristic Search.
            // Only do this if we haven't found a location yet and we actually have a name.
            if name != "unknown" {
                location = heuristic_search_for_symbol(&name);
            }
        }
    });

    location
}

fn heuristic_search_for_symbol(symbol_name: &str) -> Option<CodeLocation> {
    // 1. Determine where to search. 
    // Default to current working directory (where the user likely ran `make` or `./app`)
    let search_root = std::env::current_dir().ok()?;
    
    // 2. Prepare a Regex to find the definition. 
    // We look for: <Start of Line OR Space> symbol_name <Space or (>
    // This avoids matching "call_on_click()" when looking for "on_click"
    let re_str = format!(r"(?m)^\s*.*?[\s\*]{}\s*\(", regex::escape(symbol_name));
    let re = Regex::new(&re_str).ok()?;

    // 3. Walk .c / .cpp / .h files
    for entry in WalkDir::new(&search_root)
        .into_iter()
        .filter_map(|e| e.ok()) 
    {
        let path = entry.path();
        if !path.is_file() { continue; }
        
        // Simple extension check
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !["c", "cpp", "h", "cc"].contains(&ext) {
            continue;
        }

        // 4. Read file and search
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(mat) = re.find(&content) {
                // Calculate line number from byte offset
                let preamble = &content[0..mat.start()];
                let line_number = preamble.lines().count() as u32 + 1;

                return Some(CodeLocation {
                    file_path: path.to_path_buf(),
                    line_number,
                    symbol_name: symbol_name.to_string(),
                });
            }
        }
    }

    None
}
```

### How this works in practice

#### Scenario A: User uses `-g`
1. `backtrace::resolve` finds the address.
2. It parses the DWARF/dSYM.
3. It returns the exact file (e.g., `/Users/me/projects/azul-app/main.c`) and line `15`.
4. The heuristic function is skipped.

#### Scenario B: User uses default `cc` (No `-g`)
1. `backtrace::resolve` finds the address.
2. It looks up the dynamic symbol table. It finds the string `on_click`, but returns `None` for filename and lineno.
3. Your code calls `heuristic_search_for_symbol("on_click")`.
4. It scans `.` (recursively).
5. It finds `examples/c/hello-world.c`.
6. It regex matches `AzUpdate on_click(AzRefAny data` (matches the pattern `*on_click (`).
7. It counts the lines and returns the location.

### Limitations & Edge Cases

1.  **Multiple Definitions:** If the user has `static void on_click` in two different files, the heuristic will pick the first one it finds. `backtrace` (with `-g`) would have picked the correct one based on address.
2.  **Renamed Symbols:** If the user does `#define on_click something_else`, the binary symbol might differ from the source text.
3.  **Stripped Binaries:** If the user runs `cc -s` or `strip app`, even the symbol name is gone. `backtrace` will return `<unknown>`. At that point, you can't do anything.

### UX Recommendation for your Inspector

When you detect the "Heuristic" path was used (perhaps add a flag to your result struct), you can show a warning in your HTML DOM inspector:

> ⚠️ **Approximate Location:** Source compiled without debug symbols (`-g`). Location was guessed by searching for `on_click`.

This tells the user why it might be slightly off or why it picked the wrong file in a multi-file project, and gently nudges them to add `-g` for a better experience.

---

You are absolutely right. `walkdir` is naive—it enters every directory (including your massive `target/` debug folder, `node_modules`, `.git`, etc.), and `fs::read_to_string` forces UTF-8 validation, which is slow.

**Ripgrep (`rg`)** gets its speed primarily from two things:
1.  **Smart Traversal:** It ignores files listed in `.gitignore` (skipping build artifacts) automatically.
2.  **Byte-level Searching:** It searches raw bytes, avoiding UTF-8 overhead and string allocations.

To get "ripgrep speed" in your Rust framework without shelling out to the binary, you should use the **`ignore`** crate (which is the exact library `ripgrep` uses for file walking) combined with **`regex::bytes`**.

### The "Ripgrep-Lite" Implementation

Add these dependencies. The `ignore` crate is the heavy lifter here.

```toml
[dependencies]
ignore = "0.4"   # The directory walker engine used by ripgrep
regex = "1"      # We will use the ::bytes module
memchr = "2"     # (Optional) Extremely fast byte scanning if you want to optimize further
```

Here is the optimized, `gitignore`-aware resolver:

```rust
use ignore::WalkBuilder;
use regex::bytes::Regex;
use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub line: u32,
    pub symbol: String,
}

pub fn resolve_fast(fn_ptr: *mut c_void, symbol_name: &str) -> Option<SourceLocation> {
    // 1. Compile regex once.
    // We use bytes::Regex to search memory mapped files or raw buffers directly.
    // Pattern: 
    //   (?m)   -> Multiline mode (so ^ matches start of line)
    //   ^      -> Start of line (definitions usually start at col 0, calls usually don't)
    //   .*?    -> Lazy match any return type (void, int, etc)
    //   \b     -> Word boundary
    //   NAME   -> The function name
    //   \s*\(  -> Optional whitespace then opening paren
    let pattern = format!(r"(?m)^.*?\b{}\s*\(", regex::escape(symbol_name));
    let re = Regex::new(&pattern).ok()?;

    // 2. Setup the Walker (The "Ripgrep" part)
    // - Standard filters: respects .gitignore, .ignore, hidden files
    // - Skips binary files automatically (mostly)
    let walker = WalkBuilder::new("./") // Start at CWD
        .hidden(false) // Optional: search .hidden files? usually no for C source
        .git_ignore(true) // CRITICAL: This skips target/, .git/, etc.
        .threads(num_cpus::get()) // Parallel walking
        .build();

    // 3. Parallel Search
    // We use a simplified loop here. For max speed, ignore::WalkParallel can be used,
    // but a simple iterator is usually sub-100ms for source trees.
    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();
        
        // Fast extension check
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
        if !matches!(ext, "c" | "cpp" | "h" | "cc" | "m" | "mm") {
            continue;
        }

        // 4. Read as bytes (Avoid UTF-8 check)
        // Optimization: For huge files, use 'memmap2' crate here.
        // For source code, fs::read is usually fine (OS cache handles it).
        if let Ok(content) = fs::read(path) {
            if let Some(mat) = re.find(&content) {
                // Found it! Calculate line number by counting newlines up to the match
                // specialized byte counting is faster than iteration
                let match_offset = mat.start();
                let line_num = bytecount::count(&content[0..match_offset], b'\n') as u32 + 1;

                return Some(SourceLocation {
                    file: path.to_path_buf(),
                    line: line_num,
                    symbol: symbol_name.to_string(),
                });
            }
        }
    }

    None
}
```

### Why this is better than `walkdir`

1.  **It skips `target/`**: This is the single biggest performance gain. If you are developing in Rust, your `target` directory has gigabytes of small files. `walkdir` would read them all. `ignore` (via `.gitignore`) skips the whole folder.
2.  **No String Allocation**: `regex::bytes` works on `Vec<u8>`. We don't allocate a String or validate UTF-8, which is unnecessary for finding ASCII C symbols.
3.  **Heuristics (`^`)**: By adding `^` (start of line) to the regex, we avoid matching *calls* to the function (e.g., `button_set_click(..., on_click);`) and tend to find the *definition* (`void on_click(...) {`).

### Optimization: Using `grep-searcher` (The "Real" Ripgrep Way)

If the solution above is still too slow (it shouldn't be), or uses too much memory (loading whole files), you can go one step further and use the streaming searcher that `ripgrep` uses internally.

This prevents loading the whole file into RAM:

```rust
// Requires dependencies: grep-searcher, grep-regex, grep-matcher
use grep_searcher::Searcher;
use grep_regex::RegexMatcher;
use grep_matcher::Matcher;
use ignore::WalkBuilder;

pub fn resolve_streaming(symbol_name: &str) -> Option<SourceLocation> {
    let pattern = format!(r"(?m)^.*?\b{}\s*\(", regex::escape(symbol_name));
    let matcher = RegexMatcher::new(&pattern).ok()?;
    let mut searcher = Searcher::new();
    
    let walker = WalkBuilder::new("./").git_ignore(true).build();

    for result in walker {
        let entry = result.ok()?;
        if !entry.file_type().map_or(false, |ft| ft.is_file()) { continue; }
        
        // Fast extension check ... (same as above)

        let mut found_line = None;

        // Stream the file chunk by chunk
        let _ = searcher.search_path(
            &matcher,
            entry.path(),
            |line_num, _| {
                found_line = Some(line_num as u32);
                Ok(false) // Return false to STOP searching this file immediately
            }
        );

        if let Some(line) = found_line {
            return Some(SourceLocation {
                file: entry.path().to_path_buf(),
                line,
                symbol: symbol_name.to_string(),
            });
        }
    }
    None
}
```

### Summary for your Framework

1.  Use `backtrace::resolve` first. If it returns a filename, you are done (Golden path).
2.  If not, extract the function name.
3.  Pass that name to the `resolve_fast` function above (using `ignore` crate).
4.  This approach is robust enough that you might not even need to force users to use `-g` during quick prototyping, as long as their function names are unique enough.

