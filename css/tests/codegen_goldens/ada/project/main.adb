--  Copy target/codegen/azul.ads + azul.adb and libazul here, then:
--    gprbuild -P azul_styles.gpr && ./obj/main
with Ada.Text_IO;
with Interfaces.C;
with Azul; use Azul;
with Styles;

procedure Main is
   Style_Btn_Value : constant Az_CssPropertyWithConditionsVec := Styles.Style_Btn;
begin
   Ada.Text_IO.Put_Line
     ("Style_Btn:" & Interfaces.C.size_t'Image (Style_Btn_Value.Len) & " properties");
end Main;
