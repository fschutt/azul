{ Pascal: no optional type of its own, so an Option is built with the
  exports (`AzOptionString_none`, `AzOptionString_some`) and read with
  AzTryGetSome / AzTryGetOk / AzTryGetErr. Built and run by
  scripts/native_optionals_smoke.sh. }
program NativeOptionals;

{$mode delphi}{$H+}

uses
  SysUtils, Azul;

type
  TMyModel = class
    Folder: string;
  end;

{ The guide's file-dialog example, as written there (hello-world/pascal.md). }
function OnPickFolder(Model: TMyModel; Info: TAzCallbackInfo; Result_: TAzRefAny): TAzUpdate;
var
  Picked: TAzOptionFileOpenResult;
  Opened: TAzFileOpenResult;
  Path: TAzFilePath;
begin
  Result := azDoNothing;
  { The result is lent to the callback: downcast a clone of it. }
  Picked := AzFileOpenResult_downcast(AzRefAny_clone(@Result_));
  if AzTryGetSome(Picked, Opened) and AzTryGetSome(Opened.path, Path) then
  begin
    Model.Folder := azul_string_to(Path.inner);
    Result := azRefreshDom;
  end;
  AzOptionFileOpenResult_delete(@Picked);
end;

function OnChooseFolder(Model: TMyModel; Info: TAzCallbackInfo): TAzUpdate;
begin
  AzFileDialog_openDirectory(azul_string_from('Choose a folder'), AzOptionString_none,
    azul_refany_create(Model),
    azul_register_resumecallback(TAzResumeCallbackTypedWrapper<TMyModel>.Create(OnPickFolder)));
  Result := azDoNothing;
end;

procedure Check(Ok: Boolean; const What: string);
begin
  if not Ok then
  begin
    WriteLn(StdErr, 'FAIL: ', What);
    Halt(1);
  end;
end;

var
  Icons: TAzIconVariants;
  Light: TAzString;
  Parsed: TAzResultUrlUrlParseError;
  Url: TAzUrl;
  Err: TAzUrlParseError;
begin
  { A field: None in, None out; Some in, Some out. }
  Icons := AzIconVariants_createDefault;
  Check(not AzTryGetSome(Icons.light, Light), 'a default IconVariants has no light icon');
  AzOptionString_delete(@Icons.light);
  Icons.light := AzOptionString_some(azul_string_from('light.png'));
  Check(AzTryGetSome(Icons.light, Light) and (azul_string_to(Light) = 'light.png'), 'Some in, Some out');
  AzOptionString_delete(@Icons.light);
  Icons.light := AzOptionString_none;
  Check(not AzTryGetSome(Icons.light, Light), 'None in, None out');
  AzIconVariants_delete(@Icons);

  { A result: Ok and Err. }
  Parsed := AzUrl_parse(azul_string_from('https://azul.rs/guide'));
  Check(AzTryGetOk(Parsed, Url) and not AzTryGetErr(Parsed, Err), 'Url_parse Ok');
  AzResultUrlUrlParseError_delete(@Parsed);
  Parsed := AzUrl_parse(azul_string_from('no scheme'));
  Check(AzTryGetErr(Parsed, Err) and not AzTryGetOk(Parsed, Url), 'Url_parse Err');
  AzResultUrlUrlParseError_delete(@Parsed);

  if False then OnChooseFolder(nil, Default(TAzCallbackInfo));
  WriteLn('pascal native optionals: ok');
end.
