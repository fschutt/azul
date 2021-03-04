#include "../azul.h"

typedef struct {
    uint32_t counter;
} DataModel;

static void DataModel_delete(DataModel* restrict A) { }
static uint64_t DataModel_RttiTypeId = __LINE__;
AzString const DataModelType_RttiString = AzString_fromConstStr("DataModel");

AzString const css = AzString_fromConstStr("body { background-color: linear-gradient(135deg, #004e92 0%, #000428 100%); color: white; }");
AzNodeType const label = AzNodeType_Label(AzString_fromConstStr("Hello Azul / WebRender from C!"));
AzDom const child[] = {AzDom_new(label)};
AzDom const ui = {
    .root = AzNodeData_new(AzNodeType_Body),
    .children = AzDomVec_fromConstArray(child),
    .estimated_total_children = 1,
};

AzStyledDom myLayoutFunc(AzRefAny* restrict data, AzLayoutInfo info) {
    return AzStyledDom_new(ui, AzCss_fromString(css));
}

int main() {
    DataModel model = { .counter = 0 };
    AzRefAny upcasted = AzRefAny_newC(&model, sizeof(model), DataModel_RttiTypeId, DataModelType_RttiString, DataModel_delete);
    AzApp app = AzApp_new(upcasted, AzAppConfig_default());
    AzApp_run(app, AzWindowCreateOptions_new(myLayoutFunc));
    return 0;
}
