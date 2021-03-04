#include "../azul.h"

typedef struct {
    uint32_t counter;
} DataModel;

static void DataModel_delete(DataModel* restrict A) { }
static uint64_t DataModel_RttiTypeId = __LINE__;
AzString DataModelType_RttiString = AzString_fromConstStr("DataModel");


AzStyledDom myLayoutFunc(AzRefAny* restrict data, AzLayoutInfo info) {
    AzDom dom = AzDom_new(AzNodeType_Body);
    return AzStyledDom_new(dom, AzCss_empty());
}

int main() {
    DataModel model = { .counter = 0 };
    AzRefAny upcasted = AzRefAny_newC(&model, sizeof(model), DataModel_RttiTypeId, DataModelType_RttiString, DataModel_delete);
    AzApp app = AzApp_new(upcasted, AzAppConfig_default());
    AzApp_run(app, AzWindowCreateOptions_new(myLayoutFunc));
    // AzApp_delete(&app);
    return 0;
}
