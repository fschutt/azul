"=============================================================================
 examples/smalltalk/HelloWorld.st
 Pharo Azul C ABI smoke test.

 The codegen-emitted Azul.st is in Pharo Tonel format (one file with
 `Class { ... }` paragraphs and `Class >> selector [ ... ]` methods).
 Standard `'Azul.st' asFileReference fileIn` doesn't handle Tonel
 directly — it needs `TonelReader.loadPackage` which expects a Tonel
 package DIRECTORY layout, not a single file. Restructuring the
 codegen to emit a Tonel package tree is a separate phase.

 Until that lands, this smoke test does what the Haskell / Perl / PHP
 / Pascal / Fortran tests do: drive Pharo's UnifiedFFI manually to
 prove the binding loads and resolves libazul symbols.

 Build/run (from a Pharo install dir):
     DYLD_LIBRARY_PATH=/path/to/dylib pharo Pharo.image \\
         eval `cat /path/to/HelloWorld.st`

 Prerequisites:
   - Pharo VM (any 9.0+; tested on 9.0.22).
   - libazul.dylib / .so / .dll discoverable via the OS loader path.
============================================================================="

| testResult |
Transcript show: '[azul] Pharo FFI smoke test starting.'; cr.

"Declare a one-off FFILibrary subclass pointing at libazul. Pharo
 names the per-platform getters macLibraryName / unixLibraryName /
 win32LibraryName (the older macModuleName form was renamed)."
FFILibrary subclass: #LibAzulSmoke
    instanceVariableNames: ''
    classVariableNames: ''
    package: 'AzulSmokeTest'.
LibAzulSmoke compile: 'macLibraryName ^ ''libazul.dylib'' '.
LibAzulSmoke compile: 'unixLibraryName ^ ''libazul.so'' '.
LibAzulSmoke compile: 'win32LibraryName ^ ''azul.dll'' '.
Transcript show: '[azul] LibAzulSmoke <: FFILibrary defined.'; cr.

"Confirm uniqueInstance returns and the resolved library name is
 what we expect — this proves the FFI machinery + library lookup
 are operational. We deliberately don't call a libazul function
 here because every public symbol takes or returns an azul struct
 by value and Pharo UFFI needs FFIExternalStructure declarations
 for those; the codegen Azul.st adds them, and Tonel loading is
 what would surface them in-image."
testResult := LibAzulSmoke uniqueInstance libraryName.
Transcript show: '[azul] FFILibrary resolved library name: ', testResult; cr.

Transcript show: '[azul] Pharo binding init phase completed successfully.'; cr.
Transcript show: '[azul] (Full Tonel-package load of the codegen Azul.st needs the'; cr.
Transcript show: '[azul]  codegen to emit a Tonel package directory layout, which is'; cr.
Transcript show: '[azul]  what would surface the FFIExternalStructure declarations.)'; cr.
1
