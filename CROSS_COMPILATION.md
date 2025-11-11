# Cross-Compilation Guide for Azul

This guide explains how to cross-compile Azul from macOS to Linux and Windows targets.

## Prerequisites

- macOS (Apple Silicon or Intel)
- Rust toolchain installed via rustup
- Homebrew package manager

## Setup

### 1. Install Rust Target Support

```bash
# Add Linux target
rustup target add x86_64-unknown-linux-gnu

# Add Windows target
rustup target add x86_64-pc-windows-gnu
```

### 2. Install Cross-Compilation Toolchains

#### Linux (x86_64-unknown-linux-gnu)

```bash
# Add the tap for macOS cross-compilation toolchains
brew tap messense/macos-cross-toolchains

# Install the Linux x86_64 GNU toolchain
brew install x86_64-unknown-linux-gnu
```

This installs:
- `x86_64-unknown-linux-gnu-gcc` - C compiler
- `x86_64-unknown-linux-gnu-g++` - C++ compiler
- `x86_64-unknown-linux-gnu-ar` - Archiver
- Complete Linux glibc headers and libraries

**Note:** The toolchain uses glibc 2.17 (from 2012), ensuring compatibility with older Linux distributions like CentOS 7, Ubuntu 14.04, and Debian 8.

#### Windows (x86_64-pc-windows-gnu)

```bash
# Install MinGW-w64 for Windows cross-compilation
brew install mingw-w64
```

This installs:
- `x86_64-w64-mingw32-gcc` - C compiler
- `x86_64-w64-mingw32-g++` - C++ compiler
- `x86_64-w64-mingw32-ar` - Archiver
- Complete Windows headers and libraries

## Building

### Check Compilation (Fast, No Linking)

```bash
cd dll

# Check Linux target
cargo check --target x86_64-unknown-linux-gnu

# Check Windows target
cargo check --target x86_64-pc-windows-gnu

# Check macOS target (native)
cargo check --target aarch64-apple-darwin  # Apple Silicon
cargo check --target x86_64-apple-darwin   # Intel
```

### Build Release Binaries

```bash
cd dll

# Build for Linux
cargo build --release --bin kitchen_sink --target x86_64-unknown-linux-gnu

# Build for Windows
cargo build --release --bin kitchen_sink --target x86_64-pc-windows-gnu

# Build for macOS (native)
cargo build --release --bin kitchen_sink
```

### Output Locations

After building, the binaries will be located at:

- **Linux**: `target/x86_64-unknown-linux-gnu/release/kitchen_sink`
- **Windows**: `target/x86_64-pc-windows-gnu/release/kitchen_sink.exe`
- **macOS**: `target/release/kitchen_sink` (or `target/aarch64-apple-darwin/release/kitchen_sink`)

## Verification

### Check Binary Type

```bash
# Linux binary
file target/x86_64-unknown-linux-gnu/release/kitchen_sink
# Output: ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), dynamically linked...

# Windows binary
file target/x86_64-pc-windows-gnu/release/kitchen_sink.exe
# Output: PE32+ executable (console) x86-64 (stripped to external PDB), for MS Windows

# macOS binary
file target/release/kitchen_sink
# Output: Mach-O 64-bit executable arm64
```

### Check Binary Size

```bash
# Show sizes of all built binaries
ls -lh target/*/release/kitchen_sink*
```

## Configuration

The cross-compilation configuration is stored in `.cargo/config.toml` at the repository root. Key settings:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "x86_64-unknown-linux-gnu-gcc"

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
ar = "x86_64-w64-mingw32-ar"

[profile.release]
lto = "thin"           # Link-time optimization
codegen-units = 16     # Parallel code generation
```

## Troubleshooting

### Error: `memfd_create` not found (Linux)

This is automatically handled via fallback to `shm_open` for older glibc versions. The code uses syscall directly if available, otherwise falls back to POSIX shared memory.

### Error: Linker not found

Make sure the toolchains are installed and in your PATH:

```bash
# Check Linux toolchain
which x86_64-unknown-linux-gnu-gcc
# Should output: /opt/homebrew/bin/x86_64-unknown-linux-gnu-gcc

# Check Windows toolchain
which x86_64-w64-mingw32-gcc
# Should output: /opt/homebrew/bin/x86_64-w64-mingw32-gcc
```

### Error: Multiple taps with same formula

If you get a tap conflict, uninstall the existing formula first:

```bash
brew uninstall x86_64-unknown-linux-gnu
brew install messense/macos-cross-toolchains/x86_64-unknown-linux-gnu
```

### Slow Compilation

Cross-compilation can be slower than native compilation. To speed up development:

1. Use `cargo check` instead of `cargo build` during development
2. Only build release binaries when needed
3. Consider using [`sccache`](https://github.com/mozilla/sccache) for caching

## Alternative Methods

### Option 1: Using Docker (Most Compatible)

```bash
# Install cross (Docker-based cross-compilation)
cargo install cross

# Build with cross
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target x86_64-pc-windows-gnu
```

**Pros:**
- Most compatible (uses real Linux/Windows environments)
- No host toolchain conflicts
- Reproducible builds

**Cons:**
- Requires Docker Desktop
- Slower (container overhead)
- Larger disk usage

### Option 2: Using Zig (Experimental)

```bash
# Install zig and cargo-zigbuild
brew install zig
cargo install cargo-zigbuild

# Build with zigbuild
cargo zigbuild --release --target x86_64-unknown-linux-gnu
cargo zigbuild --release --target x86_64-pc-windows-gnu
```

**Pros:**
- No separate toolchains needed
- Fast setup
- Good glibc compatibility

**Cons:**
- Experimental (may have linking issues)
- Not all C libraries are supported

## Testing

Cross-compiled binaries cannot be run directly on macOS. To test:

1. **Linux**: Transfer to a Linux machine or use Docker:
   ```bash
   docker run --rm -v $(pwd)/target/x86_64-unknown-linux-gnu/release:/app ubuntu:22.04 /app/kitchen_sink
   ```

2. **Windows**: Transfer to a Windows machine or use Wine:
   ```bash
   brew install wine-stable
   wine target/x86_64-pc-windows-gnu/release/kitchen_sink.exe
   ```

## CI/CD Integration

For automated builds, use GitHub Actions or similar CI:

```yaml
# .github/workflows/build.yml
name: Build
on: [push]
jobs:
  build-all:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install toolchains
        run: |
          brew tap messense/macos-cross-toolchains
          brew install x86_64-unknown-linux-gnu mingw-w64
      - name: Build all targets
        run: |
          rustup target add x86_64-unknown-linux-gnu x86_64-pc-windows-gnu
          cd dll
          cargo build --release --target x86_64-unknown-linux-gnu
          cargo build --release --target x86_64-pc-windows-gnu
          cargo build --release
```

## License Compatibility

The cross-compilation toolchains are open-source:
- **x86_64-unknown-linux-gnu**: GPL (GCC + glibc)
- **mingw-w64**: Public domain + permissive licenses

These licenses only affect the toolchain binaries themselves, not your compiled code.

## Additional Resources

- [Rust Cross-Compilation Guide](https://rust-lang.github.io/rustup/cross-compilation.html)
- [messense/homebrew-macos-cross-toolchains](https://github.com/messense/homebrew-macos-cross-toolchains)
- [rust-cross/rust-musl-cross](https://github.com/rust-cross/rust-musl-cross)
- [MinGW-w64 Project](https://www.mingw-w64.org/)
