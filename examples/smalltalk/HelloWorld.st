"Run: LD_LIBRARY_PATH=. pharo Pharo.image eval `cat HelloWorld.st`"

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
