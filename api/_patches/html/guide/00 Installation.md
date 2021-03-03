<h2>Installing a supported Rust compiler</h2>

<p>
Azul (version 0.1.0) is supported on Rust 1.31 or newer. Please ensure that you have the right compiler
version, by running <code>rustc -vV</code> after installation to check the version number.
</p>

<h3>Installation on Windows</h3>

<div class="warning">
  <h4>Note</h4>
  <p>
  If you get prompted to install the <strong>MSVC toolchain</strong>, please download both the
  Visual C++ tools (available from
  <a href="https://www.visualstudio.com/downloads/#build-tools-for-visual-studio-2017">here</a>)
  and install the Windows 8 or 10 SDK (which contains the actual linker) from
  the Visual Studio Tools installer.
</p>
</div>

<div class="warning">
  <h4>Note</h4>
  <p>
    If you choose the <strong>MinGW toolchain</strong> on windows, 32-bit support is missing,
    due to problems with a C dependency. Because of this, we recommend you to use
    the MSVC toolchain on Windows or use a 64-bit compiler.
</p>
</div>

<code class="expand">DownloadFile https://win.rustup.rs/ -FileName rustup-init.exe
rustup-init -yv --default-toolchain stable --default-host x86_64-pc-windows-msvc</code>

<p>...or download the windows installer manually from
  <a href="https://win.rustup.rs/">win.rustup.rs</a>
</p>

<h3>Installation on Linux / Mac</h3>

<code class="expand"><pre>curl https://sh.rustup.rs -sSf | sh</pre></code>

<p>
If you do not trust executing a remote script, download
the script seperately, review it and then execute it. You can
also find the installation instructions for Rust on the
<a href="https://www.rust-lang.org/en-US/install.html">main website</a>.
</p>

<div class="warning">
<h4>Notes for NixOS</h4>

  <p>
  In order to install the dependencies for NixOS, copy this into a `shell.nix`
  in your directory of choice and run `nix-shell` to install the dependencies.
  </p>

  <code class="expand">with import &gt;nixpkgs&lt; {};

stdenv.mkDerivation {
  name = "rust-env";
  nativeBuildInputs = [
    rustup
    pkgconfig
    python3
  ];

  buildInputs = [
    freetype
    xorg.libxcb
    # for dialogs
    gnome3.zenity
  ];

  LD_LIBRARY_PATH = stdenv.lib.makeLibraryPath [
    xorg.libX11
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
    libglvnd
  ];
}</code>
</div>

## System dependencies

You currently need to install [CMake](https://cmake.org/download/) before you can use azul.
This applies to all platforms, since CMake is used during the build process to compile
`servo-freetype` and `harfbuzz-sys`.

### Linux

On Linux, you additionally need to install `libexpat-dev` and `libfreetype6-dev` in order
to compile `servo-freetype-sys`(see [#42](https://github.com/maps4print/azul/issues/42)).

For interfacing with the system clipboard, you also need `libxcb-xkb-dev`.
Since azul uses the system-native fonts by default, you'll also need
`libfontconfig1-dev` (which includes expat and freetype2).

Your users will need to install `libfontconfig` and
`libxcb-xkb1` installed (remember this for packaging rpm or deb packages).

Lastly, if there is no OpenGL available, azul will fallback and try to find
`libEGL.so`. Especially when testing inside of a VM, it is important to install
`libgles2-mesa-dev` - note that the software will compile if this library
isn't present, it just won't run without it.

```
sudo apt install \
    cmake libxcb-xkb-dev libfontconfig1-dev libgles2-mesa-dev \
    libfreetype6-dev libexpat-dev
```

**Note for Arch Linux:** On Arch, the package for `libfontconfig1-dev`
seems to be called `fontconfig`.

Other dependencies are statically linked in all binaries, you don't need to
worry about managing them.

## Creating a new project

Once you have Rust and Cargo installed, create a new project in the
directory of your choice:

```bash
cargo new --bin my_first_azul_app
cd my_first_azul_app
cargo run
```

## Adding azul to your dependencies

Open the /my_first_azul_app/Cargo.toml file and paste the following
lines beneath the [dependencies] section:

```toml
[dependencies]
azul = { git = "https://github.com/maps4print/azul" }
```

**WARNING: ** Azul has not yet been released on crates.io, therefore,
there is no package available. This guide will be updated once the
first stable version releases. It is recommended to version-lock your
dependency with `{ git = "...", rev = "the_last_commit_hash" }`, so
that you know which commit you are using.

This will pull in the latest stable version of azul. Open the
`/my_first_azul_app/src/main.rs` file and edit it to look like this:

```rust
extern crate azul;

fn main() {
    println!("Hello world!");
}
```

Ensure that azul builds correctly by running cargo run again. Azul
does not require third-party dependencies (dynamic libraries), it
should build out-of-the-box. If that's the case, you are ready to
move on to the next page, starting to actually build your first GUI
app with azul. If not, please report this as a bug.