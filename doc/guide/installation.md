# Installation

## Using a precompiled binary release

In order to use Azul from languages other than Rust you need to use a 
pre-compiled binary release from [/releases](https://azul.rs/releases). 
Even for Rust it is recommended to use the precompiled binary to avoid compilation 
issues (missing build-dependencies, long compilation time). The precompiled 
library is dependency-free, you do not need to install or extra 
system libraries in order to get started.

### Rust

Initialize a new cargo project with `cargo new my-project` and 
edit your `Cargo.toml` file:

```
[dependencies]
azul = "1.0.0-alpha5"
# or: azul = { git = "https://azul.rs/1.0.0-alpha5.git" }
```

Run cargo with: `cargo build --release`. NOTE: For regular Rust development,
you're done now, but you might want to use the dynamic linking for much faster
re-compilation times.

### Rust: Dynamic Linking

In order to improve iteration time, it is highly recommended to use the `.so` 
/ `.dylib` for dynamic linking. This allows you to recompile your app in 
seconds without recompiling Azul itself.

#### Step 1: Get the Library

Download the `libazul.so` (Linux) or `libazul.dylib` (macOS) and place it in 
your project's `target/debug` or `target/release` folder (ideally both). The 
goal is to have the library in the same directory as the final compiled binary.

#### Step 2: Configure cargo

First, enable the `link_dynamic` feature in your `Cargo.toml`:

```toml
[dependencies.azul]
version = "1.0.0-alpha5"
default-features = false
features = ["link_dynamic"]
```

Next, set the `AZUL_LINK_PATH` environment variable to tell Azul's build 
script where the library is located. This is a **build-time** variable.

```sh
# The path should be to the *directory* containing the library
export AZUL_LINK_PATH=$(pwd)/target/release
```

Internally, the azul crate has a build script, which will 
pick up on this environment variable and tell the Rust compiler 
where the DLL is.

#### Step 3: Solve the Run-Time Linking

When you run your binary, the OS needs to know where to find `libazul.so`. 
The best method is to embed this search path directly into your executable 
using `RPATH`.

Create a file named `.cargo/config.toml` in your project's root directory 
(create the `.cargo` folder if it doesn't exist) and add the following:

```toml
[build]
rustflags = [
  # Windows: nothing to add, because Windows will look in the
  # directory of the .exe for .dll files by default
  
  # Linux: tells the binary to look for libraries in the same directory it is in
  "-C", "link-arg=-Wl,-rpath,$ORIGIN",

  # macOS: the equivalent of $ORIGIN is @loader_path
  "-C", "link-arg=-Wl,-rpath,@loader_path",
]
```

Now, you can compile and run your project with a single command, and it will 
just work. The resulting binary is portable and self-contained, as long as 
`libazul.so` is next to it (or installed as a dependency). If you now want to
give your binary / library to a friend, you just need to bundle up the (or on 
Linux: specify libazul as a package of your repositorys manager).

```sh
# Set the build-time variable and run the project
AZUL_LINK_PATH=$(pwd)/target/release cargo run --release
```

If you see an error like this:

```rust
Compiling azul v0.0.1
error: environment variable `AZUL_LINK_PATH` not defined
 --> [...]\api\rust\build.rs:4:48
  |
4 |  println!("cargo:rustc-link-search={}", env!("AZUL_LINK_PATH"));
  |                                         ^^^^^^^^^^^^^^^^^^^^^^
```

... it means you forgot to set the `AZUL_LINK_PATH` as explained above.

This is what your project should look like:

```bash
/my-project
    /src
        main.rs
    /target
        /release
            my-project.exe
            azul.dll
            azul.dll.lib // <- Important on Windows!
    Cargo.toml
```

Now your code should run (if it doesn't, [file a bug](https://github.com/fschutt/azul/issues)).

You can now continue with the [Rust Getting Started guide](https://azul.rs/guide/getting-started-rust.html), 
which will teach you how to create a window.

### Python

In order to use Azul from Python, download the appropriate Python extension for your platform:

- Windows: `azul.pyd`
- Linux: `azul.cpython.so` (rename to `azul.so`)
- macOS: `azul.so`

Put the file in the same directory as your `main.py` file. 

Note that because of Python extension conventions the name of the file 
must be `azul.so` (or `azul.pyd` on Windows) for Python to import it correctly.

Create a main.py file with the following contents:

```python
from azul import *
```

Your project directory should now look like this:

```
/my-project:
   azul.pyd (Windows) or azul.so (Linux/macOS)
   main.py
```

Now you should be able to run the code with `python3 ./main.py`.

> [!WARNING]
> Azul only supports **Python 3**. Make sure you are 
> using the correct Python version or explicitly use `python3 ./main.py`

### C / C++

For C or C++ you will need to download the 
regular library without Python support:

- Windows: azul.dll and azul.dll.lib
- Linux: libazul.so
- MacOS: libazul.dylib

Then, create a main.c or main.cpp file with the following contents:

```c
#include "azul.h"

int main() {
  return 0;
}
```

Download the `azul.h` or `azul.hpp` file and put it in the same directory.
Your project directory should now look like this:

```sh
/my-project:
   azul.dll | libazul.so | libazul.dylib
   azul.dll.lib # windows only!
   azul.h | azul.hpp
   main.c | main.cpp
```

Compile with:

```sh
gcc -O3 -lazul -L/path/to/my-project ./main.c
```

> [!WARNING]
> Azul is compatible with either **C99** or **C++11** and newer.

Now you can run the example with `.main`.

> [!WARNING]
> On Linux the binary won't immediately start because Linux expects the library
> in the `LD_PRELOAD` path. To fix this, copy the `libazul.so`
> either into `/usr/lib` or into `/usr/x86_64-linux-gnu/lib`:
> ```sh 
> sudo copy ./libazul.so /usr/lib
> ```

### C / C++: static linking

In order to statically link from C / C++, simply use the `.a` or `.lib` files
(`libazul.lib`, `libazul.linux.a` or `libazul.macos.a`), remove the OS-specific
naming and link the project statically with GCC.

### Building the DLL

libazul is being built by the `doc` binary in /doc, which automatically
builds the azul.rs website, the binaries and generates documentation:

```sh
git clone https://github.com/fschutt/azul && cd azul
cargo build --release --manifest-path doc/Cargo.toml
```

If you only want to build the dll for your current operating system,
you can alternatively do:

```sh
cargo build \
  --release \
  --manifest-path dll/Cargo.toml \
  --no-default-features \
  --features desktop-dynamic
```

The API is versioned in the `api.json`, including all examples, guides and examples
on a per-version basis. Newer APIs never replace older APIs, but get versioned with
a new number (i.e. `AzV1Class_function(AzV1Struct)`)
