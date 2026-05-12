{ ============================================================================
  Pascal (FPC) full-GUI hello-world. Mirrors examples/c/hello-world.c.

  - Holds a TMyModel with a Counter.
  - Subclasses TAzLayoutCallbackInvoker / TAzCallbackInvoker (one per
    callback kind). The Invoke override is what libazul fires through
    after the per-kind cdecl stub looks the handler up by id.
  - Wires the layout callback into WindowCreateOptions, registers the
    button click handler the same way, and runs the app loop.

  Build:
      fpc -Mobjfpc -Sh -Fl. -k-L. -k-lazul hello-world.pas

  Run (macOS):
      DYLD_LIBRARY_PATH=. ./hello-world
  Run (Linux):
      LD_LIBRARY_PATH=. ./hello-world

  AZ_DEBUG smoke probe (run in another terminal):
      curl -s -X POST localhost:8080/ -d '(op:get_html_string)'
      for _ in 1 2 3; do
        curl -s -X POST localhost:8080/ \
          -d '(op:click,selector:.__azul-native-button)'
      done
      curl -s -X POST localhost:8080/ -d '(op:get_html_string)'
      # the counter inside the <div> should read 5 then 8.

  Status note (2026-05-13): build succeeds and the invoker plumbing is
  fully wired, but `AzApp_run` currently crashes inside libazul's
  webrender scene-building code (symbol
  `webrender::scene_building::SceneBuilder::build_item`) before the
  AZ_DEBUG probe is reachable. The crash reproduces with an empty
  default WCO (no Pascal-supplied layout callback), so it's libazul-
  side, not codegen-side. Pascal codegen struct-layout fixes that
  landed alongside this file (cbool, repr(C, u8) tag width,
  Destructor-field inclusion) are independent and remain valid.
  ============================================================================ }

program HelloWorld;

{$mode objfpc}{$H+}
{$PACKRECORDS C}

uses
  ctypes, sysutils,
  Azul;

type
  { Plain data model. }
  TMyModel = class
    Counter: Integer;
    constructor Create(c: Integer);
  end;

  { Click handler: bump counter and request a DOM refresh. }
  TMyClickHandler = class(TAzCallbackInvoker)
    procedure Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer); override;
  end;

  { Layout handler: build the DOM. }
  TMyLayoutHandler = class(TAzLayoutCallbackInvoker)
    procedure Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer); override;
  end;

constructor TMyModel.Create(c: Integer);
begin
  Counter := c;
end;

function MakeAzString(const s: ansistring): TAzString;
begin
  if Length(s) = 0 then
    Result := AzString_fromUtf8(nil, 0)
  else
    Result := AzString_fromUtf8(PChar(@s[1]), Length(s));
end;

procedure TMyClickHandler.Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer);
var
  m: TObject;
begin
  m := azul_refany_get(PAzRefAny(arg0));
  if (m <> nil) and (m is TMyModel) then
    TMyModel(m).Counter := TMyModel(m).Counter + 1;
  if out_ptr <> nil then
    PAzUpdate(out_ptr)^ := TAzUpdate_RefreshDom;
end;

procedure TMyLayoutHandler.Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer);
var
  m: TObject;
  body, counter_text, label_wrap, button_dom: TAzDom;
  btn: TAzButton;
  click_handler: TMyClickHandler;
  click_cb: TAzCallback;
  click_data: TAzRefAny;
begin
  m := azul_refany_get(PAzRefAny(arg0));
  if (m = nil) or not (m is TMyModel) then
  begin
    body := AzDom_createBody();
    if out_ptr <> nil then
      PAzDom(out_ptr)^ := body;
    Exit;
  end;

  counter_text := AzDom_createText(MakeAzString(IntToStr(TMyModel(m).Counter)));
  label_wrap := AzDom_createDiv();
  label_wrap := AzDom_withCss(label_wrap, MakeAzString('font-size: 32px;'));
  label_wrap := AzDom_withChild(label_wrap, counter_text);

  click_handler := TMyClickHandler.Create;
  click_cb := azul_register_callback(click_handler);
  click_data := azul_refany_create(TMyModel(m));

  btn := AzButton_create(MakeAzString('Increase counter'));
  btn := AzButton_withButtonType(btn, TAzButtonType_Primary);
  btn := AzButton_withOnClick(btn, click_data, click_cb);
  button_dom := AzButton_dom(btn);

  body := AzDom_createBody();
  body := AzDom_withChild(body, label_wrap);
  body := AzDom_withChild(body, button_dom);

  if out_ptr <> nil then
    PAzDom(out_ptr)^ := body;
end;

var
  model: TMyModel;
  layout_handler: TMyLayoutHandler;
  data: TAzRefAny;
  layout_cb: TAzLayoutCallback;
  wco: TAzWindowCreateOptions;
  cfg: TAzAppConfig;
  app: TAzApp;

begin
  WriteLn('[azul] Pascal full-GUI hello-world starting.');

  model := TMyModel.Create(5);
  data := azul_refany_create(model);

  layout_handler := TMyLayoutHandler.Create;
  layout_cb := azul_register_layoutcallback(layout_handler);

  wco := AzWindowCreateOptions_default();
  wco.window_state.layout_callback := layout_cb;
  wco.window_state.size.dimensions.width := 400.0;
  wco.window_state.size.dimensions.height := 300.0;
  wco.window_state.flags.decorations := TAzWindowDecorations_NoTitleAutoInject;
  wco.window_state.flags.background_material := TAzWindowBackgroundMaterial_Sidebar;

  cfg := AzAppConfig_create();
  app := AzApp_create(data, cfg);
  AzApp_run(@app, wco);
end.
