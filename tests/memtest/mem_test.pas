{ Memory test for the azul Pascal (FPC) binding. See tests/memtest/README.md.

  The harness (scripts/run_memtest.sh) measures peak RSS across a small and a
  large AZ_MEMTEST_N (RSS that scales with N is a LEAK) and fails on any crash.
  This file only exercises the create/consume/DROP paths in a loop and exits 0.
  No event loop (AzApp_run needs a display and hangs headless).

  Build, mirroring examples/pascal:
    fpc -Mobjfpc -Sh -Fl. mem_test.pas && LD_LIBRARY_PATH=. ./mem_test }

program MemTest;

{$mode objfpc}{$H+}
{$PACKRECORDS C}

uses
  ctypes, sysutils,
  Azul;

var
  n, i: Integer;
  model: TObject;
  data: TAzRefAny;
  cfg: TAzAppConfig;
  app: TAzApp;
  envN: string;

begin
  n := 200000;
  envN := GetEnvironmentVariable('AZ_MEMTEST_N');
  if envN <> '' then
    n := StrToIntDef(Trim(envN), 200000);

  { 1. The consume-by-value DROP path: AzApp_create moves the AppConfig bytes
       (nested SystemStyle) into libazul; AzApp_delete drops the App once. }
  model := TObject.Create;
  data := azul_refany_create(model);
  cfg := AzAppConfig_create();
  app := AzApp_create(data, cfg);
  AzApp_delete(@app);

  { 2. Leak loop: create/destroy a droppable AppConfig N times.
       AzAppConfig_delete drops the nested SystemStyle every iteration. }
  for i := 1 to n do
  begin
    cfg := AzAppConfig_create();
    AzAppConfig_delete(@cfg);
  end;

  WriteLn(Format('memtest pascal OK (N=%d)', [n]));
end.
