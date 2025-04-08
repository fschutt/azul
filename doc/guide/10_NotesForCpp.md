

    <h3><a href="#remarks-cpp" id="remarks-cpp">C++</a></h3>
    <br/>
    <p>1. Like in Python, default constructors of classes take the arguments in the order or the fields:</p>
    <code class="expand">// API: struct ColorU { r: u8, g: u8, b: u8 a: u8 }
// therefore the arguments to the default constructor are passed in order:
auto color = ColorU(/*r*/ 255, /*g*/ 255, /*b*/ 255, /*a*/ 255);</code><br/>
    <p>2. Explicit constructors are static functions, enums use <code>enum class</code> (C++11):</p>
    <code class="expand">auto window = WindowCreateOptions::default(LayoutSolver::Default);</code><br/>
    <p>3. All by-value arguments require <code>std::move</code> in order to prevent accidental copies:</p>
    <code class="expand">auto window = WindowCreateOptions::default(LayoutSolver::Default);
app.run(std::move(window));</code><br/>
    <p>4. In difference to C, constructing a <code>RefAny</code> does not require macros, instead generics are used:</p>
    <code class="expand">class MyStruct {
    int field;
    public:
        MyStruct(int f) noexcept: field(f) { }
}

auto object = RefAny::new(std::move(MyStruct(5)));
auto objectref = object.downcastRef&lt;MyStruct&gt;();
if (objectref): // objectref = std::optional&lt;Ref&lt;MyStruct&gt;&gt;
    std::cout << objectref->ptr.field << std::endl;
objectref.delete(); // release reference

auto objectrefmut = object.downcastRefMut&lt;MyStruct&gt;();
if (objectrefmut):// objectrefmut = std::optional&lt;RefMut&lt;MyStruct&gt;&gt;
    std::cout << objectrefmut->ptr.field << std::endl;
objectrefmut.delete(); // release reference

// "object" RefAny destructor automatically decreases refcount here</code><br/>
    <p>5. All <code>*Vec</code> types have a <code>_fromStdVector()</code> function
        that converts a <code>std::vector</code>
        into the given <code>Vec</code> type <strong>without copying the contents of the array</strong>.
        Additionally a <code>_fromStdString</code> function exists
        to convert a <code>std::string</code> into a <code>String</code>
        type. All strings are UTF-8 encoded.
    </p>
    <code class="expand">auto scancodes = std::vector{0,1,2,3};
auto converted = ScanCodeVec::fromStdVector(std::move(scancodes));
auto converted_back = converted.intoStdVector();
</code><br/>
    <p>6. Like in C, all <code>union enums</code> have special functions to emulate pattern matching:</p>
    <code class="expand">// create a union enum - Exact() is a constexpr function
auto cursor = StyleCursorValue::Exact(StyleCursor::Grab);

// destructure a union enum
if (auto result = cursor.matchRefExact()) {
    // result = const StyleCursor*
    std::cout << result << std::endl;
}

if (auto result = cursor.matchRefMutExact()) {
    // result = StyleCursor* restrict
    *result = StyleCursor::Default;
}</code><br/>