--  Copy target/codegen/azul.ads + azul.adb and libazul here, then:
--    gprbuild -P azul_styles.gpr && ./obj/main
with Ada.Text_IO;
with Interfaces.C;
with Azul; use Azul;
with Styles;

procedure Main is
begin
   null;
end Main;
