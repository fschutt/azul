//! Emits a minimal Lazarus project (`.lpi`) file for the Pascal example.
//!
//! Lazarus projects are XML files that the IDE consumes; FPC's
//! command-line build only needs the `.pas` file, but shipping a `.lpi`
//! makes opening the example in Lazarus a one-click affair.
//!
//! The generated `.lpi` is intentionally minimal — it references a single
//! `.pas` main program (e.g. `hello-world.pas`) and configures Free
//! Pascal Compiler in Object Pascal mode (`-Mobjfpc`). It does *not*
//! depend on the Lazarus Component Library (LCL); azul provides its own
//! windowing, so the example is a console program from Lazarus' point of
//! view.

/// Emit a Lazarus `.lpi` for an example named `<name>` (without
/// extension). Assumes there is a sibling `<name>.pas` file containing a
/// `program <name>;` declaration.
pub fn generate_lpi(name: &str) -> String {
    // `lpi` files are XML; we hard-code the canonical shape Lazarus
    // produces for a freshly-created console program. Re-running the
    // generator yields a deterministic file (no timestamps).
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<CONFIG>
  <ProjectOptions>
    <Version Value="12"/>
    <PathDelim Value="\"/>
    <General>
      <Flags>
        <CompatibilityMode Value="True"/>
      </Flags>
      <SessionStorage Value="InProjectDir"/>
      <Title Value="{name}"/>
      <UseAppBundle Value="False"/>
      <ResourceType Value="res"/>
    </General>
    <BuildModes Count="1">
      <Item1 Name="Default" Default="True"/>
    </BuildModes>
    <PublishOptions>
      <Version Value="2"/>
      <UseFileFilters Value="True"/>
    </PublishOptions>
    <RunParams>
      <FormatVersion Value="2"/>
    </RunParams>
    <Units Count="2">
      <Unit0>
        <Filename Value="{name}.pas"/>
        <IsPartOfProject Value="True"/>
      </Unit0>
      <Unit1>
        <Filename Value="azul.pas"/>
        <IsPartOfProject Value="True"/>
        <UnitName Value="Azul"/>
      </Unit1>
    </Units>
  </ProjectOptions>
  <CompilerOptions>
    <Version Value="11"/>
    <Target>
      <Filename Value="{name}"/>
    </Target>
    <SearchPaths>
      <IncludeFiles Value="$(ProjOutDir)"/>
    </SearchPaths>
    <Parsing>
      <SyntaxOptions>
        <SyntaxMode Value="ObjFPC"/>
        <CStyleOperator Value="False"/>
        <AllowLabel Value="False"/>
        <CPPInline Value="False"/>
      </SyntaxOptions>
    </Parsing>
    <Linking>
      <Options>
        <Win32>
          <GraphicApplication Value="False"/>
        </Win32>
      </Options>
    </Linking>
  </CompilerOptions>
  <Debugging>
    <Exceptions Count="3">
      <Item1>
        <Name Value="EAbort"/>
      </Item1>
      <Item2>
        <Name Value="ECodetoolError"/>
      </Item2>
      <Item3>
        <Name Value="EFOpenError"/>
      </Item3>
    </Exceptions>
  </Debugging>
</CONFIG>
"#,
        name = name
    )
}
