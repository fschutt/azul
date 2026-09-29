{ Copy target/codegen/azul.pas and libazul here, then:
    fpc -Mdelphi -Fl. -k-L. -k-lazul main.pas && ./main }
program Main;

{$mode delphi}

uses Azul, Styles;

var
  RenderUiValue: TAzDom;
begin
  RenderUiValue := RenderUi;
  WriteLn('RenderUi: ', RenderUiValue.len, ' properties');
end.
