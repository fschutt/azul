//! VB6 Project file (`.vbp`) emission.
//!
//! A `.vbp` is a flat INI-like file the VB6 IDE uses to load a
//! project. It enumerates every `.bas` (BAS module), `.cls` (Class
//! module), `.frm` (Form), `.ctl` (UserControl), and references the
//! VB6 runtime + any external typelibs.
//!
//! The minimum-viable shape is:
//!
//! ```text
//! Type=Exe
//! Reference=*\G{00020430-...}#... (stdole2.tlb)
//! Module=Azul; Azul.bas
//! Class=App; App.cls
//! Class=Window; Window.cls
//! Startup="Sub Main"
//! ExeName32="Azul.exe"
//! Title="Azul"
//! ```
//!
//! VB6 IDE accepts this minimal shape; it auto-fills the rest on
//! first save. The generated `.vbp` is intended as a starting point
//! the user can extend with their own forms.

/// Generate the `.vbp` body for a project that includes `Azul.bas`
/// plus one `.cls` per disposable type. The project is configured
/// as a `Type=Exe` (executable) project; users targeting a
/// `Type=OleDll` (in-process ActiveX server) can change this line
/// manually.
pub fn generate_vbp(class_names: &[String]) -> String {
    let mut out = String::new();
    out.push_str("' VB6 Project file. Open with VB6.EXE or compile with vbc.exe.\n");
    out.push_str("' === 32-BIT ONLY === This project requires 32-bit azul.dll.\n");
    out.push_str("Type=Exe\n");

    // Reference to stdole2.tlb (every VB6 project uses this).
    out.push_str("Reference=*\\G{00020430-0000-0000-C000-000000000046}#2.0#0#..\\..\\Windows\\system32\\stdole2.tlb#OLE Automation\n");

    // Main module.
    out.push_str("Module=Azul; Azul.bas\n");

    // One Class= entry per .cls file.
    for class in class_names {
        out.push_str(&format!("Class={}; {}.cls\n", class, class));
    }

    // Startup tells the IDE which Sub to invoke when running. The
    // user is expected to provide a `Sub Main` somewhere in their
    // application code (Azul.bas does not declare one because it is
    // a library module).
    out.push_str("Startup=\"Sub Main\"\n");
    out.push_str("ExeName32=\"Azul.exe\"\n");
    out.push_str("Title=\"Azul\"\n");
    out.push_str("Command32=\"\"\n");
    out.push_str("Name=\"Azul\"\n");
    out.push_str("HelpContextID=\"0\"\n");
    out.push_str("CompatibleMode=\"0\"\n");
    out.push_str("MajorVer=1\n");
    out.push_str("MinorVer=0\n");
    out.push_str("RevisionVer=0\n");
    out.push_str("AutoIncrementVer=0\n");
    out.push_str("ServerSupportFiles=0\n");
    out.push_str("VersionCompanyName=\"Azul GUI\"\n");
    out.push_str("CompilationType=0\n");
    out.push_str("OptimizationType=0\n");
    out.push_str("FavorPentiumPro(tm)=0\n");
    out.push_str("CodeViewDebugInfo=0\n");
    out.push_str("NoAliasing=0\n");
    out.push_str("BoundsCheck=0\n");
    out.push_str("OverflowCheck=0\n");
    out.push_str("FlPointCheck=0\n");
    out.push_str("FDIVCheck=0\n");
    out.push_str("UnroundedFP=0\n");
    out.push_str("StartMode=0\n");
    out.push_str("Unattended=0\n");
    out.push_str("Retained=0\n");
    out.push_str("ThreadPerObject=0\n");
    out.push_str("MaxNumberOfThreads=1\n");

    out
}
