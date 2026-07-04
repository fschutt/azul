# Language bindings — end-user review board (2026-07-04)

27 per-language reviews by independent end-user-perspective agents (workflow
`binding-end-user-review`, run wf_d0f99fa4-26e), cross-checked against the
runtime e2e matrix (`scripts/e2e_language_matrix.sh`, debug-server dll).
Goal driving this: 20+ actually-shipped bindings on the azul.rs frontpage.

| lang | tier | install (1-5) | days to ship-quality | verdict (short) |
|------|------|---------------|----------------------|-----------------|
| kotlin | shipped-issues | 2 | 1 | Keep it shipped — the binding itself is one of the strongest JVM offerings (clean-compiling 6.3 MB artifact, pinned host-invoker callbacks, consume/close double |
| lua | shipped-issues | 3 | 1 | Keep Lua on the frontpage — the macOS arm64 path is real, idiomatic, and e2e-proven — but ship the honesty fixes now: the linux/windows install steps advertise  |
| c | shipped-issues | 2 | 1.5 | Keep C on the frontpage — the binding is real (3-OS click e2e in CI, widgets, clean-compiling examples) — but ship the two half-day fixes first: the macOS dylib |
| java | shipped-issues | 2 | 1.5 | Keep Java on the frontpage — the binding itself is one of the strongest managed-FFI ports (AutoCloseable ownership model, working counter e2e, 19 callback kinds |
| node | shipped-issues | 2 | 1.5 | Currently listed but broken end-to-end: the published install steps cannot load the library on macOS or Linux, and a codegen regression makes every smart callba |
| ruby | shipped-issues | 3 | 1.5 | Keep shipping Ruby — it is one of the strongest managed bindings (real callbacks, 43 widgets, auto conversions, careful consume/finalizer discipline) and the in |
| csharp | shipped-issues | 2 | 2 | Keep C# on the frontpage but treat it as shipped-with-issues: the binding is genuinely functional (counter e2e was a real pass, guide code compiles clean today, |
| python | shipped-issues | 4 | 2 | Keep python on the frontpage — the pyo3 extension is architecturally the best binding Azul has (real callbacks, same-instance data model, honest 2-command insta |
| rust | shipped-issues | 2 | 2 | Keep shipped but fix the install story urgently: the binding itself is the best of the 27 (native language, full RAII, modules, conversions, counter e2e genuine |
| ocaml | shipped-issues | 2 | 2 | Keep OCaml listed only after a ~2-day fix pass: the binding architecture (host-invoker callbacks, handle table, dlopen with AZ_DYLIB) is sound and was e2e-green |
| cpp | shipped-issues | 2 | 2.5 | Keep C++ on the frontpage but treat it as regressed: the cpp20 path and the std-interop design are genuinely strong, yet today a fresh user fails at compile (mi |
| scala | candidate-near | 2 | 2 | Ship after ~2 focused days: the underlying Java binding is genuinely solid and Scala rides it with zero extra codegen, but today there are no install docs, no g |
| zig | candidate-near | 2 | 2.5 | Do not ship today — the published install flow dead-ends (no hello-world.zig in the release) and the example itself no longer compiles — but this is the cheapes |
| go | candidate-near | 1 | 2.5 | Ship-able in ~2-3 focused days, but only as an honest "cgo against azul.h" story: the counter e2e is already green on the raw-cgo path, while the generated wrap |
| pascal | candidate-near | 2 | 3 | Closest-to-ship candidate of the non-shipped tier: the binding compiles end-to-end with zero drift and has full typed-callback plumbing, but do not list it unti |
| powershell | candidate-far | 1 | 4 | No-ship today: the shipped module cannot even be imported (fatal CS8632 under Add-Type) and the example targets a stale API, so the counter e2e has never run in |
| lisp (Common Lisp / SBCL, CFFI) | candidate-far | 1 | 5 | No-ship today: the install steps are un-followable (missing cffi-libffi dependency, phantom hello-world.lisp) and the generated wrapper layer has never executed |
| perl | candidate-far | 2 | 6 | No-ship: the Perl binding is a working FFI smoke layer wrapped in an idiomatic facade that is structurally unsound — record-valued handles are treated as opaque |
| haskell | candidate-far | 1 | 7 | No-ship: the binding is architecturally the most complete of the exotic candidates (13.8k imports, trampolines, brackets, per-method wrappers, clean GHC typeche |
| freebasic | candidate-far | 1 | 7 | No-ship: the FreeBASIC binding has never been through fbc and fails static review on at least four independent compile-blocking axes plus two ABI-corrupting one |
| ada | candidate-far | 1 | 8 | Do not ship: the Ada binding has never been compiled by any Ada compiler, carries three confirmed ABI-layout breaks (by-value record params, 8-byte placeholders |
| smalltalk | candidate-far | 1 | 9 | No-ship: the Smalltalk binding is currently unloadable in any Smalltalk dialect (pseudo-Tonel monolith), has no callback or string bridge, and its README/api.js |
| fortran | candidate-far | 1 | 10 | No-ship: the artifact a user would download today does not even compile, and beneath that regression sit two structural gaps — a wrong-ABI tagged-union represen |
| php | candidate-far | 1 | 12 | No-ship: the published artifact fatals at parse time, the FFI path can never support callbacks (php-ffi design limitation, correctly self-diagnosed by the code) |
| cobol | candidate-far | 2 | 12 | No-ship: the install steps run but deliver a console printout, and beneath the surface every tagged union and every struct-by-value call is ABI-broken, so the c |
| vb6 | blocked | 1 | 12 | No-ship, and realistically never-ship: the binding is an explicitly acknowledged codegen demonstration (mod.rs:7 — "the audience for this binding is essentially |
| algol68 | blocked | 1 | 20 | No-ship: hard-blocked — the binding targets an ALIEN FFI syntax that Algol 68 Genie does not implement (verified failing on a68g 3.11.3 today), so no program ca |

---


## kotlin — shipped-issues (install 2/5, ~1d to ship-quality)

A fresh macOS developer following the frontpage steps curls libazul.dylib and Azul.kt successfully, then hits a wall: the very next command is `kotlinc -cp $JNA_JAR Azul.kt HelloWorld.kt ...` but no step ever downloads HelloWorld.kt (it is also not on the release page — doc/src/dllgen/deploy.rs:830-832 ships only Azul.kt + gradle files) and $JNA_JAR is never explained or fetched. If they paste the counter example from the official guide page, it fails to compile with a type mismatch on `.onClick(m, onClick)` (verified with kotlinc 2.3.21). Only after using the repo's examples/kotlin/HelloWorld.kt (which compiles cleanly against the current binding), grabbing jna-5.14.0.jar from Maven Central, and adding the -J-Xmx4g heap flag the frontpage command omits does the ~3-minute compile of the 6.3 MB Azul.kt succeed; the runtime path itself (host-invoker callbacks, counter increment) is well-engineered and was E2E-verified on prior boards.


**Guide/install truthfulness issues:**
- Guide counter example DOES NOT COMPILE: doc/guide/en/hello-world/kotlin.md:85 declares the click handler as `AzulNativeManaged.CallbackInvokerCallback`, but `Button.onClick` requires the nominal type `AzulNativeManaged.ButtonOnClickCallbackInvokerCallback` (target/codegen/kotlin/Azul.kt:114443). kotlinc 2.3.21 verified: "argument type mismatch: actual type is 'AzulNativeManaged.CallbackInvokerCallback', but 'AzulNativeManaged.ButtonOnClickCallbackInvokerCallback' was expected". The repo example (examples/kotlin/HelloWorld.kt:10) uses the correct type and compiles clean — the guide is stale relative to it.
- api.json kotlin steps (all 3 platforms) compile `HelloWorld.kt` without ever downloading or creating it: there is no `curl ... HelloWorld.kt` step and deploy.rs's kotlin BindingFile list (doc/src/dllgen/deploy.rs:830-832) ships only Azul.kt, build.gradle.kts, settings.gradle.kts — unlike vb6 which does ship its HelloWorld driver (deploy.rs:806). Followed literally, the frontpage steps fail with 'source file not found'.
- api.json kotlinc step omits `-J-Xmx4g`, but the repo's own docs say the default heap is insufficient: examples/kotlin/gradle.properties states "the Kotlin compiler daemon needs a larger heap than the default to compile it (mirrors the kotlinc -J-Xmx4g invocation in the README)", and both README and guide build commands include -J-Xmx4g.
- api.json steps reference `$JNA_JAR` (`kotlinc -cp $JNA_JAR ...`, `java ... -cp hello-world.jar:$JNA_JAR`) with no step to obtain JNA and no hint it comes from Maven Central — a frontpage user has no way to satisfy this variable.
- Guide install section (kotlin.md:38-54) claims a live self-hosted maven2 repo `https://azul.rs/ui/maven` with `rs.azul:azul:0.2.0`. The CI job exists (.github/workflows/rust.yml:3913, maven-central, website mode) but is `continue-on-error: true` and packages the JAVA sources (`cp target/codegen/java/*.java`), so the maven path gives a Kotlin user the Java binding surface, not Azul.kt; liveness of the repo is unverifiable from the repo. Also mixing the jar with a local Azul.kt would duplicate every class in package com.azul — the guide never warns about this.
- examples/kotlin/README.md:74 says "HelloWorld.kt — 67-line Python-quality port" — it is 39 lines today; README.md:75 lists "Azul.kt — generated Kotlin bindings" under Files, but examples/kotlin contains no Azul.kt (it lives in target/codegen/kotlin/ or the release page; the README's kotlinc command assumes it is already in cwd without saying where to get it).
- Guide kotlin.md:129 says "`AzString.toString()` decodes UTF-8 into kotlin.String" — the wrapper class with that behavior is `AzulString` (renamed precisely to avoid shadowing, per README.md:45-47); `AzString` is the raw JNA Structure.


**Safety issues:**
- No finalizer/Cleaner backstop on any of the ~460 AutoCloseable wrappers (doc/src/codegen/v2/lang_kotlin/wrappers.rs:673-691 emits close()/__consume() only; `grep -c 'fun finalize' Azul.kt` = 0): a user who forgets `.use{}`/close() silently leaks native memory. Not a crash hazard — but it is the only resource-management story and relies entirely on user discipline.
- Typed `registerCallback<T>` (target/codegen/kotlin/Azul.kt:~135418): when refanyGet returns null, `__data as T` erases to a null passed as non-null T — the user callback NPEs inside the JNA callback thunk; JNA swallows it and native falls back to the pre-filled default (core/src/host_invoker.rs:450-453 pre-fills out), so the click is silently dropped with only a stderr trace. Should early-return like the non-null mismatch branch does.
- Vec types without a `_clone` export fall back to buffer-borrowed iteration (wrappers.rs:975: "don't keep yielded wrappers past the Vec's lifetime") — a doc-comment-only lifetime contract; wrappers with _clone (e.g. DomVec, Azul.kt:85560) deep-clone and are safe. The borrowed fallback immediately __consume()s the yielded wrapper (wrappers.rs:1018-1019) so most uses throw IllegalStateException rather than UAF — confusing but memory-safe.
- Positives worth recording: double-free is structurally prevented (idempotent close() + `closed` flag + __consume() ownership transfer on every by-value pass, e.g. Azul.kt:81754-81810 App); use-after-consume throws IllegalStateException("closed") instead of touching freed memory; all 20 invoker callbacks are pinned in AzulHostInvoker.livePins (Azul.kt:134898) so JNA cannot GC them mid-run; RefAny host handles are released via AzApp_setHostHandleReleaser fired from the native destructor (core/src/host_invoker.rs:180-205), so the handles map does not grow unboundedly across relayouts.


**Idiomatic-ness issues:**
- Enum parameters are raw Ints: `Button.withButtonType(button_type: Int)` (Azul.kt:114505) forces `AzButtonType.Primary.value` in user code (guide line 103, HelloWorld.kt:26) instead of accepting the `AzButtonType` enum — same for AzUpdate returns written as `.value` through an out-pointer.
- The flagship hello-world uses the LOW-level SAM (`ButtonOnClickCallbackInvokerCallback` with `Pointer?` args, manual `refanyGet` + `is` check + `outPtr!!.setInt(0, result)`) even though the binding ships the much nicer typed `CallbackWithData<T>` / `registerCallback(MyDataModel::class.java) { data, info -> ... }` path (Azul.kt:135410+, advertised in README:35-39). The best API exists but the showcase doesn't use it.
- Naming split-brain in one flat `com.azul` package: idiomatic wrappers (Dom, Button, App) coexist with Az-prefixed raw Structures (AzDom), Az-prefixed enums the user MUST touch (AzUpdate, AzButtonType), and AzulNative* interfaces the user must also touch (AzulNativeManaged.ButtonOnClickCallbackInvokerCallback) — a hello-world spans all three naming families.
- Boolean-returning natives surface as `Byte` (e.g. `RefCount.canBeShared(): Byte`, Azul.kt:80981) instead of Kotlin Boolean.
- `check(!closed) { "closed" }` yields IllegalStateException("closed") with no type or hint — terse for a user-facing lifecycle error.
- Generated code compiles with dozens of always-false-condition warnings (`if (ptr == null ...)` on non-nullable `val ptr: Pointer`, e.g. Azul.kt:125104) — cosmetic but makes every user build noisy.


**Ergonomics issues:**
- Compiling the single 6.3 MB / 136k-line Azul.kt takes ~3 minutes and needs a 4 GB compiler heap (measured: 3:03 with kotlinc 2.3.21 -J-Xmx4g); every clean build of a user project pays this. No prebuilt azul.jar/klib is offered for the kotlinc path (the maven jar exists but contains the Java binding).
- Hello-world itself is competitive: 39 lines with fluent builders and a real `m.counter += 1` callback — on par with the Python/Rust reference — but the `outPtr!!.setInt(0, result)` + `AzUpdate.RefreshDom.value` plumbing is the least-Kotlin part of it.
- The gradle path (examples/kotlin/build.gradle.kts) is genuinely good — daemon caching, jna.library.path wiring, SIP note — but it is repo-tied (defaults to ../../target/{codegen,release}) and neither api.json nor the guide's manual path mentions it as the recommended workflow.
- String/Vec/Option conversions are automatic and pleasant: methods take/return kotlin.String directly (Button.create(label: kotlin.String)), Vecs implement Iterable + toList()/toByteArray(), 217 Option types have toNullable(), Results have unwrap() — the old 'no auto-conversion' audit note no longer applies to Kotlin.


**Completeness:** Full, not smoke-only. Callbacks work end-to-end via the host-invoker pattern: 20 invoker kinds wired at init with pinned JNA callbacks, 18 typed Data<T> SAM variants, layout callback returns Dom directly via byte-splice bridge; counter E2E was verified per README ("counter probe 5→8 via AZ_DEBUG") and scripts/e2e_language_matrix.md ("Java/Kotlin/Scala work once a JDK is on PATH"; Windows is a documented SKIP for a JVM-exit hang). Widgets are exposed (Button, CheckBox, TextInput, NumberInput, DropDown, ListView, TreeView, Ribbon, Accordion...). String/Vec/Option/Result auto-conversion is present (kotlin.String at boundaries, Iterable Vecs with _clone-safe iterators, toNullable ×217, unwrap). The full generated Azul.kt compiles clean with kotlinc 2.3.21 (exit 0, warnings only), and examples/kotlin/HelloWorld.kt compiles clean against it — I could not run the GUI (libazul not built, per constraints).


**Blockers to ship:**
- Guide hello-world does not compile: doc/guide/en/hello-world/kotlin.md:85 must use AzulNativeManaged.ButtonOnClickCallbackInvokerCallback (as examples/kotlin/HelloWorld.kt:10 does), not CallbackInvokerCallback — the published counter example fails kotlinc with a type mismatch.
- Frontpage api.json install steps are not honestly completable: they compile a HelloWorld.kt that no step downloads and that the release deploy does not ship (doc/src/dllgen/deploy.rs:830-832), and reference $JNA_JAR with no acquisition step. Either add HelloWorld.kt to the kotlin deploy list + a curl step + a JNA download step, or rewrite the steps to say 'create HelloWorld.kt from the guide' and how to get jna-5.14.0.jar.


**Quick wins (<1 day):**
- One-line guide fix: CallbackInvokerCallback → ButtonOnClickCallbackInvokerCallback in doc/guide/en/hello-world/kotlin.md:85 (and AzString → AzulString at line 129).
- Add `BindingFile { dst: "HelloWorld.kt", src: "kotlin/HelloWorld.kt", source: BindingSource::Examples }` to deploy.rs and a matching curl step + JNA curl step + -J-Xmx4g to api.json's kotlin installation entry.
- Emit enum-typed overloads in codegen (withButtonType(AzButtonType) etc.) so user code drops the .value noise — lang_kotlin already special-cases smart builders, this is the same mechanism.
- Rewrite HelloWorld.kt to use the typed registerCallback(MyDataModel::class.java) { data, info -> ... } path the binding already ships — kills Pointer?, refanyGet, and outPtr!! from the flagship example (~10 lines shorter, far more Kotlin).
- Refresh examples/kotlin/README.md Files section (39 lines not 67; explain where Azul.kt comes from) and mention `gradle run` as the recommended workflow in the guide.
- Drop the dead `if (ptr == null)` checks in wrappers.rs emission (ptr is non-nullable Pointer) to silence dozens of always-false warnings in every user build.


**Verdict:** Keep it shipped — the binding itself is one of the strongest JVM offerings (clean-compiling 6.3 MB artifact, pinned host-invoker callbacks, consume/close double-free protection, real auto-conversions, counter E2E on record) — but spend one focused day on documentation truth: the published guide example does not compile and the frontpage install steps reference files ($JNA_JAR, HelloWorld.kt) that no step provides.


## lua — shipped-issues (install 3/5, ~1d to ship-quality)

On macOS arm64 the published steps genuinely work: curl libazul.dylib + azul.lua, paste the 47-line hello-world from the guide, run `DYLD_LIBRARY_PATH=. luajit hello-world.lua`; the module loads, refany round-trips, Dom/Button/string paths all work (verified today against the May-30 dylib), and the counter e2e passed on the 2026-05 board. On x86-64 Linux/Windows — the majority desktop arch — the same frontpage steps end in LuaJIT's `NYI: cannot call this C function (yet)` at App.create (aggregate-by-value NYI), a failure the guide discloses only in its Common-errors section while api.json's linux/windows install steps present the flow as working. There is no LuaRocks path: the shipped rockspec is misnamed (azul-1-1.rockspec vs version 0.1.0-1 inside) and points at a nonexistent v0.1.0 tarball, so `luarocks install` rejects it.


**Guide/install truthfulness issues:**
- api.json ['0.2.0'].installation.languages.lua linux/windows steps ('LD_LIBRARY_PATH=. luajit hello-world.lua', 'luajit hello-world.lua') omit that App.create hits LuaJIT's aggregate-by-value NYI on x86-64 — the guide itself admits it: doc/guide/en/hello-world/lua.md:145-151 'On x86-64 (SysV) App.create(.., AppConfig) hits this ... the E2E board marks Lua ⊘ SKIP on x86-64'. Frontpage steps are not honest for the dominant linux/windows arch.
- api.json step descriptions say 'generate the azul.lua binding' but the steps download it; and no step fetches/creates hello-world.lua before running it.
- examples/lua/README.md:59 claims 'hello-world.lua — 93-line reference implementation' — the file is 47 lines.
- examples/lua/README.md:57-63 lists azul.lua and libazul.dylib as files in the directory, but git ls-files shows only README/rockspec/hello-world.lua are tracked — a fresh clone has neither; the local copies are stale artifacts (examples azul.lua = 76,868 lines vs regenerated target/codegen/azul.lua = 83,516).
- examples/lua/README.md:49-52 flags the __eq cdata==nil SIGSEGV as an open follow-up; it is fixed in the current codegen — every generated __eq guards type(a)/type(b) ~= 'cdata' (e.g. target/codegen/azul.lua:58047).
- target/codegen/azul-1-1.rockspec is unusable as published: filename implies version '1-1' but the spec says version = '0.1.0-1' (luarocks rejects filename/content mismatch), version is stale vs api 0.2.0, and source.url points to a maps4print/azul v0.1.0 tarball that does not exist. The guide honestly says 'no LuaRocks package yet' (lua.md:39), but the artifact ships anyway.
- examples/lua/hello-world.lua:1 comment says 'LD_LIBRARY_PATH=. luajit' but the example dir targets macOS where it must be DYLD_LIBRARY_PATH (README gets it right).


**Safety issues:**
- CONFIRMED (empirical): the binding's entire GC model is inert for real objects — LuaJIT arms metatype __gc only for ffi.new-created instances, NOT for structs returned from C calls, which is how ~every azul object is created. Verified: a dropped refany_create() RefAny never fires the host releaser after 5 forced GC cycles, while an ffi.new struct's __gc fires immediately. Consequence: every unconsumed native object leaks (clones, RefAny handles, the AzString returned inside every generated __tostring at target/codegen/azul.lua:58047-style lines, screenshot/encode results). Codegen source assumes finalizers run: doc/src/codegen/v2/lang_lua/wrappers.rs:387,506 and managed.rs:251-260.
- CONFIRMED (mechanism demonstrated): _az_string temporaries are double-owned — after `azul.Button.create('label')` the Rust-side Button shares the temp's heap buffer (b.label.vec.ptr == temp.vec.ptr verified), and the temp's finalizer bytes still say DefaultRust; running the finalizer's exact sequence (AzString_delete on the temp) then churning the Rust allocator makes the live Button's label read 'CLOBBER-...' — use-after-free plus a second free at Dom teardown. All 411 _az_string call sites are affected the moment finalizers ever arm; consume-lines in doc/src/codegen/v2/lang_lua/wrappers.rs:668-694 disarm only the user-visible variable (a Lua string, a no-op), never the AzString temp; the static path wrappers.rs:950-973 has no consume at all.
- CONFIRMED (mechanism demonstrated): static factories skip consume entirely — azul.App.create is a raw passthrough (target/codegen/azul.lua:60005; emitted by wrappers.rs:942-948) leaving `data` (RefAny, refcount 1 — one delete provably releases the host handle) and the AppConfig temp (contains bundled_fonts/routes vecs) double-owned; azul.WindowCreateOptions.create (azul.lua:72385-72390) leaves the spliced _cb temp double-owned — a single AzLayoutCallback_delete on those bytes provably kills the app's registered layout function. Latent today only because finding #1 keeps finalizers unarmed; the two bugs mask each other.
- Thread-affinity footgun: azul.Thread.create accepts a Lua function and the module registers a ThreadCallbackInvoker (target/codegen/azul.lua:60028-60032, 55720-55731) that would run the Lua function on a worker thread; scripts/BINDING_STRATEGY_PER_LANGUAGE.md:264 states Lua ThreadCallback host-code is NOT supported (no lock exists). Normal user code azul.Thread.create(data, wb, fn) corrupts the LuaJIT VM.
- Minor: Result :unwrap() deletes self in place without tombstoning the wrapper (e.g. target/codegen/azul.lua:75298-75305) — calling any further method on the same Result operates on dropped internals. Positive note: the callback-error path is safe by design — libazul's thunk pre-fills the out-param with the kind's default (core/src/host_invoker.rs:450-461), so a Lua error inside a callback degrades to DoNothing instead of reading uninitialized memory.


**Idiomatic-ness issues:**
- Generated `toString` method is camelCase and redundant — idiomatic __tostring metamethods already exist on every type; everything else is snake_case.
- The rockspec pretends at LuaRocks integration it can't deliver (misnamed file, dead source.url, 'lua >= 5.1, < 5.5' dependency while the module hard-errors on anything but LuaJIT); either make it installable or stop shipping it.
- Single flat 83,516-line azul.lua with a ~55k-line ffi.cdef is un-Lua-ish for distribution (BINDING_STRATEGY_PER_LANGUAGE.md:67 planned an azul.app/azul.dom nested-table split that was never finished), though require('azul') returning a module table is correct.
- No LDoc/EmmyLua annotations anywhere, so editors offer zero completion on an API this large.


**Ergonomics issues:**
- CSS styling is deeply nested and verbose: azul.CssPropertyWithConditions.simple(azul.CssProperty.font_size(azul.StyleFontSize.px(32.0))) — three constructors for one font-size, vs the fluent one-liner feel of the rust/python reference.
- data:clone() is mandatory in :set_on_click(data:clone(), fn) — forgetting :clone() silently consumes the layout's RefAny; nothing warns.
- No Lua-table ↔ AzVec auto-conversion, and string RETURNS need manual :to_lua_string() (auto-conversion is one-way, matching the known cross-binding gap in the auto-conversion audit).
- Frontpage install steps run hello-world.lua without ever telling the user to create it; the guide's code block is the only source.
- Otherwise strong: 47-line hello-world (close to the 35-40-line reference), :with(opts) nested-table builder, chainable add_*/set_*, enum constant tables, pcall-wrapped callbacks with stderr diagnostics.


**Completeness:** Callbacks genuinely work, not smoke-only: 20 callback kinds are wired through the host-invoker (Layout, generic Callback, plus widget events for Button/CheckBox/DropDown/NumberInput/ListView/TextInput/Tab/Ribbon/TreeView/ColorInput/FileInput/VirtualView/Thread), widgets are exposed with fluent wrappers, and the counter e2e (5→8 probe) passed on the 2026-05 board for macOS arm64; today I verified module load, refany round-trip, Dom/Button/string construction and the wco smart factory against the May-30 dylib. Auto-conversion: Lua string → AzString on all owned-string args (411 sites), Option returns auto-:to_opt() to value-or-nil, Result returns auto-:unwrap() raising Lua errors; NO Vec↔table conversion and no automatic AzString→Lua-string on returns. Hard platform hole: x86-64 (Linux/Windows) is unusable due to LuaJIT's aggregate-by-value NYI at App.create — effectively an arm64-only binding today.


**Blockers to ship:**
- Install-docs honesty: api.json lua linux/windows steps must disclose the x86-64 LuaJIT NYI failure at App.create (guide lua.md:145-151 already documents it; the frontpage steps contradict it). Doc-only fix.
- Re-verify the counter e2e against the regenerated 83,516-line azul.lua once libazul builds — the last verified pair is 2026-05-30 vintage and the current artifact has never been driven through a click (today's checks covered load + non-GUI paths only).


**Quick wins (<1 day):**
- Add the x86-64 caveat (or 'arm64 only' platform note) to api.json lua linux/windows install steps — 30 minutes.
- Fix or drop the rockspec: rename to azul-0.2.0-1.rockspec, sync version, point source.url at a real artifact — or stop emitting it until LuaRocks publishing is real (doc/src/codegen/v2/lang_lua/rockspec.rs).
- Refresh examples/lua/README.md: 47-line count, remove the fixed __eq follow-up, mark azul.lua/libazul.dylib as downloaded artifacts not repo files; fix hello-world.lua:1 LD_→DYLD_LIBRARY_PATH comment.
- Guard azul.Thread.create with error('ThreadCallback from Lua is unsupported; use the writeback pattern') per BINDING_STRATEGY_PER_LANGUAGE.md:264.
- Plug the __tostring leak: consume the toDbgString AzString after ffi.string() in the generated __tostring (one-line codegen change in wrappers.rs).
- Copy the current target/codegen/azul.lua into examples/lua or add a fetch script so the repo example matches the shipped binding.


**Verdict:** Keep Lua on the frontpage — the macOS arm64 path is real, idiomatic, and e2e-proven — but ship the honesty fixes now: the linux/windows install steps advertise a flow that dies at App.create on x86-64, and the rockspec/README are stale-to-broken (~1 focused day). Separately schedule a 2-3 day memory-model rework: the binding's finalizers never actually arm on C-returned objects (everything leaks), and the moment they do arm, the demonstrated un-consumed string/factory temps become live use-after-frees — the two bugs currently mask each other.


## c — shipped-issues (install 2/5, ~1.5d to ship-quality)

A Linux user gets a genuinely good story: apt/dnf installs /usr/include/azul.h + libazul.so, `cc hello-world.c -lazul` links, and the $ORIGIN rpath line in api.json is correct. A macOS user following the frontpage steps hits two walls: step 3 compiles a file (hello-world.c) that no step downloads (it lives in examples.zip, mentioned only in the description text), and after fetching it, `./hello-world` aborts with dyld "Library not loaded: /Users/runner/work/.../libazul.dylib" — the shipped dylib's LC_ID_DYLIB is the absolute CI build path (verified locally: otool -D on target/release/libazul.dylib shows /Users/fschutt/.../deps/libazul.dylib; rust.yml strips and codesigns but never runs install_name_tool -id @rpath/...), so the documented -Wl,-rpath,@executable_path flag is dead weight. The binding itself is solid: the repo hello-world.c compiles clean against the fresh header and CI runs a real click-the-button counter e2e for C on all three OSes.


**Guide/install truthfulness issues:**
- Guide code snippet does not compile: doc/guide/en/hello-world/c.md:155 declares `label_dom` but line 176 does `AzDom_addChild(&body, label_wrapper)` — undeclared identifier, confirmed with clang -fsyntax-only (error: use of undeclared identifier 'label_wrapper'). The official hello-world page's only C listing fails at compile.
- macOS install steps (api.json c.macos + guide c.md:287-293) produce a binary that cannot start: the published libazul.dylib keeps rustc's absolute-path LC_ID_DYLIB (verified: otool -D target/release/libazul.dylib -> /Users/fschutt/Development/azul-mobile/target/release/deps/libazul.dylib; .github/workflows/rust.yml has strip+codesign steps around lines 955-1020 but zero install_name_tool calls on the artifact). '-Wl,-rpath,@executable_path' only helps if the id were @rpath/libazul.dylib. CI's own e2e never catches this because it links and runs on the same machine.
- api.json C steps for all 3 platforms compile 'hello-world.c' but no step obtains it — the description says 'the hello-world.c source is in examples.zip' yet the command list has no examples.zip download, so pasting the 4 commands verbatim fails at the compile step with 'no such file'.
- Guide c.md:121 documents the macro as 'AZ_REFLECT_JSON(structName, destructor, fromJson, toJson)' — actual order in target/codegen/azul.h:78726 is (structName, destructor, toJsonFn, fromJsonFn). Guide line 245 has it right; line 121 contradicts it, and because the macro erases fn-pointer types via (uintptr_t) casts (azul.h:78727), swapped args compile silently.
- Guide c.md:199/246 claims 'ALWAYS pair _create with _delete before returning / every downcast must be paired with a delete' — this is wrong per the macro's own semantics: FooRef_delete after a FAILED downcast calls AzRefCount_decreaseRef without a matching increase (increment only happens on downcast success, azul.h:78756-78775), underflowing num_refs (unchecked fetch_sub, core/src/refany.rs:369-373) so every later downcastMut fails forever. The guide's own code correctly returns early WITHOUT delete — but that path leaks instead (see safety).
- Guide c.md:147 passes a char[20] to AzString_copyFromBytes(const uint8_t*, ...) without a cast → -Wpointer-sign warning; the repo examples all cast. Cosmetic but the flagship snippet should be warning-clean.
- examples/c/azul.h is stale vs target/codegen/azul.h: it still declares AzDropDown_new and AzTitlebar_create which no longer exist in the current artifact (2-line diff). Harmless today because the repo example only uses live symbols and CI copies the fresh header, but a user copying examples/c/azul.h gets link errors on those two.
- Unverifiable (not checked remotely): guide c.md:51-56 .deb/.rpm exact filenames on the GitHub 0.2.0 release, and that azul.rs/ui/release/0.2.0 currently serves the listed files. CI (rust.yml build_linux_packages ~line 1861, release staging ~2654-2689 incl. libazul.x86_64.dylib) does build/stage all of them, so the claims are plausible.


**Safety issues:**
- Failed-downcast protocol is a lose-lose trap for normal code: FooRef_create clones the refcount (azul.h:78745-78748, bumps RefCountInner.num_copies per core/src/refany.rs:165-188). If downcast fails, (a) calling FooRef_delete (as guide c.md:246 instructs) does decrease_ref without prior increase → usize underflow (unchecked fetch_sub at core/src/refany.rs:369-373) → can_be_shared_mut (refany.rs:341-345) returns false forever → app callbacks silently stop mutating state; (b) NOT calling delete (what examples/c/hello-world.c:44-45,54-55 actually do) leaks the RefCount clone → num_copies never reaches 0 → user data destructor never runs. Fix is one macro line: have FooRef_delete skip decreaseRef when value->ptr == 0 (create initializes ptr to 0; downcast sets it on success).
- No double-delete guard: calling MyDataModelRefMut_delete twice (easy in an early-return callback) double-decrements num_mutable_refs (core/src/refany.rs:398-402, unchecked) — counter wraps, borrow checking is corrupted, and a second thread/timer callback can then obtain an aliasing mutable borrow: use-after-free class hazard from a plain user mistake.
- AZ_REFLECT_JSON type-erases the toJson/fromJson fn pointers through (uintptr_t) (azul.h:78727, generated by doc/src/codegen/v2/lang_c.rs:1384+): passing them in the wrong order (which guide c.md:121 actively documents) or with a wrong signature compiles without any diagnostic and is a wrong-prototype indirect call (UB) at hot-reload/serialize time.
- Owned-string field assignment leaks: the blessed pattern `window.window_state.title = AZ_STR("Hello World")` (examples/c/hello-world.c:88, guide c.md:221) overwrites the AzString that AzWindowCreateOptions_create put there without freeing it. One-shot leak here, but the same direct-struct-write idiom applied in a RefreshDom loop leaks per frame; there is no AzWindowState_setTitle that frees the old value.
- Ownership convention (by-value arg = consumed, pointer = borrowed) is real but documented nowhere in the 78.8k-line header — generate_function_declaration (doc/src/codegen/v2/lang_c.rs:819-948) emits bare externs with no /* consumes */ annotations and generate_preamble (lang_c.rs:321-385) is purely technical, so a user who passes the same AzDom child to two AzDom_addChild calls (double-consume/double-free) gets no warning from docs or compiler.


**Idiomatic-ness issues:**
- Ownership rules exist only by convention (by-value=consume) with zero annotation in azul.h; idiomatic C libraries (e.g. cairo, GLib) state ownership per function in doc comments.
- Only ~5 hand-picked *Vec_empty initializer macros are shipped (azul.h:78626-78664) out of hundreds of Vec types; everything else requires calling into the DLL even for an empty vec.
- The AZ_STR helper for runtime strings is re-defined ad hoc in every example (examples/c/hello-world.c:7) instead of being shipped in azul.h next to AzString_fromConstStr — every user will write this macro themselves.
- Option/Result access in the examples uses raw tag poking (`field.None.tag == AzOptionJson_Tag_None`, hello-world.c:31,35) even though the header generates nice matchRef/matchMut helpers (azul.h:78598-78624) — the examples/guide never showcase the idiomatic accessor the codegen worked to provide.


**Ergonomics issues:**
- Repo hello-world.c is 98 lines vs the ~35-40-line Rust/Python reference — ~40 of those lines are AZ_REFLECT_JSON toJson/fromJson boilerplate that a hello-world does not need (plain AZ_REFLECT would do, as the guide itself says at c.md:245); the flagship example should be the ~60-line AZ_REFLECT version.
- Guide and repo example teach different dialects: guide uses AZ_REFLECT + AzDom_setCss CSS strings; the shipped example uses AZ_REFLECT_JSON + typed AzCssProperty_fontSize(AzStyleFontSize_px(32.0)) (hello-world.c:65-67). A newcomer diffing the two gets no guidance on which is canonical.
- Each callback costs a 6-line create/downcast/mutate/delete dance plus a mandatory failure branch whose correct handling is (per the safety findings) currently impossible to write both leak-free and underflow-free.
- The 78,802-line single header is slow to index/compile and IDE-hostile; no minimal 'azul-core.h' subset exists for the 30 symbols hello-world actually needs (acceptable for generated C, but worth noting).


**Completeness:** Full, not smoke-only: CI runs a real hello_world_counter.json click e2e for C on ubuntu/macos/windows (rust.yml e2e_native, family c-cpp, lines 1424-1446; scripts/e2e_language_matrix.md:46 'verified-green'). Widgets are exposed (Button/CheckBox/TextInput/DropDown/Slider — 81 widget functions; examples/c/widgets.c and calc.c syntax-check clean against the fresh header). Raw fn-pointer callbacks plus WithCtx siblings are emitted (lang_c.rs:828-948). String/Vec/Option are manual by design in C (AzString_copyFromBytes, tagged unions with generated match helpers, 963 _delete destructors) — appropriate for the language; no auto-conversion expected or possible.


**Blockers to ship:**
- macOS install docs are not honest today: the published dylib's LC_ID_DYLIB is an absolute CI path and rust.yml never rewrites it (no install_name_tool step), so the frontpage recipe's ./hello-world fails at dyld load. Fix: add `install_name_tool -id @rpath/libazul.dylib target/prod-release/libazul.dylib` BEFORE the codesign step (~rust.yml:996), or document the user-side `install_name_tool -change` workaround.
- The official guide's only C code listing does not compile (undeclared `label_wrapper`, doc/guide/en/hello-world/c.md:176) — the hello-world page must contain a compilable counter example.


**Quick wins (<1 day):**
- One-line CI fix: install_name_tool -id @rpath/libazul.dylib before codesign in rust.yml (also for libazuldbg.dylib and libazul.x86_64.dylib at staging, rust.yml:2654/2689).
- Fix guide snippet: rename label_wrapper→label_dom at c.md:176 and add (const uint8_t*) cast at c.md:147; CI-gate the guide snippet with the same clang -fsyntax-only check used here.
- Fix the AZ_REFLECT_JSON arg-order comment at c.md:121 to (destructor, toJson, fromJson).
- One-line macro fix in lang_c.rs generate_az_reflect_macro: make FooRef_delete/FooRefMut_delete no-op the decreaseRef when value->ptr == 0, then update guide c.md:199/246 so 'always pair create with delete' becomes actually safe.
- Add an examples.zip (or single hello-world.c) download step to api.json C steps on all three platforms so the 4-command block is self-sufficient.
- Ship AZ_STR(s) runtime-string macro in azul.h next to AzString_fromConstStr.
- Regenerate examples/c/azul.h from target/codegen (removes stale AzDropDown_new/AzTitlebar_create).
- Simplify examples/c/hello-world.c to the AZ_REFLECT (non-JSON) form (~60 lines) and move the JSON-reflect version to a separate example.


**Verdict:** Keep C on the frontpage — the binding is real (3-OS click e2e in CI, widgets, clean-compiling examples) — but ship the two half-day fixes first: the macOS dylib install-name (frontpage recipe currently dies at dyld) and the guide's non-compiling snippet; the AZ_REFLECT failed-downcast trap deserves the one-line macro fix in the same pass.


## java — shipped-issues (install 2/5, ~1.5d to ship-quality)

A fresh developer following the api.json frontpage steps downloads libazul.dylib and azul-java.zip, unzips, and then step 4 fails immediately: it runs `java -cp target/hello-world-1.0.0.jar ... com.azul.HelloWorld`, but no step ever builds that jar (no `mvn package` is listed), the zip contains no HelloWorld.java, and the zip's bundled pom builds artifact `azul-1.0.0.jar` from `src/main/java` while the sources unpack flat, so even `mvn package` inside the unzipped dir would produce an empty jar. Falling back to the hello-world guide, the user must hand-author a Maven project (the guide never supplies a pom) and then finds the guide's counter snippet does not compile — it declares the click handler as `AzulNativeManaged.CallbackInvokerCallback` but `Button.onClick` requires `ButtonOnClickCallbackInvokerCallback`. Only by discovering examples/java/ in the repo (working pom.xml + current HelloWorld.java) does a first window appear; from there, the counter e2e genuinely works per the README status and scripts/e2e_language_matrix.md.


**Guide/install truthfulness issues:**
- api.json ['0.2.0']['installation']['languages']['java'] (all 3 platforms): final step runs `java ... -cp target/hello-world-1.0.0.jar ... com.azul.HelloWorld` but no listed step builds it — the descriptions say 'build with Maven' yet there is no `mvn package` command, azul-java.zip contains no HelloWorld.java (verified: /Users/fschutt/Development/azul-mobile/target/codegen/java has no HelloWorld.java), and the zip's pom (artifactId `azul`) would produce target/azul-1.0.0.jar, not hello-world-1.0.0.jar. The steps as published cannot open a window.
- azul-java.zip's bundled pom cannot compile the zip's own contents: doc/src/dllgen/deploy.rs:1080-1084 stores the ~6,800 .java files flat at the zip root ('so the zip unpacks to a flat project the bundled pom.xml expects'), but the generated pom (doc/src/codegen/v2/lang_java/pom.rs) has no <sourceDirectory> override, so Maven looks in src/main/java and compiles zero sources → empty jar.
- doc/guide/en/hello-world/java.md:74 declares the click handler as `AzulNativeManaged.CallbackInvokerCallback` but `Button.onClick` (target/codegen/java/Button.java:29) requires `AzulNativeManaged.ButtonOnClickCallbackInvokerCallback` — the guide's flagship snippet is a javac type error. examples/java/HelloWorld.java:16 already uses the correct type; the guide (generated_at 2026-05-29) is stale.
- doc/guide/en/hello-world/java.md:129-138 'Build and run' invokes `mvn package` producing target/hello-world-1.0.0.jar, but the guide never provides or links any pom.xml for the user's project — the command is unrunnable as documented (the needed pom exists only in the repo at examples/java/pom.xml).
- doc/guide/en/hello-world/java.md:124 says '`AzString` decodes to java.lang.String via .toString()' — the user-facing wrapper was renamed to `AzulString` (examples/java/README.md:53-55 documents the rename); `AzString` is the raw JNA struct.
- examples/java/README.md:36-37 claims '`AzVec<T>.toList()` accessors mirror Java collection idioms' — toList() exists only on the raw AzStringVec JNA struct (target/codegen/java/AzStringVec.java:31) returning List<AzString> raw structs, not on the high-level Vec wrapper classes and not decoded to java.lang.String; toNullable()/unwrap() do exist but likewise only on raw Az* structs.
- examples/java/README.md:69 says 'HelloWorld.java — 86-line Python-quality port'; the file is 51 lines (trivially stale, in the flattering direction).
- examples/java/HelloWorld.java:1 header run command omits -XstartOnFirstThread, which the same repo's README.md:25 and the guide both state is REQUIRED on macOS — copy-pasting the file's own comment crashes on macOS.


**Safety issues:**
- Empty-string crash in every String-taking API: doc/src/codegen/v2/lang_java/wrappers.rs:1071 emits `new com.sun.jna.Memory(bytes.length)` with no zero-length guard; JNA's Memory constructor throws IllegalArgumentException for size 0, so `Button.create("")`, `Dom.createText("")`, `dom.withCss("")` etc. (e.g. target/codegen/java/Button.java:40, Dom.java:307,324) throw on perfectly normal code like an empty initial text-input value.
- Use-after-move read in equals()/hashCode()/rawPointer() after builder consumption: __consume() sets closed=true but equals() only null-checks ptr (target/codegen/java/Button.java:139-143), so after a builder chain like `withButtonType(...)` consumed `this`, calling equals() on the old reference invokes AzButton_partialEq on native memory whose ownership was transferred by-value to C — an undefined read. Normal code hits this by keeping a reference across a fluent chain.
- close() vs finalizer race and finalizer-thread deletes: close() is not synchronized (target/codegen/java/App.java:89-96), so a user-thread close() racing the GC finalizer can double-call Az*_delete; additionally all finalizer-path deletes run on the JVM finalizer thread, not the UI thread — a thread-affinity hazard if any Az type's drop is main-thread-only on macOS.
- Mitigating positive: the native thunk pre-fills the callback out-value with the kind's default (core/src/host_invoker.rs:452-455 'Pre-fill out with the kind's default so a host that fails to write... leaves us with a sane value'), so a Java callback that throws, returns null, or fails the instanceof check cannot cause an uninitialized-Dom/Update read — the failure mode is a silent no-op, not memory corruption.


**Idiomatic-ness issues:**
- Flat namespace: 6,799 classes in a single `com.azul` package/directory — raw Az* JNA structs, *Helpers, and high-level wrappers all intermixed; IDE completion is flooded and there is no visual distinction between the safe wrapper tier (App, Dom, Button) and the unsafe struct tier (AzApp, AzDom).
- Non-Java method names from codegen: `clone_()` and `default_()` with trailing underscores (App.java:57,66) instead of idiomatic copy()/newDefault().
- Stringly/int-typed enums at the wrapper boundary: `withButtonType(int)` + `AzButtonType.Primary.value` (Button.java:96, HelloWorld.java:38) instead of accepting the enum type directly.
- Error handling: `unwrap()` throws bare RuntimeException with a toString'd payload (AzResultEmptyStructFileError.java:27-37); no typed exception hierarchy, no checked alternative like an Optional-returning ok().
- Convenience accessors (toNullable/unwrap/isOk/toList) live on the raw Az* JNA structs rather than the high-level wrappers, so idiomatic use forces users down into the FFI tier; AzStringVec.toList() yields List<AzString>, not List<java.lang.String>.
- Positive: AutoCloseable + try-with-resources + idempotent close() + finalizer fallback is exactly the right Java resource idiom, and consume-on-transfer with IllegalStateException('closed') on reuse is a clean ownership model.


**Ergonomics issues:**
- Raw-SAM callback API leaks FFI plumbing into user code: the hello world needs `Pointer dataPtr`, `refanyGet`, `instanceof`, and `outPtr.setInt(0, result)` (HelloWorld.java:17-25). Typed `ButtonOnClickCallbackWithData<T>` registrations exist in AzulHostInvoker but `Button.onClick` (Button.java:29) only accepts the raw SAM — README.md:57-58 itself admits 'smart-factory integration still TODO'.
- Silent callback drop on wrong SAM type: the dispatcher's instanceof check (AzulHostInvoker.java:59-62,123-126) silently skips mismatched handlers — a user who registers the generic CallbackInvokerCallback for a button gets a button that does nothing, with zero log output or exception.
- Run command is needlessly complex: docs prescribe `-cp target/hello-world-1.0.0.jar:$HOME/.m2/repository/.../jna-5.14.0.jar` although the example pom's shade plugin (examples/java/pom.xml:141-153) already bundles JNA into a fat jar with a Main-Class manifest — `java -XstartOnFirstThread -Djna.library.path=. -jar target/hello-world-1.0.0.jar` would suffice.
- Hello world is 51 lines vs the ~35-40-line Python/Rust reference — respectable for Java, but the instanceof-guard + out-pointer dance is pure boilerplate the reference languages don't have.
- macOS launch footgun: forgetting -XstartOnFirstThread crashes/hangs; nothing in the binding detects the wrong thread and produces a friendly error.


**Completeness:** Callbacks genuinely work end-to-end: README claims counter probe 5→8 via AZ_DEBUG verified, and scripts/e2e_language_matrix.md:101 lists java as working once a JDK is on PATH (host-dependent, not smoke-only). 19 widget/event callback invoker kinds are wired (AzulNativeManaged.java), typed Data<T> register overloads for 17 of them, plus typed LayoutCallback/VirtualViewCallback bridges that splice Dom bytes into the out-pointer. Widgets are exposed (Button, CheckBox, DropDown, NumberInput, ListView, TreeView, Tab, Ribbon, FileInput, ColorInput...). Auto-conversion: owned String args accept java.lang.String everywhere (UTF-8 marshal emitted per-call); Vec/Option/Result have accessors only at the raw-struct tier (U8Vec.toByteArray/U32Vec.toIntArray primitives, AzStringVec.toList→List<AzString>, toNullable/unwrap on Az* structs) — no automatic List/Optional conversion at the wrapper tier.


**Blockers to ship:**
- api.json java install steps cannot produce a running app: no `mvn package` step, no HelloWorld.java or usable project pom in the downloaded artifacts, and the referenced target/hello-world-1.0.0.jar is never built (the zip's own pom would name it azul-1.0.0.jar). Fails the 'honest install docs' bar on all three platforms.
- azul-java.zip's bundled pom compiles zero sources as unpacked (flat zip layout vs pom's default src/main/java, deploy.rs:1080 vs lang_java/pom.rs) — there is currently no self-contained downloadable path to a first window.
- The official guide's counter snippet does not compile (doc/guide/en/hello-world/java.md:74 uses CallbackInvokerCallback where Button.onClick requires ButtonOnClickCallbackInvokerCallback) — the one code block a new user copies is a javac error.


**Quick wins (<1 day):**
- Regenerate the guide snippet from the current examples/java/HelloWorld.java (fixes the ButtonOnClickCallbackInvokerCallback type error and the AzString→AzulString naming) — minutes once the guide regen runs.
- Fix api.json java steps: add HelloWorld.java + a ready-to-build project pom into azul-java.zip (or a separate azul-java-quickstart.zip), insert the `mvn package` step, and align the jar name.
- Add `<sourceDirectory>.</sourceDirectory>` (or repack files under src/main/java) in doc/src/codegen/v2/lang_java/pom.rs so the shipped zip builds as unpacked.
- Guard empty strings in wrappers.rs:1071 — emit `bytes.length == 0 ? AzString_default() : ...` or allocate Memory(Math.max(1, len)) — removes a whole class of first-day crashes.
- Simplify the documented run command to `java -XstartOnFirstThread -Djna.library.path=. -jar target/hello-world-1.0.0.jar` (shade already bundles JNA + Main-Class), and add -XstartOnFirstThread to HelloWorld.java:1's header comment.
- Add a typed overload `Button.onClick(Class<T>, ButtonOnClickCallbackWithData<T>)` delegating to the existing registerButtonOnClickCallback(Class, typed) — the plumbing already exists, only the smart-factory hook is missing (README's own TODO).
- Make the dispatcher log a warning (System.err) when a handle resolves but the instanceof check fails, so wrong-SAM registration stops being a silent no-op.
- Update README staleness: 51-line hello world, and clarify that toNullable/unwrap/toList live on the raw Az* structs.


**Verdict:** Keep Java on the frontpage — the binding itself is one of the strongest managed-FFI ports (AutoCloseable ownership model, working counter e2e, 19 callback kinds, typed Data<T> bridges) — but the published install path is currently dishonest: the api.json steps and the guide snippet both fail as written, and fixing those docs/packaging gaps (~1.5 days) should happen before the next release goes out.


## node — shipped-issues (install 2/5, ~1.5d to ship-quality)

A fresh developer follows the azul.rs steps: three curl downloads succeed (all URLs return 200), `npm install koffi` works, they paste the guide's 60-line hello-world. Then `DYLD_LIBRARY_PATH=. node hello-world.js` dies immediately with `Failed to load shared library: dlopen(azul, ...)` because azul.js calls koffi.load('azul') with a bare name and koffi does no lib-prefix/suffix mangling (empirically verified) — dlopen never even looks for a file named libazul.dylib, so the documented library-path env vars cannot help on macOS or Linux. If they read the 9 MB azul.js source and discover the undocumented AZ_LIB escape hatch, the app then aborts the first layout: `Button.on_click(model, fn)` double-registers the callback and throws `TypeError: azul.registerCallback: expected function, got object`, so the guide's counter example cannot work at all today.


**Guide/install truthfulness issues:**
- azul.js:60-61 (and hosted https://azul.rs/ui/release/0.2.0/azul.js, same bytes): comment claims koffi 'resolves the platform DLL name automatically (azul -> azul.dll / libazul.so / libazul.dylib)' — FALSE, verified empirically: koffi.load passes the bare name straight to dlopen, which searches for a literal file 'azul' and fails even with DYLD_LIBRARY_PATH=./LD_LIBRARY_PATH=. set
- doc/guide/en/hello-world/node.md:141-142: 'The native library must be in the working directory or on DYLD_LIBRARY_PATH / LD_LIBRARY_PATH / PATH' — false; no searched filename ever matches libazul.*; the only working path is the AZ_LIB env var (azul.js:56), which is documented nowhere user-facing
- examples/node/README.md:9 '✅ Full GUI E2E — counter probe 5→8 verified' and README.md:22-24 'Build + Run: node hello-world.js' — not true of current artifacts: the loader fails, and Button.on_click now throws (regression since the May verification; see safety findings)
- api.json ['0.2.0'].installation.languages.node (all 3 platforms): final step runs `node hello-world.js` but no step downloads or creates hello-world.js; macOS/Linux run steps (`DYLD_LIBRARY_PATH=.` / `LD_LIBRARY_PATH=.`) do not work with the shipped loader
- doc/guide/en/hello-world/node.md:30-32 'the same azul.js covers all three runtimes' — misleading: Bun/Deno invoker branches never write callback return values (azul.js:48650, 49114 gate `runtime === 'node-koffi'` around koffi.encode), and the Bun path builds 'azul.dylib' without the lib prefix (azul.js:97), so Bun/Deno cannot run a real app
- examples/node/README.md:71 'azul.js — 6.8 MB generated binding' — actually 9.2 MB (stale, trivial)
- azul.js:12-13 header: library may be 'placed in the same directory as this file' — dlopen does not search the module's directory, and never with the bare 'azul' name
- scripts/e2e_language_matrix.md:102 attributes the macOS failure to SIP-stripped DYLD_* on hardened node — wrong root cause (verified: non-hardened homebrew node with DYLD honored still fails; the bare-name mangling gap is the bug) and its 'WORKS on Linux' claim is doubtful for the same reason


**Safety issues:**
- Broken smart callback setters throw at runtime: doc/src/codegen/v2/lang_node/wrappers.rs:372-379 emits on_click(data,fn) that registers fn then calls this.with_on_click(__data,__cb); wrappers.rs:1290-1308 makes with_on_click unconditionally re-register the already-registered struct; registerCallback throws TypeError on non-functions (target/codegen/node/azul.js:49211-49214, with_on_click at 121761-121763). Every widget smart setter (~all on_* helpers) is affected; thrown inside the layout callback it leaves the native out-struct uninitialized
- Callback invoker error path returns uninitialized native memory: all invokers catch JS exceptions but never write outPtr on failure (azul.js:49108-49128 layout invoker) — a throwing layout callback hands libazul an uninitialized AzDom by-value return → UB/crash, not a clean fallback
- Bun/Deno runtimes: koffi.encode of callback returns is gated on runtime==='node-koffi' (azul.js:48650, 49114), so on Bun/Deno the native side always reads an unwritten return struct — silent UB on runtimes the header advertises as supported
- Use-after-consume passes null to native: consuming methods null this._ptr (e.g. Dom.with_child, azul.js:128633-128638; App.run consumes the window, 61532-61535) but no method guards against a null _ptr on entry (only toString does) — reusing a moved wrapper, e.g. running the same WindowCreateOptions twice, sends NULL into a deref-ing C call
- Deliberate bounded leak: every generated toString skips freeing the returned AzString's owned U8Vec (azul.js:61565-61568 and repeated per class) — documented in-source, low severity


**Idiomatic-ness issues:**
- snake_case methods (create_body, with_child, on_click, default_) instead of JS camelCase — consistent with other Azul bindings but reads foreign to node developers
- No TypeScript definitions (index.d.ts) despite a 9 MB API surface — modern node users get zero autocomplete/type safety; package.json 'files' ships only azul.js + README
- Duplicate class members: every wrapper emits toString(instance) then a second zero-arg toString() (e.g. azul.js:61552 and 61557) — second silently wins; clone(instance)/toString(instance) carry phantom `instance` params that are passed as extra FFI args (harmless only because koffi ignores extras)
- Header claims 'no var' (azul.js:16) but optionToNullable/resultUnwrap/resultIsOk use var (azul.js:174303-174323)
- Option/Result handling is via module-level helpers (optionToNullable, resultUnwrap) instead of methods, an acknowledged koffi-union limitation — workable but un-JS-like; no toNullable()/unwrap() on values
- package.json version is 0.1.0 while the release channel is 0.2.0; no npm package at all (name squatted — honestly documented in the guide)
- CommonJS-only output is fine, but the flat 1000+-symbol module.exports object makes tree-shaking and discovery hard


**Ergonomics issues:**
- hello-world is 57 lines vs the ~35-40-line python/rust reference; extra weight: mandatory process.on('uncaughtException') safety net (the guide tells users to keep it), refanyGet null-guards in every callback, and a 10-line destructuring import block
- Callbacks receive a raw dataPtr that must be manually round-tripped through refanyGet(dataPtr) with a null check, instead of receiving the JS model object directly
- registerCallback('ButtonOnClickCallback', fn) kind strings are stringly-typed when users drop below the smart setters
- The AZ_LIB env var is the only reliable way to point at the library and is discoverable only by reading generated source
- Positives worth keeping: .with({...}) recursive opts builder with auto string→AzString conversion, Update/ButtonType frozen enum objects, createWithLayout smart factory


**Completeness:** Near-complete on paper, broken in practice: 20 callback kinds wired through the host-invoker (generic Callback, LayoutCallback, ThreadCallback + 17 widget callbacks), widgets exposed (Button, CheckBox, DropDown, NumberInput, TextInput, ListView, Tab, Ribbon, TreeView, FileInput, ColorInput, MsgBox/FileDialog), automatic JS-string→AzString on args and in .with(opts), frozen enum constants, per-type FinalizationRegistry + explicit delete() + _consume move semantics. But the counter e2e bar FAILS today: the loader cannot find libazul without undocumented AZ_LIB, and all smart on_* setters throw from double-registration. Option/Result are manual module helpers, Vec has no automatic array conversion, and Bun/Deno support is decorative (callback returns never written).


**Blockers to ship:**
- Loader: koffi.load('azul') bare name never resolves libazul.dylib/libazul.so — the published api.json install steps fail at the run step on macOS and Linux (verified empirically with koffi; hosted azul.js has the same code). Fix: mangle the platform filename in loadNodeKoffi (and Bun's missing lib prefix) like the Deno branch already does (doc/src/codegen/v2/lang_node/mod.rs:272-285), or document AZ_LIB in api.json + guide
- Callback regression: smart setters double-register and throw TypeError (wrappers.rs:372-379 + 1290-1308), breaking the exact Button.on_click(model, fn) line in the guide, examples/node/hello-world.js:33, and every widget callback — counter e2e cannot pass. Fix: make registerCallback pass through already-registered callback structs, or guard emission with a typeof check; then re-run the counter probe
- api.json node steps never obtain hello-world.js yet end with `node hello-world.js` — add a download step or a copy-the-snippet note


**Quick wins (<1 day):**
- One-line codegen fix in loadNodeKoffi: derive the platform filename (lib prefix + .dylib/.so/.dll suffix) exactly as the Deno branch does; same for Bun's missing 'lib' prefix (doc/src/codegen/v2/lang_node/mod.rs:285, :331)
- Add `if (typeof fn !== 'function') return fn;` passthrough at the top of registerCallback (managed.rs emission) — unbreaks all 76 smart on_* setters in one line
- Write outPtr with a safe default (Update.DoNothing / empty Dom) in invoker catch blocks so a throwing callback can't hand native code uninitialized memory
- Refresh examples/node/README.md: correct size (9.2 MB), correct run command, re-verify or remove the '✅ Full GUI E2E' badge
- Drop the duplicate toString(instance) emission and the phantom instance param on clone/toString in wrappers.rs
- Bump generated package.json version to 0.2.0 and add an entry-method null-_ptr guard ('use after move' Error) to wrapper methods


**Verdict:** Currently listed but broken end-to-end: the published install steps cannot load the library on macOS or Linux, and a codegen regression makes every smart callback setter (including the guide's own hello-world) throw — pull or fix before the next release, but both root causes are small, well-localized codegen fixes reachable in about 1.5 focused days.


## ruby — shipped-issues (install 3/5, ~1.5d to ship-quality)

A fresh macOS developer following the frontpage steps downloads libazul.dylib and azul.rb (both URLs verified live today, HTTP 200 at azul.rs/ui/release/0.2.0/), then hits a Gem::FilePermissionError at step 2 because `gem install ffi` cannot write to system Ruby's gem dir — the README knows the fix (`gem install --user-install ffi -v 1.15.5`) but the frontpage and guide don't. After pasting the 51-line hello-world from the guide, `AZ_LIB_DIR=. ruby -I. hello-world.rb` works without DYLD hacks because azul.rb resolves the dylib from AZ_LIB_DIR or its own directory (target/codegen/azul.rb:17-25). The counter e2e was a genuine pass per the 2026-05 matrix (`ruby ✓ WORKS genuine pass`, scripts/BINDING_STRATEGY_PER_LANGUAGE.md:99), but the artifact has been regenerated since (examples/ruby vendors a stale May-30 azul.rb whose md5 differs from target/codegen/azul.rb) and two double-free hazards sit directly on the canonical hello-world path (widget `.dom` under GC, `app.run(window.ptr)` at exit).


**Guide/install truthfulness issues:**
- api.json ruby steps (all 3 platforms) say the description "generate the azul.rb binding" but the step merely downloads it (`curl -O $HOSTNAME/ui/release/$VERSION/azul.rb`) — wording bug, nothing is generated.
- `gem install ffi` (api.json ruby step 2; doc/guide/en/hello-world/ruby.md:43) fails with Gem::FilePermissionError on macOS system Ruby — the exact setup the guide endorses at ruby.md:36 ("system Ruby on macOS works"). examples/ruby/README.md:13 itself uses `gem install --user-install ffi -v 1.15.5`; the frontpage/guide omit both --user-install and the 1.15.x pin (ffi >= 1.16 may not build on Ruby 2.6).
- Guide ruby.md:115 and examples/ruby/hello-world.rb:51 teach `app.run(window.ptr)` — passing the raw FFI struct bypasses App#run's `Azul._consume(root_window)` finalizer disarm; the correct, supported form is `app.run(window)`. The doc teaches the unsafe variant.
- Guide ruby.md:83-84 / README.md:53 claim CssProperty must be built via `Azul::Native.az_css_property_*` "for now" — stale as a limitation: `Dom#with_css(css_string)` exists in the artifact (target/codegen/azul.rb:91665, AzDom_withCss) and would replace the whole Native dance, matching the Python reference example.
- examples/ruby/README.md:61 claims "hello-world.rb — 69-line Python-quality port"; the file is 51 lines. README status ("Full GUI E2E verified", line 8) dates from 2026-05 against the vendored May-30 azul.rb, which no longer matches target/codegen/azul.rb (md5 differs) — unverified against current artifacts.
- target/codegen/azul.gemspec:6 says version 0.1.0 while the release is 0.2.0; azul.gemspec:13 requires ruby >= 2.7 contradicting the guide/README claim of "Ruby 2.6+"; azul.gemspec:15-22 lists lib/native/*/libazul.* files that do not exist in the artifact set (no gem is published — guide honestly says the rubygems name is taken).
- Guide ruby.md:144-145 troubleshooting points at DYLD_LIBRARY_PATH — SIP strips DYLD_* for /usr/bin/ruby, so that advice can never work with system Ruby; the mechanisms that actually work (AZ_LIB_DIR, lib next to azul.rb) go unmentioned there.
- api.json steps end with `ruby -I. hello-world.rb` but no step ever fetches or creates hello-world.rb — frontpage steps alone don't produce a runnable app (code must be pasted from the guide).


**Safety issues:**
- SYSTEMIC double-free: consuming instance methods that return a DIFFERENT type never disarm self's finalizer. doc/src/codegen/v2/lang_ruby/wrappers.rs:815-826 (plain `Some(_)` branch of emit_method_body_instance) ignores `consumes_self`, and the emitter never consults the self arg's ref-kind — api.json Button.dom is `self: "value"` (consuming, C sig azul.h:43865 `AzDom AzButton_dom(AzButton button)`), yet generated `Button#dom` (target/codegen/azul.rb ~87447; ProgressBar variant at azul.rb:75105) neither undefines the finalizer nor nils @ptr. The wrapper's finalizer later calls az_button_delete on the moved-out struct → double free of the label AzString and the on_click RefAny. Hits all 43 widget `.dom` methods — i.e. the canonical hello-world path (`Azul::Dom.new(button.dom)`) — whenever GC runs after a relayout.
- Exit-time double-free taught by the docs: `app.run(window.ptr)` (examples/ruby/hello-world.rb:51, guide ruby.md:115) passes the raw AzWindowCreateOptions by value; App#run's `Azul._consume(root_window)` no-ops on a raw FFI::Struct, so the `window` wrapper's finalizer stays armed and az_window_create_options_delete fires at process exit on internals libazul already owns/freed. `app.run(window)` would disarm it correctly (App#run, azul.rb; _consume at azul.rb:55080).
- WindowCreateOptions.create(layout) (azul.rb:95544) is a live trap: the codegen's own comment (wrappers.rs:197-200) admits the legacy `_create` path "discards the ctx (the host-handle id) — so callbacks fire but the user's Proc is never reached"; additionally it passes the AzLayoutCallback struct from _register_callback into an attach_function expecting `:az_layout_callback_type` (azul.rb:41014), which raises at runtime. Python's docs use exactly this name (`WindowCreateOptions.create(layout)`), so cross-language users will call it.
- Callback exceptions are swallowed: invoker rescue prints to stderr and never writes out_ptr (doc/src/codegen/v2/lang_ruby/managed.rs:333-340). Mitigated — libazul pre-fills the out value with the kind's default (core/src/host_invoker.rs:450-461), so no UB, but a typo in a layout lambda silently renders an empty default Dom instead of failing loudly. Same silent path if a callback returns nil/true (no branch matches, azul.rb:55131-55148).
- _apply_opts (azul.rb:55101-55114) overwrites AzString fields (e.g. the default window title) without deleting the previous value — per-assignment leak, not a crash.


**Idiomatic-ness issues:**
- Widget `.dom` and other non-self-typed returns come back as raw Native::AzDom FFI::Structs, forcing users to hand-wrap: `Azul::Dom.new(button.dom)` (hello-world.rb:36) — inconsistent with self-typed methods, which return wrapper instances.
- Result#unwrap raises bare RuntimeError with a formatted string (e.g. `raise "ResultU8VecEncodeImageError unwrap on Err: ..."`, azul.rb:20289+) instead of a dedicated Azul::Error exception class users could rescue selectively.
- `class String` inside module Azul shadows ::String in Azul-scoped code; generated code carefully uses `::String` (azul.rb:55103) but user code doing `include Azul` inherits the shadow.
- The docs teach `.ptr` leakage in user code (`app.run(window.ptr)`, `window.ptr`) — the escape hatch is fine to have, but the happy path should never require it.
- Callback error handling is stderr-only with no hook to install a custom error handler — un-Ruby-ish; a raise-into-caller or configurable handler would fit better.
- Otherwise genuinely idiomatic: snake_case methods, module-scoped enum aliases (Azul::Update::RefreshDom), Enumerable Vecs with `each`, `clone`/`dup` aliases, `==`/`hash`/`to_s` routed through C helpers, finalizer procs correctly capture only ptr (wrappers.rs:127-134).


**Ergonomics issues:**
- Hello-world is 51 lines vs Python's 34, and the gap is almost entirely the raw `Azul::Native.az_style_font_size_px` / `az_css_property_font_size` / `CssPropertyWithConditions.simple` dance (hello-world.rb:24-27) — unnecessary since `Dom#with_css('font-size:32px;')` exists (azul.rb:91665) and is what Python's reference example uses.
- Manual `Azul::Dom.new(button.dom)` re-wrap required because widget .dom returns a raw struct.
- Calling a method on a consumed wrapper (after a `with_*` builder moved it) passes nil @ptr into ffi and raises a cryptic TypeError instead of a clear 'object was consumed' error.
- No packaged distribution: users curl a single 4 MB azul.rb; the gemspec exists but is unpublishable as written (phantom native file list, stale version).
- Good ergonomics worth keeping: `.with(nested_hash)` builder with auto String→AzString, `RefAny.wrap` of any Ruby object, smart `on_click(model, lambda)` and `create_with_layout(lambda/block)`, Option→nil and Result→raise auto-unwrap at the wrapper boundary.


**Completeness:** Full-featured, not smoke-only: 20 callback kinds wired via the host-invoker pattern (layout, button/checkbox/dropdown/text-input/number-input/list-view/tab/ribbon/tree-view/thread/virtual-view...), with a shared host-handle releaser so Ruby objects are dropped when libazul releases them. 43 widgets expose `.dom`. Automatic conversions all present: Ruby String→AzString on Owned args, Azul::String#to_s, Option→nil, Result→value-or-raise, Vec as Enumerable with clone-per-element (dangling-borrow-safe where _clone exists). Counter e2e (5→8 probe) was a verified genuine pass in the 2026-05 matrix; the artifact has been regenerated since (example vendors stale May-30 azul.rb) so it needs a re-run, but no symbol drift was found — every function/constant the guide and hello-world use exists in the current target/codegen/azul.rb.


**Blockers to ship:**
- Frontpage install step 2 (`gem install ffi`, api.json ruby macos/linux) fails with Gem::FilePermissionError on the macOS system Ruby the guide explicitly endorses — change to `gem install --user-install ffi` (and consider the 1.15.x pin the README uses) so the published steps actually work on a fresh machine.
- Re-run the counter e2e against the CURRENT regenerated azul.rb + 0.2.0 libazul (last verified pass was 2026-05 against the now-stale May-30 artifact vendored in examples/ruby; md5 differs from target/codegen/azul.rb).


**Quick wins (<1 day):**
- One-line doc/example fix: `app.run(window.ptr)` → `app.run(window)` in examples/ruby/hello-world.rb:51 and doc/guide/en/hello-world/ruby.md:115 — removes the exit-time double-free and the .ptr leak from the canonical example.
- Rewrite the css section of hello-world + guide to use `Dom#with_css('font-size: 32px;')` (already in the artifact, azul.rb:91665) — drops 4 lines of Azul::Native.* calls, kills the stale 'CssProperty has no wrapper' caveat, and brings the example to ~Python parity (~40 lines).
- Codegen fix in doc/src/codegen/v2/lang_ruby/wrappers.rs emit_method_body_instance: when the self arg is Owned/by-value (api.json `self: "value"`), undefine the finalizer + nil @ptr in the plain-return branches too (lines 782-787 and 815-826) — closes the 43-widget `.dom` double-free class; regen artifact.
- Wrap non-self-typed struct returns in their wrapper class when one exists (so `button.dom` returns Azul::Dom, no manual Dom.new).
- Remove or fix the broken legacy `WindowCreateOptions.create` (azul.rb:95544) — alias it to create_with_layout; the codegen comment already documents it as non-functional.
- Freshen examples/ruby: re-vendor current azul.rb, fix README stale claims ('69-line' → 51, dated status notes), and bump azul.gemspec to 0.2.0 with a consistent required_ruby_version.
- api.json wording: change 'generate the azul.rb binding' to 'download the azul.rb binding' and add a step (or note) for obtaining hello-world.rb.


**Verdict:** Keep shipping Ruby — it is one of the strongest managed bindings (real callbacks, 43 widgets, auto conversions, careful consume/finalizer discipline) and the install story is real, but it ships with a systemic widget-`.dom` double-free in the codegen, a doc-taught `run(window.ptr)` double-free, and a frontpage `gem install ffi` step that fails on the endorsed macOS setup; ~1.5 focused days (one codegen branch fix + regen, three doc one-liners, e2e re-run) takes it from shipped-issues to shipped-solid.


## csharp — shipped-issues (install 2/5, ~2d to ship-quality)

A fresh developer following the frontpage steps runs 3 curls, makes runtimes/ RID dirs, then hits a hard error at the final step: `dotnet run -c Release` fails with "An executable project must set OutputType 'Exe'. The current OutputType is 'Library'" (reproduced today with dotnet 10.0.107) — the steps never download any hello-world source and the shipped Azul.csproj is a class library with EnableDefaultCompileItems=false, so even writing their own Program.cs next to it is silently excluded from compilation. If they instead find the guide page, they must hand-assemble a project the docs never show; a determined user who writes an Exe csproj and pastes the guide code does get a working app — the guide's counter example compiles with 0 errors against the current 8.6 MB Azul.cs, and the counter e2e genuinely passed in the 2026-05 matrix. The generated DllImportResolver (Azul.cs:49866) already finds libazul.dylib in the project dir, so the documented DYLD_LIBRARY_PATH dance is unnecessary.


**Guide/install truthfulness issues:**
- api.json ['0.2.0']['installation']['languages']['csharp'] (all 3 platforms): final step 'dotnet run -c Release' cannot succeed as written — no hello-world.cs/Program.cs is downloaded and target/codegen/Azul.csproj:8-31 is OutputType Library. Verified failure today: 'An executable project must set OutputType Exe. The current OutputType is Library.'
- target/codegen/Azul.csproj:17 sets EnableDefaultCompileItems=false and compiles only Azul.cs, so a user-created Program.cs beside it is silently ignored — neither api.json steps nor the guide mention this trap.
- doc/guide/en/hello-world/csharp.md:32-33 claims 'No Marshal.AllocHGlobal, no .Raw extraction, no IntPtr ceremony in your code' — contradicted by the guide's own example, where every callback takes raw 'IntPtr dataPtr, IntPtr infoPtr' and must call HostInvoker.RefanyGet(dataPtr) as MyDataModel (csharp.md:87-99).
- TFM story is inconsistent: guide says '.NET 8+' (csharp.md:37), examples/csharp/Hello.csproj:14 targets net10.0, shipped target/codegen/Azul.csproj:9 targets net6.0.
- macOS install steps copy ONE downloaded single-arch libazul.dylib into BOTH runtimes/osx-x64/native and runtimes/osx-arm64/native (api.json csharp macos steps 4-6); guide csharp.md:47 says Intel needs a different file (libazul.x86_64.dylib), so one RID dir always holds a wrong-arch binary. Also plain <None Copy> items don't enter deps.json native-asset probing, so the runtimes/ layout likely does nothing for a local dotnet run anyway.
- examples/csharp/README.md stale stats: ':59-64 Files' lists 'Azul.cs — generated bindings (5.9 MB)' but Azul.cs is NOT present in examples/csharp (must be copied from target/codegen; current size 8.6 MB); ':61 hello-world.cs — 84-line' (actual 49 lines); ':20-21 ~120 K LOC, 11,696 static extern' (actual 188,184 lines, 13,292 static extern).
- README.md:7 '✅ Full GUI E2E — counter probe 5→8 via AZ_DEBUG verified' — matches the 2026-05 scripts/e2e_language_matrix.md '✓ WORKS genuine pass' row but is unverifiable today (libazul not built); plausibly still true since the guide/example code compiles clean against the current binding.
- Guide csharp.md:63-64 and README:17 push DYLD_LIBRARY_PATH/LD_LIBRARY_PATH; the generated resolver (target/codegen/Azul.cs:49866-49885) already probes AppContext.BaseDirectory and the cwd, so 'put the dylib in the project dir' suffices — and DYLD_* advice collides with macOS SIP stripping in some launch paths (documented in scripts/e2e_language_matrix.md:18-21).


**Safety issues:**
- CONFIRMED latent double-free in the callback path the guide and hello-world actually use: the untyped invoker writes a returned wrapper's Raw struct bytes to outPtr (ownership moves to native) but never consumes the wrapper — doc/src/codegen/v2/lang_csharp/managed.rs:385-402 emits no __Consume, artifact target/codegen/Azul.cs:187189-187200. The wrapper finalizer ~Dom() → Dispose(false) → AzDom_delete (wrappers.rs:998-1015) later frees the same heap pointers the framework now owns. Any GC in a long-running app (e.g. clicking the counter enough times) triggers a nondeterministic double-free; short e2e probes pass because finalizers never run.
- CONFIRMED immediate use-after-free in the typed Data<T> path advertised in README ('CC-1, 17 of 19 callback kinds'): managed.rs:624-627 emits '__result.Dispose();' right after StructureToPtr hands the Dom bytes to native (artifact Azul.cs:187711, 188058). The comment claims Dispose 'detaches', but Dispose(true) calls Az<X>_delete (wrappers.rs:1005-1008); only the internal __Consume() detaches. Anyone using RegisterLayoutCallback<T>/RegisterVirtualViewCallback<T> hands the framework a freed DOM.
- Public 'Raw => _inner' (wrappers.rs:513, e.g. Azul.cs:160716) returns the owned FFI struct by value with no _disposed guard and no consume — passing it to two consuming call sites, or using a wrapper after __Consume, double-frees; only guarded by a 'use with care' doc comment.
- Finalizers run Az*_delete on the GC finalizer thread while the UI thread may be inside App.Run — native teardown thread-affinity is undocumented and unenforced (wrappers.rs:1015).
- Mitigations that DO exist: all invoker delegates and the releaser are pinned in _livePins (managed.rs:139-141,422) so no collected-thunk crashes, and the native thunk pre-fills out-values with kind defaults (core/src/host_invoker.rs:450-453) so a throwing callback yields an empty Dom rather than uninitialized memory.


**Idiomatic-ness issues:**
- Mixed naming surface: wrapper classes are clean C# (Dom, Button, App) but enums leak FFI prefixes into user code — the guide has users write '(int)AzUpdate.RefreshDom' and 'AzButtonType.Primary'; idiomatic would be Update/ButtonType and returning the enum, not an int cast.
- Smart builders accept bare 'Delegate' and dispatch via DynamicInvoke (managed.rs:350, Azul.cs:187168) — zero compile-time signature checking; a wrong-shape delegate means a silently dead button plus a stderr line. Idiomatic C# uses typed delegates.
- Callback exceptions are reported as e.Message of the TargetInvocationException — literally 'Exception has been thrown by the target of an invocation.' with no inner exception or stack (managed.rs:411-419).
- Generated code is Nullable=disable-era: consuming it from a Nullable=enable project (the repo's own Hello.csproj does) yields 30 CS8600/CS8603 warnings, plus one CS8632 inside Azul.cs itself (line 91693).
- No NuGet package for the most package-manager-centric ecosystem reviewed — users vendor an 8.6 MB single-file, single-namespace Azul.cs; the guide honestly admits 'There is no NuGet package yet'.


**Ergonomics issues:**
- hello-world is 49 lines vs the ~35-40-line Rust/Python reference; the overhead is pure ceremony: 'HostInvoker.RefanyGet(dataPtr) as MyDataModel' + null-check in every callback, 'new Func<IntPtr, IntPtr, Dom>(Layout)' wrapping, and '(int)AzUpdate.*' return casts.
- The fix for that ceremony already exists — typed RegisterLayoutCallback<T>/ButtonOnClickCallbackWithData<T> ('(MyDataModel data, LayoutCallbackInfo info) => Dom') — but neither the example nor the guide uses it, and it currently has the use-after-free bug, so the worst-ergonomics path is the only safe one.
- README (examples/csharp/README.md:24-40) documents the idiomatic surface (Data<T>, AsNullable, ToArray) that the actual example and guide never demonstrate, so a new user copies the low-level style.


**Completeness:** Well beyond smoke: callbacks work end-to-end via the untyped host-invoker path (counter e2e '✓ WORKS genuine pass' in scripts/e2e_language_matrix.md:99, 2026-05; guide code compiles 0-errors against today's Azul.cs). ~463 IDisposable wrapper classes; widgets exposed (Button, Card, Chip, TabHeader, Textarea, ProgressBar via Az*_create externs); automatic string→AzString in method params (Button.Create(string)); Option AsNullable(), Result Unwrap(), generic Vec ToArray() + IEnumerable<T> on wrapper-element Vecs, primitive Vec ToByteArray/ToIntArray/ToFloatArray. Typed Data<T> registration emitted for 17/19 callback kinds but is currently the UAF path.


**Blockers to ship:**
- Frontpage install steps are not honest as published: api.json csharp steps end in a reproducible hard error (dotnet run on a Library csproj, no example source ever downloaded). Fix = add hello-world.cs + an Exe scaffold (repo already has examples/csharp/Hello.csproj) to the download steps, or make the published csproj runnable.
- Untyped callback-return path double-free (managed.rs:385-402): it sits directly under the flagship counter demo — any GC while the app runs finalizes the returned Dom and frees memory the framework owns, so the counter e2e is only reliable in short runs. One localized codegen fix (consume the wrapper after Raw writeback) + regen.


**Quick wins (<1 day):**
- Change managed.rs:627 '__result.Dispose();' to '__result.__Consume();' (typed-path UAF, one line, same assembly so internal is accessible).
- In the untyped writeback (managed.rs:385-402), after extracting Raw call the wrapper's __Consume via the same reflection handle (~4 emitted lines) to kill the finalizer double-free.
- Fix api.json csharp steps: curl hello-world.cs + a runnable Hello.csproj (Exe, net8.0, default compile items) instead of the library Azul.csproj; deletes the only hard install failure.
- Refresh examples/csharp/README.md stale numbers and its Files list (Azul.cs is not in that directory).
- Catch TargetInvocationException in invokers and print InnerException + stack instead of the generic DynamicInvoke message.
- Unify the TFM story to net8.0 (guide, example csproj, generated csproj) and mention that the DllImportResolver makes 'dylib in project dir' sufficient — drop the DYLD_LIBRARY_PATH incantation from the happy path.


**Verdict:** Keep C# on the frontpage but treat it as shipped-with-issues: the binding is genuinely functional (counter e2e was a real pass, guide code compiles clean today, rich wrapper/widget surface), yet the published install steps dead-end at `dotnet run` and both callback-return paths have code-confirmed memory bugs — about 2 focused days (install-step fix + two small codegen ownership fixes + doc refresh + re-run e2e) to solid.


## python — shipped-issues (install 4/5, ~2d to ship-quality)

On macOS a fresh developer runs one curl (azul.so) and pastes the guide's counter example — it crashes immediately with AttributeError at `window.window_state.title = ...` because the binding exposes zero struct fields, and even after deleting those lines the button never increments because `Update.RefreshDom()` (guide says "constructor calls, hence the trailing ()") raises TypeError that the trampoline silently swallows into DoNothing. If they instead grab examples/python/hello-world.py (the filename the frontpage install steps reference), they get a silently blank window: `body.style(Css.empty())`, `HoverEventFilter.MouseUp()` and `Update.RefreshDom()` all fail against the current artifact and no logger is ever installed, so no traceback appears anywhere. Only the sibling examples/python/hello_world.py (underscore, last verified 2026-05-28) matches the current binding and should work — but it is not the file the docs point at, and python is explicitly SKIPped in the counter-E2E matrix, so nothing proves the shipped artifact today.


**Guide/install truthfulness issues:**
- doc/guide/en/hello-world/python.md:79 uses `Dom.p_with_text(...)` — no such method; the artifact only has `create_p_with_text` (target/codegen/python_api.rs:125033). AttributeError inside layout, swallowed to a blank window.
- python.md:84 `button.set_on_click(data, on_click)` — Button has only the builder `with_on_click` (python_api.rs:122115); `set_on_click` does not exist anywhere in the artifact (grep: 0 hits).
- python.md:110-112 `window.window_state.title = "Hello World!"` / `...size.dimensions.width = 400.0` — the binding generates ZERO field getters/setters (grep '#[getter]'/'#[setter]'/'pyo3(get' = 0 across 4.8MB); AttributeError at startup. There is currently NO way to set window title/size from Python (WindowCreateOptions exposes only `create`, python_api.rs:127376-127404).
- python.md:96 claims 'Update variants in Python are constructor calls, hence the trailing ()' — false: unit variants are #[classattr] instances (python_api.rs:128791-128800); `Update.RefreshDom()` raises TypeError (not callable) which the trampoline converts to DoNothing, freezing the counter. The guide contradicts itself at line 178 ('return Update.RefreshDom', no parens — that form is correct).
- python.md:88-89 'Mutating set_/add_ methods are also available' — every generated set_*/add_* on value types clones self, mutates the clone and discards it (e.g. add_child python_api.rs:118842-118850, set_title 118966-118975): they are silent no-ops.
- examples/python/hello-world.py (the file api.json's `python3 hello-world.py` step implies) is stale on 3 APIs: line 25 `body.style(Css.empty())` (Dom has no `style` method, grep 'fn style(' = 0 Dom hits), line 17 `HoverEventFilter.MouseUp()` (classattr, not callable), line 29 `Update.RefreshDom()` (same). Result: silently blank window. examples/python/widgets.py:15 has the same MouseUp() staleness.
- python.md:54 'Targets Python 3.10+' — unverifiable: dll/Cargo.toml:41 enables pyo3 'abi3' WITHOUT an abi3-py310 pin, so the real minimum is whatever interpreter CI built with.
- examples/python/hello_world.py:24 docstring 'Verified 2026-05-28: runs headless under AZ_E2E and prints test result: ok' — stale/unverifiable: scripts/e2e_language_matrix.md:71 says python is 'NOT a counter-E2E binding: no AZ_E2E example ... always SKIP', so no automated run backs this today.
- api.json install steps themselves are consistent with the CI release pipeline (.github/workflows/rust.yml:1122-1162 really produces azul.so / azul.cpython.so / azul.pyd) — this part is truthful, modulo the unverifiable live URL.


**Safety issues:**
- Silent exception swallowing with NO logger installed: every trampoline (invoke_py_callback python_api.rs:52526-52596, invoke_py_layout_callback 53389-53440, etc.) turns a Python exception into DoNothing/empty-Dom with only `log::error!` behind `#[cfg(feature="logging")]` — but under the python-extension build fern is disabled (dll/src/desktop/app.rs:247-254 gates on not(pyo3_logger)) and pyo3_log::init is never called anywhere (grep 'pyo3_log' in python_api.rs and dll/src = no init call). A typo in a callback = blank window/dead button with zero output. This is the #1 hazard a normal user hits.
- Universal native-memory leak: 0 `impl Drop`/`Drop for` in the entire 4.8MB artifact; pyclass wrappers hold a repr(C) mirror struct (e.g. AzDom python_api.rs:65939) whose drop is a no-op, so every Dom/String/Vec built in Python leaks its Rust heap when GC'd — and the layout callback rebuilds the DOM on every RefreshDom, so memory grows per click/frame. (Deliberate leak-over-double-free tradeoff — memory-SAFE thanks to deep Clone-at-boundary, e.g. mirror AzDomVec Clone transmutes to real DomVec and deep-clones, python_api.rs:25881 — but a real leak in long-running apps.)
- App.run holds the GIL for the whole event loop: `fn run(&self, ...)` (python_api.rs:100395-100403) never uses py.allow_threads, so any background Python thread the user spawns (downloads, logging, schedulers) starves forever and Ctrl+C is dead. Normal desktop-app code hits this immediately.
- Silent no-op mutators: all set_*/add_* methods on value types operate on a discarded clone (`let mut __cloned = _self.clone(); __cloned.set_x(...)`, e.g. python_api.rs:118966 set_title, 118842 add_child, 104731 modify_window_state) — not memory-unsafe, but state silently vanishes, which reads like the classic 'mutation isn't sticking' bug the guide itself warns about (python.md:180) while advertising these methods (python.md:88).
- Positives worth recording: unit trampolines pass the SAME Py object back (py_data.clone_ref, python_api.rs:52586) so `data.counter += 1` semantics are genuine; RefCount clone is a real refcount bump (python_api.rs:17431); thread-affine types are marked `unsendable` (e.g. App python_api.rs:57565) so cross-thread misuse raises instead of UB.


**Idiomatic-ness issues:**
- Flat namespace with 1438 classes registered into one `azul` module (python_api.rs:151073) and `from azul import *` in all examples; scripts/BINDING_STRATEGY_PER_LANGUAGE.md:61 itself planned `azul/__init__.py` + submodules (azul.app, azul.dom, azul.widgets) — never done; guide even says Button is 'from the azul.widgets module' (python.md:82) which does not exist.
- Inconsistent enum-variant convention is a live trap: unit variants are class attributes (`Update.RefreshDom`, `HoverEventFilter.MouseUp` — #[classattr]) while data variants are called (`EventFilter.Hover(x)` — #[staticmethod], python_api.rs:138313); calling a unit variant raises TypeError that callbacks then swallow silently. Un-pythonic (stdlib enum members are never 'sometimes callable') and it has already bitten the project's own guide and 2 examples.
- No exceptions are ever raised from the binding: no PyResult/PyErr anywhere; all failures degrade to silent defaults (DoNothing, empty Dom) — opposite of Python's 'errors should never pass silently'.
- No properties/field access at all (0 getters/setters): everything is method-only, and the mutating method half (set_*/add_*) is no-ops, so the ONLY working idiom is the with_* builder chain — fine, but undocumented as such.
- No .pyi type stubs shipped for a 1438-class C extension → zero IDE autocomplete/mypy support.
- `create_*` factory naming (Dom.create_div, Dom.create_text) is C-flavored; Python would prefer Dom.div()/Dom.text() or real constructors, though this is consistent cross-language design, not a bug.


**Ergonomics issues:**
- The good: examples/python/hello_world.py is a genuine ~40-line fluent hello world with `data.counter += 1` — meets the project's own reference standard, plain classes, plain str, no ceremony. When the binding works, this IS the best-looking Azul language.
- Window title/size cannot be configured at all from Python: WindowCreateOptions has only `create(layout)` (python_api.rs:127376), no with_title/with_size, no field access — the guide's own hello world needs features the binding doesn't expose.
- `with_children` takes an AzDomVec that Python cannot populate: AzDomVec has create/with_capacity/len/is_empty but NO push and no list->DomVec conversion (python_api.rs:107990-108040) — users must chain with_child n times; a Python list should be accepted.
- Every builder step deep-clones the whole subtree (with_child clones self + child, python_api.rs:125105-125114) → O(n^2) DOM construction plus a leaked intermediate per step (no Drop).
- Only U8Vec and StringVec auto-convert to/from Python (python_api.rs:52296-52302, 52245); all other Vec/Option types stay as opaque wrapper classes.
- Two hello-world files with near-identical names (hello-world.py stale/broken, hello_world.py current) plus hello_world_window.py in the same directory; the docs point at the broken one; no README in examples/python/ to disambiguate.


**Completeness:** Far beyond smoke: full callback plumbing exists by inspection (layout, event, button on_click, timer, thread+writeback, virtual-view, text/number-input, map callbacks — trampolines at python_api.rs:52526-53440+) and the trampoline hands back the same Python data instance, so `data.counter += 1` semantics are real; ~1438 pyclasses including the widget set (Button, CheckBox, TextInput, ProgressBar, TreeView, ListView, Map, Titlebar...). Auto-conversion: str<->AzString everywhere (2900+ methods return String), Vec only for u8/String, Option/other Vecs stay wrapped; zero struct-field access; all set_*/add_* mutators are silent no-ops. However there is NO wired counter-E2E for python (matrix always SKIPs it), so 'callbacks work' rests on a 2026-05-28 manual note plus code inspection, not a current run.


**Blockers to ship:**
- Guide hello-world (doc/guide/en/hello-world/python.md) does not run against the shipped binding: 4 independent API mismatches (p_with_text, set_on_click, window_state field assignment, Update.RefreshDom() parens) — the frontpage promise fails on paste.
- examples/python/hello-world.py — the file the install steps tell users to run — is stale on 3 APIs and produces a silently blank window (no logger installed, exceptions swallowed).
- Counter e2e is unproven for the shipped artifact: scripts/e2e_language_matrix.md:71 marks python 'always SKIP'; the ship bar (hello-world counter e2e passes) is currently an unverified 6-week-old claim, and libazul isn't built in this checkout to re-verify.


**Quick wins (<1 day):**
- Replace examples/python/hello-world.py and the guide snippet with the verified shape of examples/python/hello_world.py (no-parens unit variants, with_on_click builder, no window_state lines); delete or fix widgets.py's `MouseUp()` and the guide's 'trailing ()' paragraph. ~2 hours.
- Surface callback exceptions: in lang_python.rs trampoline Err arm, call e.print(py) (or write the traceback to stderr) instead of feature-gated log::error!, and/or call pyo3_log::init in the pymodule — one codegen change kills the 'silent blank window' failure mode entirely.
- Make unit-variant instances callable (add __call__ returning self on classattr enum instances) so both `RefreshDom` and `RefreshDom()` work — retroactively fixes every stale doc/example in one stroke.
- Either generate real receiver-mutating set_*/add_* (write __cloned back through &mut self) or stop emitting them on value types; also add with_title/with_size to WindowCreateOptions so the hello world can name its window.
- Accept Python lists in with_children (FromPyObject for DomVec) and pin abi3-py310 in dll/Cargo.toml:41 to make the 'Python 3.10+' claim true.
- Wire a python row into scripts/e2e_language_matrix.sh (build python-extension in CI already happens at rust.yml:1122; just run hello_world.py under AZ_E2E headless).
- Emit a .pyi stub file from the same IR for IDE autocomplete.


**Verdict:** Keep python on the frontpage — the pyo3 extension is architecturally the best binding Azul has (real callbacks, same-instance data model, honest 2-command install) — but ship the ~2 days of fixes urgently: today every documented entry point (guide snippet and the referenced hello-world.py) fails against the current artifact, and the failure mode is a silent blank window because callback exceptions are swallowed with no logger installed.


## rust — shipped-issues (install 2/5, ~2d to ship-quality)

A fresh developer runs the frontpage command 'cargo add azul --git https://github.com/fschutt/azul' and gets an immediate cargo error: there is no package named 'azul' in the repo (it's 'azul-dll'). If they find the hello-world guide and use its correct 'cargo add azul-dll --rename azul --tag 0.2.0' form, 'cargo build' then panics in azul-dll's build.rs with 'MISSING GENERATED FILE — run: cd doc && cargo run --release -- codegen all', because the generated binding sources are gitignored and absent from the repo and the 0.2.0 tag; that fix command cannot be run inside a cargo git checkout. The only working path today — clone the repo, build the azul-doc tool, run 'codegen all', then use a path dependency — is what the in-repo CI does (counter e2e genuinely passes there) but is documented nowhere user-facing. The prebuilt libazul.dylib/.dll/.so, .deb and .rpm artifacts all verifiably exist (HTTP 200), but no published cargo instruction can consume them.


**Guide/install truthfulness issues:**
- api.json rust install step 1: "cargo add azul --git https://github.com/fschutt/azul" — FALSE/broken: no package named 'azul' exists anywhere in the workspace (dll/Cargo.toml:3 name = "azul-dll"; only [lib] name = "azul" at line 17). cargo add resolves by package name → immediate error. The guide (doc/guide/en/hello-world/rust.md:106-108) uses the correct 'cargo add azul-dll --rename azul' form, so frontpage and guide contradict each other.
- api.json rust install step 2: "cargo build" — FALSE: build fails for any git-dependency consumer. dll/src/lib.rs:195-245 include!()s ../target/codegen/{dll_api_internal.rs,dll_api_external.rs,reexports.rs}; target/ is gitignored, 'git ls-files target/codegen' is empty on master, and 'git ls-tree -r 0.2.0 target/codegen' shows 0 files at the tag. dll/build.rs:418-453 (and the tag's build.rs:154+) deliberately panics: 'Missing generated file ... Run: cargo run --release -p azul-doc -- codegen all' — not runnable inside ~/.cargo/git/checkouts.
- Guide rust.md:85-94: "export AZ_LINK_PATH=/my/path/to/libazul.so ... build.rs defaults to system-installed libazul if unset" — the env var name is wrong. dll/build.rs:487 reads AZ_DLL_PATH only ('grep AZ_LINK_PATH dll/build.rs' = no hits), and it expects comma-separated DIRECTORIES (dll/build.rs:480-481), not a file path as shown. Same wrong var in README.md:42 ('cargo add azul --features link_dynamic' there is triple-wrong: package, feature spelling, env var) and in the release-page template doc/src/dllgen/deploy.rs:1655.
- Guide rust.md:151: "This will give you a guaranteed build" (link-static via git tag) — FALSE for the documented git-dependency route, same missing-generated-files panic.
- Guide rust.md:184/188 code snippet: 'extern "C" fn my_layout_func(data: RefAny, ...)' followed by 'data.downcast_ref::<DataModel>()' does NOT compile — downcast_ref takes &mut self (target/codegen/dll_api_internal.rs:38199; codegen source doc/src/codegen/v2/lang_rust.rs:483). Parameter must be 'mut data: RefAny' as the repo example correctly has (examples/rust/src/hello-world.rs:10).
- Guide rust.md:259: downcast_ref "returns Option<RefMut<DataModel>>" — wrong: it returns Option<Ref<T>>; RefMut comes from downcast_mut.
- Verified TRUE (credit where due): all binary-artifact URLs work — https://azul.rs/ui/release/0.2.0/libazul.dylib and azul.dll return HTTP 200, the GitHub-release .deb and .rpm resolve to HTTP 200; 'Azul is not published on crates.io yet' is accurate; the code API used in the guide (Dom::create_p_with_text, with_css, Button::create/set_on_click/dom, WindowCreateOptions::create) all exist in the generated binding.


**Safety issues:**
- AzString::as_str uses core::str::from_utf8_unchecked on library-provided bytes (codegen source doc/src/codegen/v2/lang_rust.rs:565; artifact target/codegen/dll_api_internal.rs:38236 and same in dll_api_external.rs) — UB with completely normal code if any AzString ever carries non-UTF8 bytes, e.g. FilePath::as_string (dll_api_internal.rs:30032) for a non-UTF8 Linux filename, or strings produced by another language binding sharing the same libazul process. Should be checked or lossy.
- RefAny's runtime type check truncates TypeId to a weak 8-byte hash: get_type_id_static reads the first 8 bytes of core::any::TypeId via raw-slice reinterpretation and byte-shift-sums them (doc/src/codegen/v2/lang_rust.rs:367-395; target/codegen/dll_api_external.rs:38221-38226). A collision between two user types would let downcast_ref/downcast_mut reinterpret one type as another (UB). Probability is astronomically low, but a safety-checked cast should compare the full 128-bit TypeId; also the raw-byte read of an opaque std type is layout-fragile across compiler versions.
- AzString::into_string's doc-comment claims 'If the memory was library-allocated, takes ownership without copying' but the body is always self.as_str().to_string() (dll_api_internal.rs:38243-38247) — no safety bug (Drop still runs), but the comment describes an ownership transfer that doesn't exist.
- Positive findings worth recording: the dynamic binding wires 841 Drop impls + Clone/PartialEq/Hash through the C API (rust/dynamic_binding.rs:164-268); by-value self methods rely on Rust move semantics so no double-free path exists in normal code; RefAny borrows are runtime-checked (can_be_shared/can_be_shared_mut guards, lang_rust.rs:421) so callback reentrancy degrades to downcast=None instead of UAF; RefAny::new's stack-copy destructor trampoline is sound.


**Idiomatic-ness issues:**
- Constructor naming is C-API-derived, not Rusty: App::create, AppConfig::create, Button::create, Dom::create_body/create_text instead of new()/body()/text() (pervasive; e.g. target/codegen/dll_api_internal.rs:29387+). Consistent cross-language but reads foreign in Rust.
- Inherent associated fns duplicate std traits: every type gets 'pub fn clone(instance: &X) -> X' and 'pub fn default()' alongside real Clone impls (e.g. target/codegen/azul.rs:17165-17166, dll_api_internal.rs AzButton::clone) — API noise in docs/autocomplete; Default trait itself is only implemented for Option wrappers, not core types.
- No std::Result integration: ResultXmlXmlError etc. expose only ok()/err() constructors (dll_api_internal.rs:34730-34739); Options got 217 into_option() helpers but Results got no into_result() — asymmetric, and Drop-carrying enums make manual payload extraction require clone or unsafe.
- Callbacks must be free 'extern "C" fn' with RefAny type-erasure and runtime downcast — no closure support, unlike every mainstream Rust GUI library. Documented and deliberate (C ABI even in-process, guide rust.md:258), but it is the single biggest 'this isn't normal Rust' moment; a thin generic closure-to-extern-fn adapter would remove it.
- downcast_ref requiring &mut self is surprising for a shared borrow (forces 'mut data' bindings everywhere and caused the guide's own snippet bug).
- Good: module system properly used (azul::app/dom/widgets/prelude via reexports.rs), snake_case methods, builder with_* / setter set_* duality, RAII Drop everywhere (841 Drop impls in the dynamic binding), IntoIterator on vecs.


**Ergonomics issues:**
- Hello-world is at the reference standard: 49 lines with comments, fluent Dom builder, data.counter += 1 callback (examples/rust/src/hello-world.rs) — no gap here.
- Default feature is link-static, so the documented first-contact 'cargo add + build' compiles ~300 crates for 2-4+ minutes, contradicting the guide's core pitch of fast DLL-based rebuilds; link-dynamic requires understanding --no-default-features first.
- Generated Result enums are painful: they carry custom Drop so payloads can't be moved out by pattern match; even the generator's own code resorts to 'unsafe { core::ptr::read(json) } + mem::forget' to extract an Ok value (target/codegen/azul.rs, RefAny::new_serde serialize trampoline, ~line 48711). Users must clone or write unsafe.
- examples/rust has no README.md (most other language example dirs carry status/setup notes); nothing tells a user how to build the examples without discovering the codegen prerequisite.
- CSS is stringly-typed at the happy path (set_css("font-size: 50px")) — acceptable by design since a typed CssProperty API also exists, but the guide only shows strings.
- reexports.rs prelude has silently-missing entries emitted as comments: '// WARNING: Type 'On'/'Label'/'WindowState'/'OptionStyledDom' not found in any module' (target/codegen/reexports.rs prelude/widgets modules).


**Completeness:** Full, not smoke-only — in-repo. Callbacks work end-to-end (hello-world counter with data.counter += 1 is the AZ_E2E gate example; scripts/e2e_language_matrix.md line 99 lists rust as 'genuine pass'). ~30+ widgets exposed via azul::widgets (Button, TextInput, CheckBox, ListView, TreeView, Ribbon, Map, Video, ...) and a 13-example suite (async, calc, opengl, widgets, anim, icu, fluent, http/zip). Auto-conversions present: impl From<&str>/From<String> for AzString, as_str/into_string, 217 into_option() helpers, as_slice/iter_mut/IntoIterator on Vec types, auto From impls between wrapper types (Into<T> params on every method). Gap: generated Result enums (ResultXmlXmlError etc.) have no into_result()/std Result conversion. All of this only reachable via in-repo build today, since the published install path doesn't build.


**Blockers to ship:**
- Frontpage install steps (api.json ['0.2.0'].installation.languages.rust) fail at step 1: 'cargo add azul --git https://github.com/fschutt/azul' — no package named 'azul' exists in the repo (package is 'azul-dll', only the [lib] name is 'azul'; dll/Cargo.toml:3,17). The guide's corrected command exists but the frontpage one errors immediately.
- Even with the correct package name, 'cargo build' as a git dependency panics in dll/build.rs check_generated_files() (dll/build.rs:418-453): the required generated sources (target/codegen/dll_api_internal.rs, dll_api_external.rs, reexports.rs) are gitignored and absent from both master and the 0.2.0 tag ('git ls-tree -r 0.2.0 target/codegen' = 0 files; at the tag it's target/codegen/v2/dll_api_static.rs, also absent). The panic tells users to run azul-doc codegen — impossible inside a ~/.cargo/git checkout. Fix by committing/bundling generated bindings at release tags, generating in build.rs, or rewriting the install docs to a truthful clone+codegen+path-dep flow.
- Guide's dynamic-link instructions reference an env var that does not exist: 'export AZ_LINK_PATH=...' (doc/guide/en/hello-world/rust.md:85-94, README.md:42, doc/src/dllgen/deploy.rs:1655) while dll/build.rs only reads AZ_DLL_PATH (dll/build.rs:480-487), and AZ_DLL_PATH takes comma-separated DIRECTORIES, not the file path shown.


**Quick wins (<1 day):**
- Fix api.json rust install steps to 'cargo add azul-dll --rename azul --git https://github.com/fschutt/azul --tag 0.2.0' (matches the guide) — 10 minutes.
- Add AZ_LINK_PATH as an accepted alias of AZ_DLL_PATH in dll/build.rs (or sed AZ_LINK_PATH→AZ_DLL_PATH across guide rust.md:94, README.md:42, deploy.rs:1655) and make it accept a file path by taking parent() — under an hour.
- Fix guide snippet: 'mut data: RefAny' in my_layout_func (rust.md:184) and correct 'Option<RefMut<...>>' to Option<Ref<...>> (rust.md:259).
- Generate into_result() for Result enums, mirroring the existing into_option() codegen in doc/src/codegen/v2/lang_rust.rs:1001-1100.
- Replace from_utf8_unchecked in AzString::as_str with a checked/lossy variant (doc/src/codegen/v2/lang_rust.rs:565).
- Add examples/rust/README.md documenting the clone → 'cargo run -p azul-doc -- codegen all' → 'cargo run --example hello-world' flow so the in-repo path is at least discoverable.
- Clean the '// WARNING: Type X not found' entries out of reexports.rs prelude (fix or drop the four stale names: On, Label, WindowState, OptionStyledDom).
- Delete or finish the legacy target/codegen/azul.rs artifact (generator.rs:155 calls it 'legacy, may be removed'; it has zero extern declarations and cannot compile standalone — good that it 404s on the release page, but it's dead weight in the codegen run).


**Verdict:** Keep shipped but fix the install story urgently: the binding itself is the best of the 27 (native language, full RAII, modules, conversions, counter e2e genuinely green in CI), yet every published install command fails — the frontpage names a nonexistent package and both frontpage and guide paths die in build.rs because the generated binding sources are gitignored and absent from the repo and the 0.2.0 tag, with the guide additionally documenting a nonexistent AZ_LINK_PATH env var; roughly two focused days (commit/bundle generated sources at release tags + doc corrections) restores honesty.


## ocaml — shipped-issues (install 2/5, ~2d to ship-quality)

A fresh developer following the published api.json steps runs `opam install ctypes ctypes-foreign dune`, curls libazul + azul.ml/.mli/dune/dune-project, then hits `LD_LIBRARY_PATH=. dune exec ./hello_world.exe` — which fails three ways: the steps never download hello_world.ml; the shipped `dune` declares `(public_name azul)` with no azul.opam so `dune build` hard-errors immediately (verified); and even after removing public_name, dune's default dev-profile flags turn a duplicate `val create` in azul.mli (line 10752, shadowed by line ~10796 in the same ColorU signature) into an Error(warning 32). A user who instead copies examples/ocaml/ gets a fourth failure: hello_world.ml:36 no longer type-checks because Button.with_on_click now takes a typed az_button_on_click_callback (azul.mli:15665) while the example registers a generic azul_register_callback — a one-line fix (azul_register_button_on_click_callback) makes it compile clean, verified in scratchpad. So today nobody reaches a window from the docs, though the underlying binding machinery (host-invoker callbacks, RefAny handle table, dlopen with AZ_DYLIB override) is genuinely solid.


**Guide/install truthfulness issues:**
- api.json ocaml steps (all 3 platforms) end with `dune exec ./hello_world.exe` but no step downloads or creates hello_world.ml, and the downloaded `dune` (target/codegen/dune) contains only a library stanza — dune exec has no executable to run.
- Shipped scaffolding does not build: target/codegen/dune line 15 `(public_name azul)` with no azul.opam file -> `Error: You cannot declare items to be installed without adding a <package>.opam file` (reproduced with dune 3.x from the exact shipped files).
- Even with public_name removed, `dune build` fails under default flags: duplicate `val create` in the same module signature (target/codegen/azul.mli:10752 vs ~10796, module ColorU) triggers Error (warning 32 [unused-value-declaration]). The example's private dune (examples/ocaml/dune) suppresses -32 etc.; the shipped codegen dune does not — the release artifacts as-downloaded cannot compile.
- Guide code snippet (doc/guide/en/hello-world/ocaml.md:87,105) and examples/ocaml/hello_world.ml:20,36 no longer compile against the current artifact: `azul_register_callback` returns az_callback but Button.with_on_click requires az_button_on_click_callback (azul.mli:15665) — regression from the typed Button.onClick change (3af3fac9b) that fixed JVM/C# examples but missed OCaml. Verified type error + verified one-line fix compiles.
- examples/ocaml/README.md:8 claims '✅ Full GUI E2E — counter probe 5→8 via AZ_DEBUG verified (landed 2026-05-12)' — stale: the example does not compile today, so the claim is not currently reproducible.
- Guide download URLs (https://azul.rs/ui/release/0.2.0/azul.ml etc., ocaml.md:46-57) are unverifiable offline; the filenames do exist in target/codegen, but the guide also tells users to download the broken `dune`/`dune-project` scaffolding described above.
- Honest bits worth keeping: 'There is no opam package yet' (ocaml.md:41), the `open Azul` shadows Stdlib.String warning (ocaml.md:63), and the azul_consume/SIGABRT explanation (ocaml.md:118-121,157) are all true and verified.


**Safety issues:**
- Latent double-free in the DOCUMENTED pattern: `raw_dom` (azul.ml:49420) extracts the struct bytes without marking the wrapper disposed, but Gc.finalise on every wrapper calls Az<X>_delete (make_dom, azul.ml:49403-49411). In the guide/example's own layout callback, the create_text temporary, label_div, and the final body dom are passed/returned by value into libazul (which takes ownership of their heap pointers) while their OCaml wrappers stay finalizable — the next major GC double-frees the entire returned DOM. hello-world only survives because its heap is too small to trigger a major GC; any real app crashes eventually. Only app_config gets azul_consume in the example (hello_world.ml:51); the three dom wrappers do not. Fix: make raw_* consume, or make with_child/layout-return take wrappers and consume them (codegen: doc/src/codegen/v2/lang_ocaml/wrappers.rs ~line 1037-1065 handles self-consume but never argument-consume).
- ThreadCallback invoker (azul.ml:40664-40676) is a Foreign.funptr with default runtime_lock:false and no caml_c_thread_register — libazul invokes it from a worker thread that neither holds nor acquires the OCaml runtime lock -> heap corruption/crash if any user touches the Thread API. scripts/BINDING_STRATEGY_PER_LANGUAGE.md:261 documents that OCaml needs the acquire/release pair; it is not implemented.
- azul_refany_get is `'a option` via Obj.magic/Obj.obj (azul.ml:40723-40729): with two different RefAny payload types in one app, a wrong or copy-pasted type annotation silently reinterprets memory — no runtime type tag check. Normal multi-callback code can hit this.
- azul_consume : 'a -> unit does Obj.set_field (Obj.repr a) 1 true (azul.ml:40733-40736): calling it on anything that is not a {raw; disposed} wrapper record (e.g. a raw Ctypes.structure, an int) corrupts memory or crashes; no type guard.
- Every generated to_string/to_dbg_string (e.g. Button azul.ml:69208-69215; emitter doc/src/codegen/v2/lang_ocaml/wrappers.rs:748+) reads the returned AzString's bytes but never calls AzString_delete — per-call memory leak.
- azul_register_*_callback + azul_refany_create allocate a fresh handle-table entry on EVERY layout invocation (pattern in guide, hello_world.ml:20-21); cleanup relies entirely on libazul calling the host-handle releaser (azul.ml:40365-40371) when it drops the callback structs — unverifiable without a runtime; if the releaser is not called per relayout the table grows unboundedly.


**Idiomatic-ness issues:**
- Update codes are bare ints (`1 (* Update.RefreshDom *)`, hello_world.ml:12) instead of a variant type; Azul.Update.refresh_dom constants exist (azul.ml:4253-4257) but even the reference example does not use them. Idiomatic OCaml would be `type update = Do_nothing | Refresh_dom | ...` with conversion in the invoker.
- Callback parameters are untyped `unit Ctypes.ptr` requiring manual Ctypes.from_voidp casts (hello_world.ml:6-8); CallbackInfo is never wrapped, so anything beyond the counter demo drops to raw FFI.
- camelCase leaks into an snake_case API: az_option_timer_id_intoSome etc. (160 occurrences, azul.ml:10419+) violate OCaml naming conventions.
- Duplicate method names within one module signature silently shadow (ColorU has two `val create`, azul.mli:10752/10796) — the first overload is unreachable; this is the OCaml echo of the JVM/C# create-overload collision fixed elsewhere in 3af3fac9b.
- `open Azul` shadows Stdlib.String (and the flat top-level also exposes ~40k values like ffi_az_* / az_*_field_* beside the clean submodules); guide works around it by banning `open Azul` rather than the binding avoiding the collision.
- Resource management is GC-finalizer + manual azul_consume with Obj tricks — no `with_`/scoped idiom and no typed ownership transfer; users must reason about move semantics the language cannot express here.


**Ergonomics issues:**
- hello-world is 53 lines vs the ~35-40-line python/rust reference, and the extra lines are pure ceremony: from_voidp casts, four locally-defined arg-flip helpers just to use |> (hello_world.ml:23-26), magic int literals for ButtonType (1) and Update (0/1), and a mandatory azul_consume with a comment explaining a SIGABRT.
- Ownership rules are inconsistent and memorization-based: with_* methods consume self automatically, but passing a wrapper's raw bytes as an argument (raw_dom child) or returning them from layout does NOT consume — the user must know which calls need manual azul_consume, and the penalty for forgetting is a delayed GC-time crash.
- Button.dom returns a raw az_dom structure while Dom methods return managed wrappers (azul.ml:69200) — inconsistent return conventions within one fluent chain.
- The generic azul_register_callback still exists alongside 15+ typed azul_register_<widget>_callback helpers with no compile-time guidance on which one a given with_on_click needs — exactly the trap the current example fell into.
- Option/Result extractors return raw `<payload_ffi> Ctypes.structure option` sharing bytes with the parent (README caveat), not owned OCaml values — a clone footgun documented instead of designed away.


**Completeness:** Callbacks are fully wired (host-invoker pattern: generic Callback, LayoutCallback, ThreadCallback, plus 15+ typed widget callbacks with pinned invokers and a host-handle releaser), and the counter e2e passed in May 2026 — but the shipped example no longer compiles against the current artifact (typed Button.onClick), so today it is effectively compile-broken pending a verified one-line fix + runtime re-verification. Widgets are broadly exposed (Button, CheckBox, NumberInput, TextInput, DropDown, TreeView, Ribbon, Tab, Chip, Card, FileInput, MapWidget...). Auto-conversion: owned String args accept plain OCaml strings (azul_az_string) and String.to_string decodes back; 52 Vec modules have clone-out to_list + 5 primitive to_array; 160 Option/Result intoSome/intoOk extractors exist but yield raw Ctypes structures, not native 'a option/result at method boundaries.


**Blockers to ship:**
- hello_world.ml / guide snippet does not compile against the current bindings (azul_register_callback vs typed az_button_on_click_callback, hello_world.ml:20/36, azul.mli:15665) — fix is one line (azul_register_button_on_click_callback, verified to compile), then the counter e2e must be re-run once libazul is built.
- Published install path cannot produce a first window: shipped target/codegen/dune fails `dune build` ((public_name azul) without azul.opam), lacks warning-suppression so azul.mli's duplicate `val create` is a hard error, has no executable stanza, and api.json steps never fetch hello_world.ml — fix dune.rs emitter (drop public_name or ship azul.opam, add the -w flags from examples/ocaml/dune) and add the hello_world.ml step to api.json.


**Quick wins (<1 day):**
- Apply the verified one-line fix to examples/ocaml/hello_world.ml and the guide snippet: azul_register_callback -> azul_register_button_on_click_callback (compiles clean immediately).
- In doc/src/codegen/v2/lang_ocaml/dune.rs: remove (public_name azul) and copy the warning-suppression flags + explanatory comment from examples/ocaml/dune; optionally emit a commented executable stanza for hello_world.
- Add a `curl -O .../hello_world.ml` step to api.json ocaml install steps on all three platforms (or change the last step to instruct pasting the guide snippet).
- Deduplicate method emission so a module signature never declares the same val twice (ColorU double `val create`) — also un-shadows the lost overload.
- Make raw_dom/raw_* mark the wrapper consumed (Dom is documented create-only), eliminating the latent double-free in the canonical child/return pattern without any user-facing API change.
- Free the AzString in generated to_string/to_dbg_string helpers (wrappers.rs:748+).
- Use Azul.Update.refresh_dom / do_nothing constants in the example instead of bare 0/1; update README.md status line to reflect current state.


**Verdict:** Keep OCaml listed only after a ~2-day fix pass: the binding architecture (host-invoker callbacks, handle table, dlopen with AZ_DYLIB) is sound and was e2e-green in May, but today the shipped example doesn't compile, the downloadable dune scaffolding hard-fails, and the canonical DOM-building pattern carries a latent GC-time double-free — all cheap to fix, and until then the frontpage promise is not honest.


## cpp — shipped-issues (install 2/5, ~2.5d to ship-quality)

A fresh macOS user follows the frontpage steps: curl libazul.dylib + azul20.hpp, then clang++. The compile fails immediately with "'azul.h' file not found" because every azulNN.hpp does `#include "azul.h"` (azul17.hpp:21) and no step downloads it. After finding azul.h on the release page, the link succeeds, but ./hello-world fails to launch: the dylib's LC_ID_DYLIB is the absolute build path (verified: examples/cpp/cpp20/libazul.dylib reports /Users/fschutt/.../target/release/deps/libazul.dylib; no -install_name/@rpath or install_name_tool anywhere in dll/build.rs or rust.yml), so the documented -Wl,-rpath,@executable_path does nothing. Once past both hurdles, the cpp20/cpp11/cpp14/cpp23 hello-worlds are solid (all six syntax-check clean against today's artifacts; cpp20 counter e2e was verified green in prior sessions), but a user who follows the guide — which is explicitly written for C++17 — gets a snippet that does not compile (with_component_css) on a header (azul17.hpp) whose builder chains double-free at runtime.


**Guide/install truthfulness issues:**
- Guide example does not compile: doc/guide/en/hello-world/cpp.md:199 uses `.with_component_css(Css::empty())` — no such member exists in azul17.hpp (only void add_component_css/set_component_css, azul17.hpp:29241-29242). Verified: clang++ -std=c++17 -fsyntax-only on the extracted snippet fails with exactly this one error and passes with the line removed.
- Install steps omit azul.h: api.json cpp20 macOS/linux/windows steps download only libazul + azulNN.hpp, and the guide's download block (cpp.md:69-80) likewise — but every azulNN.hpp requires azul.h (azul17.hpp:21 `#include "azul.h"`). First compile fails. (The C row of scripts/e2e_language_matrix.md:48 even lists 'azul20.hpp,azul.h' as required artifacts.)
- Deducing-this claim is false: cpp.md:126-129 says azul23.hpp emits every with_* as `template<class Self> auto with_xxx(this Self&& self,…)`; the feature is hardcoded off (doc/src/codegen/v2/lang_cpp/cpp20.rs:872 and :982 `let use_deducing_this = false;`) and azul23.hpp contains zero occurrences. cpp.md:286's claim that the cpp23 example 'exercises the deducing-this builders' is also false — examples/cpp/cpp23/hello-world.cpp:31-39 reassigns lvalues instead.
- Modules recipe doesn't work: cpp.md:336-337 says `clang++ -std=c++20 -fmodules -c azul.cppm` — -fmodules is clang header-modules, not named modules; Apple clang 16 (current CLT) fails to compile even a minimal .cppm with any flag combination I tried (--precompile, -x c++-module). azul.cppm's own header comment says `-fmodules-ts` (a GCC-ism). The module path is untested and currently unusable on macOS.
- Borrow-tracking claim is false: cpp.md:207-209 'nullptr means either the type doesn't match or the RefAny is already borrowed elsewhere' — downcast_ref/downcast_mut only check the type tag and call AzRefAny_getDataPtr with no borrow bookkeeping (azul17.hpp:7167-7180 member versions, :1137-1150 free functions).
- Guide-recommended callback style leaks: cpp.md:174-179 and :266 recommend free `azul::downcast_ref<T>(data)` 'so you don't have to wrap the parameter' — but the framework hands the callback an OWNED clone (layout/src/window.rs:4434 `(callback.cb)(data.clone(), callback_info)`), so never wrapping/deleting it leaks one strong ref per layout/click invocation. This contradicts cpp.md:267's own statement that 'the framework hands the callback an owned reference; the destructor decrements the refcount'. The shipped cpp17 example (examples/cpp/cpp17/hello-world.cpp:19,34) has the same leak.
- cpp.md:225 recommends a double-free: '*ok is an AzUrl; the Url wrapper would adopt it via Url(*ok)' — structured-binding get<I> copies the raw payload while the hidden Result wrapper retains ownership (common.rs:389-398) and ~ResultUrlUrlParseError deletes it (azul17.hpp:40391); adopting *ok into a Url wrapper frees the same payload twice.
- Unverifiable from repo: cpp.md:56-62 .deb/.rpm download URLs for release 0.2.0 (no packaging step found in rust.yml for these); cpp.md:234 'std::nullopt … will convert to AzOptionUrl when the codegen needs it' is misleading — model fields are never converted, they're just memcpy'd bytes inside the RefAny.


**Safety issues:**
- ALL dialects — RefAny destructor protocol violation = double free at last drop + leak per create: detail::type_destructor<T> does `delete static_cast<T*>(ptr)` (azul17.hpp:1123-1125, identical in azul11/14/20/23.hpp; azul03.hpp:58 macro version) on a buffer that Rust allocated (core/src/refany.rs:739-748 memcpys into its own alloc), and Rust then deallocs the SAME pointer again (core/src/refany.rs:240-244). The contract is destroy-in-place only — see Rust's own default_custom_destructor (refany.rs:619-637) and the C example's empty destructor (examples/c/hello-world.c:10). Additionally `T* heap = new T(...)` in RefAny::create (azul17.hpp:7151-7152, azul03.hpp:60) is never freed → sizeof(T) leak per create. Cross-CRT delete-vs-Rust-alloc is also formally UB and a real heap-corruption risk on Windows.
- azul17.hpp + azul03.hpp — 218 value-self methods emitted as `const` passing raw inner_ without release(): cpp17.rs:368-382 and cpp03.rs:301-316 detect self_is_value but pass `inner_` (a copy) and keep the method const, so after any `with_*`/`.dom()` call BOTH the original wrapper and the C-consumed value are 'owned' → AzDom_delete on already-consumed data at end of every builder-chain statement. Verified in today's fresh artifact: `inline Dom Dom::with_child(Dom child) const { return Dom(AzDom_withChild(inner_, child.release())); }` vs the correct azul11/14/20/23 emission `with_child(Dom child) { ...release(), ... }`. The 2026-06-01 value-self fix (present in cpp11.rs:209-238 and cpp20.rs:1035-1050) was never ported to cpp17.rs/cpp03.rs. The shipped cpp17 hello-world (lines 24-30) double-frees on every layout call; only cpp20 is e2e-tested (e2e_language_matrix.md:48).
- azul03.hpp:63-64 — AZ_REFLECT passes `sizeof(structName)` as BOTH len and align to AzRefAny_newC; any struct whose size is not a power of two (e.g. three uint32_t = 12 bytes) makes Layout::from_size_align fail → .expect panic/abort at refany.rs:736. azul.h already has an AZ_ALIGNOF macro that should be used instead.
- Option/Result → std conversions alias without ownership transfer: e.g. OptionString::toStdOptional() and its implicit `operator std::optional<AzString>()` (azul17.hpp OptionString class) copy the raw heap-owning payload while the wrapper retains ownership — using the std::optional after a temporary wrapper dies is use-after-free; re-adopting the payload into a wrapper is a double free. Same pattern in structured-binding get<I> (common.rs:389-398).
- Guide/example callback style never releases the framework's owned RefAny clone (layout/src/window.rs:4434) → unbounded refcount growth / model leak in long-running apps (examples/cpp/cpp17/hello-world.cpp:19, guide cpp.md:179).


**Idiomatic-ness issues:**
- Raw C surface leaks into normal user code: callback signatures must spell AzRefAny/AzCallbackInfo (unavoidable, documented), but enum VALUES also stay C-style (AzUpdate_RefreshDom, AzButtonType_Primary) even though 485 `using X = AzX;` type aliases exist (azul17.hpp:42182) — no azul::Update::RefreshDom.
- Naming mixes snake_case methods (with_child, create_body) with camelCase trait methods (partialEq, toDbgString, isSome, toStdOptional) in the same classes.
- Callbacks are raw C function pointers only; captureless lambdas would convert but this is nowhere documented — C++ users expect at least a documented lambda story; state must round-trip through RefAny + manual nullptr-checked downcast.
- Generated headers use CRLF line endings throughout (codegen emits \r\n) — noisy in unix editors/diffs.
- Two parallel styles (raw Az* C API vs RAII wrappers) are mixed within single examples (e.g. cpp17 hello-world uses AzRefAny_clone next to RefAny wrappers), teaching an inconsistent idiom.
- No pkg-config file, CMake config/find module, or Homebrew tap — bare -I/-L/rpath flags only; guide itself admits no package manager story outside deb/rpm.


**Ergonomics issues:**
- Hello-world is 45-60 lines vs the 34-line python reference — acceptable for C++, but the cpp20/cpp23 examples pad the minimal path with feature-demo cruft (count_zero_bytes span demo, homepage_ok expected demo) that obscures the counter pattern.
- Every callback needs the wrap-downcast-nullptr-check dance (4 lines) vs python's `data.counter += 1`; downcast failure is silent (returns nullptr) with no diagnostic of WHY (type mismatch has no message).
- The layout callback must return AzDom while building with azul::Dom — the &&-qualified operator makes `return Dom::create_body();` work but `.release()` vs move vs implicit-conversion rules take a full guide section to explain and are easy to get wrong (a copy = double free per the guide's own 'Common errors').
- Six dialect headers is a genuinely good idea but doubles the doc/test surface; only cpp20 is e2e-tested (e2e_language_matrix.md:48) and cpp17/cpp03 rotted — exactly the risk of the 6-header design.
- No README.md anywhere under examples/cpp/ (task brief assumed one exists; other languages have honest-status READMEs).
- Positive: single-header compile cost is fine (~1.5s syntax-only for the 2.5MB azul20.hpp), and implicit std::string/string_view/std::optional/std::span/std::expected interop is real and pleasant.


**Completeness:** Callbacks genuinely work on the cpp20 path (counter e2e verified green in prior sessions; framework ownership contract confirmed at layout/src/window.rs:4434), widgets are exposed (Button, Chip, Card, TabHeader, TreeView, NumberInput etc. + widgets.cpp example per dialect), and native conversions are the best of the bindings I'd expect: implicit std::string/std::string_view both directions, sv-literal overloads on all 498 String-taking methods, toStdOptional/toStdVector/toSpan/toStdExpected + structured bindings on Results. BUT 2 of the 6 downloadable dialect headers (azul03.hpp, azul17.hpp) are runtime-broken for all 218 value-self builder methods, and the RefAny destructor protocol double-frees at teardown in ALL dialects — so completeness is cpp20/11/14/23-real, cpp17/03-facade.


**Blockers to ship:**
- Install docs cannot produce a first window: api.json cppNN steps and guide download block omit azul.h, which every azulNN.hpp #includes (azul17.hpp:21) — documented compile step fails for 100% of fresh users on every platform.
- macOS documented run step fails: released dylib keeps its absolute build-path install name (no -install_name/@rpath in dll/build.rs, no install_name_tool in rust.yml; verified on examples/cpp/cpp20/libazul.dylib) so the documented -Wl,-rpath,@executable_path link produces a binary dyld cannot load on any other machine.
- azul17.hpp (the dialect the guide is written for) and azul03.hpp double-free on every builder chain: cpp17.rs:368-382 / cpp03.rs:301-316 missed the value-self release() fix that cpp11/cpp20 have — the downloadable cpp17 hello-world crashes/corrupts at runtime, and the frontpage advertises cpp03..cpp23 install variants.
- The guide's counter example does not compile (with_component_css, cpp.md:199) — fails the honest-docs bar even for working dialects.


**Quick wins (<1 day):**
- Add `curl -O $HOSTNAME/ui/release/$VERSION/azul.h` to all six cppNN api.json install entries and the guide download block (azul.h is already uploaded by rust.yml:1211).
- Port the value-self fix from cpp11.rs:209-238 into cpp17.rs:368-382 and cpp03.rs:301-316 (emit release()/memset-release, drop const), regenerate — one mechanical change fixes 218 double-free methods in two headers.
- Change detail::type_destructor<T> to `static_cast<T*>(ptr)->~T();` (placement destroy, no delete) in all dialect emitters + azul03 AZ_REFLECT macro, and make RefAny::create pass a stack value instead of a leaked `new T` — fixes the universal teardown double-free and the per-create leak.
- Fix azul03 AZ_REFLECT align argument: reuse azul.h's existing AZ_ALIGNOF instead of sizeof (azul03.hpp:64).
- Delete cpp.md:199's .with_component_css line (verified: snippet compiles clean without it) and rewrite the free-downcast advice to wrap the owned callback RefAny.
- Add `println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libazul.dylib")` for macOS in dll/build.rs (or install_name_tool in CI) so the documented rpath line actually works.
- Remove or 'not yet implemented'-flag the deducing-this (cpp.md:121-129) and modules (cpp.md:334-338) sections until real.
- Extend scripts/e2e_language_matrix.sh to run cpp17 and cpp03 hello-worlds in addition to cpp20 so dialect regressions can't hide behind syntax-only CI (rust.yml:906-911).


**Verdict:** Keep C++ on the frontpage but treat it as regressed: the cpp20 path and the std-interop design are genuinely strong, yet today a fresh user fails at compile (missing azul.h step), fails at launch on macOS (install-name), and the guide teaches a dialect (C++17) whose header double-frees every builder chain — about 2.5 focused days (two codegen fixes, one link flag, doc truth pass) restore it to shipped-solid.


## scala — candidate-near (install 2/5, ~2d to ship-quality)

There are no published install steps for Scala anywhere on the frontpage (api.json '0.2.0'.installation.languages has 31 entries, no 'scala'), so a fresh developer only finds examples/scala/README.md after cloning the repo. Following it, they must first build the Java example (mvn package in examples/java, which compiles 6,798 generated .java files from target/codegen/java), locate a JNA jar in ~/.m2, and hope the Homebrew-specific default paths in build.sh (scala 3.8.3 Cellar, openjdk@17 17.0.19) match their machine, then run scalac + java -XstartOnFirstThread with a 5-entry classpath and DYLD_LIBRARY_PATH. If everything lines up, HelloWorld.scala compiles (verified today with scalac 3.8.3, exit 0) and a counter window should appear, since it rides the same com.azul bytecode that passes the Java e2e. Note the committed example did NOT compile against current codegen until an uncommitted working-tree fix today (CallbackInvokerCallback -> ButtonOnClickCallbackInvokerCallback rename).


**Guide/install truthfulness issues:**
- api.json '0.2.0'.installation.languages has NO 'scala' key at all — zero frontpage install steps for this language (java is present; scala would piggyback on azul-java.zip but nothing documents that)
- examples/scala/README.md:47-53 'Gotchas': claims unqualified `String` resolves to com.azul.String and 'shadows java.lang.String', and that 'The `str` helper and main(args) qualify to java.lang.String explicitly' — STALE: the wrapper was renamed to AzulString (target/codegen/java has AzulString.java, no String.java), there is no `str` helper in the current example, and HelloWorld.scala:49 uses unqualified `String.valueOf` and compiles fine
- examples/scala/README.md:59 'libazul.dylib — symlink to ../java/libazul.dylib' — it is a regular 38.8MB file copy on disk, not a symlink (untracked, so harmless, but the claim is false)
- examples/scala/README.md:9 'Full GUI E2E — counter probe 5→8 via AZ_DEBUG verified' — unverifiable today and was false for the committed source until today's uncommitted diff: HEAD's HelloWorld.scala references AzulNativeManaged.CallbackInvokerCallback with 4-arg invoke for the button click, which no longer matches the widget-specific ButtonOnClickCallbackInvokerCallback registration path in current codegen
- examples/scala/README.md:57 '77-line port' vs actual 72 lines; HelloWorld.scala:6 comment 'Builds at ~50 LOC' vs actual 72 — trivial drift
- build.sh:19,26-27 hardcodes /opt/homebrew/Cellar/openjdk@17/17.0.19 and scala/3.8.3 jar paths as defaults — env-overridable but silently wrong on Linux or any other Homebrew version; README presents the same paths as the canonical direct invocation (README.md:27-29)


**Safety issues:**
- examples/scala/HelloWorld.scala:36-40 (writeDom): copies the AzDom struct bytes into outPtr (transferring ownership to libazul) but never marks the Dom wrapper consumed; the wrapper's GC finalizer (target/codegen/java/Dom.java:3110 -> close() -> AzDom_delete, Dom.java:3096-3101) can later delete DOM contents libazul now owns — latent double-free. The binding already has the safe path: AzulHostInvoker.registerLayoutCallback(LayoutCallback) calls result.__consume() after the splice — the example just bypasses it
- examples/scala/HelloWorld.scala:50-55: `new Dom(Button.create(...).onClick(...).dom().rawPointer())` creates DOUBLE ownership — the temporary Dom returned by .dom() still owns the pointer, becomes garbage immediately, and its finalizer AzDom_delete's a pointer the second wrapper (and later libazul) still uses: use-after-free/double-free window under GC pressure
- examples/scala/HelloWorld.scala:65-70: uses raw AzulNativeApp.AzApp_create/AzApp_run instead of the App wrapper; the WindowCreateOptions wrapper is never __consume()'d after its bytes are passed by value, so its finalizer (WindowCreateOptions.java:112-118 AzWindowCreateOptions_delete) can free window-state contents mid-run (the generated App.java run() correctly calls root_window.__consume() — again the example bypasses the safe wrapper)
- Structural: user code is instructed to live in `package com.azul` (HelloWorld.scala:15, README.md:49-53), which voids all package-private guardrails — the ownership-taking Dom(Pointer) constructor and rawPointer() become freely reachable, and the shipped example demonstrates exactly the misuse pattern users will copy
- Positive findings (no action): JNA callback premature-GC is correctly prevented via livePins (AzulHostInvoker.java:32 + livePins.add for every invoker), and refany host handles are released via AzApp_setHostHandleReleaser -> handles.remove (AzulHostInvoker.java:44-49), so no unbounded data-model leak


**Idiomatic-ness issues:**
- User code forced into `package com.azul` — un-idiomatic for Scala (and unlocks internal package-private API); a user-owned package works for the public API surface
- Example uses anonymous-class SAM instantiation (`new AzulNativeManaged.ButtonOnClickCallbackInvokerCallback { override def invoke... }`) where Scala 3 lambda SAM conversion works — verified compiling with plain lambdas
- No Scala-native layer: Java's toNullable returns a nullable box where Scala users expect Option[T]; no Try/Either mapping for AzResult; acceptable for a rides-on-Java binding but README's 'What's idiomatic' section (README.md:37-45) oversells this
- No scala-cli or sbt story — modern Scala onboarding is `scala-cli run .` with //> using directives; instead there is a bash script with hardcoded Homebrew Cellar paths
- Raw JNA plumbing (Pointer, Structure.newInstance, byte-array splicing) sits in the middle of the flagship example even though the binding's typed SAMs (AzulHostInvoker.LayoutCallback returning Dom) make it unnecessary


**Ergonomics issues:**
- Hello-world is 72 lines vs the ~35-40-line python/rust reference, and ~20 of those lines are Pointer/Structure/byte-splice plumbing the Java binding already abstracts away — a 44-line typed-SAM Scala port (LayoutCallback returns Dom directly, App wrapper with close()) compiles clean today, so the gap is example quality, not binding capability
- The click handler itself is good: refanyGet + pattern match + m.counter += 1 + RefreshDom reads naturally in Scala
- 5-entry runtime classpath (HelloWorld.jar : java classes : JNA : scala-library : scala3-library) plus DYLD_LIBRARY_PATH plus -Djna.library.path plus -XstartOnFirstThread is a lot of incantation with no packaging to hide it
- Depends on examples/java/target/classes existing — Scala users must run a Maven build of a different language's example first


**Completeness:** Near-complete by inheritance: Scala consumes the full Java JNA surface (6,798 generated classes incl. AzulNativeWidgets with Button/ListView/TreeView/Accordion etc., widget-specific typed callback invokers, Option.toNullable/AzulString conversions, AutoCloseable wrappers). Callbacks are real (button-click counter + layout callback wired through the host-invoker; Java path e2e-verified per scripts/e2e_language_matrix.md, Scala jar rebuilt today against current codegen), but the Scala counter e2e itself is unverified this session (libazul not runnable per constraints) and the committed example was compile-broken against current codegen until today's uncommitted fix. No automatic Scala Option/Seq conversions — Java-level toNullable/toString only.


**Blockers to ship:**
- No 'scala' entry in api.json '0.2.0'.installation.languages — frontpage install steps must be written (can largely mirror java's azul-java.zip steps + the scalac/classpath invocation)
- No doc/guide/en/hello-world/scala.md guide page (all 11 shipped languages have one)
- The compile fix in examples/scala/HelloWorld.scala (CallbackInvokerCallback -> ButtonOnClickCallbackInvokerCallback) is uncommitted working-tree state — HEAD's example does not compile against current codegen; must be committed
- Counter e2e (AZ_DEBUG 5->8 probe) must be re-run against a current libazul build to back the README claim — not verifiable this session


**Quick wins (<1 day):**
- Rewrite HelloWorld.scala to the typed-SAM form (AzulHostInvoker.LayoutCallback returning Dom + App wrapper + Button...dom() without re-wrapping rawPointer) — a 44-line version compiles clean today and eliminates all three double-free hazards plus ~20 lines of plumbing
- Delete the stale 'Gotchas' String-shadowing section and fix the symlink/line-count claims in examples/scala/README.md
- Move user code out of `package com.azul` in the example (works with the public API and closes the package-private footgun)
- Replace hardcoded Homebrew Cellar paths in build.sh with `cs fetch` (coursier) or `scala-cli` resolution so Linux/CI work out of the box
- Add the api.json installation.languages.scala entry reusing azul-java.zip (no new release artifact needed)


**Verdict:** Ship after ~2 focused days: the underlying Java binding is genuinely solid and Scala rides it with zero extra codegen, but today there are no install docs, no guide page, an uncommitted compile fix, and a flagship example that bypasses the binding's own safety layer with hand-rolled pointer splicing — rewrite it to the 44-line typed-SAM form (proven to compile), write honest docs, and re-verify the counter e2e.


## zig — candidate-near (install 2/5, ~2.5d to ship-quality)

A fresh developer runs the 4 curl commands from api.json and gets azul.h, azul.zig, build.zig, and the dylib — then `DYLD_LIBRARY_PATH=. zig build run` immediately fails because build.zig (target/codegen/build.zig:21) hard-codes `root_source_file = "hello-world.zig"`, a file no install step downloads and which is absent from the zig deploy list (doc/src/dllgen/deploy.rs:808-810). If they copy examples/zig/hello-world.zig from the repo, compilation fails at hello-world.zig:93: the example passes an `AzCallback` struct to `AzButton_setOnClick`, but the current header takes a bare `AzButtonOnClickCallbackType` fn pointer (binding drift already flagged in scripts/e2e_language_matrix.md:106). After a verified one-line fix (pass `onClick` directly), the example compiles clean under zig 0.16; runtime counter behavior could not be verified because libazul is not built in this session.


**Guide/install truthfulness issues:**
- api.json ['0.2.0']['installation']['languages']['zig'] (all 3 platforms): the 5 steps never download hello-world.zig, but the downloaded build.zig requires it (`b.path("hello-world.zig")`, target/codegen/build.zig:21) and deploy.rs ships no hello-world.zig for zig (doc/src/dllgen/deploy.rs:808-810, unlike pascal/perl/cobol at :790/:794/:767) — `zig build run` fails on step 5 as written
- examples/zig/README.md:8 '✅ Full GUI E2E — counter probe 5→8 verified' — stale: the current hello-world.zig does not even compile against the current azul.h (verified with zig 0.16 build-obj; error at hello-world.zig:93, AzButton_setOnClick signature drift)
- examples/zig/README.md:12 'Zig 0.11+' — false: build.zig uses the 0.13+ Module API and its own header says 'Tested against Zig 0.16 ... 0.16 dropped the legacy fields entirely'; `callconv(.c)` lowercase also requires 0.14+
- examples/zig/README.md:18 build command `zig build-exe hello-world.zig -lc -lazul -L. -rpath . -framework Foundation` — broken: it lacks `-I.`, and I verified @cImport fails with 'C import failed' when azul.h is not on the include path; it also contradicts hello-world.zig:1 which says 'zig build run'
- examples/zig/README.md:44 '133-line reference implementation' — file is 123 lines
- examples/zig/README.md:24-27 describes a pure-@cImport binding with 'no wrapper layer', but the shipped azul.zig is 41,758 lines, ~85% of which is an 'idiomatic wrapper' layer the README never mentions


**Safety issues:**
- Latent-compile-error minefield in the wrapper layer: Zig analyzes functions lazily, so the 40 type errors (below) ship silently and only explode on the user's first call; there is no refAllDecls-style compile gate for azul.zig anywhere in scripts/ or CI
- Use-after-consume is unguarded except in deinit: consuming builder methods set `self.consumed = true` (e.g. target/codegen/azul.zig:21876-21886, Button.with_button_type / Button.dom) but every other method on the stale wrapper still passes the Rust-moved bytes to C — a normal `btn.with_button_type(...); btn.dom()` misuse is UB with no diagnostic; only a follow-up deinit is protected (wrappers.rs:419-426)
- examples/zig/libazul.dylib install_name is the maintainer's absolute dev path `/Users/fschutt/Development/azul-mobile/target/release/deps/libazul.dylib` (otool -D), so the checked-in example dylib only resolves via DYLD_LIBRARY_PATH leaf-name fallback; generated build.zig sets no rpath (target/codegen/build.zig:29-32), leaving runtime loading dependent on DYLD_/LD_LIBRARY_PATH env vars


**Idiomatic-ness issues:**
- Wrapper methods are snake_case (`set_on_click`, `from_utf8`, `add_window`) — Zig convention (std lib, community) is camelCase for functions; cause: doc/src/codegen/v2/lang_zig/wrappers.rs:744-749 idiomatic_method_name passes api.json names through verbatim
- Wrapper layers do not compose: every wrapper method takes raw `C.Az*` params, not wrapper types — `Button.create(label: C.AzString)` cannot accept the `String` wrapper (target/codegen/azul.zig:21851, 2114), so users end up mixing both layers or ignoring wrappers entirely (the example ignores them)
- No native-type conversions: no `[]const u8` slice helpers on String (only ptr+len `from_utf8(ptr: *const u8, len: usize)`, azul.zig:16065), no Zig optionals for AzOption*, no error unions for Result-like types — everything stays C-shaped
- clone() and pure getters take `self: *Self` instead of `*const Self`/`Self` (wrappers.rs:382), so they cannot be called on const bindings
- No comptime RefAny helper: users hand-roll ~35 lines of type-id/upcast/downcast boilerplate (hello-world.zig:19-55) that a `fn RefAnyOf(comptime T: type)` generic could generate — Zig is uniquely good at this and the binding doesn't use it
- scripts/BINDING_STRATEGY_PER_LANGUAGE.md:73 still claims a 'file-as-module azul/app.zig, azul/dom.zig' layout; the actual artifact is one flat 41k-line file (acceptable for Zig, but the strategy doc is stale)


**Ergonomics issues:**
- hello-world is 123 lines vs the ~35-40 line python/rust reference; ~35 lines are RefAny reflection boilerplate and the rest is raw C calls with manual AzString_fromUtf8(ptr, len) for every literal
- The example uses only the raw `azul.C.*` layer, so the 41k-line wrapper layer has zero demonstrated usage; a user who follows the azul.zig header advice ('most users should prefer the idiomatic wrapper structs') hits the broken callback setters immediately
- String creation is 2 lines per literal (`const b = "x"; C.AzString_fromUtf8(b.ptr, b.len)`) — a 5-line `azul.str("x")` helper would halve visual noise
- install steps require env-var prefixed launch (`DYLD_LIBRARY_PATH=.` / `LD_LIBRARY_PATH=.`) because build.zig sets no rpath — one addRPath call removes a whole step


**Completeness:** Raw C layer is complete: 5482 generated wrapper fns, all widgets exposed (Button, TextInput, CheckBox, ListView, DropDown, TreeView, Ribbon, Tab, NumberInput, ColorInput, FileInput, Map), and Zig fn pointers with callconv(.c) make callbacks structurally sound without the host-invoker pattern (README claim verified true). But the counter e2e is currently compile-broken (setOnClick drift) and runtime-unverified. The "idiomatic wrapper" layer has 40 latent compile errors covering essentially EVERY widget callback setter — verified by a refAllDecls-style zig test: wrappers.rs map_arg_type (doc/src/codegen/v2/lang_zig/wrappers.rs:657-684) maps callback args to the callback STRUCT (`C.AzButtonOnClickCallback`) where the C fn takes the `...CallbackType` fn-pointer typedef (target/codegen/azul.zig:21861, 21865, 2178, 22112, +34 more), plus RefAny.get_data_ptr declares `*const void` where cImport returns `?*const anyopaque` (azul.zig:2939; primitive_to_zig maps c_void to "void" at wrappers.rs:733). No automatic String/Vec/Option conversion to native Zig types anywhere.


**Blockers to ship:**
- hello-world.zig does not compile against the shipped azul.h: examples/zig/hello-world.zig:92-93 wraps the fn pointer in AzCallback_create but AzButton_setOnClick takes AzButtonOnClickCallbackType (azul.h:43855); one-line fix verified to compile clean under zig 0.16, then counter e2e must be run against a built libazul
- Install steps are not honest/complete: hello-world.zig must be added to the zig deploy list (doc/src/dllgen/deploy.rs:808-810) and a download step added to api.json zig install steps, otherwise step 5 (`zig build run`) fails for every user
- Wrapper-layer callback setters (40 compile errors incl. Button.set_on_click — the exact counter path) must be fixed in wrappers.rs map_arg_type or the wrapper layer explicitly de-advertised; shipping a file whose own header steers users to broken code fails the honesty bar
- No doc/guide/en/hello-world/zig.md exists (11 shipped languages only) — the frontpage install entry deep-links to /guide/hello-world/<lang>, so the page must be written


**Quick wins (<1 day):**
- Fix examples/zig/hello-world.zig:92-93: delete the AzCallback_create call and pass `onClick` directly (verified compiling)
- Add `BindingFile { dst: "hello-world.zig", src: "zig/hello-world.zig", source: BindingSource::Examples }` to deploy.rs and a matching curl step to api.json zig install steps
- wrappers.rs: map callback-typedef args to `C.Az<Name>CallbackType` and change primitive_to_zig c_void to `anyopaque` (return position `?*const anyopaque`); regenerate
- Add a CI compile gate: `zig test refall.zig -I. -lc --test-no-exec -fno-emit-bin` with a comptime loop referencing every wrapper decl (catches all 40 latent errors today; script already prototyped in this review)
- build.zig: addRPath('.') so macOS/Linux steps drop the DYLD_/LD_LIBRARY_PATH prefix
- Rewrite examples/zig/README.md: min Zig 0.16, `zig build run` flow, remove stale build-exe command and the '133-line'/'0.11+'/'E2E verified' claims
- Emit a comptime `fn RefAnyOf(comptime T: type)` helper in azul.zig to replace the 35-line hand-rolled upcast/downcast block, bringing hello-world near the 40-line reference standard


**Verdict:** Do not ship today — the published install flow dead-ends (no hello-world.zig in the release) and the example itself no longer compiles — but this is the cheapest promotion on the board: a verified one-line example fix, a deploy-list entry, a wrappers.rs callback-type mapping fix, and a guide page get zig to frontpage quality in ~2.5 focused days, since the raw-C @cImport foundation is genuinely solid and callbacks need no host-invoker machinery.


## go — candidate-near (install 1/5, ~2.5d to ship-quality)

A fresh developer following the published macOS steps curls libazul.dylib, azul.h, the four generated .go files and go.mod successfully, then hits a wall: `go build` fails with ~40 type errors inside the downloaded wrappers.go (verified by compiling target/codegen/go with go 1.25.6), and even if it compiled, the steps never download or tell you to write a main program, so there is no `./hello-world` to run — the downloaded go.mod defines the library module `github.com/azul/azul-go`, and a user's `package main` cannot even live in the same directory as the `package azul` files. The only path that actually works today is the undocumented examples/go/main.go approach: 158 lines of raw cgo against azul.h that bypasses the generated binding entirely — that path is genuinely e2e-green (scripts/e2e_language_matrix.sh:545-569 builds and runs it; examples/go/hello-world-go-e2e was rebuilt today).


**Guide/install truthfulness issues:**
- api.json ['0.2.0'].installation.languages.go (all 3 platforms): final steps 'go build' then './hello-world' are false — no step downloads or creates a hello-world source file; the curled go.mod declares library module github.com/azul/azul-go, so `go build` builds (or rather fails to build) a library and produces no executable. './hello-world: no such file or directory' is guaranteed.
- api.json go steps: the downloaded wrappers.go does not compile at all — 40 type errors (verified: `go build` of target/codegen/go fails, e.g. wrappers.go:1085 'x.Close undefined (type *ColorU has no field or method Close)' and wrappers.go:22054 'cannot use on_click (struct type _Ctype_AzButtonOnClickCallback) as *[0]byte'). Every install path that includes wrappers.go is dead on arrival.
- api.json go linux step 'CGO_LDFLAGS="-L. -Wl,-rpath,$ORIGIN"' — $ORIGIN inside double quotes is expanded by the user's shell to empty; needs \$ORIGIN or single quotes to reach the linker.
- examples/go/README.md:18 'DYLD_LIBRARY_PATH=. go run main.go' — insufficient; main.go's cgo block only has '#cgo LDFLAGS: -lazul' with no -L, so the link fails without CGO_CFLAGS="-I." CGO_LDFLAGS="-L." (main.go:1's own comment and scripts/e2e_language_matrix.sh:554-557 both use the full flags; the e2e script additionally links AppKit/OpenGL frameworks).
- examples/go/README.md:40 'main.go — 165-line reference implementation' — it is 158 lines (trivial drift).
- wrappers.go:134-137 (and every generated Close doc): 'Safe to call more than once; subsequent calls are no-ops' — false; there is no closed-flag guard and Az*_delete is a bare drop_in_place (target/codegen/dll_api_internal.rs:104960), so a second Close() is a double-drop.
- target/codegen/go/azul.go header + doc/src/codegen/v2/lang_go/mod.rs:47-48 claim types.go 'drops the Az prefix' — enums in types.go keep it (AzStyleCursor_Alias, types.go:20-53), inconsistent with wrapper types (Dom, Button) that drop it.
- scripts/BINDING_STRATEGY_PER_LANGUAGE.md:72 'Go | sub-packages | azul/app, azul/dom' — stale/aspirational; reality is one flat 48k-line `package azul`.
- examples/go/go.mod requires github.com/azul/azul-go with `replace => ../azul-go` — that directory does not exist in the repo, and main.go never imports the package, so the require is dead weight that documents a layout no one has exercised.
- examples/go/README.md:7 'Full GUI E2E — counter probe 5→8 verified' — true, but only for the raw-cgo main.go path; it silently does not cover the generated binding files the install docs ship.


**Safety issues:**
- Generated package cannot compile — finalizers reference nonexistent destructors: doc/src/codegen/v2/lang_go/wrappers.rs:408-411 emits `runtime.SetFinalizer(ret, func(x *T){ x.Close() })` on every returns-self method WITHOUT checking has_delete (unlike emit_static_factory at :299-306 which does), so ColorU, ScrollIntoViewOptions, SvgRect, SvgVector get finalizers calling a Close() that is never emitted → hard compile errors at wrappers.go:1085 et al.
- Callback parameter type mismatch: doc/src/codegen/v2/lang_go/wrappers.rs:533-544 (map_arg_type) lowers callback-wrapper structs (ButtonOnClickCallback etc.) to `C.AzButtonOnClickCallback`, but the C ABI takes the raw fn-pointer typedef (azul.h:43855 `AzButton_setOnClick(..., AzButtonOnClickCallbackType)`), producing 36 compile errors across every widget callback setter (wrappers.go:22054 etc.). After fixing both bugs locally, the whole package compiles clean — so both are small, mechanical codegen fixes.
- Double-free invited by documentation: Close() has no guard flag (lang_go/wrappers.rs:229-238; artifact wrappers.go:138-145) yet its doc says calling twice is a no-op; AzDom_delete/AzApp_delete are `core::ptr::drop_in_place` (target/codegen/dll_api_internal.rs:104960), so `foo.Close(); defer foo.Close()` — the exact pattern the doc blesses — is a double-drop/use-after-free.
- Finalizer thread-affinity: runtime.SetFinalizer runs Az*_delete on the Go GC finalizer goroutine (arbitrary OS thread); *App/*Window-adjacent objects can be destroyed off the GUI thread by normal GC pressure (lang_go/wrappers.rs:299-306, 408-411).
- By-value consume double-free: methods like Dom.AddChild(child C.AzDom) (wrappers.go:25737) consume the raw C value; passing the same C.AzDom to two calls (natural when reusing a 'template' node) is a silent double-free with no wrapper-level protection — the finalizer-disarm consume logic (wrappers.rs:415-417) only covers self, not value arguments.


**Idiomatic-ness issues:**
- Public API exposes package-local cgo types — the cardinal cgo sin. Verified empirically: a consumer package's own C.AzString is rejected with 'cannot use s (variable of struct type _Ctype_struct_AzString) as azul._Ctype_AzString value in argument to azul.NewButtonCreate'. Since Go cgo types do not cross package boundaries, every wrapper function taking C.AzString/C.AzRefAny/C.Az*Callback* (248 C.AzString params in wrappers.go alone) is UNCALLABLE from user code. The generated 'idiomatic' layer is architecturally decorative unless users copy the files into their own package.
- No native-type boundary: no exported func taking Go `string`, `[]byte`, or `func` values anywhere; azul.go's goString/cString helpers (azul.go:50-63) are unexported and *C.char-based while the ABI uses AzString.
- Single flat 48k-line `package azul` instead of the sub-packages the strategy doc promised; enum constants keep the Az prefix (AzStyleCursor_Alias) while wrapper types drop it — two naming conventions in one package.
- `New` prefix misused for non-constructors: NewMsgBoxOk/NewMsgBoxInfo (wrappers.go:159-176) show a dialog and return nothing; NewFileDialogOpenFile returns a C option, not a *FileDialog.
- Close() error always returns nil — io.Closer implemented in letter but errors can never surface; no error-idiom mapping for Az Result/Option unions (returned raw as C unions).


**Ergonomics issues:**
- The only working hello-world is 158 lines of raw cgo (examples/go/main.go): manual C forward-declarations and cast shims for every callback (main.go:16-28), byte-slice + unsafe.Pointer gymnastics for every string (main.go:57-58,110-111,124-125), hand-rolled RTTI type-id via package-var address (main.go:49-50) — versus the ~35-40-line fluent-builder reference in examples/python and examples/rust.
- The fluent With* builder chain exists in wrappers.go (WithChild/WithClass/WithCssProperty) but cannot be used: params are cross-package-incompatible cgo types and wrapper structs hide `inner`, so a *Dom cannot even be passed as a child of another *Dom (AddChild takes C.AzDom, no accessor).
- Every callback registration requires the user to write their own C shim block with //export + fn-pointer cast helpers — boilerplate the binding could generate once (a handle-registry + trampoline pattern) but doesn't.
- cgo build tax: users need clang/gcc, azul.h on the include path, and correct CGO_CFLAGS/CGO_LDFLAGS invocations; cross-compiling is admitted to be 'genuinely painful' (azul.go:24-27) — inherent to cgo but worth honest framing on the frontpage.


**Completeness:** Two-track. Raw-cgo track (examples/go/main.go): fully working — layout callback, button on-click callback mutating `counter`, widgets (Button via C ABI), counter e2e 5→8 green via scripts/e2e_language_matrix.sh:545-569 (hello-world-go-e2e rebuilt today). Generated-binding track (target/codegen/go): does not compile (40 errors, 2 codegen bugs); after local fixes it compiles but is unusable from a consumer package because all parameters are package-local cgo types; wrappers cover App/Dom/Button/TextInput/ListView/etc. and register finalizers + consume logic (R9 claim verified at wrappers.go WithChild), but there is zero automatic string/vec/option conversion to native Go types and no Go-native callback support.


**Blockers to ship:**
- api.json install steps are false end-to-end: they never provide a hello-world source, `go build` of the downloaded files fails with 40 compile errors, and a user main package cannot share the directory with the `package azul` files — the steps must be rewritten around a flow that actually works (e.g. curl azul.h + libazul + a main.go modeled on examples/go/main.go, with correct CGO_CFLAGS/CGO_LDFLAGS).
- Shipped binding artifact does not compile: fix doc/src/codegen/v2/lang_go/wrappers.rs:408 (gate the returns-self finalizer on has_delete) and wrappers.rs:533-544 (lower callback-struct args to C.Az…CallbackType per azul.h:43855) — verified these two fixes take the 48k-line package to a clean `go build`. Either fix them or stop distributing the .go wrapper files.
- No doc/guide/en/hello-world page for Go — must be written before frontpage listing.


**Quick wins (<1 day):**
- Two one-liner-scale codegen fixes in lang_go/wrappers.rs (has_delete gate at :408; callback-typedef lowering at :533-544) make the entire generated package compile — verified locally.
- Rewrite the api.json go steps to the proven e2e recipe: add a `curl -O .../main.go` step, use CGO_CFLAGS="-I." CGO_LDFLAGS="-L." go build, and fix the Linux \$ORIGIN quoting.
- Fix the false 'safe to call more than once' Close() doc, or better, emit a `closed bool` guard in Close() (5-line codegen change) to make it true.
- Export a `func NewString(s string) *String` (and `func (s *String) String() string`) built on AzString_fromUtf8 — takes a native Go string so it IS callable cross-package, and removes the worst boilerplate.
- Fix examples/go/README.md build command (add CGO flags) and the 165-line claim; add a go row to the e2e_language_matrix.md table (the script has lang_go but the doc table omits it).
- Add a CI step that runs `go build` on target/codegen/go so the generated package can never regress to uncompilable again.


**Verdict:** Ship-able in ~2-3 focused days, but only as an honest "cgo against azul.h" story: the counter e2e is already green on the raw-cgo path, while the generated wrapper package has never compiled and — because Go cgo types don't cross package boundaries — can never be called as designed; fix the two codegen compile bugs, rewrite the install steps around the working main.go flow, and write the guide, deferring a truly idiomatic Go-native API (~5 more days) to a later release.


## pascal — candidate-near (install 2/5, ~3d to ship-quality)

A fresh FPC user who follows the published api.json steps downloads libazul.dylib, azul.pas and hello-world.pas, then runs `fpc -Mobjfpc -Sh hello-world.pas` — and it dies at the link step with `ld: symbol(s) not found` (verified with fpc 3.2.2 on this machine), because the generated unit has no `{$linklib azul}` and the documented command omits `-Fl. -k-lazul`. If they find the correct command hidden in the comment on line 1 of hello-world.pas, everything compiles cleanly in 1.8 s with zero errors (the whole 64,969-line unit + example, verified with -Cn). Whether the window then opens is unknown: the last recorded run (2026-05-13) crashed inside libazul's webrender in AzApp_run, a libazul-side bug that predates two months of macOS render fixes and has never been retested; the counter e2e has never been green for Pascal.


**Guide/install truthfulness issues:**
- api.json macOS/linux/windows step `fpc -Mobjfpc -Sh hello-world.pas` is broken as published: VERIFIED failure `ld: symbol(s) not found for architecture arm64` because target/codegen/azul.pas declares `external AzulLib` without any `{$linklib azul}` (nothing in doc/src/codegen/v2/lang_pascal/mod.rs:161-172 or lpi.rs emits it) and the command lacks `-Fl. -k-lazul`. The working command is the one in examples/pascal/hello-world.pas:1: `fpc -Mobjfpc -Sh -Fl. -k-L. -k-lazul hello-world.pas`.
- api.json description says "Download azul.pas, the Lazarus project, and the native library" but no step downloads azul.lpi/hello-world.lpi on any platform — the Lazarus project is never fetched.
- Both Lazarus project files are non-functional for linking: target/codegen/azul.lpi lists azul.pas twice as Unit0 and Unit1 (lpi.rs:47-55), uses Windows `PathDelim="\"` (lpi.rs:26), and neither it nor examples/pascal/hello-world.lpi contains any linker options for libazul — a Lazarus build hits the same unresolved-symbols failure.
- examples/pascal/README.md:3-8 claims "Blocked on libazul-side fix... AzApp_run crashes inside webrender SceneBuilder::build_item" — dated 2026-05-13 and unverifiable today; libazul has since had major macOS render reworks (GPU/CPU fallback, damage/present rewrite through 2026-07), so this claim is stale until retested.
- examples/pascal/README.md:24-29 "Files" section lists `azul.pas` and `libazul.dylib` as if shipped, but git tracks only README.md, hello-world.lpi and hello-world.pas (verified via git ls-files); those two are untracked local build leftovers.
- scripts/e2e_language_matrix.md:66,107 repeats the stale "AzApp_run access-violation" verdict from 2026-05-27 — it also states pascal was never verified for the counter scenario, which is the accurate part.


**Safety issues:**
- Handler-object leak on every DOM rebuild: azul_releaser_impl (target/codegen/azul.pas:42656-42666, emitted by doc/src/codegen/v2/lang_pascal/managed.rs:210-222) removes the handle-table entry but never calls Value.Free — Pascal has no GC, so every TAz*CallbackInvoker instance created per layout pass (hello-world.pas:78 creates TMyClickHandler.Create in the layout callback) leaks permanently once libazul releases the handle.
- Handle table has zero synchronization (managed.rs:174-222, azul.pas:42630-42666): azul_alloc_handle does SetLength on a plain dynamic array while azul_releaser_impl may run from another thread (ThreadCallback invokers are part of the surface, azul.pas:42947) — concurrent register/release corrupts the array. Fine single-threaded, real hazard once a user touches AzThread.
- Class-wrapper consuming-argument double-free: emit_method_impl (wrappers.rs:436-468) flips FOwned:=False only for by-value SELF; an owned by-value argument (e.g. passing otherWrapper.Raw into a consuming method) is consumed by Rust while the argument's wrapper keeps FOwned=True, so its destructor calls <Type>_delete on consumed bytes (wrappers.rs:282-290). Normal code path for anyone using the 463 T* class wrappers.
- Use-after-consume on self: after one by-value-self method call the wrapper is silently disarmed (wrappers.rs:466-468) but FRaw still holds consumed bytes; a second method call passes those stale bytes to libazul with no guard/exception — e.g. calling two builder methods on the same TDom wrapper.
- azul_lookup_handle is O(n) linear scan per callback dispatch (azul.pas:42648-42654) — perf cliff, not a crash, but every event goes through it.


**Idiomatic-ness issues:**
- Invoker Invoke signatures are raw and un-Pascal: `Invoke(id: cuint64; arg0, arg1: Pointer; out_ptr: Pointer)` forces users to cast PAzRefAny(arg0) and write `PAzUpdate(out_ptr)^ := ...` by hand (hello-world.pas:44-53); the per-kind classes exist precisely so these could be typed (PAzRefAny; PAzCallbackInfo; out Update).
- No native string conversion anywhere: zero AnsiString references in the 3.2 MB azul.pas; users must hand-roll MakeAzString with @s[1]/Length (hello-world.pas:36-42) and there is no AzString→AnsiString direction at all.
- Single flat 64,846-line unit vs the unit-per-module plan in scripts/BINDING_STRATEGY_PER_LANGUAGE.md:71 (`Azul.App.pas`, ...); compiles fast (1.8 s) so this is polish, not a blocker.
- No exception-based error handling: Result/Option types are returned as raw tagged records; no EAzulError, no nil-check helpers — acceptable for an FFI layer but below Delphi/FPC norms for the 'idiomatic' class layer.
- The 463 idiomatic class wrappers are a dead end for the main flow: methods accept and return raw TAz* records rather than wrapper classes (wrappers.rs:444-455 comment admits it), so hello-world uses none of them; the flat C-style API is the real surface.
- Generated header comment contains `{$mode objfpc}` inside a { } block comment (mod.rs:167-168), producing two 'Comment level 2 found' warnings on every user compile — cosmetic but looks broken on first contact.


**Ergonomics issues:**
- hello-world.pas is 123 lines vs the ~35-40-line python/rust reference: two class declarations + two Invoke overrides + a hand-written MakeAzString + explicit register calls + pointer casts replace what is a 3-line closure in the reference languages (Pascal has no closures-as-funcptr, but typed Invoke params and a bundled string helper would cut ~30 lines).
- Callback wiring is 3 steps per callback (subclass, instantiate, azul_register_* to get the cdata struct) and the returned TAzButtonOnClickCallback/TAzLayoutCallback must be threaded into builder calls manually.
- Builder chain is assignment-heavy (`label_wrap := AzDom_withChild(label_wrap, ...)`) because with* functions consume by value and return a new record — workable but reads awkwardly.
- Model access requires a manual nil + `is` check and typecast on every callback entry (hello-world.pas:47-50, 64-71).


**Completeness:** Full surface, runtime-unverified. 13,291 external functions, complete typed host-invoker plumbing for 21 callback kinds including all widget callbacks (Button/CheckBox/DropDown/ListView/TreeView/Tab/Ribbon/TextInput/NumberInput/ColorInput/FileInput/VirtualView/Thread/Layout) auto-registered at unit load; refany round-trip helpers present; example was freshly adapted to the typed-callback API (uncommitted diff) and the entire example+binding compiles clean with fpc 3.2.2 in 1.8 s, proving zero API drift. But the counter e2e has NEVER passed: the last real run (2026-05-13) crashed in AzApp_run libazul-side, and no auto String/Vec/Option conversion to native Pascal types exists (confirms auto_conversion_audit).


**Blockers to ship:**
- Counter e2e has never been green: last runtime attempt (2026-05-13) hit an EAccessViolation in AzApp_run (README-documented, diagnosed libazul-side). Must rebuild libazul and re-run — likely fixed by the June/July render reworks, but ship requires proof.
- Published install commands verifiably fail: `fpc -Mobjfpc -Sh hello-world.pas` cannot link (no {$linklib azul} in generated azul.pas, no -Fl./-k-lazul in api.json steps for all three platforms). Fix either the codegen header or the api.json commands.
- No doc/guide/en hello-world page for Pascal exists — must be written before frontpage listing.


**Quick wins (<1 day):**
- Emit `{$linklib azul}` after the unit directives in doc/src/codegen/v2/lang_pascal/mod.rs (~line 108) — FPC already passes `-L.` to ld (verified in ppaslink.sh), so the plain documented `fpc -Mobjfpc -Sh hello-world.pas` would then work with the dylib in cwd; alternatively update api.json steps to the known-good `fpc -Mobjfpc -Sh -Fl. -k-lazul hello-world.pas`.
- Free owned invoker objects in azul_releaser_impl (managed.rs:210-222): add an Owned:Boolean per handle-table entry, set True in azul_register_* / False in azul_refany_create, call Value.Free on release — closes the per-relayout leak in ~20 lines.
- Add `function AzStr(const s: AnsiString): TAzString;` and `function AzStrToPas(const s: TAzString): AnsiString;` helpers to the managed prelude — removes the hand-rolled MakeAzString from every user program.
- Escape the `{$mode objfpc}`/`{$PACKRECORDS C}` text in the generated header comment (mod.rs:167-168) to kill the two nested-comment warnings on every compile.
- Fix lpi.rs: dedupe the double azul.pas Unit entry (lpi.rs:47-55), use forward-slash PathDelim, add linker options; and add the .lpi curl step to api.json or drop 'the Lazarus project' from the description.
- Commit the pending typed-callback adaptation of examples/pascal/hello-world.pas (currently uncommitted working-tree diff) and refresh README.md's stale 2026-05-13 crash banner after retest.


**Verdict:** Closest-to-ship candidate of the non-shipped tier: the binding compiles end-to-end with zero drift and has full typed-callback plumbing, but do not list it until the 2026-05 AzApp_run crash is retested against current libazul and the verifiably-broken fpc link command in api.json is fixed; if the crash is gone (likely, given the July render fixes), ~3 focused days (linklib+docs, releaser leak, string helpers, guide page) gets it to frontpage quality.


## powershell — candidate-far (install 1/5, ~4d to ship-quality)

A fresh developer follows the published steps, downloads Azul.psd1/Azul.psm1 plus the native lib, and dies at step 4: `Import-Module ./Azul.psd1` throws "error CS8632: nullable reference annotation..." on every platform, because Add-Type treats the C# embed's `object?` annotations as fatal (verified live on pwsh 7.5/macOS; the Windows steps use `powershell` 5.1 where the embed cannot compile at all — NativeLibrary/ModuleInitializer are .NET-Core-only). Even after patching in `-IgnoreWarnings` (verified: full module then imports in ~8s with 1975 exported functions), the committed hello-world.ps1 cannot run: it passes a raw ScriptBlock to `RegisterCallback(Delegate)` (verified: ScriptBlock→System.Delegate conversion fails), and passes raw `AzRefAny`/`AzAppConfig`/`AzWindowCreateOptions` structs to wrapper methods whose parameter types (`Azul.RefAny`, `Azul.AppConfig`, `Azul.WindowCreateOptions`) have internal-only constructors, so no coercion is possible. Also, examples/powershell/ contains Azul.psd1 but not Azul.psm1, so the example's own `Import-Module` fails with module-not-found before any of that. No one has ever seen the counter window from PowerShell on any platform.


**Guide/install truthfulness issues:**
- README claim (examples/powershell/README.md:11-13): Windows codegen module is 'untested but mechanically should work since the C# binding is verified PASS' — false: Import-Module of target/codegen/Azul.psd1 fails on pwsh 7 with fatal CS8632 (verified live), and hello-world.ps1 calls signatures that don't exist in the current binding (Dom.CreateText now takes System.String, Button.WithOnClick takes Azul.RefAny + Azul.ButtonOnClickCallback wrappers with internal ctors, App.Create takes Azul.RefAny+Azul.AppConfig — the script passes raw Az* structs and ScriptBlocks).
- README (examples/powershell/README.md:38-39): 'Windows PowerShell 5.x also works but Add-Type semantics differ slightly' — false: the embedded C# uses System.Runtime.InteropServices.NativeLibrary (Azul.psm1:49898-49913) and [ModuleInitializer] (Azul.psm1:49896), both .NET Core 3+/.NET 5+ only; 5.1's .NET Framework Add-Type can never compile it. Azul.psd1:14 'PowerShellVersion = 5.1' repeats the false claim.
- README Files section (examples/powershell/README.md:70-72) lists 'libazul.dylib / azul.dll — prebuilt native library' as present — the directory contains only Azul.psd1, hello-world.ps1, README.md; crucially Azul.psm1 (the RootModule that Azul.psd1:7 points at) is also missing, so hello-world.ps1:6 Import-Module fails immediately.
- api.json ['0.2.0'].installation.languages.powershell publishes full macOS install steps while README.md:3-10 says 'macOS: blocked (won't pursue) — pwsh REPL holds the Cocoa main thread'; publishing those steps is dishonest.
- api.json Windows steps run `powershell -ExecutionPolicy Bypass` (Windows PowerShell 5.1) — guaranteed compile failure of the embed; must be `pwsh`.
- api.json step 4 (`pwsh -NoProfile -Command "Import-Module ./Azul.psd1; Set-AzulLibraryPath ..."`) runs in a throwaway process whose env vars die before step 5 — the step accomplishes nothing persistent.
- api.json step 5 runs `pwsh ./hello-world.ps1` but no prior step downloads hello-world.ps1.
- Azul.psm1 header example (target/codegen/Azul.psm1:14-15, from doc/src/codegen/v2/lang_powershell/mod.rs:108-110): '$app = New-AzulApp -Data $data -Config $cfg' — false twice: the parameters are -InitialData/-AppConfig, and that definition is silently shadowed by the later zero-arg App.default definition (verified: Get-Command New-AzulApp shows no user parameters).
- Set-AzulLibraryPath non-Windows branch (mod.rs:196-203, Azul.psm1:188245-188248) sets LD_LIBRARY_PATH/DYLD_LIBRARY_PATH at runtime — the loader reads these at process start (and SIP strips DYLD_* anyway), so it is a no-op for the current process; the thing that actually works is the DllImport resolver probing CWD/AppContext.BaseDirectory (Azul.psm1:49901-49913). The install steps and error text lean on this placebo function.


**Safety issues:**
- Runspace/thread affinity: callbacks are PowerShell scriptblocks converted to delegates and invoked via fn.DynamicInvoke from native code (Azul.psm1:186340ff). Invocations from non-pipeline threads (Thread/Timer callbacks, e.g. New-AzulThread at Azul.psm1:189412) will throw 'no default runspace'; the generated invokers catch Exception and write to stderr, so the app silently drops callbacks rather than crashing — a footgun users will hit with any timer/thread API, and there is no documented guidance.
- Consume-by-value double-free hazard surfaced by the example pattern: hello-world.ps1:89-110 extracts $wco.Raw (an AzWindowCreateOptions struct copy), mutates it, and would pass it to native; PowerShell's unbox-on-field-access struct-copy semantics make it easy to keep and reuse a raw struct after the native side consumed it (double-consume of window_state strings). The C# wrapper classes guard this with _disposed checks, but the example teaches users to bypass the wrappers via .Raw.
- Positive: the memory-management core is the verified C# host-invoker layer embedded verbatim — RefAny host-handle table with a native releaser removing entries (Azul.psm1:186331-186337), delegates pinned in _livePins, IDisposable wrappers with finalizers and ObjectDisposedException guards (Azul.psm1:91244ff). No double-free/UAF found in the generated layer itself.


**Idiomatic-ness issues:**
- 74 duplicate function names silently collapse last-wins because pick_noun (doc/src/codegen/v2/lang_powershell/cmdlets.rs:347-352) names every constructor just New-Azul<Type>: New-AzulDom is defined 191 times (only the last ctor survives), New-AzulApp's create(data,config) is shadowed by App.default() (verified via Get-Command), New-AzulButton, New-AzulAppConfig, New-AzulWindowCreateOptions all collapse. The 'idiomatic' Verb-Noun layer is effectively unusable for constructors.
- Self-argument leak for multi-word types: cmdlets.rs:172-177 filters the self arg by comparing the snake_case IR arg name (e.g. 'icon_provider_handle') to the lowercased class name ('iconproviderhandle'); mismatch leaks self as a mandatory [IntPtr] parameter AND passes it as an extra argument — e.g. Set-AzulIconProviderHandleSetResolver calls $Instance.SetResolver($IconProviderHandle, $Resolver) but the C# method is SetResolver(resolver) (Azul.psm1:91273) → runtime MethodException for every instance method of every multi-word class.
- Copy-* shims are generated (pick_verb maps clone→'Copy', cmdlets.rs:314) but 'Copy-Azul*' is absent from both Export-ModuleMember (cmdlets.rs:92-105) and Azul.psd1 FunctionsToExport — dead unexported code; those shims are also broken (Copy-AzulAppClone passes a spurious [IntPtr]$InstanceArg).
- The flagship example doesn't use the Verb-Noun layer at all — hello-world.ps1 drives raw [Azul.*] static classes, GCHandle pinning, and reflection pokes into FFI structs ($ws.layout_callback = ...), i.e. C# written in PowerShell syntax.
- Shim parameters are pointer/raw-struct typed ([IntPtr], [Azul.AzRefAny]) rather than accepting pipeline-friendly PowerShell values; psd1 uses wildcard FunctionsToExport, against PS Gallery best practice.


**Ergonomics issues:**
- hello-world.ps1 is 111 lines vs the ~35-40-line Python/Rust reference: 13 Write-Host debug lines, a 14-line Convert-AzulString GCHandle helper that is now dead weight (Dom.CreateText/Button.Create take System.String directly since Phase I auto-conversion), and reflection-based struct mutation to install the layout callback instead of a constructor that accepts one.
- First import costs a full Roslyn compile of an 8.6MB C# embed (~6-8s measured on M-series; header claims '~1s'), paid once per pwsh process.
- The typed convenience API that would enable a clean counter example exists in the embed (RegisterCallback[T](CallbackWithData<T>), RegisterLayoutCallback[T]) but neither the example nor any shim uses it.
- Azul.psm1 is 9.9MB/237k lines; PowerShell parses all 2804 shim functions at import.


**Completeness:** Below smoke: the shipped module cannot be imported (fatal CS8632 under Add-Type), so nothing has ever executed. With a 1-line fix the full C# surface compiles and 1975 functions export — widgets, typed callback registration, string auto-conversion (CreateText(System.String)), and the verified C# host-invoker/RefAny plumbing are all present in the embed — but the counter e2e has never run in PowerShell on any platform (scripts/e2e_language_matrix.md: SKIP, Windows-only), the committed example targets a stale API surface and fails at multiple call sites, and the Verb-Noun shim layer is broken for constructors (191-way New-AzulDom collapse) and multi-word instance methods.


**Blockers to ship:**
- Azul.psm1 fails Import-Module on every platform: Add-Type treats CS8632 (object? without #nullable context) as fatal — add -IgnoreWarnings (or emit '#nullable enable') in generate_addtype_block, doc/src/codegen/v2/lang_powershell/mod.rs:152-159 (fix verified live: with -IgnoreWarnings the module imports cleanly).
- hello-world.ps1 must be rewritten against the current C# surface (typed RegisterCallback[T]/RegisterLayoutCallback[T], wrapper-class arguments instead of raw Az* structs and ScriptBlock→Delegate casts) and the counter e2e actually run once on Windows — the only supported platform; today it fails at lines 64, 66, 108, 110.
- examples/powershell/ ships Azul.psd1 whose RootModule Azul.psm1 is not in the directory — the example cannot even import; ship the psm1 (or document copying it).
- api.json install steps are false as published: Windows steps invoke `powershell` 5.1 which can never compile the embed (must be pwsh 7+); macOS steps are published for a platform the README declares blocked; hello-world.ps1 is never downloaded. psd1 PowerShellVersion='5.1' claim must change to '7.0'.
- No doc/guide/en hello-world page exists — must be written.
- Windows runtime verification requires a Windows machine/runner (macOS is hard-blocked by the pwsh CFRunLoop/NSApp conflict, per README and powershell_macos_eventloop memory).


**Quick wins (<1 day):**
- One-line codegen fix: append -IgnoreWarnings -WarningAction SilentlyContinue to the Add-Type call in mod.rs generate_addtype_block (verified working).
- Disambiguate constructor nouns in pick_noun (cmdlets.rs:347-352) — e.g. New-AzulDomText, New-AzulDomBody — eliminating all 74 last-wins collisions.
- Fix the self-arg filter (cmdlets.rs:172-177) to compare against the snake_case form of the class name, un-breaking every multi-word-class instance shim.
- Add 'Copy-Azul*' to the export lists or fold clone into an exported verb.
- Correct api.json: use pwsh on Windows, drop or blocked-flag the macOS section, add a curl step for hello-world.ps1; fix psd1 PowerShellVersion and the README 5.x claim.
- Strip Write-Host debug spam and the obsolete Convert-AzulString helper from hello-world.ps1; use CreateText(string) and RegisterCallback[T] to get it near 40 lines.
- Make Set-AzulLibraryPath honest: on non-Windows have it feed a directory list the DllImport resolver actually consults, instead of setting loader env vars post-launch.


**Verdict:** No-ship today: the shipped module cannot even be imported (fatal CS8632 under Add-Type) and the example targets a stale API, so the counter e2e has never run in PowerShell anywhere. The bones are unusually good — it embeds the verified C# binding verbatim — so ~4 focused days (1-line import fix, example rewrite, shim de-duplication, truthful Windows-only docs + guide, one real Windows e2e run) gets it to frontpage quality as a Windows-only listing.


## lisp (Common Lisp / SBCL, CFFI) — candidate-far (install 1/5, ~5d to ship-quality)

A fresh developer curls libazul.dylib, azul.lisp and azul.asd (all three genuinely exist on the release channel) and runs the documented `DYLD_LIBRARY_PATH=. sbcl --script hello-world.lisp` — which fails three ways at once: hello-world.lisp was never downloaded (no step provides it, no guide page exists), `sbcl --script` skips ~/.sbclrc so Quicklisp/CFFI are not findable, and even a savvy user who quickloads :cffi hits 'Unable to call structures by value without cffi-libffi loaded' at the first defcfun because azul.asd only depends on #:cffi (verified experimentally). If they discover examples/lisp/hello-world.lisp and its real invocation line, they get a 110-line program full of cffi:foreign-slot-offset surgery, and on macOS App.Run is still blocked by the SBCL/NSApplication main-thread conflict per the README. Nobody gets a window today.


**Guide/install truthfulness issues:**
- api.json lisp steps (all 3 platforms), step 4: 'LD_LIBRARY_PATH=. sbcl --script hello-world.lisp' — hello-world.lisp is never downloaded in steps 1-3 and is not on the release channel (doc/target/deploy/ui/release/0.2.0/ has no lisp hello-world); additionally `sbcl --script` skips ~/.sbclrc, so Quicklisp/ASDF cannot find CFFI and the command fails even with the file present.
- api.json steps omit the hard prerequisite entirely: cffi-libffi (plus system libffi, pkg-config, and a C compiler to grovel). Verified with SBCL: loading a by-value defcfun with plain :cffi errors at macroexpansion — 'Unable to call structures by value without cffi-libffi loaded'. azul.lisp is wall-to-wall by-value structs (e.g. target/codegen/azul.lisp:34741 AzDom_createText takes (:struct az-string)).
- target/codegen/azul.asd:5-6,15 claims 'The single dependency is CFFI' and declares :depends-on (#:cffi) — false; #:cffi-libffi is mandatory (codegen source: doc/src/codegen/v2/lang_lisp/asd.rs:30).
- examples/lisp/azul-example.asd:7-8 tells users to '(ql:quickload :azul-example)' then '(azul-hello:run-app)' — hello-world.lisp defines no azul-hello package and no run-app function; it executes app-run at load time and manually (load "azul.lisp"), conflicting with the ASDF system it is supposedly part of.
- examples/lisp/README.md:3 'Codegen is correct' — overstated: the generated wrapper layer has at least two runtime-fatal bugs (make-window-create-options-create passes an AzLayoutCallback struct plist to a :pointer parameter, azul.lisp:103127 vs :32626; layout-invoker memcpy treats a struct plist as :pointer, azul.lisp:80326). The tag-width fix commit 744f1e90c it cites does exist.
- api.json description 'load via SBCL/CCL/etc.' — CCL path unverifiable and the shipped example is SBCL-only (sb-ext:string-to-octets at examples/lisp/hello-world.lisp:21).


**Safety issues:**
- Every close-* destructor on a by-value-constructed class is broken: constructors store the CFFI by-value return (a plist — experimentally verified CFFI/libffi returns CONS, not a pointer) in the `ptr` slot, then close-dom calls (cffi:null-pointer-p plist) → TYPE-ERROR, and %az-dom-delete expects :pointer anyway. doc/src/codegen/v2/lang_lisp/wrappers.rs:149-157; target/codegen/azul.lisp:99265-99268. Net effect: there is NO working deallocation path, so every wrapper not consumed by a native call leaks its Rust-side heap allocation; there are also zero GC finalizers (no trivial-garbage anywhere in the 109k-line binding).
- LayoutCallback/VirtualViewCallback invokers do (cffi:foreign-funcall "memcpy" :pointer out :pointer (dom-ptr ret) ...) where dom-ptr holds a plist → type error, swallowed by the surrounding handler-case, leaving the `out` AzDom uninitialized — libazul then reads garbage stack memory as a DOM (UB/crash). managed.rs:209; azul.lisp:80316-80329. This alone makes the counter e2e impossible with the generated code.
- Callback handle table *azul-handles* is a plain non-:synchronized hash table and the id allocator is a bare incf; ThreadCallback invokers run on background threads → concurrent gethash/setf/remhash corrupts SBCL hash tables. managed.rs:85,93; azul.lisp:80115-80129.
- Consuming by-value self semantics unguarded: e.g. dom-with-child consumes both DOMs natively, but the old CLOS instances keep stale plists containing the moved-out interior pointers; re-passing one to any native call is use-after-free. wrappers.rs:282 (call_args[0] = (class-ptr obj) with no consumed marking).
- All user-callback conditions are caught and merely printed ([azul] Callback error), with the out-param unwritten — a Lisp error in a click handler silently hands libazul an uninitialized AzUpdate/AzDom instead of failing cleanly. azul.lisp:80131-80140.


**Idiomatic-ness issues:**
- Flat namespace: one giant :azul package with thousands of (export ...) calls, vs the project's own plan of :azul/app, :azul/dom subpackages (scripts/BINDING_STRATEGY_PER_LANGUAGE.md:76).
- No GC finalizer story (trivial-garbage is the CL norm for FFI handles); only manual close-* + with-* — and both are currently broken for by-value types.
- The `ptr` slot/accessor name is a lie for most classes — it holds a CFFI struct plist, not a pointer; users must understand CFFI's by-value plist representation to use the binding at all.
- Errors are printed to *error-output* instead of signaling proper CL conditions; no condition type hierarchy exists.
- register-callback is stringly-typed ((register-callback "LayoutCallback" fn)) rather than keyword/generic-function dispatch; unknown strings error at runtime (azul.lisp:80425).
- Payload-bearing tagged-union variants are SKIPPED with a comment telling users to foreign-alloc and poke slots by hand (wrappers.rs:434-439).
- Constructor names like make-window-create-options-create / make-dom-create-text double up the constructor verb; kebab-case itself is fine.


**Ergonomics issues:**
- hello-world is 110 lines vs the ~35-40-line python/rust reference, and ~40 of those lines are cffi:foreign-slot-offset pointer surgery to install the layout callback — needed only because make-window-create-options-create is broken (azul.lisp:103127 passes a struct plist to the :pointer param of azul.lisp:32626; C signature at azul.h:42777 takes a raw AzLayoutCallbackType fn ptr).
- No Lisp string → AzString conversion anywhere: users write an 11-line byte-buffer helper just to make a string (examples/lisp/hello-world.lisp:18-28); no Vec or Option conversion either.
- Callback arguments arrive as raw :pointer arg0/arg1 with no typed wrappers; refany-get is the only convenience.
- API is level-inconsistent: some wrappers take CLOS instances (obj), others require the raw struct plist (app-run root-window), forcing users to constantly unwrap with -ptr accessors.
- Single 4.3MB azul.lisp compiles slowly on every load unless ASDF FASL-cached; the example's own header demands --dynamic-space-size 8192.


**Completeness:** Smoke-only. The host-invoker infrastructure is genuinely complete on paper — 21 callback kinds registered incl. all widget callbacks (button/checkbox/dropdown/listview/ribbon/treeview, azul.lisp:80361-80389) and RefAny round-trip is verified — but the counter e2e cannot pass: the layout-callback Dom-return memcpy path type-errors on plists, WindowCreateOptions creation is broken, and macOS App.Run is blocked by the SBCL/NSApp main-thread conflict (README). No automatic String/Vec/Option conversion (make-dom-create-text requires a hand-built AzString struct).


**Blockers to ship:**
- Binding cannot load as documented: azul.asd lacks the #:cffi-libffi dependency and api.json steps never mention libffi/quicklisp — first defcfun macroexpansion fails with plain :cffi (verified). Fix asd.rs:30 + api.json steps.
- Layout-callback Dom return is broken in generated code: invoker memcpy treats the dom's struct plist as a :pointer (managed.rs:209, azul.lisp:80326) — no window content can ever render, so the counter e2e bar is unreachable.
- No usable path to install a layout callback: make-window-create-options-create passes an AzLayoutCallback plist into a :pointer param for a C fn that wants a raw fn ptr (azul.lisp:103127/32626, azul.h:42777); only workaround is the 40-line foreign-slot-offset surgery in hello-world.
- api.json step 4 references hello-world.lisp that no step downloads and that is absent from the release channel; no doc/guide page exists for lisp — install docs are not honest today.
- Counter e2e must actually pass on at least one platform: macOS is additionally blocked by the libazul-side SBCL/NSApplication main-thread conflict (examples/lisp/README.md:3-7, same Phase C item as Pascal/PowerShell); Linux is untested.


**Quick wins (<1 day):**
- Add #:cffi-libffi to azul.asd (asd.rs:30) and prepend api.json steps with quicklisp + `brew install libffi pkg-config` / apt equivalents; replace `sbcl --script` with the working invocation already documented in hello-world.lisp line 1.
- Fix the invoker Dom/VirtualViewReturn write path in managed.rs: replace the memcpy with (setf (cffi:mem-ref out '(:struct az-dom)) ret-plist) — CFFI writes plists into foreign memory natively (verified); one-line codegen change per callback kind.
- Make *azul-handles* (make-hash-table :test 'eql :synchronized t) and guard the id incf (managed.rs:85,93).
- Type-check in close-*: only call %az-*-delete when (cffi:pointerp p), eliminating the guaranteed TYPE-ERROR (wrappers.rs:151-156).
- Emit the WCO-default + layout-callback slot-poke sequence (currently hand-written in hello-world.lisp:84-109) as the body of make-window-create-options-create so users get a one-call constructor.
- Add a (lisp-string->az-string "foo") helper to codegen (the hello-world az-str helper, generalized) — removes 15 lines from every user program.
- Fix azul-example.asd to match reality (package azul-hello with run-app, or delete the .asd) and publish hello-world.lisp to the release channel.


**Verdict:** No-ship today: the install steps are un-followable (missing cffi-libffi dependency, phantom hello-world.lisp) and the generated wrapper layer has never executed a full GUI path — the layout-callback return and WindowCreateOptions creation both type-error on first use. The host-invoker plumbing underneath is solid, so ~5 focused days (codegen fixes above + Linux counter e2e + guide page) gets it to the frontpage bar, with macOS additionally gated on the external SBCL/NSApp event-loop item.


## perl — candidate-far (install 2/5, ~6d to ship-quality)

A fresh developer follows the macOS steps: four curls succeed, `cpanm --installdeps .` installs FFI::Platypus/CheckLib cleanly, then the final command `DYLD_LIBRARY_PATH=. perl -Ilib hello-world.pl` dies with "Can't locate Azul.pm in @INC" — Azul.pm was downloaded to the current directory, but hello-world.pl (examples/perl/hello-world.pl:6) does `use lib "$Bin/lib"` and perl 5.26+ removed '.' from @INC, and no step creates lib/. If they hand-fix the layout (mkdir lib; mv Azul.pm lib/; AZ_LIB_DIR=$PWD because CheckLib ignores DYLD_LIBRARY_PATH), they get a console-only smoke test that prints AzString/RefAny round-trip messages and then tells them itself that "Full App.run wiring requires layout / callback closures". No window ever opens; there is no way to open one with the published surface.


**Guide/install truthfulness issues:**
- api.json ['0.2.0'].installation.languages.perl (all 3 platforms): final step `perl -Ilib hello-world.pl` fails — the steps download Azul.pm into cwd but never create lib/ or move it there; hello-world.pl expects $Bin/lib/Azul.pm and perl>=5.26 has no '.' in @INC, so the run dies 'Can't locate Azul.pm'.
- api.json macos description: 'run with libazul.dylib on DYLD_LIBRARY_PATH' — false mechanism. Generated Azul.pm resolves the library via FFI::CheckLib::find_lib_or_die with libpath=[AZ_LIB_DIR, dirname(Azul.pm)] (doc/src/codegen/v2/lang_perl/mod.rs:148-161); FFI::CheckLib 0.31 only consults FFI_CHECKLIB_PATH (CheckLib.pm:206), never DYLD_LIBRARY_PATH. Same for the linux LD_LIBRARY_PATH claim. It only works if the dylib sits next to Azul.pm — which contradicts the lib/ layout the run command implies.
- examples/perl/README.md:3-8: 'invoker drops out_ptr from the user sub and declares the Platypus closure return as void' — half stale. Since the B.5.1 fix, managed.rs:189-207 DOES pass out_ptr as the trailing user-sub arg (see generated Azul.pm:33194-33201 LayoutCallback invoker calling $sub->($_[1],$_[2],$_[3])). The still-true part (struct returns need a record-to-pointer memcpy primitive) is only in a code comment, not the README.
- examples/perl/README.md:3 'Host-invoker smoke layer works' — unverifiable today (last verified 2026-05 per scripts/e2e_language_matrix.md:63; could not re-run under review constraints); plausible but should be re-proven against current master before shipping.
- examples/perl/cpanfile:9-10: 'Generated Azul.pm lives under target/codegen/v2/perl/lib/Azul.pm' — actual artifact path is target/codegen/Azul.pm.
- examples/perl/README.md:28 'libazul.dylib — prebuilt native library': a 36MB arm64 dylib is checked into examples/perl/ with no provenance/version note; it may be stale vs the shipped release.


**Safety issues:**
- GC-time garbage-pointer free in ALL 463 wrapper classes: constructors are attached as record-by-value returns (e.g. Azul.pm:21948 `AzApp_create => ... => 'AzApp'`), the wrapper blesses that FFI::Platypus::Record object as if it were a pointer (doc/src/codegen/v2/lang_perl/wrappers.rs:97-103), and DESTROY passes it to `Az*_delete` declared `['opaque']` (wrappers.rs:117-127; Azul.pm:34046, 29334). Empirically verified with a libc-only probe: Platypus 'opaque' silently numifies a blessed ref to the referent SV's address — no croak. So `my $s = Azul::String->from_utf8(...)` going out of scope calls AzString_delete on a pointer into the Perl heap → heap corruption/segfault in completely normal code.
- Same record/opaque incoherence at call time: instance methods pass $$self (a Record object) as an 'opaque' self arg — e.g. Azul.pm:34066 AzApp_run($$self, ...) with attach ['opaque','AzWindowCreateOptions'] (Azul.pm:21951) → wrong pointer passed to C even before destruction.
- Owned (by-value) non-self arguments are never consumed: wrappers.rs:324-329 unwrap_expr passes $arg->ptr but only self-by-value gets `$$self = undef` (wrappers.rs:219-229). E.g. `$dom->add_child($child)` (Azul.pm:19623 takes AzDom by value, Azul.pm:55794) — C takes ownership, then $child's DESTROY deletes again → double free.
- Callback exceptions are swallowed with `warn` and the out-pointer is never written (managed.rs:200-207, Azul.pm:33198-33200): a `die` inside a layout callback returns with the AzStyledDom out-buffer uninitialized, which libazul then reads → UB. Even without exceptions, nothing writes the struct return — the user sub receives a raw out_ptr and is expected to memcpy bytes itself.
- examples/perl/hello-world.pl:14 teaches `my $ptr = unpack('J', pack('P', $src))` — a raw-pointer-to-Perl-buffer hack with no lifetime guarantee; the flagship example normalizes an unsafe pattern because there is no string auto-conversion.


**Idiomatic-ness issues:**
- Duplicate method collision: the `new`→`create` rename (wrappers.rs:342-351) collides with existing `*_create` C factories, producing two `sub create` in Azul::Accordion (Azul.pm:54107 vs 54112) and Azul::ComboBox (Azul.pm:57107 vs 57112); the arg-taking factory is silently shadowed and every `use Azul` prints 'Subroutine create redefined' warnings (verified with perl -c).
- register_callback('LayoutCallback', $sub) is stringly-typed kind dispatch through a giant if/elsif chain (managed.rs:141-158) — unPerlish; per-class `->with_callback(sub {...})` style would be idiomatic.
- No POD documentation anywhere in the 58,741-line generated Azul.pm — `perldoc Azul` yields nothing; CPAN culture expects POD.
- Hardcoded `our $VERSION = '0.1.0'` (mod.rs:137) while the release channel is 0.2.0.
- Error handling is inconsistent: register_callback dies on unknown kind, refany_get returns undef, callback errors warn-and-continue; no unified croak-with-class convention.
- Nested struct fields are emitted as opaque byte blobs `'string(sizeof(AzU8Vec))'` (e.g. AzString record, Azul.pm:10579-10585), so record field access for any non-primitive field returns raw bytes — records are effectively write-only.


**Ergonomics issues:**
- No Perl-string→AzString auto-conversion: creating a string requires `unpack('J', pack('P', $s))` plus explicit length (hello-world.pl:13-15); reading one back is impossible without manually unpacking ptr/len/cap from the record's byte blob. Same absence for Vec/Option/Result (matches the repo-wide auto_conversion_audit).
- hello-world.pl is a 37-line console smoke test vs the reference 34-line python counter GUI (examples/python/hello-world.py) — there is no perl counter example at all, and none is currently writable against the published surface.
- Callback subs receive raw opaque integer addresses ($_[1], $_[2], ...) with no typed CallbackInfo/RefAny wrappers; even recovering user data inside a callback is unclear because refany_get's arg is attached as an 'AzRefAny' record (managed.rs:57-59) while callbacks hand you a raw address.
- Layout callbacks that must return a struct (AzStyledDom) require the user to hand-memcpy record bytes into a raw out_ptr — acknowledged as unsolved in managed.rs:195-196 ('Struct returns (AzDom) still need a record-to-pointer memcpy primitive from the spike in B.5.2').


**Completeness:** Smoke-only. The host-invoker plumbing is fully generated (releaser + per-kind pinned closures + register_callback for every callback kind, Azul.pm:32924-33203) and AzString/RefAny round-trips reportedly passed in 2026-05, but the counter e2e bar is not met: layout callbacks cannot return an AzStyledDom (struct-return writeback unimplemented), and the idiomatic wrapper layer is incoherent (record returns blessed as pointers), so App.run has never worked. Widget wrapper classes (Button, Accordion, ComboBox, ~463 classes with finalizers, 13,293 attach lines, 1,445 records) are generated but unusable until the pointer/record model is fixed. No automatic String/Vec/Option conversion.


**Blockers to ship:**
- Counter e2e impossible: layout-callback struct return (AzStyledDom) has no marshalling path — managed.rs:195-196 admits the record-to-pointer memcpy primitive was never built; without it no window can be created from Perl.
- Wrapper layer record/opaque incoherence: constructors return Platypus records but wrappers/DESTROY/methods treat them as opaque pointers (wrappers.rs:97-127), passing garbage pointers to Az*_delete and to self args — any idiomatic hello-world would corrupt the heap even if layout callbacks worked.
- Published install steps fail at the final command on all 3 platforms: Azul.pm location vs `-Ilib` mismatch, and DYLD_LIBRARY_PATH/LD_LIBRARY_PATH claims are ineffective because FFI::CheckLib ignores them (api.json perl entry).
- No doc/guide/en/hello-world page for perl exists — must be written before frontpage listing.


**Quick wins (<1 day):**
- Fix the api.json install steps: drop the DYLD/LD_LIBRARY_PATH incantations, add `mkdir lib && mv Azul.pm lib/` (or change hello-world.pl to `use lib $Bin`), and document AZ_LIB_DIR — 30 minutes.
- Fix the duplicate `sub create` collision in wrappers.rs perl_method_name (rename the C `_create` factory or the `new` rename to e.g. `create_default`) — kills the load-time warnings and the silently-shadowed Accordion/ComboBox factories.
- Refresh examples/perl/README.md: out_ptr IS passed since B.5.1; state the real remaining gap (struct-return memcpy) and remove/annotate the checked-in 36MB libazul.dylib.
- Consume Owned non-self args in wrappers.rs (invalidate the argument wrapper after by-value passes), mirroring the existing self-by-value `$$self = undef` consume — closes the add_child-style double-free class.
- Bump generated $VERSION to match the release and fix the stale target/codegen path comment in examples/perl/cpanfile.


**Verdict:** No-ship: the Perl binding is a working FFI smoke layer wrapped in an idiomatic facade that is structurally unsound — record-valued handles are treated as opaque pointers (heap corruption on GC), and layout callbacks cannot return a DOM, so no window has ever opened from Perl; roughly 6 focused days (pointer-model rework, struct-return marshalling, counter example, guide, install-step fixes) separate it from the frontpage bar.


## haskell — candidate-far (install 1/5, ~7d to ship-quality)

A fresh developer following the published macOS steps downloads libazul.dylib, azul.cabal, and three .hs files, then runs `cabal build --extra-lib-dirs=.` — which fails immediately because azul.cabal declares `c-sources: cbits/azul_shims.c` (target/codegen/haskell/azul.cabal:38) but the steps never download the cbits files. If the user hunts down azul_shims.c and azul.h themselves, the C shim fails to compile with 20+ type errors (Az*Callback struct passed where Az*CallbackType fn-pointer expected — verified with `cc -fsyntax-only`). Even past that, the final step `cabal run hello-world` targets an executable that does not exist in the downloaded azul.cabal (library-only), and no HelloWorld.hs is ever downloaded. The in-repo example (examples/haskell) is a console smoke test that registers one callback and prints messages — it never opens a window, and the current API physically cannot open one (poking a LayoutCallbackType is a generated no-op, so the FunPtr can never be spliced in at the typed level).


**Guide/install truthfulness issues:**
- api.json haskell steps (all 3 platforms) omit downloading cbits/azul_shims.c and cbits/azul.h, but the downloaded azul.cabal requires them ('c-sources: cbits/azul_shims.c', target/codegen/haskell/azul.cabal:38-39) — `cabal build` fails on every platform as published
- api.json final step '`cabal run hello-world`' (also '`DYLD_LIBRARY_PATH=. cabal run hello-world`') — the downloaded azul.cabal defines only a library, no `hello-world` executable, and no HelloWorld.hs / example .cabal is ever downloaded; the step cannot succeed
- target/codegen/haskell/cbits/ contains only azul_shims.c — the azul.h it #includes (line 8) is missing from the artifact directory, so even downloading the whole package dir does not build
- examples/haskell/README.md:24 'cabal build' + line 9 '🟢 Codegen-side polished' — false today: the current generated shim fails to compile with 20+ C type errors (identical file in examples/azul-haskell/cbits/azul_shims.c, diff-verified); `cabal build` in examples/haskell fails
- examples/haskell/README.md:11-13 'AZ_DEBUG full-GUI verification is blocked at the libazul macOS webrender side (C.1, same blocker as Pascal/Lisp)' — stale: C.1 cleared ~2026-06-01 (9 languages ship full GUI on macOS since); the real blockers are Haskell-side (H.2 FunPtr splice, shim compile errors)
- examples/haskell/README.md:98 'HelloWorld.hs — Python-quality smoke test (~64 LOC)' — it is 41 LOC and nowhere near the Python reference: it opens no window and increments no counter
- examples/haskell/README.md:140-145 'Per-method emit layer ... re-queued as item 18' — stale in the other direction: per-method wrappers (appRun, domAddChild, buttonDom, ...) now exist in target/codegen/haskell/src/Azul.hs; what is still missing is every constructor (zero `_create_via` calls in Azul.hs)
- Generated Azul.hs header (lines 29-31) and azul.cabal description (lines 20-22) claim "'RefAny' a is a phantom-typed newtype so downcasts are statically tracked" — the actual wrapper (Azul.hs:2331) is `data RefAny` fixed to `Ptr (T.RefAny ())`, no phantom parameter, no typed downcast API, and no StablePtr bridge to put Haskell data inside one at all
- examples/haskell/cabal.project:11-14 hardcodes extra-lib-dirs `/Users/fschutt/Development/azul/examples/azul-haskell` — the maintainer's machine path, and even the wrong checkout (azul, not azul-mobile); broken for every user


**Safety issues:**
- ccall safe/unsafe decision is wrong for the event loop: doc/src/codegen/v2/lang_haskell/functions.rs:285-289 marks a function `safe` only if an argument is directly a callback typedef (function_takes_callback, functions.rs:614-622). AzApp_run takes App + WindowCreateOptions, so c_AzApp_run_via is `foreign import ccall unsafe` (target/codegen/haskell/src/Azul/Internal/FFI.hs:9237) — yet App.run re-enters Haskell through every layout/event trampoline. GHC forbids callbacks into Haskell during an unsafe foreign call (crash or RTS deadlock), and the unsafe call also blocks GC for the app's entire lifetime. Every real app hits this on frame one once windows work.
- Storable offsets ignore C padding — acknowledged in the generator itself (doc/src/codegen/v2/lang_haskell/types.rs:12-16: 'for structs with padding the user can always fall back'). Concrete corruption: Types.hs:14505-14517 InvalidCharMultipleError peeks its U8Vec field at byte offset 1 (sizeOf Word8) where the C ABI places it at 8; alignment_total is taken from the FIRST field, not the max. Any struct mixing small scalars with pointers (including tag+payload Option/Result unions) is mis-read/mis-written by normal peek/poke.
- Callback-type Storable poke is a silent no-op: Types.hs:38624-38629 `data LayoutCallbackType = LayoutCallbackType; poke _ _ = pure ()` (same for all *CallbackType). A user who allocas, pokes their trampoline FunPtr wrapper, and calls c_AzWindowCreateOptions_create_via passes uninitialized alloca garbage as the function pointer — libazul later jumps to a garbage address. The poke compiles and 'succeeds'.
- One global slot per callback typedef: target/codegen/haskell/cbits/azul_shims.c:8442-8444 (g_AzButtonOnClickCallbackType_inner et al.) — registering onClick for a second button silently rebinds ALL buttons to the last handler. README admits it (examples/haskell/README.md:92-94) but nothing in the generated API stops the user.
- Trampolines return uninitialized memory when no inner is registered: cshim.rs:87-93 / azul_shims.c:8534 `AzDom __ret; if (g_..._inner) ...; return __ret;` — garbage AzDom/AzUpdate (wild vec pointers) handed to Rust, which will free them.
- FunPtr wrappers from mk_* are never freed (no freeHaskellFunPtr anywhere in FFI.hs); every register<X>Callback call leaks the previous wrapper, and Haskell exceptions escaping the user callback unwind straight through the C/Rust frames (no catch in register helpers, FFI.hs:33913-33919).
- Builder/consume methods return untracked raw values: e.g. buttonWithButtonType (Azul.hs:19986-19993) consumes the managed wrapper but returns a bare Storable T.Button with no wrapper, no finalizer (zero newForeignPtr/FinalizerPtr in the whole package) — dropping it leaks the Rust heap contents; poking it into two places double-frees.


**Idiomatic-ness issues:**
- No high-level constructors at all: zero uses of `_create_via` in Azul.hs — creating an App, Button, String, or WindowCreateOptions forces the user into Azul.Internal.FFI plus manual alloca/poke/peek; the 'Internal' module is effectively the public API
- No String -> AzString conversion helper (only the one-way azStringToString decoder); users must call c_AzString_copyFromBytes_via with a raw Word8 pointer to label a button
- Option/Result map to isSome/isOk IO predicates on pointers instead of Maybe/Either; Vec gives IO lists but nothing converts back — no native-type auto-conversion (matches auto_conversion_audit)
- azStringToString uses locale-dependent Foreign.C.String.peekCStringLen on UTF-8 bytes (Types.hs:14493-14498) — should use GHC.Foreign with utf8; mojibake on non-UTF-8 locales
- refAnySetSerializeFn/refAnySetDeserializeFn take CSize where a function pointer is meant (Azul.hs:2374-2382) — stringly/wrongly typed
- Three monolithic modules (Azul.hs 1.28MB, Types.hs 2.26MB, FFI.hs 2.52MB, 13,834 foreign imports) — typechecks cleanly (verified with ghc -fno-code) but is a multi-minute, memory-heavy compile in every user project; idiomatic Haskell would split per domain
- Eq instances go through unsafePerformIO + FFI (acceptable pattern, but combined with the padding bug the comparisons can read garbage)


**Ergonomics issues:**
- Hello-world cannot exist yet: the 41-line examples/haskell/HelloWorld.hs opens no window (registers one callback, prints, exits); vs the reference ~35-40-line Python/Rust hello world with window + counter the gap is not verbosity but impossibility (H.2 splice + no-op callback poke)
- Every operation is Ptr-level plumbing: alloca/poke/call/peek even for wrapper methods; managed wrappers take Ptr T.X inputs but methods return raw T.X values, so chaining requires re-wrapping by hand
- register<X>Callback returns an untyped `FunPtr ()` the user must 'splice' manually — and there is no working splice target
- No data-model story: without a StablePtr-based RefAny bridge there is no equivalent of `data.counter += 1`; MyDataModel in HelloWorld.hs is decorative
- cabal.project requires editing absolute extra-lib-dirs paths by hand; no rpath/install_name_tool guidance for macOS SIP realities


**Completeness:** Smoke-only, and currently regressed below smoke: the package does not build (C shim: 20+ type errors passing Az*Callback structs to Az*CallbackType fn-ptr parameters in every widget onClick/onChange setter and createVirtualView shim). Callback registration plumbing exists (13,834 FFI imports, per-typedef trampolines, register helpers) and per-method wrappers + with/dispose brackets cover most of the API surface including widgets (Button, CheckBox, TextInput, MapWidget...), but: no constructors, no window-open path (callback-type poke is a no-op), no Haskell-data RefAny (no StablePtr), no Maybe/Either/String auto-conversion, single global slot per callback typedef. Counter e2e: definitively not met; scripts/e2e_language_matrix.md:109 already lists haskell as FAILS.


**Blockers to ship:**
- Generated C shim does not compile: cshim.rs emits widget-callback setter shims passing `const Az*Callback` structs where azul.h wants `Az*CallbackType` fn pointers — 20+ hard errors (verified cc -fsyntax-only against target/codegen/azul.h); the cabal package is unbuildable for everyone
- No working path from a Haskell callback to a window: *CallbackType Storable poke is `pure ()` (Types.hs:38624+), so the trampoline FunPtr cannot be spliced into WindowCreateOptions — hello-world counter e2e is impossible, not just unwritten (README H.2)
- c_AzApp_run_via imported `ccall unsafe` while the event loop calls back into Haskell — undefined behavior in GHC the moment a window works; must be `safe` (functions.rs:285)
- No RefAny bridge for Haskell data (no StablePtr helper) — the counter data model of the e2e bar cannot be stored/retrieved
- Install docs are false end-to-end: missing cbits downloads, missing azul.h in the artifact dir, `cabal run hello-world` targets a nonexistent executable, no HelloWorld.hs step
- Storable offsets ignore C struct padding (types.rs:12-16) — mis-reads any padded struct incl. Option/Result tag+payload; must compute aligned offsets before real apps can trust peeked data
- No doc/guide/en hello-world page exists for haskell (must be written for frontpage listing)


**Quick wins (<1 day):**
- Ship cbits/azul.h alongside azul_shims.c in target/codegen/haskell/cbits and add the two missing curl steps (azul_shims.c, azul.h) plus HelloWorld.hs + executable stanza to api.json's haskell steps
- Fix cshim.rs to pass the fn-ptr member (or the correct *CallbackType arg type) for widget-callback setters — one localized codegen bug removes all 20+ compile errors
- Change `data XCallbackType = XCallbackType` to `newtype XCallbackType = XCallbackType (FunPtr ())` with a real poke, and have register<X>Callback return that type — closes H.2 at the type level
- Flip the safety heuristic: mark App_run/timer/thread/any-callback-reachable imports `ccall safe` (or simply all `safe`; the perf delta is irrelevant for a GUI toolkit)
- Delete the hardcoded /Users/fschutt/... extra-lib-dirs from examples/haskell/cabal.project; use a relative package + --extra-lib-dirs override in docs
- Rewrite examples/haskell/README.md status section: C.1 is long cleared, per-method wrappers exist, the blockers are the shim compile + H.2
- Swap peekCStringLen for GHC.Foreign.peekCStringLen utf8 in the azStringToString emitter


**Verdict:** No-ship: the binding is architecturally the most complete of the exotic candidates (13.8k imports, trampolines, brackets, per-method wrappers, clean GHC typecheck), but today the package literally does not compile, no window can ever be opened from Haskell, and the published install steps fail at step 7 of 8 — roughly a week of focused codegen work (shim fix, FunPtr-carrying callback types, safe-ccall, padding-aware Storable, StablePtr RefAny, guide) separates it from the frontpage bar.


## freebasic — candidate-far (install 1/5, ~7d to ship-quality)

A fresh developer on Windows/Linux downloads libazul, azul.bi and hello-world.bas per the frontpage steps and runs `fbc hello-world.bas`. Compilation fails immediately and catastrophically: azul.bi uses the nonexistent FreeBASIC types `LongLong`/`ULongLong` (~6,100 occurrences), names 285 parameters `len` (an fbc keyword), and declares tagged unions that embed by-value fields of structs defined thousands of lines later (e.g. AzOptionRawImage at azul.bi:6269 uses AzRawImage defined at :13559 — fbc is one-pass). Even if they got past that, hello-world.bas is stale against the artifact (treats AzUpdate as a tagged union with `.tag`, calls the 8-arg AzRefAny_newC with 5 args, calls a nonexistent AzDom_style). On macOS the story ends earlier: there is no official FreeBASIC build for macOS at all, yet api.json publishes a macOS install tab. This binding has never been compiled by fbc — the e2e matrix honestly records it as SKIP (toolchain absent).


**Guide/install truthfulness issues:**
- api.json ['0.2.0'].installation.languages.freebasic has a macOS tab ("fbc hello-world.bas", "DYLD_LIBRARY_PATH=. ./hello-world") — FreeBASIC ships official builds only for Windows/Linux/DOS; examples/freebasic/README.md:3 itself admits "No macOS-aarch64 build of FreeBASIC". The macOS tab is unexecutable fiction.
- azul.bi:6 header claims "Compatible with FreeBASIC 1.10+" — false; the file cannot compile with any fbc version (nonexistent LongLong/ULongLong types, `len` keyword params, forward-reference type ordering, broken wrapper section).
- hello-world.bas:1 comment "fbc hello-world.bas && LD_LIBRARY_PATH=. ./hello-world" — the program does not compile against the current azul.bi (AzUpdateTag_RefreshDom does not exist, azul.bi:262 defines AzUpdate as a plain Enum; AzRefAny_newC called with 5 args at hello-world.bas:121 vs 8-arg declaration at azul.bi:16236; AzDom_style at hello-world.bas:91 has 0 matches in azul.bi/azul.h; AzButton_setOnClick at hello-world.bas:83 passes a raw fn ptr where azul.bi:18258 expects an AzButtonOnClickCallback struct {cb, callable}).
- azul.bi:34117 wrapper banner "Use: Dim app As Azul.App = Azul.App(data, cfg) ' auto-cleanup" — the entire Namespace Azul wrapper section cannot compile (see safety/idiom issues).
- examples/freebasic/README.md:3-4 says "Codegen produces `.bas` bindings" — it produces a `.bi` include header (minor).
- scripts/BINDING_STRATEGY_PER_LANGUAGE.md:566 "FreeBASIC | working? (surprising) | verify" — stale; static review shows it has never compiled. scripts/e2e_language_matrix.md:70 (SKIP, toolchain absent) is the accurate record.
- doc/src/codegen/v2/lang_freebasic/mod.rs:30-33 doc comment claims "`LongInt` (32-bit), `LongLong` (64-bit)" — in FreeBASIC, Long is 32-bit, LongInt is 64-bit, and LongLong does not exist; the false comment is the root cause of the broken type map.


**Safety issues:**
- ABI-fatal integer widths: doc/src/codegen/v2/lang_freebasic/mod.rs:172-173 maps i32→LongInt and u32→ULongInt, which are 64-bit in FreeBASIC (correct would be Long/ULong). ~300 struct fields (` As LongInt`) get double their C size — every affected struct passed by value to libazul would corrupt memory if the file ever compiled.
- Tagged-union tag width mismatch: types.rs:196-204+267 emits `tag As <Enum>` where FB enums are Integer-sized (4/8 bytes), but the C ABI uses 1-byte tags with _Force8Bit (azul.h:7053-7068: AzOptionI32Variant_Some = {uint8_t tag; int32_t payload}). Every Option/Result/union crossing the FFI would have wrong tag width and wrong payload offsets.
- Wrapper double-free by design: wrappers.rs:251-259 emits a Destructor calling <Type>_delete when owned=True, but no copy Constructor and no Operator Let — any FB UDT assignment/return shallow-copies raw+owned, so both copies call _delete (double free). The header's own suggested usage pattern (azul.bi:34117) triggers wrapper copies.
- Example teaches a corrupting downcast: hello-world.bas:38 and :62 do `modelPtr = CPtr(MyDataModel Ptr, @data)` — reinterprets the stack copy of the AzRefAny header as the user model, so `counter += 1` (line 40) would overwrite the RefAny's internal pointer field, not the counter. No downcast helpers (AzRefAny_downcastRef/Mut) are exposed in azul.bi to do it correctly.
- hello-world.bas:121-127 passes align=0 to AzRefAny_newC (C side uses AZ_ALIGNOF, azul.h:78738); align 0 is an invalid Rust Layout and undefined behavior in the allocator.
- Wrapper methods pass self twice: wrappers.rs:421-427 filters the self arg by comparing to lowercase class name ("imageref") but api.json arg names are snake_case ("image_ref"), so for every multi-word type the impl calls FFI with @this.raw AND the user-supplied self ptr (azul.bi:41911 `AzImageRef_is_invalid(@this.raw, image_ref)` vs 1-arg extern at :19187) — arity/ABI mismatch on essentially all wrapper methods.


**Idiomatic-ness issues:**
- Reserved-word sanitizer (mod.rs:220-322) covers statement keywords but not FB built-in functions: `len` is used as a parameter name 285 times (e.g. azul.bi:19525 AzString_copyFromBytes `ByVal len As ULongLong`) — Len is an fbc keyword and this is a syntax error; Str/Val/Left/Right/Chr etc. are also unguarded.
- Declaration ordering ignores FreeBASIC's one-pass compiler: types.rs emits unit enums → tagged unions → POD structs → callback typedefs, but tagged unions embed later-defined structs by value (AzOptionRawImage azul.bi:6269 vs AzRawImage :13559; AzOptionDom :8738 vs AzDom :14414) and structs embed later-defined callback typedefs (AzButtonOnClickCallback :12516 uses AzButtonOnClickCallbackType declared at :14740). The comment at types.rs:61-68 claiming fbc "tolerates forward references" is wrong for by-value fields; a topological sort is required.
- Case-insensitivity collisions: FB treats GetRawImage and GetRawimage as the SAME identifier — Azul.ImageRef declares both (azul.bi ~34240) → duplicate-definition error. wrappers.rs has no case-insensitive dedup.
- Duplicate constructor overloads: wrappers.rs:151-155 emits one Constructor per IR constructor with no signature dedup — IconProviderHandle gets two `Declare Constructor ()` and ImageRef two `(ByVal ... As AzRawImage)` / two `(ByVal texture As AzTexture)` → fbc duplicate-definition errors.
- Wrapper impls call snake_case FFI names (`{ffi}_{method_name}`, wrappers.rs:317,400) while externs are declared under camelCase c_name (functions.rs:109): `AzImageRef_is_invalid` vs declared `AzImageRef_isInvalid` — undefined identifier for every multi-word method.
- Good idiom choices where they exist: Namespace Azul + Constructor/Destructor RAII is the right FB idiom, explicit pinned enum values (types.rs:174) are correct, and Alias "..." for case-sensitive link names is exactly right — the skeleton is sound, the emission is unfinished.


**Ergonomics issues:**
- hello-world.bas is 141 lines vs the ~35-40 line Python/Rust reference: user hand-writes an AzStr helper (lines 25-27), a destructor stub (12-14), JSON round-trip stubs (96-106), and a manual (and wrong) RefAny downcast — no AZ_REFLECT-style macro equivalent exists for FreeBASIC.
- No native-String acceptance anywhere: every string argument requires the user's own AzString_copyFromBytes(StrPtr(s), 0, Len(s)) dance; the idiomatic wrapper layer never takes FB String.
- No downcast/upcast helpers exposed, so the data-model round trip (the core of the counter pattern) cannot be written safely at all.
- Single 60k-line / 3.7 MB azul.bi include — slow to parse and impossible to browse; no doc guide page exists for FreeBASIC.


**Completeness:** Never compiled, so nothing is verified — below smoke level. Structurally: full FFI surface is emitted (15,157 extern declares incl. Button/ListView/NumberInput widget callbacks) and an RAII wrapper layer exists on paper, but the wrapper layer is non-compiling (wrong symbol spellings, double-self args, duplicate overloads) and there is no auto String/Vec/Option conversion. The counter e2e bar is unreachable: the example's callback pattern is both stale against the API and memory-corrupting as written.


**Blockers to ship:**
- azul.bi does not compile under any fbc: (a) ~6,100 uses of nonexistent LongLong/ULongLong types (mod.rs:174-179), (b) 285 parameters named `len` (fbc keyword), (c) tagged unions embedding by-value fields of structs defined later in the file (no topological sort; fbc is one-pass), (d) the entire Namespace Azul wrapper section (wrong FFI symbol names, double self args, duplicate overloads, case-collision methods).
- ABI layout is wrong even after it compiles: i32/u32 mapped to 64-bit FB types, and 1-byte C tags (_Force8Bit) emitted as Integer-sized FB enums — the counter e2e would corrupt memory.
- hello-world.bas must be rewritten against the current API (AzUpdate plain enum, 8-arg AzRefAny_newC, AzButtonOnClickCallback struct, no AzDom_style) with a correct downcast; the current one cannot pass the counter e2e.
- No way to verify: fbc has no macOS build (maintainer's platform) — a Linux CI job (or Windows) running fbc + the counter e2e must exist before listing.
- api.json macOS tab must be removed or replaced with an honest 'Linux/Windows only' note; a doc/guide/en hello-world page must be written.


**Quick wins (<1 day):**
- Fix the integer map in mod.rs:172-179 (i32→Long, u32→ULong, i64→LongInt, u64/usize→ULongInt) and correct the false 'LongInt (32-bit)' doc comment — 30 minutes, removes ~6,400 errors.
- Add FB built-in function names (len, str, val, left, right, chr, asc, abs, sgn, int, fix, space, string) to is_freebasic_reserved in mod.rs:220.
- Drop the macOS tab from api.json's freebasic installation entry; state 'Windows / Linux x86_64 only (no official macOS FreeBASIC build)'.
- Emit the callback typedefs (types.rs step 5) BEFORE the struct section — fixes one whole class of ordering errors cheaply.
- Gate the wrapper Namespace behind a flag / stop emitting it until rewritten — ship an honest FFI-only azul.bi with a much smaller error surface.
- Update scripts/BINDING_STRATEGY_PER_LANGUAGE.md:566 from 'working? (surprising)' to 'broken — never compiled'.


**Verdict:** No-ship: the FreeBASIC binding has never been through fbc and fails static review on at least four independent compile-blocking axes plus two ABI-corrupting ones; the hello-world is stale and memory-unsafe. The generator skeleton (RAII wrappers, Alias link names, pinned enums) is a sound design, but reaching the counter-e2e bar needs ~7 focused days including a Linux CI verification loop and a guide, since the maintainer's macOS machine cannot run fbc at all.


## ada — candidate-far (install 1/5, ~8d to ship-quality)

A fresh Ada developer follows the frontpage steps and stalls immediately: macOS has no brew GNAT (they must discover Alire themselves), and step 4 downloads hello_world.gpr — a file that is not among the generated artifacts (target/codegen ships azul.gpr, a static-library project). Even if they hand-write the .gpr, no step downloads hello_world.adb, so `gprbuild -P hello_world.gpr` fails with 'main not found'. If they copy the repo example instead, they compile a 4.3 MB azul.ads that no GNAT compiler has ever parsed (README admits the toolchain was never available to the authors), and the example they get is an FFI smoke test that prints log lines and exits — no window, no counter. Any attempt at a real GUI hits confirmed ABI breaks (structs-by-value passed as pointers, 8-byte placeholders where C has 16-byte unions, 32-bit tags vs C u8 tags) and no-op callback stubs.


**Guide/install truthfulness issues:**
- api.json ada steps (all 3 platforms) say `curl -O $HOSTNAME/ui/release/$VERSION/hello_world.gpr` — no such artifact exists; target/codegen contains only azul.gpr, which is a *library* project (Library_Name azul_bindings) with no Main and cannot build an executable.
- The install steps never download hello_world.adb (or any main program), so the very next step `gprbuild -P hello_world.gpr` cannot succeed even if the .gpr existed — the project's Main is hello_world.adb (examples/ada/hello_world.gpr:18).
- linux/macos steps put LD_LIBRARY_PATH=. / DYLD_LIBRARY_PATH=. on the *build* command but not on the run step `./obj/hello_world`; the .gpr adds only `-L. -lazul` with no rpath, so the built binary fails at launch with 'library not found' on both platforms.
- api.json description 'build with GNAT/Alire' — no Alire manifest (alire.toml) is generated anywhere; Alire cannot actually be used as stated.
- examples/ada/hello_world.adb:1 comment `gnatmake -gnat2012 hello_world.adb -largs -L. -lazul && ...` presents a working build line, but examples/ada/README.md:3-7 admits GNAT was never installable in the dev environment — neither the example nor the 115,928-line azul.ads has ever been syntax-checked by any Ada compiler.
- examples/ada/README.md:3 'Toolchain unavailable on macOS-aarch64' is stale: Alire has shipped GNAT-FSF for macos-aarch64 since ~2023/24; the brew claim is true but the overall conclusion (SKIP on this platform) no longer holds.


**Safety issues:**
- CONFIRMED layout corruption: 257 skipped types (VecRef/DestructorOrClone/etc.) are emitted as `subtype Az_X is System.Address` (doc/src/codegen/v2/lang_ada/types.rs:52-57) yet embedded BY VALUE in records — target/codegen/azul.ads:14150 gives Az_U8Vec.Destructor 8 bytes where C's union AzU8VecDestructor is 16 bytes (azul.h:10115, 17122-17127). Az_String contains Az_U8Vec, so every string/vec-bearing struct in the binding has wrong size and field offsets → memory corruption on first real use.
- CONFIRMED parameter-passing ABI break: functions.rs (doc/src/codegen/v2/lang_ada/functions.rs:117-127) emits owned struct args as plain in-mode record parameters with pragma Convention(C); per Ada RM B.3, in-mode records map to `T*` on the C side unless C_Pass_By_Copy is used — and `grep C_Pass_By_Copy azul.ads` finds zero uses. Every one of the 13,292 imports taking a struct by value (Az_App_Create, Az_App_Run at azul.ads:52158-52183, all DOM builders) passes a pointer where libazul expects the struct in registers → garbage args/crash.
- CONFIRMED tag-size mismatch on all tagged unions: types.rs:304-323 emits tag enums with no 'Size clause (azul.ads:9402-9409, Az_OptionI32_Tag), so GNAT uses int-sized (32-bit) discriminants; the C ABI tags are forced 8-bit (`AzOptionI32_Tag__Force8Bit = 0xFF`, azul.h:7056). Every Option/Result/union type crossing the ABI is misaligned. Additionally, plain Ada variant records give no C-union layout guarantee (no Unchecked_Union anywhere).
- Delete-on-garbage footgun in Controlled wrappers: wrappers.rs:127 defaults `Owned : Boolean := True` while Inner is uninitialized, and no constructor/factory returning a wrapper exists — so a user who merely declares `X : App_T;` gets Az_App_Delete called on uninitialized stack memory at scope exit (Finalize body, azul.adb:7-13).
- Callback stubs never write their out-pointer: all per-kind invoker stubs are no-ops (managed.rs:245-263; azul.adb:7034-7043 'user-callback dispatch is the second-pass agent's job'), so if App.run ever reached a layout callback, libazul would consume an unwritten StyledDom return slot → undefined behavior.
- Handle table (managed.rs:140-211) is not thread-safe (unsynchronized global array mutated from the releaser, which libazul may call off the main thread) and leaks the old array on every insert and every release (no Unchecked_Deallocation) — O(n^2) copying plus unbounded leak.
- Likely compile error, unverifiable without GNAT: `pragma Unreferenced` is emitted AFTER `begin` inside statement sequences (azul.adb:7038-7041); GNAT expects it in the declarative part — the whole file has never been parsed by a compiler, so this and similar latent errors are unfiltered.


**Idiomatic-ness issues:**
- Single flat 115,928-line `package Azul` (azul.ads); idiomatic Ada would split into child packages (Azul.App, Azul.Dom, Azul.Widgets, ...) — GNAT compile time and namespace pollution aside, no Ada shop ships one monolithic spec this size.
- Callback typedefs collapse to `subtype Az_XCallbackType is System.Address` (types.rs:105-109) instead of `access procedure/function ... with Convention => C` — users cannot form a callback pointer without Unchecked_Conversion, the least idiomatic construct in the language.
- 463 Ada.Finalization.Controlled wrappers exist (azul.ads:111915+) but wrappers.rs emits ONLY Finalize/Adjust — no constructors, no methods, no way to obtain one from any API call; the RAII layer is orphaned decoration and the raw pragma-Import free functions are the only usable surface.
- No exception-based error handling and no Option/Result mapping to Ada idioms; C-shaped tagged unions are the raw user surface.
- No String helpers (To_Ada/From_Ada between Az_String and Ada String) — consistent with the repo-wide auto-conversion audit, but Ada users expect at least Interfaces.C.Strings-level helpers.
- Positives worth keeping: reserved-word sanitization incl. shadow names (mod.rs:260-295), Pascal_Snake naming, exact C link names via pragma Import, case-insensitive duplicate detection (functions.rs:44-57), enum rep clauses pinning values.


**Ergonomics issues:**
- hello_world.adb is 52 lines that open no window and increment no counter — vs the ~35-40-line python/rust reference that builds a real UI with a `data.counter += 1` callback; a genuine Ada counter app is impossible today (callback stubs are no-ops).
- Everything pointer-shaped is stringly `System.Address`; the example needs System.Address_To_Access_Conversions gymnastics (hello_world.adb:17-18) just to round-trip one record address.
- Az_ prefix + Pascal_Snake gives workable but verbose call sites (Az_App_Create, Az_String_From_Utf8) with no fluent DOM-builder layer at all.
- azul.gpr forces -gnatwa -gnatyM120 style/warning checks onto a 4.3 MB generated file the authors never compiled — first user build will drown in warnings if it parses at all.


**Completeness:** Smoke-only, and below the smoke bar of sibling languages: RefAny round-trip is the only exercised path, and even that has never actually been compiled or run (no GNAT ever available to the authors). All host-invoker callback stubs are deliberate no-ops, so no callback of any kind can dispatch to Ada code; the layout callback cannot produce a DOM; App.run is unreachable in practice. No widget wrappers, no automatic String/Vec/Option conversion, wrapper types have no constructors or methods. Raw FFI import coverage is broad (13,292 imports, full type surface) but sits on three confirmed ABI mismatches.


**Blockers to ship:**
- Binding has never been compiled: stand up a GNAT toolchain (Alire on Linux CI at minimum, Alire macos-aarch64 now exists) and get azul.ads/azul.adb through the compiler; fix fallout (e.g. pragma Unreferenced placement after `begin` in azul.adb stub bodies).
- Fix the three ABI breaks in doc/src/codegen/v2/lang_ada: (a) emit real layouts for the 257 System.Address placeholder subtypes that are embedded by value (VecDestructor unions etc. — types.rs:52-57); (b) use Convention C_Pass_By_Copy (or pointer-taking thunk signatures) for in-mode record parameters (functions.rs) so struct-by-value C symbols are callable; (c) force 8-bit tags on tagged unions to match repr(C,u8).
- Implement real callback dispatch: replace the no-op invoker stubs (managed.rs:245-263) with a handle-id → Ada-callback registry so the layout callback and button on-click actually run — prerequisite for the counter e2e bar.
- Write and pass a real hello-world counter e2e (window + label + button + data.counter increment); the current example is an FFI smoke test that opens no window.
- Fix install docs: publish hello_world.gpr AND hello_world.adb as downloadable artifacts (or rewrite steps around azul.gpr), move LD_LIBRARY_PATH/DYLD_LIBRARY_PATH to the run step or add rpath in the .gpr, and write the missing doc/guide/en hello-world page for Ada.


**Quick wins (<1 day):**
- Fix api.json ada steps now (correct filenames, add hello_world.adb download, move library-path env to the run command) — 1 hour, removes the guaranteed first-contact failure.
- Add `for X_Tag'Size use 8;` (types.rs) and `Convention => C_Pass_By_Copy` emission (functions.rs) — small, mechanical codegen changes that eliminate two of the three ABI breaks.
- Change wrapper default to `Owned : Boolean := False` (wrappers.rs:127) so a declared-but-unused wrapper cannot delete garbage memory.
- Update examples/ada/README.md to point at Alire for macOS-aarch64 GNAT (the brew-only claim is stale) and to state plainly that callbacks are not yet wired.
- Run gnatmake -gnatc (semantic check only, no libazul needed) over azul.ads in Linux CI to lock in parseability once a toolchain exists.


**Verdict:** Do not ship: the Ada binding has never been compiled by any Ada compiler, carries three confirmed ABI-layout breaks (by-value record params, 8-byte placeholders for 16-byte unions, 32-bit vs u8 union tags), and all callback invokers are deliberate no-op stubs — the counter e2e is structurally impossible today. The codegen skeleton is thoughtful (naming, dedupe, Controlled-wrapper scaffolding), but reaching frontpage quality needs an ABI overhaul, a callback dispatch layer, a first-ever compile, a real example, and truthful install docs — roughly 8 focused days.


## smalltalk — candidate-far (install 1/5, ~9d to ship-quality)

A fresh developer following the api.json macOS steps downloads libazul.dylib, Azul.st, BaselineOfAzul.st, and installs Pharo 11 — all fine. Step 5 then dies: `'Azul.st' asFileReference fileIn` feeds a Tonel-format, 4724-classes-in-one-file artifact to Pharo's chunk-format parser, which cannot parse `Class { ... }` syntax; proper Tonel loading needs a per-package directory layout the codegen does not emit (the known blocker in examples/smalltalk/README.md). Step 6 references `hello-world.st`, a file that is never downloaded and does not exist anywhere in the repo (the actual file is HelloWorld.st, a Pharo-only smoke test that deliberately calls zero libazul functions). Trying the GNU Smalltalk route instead is worse: gst 3.2.5 parse-errors on Azul.st at line 20 and every statement of HelloWorld.st raises doesNotUnderstand (FFILibrary is Pharo UFFI). Net result: the binding is loadable by no Smalltalk system today, and no user can render a first window by any documented path.


**Guide/install truthfulness issues:**
- api.json ['0.2.0'].installation.languages.smalltalk (all 3 platforms), step 5: "'Azul.st' asFileReference fileIn. 'BaselineOfAzul.st' asFileReference fileIn. (Smalltalk at: #BaselineOfAzul) loadDefault." — FALSE three ways: (a) fileIn is Pharo's chunk-format loader and cannot parse the Tonel `Class { ... }` syntax the generator emits (verified: target/codegen/Azul.st:20 is the first Class paragraph); (b) real Tonel loading requires a one-class-per-file package directory, not a single 6.8 MB multi-class file (acknowledged blocker in examples/smalltalk/README.md:3-6); (c) `loadDefault` is not a standard BaselineOf class-side API and the baseline points at repository 'github://azul-gui/azul-smalltalk:main/src' (target/codegen/BaselineOfAzul.st:9), a repo/layout that does not exist.
- api.json step 6 (all platforms): runs `hello-world.st` — no step downloads it and no file with that name exists; the repo file is examples/smalltalk/HelloWorld.st, and it is a smoke test that never calls a libazul symbol (its own comment, HelloWorld.st:20-24: "We deliberately don't call a libazul function here").
- examples/smalltalk/README.md:3-5: "Codegen emits a Azul.st file that GNU Smalltalk (gst) accepts for the smoke layer" — FALSE; verified gst 3.2.5 parse error at line 20 of Azul.st (`parse error, expected '}'`).
- examples/smalltalk/README.md:10: "GNU Smalltalk smoke test runs." — FALSE; verified: every statement of HelloWorld.st raises doesNotUnderstand under gst (FFILibrary/subclass:...package: are Pharo-only), yet gst exits 0, which is likely how a past harness recorded a false pass. scripts/e2e_language_matrix.md:71 repeats the same stale claim ("gst runs smoke only").
- Generated header claim, target/codegen/Azul.st:8 (emitted by doc/src/codegen/v2/lang_smalltalk/mod.rs:121): "Load the file with Iceberg or by `'Azul.st' asFileReference fileIn`" — both false: Iceberg needs a git repo with Tonel package directories; fileIn cannot parse Tonel.
- README.md:21-24 claims R14 consume (`handle := nil`) "Closes the double-free for by-value C calls" — only self-by-value is consumed; owned by-value non-self arguments (e.g. AzAppConfig in AzApp_create, AzWindowCreateOptions in AzApp_run) are never consumed, so their wrapper finalizers still double-free (see safety).
- macOS step uses `DYLD_LIBRARY_PATH=. ./pharo ...` — the pharo launcher from get.pharo.org is a #!/bin/sh script, and macOS SIP strips DYLD_* when exec-ing the protected shell interpreter, so the variable never reaches the VM; works only if dlopen's cwd fallback happens to fire.


**Safety issues:**
- Double-free on owned by-value arguments: wrappers nil `handle` only when SELF is passed by value (doc/src/codegen/v2/lang_smalltalk/wrappers.rs:328-366); any OTHER owned struct argument passed by value is consumed by Rust while its Smalltalk wrapper keeps an armed finalizer. Normal code: `AzulApp create: refAny app_config: cfg handle` (Azul.st:166967, C sig `AzApp_create(AzRefAny initial_data, AzAppConfig app_config)` at Azul.st:90760) — when the AzulAppConfig wrapper is GC'd, finalize calls AzAppConfig_delete on already-consumed bytes. Same for AzWindowCreateOptions in `app run:` (Azul.st:90781).
- Consume mechanism may not reach the finalizer at all: setHandle: registers with `FinalizationRegistry default` (wrappers.rs:147-176; Azul.st:166952-166956). If Pharo's registry captures an executor shallow-copy at add: time (classic WeakRegistry semantics, which the codegen comment itself cites at wrappers.rs:172-173), the later `handle := nil` on the original never propagates to the executor, so even self-consume (e.g. AzulButton >> withOnClick:, Azul.st:195720-195725) still double-frees. Whether `FinalizationRegistry default` even exists as a class-side selector in Pharo 11 is unverified — if absent, every wrapper construction raises DNU. Untested because the artifact cannot be loaded.
- By-value struct passing/return through UFFI is unvalidated: nearly every primitive passes and returns FFIExternalStructure by value (e.g. `AzApp AzApp_create(...)` Azul.st:90760, `AzString AzString_fromUtf8(...)` Azul.st:83194). Pharo UFFI's by-value struct support is historically incomplete on ARM64 macOS; nothing in-repo has ever executed one of these calls.
- Callback typedefs are erased to void*-sized FFIExternalType aliases (doc/src/codegen/v2/lang_smalltalk/types.rs:331-352; ~20 'SKIPPED: callback typedef' markers at Azul.st:65684+). A user who follows the emitted hint 'use #ffiCallback: at the call site' with a slightly-wrong signature corrupts the stack; there is no generated safe adapter, no host-invoker wiring for Smalltalk, and no GC-anchoring of the FFICallback object, so even a correct one can be collected while Azul still holds the fn pointer.
- Consumed-by-value `create:`/factory arguments include AzString: `AzulButton class >> create: label` (Azul.st:195703) takes AzString by value — if built via the AzulString wrapper, the same double-free as above; if built raw, the user must keep byte buffers alive manually during the call (no pinning helper emitted).


**Idiomatic-ness issues:**
- Keyword selectors keep raw snake_case C argument names: `create: initial_data app_config: app_config` (Azul.st:166967), `withType: label button_type: button_type` (Azul.st:195709) — wrappers.rs:263-271 and functions.rs:181-191 camelCase only the FIRST keyword and pass `a.name` through verbatim for the rest. Smalltalk selectors are never snake_case.
- One 6.8 MB file with 4724 classes in a pseudo-Tonel format matches no Smalltalk module system: not chunk format (fileIn), not real Tonel (one class per file + package.st directories), not a Metacello-loadable repo — the emitted BaselineOfAzul declares packages 'Azul-Native'/'Azul-Types'/'Azul-Core' that exist only as #category strings inside the monolith.
- `AzulApp >> clone: instance` (Azul.st:166989) is an instance method that ignores `handle` and demands the raw FFI struct as an argument — the takes_self detection in wrappers.rs:299-309 only matches args named `self` or the lowercased class name, and Azul's clone functions name it `instance`.
- Wrapper methods take raw FFIExternalStructure handles, not wrapper objects: `app addWindow: createOptions` needs `azulWindowCreateOptions handle`, so the 'idiomatic' layer leaks its own abstraction at every cross-type call (no `#handle`-unwrapping coercion in wrappers.rs:299-309).
- Generated temp `| _ret |` (wrappers.rs:350-364) uses a leading-underscore identifier — legal in modern Pharo but unidiomatic, and outright hostile to older parsers where `_` was assignment.
- Wrapper classes have no class comments (a strong Pharo convention); docs are emitted as free-floating string literals between Tonel paragraphs (types.rs:368-374), which a real Tonel parser would reject anyway.
- README/examples conflate GNU Smalltalk and Pharo: HelloWorld.st is Pharo-UFFI-only code shipped under a README titled 'GNU Smalltalk / Pharo' that claims gst runs it.


**Ergonomics issues:**
- There is no GUI hello-world at all: examples/smalltalk/HelloWorld.st (33 lines) defines an FFILibrary subclass and prints its resolved library name — zero libazul calls, no window, no DOM, no counter. Nothing comparable to the 35-40-line python/rust fluent-builder reference exists or could be written today.
- No Smalltalk String → AzString conversion: AzulString offers only `fromUtf8: ptr len:` style raw-pointer factories (Azul.st:184760-184806); the user must hand-marshal a ByteArray/ExternalAddress for every label. No Vec, Option, or Result conversions either (matches the repo-wide auto_conversion_audit).
- Building the mandatory AzRefAny app-data requires `azRefAnyNewC: ptr len: align: type_id: type_name: destructor: serialize_fn: deserialize_fn:` (Azul.st:70380-70382) with a hand-rolled destructor callback — 8 raw arguments before the first window can exist.
- Tagged-union values (all styling/options enums) must be built by poking fields on FFIExternalUnion instances; only unit variants get factory helpers (wrappers.rs:377-416).
- Every wrapper-to-wrapper call requires manual `foo handle` unwrapping, and constructors consume their by-value args silently, so the natural fluent style is also the double-free style.


**Completeness:** Smoke-only, and below the usual smoke bar: the shipped 'smoke test' never invokes a single libazul symbol (HelloWorld.st:20-24 says so explicitly), and the binding artifact itself cannot be loaded into any Smalltalk image (Tonel monolith: fileIn rejects it, TonelReader needs a directory layout, gst parse-errors at Azul.st:20). Callbacks do not work: all ~20 callback typedefs are erased to void* with a 'use #ffiCallback: yourself' comment, and there is no host-invoker wiring for Smalltalk, so the counter e2e is impossible. Widget wrapper classes are emitted (AzulButton, AzulListView, etc., e.g. Azul.st:195703-195725) but are unusable without callback and string support. No automatic String/Vec/Option/Result conversion exists.


**Blockers to ship:**
- Binding artifact is unloadable in every Smalltalk system: single-file multi-class Tonel is rejected by Pharo fileIn (chunk parser), by TonelReader (needs package-directory layout — the acknowledged blocker in examples/smalltalk/README.md:3-6 and memory note smalltalk_tonel_blocker), and by gst (verified parse error at Azul.st:20). Codegen must emit either a real Tonel package directory or chunk-format fileIn source.
- No callback path: callback typedefs are void* aliases with no generated ffiCallback adapters and no host-invoker integration (types.rs:331-352), so the hello-world counter e2e cannot pass.
- No Smalltalk String → AzString bridge (only raw ptr/len factories, Azul.st:184760+), without which even a static-label hello-world cannot be written in normal code.
- api.json install steps are false end-to-end (fileIn of Tonel fails; step 6 runs a file that is never downloaded and does not exist) and there is no doc/guide/en hello-world page — both must be rewritten against a working flow.
- Zero runtime verification ever performed against libazul from Pharo (by-value struct FFI, FinalizationRegistry API, callback marshalling all untested); a Pharo-in-the-loop e2e run is required before listing, including checking that AzApp_run's event loop coexists with the Pharo VM on macOS.


**Quick wins (<1 day):**
- Fix the README (examples/smalltalk/README.md): delete the false 'GNU Smalltalk smoke test runs' and 'gst accepts Azul.st' claims, retitle Pharo-only — 15 minutes.
- Fix api.json step 6 filename (hello-world.st → an actually-shipped file) and add the missing download step, or mark the language as experimental with honest steps — 30 minutes.
- Fix the generated header lie at mod.rs:121 ('fileIn' loading claim) — 5 minutes.
- camelCase all keyword selectors (apply snake_to_lower_camel to subsequent keywords in wrappers.rs:263-271 and functions.rs:181-191) — under an hour, removes the most visible idiom violation.
- Add `AzulString class >> fromString: aString` that marshals a Smalltalk String via utf8Encoded ByteArray, and make wrapper methods send #handle to Azul wrapper arguments — under a day, removes the two worst ergonomic walls.
- Make the smoke harness fail on gst DNU output (gst exits 0 despite per-statement errors), so status boards stop recording false passes — an hour.


**Verdict:** No-ship: the Smalltalk binding is currently unloadable in any Smalltalk dialect (pseudo-Tonel monolith), has no callback or string bridge, and its README/api.json claims are demonstrably false, so nothing between download and first window works. Reaching the counter-e2e bar needs a Tonel-package emission rewrite plus callback/string plumbing and a first-ever Pharo runtime validation — roughly 9 focused days with real risk of a Pharo-VM/event-loop blocker at the end.


## fortran — candidate-far (install 1/5, ~10d to ship-quality)

A fresh developer with gfortran curls libazul.dylib, azul.f90 and the Makefile per api.json, then runs the documented `gfortran -c azul.f90` — and hits hard compile errors: `Accordion_create` and `ComboBox_create` are each defined twice in the generated module (gfortran: "Procedure 'accordion_create' at (1) is already defined"). If they try `make` instead, the shipped FFLAGS `-std=f2008` turn the 1,359 over-132-column lines into ~40,000 hard line-truncation errors. There is no downloadable hello_world.f90 (the final link step references it but no step fetches it, and no guide page exists to copy it from), so even a determined user dead-ends before any window. Verified: after hand-renaming the two duplicate factories, the full 154,558-line module compiles clean in ~10s and the repo's smoke test compiles against it — but that smoke test only round-trips an AzString and a RefAny handle; no window, no callback, no counter is possible because callback invoker stubs are empty and the tagged-union ABI is wrong.


**Guide/install truthfulness issues:**
- api.json fortran step 4 'gfortran -c azul.f90' fails today: duplicate procedures Accordion_create (azul.f90:149770 vs 149777) and ComboBox_create (azul.f90:152820 vs 152827), caused by wrappers.rs:284-288 mapping both 'new' and 'create' constructors to the same 'create' suffix. Verified with gfortran 15.2 default flags.
- api.json final step links 'hello_world.f90' on all three platforms, but no prior step downloads it, deploy.rs BINDING_FILES (doc/src/dllgen/deploy.rs:768-770) ships only azul.f90 + Makefile for fortran (unlike pascal/cobol which ship their hello-world), and no doc/guide/en fortran page exists to copy it from. The documented flow cannot produce a binary.
- The downloaded Makefile itself fails: FFLAGS '-O2 -std=f2008 -fimplicit-none' (makefile.rs:45, target/codegen/Makefile.fortran:19) makes the 1,359 lines >132 columns in azul.f90 hard errors ('Line truncated ... -Werror=line-truncation', 40,337 error lines observed). Needs -ffree-line-length-none or generator-side line wrapping.
- examples/fortran/README.md:11 'Smoke test (AzString round-trip + refany_create) verified' is stale: the CURRENT regenerated azul.f90 (2026-07-04, md5-identical between examples/ and target/codegen/) does not compile at all, so even the smoke claim no longer holds; README.md:21 'make' also fails per the -std=f2008 issue.
- azul.f90 header comment (lines 10-12) and mod.rs:88-92 advertise 'gfortran -c azul.f90' as the build — fails as above.
- macOS api.json steps give no rpath or DYLD_LIBRARY_PATH guidance; the Makefile's LDFLAGS '-Wl,-rpath,$$ORIGIN' (makefile.rs:46) is an ELF-ism that embeds a literal '$ORIGIN' string on macOS (should be @loader_path). Worse, the shipped examples/fortran/libazul.dylib install name is the absolute dev path '/Users/fschutt/Development/azul-mobile/target/release/deps/libazul.dylib' (otool -D), so a linked binary will not launch on any end-user machine without DYLD_LIBRARY_PATH — mentioned only in the repo README, not in the frontpage steps.
- README.md is otherwise commendably honest: it states the tagged-union codegen gap and 'Full GUI: not reachable' — that claim is verified still true against types.rs:223 and azul.h.


**Safety issues:**
- ABI corruption on every tagged union: doc/src/codegen/v2/lang_fortran/types.rs:223 (emit_tagged_union) emits {integer(c_int) tag; type(c_ptr) payload} (e.g. azul.f90:5743 AzOptionI64, :10739 AzOptionDomNodeId), but the real C ABI is union{struct{uint8_t tag; <inline payload>}} (target/codegen/azul.h:18384-18398). Wrong tag width AND wrong payload representation: any by-value AzOption/AzResult/union argument or return is memory-corrupting, and every POD struct that embeds a union field has all subsequent fields at wrong offsets. 463 tagged unions / 217 Option types affected — this silently corrupts memory in perfectly normal code (WindowCreateOptions etc.).
- Callback invoker stubs never write results: doc/src/codegen/v2/lang_fortran/managed.rs:188-204 emits empty per-kind invoker bodies ('First-pass plumbing only') that azul_host_invoker_init registers with libazul (managed.rs:234-246). If a user attaches any widget callback and it fires, libazul reads an unwritten out-parameter (e.g. Update) — uninitialized-memory UB from normal code.
- Wrapper double-free by intrinsic assignment: wrapper types (wrappers.rs:139-151) carry {raw, owned=.true.} with a final:: finalizer calling <Type>_delete (wrappers.rs:215-227) but define no assignment(=); `type(App)::a,b; b=a` gives two owners of one raw handle → double delete at scope exit. Also a never-assigned local has owned=.true. with UNINITIALIZED raw bytes, so its finalizer deletes garbage.
- wrappers.rs:271-275 claims assigning a factory result 'transfers ownership' — false under F2008 finalization semantics (gfortran >=13 finalizes the function-result temp after assignment), so `app = App_create(...)` would delete the underlying AzApp the caller's copy still holds: use-after-free + double-free by design. Currently latent only because the factories are accidentally private (see idiom issues).


**Idiomatic-ness issues:**
- The 'idiomatic' OO wrapper layer is unreachable: the module is `private` by default and wrappers.rs never emits `public ::` for factory functions (verified: zero `public :: *_create` wrapper factories in azul.f90; App_create at azul.f90:132644 is private), while the wrapper types' raw component is `private` too — so users cannot construct App/String/etc. wrappers at all. The only usable surface is the raw C-style az_* interface (which IS public), defeating the entire final::/TBP design.
- No generic interfaces for overloaded constructors — the direct cause of the Accordion_create/ComboBox_create duplicate-procedure compile break; idiomatic Fortran would use a generic `interface Accordion` block.
- Single flat 6.3MB / 154,558-line module in one namespace; no submodules, ~20k public symbols. Compiles in ~10s once fixed, but hostile to editors and incremental builds.
- No character(*) convenience layer: users must null-terminate, take c_loc, and pass explicit c_size_t lengths (hello_world.f90:8,25-26) — no az_string helper accepting a Fortran character string.
- No error-handling idiom: no stat=/errmsg= convention and Result unions are ABI-broken, so fallible calls have no usable channel.
- Unsigned integers map to signed kinds (mod.rs:263-271) — unavoidable in Fortran and honestly documented; fine.


**Ergonomics issues:**
- hello_world.f90 is a 51-line console smoke test (AzString + RefAny round-trip, explicitly prints 'Full App.run wiring requires layout / callback wrappers') vs the reference standard ~35-40-line GUI counter app in examples/python and examples/rust. There is no window, no DOM, no callback anywhere in the fortran example.
- Everything is stringly/pointer-typed at the use site: c_loc(), C_NULL_CHAR, manual byte lengths, and tagged-union payloads that the docs tell users to cast via c_f_pointer by hand (types.rs comment emitted at azul.f90:10746-10747).
- Building the binding costs a 154k-line compile of azul.f90 before the user's first line of code; any FFLAGS deviation (e.g. the shipped -std=f2008) explodes into tens of thousands of errors.


**Completeness:** Smoke-only, and currently regressed below even that (generated azul.f90 does not compile). Callbacks: host-invoker registration plumbing exists (releaser + 16 per-widget invoker setters) but every invoker body is an empty stub (managed.rs:188-204) — no user callback can ever be dispatched, so the counter e2e bar is unreachable. Widgets: exposed only as raw C-ABI declarations (Button/ListView/etc. setters exist as az_* externs); the OO wrapper layer exists but is entirely private/unconstructible. No automatic String/Vec/Option conversion to native Fortran types; all 463 tagged unions (incl. every Option/Result) have a wrong-ABI representation that corrupts by-value calls. Prior status boards (scripts/e2e_language_matrix.md:64,108: "smoke-only ... no counter E2E, FAILS") remain accurate on the tier but understate today's state — the smoke path itself is broken until the duplicate-factory regression is fixed.


**Blockers to ship:**
- Generated azul.f90 does not compile: duplicate Accordion_create/ComboBox_create procedures (wrappers.rs:284-288 collapses 'new' and 'create' to the same name) plus 1,359 lines >132 columns that hard-fail under the shipped Makefile's -std=f2008.
- Tagged-union ABI mismatch (types.rs:223 tag:c_int + payload:c_ptr vs C's uint8-tag inline union) corrupts every by-value Option/Result/union and every struct embedding one — no window can be opened; this is the acknowledged 'codegen rewrite needed' and it gates any GUI at all.
- Callback dispatch is unimplemented (empty invoker stubs in managed.rs:188-204) — the hello-world counter e2e cannot pass.
- Install docs reference a hello_world.f90 that no step downloads and no guide page provides; a fortran guide/hello-world page must be written with a real counter example.


**Quick wins (<1 day):**
- Fix the duplicate-factory collision in wrappers.rs:284-288 (disambiguate by arity or keep method_name verbatim, ideally behind a generic interface) — after hand-patching just these two names the entire module compiles clean, so this single fix restores the smoke tier.
- Add -ffree-line-length-none to FFLAGS in makefile.rs:45 (or wrap long lines with '&' continuations in the generator) so the shipped Makefile works with its own file.
- Ship hello_world.f90 via deploy.rs BINDING_FILES and add a matching curl step to api.json (pascal/cobol already follow this pattern at deploy.rs:790/:766).
- Emit @loader_path instead of $ORIGIN in the Makefile rpath on macOS, and add a DYLD_LIBRARY_PATH note to the macOS api.json steps; normalize the dylib install name at deploy time (install_name_tool -id @rpath/libazul.dylib).
- Either export the wrapper factories (`public :: App_create` etc.) with an assignment guard, or strip the dead wrapper layer from the emission — today it is 463 finalizer types of unreachable code that doubles the module size and carries latent double-free semantics.
- Refresh examples/fortran/README.md so its 'verified' claim carries a date and the make instructions match reality.


**Verdict:** No-ship: the artifact a user would download today does not even compile, and beneath that regression sit two structural gaps — a wrong-ABI tagged-union representation affecting all 463 unions and completely stubbed callback dispatch — that make a first window, let alone the counter e2e, impossible without the acknowledged codegen rewrite (~10 focused days including a guide). Credit where due: the repo README is honest about the smoke-only status, and one small fix (duplicate factory names) plus one Makefile flag would at least restore a truthful smoke-tier candidate.


## php — candidate-far (install 1/5, ~12d to ship-quality)

A fresh macOS developer follows the frontpage steps: brew install php, curl libazul.dylib + Azul.php + composer.json, then runs `php -d ffi.enable=true hello-world.php` — a file the steps never told them to download. After copying it from the repo, PHP dies instantly with `Parse error: syntax error, unexpected token "switch" ... on line 118037` because the generated Azul.php declares `final class Switch` (a reserved word), so require_once of the binding fatals before a single FFI call. Even with that fixed, the FFI path throws a RuntimeException by design as soon as any callback is registered (php-ffi rejects closure-to-fnpointer), so no window with content can ever open; the alternative native-extension path requires an undistributed 91 MB cargo build with LIBCLANG_PATH/RUSTFLAGS incantations and still cannot wire a layout callback. The end state today is: nothing runs, not even the smoke test.


**Guide/install truthfulness issues:**
- api.json ['0.2.0'].installation.languages.php (all 3 platforms): final step is `php -d ffi.enable=true hello-world.php`, but no step downloads hello-world.php — only libazul, Azul.php and composer.json are curled. The command fails with 'No such file' on a literal follow-through.
- api.json final step cannot succeed even with the file: target/codegen/Azul.php (and the identical examples/php/Azul.php) fails `php -l` on PHP 8.5.6 — 'Parse error: unexpected token "switch"' at line 118037 (`final class Switch`). Verified today; every install path that loads Azul.php is dead.
- target/codegen/composer.json:11-14 declares psr-4 autoload 'Azul\' => 'src/', but the deliverable is a single flat Azul.php — composer autoloading can never resolve it, and no install step runs composer anyway; the curl of composer.json is decorative.
- examples/php/README.md:71 claims '`libazul-ext.dylib` — prebuilt extension (91 MB; debug build)' is among the files; the directory contains no such file (only libazul.dylib, 36 MB).
- examples/php/README.md:83-87 (R15) claims '$this->ptr = null ... so __destruct skips the Az<X>_delete call'. False against current codegen: wrappers.rs:217-223 emits __destruct with NO null/isset guard, and the property is declared non-nullable `private \FFI\CData $ptr` (wrappers.rs:137), so the null assignment itself is a TypeError.
- examples/php/hello-world-ext.php:44 asserts azul_version() === '0.0.7' and target/codegen/php_api.rs:67 hard-codes "0.0.7" while the project ships 0.2.0 — stale version constant baked into the smoke gate.
- api.json linux steps list both `sudo apt-get install -y php-ffi` AND `sudo dnf install -y php-ffi` as sequential commands; one always fails on any given distro.
- README.md:15 'Host-invoker smoke layer verified (build + load + Azul\Dom class round-trip)' is unverifiable today — the prebuilt extension it refers to is absent, and the FFI-path artifact does not parse.


**Safety issues:**
- Fatal consume mechanism: wrappers.rs:479-485 emits `$this->ptr = null;` into the non-nullable typed property `private \FFI\CData $ptr;` (wrappers.rs:137) — every by-value consuming method (e.g. Button::with_on_click at Azul.php:110560-110567, Button::dom at Azul.php:110599) throws TypeError immediately after the native call. If the property were made nullable without also guarding __destruct (wrappers.rs:217-223 calls FFI::addr($this->ptr) unconditionally), destruction of a consumed object either throws or double-frees memory Rust already owns.
- Receiver-dedup arity bug: user_args (wrappers.rs:590-596) filters the self arg only when its C name equals the lowercased-concatenated class name, but C args are snake_case — e.g. Azul.php:56546 IconProviderHandle::set_resolver($icon_provider_handle, $resolver) passes 3 args to 2-arg AzIconProviderHandle_setResolver (azul.h:42780). Every instance method of every multi-word class (WindowCreateOptions, NodeData, ...) has a duplicated receiver parameter and a wrong-arity native call.
- clone()/toString() broken on ALL ~1445 wrapper classes: emit_instance_method_alias (wrappers.rs:494-526) keeps the C `instance` arg, so Azul.php:65375 App::clone($instance) calls 1-arg AzApp_clone (azul.h:53142) with 2 args — FFI arity error at every call site.
- Unconditional-ownership double-free: __construct 'takes ownership' of any CData (wrappers.rs:143,150-153) and raw() hands the cdata back (Azul.php class bodies); wrapping the same cdata twice, or wrapping a borrowed payload from a union payload<Variant>() accessor, produces two __destructs calling Az<T>_delete on the same bytes.
- Extension path marks every class Send+Sync without justification: php_api.rs:135-136 `unsafe impl Send/Sync for AzulApp` (same for Button, Dom, ...) — under ZTS PHP these wrap thread-affine window/app state.
- Extension generic invoker drops all callback arguments and the return slot: php_api.rs:41-62 ignores _args/_n_args/_ret, so a layout callback can never write its StyledDom return — libazul reads whatever was in the ret buffer (uninitialized-read hazard the moment the invoker is actually wired to App::run).


**Idiomatic-ness issues:**
- Mixed naming conventions in one API: facade methods are camelCase (registerCallback, refanyCreate) while wrapper methods are snake_case (set_on_click, create_body) — PHP convention (PSR-12/PER) is camelCase methods throughout.
- toString($instance)/clone($instance) instead of PHP magic __toString()/__clone(); default_() with trailing underscore instead of a create()/new-style factory.
- No composer/Packagist story: single 7.1 MB flat file with a psr-4 declaration that cannot match it; idiomatic PHP delivery is a composer package (even a classmap over one file would work).
- Reserved-word handling is half-done: types like String become Azul\AzString (prefix kept) while Switch was missed entirely — inconsistent class naming plus a parse error.
- Errors are not exceptions: native failures surface as raw FFI errors, callback exceptions are swallowed to STDERR (managed.rs:326-333); PHP norm is typed exceptions.
- Extension path is stringly-typed by design: callbacks registered by global function NAME (azul_register_layout_callback('layout')) with JSON-string args — closures/first-class callables (PHP 8.1 `foo(...)` syntax) are the idiom ext-php-rs supports via ZendCallable.


**Ergonomics issues:**
- There is no GUI hello-world at all: hello-world.php (34 lines) is an AzString round-trip smoke test; hello-world-ext.php (143 lines) is a handle-table smoke test. Nothing comparable to the 35-40-line python/rust counter app is writable today.
- No PHP string → AzString conversion anywhere in the FFI path: users must allocate `$ffi->new('uint8_t[N]')`, copy byte-by-byte in a PHP for-loop, and call AzString::from_utf8($ptr,$len) (Azul.php:98014) for every label/text — see hello-world.php:18-26.
- Fluent chaining is impossible: consuming builders return raw \FFI\CData instead of `new self(...)` (wrappers.rs:482-485), so Button::create(...)->with_on_click(...)->dom() fatals with 'call to a member function on FFI\CData' — after first throwing the TypeError from the null assignment.
- RefAny model on the ext path stores JSON snapshots (php_api.rs:74-80), so the reference `data.counter += 1` mutation pattern cannot work — a callback would mutate a decoded copy with no writeback.
- Two half-paths the user must choose between (php-ffi: no callbacks by design; extension: must cargo-build a 91 MB PHP-version-locked .dylib with LIBCLANG_PATH + dynamic_lookup RUSTFLAGS), documented only in the example README.


**Completeness:** Smoke-only, and currently below smoke (artifact does not parse). FFI path: all ~1445 classes/widgets are wrapped and callback auto-registration is emitted in wrapper methods, but ensureHostInvokerInit deliberately throws on standard php-ffi (managed.rs:131-150), so every callback registration — and therefore any window content — is unreachable; no String/Vec/Option auto-conversion exists (auto_conversion_audit confirms). Extension path: only a handful of classes are emitted and they are skeletal — WindowCreateOptions has only default() (php_api.rs:1587-1591), Button skips setOnClick ('6 skipped'), the generic invoker discards callback args and return values, and callbacks are function-name strings with JSON args. The counter e2e is impossible on both paths; scripts/e2e_language_matrix.md:62 ('counter E2E FAILS') matches current code.


**Blockers to ship:**
- Azul.php does not parse: `final class Switch` at line 118037 — add the full PHP reserved-keyword list to php_class_name_is_reserved (wrappers.rs:670-702) and regenerate; until then every install path fatals at require time.
- No functional callback path, so the hello-world counter e2e bar is unreachable: php-ffi path throws by design (managed.rs:131-150); the extension path lacks layout-callback wiring on WindowCreateOptions, Button::setOnClick, and real arg/return marshalling in the generic invoker (php_api.rs:41-62, 1587-1591).
- Systemic wrapper bugs make even the non-callback API unusable: duplicated-receiver arity bug (wrappers.rs:590-596), broken clone/toString aliases on all classes (wrappers.rs:494-526), and TypeError-throwing consume mechanism on a non-nullable $ptr (wrappers.rs:137,479-485) with an unguarded __destruct.
- Install docs are false: hello-world.php is never downloaded in any platform's steps, and the final command cannot succeed; composer.json's psr-4 autoload points at a nonexistent src/ layout.
- Extension distribution problem unsolved: ext-php-rs binaries are ABI-locked to PHP minor version and NTS/ZTS flavor; there is no prebuilt-extension release channel, only a local cargo build with LIBCLANG_PATH/RUSTFLAGS incantations.
- No doc/guide/en hello-world page for PHP exists — one must be written before frontpage listing.


**Quick wins (<1 day):**
- Extend php_class_name_is_reserved (wrappers.rs:670) with hard keywords (switch, default, function, class, do, for, print, exit, clone, use, global, ...) — one-line-per-word fix that makes the 7 MB artifact parse again; verify with `php -l` in codegen CI (cheap, no libazul needed).
- Fix user_args (wrappers.rs:590-596) to also filter the snake_case class name, and drop the leftover `instance` arg in emit_instance_method_alias — removes the duplicated-receiver arity breakage across all multi-word classes and all clone/toString methods.
- Change the property to `private ?\FFI\CData $ptr` and emit `if ($this->ptr === null) { return; }` at the top of __destruct — makes the R15 consume mechanism actually do what the README claims.
- Wrap consuming-builder returns in `new self(...)` so fluent chains type-check, and add an `AzString::fromPhp(string $s)` helper that does the uint8_t buffer dance internally, auto-applied for AzString-typed args.
- Fix api.json php steps: add the hello-world.php curl line, delete the composer.json line (or ship a matching classmap composer.json), and split apt-get/dnf into distro-specific alternatives.
- Refresh examples/php/README.md: remove the nonexistent libazul-ext.dylib entry, correct the 6.3 MB size, and update the 0.0.7 version constant in php_api.rs + the assert in hello-world-ext.php.


**Verdict:** No-ship: the published artifact fatals at parse time, the FFI path can never support callbacks (php-ffi design limitation, correctly self-diagnosed by the code), and the extension path that could support them is a JSON-smoke skeleton without layout-callback wiring or arg marshalling — reaching the counter-e2e bar means finishing and distributing the ABI-locked native extension plus fixing three systemic codegen bugs, roughly two focused weeks.


## cobol — candidate-far (install 2/5, ~12d to ship-quality)

A fresh developer with GnuCOBOL runs the 5 published commands and they all succeed: the files download (deploy manifest at doc/src/dllgen/deploy.rs:766-767 really ships azul.cpy + hello-world.cob), cobc compiles in ~4s, and the binary runs — but it only prints a dozen DISPLAY lines ('COBOL FFI smoke test starting... Full app wiring requires user-side ENTRY paragraphs') and exits. No window ever appears; hello-world.cob never calls a single libazul function. Anyone who then tries to write a real app hits hash-mangled 30-char identifiers (FN-AZ-ICON-PROVIDER-HANDL-6f00), struct-by-value calls that GnuCOBOL silently downgrades to pointer passing, and tagged-union records whose tag width and payload size do not match the C ABI.


**Guide/install truthfulness issues:**
- examples/cobol/README.md:14 lists 'hello-world.cbl' but the file is hello-world.cob; README.md:16 lists 'Makefile — cobc build invocation' but no Makefile exists in examples/cobol/ (verified with ls).
- api.json cobol install steps ('Download ... build with cobc' then run) imply a GUI hello world; the shipped hello-world.cob is a console-only smoke test that itself prints that full app wiring is impossible without hand-written ENTRY paragraphs. The install page carries no smoke-tier disclaimer.
- Copybook/codegen docs claim 'Tagged unions use REDEFINES to overlay each variant payload ... so the COBOL record matches the Rust #[repr(C)] enum layout' (doc/src/codegen/v2/lang_cobol/mod.rs:19-21, types.rs:8-9, types.rs:246-254). Reality: types.rs:366-381 emits TAG BINARY-LONG + opaque PIC X(64) — no REDEFINES, and the layout matches neither the claim nor the ABI.
- types.rs:373-374 comment: 'largest in practice is ~32 bytes for a Vec descriptor'. False — AzOptionDom's Some payload is a full AzDom (azul.h:36469-36474: NodeData + DomVec + CssVec + size_t), far over 64 bytes; many CssProperty/Option unions also exceed it.
- azul.cpy:64620-64623 suggested scaffolding calls FN-AZ-APP-SET-HOST-HANDLE-RELEASER — that constant does not exist (mangled real name is FN-AZ-APP-SET-HOST-HANDLE-9778, azul.cpy:64625) — and uses invalid COBOL syntax 'USING BY VALUE ENTRY "azul-releaser"' (GnuCOBOL requires SET ptr TO ENTRY "x" first). Source: doc/src/codegen/v2/lang_cobol/managed.rs:59-62.
- functions.rs:53-54 banner says 'Pointers are passed BY VALUE; records BY REFERENCE', but functions.rs:115-126 emits 'BY VALUE' in the SIGNATURE comment for every owned (struct-by-value) argument — self-contradictory, and both variants are ABI-wrong for small structs (see safety).
- mod.rs:45-53 'Wiring' note says the module 'is NOT wired from v2/mod.rs' and references scripts/api-json-additions/cobol.json patches to be merged — stale; generation is wired (generator.rs:434 writes azul.cpy) and api.json already contains the cobol install section.


**Safety issues:**
- Wrong tag width on ALL tagged unions: types.rs:367 (and :325) emit '05 TAG USAGE BINARY-LONG' (4-byte signed), but the C ABI tag is uint8_t — every union in azul.h uses the *_Tag__Force8Bit = 0xFF pattern (e.g. azul.h:39060-39064 AzOptionDom_Tag). A COBOL read of TAG picks up 3 padding bytes; a write clobbers padding/payload. Any union inspection or construction is undefined behavior.
- Undersized/misaligned union payload: types.rs:378-381 emits a fixed 'PIC X(64)' payload anchor at offset 4. C payload sits at the union's natural alignment (offset 8 for pointer-bearing variants) and can far exceed 64 bytes (AzOptionDom Some payload = AzDom, azul.h:36469). Every struct that embeds a tagged union inline therefore has wrong size and wrong offsets for all subsequent fields — silent memory corruption when passed by reference to libazul.
- Struct-by-value calls silently become pointer passes: verified with cobc 3.2 — 'CALL ... USING BY VALUE <group-item>' produces 'warning: BY CONTENT assumed' and passes an address. On ARM64/x86-64, structs ≤16 bytes are passed in registers, so following the copybook's SIGNATURE comments (functions.rs:115-126) for e.g. AzRefAny/AzAppConfig-by-value args feeds a pointer where registers are expected → garbage or crash for a completely by-the-book user.
- Struct returns unsupported: GnuCOBOL CALL...RETURNING cannot arrange sret hidden-pointer returns; every constructor returning an Az struct by value (AzApp_create azul.h:45751 returns 16-byte AzApp; AzString_fromUtf8 returns AzString) corrupts or truncates the return. managed.rs:7-9 admits this yet the copybook still emits 'RETURNING USAGE TYAZ-...' banners with no warning per function.
- No destructor automation and no double-free guard: ~200 types listed in the MANUAL CLEANUP block (azul.cpy:64686+) require manual FN-*-DELETE calls; the NULL-check FREE-AZ-APP pattern (wrappers.rs:69-73) is comment-only documentation, so leaks on any early exit and double-frees on repeated CALL are entirely on the user.
- Callback reentrancy hazard: the suggested ENTRY-paragraph host-invoker scaffolding (managed.rs:44-62) has azul's event loop re-entering the GnuCOBOL runtime via ENTRY symbols with a fixed OCCURS 256 handle table, no overflow handling, and no cob_init/thread-affinity guidance — the GnuCOBOL runtime is not thread-safe.


**Idiomatic-ness issues:**
- Hash-suffix mangling makes half the API unguessable: 30-char truncation + FNV suffix (mod.rs:188-203) yields FN-AZ-ICON-PROVIDER-HANDL-6f00 vs -7bb3 vs -1ec6 for create/withResolver/setResolver — a COBOL author cannot tell them apart without scrolling to the comment. A curated abbreviation table (ICON-PROVIDER → ICOPRV) would be the idiomatic mainframe answer.
- Every tagged-union record names its discriminant just 'TAG' (types.rs:367), forcing qualified references (TAG OF WS-FOO) everywhere; COBOL convention is prefixed unique field names.
- Union payloads exposed as raw PIC X(64) bytes are not usable COBOL — the idiom would be per-variant REDEFINES groups (which the docs falsely claim exist).
- Single flat 65k-line, 3.2MB copybook COPY'd into WORKING-STORAGE injects ~1,889 typedefs and thousands of level-78s into every program (~4s compile overhead measured); splitting into azul-types/azul-functions/azul-widgets copybooks would fit COBOL norms.
- No ON EXCEPTION guidance for dynamic CALL failures — a missing symbol aborts the run; idiomatic GnuCOBOL FFI documents CALL ... ON EXCEPTION handlers.
- Positives worth keeping: level-78 C-symbol constants preserving linker case is genuinely good GnuCOBOL FFI style, and the reserved-word sanitizer (mod.rs:233-333) is thorough.


**Ergonomics issues:**
- hello-world.cob (29 lines) does nothing but DISPLAY constants — no window, no callback; vs the ~35-40-line real counter apps in examples/python and examples/rust. There is no path to a comparable COBOL app today.
- A real counter app would need ~200 LOC of user-written handle-table + ENTRY-paragraph scaffolding (per README and scripts/e2e_language_matrix.md:958-959) on top of manually flattening every struct argument — and even then hits the struct-by-value/sret ABI wall.
- Everything is stringly/manual: build AzString via AzString_fromUtf8 (itself struct-returning, i.e. broken), no helper to convert PIC X fields to AzString or back, no Vec/Option helpers.
- No Makefile despite README promising one; user must know the exact cobc flag set (-x -free -I. -L. -lazul).


**Completeness:** Smoke-only, below the counter bar. The copybook is declarative surface only: constants + record typedefs + doc comments; zero executable wrapper code. Callbacks: libazul-side host-invoker hooks are aliased (AzApp_setCallbackInvoker etc., azul.cpy:64625+) but the COBOL-side wiring exists only as a comment block that references a nonexistent constant and uses invalid syntax — no callback has ever run e2e (scripts/e2e_language_matrix.md:65 'smoke-tier → FAILS counter'). Widgets: names/constants are emitted (all widget callbacks appear) but unusable through the broken struct ABI. No automatic String/Vec/Option conversion of any kind — consistent with the repo-wide auto_conversion_audit. The e2e recipe (e2e_language_matrix.sh:960-973) compiles and runs the DISPLAY-only program and calls that a pass.


**Blockers to ship:**
- Counter hello-world e2e cannot pass: GnuCOBOL cannot express the C ABI the binding requires — struct-by-value args are silently downgraded to BY CONTENT (verified with cobc 3.2) and struct returns (AzApp_create, AzString_fromUtf8) have no sret support. Needs a generated pointer-only C shim layer or byref wrapper variants in libazul; this is a codegen feature, not polish.
- Tagged-union records are ABI-wrong for every union: 4-byte TAG vs uint8_t C tag, and fixed PIC X(64) payload undersized/misaligned (types.rs:325,367,378) — any program touching a union or a struct embedding one corrupts memory. Must emit correct tag width and computed payload sizes.
- Working callback scaffolding does not exist: the only documented pattern (managed.rs:44-62 / azul.cpy:64606-64623) references a nonexistent constant and invalid syntax, so no button click can ever reach user code.
- No hello-world guide page exists (doc/guide/en) and the api.json install section would need an honest description; currently it implies a GUI app that the shipped example explicitly disclaims.


**Quick wins (<1 day):**
- Fix examples/cobol/README.md: hello-world.cbl → hello-world.cob, remove the phantom Makefile entry (or add a 3-line Makefile).
- Fix managed.rs:59-62 scaffolding doc: use the real mangled constant FN-AZ-APP-SET-HOST-HANDLE-9778 and valid syntax (SET WS-PTR TO ENTRY "azul-releaser" then CALL ... BY VALUE WS-PTR), then regenerate.
- Delete/replace the stale REDEFINES claims in mod.rs:19-21 and types.rs module docs so the copybook stops describing a layout it doesn't emit.
- Change union TAG emission to BINARY-CHAR UNSIGNED (types.rs:325,367) so tag reads at least match the uint8_t ABI, and bump/annotate the payload-anchor comment that falsely claims 32-byte max.
- Add an honest smoke-tier note to the api.json cobol install description ('console FFI smoke test; full GUI requires hand-written ENTRY paragraphs — see README'), matching the truthful-install-docs precedent from commit 01293b05a.
- Mark struct-by-value/struct-returning functions in the SIGNATURE banners as 'NOT DIRECTLY CALLABLE FROM COBOL' in functions.rs so users don't walk into UB.


**Verdict:** No-ship: the install steps run but deliver a console printout, and beneath the surface every tagged union and every struct-by-value call is ABI-broken, so the counter e2e is unreachable without a generated pointer-only shim layer (~2 weeks of codegen work). Spend one quick-win day on doc truthfulness now and keep COBOL as an honestly-labeled smoke-tier curiosity until a shim backend is justified by demand.


## vb6 — blocked (install 1/5, ~12d to ship-quality)

A fresh developer curls azul.i686.dll, Azul.bas, HelloWorld.bas and HelloWorld.vbp (all four ARE published by the release pipeline, doc/src/dllgen/deploy.rs:805-807 and rust.yml:2688) and then hits `vb6.exe /make HelloWorld.vbp` — which fails unless they own the discontinued, license-gated VB6 IDE; the documented fallback `vbc.exe` is the VB.NET compiler and cannot build this project at all. If they do own VB6, HelloWorld.bas compiles (it inlines its own Declares and never uses the downloaded Azul.bas), but the process crashes at the first FFI call: every Declare is stdcall against the DLL's cdecl exports, AzRefAny_newC is declared with 5 args where the real export takes 8 (target/codegen/dll_api_external.rs:58447), and struct-by-value returns like AzString are declared As Long (dll_api_external.rs:60065), so the sret pointer protocol corrupts the stack immediately. No window, no counter — and this has never been compiled or run anywhere (scripts/e2e_language_matrix.sh:1174-1176 always SKIPs off-Windows; Windows CI has no VB6.EXE either).


**Guide/install truthfulness issues:**
- api.json vb6.windows description claims "Use the VB6 IDE or vbc.exe" — false: vbc.exe is the VB.NET compiler and cannot compile .vbp projects, Attribute lines, or As-Any Declares. The false claim is repeated in the generated Azul.bas header ("compile via vbc.exe", doc/src/codegen/v2/lang_vb6/mod.rs:164), Azul.vbp line 1 (vbp.rs), and examples/vb6/HelloWorld.bas:2 ("vbc.exe /out:HelloWorld.exe HelloWorld.bas").
- Install step 3 downloads Azul.bas, but HelloWorld.vbp:9 references only `Module=HelloWorld; HelloWorld.bas` — Azul.bas is never used. Worse, if the user adds it, it cannot compile: 8,034 `Declare Function ... As Az<UDT>` lines (VB6 forbids UDT returns from Declare) and Public Type fields referencing callback types that exist only as comments (e.g. target/codegen/vb6/Azul.bas:14041 `cb As AzButtonOnClickCallbackType` vs the type only appearing in a SKIPPED comment at :16280).
- The install steps implicitly claim the hello world runs. It cannot: examples/vb6/HelloWorld.bas declares AzRefAny_newC with 5 args (line 71-73) vs the real 8-arg export (dll_api_external.rs:58447), treats AzString/AzDom/AzApp struct-by-value returns as Long, and passes AzApp_run's WindowCreateOptions as a pointer where the C ABI takes it by value (dll_api_external.rs:61031). scripts/e2e_language_matrix.md:73 confirms it is a permanent SKIP.
- examples/vb6/HelloWorld.bas:97-98 claims passing UTF-16 BSTR bytes as the string payload is "fine" for ASCII — false: UTF-16LE ASCII has interleaved NUL bytes, so even the "Increase counter" label would arrive as garbage in a UTF-8-expecting AzString.
- Azul.bas header (mod.rs:157-160) says the binding "will ONLY work against a 32-bit azul.dll" — implying it works with one; it does not (cdecl/stdcall mismatch plus UDT-by-value ABI holes affect every call). By contrast, examples/vb6/README.md is honest: "32-bit Windows niche, out of scope"; api.json does not carry that disclaimer.


**Safety issues:**
- Delete-on-uninitialized-memory by design: for every constructor returning a struct by value (i.e. essentially ALL Azul constructors), the generated Init sub emits only SKIPPED comments but STILL sets m_owned = True (doc/src/codegen/v2/lang_vb6/wrappers.rs:296-316; artifact target/codegen/vb6/App.cls:45-51), so Class_Terminate (wrappers.rs:158-167, App.cls:26-31) calls AzXxx_delete on a zero-initialized record — Rust drop of garbage for any class a user instantiates and releases.
- Calling-convention mismatch on every call: azul.dll i686 exports are extern "C" (cdecl, dll_api_external.rs / dll/src/lib.rs), but VB6 Declare is stdcall-only. All 13,251 generated Declares corrupt the stack or raise error 49; lang_vb6/functions.rs has no calling-convention handling at all (grep for stdcall/cdecl finds nothing).
- Callback ABI is inexpressible: AzCallbackType = extern "C" fn(AzRefAny, AzCallbackInfo) -> AzUpdate — structs BY VALUE, cdecl (dll_api_external.rs:404). VB6 AddressOf produces stdcall procedures that cannot receive UDTs by value; the generated code just comments "Pass the address of a Public Function via AddressOf" (Azul.bas:16255-16264), which guarantees stack corruption on the first click. No host-invoker shim exists for vb6 (unlike the 15 managed languages).
- Struct-by-value args silently declared ByRef: functions.rs:165-168 admits "this changes the C ABI shape — caller must verify" — e.g. AzApp_run declared (ByVal app As Long, ByRef root_window As AzWindowCreateOptions) at Azul.bas:30897 against a by-value C signature; every such call is a stack-corrupting ABI mismatch.
- WrapRaw double-free: wrappers.rs:172-180 byte-copies an existing record and sets m_owned = True without invalidating the source, so both copies run _delete on the same interior pointers.
- u64→Currency silent 10000x scaling (mod.rs:246-255): AzRefAny_newC's type_id is declared As Currency (Azul.bas:19172), so RefAny type ids are silently multiplied/divided by 10,000 — type-confusion in downcasts even if everything else worked.
- Init ignores null constructor returns but claims ownership anyway (wrappers.rs:308-316): If ret_ = 0 the record stays zeroed yet m_owned = True → delete-on-garbage at Terminate.


**Idiomatic-ness issues:**
- No VB6 error handling anywhere: no Err.Raise on null/failed FFI results; failures are silent (wrappers.rs emit_init_sub / emit_method).
- Clone method leaks the raw C self-arg into the VB6 signature: `Public Function Clone(ByVal instance As Long) As AzApp` while ALSO passing VarPtr(m_raw) (App.cls:73-75) — visible_user_args (wrappers.rs:403-409) filters only "self"/classname, not "instance".
- Callback signature doc-comments have blank parameter names: "ByVal  As AzRefAny" (Azul.bas:16259 etc.) — the arg-name field is empty in the IR emission.
- Flat 465-file project: Azul.vbp lists every .cls (target/codegen/vb6/Azul.vbp); loading 465 class modules into the VB6 IDE for a hello world is hostile even by VB6 standards. The Class_Initialize/Class_Terminate RAII pattern and PascalCase naming are, to be fair, correctly idiomatic in spirit.
- The shipped example abandons the binding's own idioms entirely: HelloWorld.bas uses raw CopyMemory pointer surgery and hand-written Declares instead of the generated classes (which it cannot use, since they don't work).


**Ergonomics issues:**
- hello-world is 199 lines (examples/vb6/HelloWorld.bas) vs the ~35-40 line Python/Rust reference; ~80 of those lines are hand-duplicated Declare statements because the generated Azul.bas is unusable.
- The example itself contains three explicit SKIPPED admissions: no typed RefAny downcast (line 106-109), no upcast/AZ_REFLECT equivalent (178-180), no window title/size/flags access (191-194).
- Everything is Long-as-pointer; reading any struct field requires manual CopyMemory with hand-computed offsets. No String/Vec/Option conversion helpers exist; the one string helper (AzStr, line 99-101) is byte-level wrong.
- i64/u64 values require manual divide-by-10000 Currency arithmetic per the codegen's own comment (mod.rs:249-254).


**Completeness:** Below smoke-only: never compiled by any CI or human (e2e always SKIPs — scripts/e2e_language_matrix.sh:1174, matrix row 73). Callbacks are architecturally impossible (cdecl struct-by-value vs stdcall AddressOf; no host-invoker shim). All ~465 widget classes are emitted but every constructor is a SKIPPED comment stub (172 of 465 .cls files contain SKIPPED; Azul.bas has 11,095 SKIPPED markers against 13,251 Declares). No automatic String/Vec/Option conversion. The counter e2e bar is not close; it is unreachable with the current architecture.


**Blockers to ship:**
- Counter e2e cannot pass with the current architecture: azul's callback ABI (extern "C" cdecl, AzRefAny/AzCallbackInfo passed by value — dll_api_external.rs:404) is inexpressible from VB6 (stdcall-only AddressOf, no UDT-by-value). Requires a generated C shim/host-invoker DLL with stdcall out-pointer exports that does not exist.
- Generated Azul.bas does not compile in VB6: 8,034 Declares return UDTs by value (illegal in VB6 Declare) and Public Type fields reference *CallbackType names that are only emitted as comments (Azul.bas:14041 vs :16280).
- examples/vb6/HelloWorld.bas is ABI-fiction: wrong arity for AzRefAny_newC (5 vs 8 args), struct returns declared As Long, cdecl/stdcall mismatch — it would crash at the first call even with the correct 32-bit dll present.
- No VB6 toolchain exists on any CI runner or maintainer machine (discontinued, license-gated, 32-bit-Windows-only) — the frontpage e2e bar can never be honestly verified.
- No doc/guide/en/hello-world page exists for vb6, and the api.json install steps contain the false vbc.exe claim.


**Quick wins (<1 day):**
- Remove or fix the "or vbc.exe" claim in api.json ['0.2.0'].installation.languages.vb6, mod.rs:164 (Azul.bas header), vbp.rs (Azul.vbp header), and HelloWorld.bas:2 — vbc.exe is VB.NET.
- Propagate the honest examples/vb6/README.md disclaimer ("out of scope, kept for users who specifically need VB6") into the api.json description, or remove vb6 from the installation languages list entirely so the frontpage never implies it works.
- Drop the unused `curl Azul.bas` install step (HelloWorld.vbp never references it).
- wrappers.rs:316 — stop setting m_owned = True in SKIPPED Init paths; removes the delete-on-uninitialized-memory hazard from every generated class.
- wrappers.rs:403-409 — add "instance" to the self-arg filter so Clone() stops exposing a raw pointer parameter.
- Fix the blank argument names in callback signature comments (Azul.bas "ByVal  As AzRefAny").
- Fix the example's AzStr helper to do a real UTF-16→UTF-8 conversion instead of passing raw BSTR bytes.


**Verdict:** No-ship, and realistically never-ship: the binding is an explicitly acknowledged codegen demonstration (mod.rs:7 — "the audience for this binding is essentially zero") whose generated code cannot compile, whose example cannot run, and whose callback ABI is fundamentally inexpressible in VB6 without a new stdcall C-shim layer plus a toolchain no CI can legally run. Keep the artifacts and the honest README, but scrub the vbc.exe claim and the works-if-you-follow-these-steps framing from api.json.


## algol68 — blocked (install 1/5, ~20d to ship-quality)

A user follows the api.json macOS steps: brew install algol68g works, the three curl downloads work, then `DYLD_LIBRARY_PATH=. a68g hello-world.a68` aborts immediately with "a68g: abend: not enough memory" because the 2.5 MB / 31k-line azul.a68 include exceeds a68g's default heap. If they discover `--heap=512M` on their own, they instead get thousands of warnings ("ignoring trailing character _") followed by hard syntax errors on every one of the 13,700+ `ALIEN "AzFoo_bar" ! "azul"` declarations: a68g 3.11.3 has no ALIEN keyword and no dynamic FFI of any kind. No window ever opens; nothing can be salvaged by the user because the binding targets a foreign-function syntax that no available Algol 68 implementation supports. The example README (examples/algol68/README.md) honestly admits this; the frontpage install steps do not.


**Guide/install truthfulness issues:**
- api.json ['0.2.0'].installation.languages.algol68 final steps ('LD_LIBRARY_PATH=. a68g hello-world.a68' / 'DYLD_LIBRARY_PATH=. a68g hello-world.a68' / 'a68g hello-world.a68') promise a working run — false. Verified today with a68g 3.11.3: default heap aborts with 'abend: not enough memory' on the 2.5 MB include; with --heap=512M it fails with 'syntax error: tag "ALIEN" has not been declared properly' on every FFI declaration (target/codegen/azul.a68:11147ff).
- Generated header claim (target/codegen/azul.a68 lines 5-7): 'TARGET: Algol 68 Genie (a68g) >= 3.0 ... the ALIEN declarations below are an a68g-specific extension' — false. No version of a68g implements an ALIEN foreign-function extension; a68g has no dynamic C FFI at all.
- doc/src/codegen/v2/lang_algol68/mod.rs:113-120 claims 'a68g >= 3.5 deprecated the `alien convention` PRAGMAT and exposes foreign symbols via implicit linkage (the runtime dlopens the process's loaded libraries and resolves ALIEN PROC symbols by name)' — fabricated; no such mechanism ever existed in a68g.
- mod.rs:333-334 and examples/algol68/hello-world.a68:17-18 claim 'a68g's runtime marshals STRING to a null-terminated char* automatically' when calling an ALIEN PROC — unverifiable/false, since the FFI it describes does not exist.
- Windows install description says to download the a68g Windows build from jmvdveer.home.xs4all.nl but the steps themselves never install the toolchain (api.json algol68.windows).
- The one honest document is examples/algol68/README.md ('a68g ... rejects the codegen's PROC ... ALIEN ... syntax — that's a different a68 dialect') — verified true today; scripts/e2e_language_matrix.md:69,110 also correctly records the failure and the a68g OOM.


**Safety issues:**
- Whole-binding: even if ALIEN parsed, a68g is an interpreter whose STRUCT values are internal representations, not C-ABI memory; every by-value struct argument/return (e.g. target/codegen/azul.a68:11147 passes AZLAYOUTSIZE by value) would be memory corruption. Type map in doc/src/codegen/v2/lang_algol68/mod.rs:352-368 pretends layout compatibility (u8->CHAR, u16->SHORT INT, f32->REAL where a68g REAL is a C double, u64->LONG INT where a68g LONG INT is software multiprecision).
- Callback MODEs are plain Algol 68 PROC modes (azul.a68:2202-2247, e.g. MODE AZREFANYDESTRUCTORTYPE = PROC (REF VOID) VOID) handed directly to C function-pointer slots with no trampoline; an interpreted a68g PROC value is not a C function pointer — instant crash if it ever linked (codegen: doc/src/codegen/v2/lang_algol68/types.rs callback emission).
- Manual-delete convention with no use-after-free protection: wrappers.rs:81-94 emits `delete az <type> = (REF <TYPE> value) VOID: ALIEN "Az<Type>_delete"` and only a comment says 'Caller must NOT use the value afterwards'; the REF is not nilled, so double-delete and use-after-free are one typo away, and everything leaks unless the user calls delete by hand (wrappers.rs:64-67 admits a68g GC does not track ALIEN resources).
- examples/algol68/hello-world.a68:74-80 calls `az ref any new c` with 5 arguments against an 8-parameter declaration (azul.a68:12349: ptr, len, align, type_id, type_name, destructor, serialize_fn, deserialize_fn) and passes `LONG INT (LWB model)` as the size — LWB of a STRUCT is invalid and the comment admits it is a placeholder; if this ever ran it would construct a RefAny with a garbage size.


**Idiomatic-ness issues:**
- Reserved-word sanitization (mod.rs:228-244) appends '_' (end -> end_), but a68g warns 'ignoring trailing character "_" in identifier' and strips it, producing warning spam on every use (azul.a68:3036,4056,4151...) and silently un-doing the collision avoidance.
- Single flat 2.5 MB / 31,169-line include file with no module split; a68g cannot even parse it under its default heap (needs undocumented --heap=512M). Algol 68 has no module system in a68g, but the file could be pruned to the used API surface.
- Positive: identifier conventions are genuinely thoughtful — lowercase-with-spaces PROC names (`az dom add child`), UPPERCASE MODEs, digits spelled out (AZUEIGHTVEC) because bold words are letters-only, matching-# comments; if the FFI existed this would read as plausible Algol 68.
- Error handling: none — raw ALIEN declarations only, no result checking, no NIL checks; hundreds of '# SKIPPED: enum ...VecDestructor #' type gaps (azul.a68:2087-2106).


**Ergonomics issues:**
- hello-world.a68 is 96 lines vs the ~35-40-line python/rust reference, and contains three admitted 'SKIPPED' placeholders (no RefAny downcast wrapper, no upcast/REFLECT macro, sizeof faked with LWB).
- The click callback (hello-world.a68:26-31) never increments the counter and the label is hardcoded to "0" (line 38) — even as aspirational code it does not implement the counter demo.
- Deep field pokes instead of builders: `width OF dimensions OF size OF window state OF window := 400.0` (hello-world.a68:84-85).
- No string/vec/option sugar: users must call `az string copy from bytes (s, 0, UPB s)` for every literal; AZSTRING/AZOPTION*/AZ*VEC are raw structs with no conversion to native STRING/[]MODE.


**Completeness:** Non-functional: the binding does not parse under a68g (the only realistic implementation), so nothing works — not smoke, not callbacks, not widgets. Coverage on paper is broad (13,716 ALIEN declarations incl. Button/widget C functions and paired delete helpers) but there is no callback trampoline, no RefAny downcast/upcast wrappers, and no String/Vec/Option auto-conversion. The codegen's own mod.rs:10-15 states the audience is 'Effectively zero' and the binding exists as a universal-framework showcase.


**Blockers to ship:**
- Algol 68 Genie has no foreign-function interface: a68g 3.11.3 rejects every `ALIEN "sym" ! "azul"` declaration ('tag "ALIEN" has not been declared properly', verified today), and no a68g version implements one. The entire binding targets a nonexistent dialect feature; making it real requires either a compiled a68g plugin/bridge written in C against a68g internals (including callback trampolines and struct marshalling for an interpreter with non-C data layout) or a different, effectively dead compiler (Algol 68RS/a68toc lineage). This is a hard external toolchain blocker — the counter e2e bar is unreachable.
- Even the aspirational hello-world is internally inconsistent (5 args passed to the 8-parameter az ref any new c, invalid LWB-as-sizeof, callback never increments the counter) — there is no reference program to certify even if an FFI appeared.
- api.json install steps currently promise a run that provably fails; they must be removed or rewritten as an honest 'experimental, does not run' notice before any frontpage exposure.


**Quick wins (<1 day):**
- Truth-fix the frontpage: either drop algol68 from api.json installation.languages or replace the final run step with an explicit experimental/non-functional notice mirroring examples/algol68/README.md.
- Fix the false claims baked into the artifact: azul.a68 header lines 5-7 and mod.rs:113-120/333-334 should say the ALIEN syntax is hypothetical (Algol 68RS-style), not an a68g extension.
- Make the hello-world at least self-consistent: correct the az ref any new c arity (8 args, azul.a68:12349), replace the LWB sizeof placeholder with a documented constant, and have the callback increment counter OF the model so the file can serve as the spec for any future bridge.
- Change reserved-word sanitization from trailing '_' (stripped with warnings by a68g) to a suffix a68g keeps, e.g. 'end x' or 'endv' (mod.rs:228-244), and document the required `a68g --heap=512M` in the generated header (default heap aborts on the 2.5 MB include).


**Verdict:** No-ship: hard-blocked — the binding targets an ALIEN FFI syntax that Algol 68 Genie does not implement (verified failing on a68g 3.11.3 today), so no program can ever link, and reaching the counter-e2e bar would mean building a custom C bridge/trampoline layer into an interpreter (~15-25 days, questionable value given the codegen's own 'audience: effectively zero' note); keep it as an honest showcase but remove the promising install steps from the frontpage data.

---

## Addendum (2026-07-04, post-fix): Az-prefix idiomatics — state + emitter gap catalog

Examples rewritten to the non-prefixed idiomatic style and re-verified green:
cpp03/cpp14/cpp17/cpp20/cpp23 (wrapper classes + AZ_REFLECT), scala (typed
LayoutCallback SAM, no Structure byte-splicing). Already idiomatic: cpp11,
rust, python, lua, ruby, node, csharp/java/kotlin/ocaml (at their artifact's
ceiling). C-ABI by design: c, zig, fortran. Cannot cover yet: go, pascal.

Emitter gaps that still force Az-prefixed types into examples (all verified,
fix = codegen work):
1. C++ enum CONSTANTS not aliased (`AzUpdate_RefreshDom`) — lang_cpp/cpp11.rs:598, cpp14.rs:87, cpp17.rs:819, cpp20.rs:824, cpp03.rs:675ff; fix = enum class re-emission or inline constexpr aliases.
2. C++ callback registration takes raw fn-ptr typedefs — lang_cpp/common.rs:663-676.
3. C++23 toStdExpected / C++17 toStdOptional yield raw Az payloads — lang_cpp/cpp20.rs:689ff, :356-360.
4. C# enums Az-prefixed inside `namespace Azul` — lang_csharp/types.rs:97,:266.
5. Java/Kotlin/Scala enums Az-prefixed — lang_java/mod.rs:153; lang_kotlin/wrappers.rs:475, managed.rs:310.
6. Go wrappers leak C.Az* in every signature; NO callback trampolines emitted — lang_go/wrappers.rs:8-25; counter example must stay cgo-direct until a trampoline layer exists.
7. Pascal wrapper classes take/return raw TAz* records + ctor naming collides (TDom.CreateCreateBody) — lang_pascal/wrappers.rs:122/193/218.
8. OCaml lacks non-prefixed enum constants — azul.mli az_update_variant_* etc.
9. Lisp WCO wiring discards host-invoker ctx (raw fn ptr only) — lang_lisp/mod.rs:~266.
10. Haskell idiomatic module lacks static ctors + refany create/get (raw c_Az*_via only) — lang_haskell/; the green example uses raw _via calls for this reason.
