# Copy target/codegen/azul.nim and libazul next to this file, then:
#   nim c -d:release -r main.nim
import styles

let styleBtnValue = styleBtn()
echo "styleBtn: ", styleBtnValue.len, " properties"
