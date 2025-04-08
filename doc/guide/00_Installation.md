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
azul = "1.0.0-alpha"
# or: azul = { git = "https://azul.rs/1.0.0-alpha.git" }
```

Run cargo with: `cargo build --release`. 

### Rust: dynamic linking

In order to improve iteration time, it is highly recommended to use the .dll
for dynamic linking. Download the azul.dll / libazul.so and put it in your 
`myproject/target/debug` (or `target/release`) folder. The library has to be 
in the same directory as the binary, in order to be found when launching.

Then, export the path to the DLL file via 

```sh
export AZUL_LINK_PATH=/path/to/target/release/azul.dll
```
On Windows, use `set AZUL_LINK_PATH`. Internally, the azul crate has 
a build script, which will pick up on this environment variable
and tell the Rust compiler where the DLL is.

Now, enable the feature `link_dynamic` in your Cargo.toml:

```toml
[dependencies.azul]
version = "1.0.0-alpha1"
default-features = false
features = ["link_dynamic"]
```

If you see an error like this:

```rust
Compiling azul v0.0.1
error: environment variable `AZUL_LINK_PATH` not defined
 --> [...]\api\rust\build.rs:4:48
  |
4 |  println!("cargo:rustc-link-search={}", env!("AZUL_LINK_PATH"));
  |                                                ^^^^^^^^^^^^^^^^^^^^^^
```

... it means you forgot to set the `AZUL_LINK_PATH` as explained above.

On Linux or Mac the operating system needs the library to be in the `LD_PRELOAD` 
path, so you can just `AZUL_LINK_PATH` to the same path:

```
sudo cp ./libazul.so /usr/lib
export AZUL_LINK_PATH=/usr/lib/libazul.so
cargo run --release
```

On Windows things are different: First, you will also need to put the
`azul.dll.lib` file into the same directory as the `azul.dll` file 
and second, in order to run the binary, the `azul.dll` file has
to be in the same directory as the .exe.

Note that the DLL is built in release mode, so you can iterate on your binary
in debug mode, and when you're done programming your library, you can switch back
to static linking.

```bash
/my-project
    /src
        main.rs
    /target
        /release
            my-project.exe
            azul.dll
            azul.dll.lib
    Cargo.toml
```

```bash
SET AZUL_LINK_PATH=/path/to/my-project/target/release
cargo run --release
```

Now your code should run (if it doesn't, file a bug).
You can now continue with the [next tutorial](..), 
which will teach you how to create a window.

### Python

In order to use Azul from Python, download the `windows.pyd` / 
`linux.pyd` or `mac.pyd` and put it in to the same directory as 
your `main.py` file. 

Note that because of Python extension conventions the name of the file 
must match the name of the library (i.e. `azul.so` instead of `libazul.so`).

Create a main.py file with the following contents:

```python
from azul import *
```

Your project directory should now look like this:

```
/my-project:
   azul.pyd (or: azul.so)
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
