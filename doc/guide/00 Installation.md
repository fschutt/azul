<h2>Using a precompiled binary release</h2>

  <p>
    In order to use Azul from languages other than Rust you need to use a
    pre-compiled binary release from
    <a href="$$ROOT_RELATIVE$$/releases">$$ROOT_RELATIVE$$/releases</a>.
    <br/>
    <br/>
    Even for Rust it is recommended to use the precompiled binary to avoid
    compilation issues (missing build-dependencies, long compilation time).
    The precompiled library is dependency-free, you do not need to install or
    extra system libraries in order to get started.
  </p>

  <br/>

  <h3>Python</h3>

  <p>
    In order to use Azul from Python, download the <code>azul.pyd</code> file
    (or <code>azul.so</code> on Linux / Mac) and put it in to the same directory as your
    <code>main.py</code> file. Note that because of Python extension
    conventions the name of the file must match the name of the library (i.e.
    <code>azul.so</code> instead of <code>libazul.so</code>).
  </p>

  <p>Create a main.py file with the following contents:</p>

  <code class="expand">from azul import *</code>

  <p>Your project directory should now look like this:</p>

  <code class="expand">/my-project:
   azul.pyd (or: azul.so)
   main.py</code>

  <p>Now you should be able to run the code with</p>

  <code class="expand">python ./main.py</code>

  <br/>
  <div class="warning">
    <i><h4>"I received a weird Python error"</h4></i>

    <p>Azul only supports <strong>Python 3</strong>.
    Make sure you are using the correct Python version or explicitly use <code>python3 ./main.py</code>.</p>
  </div>

  <br/>
  <h3>C / C++</h3>

  <p>For C or C++ you will need to download the regular
  library without Python support:</p>

  <br/>

  <ul>
    <li><p>&nbsp;&nbsp;Windows:&nbsp;<code>azul.dll</code> and <code>azul.dll.lib</code></p></li>
    <li><p>&nbsp;&nbsp;Linux:&nbsp;<code>libazul.so</code></p></li>
    <li><p>&nbsp;&nbsp;Mac:&nbsp;<code>libazul.dylib</code></p></li>
  </ul>

  <p>Create a main.c or main.cpp file with the following contents:</p>

  <code class="expand">#include "azul.h"

int main() {
  return 0;
}
</code>

  <p>Your project directory should now look like this:</p>

  <code class="expand">/my-project:
   azul.dll / libazul.so / libazul.dylib
   azul.dll.lib // windows only!
   azul.h / azul.hpp
   main.c / main.cpp</code>

  <p>Compile with:</p>

  <code class="expand">gcc -O3 -lazul -L/path/to/my-project ./main.c</code>

  <br/>
  <div class="warning">
    <i><h4>Compatibility</h4></i>
    <p>Azul is compatible with either <strong>C99</strong> or <strong>C++11</strong> and newer.</p>
  </div>

  <p>Now you can run the example with </p>
  <code class="expand">./main</code>

  <br/>

  <div class="warning">
    <i><h4>Running on Linux: libazul not found</h4></i>
    <p>
      On Linux the binary won't immediately start because Linux expects the library
      in the <code>LD_PRELOAD</code> path. To fix this, copy the <code>libazul.so</code>
      either into <code>/usr/lib</code> or into <code>usr/x86_64-linux-gnu/lib</code>:
      <code class="expand">sudo copy ./libazul.so /usr/lib</code>
    </p>
  </div>

  <br/>
  <h3>Rust</h3>

  <p>
    Initialize a new cargo project with <code>cargo new my-project</code> and edit your
    <code>Cargo.toml</code> file:
  </p>

<code class="expand">[dependencies]
azul = "1.0.0-alpha"</code>

  <p>Run cargo with:</p>
  <code class="expand">cargo build --release</code>
  <p style="font-weight: bold;font-size:14px;">This will throw an error because the compiler doesn't know where the azul.dll file is:</p>
  <code class="expand">Compiling azul v0.0.1
error: environment variable `AZUL_LINK_PATH` not defined
 --> [...]\api\rust\build.rs:4:48
  |
4 |         println!("cargo:rustc-link-search={}", env!("AZUL_LINK_PATH"));
  |                                                ^^^^^^^^^^^^^^^^^^^^^^
  |</code>

  <p>
    In order to tell rustc about the library, you will need to set the environment
    variable <code>AZUL_LINK_PATH</code> to the path of the downloaded library.
  </p>

  <p>On Linux or Mac the operating system needs the library to be in the
    <code>LD_PRELOAD</code> path, so you can just <code>AZUL_LINK_PATH</code>
    to the same path:
  </p>

  <code class="expand">sudo cp ./libazul.so /usr/lib
export AZUL_LINK_PATH=/usr/lib
cargo run --release</code>

  <p>
    On Windows things are different: First, you will also need to put the
    <code>azul.dll.lib</code> file into the same directory as the
    <code>azul.dll</code>file and second,
    in order to run the binary, the <code>azul.dll</code> file has
    to be in the same directory as the .exe.
  </p>
  <p>
    For Rust development it makes the most sense to put both files into the
    <code>/target/release</code> directory:
  </p>
  <code class="expand">/my-project
    /src
        main.rs
    /target
        /release
            my-project.exe
            azul.dll
            azul.dll.lib
    Cargo.toml
</code>
<code class="expand">SET AZUL_LINK_PATH=/path/to/my-project/target/release
cargo run --release
</code>

<p>Now your code should run (if it doesn't, file a bug).
  You can now continue with the next tutorial which will teach you how to
  <a href="$$ROOT_RELATIVE$$/guide">create a window.</a></p>

<br/>
<h2>Static linking</h2>

  <h3>C / C++</h3>

  <p>In order to link azul staticall from C or C++, apply the same procedure
    as for the dynamically linked version, but use the <code>libazul.a</code>
    file to link statically (as a single binary) instead of dynamically (as a
    split binary + azul.dll).
  </p>

  <h3>Rust</h3>
    <p>In order to link Rust as a regular crates.io dependency, you need to enable the
      <code>link-static</code> feature:</p>
    <code class="expand">[dependencies]
azul = { version = "1.0.0-alpha", features = ["link-static", "rlib"]}</code>
    <p>By default this feature is disabled because re-linking the application can
    take a long time (even if compilation is fast). Usually you'd only want to do
  this when you are finished with the development of your application.</p>

  <br/>


<h2>Building the library from source</h2>

  <p>Azul is mostly self-contained and using a default Rust installation you should
    be able to just compile it with:</p>
    <code class="expand">git clone https://github.com/fschutt/azul
cd azul-dll
cargo build --release
    # --features="python3": optional Python support
    # --features="cdylib": compile as cdylib (.dll)
    # --features="staticlib": compile as static library (.a)</code>

    <br/>

    <h3>Development</h3>
  <p>Since Azul targets many languages, the bindings are auto-generated by a combination of a
    Python script (<code>build.py</code>) and a <code>api.json</code> file describing every class
    with the respective C field layout:
  </p>

  <code class="expand">{
  "App": {
    "external": "azul_impl::app::AzAppPtr",
    "doc": "Main application class",
    "struct_fields": [
        {"ptr": {"type": "*const c_void"}}
    ],
    "constructors": {
        "new": {
            ...
        }
    },
    ...
}</code>

  <p>The script then creates the proper API bindings. The API bindings only have
  to be refreshed when they are changed - which should happen infrequently.</p>

  <br/>
  <br/>

  <a href="$$ROOT_RELATIVE$$/guide">Back to overview</a>
