/* C: no optional type of its own. An Option is built with `<Option>_none()` /
 * `<Option>_some(x)` and read with `<Option>_matchRefSome`, which
 * AZ_LET_SOME / AZ_LET_OK wrap into "bind the payload or end the block".
 * Built and run by scripts/native_optionals_smoke.sh. */
#include "azul.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    AzString folder;
} MyDataModel;

void MyDataModel_destructor(void* m) { (void)m; }
AZ_REFLECT(MyDataModel, MyDataModel_destructor);

/* The guide's file-dialog example, as written there (hello-world/c.md). */
AzUpdate on_pick_folder(AzRefAny data, AzCallbackInfo info, AzRefAny result) {
    MyDataModelRefMut d = MyDataModelRefMut_create(&data);
    if (!MyDataModel_downcastMut(&data, &d)) {
        MyDataModelRefMut_delete(&d);
        return AzUpdate_DoNothing;
    }
    AzOptionFileOpenResult picked = AzFileOpenResult_downcast(result);
    const AzFileOpenResult* open;
    AZ_LET_SOME(AzOptionFileOpenResult, &picked, open, MyDataModelRefMut_delete(&d); return AzUpdate_DoNothing);
    const AzFilePath* path;
    AZ_LET_SOME(AzOptionFilePath, &open->path, path, AzOptionFileOpenResult_delete(&picked); MyDataModelRefMut_delete(&d); return AzUpdate_DoNothing); /* cancelled */
    d.ptr->folder = AzString_clone(&path->inner);
    AzOptionFileOpenResult_delete(&picked);
    MyDataModelRefMut_delete(&d);
    return AzUpdate_RefreshDom;
}

AzUpdate on_choose_folder(AzRefAny data, AzCallbackInfo info) {
    AzFileDialog_openDirectory(AZ_STR("Choose a folder"), AzOptionString_none(), AzRefAny_clone(&data), on_pick_folder);
    return AzUpdate_DoNothing;
}

static void check(int ok, const char* what) {
    if (!ok) {
        fprintf(stderr, "FAIL: %s\n", what);
        exit(1);
    }
}

int main(void) {
    /* A field: None in, None out; Some in, Some out. */
    AzIconVariants icons = AzIconVariants_createDefault();
    check(AzOptionString_isNone(&icons.light), "a default IconVariants has no light icon");
    AzOptionString_delete(&icons.light);
    icons.light = AzOptionString_some(AZ_STR("light.png"));
    const AzString* light;
    int found = 0;
    do {
        AZ_LET_SOME(AzOptionString, &icons.light, light, break);
        found = light->vec.len == 9 && memcmp(light->vec.ptr, "light.png", 9) == 0;
    } while (0);
    check(found, "Some in, Some out");
    AzOptionString_delete(&icons.light);
    icons.light = AzOptionString_none();
    check(AzOptionString_isNone(&icons.light), "None in, None out");

    /* A result: Ok and Err. */
    AzResultUrlUrlParseError ok = AzUrl_parse(AZ_STR("https://azul.rs/guide"));
    const AzUrl* url;
    found = 0;
    do {
        AZ_LET_OK(AzResultUrlUrlParseError, &ok, url, break);
        found = url != NULL;
    } while (0);
    check(found, "Url_parse Ok");
    AzResultUrlUrlParseError err = AzUrl_parse(AZ_STR("no scheme"));
    check(AzResultUrlUrlParseError_isErr(&err), "Url_parse Err");
    AzResultUrlUrlParseError_delete(&ok);
    AzResultUrlUrlParseError_delete(&err);
    AzIconVariants_delete(&icons);
    (void)on_choose_folder;
    printf("c native optionals: ok\n");
    return 0;
}
