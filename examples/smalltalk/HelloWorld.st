| testResult |
Transcript show: '[azul] Pharo FFI smoke test starting.'; cr.

FFILibrary subclass: #LibAzulSmoke
    instanceVariableNames: ''
    classVariableNames: ''
    package: 'AzulSmokeTest'.
LibAzulSmoke compile: 'macLibraryName ^ ''libazul.dylib'' '.
LibAzulSmoke compile: 'unixLibraryName ^ ''libazul.so'' '.
LibAzulSmoke compile: 'win32LibraryName ^ ''azul.dll'' '.
Transcript show: '[azul] LibAzulSmoke <: FFILibrary defined.'; cr.

testResult := LibAzulSmoke uniqueInstance libraryName.
Transcript show: '[azul] FFILibrary resolved library name: ', testResult; cr.

Transcript show: '[azul] Pharo binding init phase completed successfully.'; cr.
Transcript show: '[azul] (Full Tonel-package load of the codegen Azul.st needs the'; cr.
Transcript show: '[azul]  codegen to emit a Tonel package directory layout, which is'; cr.
Transcript show: '[azul]  what would surface the FFIExternalStructure declarations.)'; cr.
1
