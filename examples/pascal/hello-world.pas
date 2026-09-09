program HelloWorld;

{$mode objfpc}{$H+}
{$PACKRECORDS C}

uses
  ctypes, sysutils,
  Azul;

type
  TMyModel = class
    Counter: Integer;
    constructor Create(c: Integer);
  end;

  TMyClickHandler = class(TAzButtonOnClickCallbackInvoker)
    procedure Invoke(id: cuint64; arg0: Pointer; arg1: Pointer; out_ptr: Pointer); override;
  end;

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
  label_dom, body: TDom;
  btn: TButton;
  click_handler: TMyClickHandler;
  click_cb: TAzButtonOnClickCallback;
  click_data: TAzRefAny;
begin
  m := azul_refany_get(PAzRefAny(arg0));
  if (m = nil) or not (m is TMyModel) then
  begin
    body := TDom.CreateBody;
    if out_ptr <> nil then
      PAzDom(out_ptr)^ := body.Release;
    body.Free;
    Exit;
  end;

  label_dom := TDom.CreatePWithText(MakeAzString(IntToStr(TMyModel(m).Counter)))
                   .WithCss(MakeAzString('font-size: 32px; margin: 0;'));

  click_handler := TMyClickHandler.Create;
  click_cb := azul_register_buttononclickcallback(click_handler);
  click_data := azul_refany_create(TMyModel(m));

  btn := TButton.Create(MakeAzString('Increase counter'))
                .WithButtonType(TAzButtonType_Primary)
                .WithOnClick(click_data, click_cb);

  body := TDom.CreateBody.WithChild(label_dom).WithChild(btn.Dom);

  if out_ptr <> nil then
    PAzDom(out_ptr)^ := body.Release;

  label_dom.Free;
  btn.Free;
  body.Free;
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

  cfg := AzAppConfig_create();
  app := AzApp_create(data, cfg);
  AzApp_run(@app, wco);
end.
