    <h3><a href="#remarks-c" id="remarks-c">C</a></h3>
    <br/>
    <p>1. Functions are named "Az" + class name + "_" + function name in pascalCase. Modules / namespaces do not exist:</p>
    <code class="expand">app::App::new() = AzApp_new()</code><br/>
    <p>2. Enums are named "Az" + enum name + "_" + variant name:</p>
    <code class="expand">LayoutAlignItems::Stretch = AzLayoutAlignItems_Stretch</code><br/>
    <p>3. <code>union enum</code>s have special compile-time macros that crate the correct union variant + tag at compile time:</p>
    <code class="expand">AzStyleCursorValue cursor = AzStyleCursorValue_Exact(AzStyleCursor_Grab);</code><br/>

    <p>4. If a class is marked with <span class="chd">has destructor</span>, it has a corresponding <code>_delete()</code> function. The destructors automatically call the sub-destructors of all fields (i.e. you do not need to recurse and delete every field manually).</p>
    <code class="expand"># App is marked as "has_destructor":
AzApp app = AzApp_new(/* ... */); # constructor
AzApp_delete(&app); # destructor
# value of app is undefined here</code><br/>

    <p>5. All classes can be deep-copied via <code>_deepCopy()</code> - note that this might be very expensive for large objects.</p>
    <code class="expand">AzWindowCreateOptions w = AzWindowCreateOptions_new();
AzWindowCreateOptions copy = AzWindowCreateOptions_copy(&w);</code><br/>


    <p>6. To emulate pattern matching in C, every <code>union enum</code> has a corresponding <code>$union_matchRef$variant()</code> and <code>$union_matchMut$variant()</code> function.
        Both take a pointer to the union and an (uninitialized) pointer to the output. If the union is of variant $variant, the output pointer will be initialized:</p>
<code class="expand">// create a union enum
AzStyleCursorValue cursor = AzStyleCursorValue_Exact(AzStyleCursor_Grab);

// destructure a union enum
AzStyleCursor* result;
if AzStyleCursorValue_matchRefExact(&cursor, &result) {
    printf("ok!\n");
    // result is initialized here
}</code><br/>
    The difference between <code>_matchRef()</code> and <code>_matchMut()</code>
    is that takes a <code>const *</code> and <code>_matchMut()</code> takes a <code>restrict *</code>
    to the result. In the example, the lifetime of <code>result</code> is equal to the lifetime of
    <code>cursor</code> (since <code>result</code> simply points to the payload of the tagged <code>cursor</code> union).</p><br/>

    <p>7. Run-time type reflection for <code>RefAny</code> can implemented via the <code>AZ_REFLECT</code> macro:</p>
    <code class="expand">typedef struct { int field; } MyStruct;
void MyStruct_delete(MyStruct* restrict A) { } // destructor
AZ_REFLECT(MyStruct, MyStruct_delete);</code><br/>

    <p>The <code>AZ_REFLECT</code> macro generates functions to upcast from your struct to a
        <code>RefAny</code> as well as to do a checked downcast from a <code>RefAny</code> to your custom data type:</p>
    <code class="expand">// create a new RefAny
AzRefAny object = MyStruct_upcast(MyStruct { .field = 5 });

// Creates an uninitialized borrow and downcast it
// fails if "object" is already borrowed mutably
// or if the "RefAny" is not of type "MyStruct" (type check at runtime)
MyStructRef structref = MyStructRef_create(&object);
if MyStruct_downcastRef(&object, &structref) {
    printf!("ok! - %d", structref->ptr.field);
}

// unlocks "object" to be borrowed mutably again
MyStructRef_delete(&structref);

// Creates a borrow that can MODIFY the contents of the RefAny
// (mutable "* restrict" borrow instead of "const*" borrow)
//
// The object cannot be borrowed referentially and mutably at
// the same time - this invariant is checked at runtime.
MyStructRefMut structrefmut = MyStructRefMut_create(&object);
if MyStruct_downcastRefMut(&object, &structrefmut) {
    // structrefmut can modify the contents of the RefAny
    structrefmut->ptr.field = 6;
    // prints "6", value is now changed, visible to all copies of "object"
    printf!("ok! - %d", structref->ptr.field);
}

MyStructRefMut_delete(&structrefmut);

// decreases the refcount - if refcount is 0, destructor is called
if !MyStructRefAny_delete(&object) { /* RefAny is not of type "MyStruct" */ }</code><br/>
    <p>8. All <code>*Vec</code> data types have a <code>_empty()</code> constructor macro (to create an empty vec at compile time).
The <code>AzString</code> type has an extra <code>_fromConstStr()</code> macro to define compile-time strings:</p>
    <code class="expand">AzScanCodeVec v = AzScanCodeVec_empty();
AzString s = AzString_fromConstStr("hello"); // evaluated at compile-time</code><br/>
    <br/>