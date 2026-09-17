program HelloWorld;

{$mode delphi} // Enables modern features like closures and generics

uses
  SysUtils, Azul;

type
  TMyModel = class
    Counter: Integer;
  end;

  TMyController = class
    function Layout(M: TAzRefAny; Info: TAzLayoutCallbackInfo): TAzDom;
    function OnClick(M: TAzRefAny; Info: TAzCallbackInfo): TAzUpdate;
  end;

function MakeAzString(const s: ansistring): TAzString;
begin
  if Length(s) = 0 then
    Result := AzString_fromUtf8(nil, 0)
  else
    Result := AzString_fromUtf8(PChar(@s[1]), Length(s));
end;

function TMyController.Layout(M: TAzRefAny; Info: TAzLayoutCallbackInfo): TAzDom;
var
  TypedModel: TMyModel;
  LabelDom, BodyDom: TDom;
  Btn: TButton;
  Data: TAzRefAny;
begin
  // Extract our typed model from the raw callback reference
  TypedModel := TMyModel(azul_refany_get(@M));
  
  // 1. Strings are implicit. Builder pattern takes ownership automatically.
  LabelDom := TDom.CreateTextDoNotUseWithoutBlockLevelWrapper(MakeAzString(IntToStr(TypedModel.Counter)))
                  .WithCss(MakeAzString('font-size: 32px; margin: 0;'));

  Data := azul_refany_create(TypedModel);
  
  // 2. Wrap the callback method using our provided Wrapper classes
  Btn := TButton.Create(MakeAzString('Increase counter'))
                .WithButtonType(TAzButtonType_Primary)
                .WithOnClick(Data, azul_register_buttononclickcallback(
                     TAzButtonOnClickCallbackWrapper.Create(Self.OnClick)
                ));

  // 3. Normal function returns, no `out_ptr` assignments
  BodyDom := TDom.CreateP.WithChild(LabelDom).WithChild(Btn.Dom);
  Result := BodyDom.Release;
end;

function TMyController.OnClick(M: TAzRefAny; Info: TAzCallbackInfo): TAzUpdate;
var
  TypedModel: TMyModel;
begin
  TypedModel := TMyModel(azul_refany_get(@M));
  TypedModel.Counter := TypedModel.Counter + 1;
  Result := TAzUpdate_RefreshDom;
end;

var
  App: TApp;
  Model: TMyModel;
  Controller: TMyController;
  LayoutCB: TAzLayoutCallback;
  Wco: TAzWindowCreateOptions;
  Cfg: TAzAppConfig;
  Data: TAzRefAny;
begin
  WriteLn('[azul] Pascal full-GUI hello-world starting.');

  Model := TMyModel.Create;
  Model.Counter := 5;
  Controller := TMyController.Create;
  Data := azul_refany_create(Model);

  LayoutCB := azul_register_layoutcallback(TAzLayoutCallbackWrapper.Create(Controller.Layout));

  Wco := AzWindowCreateOptions_createDefault();
  Wco.window_state.layout_callback := LayoutCB;
  Wco.window_state.size.dimensions.width := 400.0;
  Wco.window_state.size.dimensions.height := 300.0;

  Cfg := AzAppConfig_create();
  
  // 4. Object ownership is transferred, we do not need to manually free them
  App := TApp.Create(Data, Cfg);
  App.Run(Wco);
  
  App.Free;
  Model.Free;
  Controller.Free;
end.
