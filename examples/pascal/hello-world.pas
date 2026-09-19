program HelloWorld;

{$mode delphi} // Enables modern features like anonymous methods and generics

uses
  SysUtils, Azul;

type
  TMyModel = class
    Counter: Integer;
  end;

// Deviation from the guide: FPC 3.2.2 has no anonymous methods, so the inline
// closure is a named function and the model type is spelled out at the call.
function OnIncrease(M: TMyModel): TAzUpdate;
begin
  M.Counter := M.Counter + 1;
  Result := azRefreshDom;
end;

function Layout(Model: TMyModel): TDom;
var
  LabelDom: TDom;
  Btn: TButton;
begin
  // 1. Strings are implicit. Builder pattern takes ownership automatically.
  LabelDom := TDom.P(IntToStr(Model.Counter))
                  .WithCss('font-size: 32px; margin: 0;');

  Btn := TButton.Create('Increase counter')
                .SetButtonType(azPrimary)
                // 2. Typed callback instead of Invoker classes (see OnIncrease above)
                .OnClick<TMyModel>(OnIncrease);

  // 3. Normal function returns, no `out_ptr` assignments
  Result := TDom.Body.AddChild(LabelDom).AddChild(Btn.Dom);
end;

var
  App: TAzApp<TMyModel>; // 4. Generics!
  Model: TMyModel;
begin
  WriteLn('[azul] Pascal full-GUI hello-world starting.');

  Model := TMyModel.Create;
  Model.Counter := 5;

  // 5. App manages the C setup, layout registration, and window defaults
  App := TAzApp<TMyModel>.Create(Model, Layout);
  App.Window.Title := 'Hello World';
  App.Window.Width := 400;
  App.Window.Height := 300;
  
  App.Run;
  
  App.Free;
  Model.Free;
end.
