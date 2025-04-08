<p>1. Every type implements <code>__str__()</code> and <code>__repr__()</code> for debug printing</p></br>

<p>2. Simple (non-union) enum types implement<code>__richcmp__()</code>:</p>

<code class="expand">align = LayoutAlignItems.Stretch
if align == LayoutAlignItems.Stretch:
print("ok!")</code>
<br/>

<p>3. <code>union enums</code> have a <code>@staticmethod</code>
    constructor and a hidden <code>.match()</code> function
    that will return the enum tag as a string and the enum payload as a PyObject:</p>

<code class="expand">size = OptionLogicalSize.Some(LogicalSize(600, 800))
tag, payload = size.match()
if tag == "Some":
print("size is {}, {}".format(payload.width, payload.height))
elif tag == "None":
print("no size available")</code><br/>

<p>4. <code>new()</code> constructors are the default constructors:</p>

<code class="expand">dom = Dom(NodeType.Div) # not Dom.new(NodeType.Div)!</code><br/>

<p>5. If no explicit <code>new()</code> constructors exist, the default constructor takes all arguments in the order as they are specified in the API:</p>

<code class="expand"># API: struct ColorU { r: u8, g: u8, b: u8 a: u8 }
# therefore the arguments to the default constructor are passed in order:
color = ColorU(/*r*/ 255, /*g*/ 255, /*b*/ 255, /*a*/ 255)</code><br/>

<p>6. Whenever a <code>RefAny</code> parameter is required to call a function, you can use any PyObject as a replacement:</p>
    <code class="expand">mydata = MyClass() # your custom data type
# App::new() takes a RefAny as the first argument - pass in your custom data type here
app = App(mydata, AppConfig(LayoutSolver.Default))</code><br/>

<p>7. All functions that take a <code>*Vec</code> type accept a PyList and all <code>*Option</code> types are automatically converted to and from Pythons <code>None</code> value:</p>
    <code class="expand">monitors = app.get_monitors() # returns MonitorVec = PyList[Monitor]
print(str(monitors[0]))

# returns OptionLogicalPosition = Optional[LogicalPosition]
cursor_relative = callbackinfo.get_cursor_relative_to_viewport()
if cursor_relative is not None:
print(str(cursor_relative))
</code><br/>

<p>8. Due to a bug in the Python PyO3 bindings, you cannot modify a struct through a struct (see <a href="https://github.com/PyO3/pyo3/issues/1603">issue</a>).</p>

<code class="expand">window = WindowCreateOptions(LayoutSolver.Default)
window.state.flags.frame = WindowFrame.Maximized # does not work

# workaround (yes, it's annoying):
state = window.state.copy()
flags = state.flags.copy()
flags.frame = WindowFrame.Maximized
state.flags = flags
window.state = state</code>
