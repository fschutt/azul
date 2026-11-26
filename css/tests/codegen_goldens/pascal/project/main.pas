{ Copy target/codegen/azul.pas and libazul here, then:
    fpc -Mdelphi -Fl. -k-L. -k-lazul main.pas && ./main
  Linux: add -Aas (azul.pas has more object sections than fpc 3.2's ELF writer can number). }
program Main;

{$mode delphi}

uses Azul, Styles;

var
  StyleBtnValue: TAzCssPropertyWithConditionsVec;
begin
  StyleBtnValue := StyleBtn;
  WriteLn('StyleBtn: ', StyleBtnValue.len, ' properties');
end.
