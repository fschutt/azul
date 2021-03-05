
/* macro to turn a compile-time string into a compile-time AzString
 *
 * AzString foo = AZ_STATIC_STRING(\"MyString\");
 */
#define AzString_fromConstStr(s) { .vec = { .ptr = s, .len = sizeof(s) - 1, .cap = sizeof(s) - 1, .destructor = { .NoDestructor = { .tag = AzU8VecDestructorTag_NoDestructor, }, }, }, }

#define AzNodeData_new(nt) { \
    .node_type = nt, \
    .dataset = AzOptionRefAny_None, \
    .ids_and_classes = AzIdOrClassVec_empty, \
    .callbacks = AzCallbackDataVec_empty, \
    .inline_css_props = AzNodeDataInlineCssPropertyVec_empty, \
    .clip_mask = AzOptionImageMask_None, \
    .tab_index = AzOptionTabIndex_None, \
}

#define AzDom_new(nt) { \
    .root = AzNodeData_new(nt),\
    .children = AzDomVec_empty, \
    .estimated_total_children = 0, \
}

#define AZ_REFLECT(structName, destructor) \
    /* in C all statics are guaranteed to have a unique address, use that address as a TypeId */ \
    static uint64_t const structName##_RttiTypePtrId = 0; \
    static uint64_t const structName##_RttiTypeId = (uint64_t)(&structName##_RttiTypePtrId); \
    static AzString const structName##_Type_RttiString = AzString_fromConstStr(#structName); \
    \
    AzRefAny structName##_upcast(structName const s) { \
        return AzRefAny_newC(&s, sizeof(structName), structName##_RttiTypeId, structName##_Type_RttiString, destructor); \
    } \
    \
    /* generate structNameRef and structNameRefMut structs*/ \
    typedef struct { const structName* ptr; AzRefCount sharing_info; } structName##Ref; \
    typedef struct { structName* restrict ptr; AzRefCount sharing_info; } structName##RefMut; \
    \
    structName##Ref structName##Ref_create(AzRefAny* const refany) { \
        structName##Ref val = { .ptr = 0, .sharing_info = AzRefCount_deepCopy(&refany->sharing_info) };    \
        return val;    \
    } \
    \
    structName##RefMut structName##RefMut_create(AzRefAny* const refany) { \
        structName##RefMut val = { .ptr = 0, .sharing_info = AzRefCount_deepCopy(&refany->sharing_info), };    \
        return val;    \
    } \
    \
    /* if downcastRef returns true, the downcast worked */ \
    bool structName##_downcastRef(AzRefAny* restrict refany, structName##Ref * restrict result) { \
        if (!AzRefAny_isType(refany, structName##_RttiTypeId)) { return false; } else { \
            if (!AzRefCount_canBeShared(&refany->sharing_info)) { return false; } else {\
                AzRefCount_increaseRef(&refany->sharing_info); \
                result->ptr = (structName* const)(refany->_internal_ptr); \
                return true; \
            } \
        } \
    } \
    \
    /* if downcastRefMut returns true, the mutable downcast worked */ \
    bool structName##_downcastMut(AzRefAny* restrict refany, structName##RefMut * restrict result) { \
        if (!AzRefAny_isType(refany, structName##_RttiTypeId)) { return false; } else { \
            if (!AzRefCount_canBeSharedMut(&refany->sharing_info)) { return false; }  else {\
                AzRefCount_increaseRefmut(&refany->sharing_info); \
                result->ptr = (structName* restrict)(refany->_internal_ptr); \
                return true; \
            } \
        } \
    } \
    \
    /* releases a structNameRef (decreases the RefCount) */ \
    bool structName##Ref_delete(structName##Ref* restrict value) { \
        AzRefCount_decreaseRef(&value->sharing_info); \
    }\
    \
    /* releases a structNameRefMut (decreases the mutable RefCount) */ \
    bool structName##RefMut_delete(structName##RefMut* restrict value) { \
        AzRefCount_decreaseRefmut(&value->sharing_info); \
    }\
    /* releases a structNameRefAny (checks if the RefCount is 0 and calls the destructor) */ \
    bool structName##RefAny_delete(AzRefAny* restrict refany) { \
        if (!AzRefAny_isType(refany, structName##_RttiTypeId)) { return false; } \
        AzRefAny_delete(refany); \
        return true; \
    }