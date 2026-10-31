program HelloWorld;

{$mode delphi}

uses
  SysUtils, Azul;

type
  TMyModel = class
    Counter: Integer;
  end;

function OnIncrease(Model: TMyModel; Info: TAzCallbackInfo): TAzUpdate;
begin
  Model.Counter := Model.Counter + 1;
  Result := azRefreshDom;
end;

function Layout(Model: TMyModel; Info: TAzLayoutCallbackInfo): TDom;
var
  LabelDom: TDom;
  Btn: TButton;
begin
  LabelDom := TDom.P(IntToStr(Model.Counter))
    .WithCss('font-size: 32px; margin: 0;');

  Btn := TButton.Create('Increase counter')
    .SetButtonType(azPrimary)
    .OnClick<TMyModel>(OnIncrease);

  Result := TDom.Body.AddChild(LabelDom).AddChild(Btn.Dom);
end;

var
  Model: TMyModel;
  App: TAzApp<TMyModel>;
begin
  Model := TMyModel.Create;
  Model.Counter := 5;

  App := TAzApp<TMyModel>.Create(Model, Layout);
  App.Window.Title := 'Hello World';
  App.Window.Width := 400;
  App.Window.Height := 300;
  App.Run;

  App.Free;
end.
