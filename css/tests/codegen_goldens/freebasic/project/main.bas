' Copy target/codegen/azul.bi and libazul here, then:
'   fbc main.bas -p . -l azul && ./main
#include "styles.bas"

Dim StyleBtnValue As AzCssPropertyWithConditionsVec = StyleBtn()
Print "StyleBtn: "; StyleBtnValue.len; " properties"
