
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