# Copy target/codegen/azul.nim and libazul next to this file, then:
#   nim c -d:release -r main.nim
import styles

let renderUiValue = renderUi()
echo "renderUi: ", renderUiValue.len, " properties"
