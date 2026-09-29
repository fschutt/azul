// Copy target/codegen/azul.d and libazul next to this file, then:
//   ldc2 main.d styles.d azul.d -L-L. -L-lazul && ./main
import std.stdio;
import styles;

void main()
{
    auto renderUiValue = renderUi();
    writeln("renderUi: ", renderUiValue.length, " properties");
}
