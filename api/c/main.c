#include "../azul.h"
#include <stdio.h>

typedef struct {
    uint32_t counter;
} DataModel;

static void DataModel_delete(void* restrict A) { }
AZ_REFLECT(DataModel, DataModel_delete);

static AzString const css = AzString_fromConstStr("body { font-size: 50px; }");
static AzNodeType const label = AzNodeType_Label(AzString_fromConstStr("Hello Azul / WebRender from C!"));
static AzDom const child[] = {AzDom_new(label)};
static AzDom const myUI = {
    .root = AzNodeData_new(AzNodeType_Body),
    .children = AzDomVec_fromConstArray(child),
    .total_children = 1, // len(child)
};

AzStyledDom myLayoutFunc(AzRefAny* restrict data, AzLayoutInfo info) {
    DataModelRef d = DataModelRef_create(data);
    if (DataModel_downcastRef(data, &d)) {
        printf("counter: %d\r\n", d.ptr->counter);
    }
    DataModelRef_delete(&d);
    return AzStyledDom_new(myUI, AzCss_fromString(css));
}

int main() {
    DataModel model = { .counter = 5 };
    AzApp app = AzApp_new(DataModel_upcast(model), AzAppConfig_default());
    AzApp_run(app, AzWindowCreateOptions_new(myLayoutFunc));
    return 0;
}
