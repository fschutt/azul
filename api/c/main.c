#include "azul.h"

typedef struct {
    uint32_t counter;
} DataModel;

// static uint8_t* DataModelType = "DataModel";
static uint64_t DataModelTypeId = 0;
static void DataModelRefAnyDestructor(void* restrict A) { }

AzString DataModelTypeString = {
    .vec = {
        .ptr = "DataModel",
        .len = sizeof("DataModel"),
        .cap = sizeof("DataModel"),
        .destructor = {
            .NoDestructor = {
              .tag = AzU8VecDestructorTag_NoDestructor,
            },
        },
    },
};

int main() {
    DataModel model = { .counter = 0, };
    AzRefAny opaque_model = AzRefAny_newC(&model, sizeof(model), DataModelTypeId, DataModelTypeString, DataModelRefAnyDestructor);
    AzApp app = AzApp_new(opaque_model, AzAppConfig_default());
    AzApp_delete(&app);
    return 0;
}
