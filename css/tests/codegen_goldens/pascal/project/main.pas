{ Copy target/codegen/azul.pas and libazul here, then:
    fpc -Mdelphi -Fl. -k-L. -k-lazul main.pas && ./main }
program Main;

{$mode delphi}

uses Azul, Styles;

var
  StyleBtnValue: TAzCssPropertyWithConditionsVec;
begin
  StyleBtnValue := StyleBtn;
  WriteLn('StyleBtn: ', StyleBtnValue.len, ' properties');
end.
