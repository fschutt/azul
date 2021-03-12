#include "azul.h"

typedef struct {
    uint32_t counter;
} DataModel;

void DataModel_delete(DataModel* restrict A) { }
AZ_REFLECT(DataModel, DataModel_delete);

AzStyledDom myLayoutFunc(AzRefAny* restrict data, AzLayoutInfo info) {

    AzString counter_string;

    DataModelRef d = DataModelRef_create(data);
    if (DataModel_downcastRef(data, &d)) {
        AzFmtArgVec fmt_args = AzFmtArgVec_fromConstArray({{
            .key = AzString_fromConstStr("counter"),
            .value = AzFmtValue_Uint(d.ptr->counter)
        }});
        counter_string = AzString_format(AzString_fromConstStr("Counter is now: {counter}"), fmt_args);
    } else {
        return AzStyledDom_empty();
    }
    DataModelRef_delete(&d);

    AzDom const html = {
        .root = AzNodeData_new(AzNodeType_Body),
        .children = AzDomVec_fromConstArray({AzDom_new(AzNodeType_Label(counter_string))}),
        .total_children = 1, // len(children)
    };
    AzCss const css = AzCss_fromString(AzString_fromConstStr("body { font-size: 50px; }"));
    return AzStyledDom_new(html, css);
}

int main() {
    DataModel model = { .counter = 5 };
    AzApp app = AzApp_new(DataModel_upcast(model), AzAppConfig_default());
    AzWindowCreateOptions root_Winoptions = AzWindowCreateOptions_new(myLayoutFunc);
    options.hot_reload = true; // hot reload the UI
    AzApp_run(app, options);
    return 0;
}
