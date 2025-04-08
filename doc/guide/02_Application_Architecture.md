<h2>Why Azul?</h2>

<p>
    In order to understand why Azul forces the separation of data model,
    model-to-view function and callbacks, you have to understand the problem with nearly
    every other toolkit. In other toolkits, your app would look roughly like this:
</p>

<code class="expand">class MyApp extends othertoolkit.App:
    self.button = Button(...)
    self.text_input = TextInput(...)
    self.output = Label(...)

    self.add(self.button)
    self.add(self.text_input)
    self.add(self.output)

    @override
    def on_click():
        # we somehow need to access the text input
        input = self.text_input.getText()
        calculated = do_somthing_with_input(input)
        self.output.setText(calculated)
        self.text_input.setText("")
</code>

<p>This example is simple, but there are already a couple of problems:</p>
<br/>
<ol>
    <li><p>The application data has to store UI objects in order to access them from callbacks: You do not care about the UI objects, you only care about the input data, but you have to carry them around in order to get access to the <code>text_input.getText()</code></p></li>
    <br/>
    <li><p>You need to remember to reset the text input after calculation and update the UI. Forget one <code>setText</code> call and the UI will be out of sync. Other functions might expect the UI to be in a certain state (i.e. the text input is expected to be empty before calling function <code>foo()</code>)</p></li>
    <br/>
    <li><p>Removing and exchanging UI objects on state changes becomes very hard.
        Some frameworks auto-delete children objects, which means you need to make sure that no other objects are holding references to <code>self.text_input</code></p></li>
    <br/>
    <li><p>The state is not easily testable because there are a lot of stateful calls changing
    your UI objects. Other classes might unexpectedly change UI objecs in their parent classes -
    not to speak of multithreading</p></li>
    <br/>
    <li><p>Layout and hierarchies become a pain, even using "layout managers", there is little room for exchanging layout managers (i.e. switching from a horizontal layout to a vertical layout one at runtime)</p></li>
    <br/>
    <li><p>Your inheritance hierarchy is bound to the visual hierarchy of objects the screen. This works for linear hierarchies (i.e. <code>SideBarPanel extends Sidebar extends App</code>), but not for non-linear ones (SideBar has to interact with MainTable). Redesigning your App will force you to refactor inheritance hierarchies, which takes time.</p></li>
    <br/>
    <li><p>Most of the code is run on startup, leading to both bad startup time and less flexibility when for example switching screens in a single-page application</p></li>
    <br/>
    <li><p>Because of the inheritance and because you need to store UI objects inside of your class, you've created a hard dependency on the toolkit. Should you ever want to migrate away from the toolkit, it will be painful since your code could depend on functionality inherited from the GUI toolkit.</p></li>
    <br/>
    <li><p>Callbacks can only access data from the <code>MyApp</code> class, not from any other UI classes. If multiple, unrelated UI objects need to talk to each other,
        you need superclasses or references to the least common "parent object" in an inheritance hierarchy</p></li>
</ol>

<br/>
<h2>IMGUI is not the solution</h2>

<p>
    Solutions such as Immediate Mode GUI "solve" this problem by hiding the
    data binding to the callback in a closure-with-captured-arguments instead
    of a class-with-state-and-functions: The form is different, but the
    operation is the same.
</p>

<p>
    A closure is just an anonymous (non-nameable) function on an
    anonymous (non-nameable) struct which contains all captured variables.
    The effect is the same as a class-with-methods and on top of that,
    it provides even less layout flexibility than even object-oriented code.
</p>


<p>
    Immediate Mode GUI solves the synchronization problem, but it fails at
    the other two problems. It doesn't solve the problem and the
    performance tradeoffs are - usually - immense.
</p>

<br/>
<h2>The trifecta of UI toolkits</h2>

<p>
    Any toolkit has to at least solve the three following problems in order
    to be more than just a "rendering library":
</p>

<br/>
<ol>
    <li><p><strong>Data Access / Model-View separation: </strong> Somehow the callback needs access to both the data model (i.e. the class) and the stateful UI object (to scrape the text out), but at the same time the "model" should be separate from the UI so that logic functions do not depend on view data.</p></li>
    <li><p><strong>Synchronization:</strong> It is very easy for the visual UI state and the data model to go out of sync. Solutions are "Observer patterns" (callbacks that run when something changes), React-like reconciliation or "just redraw everything" (IMGUI)</p></li>
    <li><p><strong>Inter-widget communication: </strong> Most toolkits assume that the widget hierarchy and the inheritance (or function call) hierarchy are the same. If two UI objects that have no common parent have to talk to each other, you now have a massive problem.</p></li>
</ol>

<p>
    Many toolkits do not solve these problems at all, instead
    shoving the responsibility onto the application programmer
    (aka. "not my job"). The result of such "freedom" to design
    any application style is often sub-par, but enjoys a large
    popularity because it secures the job of whoever first wrote the application.
</p>

<br/>
<h2>Serializing the UI hierachy</h2>

<p>
    So what would a "proper" toolkit look like? As computers got faster and
    rendering methods evolved, both browsers and UI toolkits moved away
    from a direct render-to-screen to a display-list or a "list of commands"
    for the renderer. Often times this command list is batched or computed
    against the last frame to minimize changes - while there is a small overhead,
    it is almost unnoticable.
</p>
<p>
    The next problem is that widget-specific data has to be either stored
    on the programmer side (in the application, using inheritance or traits)
    or in the framework (either using data attributes or - worse -
    global state modifying functions such as synchronous
    <code>setColor(RED); draw(); swap();</code> calls).
</p>
<p>
    So the only real "sane" way is to serialize the entire UI hierarchy
    and then perform layout, state and cache analysis in the toolkit.
    A good comparison is to compare XML to function call stacks - compare:
</p>

<code class="expand">&lt;div class="parent"&gt;
    &lt;div class="child"&gt;&lt;/div&gt;
    &lt;div class="child"&gt;&lt;/div&gt;
    &lt;div class="child"&gt;&lt;/div&gt;
&lt;/div&gt;</code>

    <p>with:</p>

<code class="expand">div(class="parent", children = [
    div(class="child")
    div(class="child")
    div(class="child")
])</code>

<p>
    Composing UI hierarchies via functions makes much more
    sense than composing UI hierarchies via inheritance
    because the latter is often language-specific and not
    supported in all languages, whereas functions are language
    agnostic. Additionally you can render UI nodes in parallel:
</p>

<code class="expand">let children = data_collection
    .par_iter()
    .map(|object| render_ui(object))
    .collect();

return parent.with_children(children);</code>

<br/>
<h2>Data access: A question of format and locality</h2>

<p>
    Great, so we need to serialize the UI into a tree structure.
    But where do we store our widget data? And in what format?
</p>
<p>
    Inheritance-based toolkits only allow one format: You have to
    inherit from a UI object and then construct your application
    as a series of UI objects. Azul however, stores the application
    data as an implementation-agnostinc <code>RefAny</code> struct:
    similar to <code>PyObject</code> or Javascripts <code>Object</code>
    it just stores "some data", but the toolkit doesn't know what the
    type is. You can upcast your data and wrap it in a <code>RefAny</code>
    via <code>RefAny::new</code> and then get immutable or mutable
    access again via <code>.downcast_ref()</code> or <code>.downcast_mut()</code>,
    respectively:
</p>

<code class="expand">let data = RefAny::new(5);
let data_ref = data.downcast_ref::&lt;usize&gt;();
println!("{}", data); // prints "5"</code>

<p>
    In Python, this process is transparent to the user,
    since every Python object is already wrapped in an opaque,
    reference-counted type, so it would look like this:
</p>

<code class="expand">data = RefAny(5); // RefAny&lt;PyObject&lt;PyInteger&gt;&gt;
print(str(data._py_data)) // prints "5"</code>

<p>
    While Rust only knows about the <code>RefAny</code>, the Python bindings
    automatically perform the front-and-back conversion from python objects.
    Both the object type and immutable / mutable reference count is checked
    at runtime: When all references to a <code>RefAny</code> are deleted, the internal
    object is deleted, too.
</p>

<p>Azul can store <code>RefAny</code> data in three locations:</p>

<br/>
<ol>
    <li><p><strong>Callback-local data:</strong> Data is stored directly on the callback: other callbacks cannot access this</p></li>
    <li><p><strong>Widget-local data:</strong>Stored on the UI XML nodes as a <code>dataset</code> property: Callbacks can have shared access to this data via <code>callback.get_dataset(node_id)</code>: but only if they know the <code>NodeId</code> that the dataset is attached to.</p></li>
    <li><p><strong>Application-global data:</strong>Stored inside the <code>App</code> struct: all layout callbacks are rendered from this data.</p></li>
</ol>

<p>
    A useful property is that <code>RefAny</code> structs can contain both callbacks
    and <code>RefAny</code> types themselves (nested <code>RefAny</code>). This way
    you can implement user-provided functionality by simply creating some fields and
    wrapper-methods that "wrap" a user-provided <code>RefAny</code> and a callback
    to be called when your custom event is ready:
</p>

<code class="expand">class MyWidget:
    def __init__():
        self.on_text_clicked = None

    # Allow users to set a custom event
    def set_on_text_clicked(data, callback):
        self.on_text_clicked = tuple(data, callback)

    def dom():
        dom = Dom.text("hello")
        if self.on_text_clicked is not None:
            dom.add_callback(On.MouseUp, self.on_text_clicked, _handle_event)
        return dom

# Note that data is the tuple(data, callback):
# you can use custom classes in order to customize callbacks
# in total, this provides more flexibility than @override
def _handle_event(data, callbackinfo):

    # "data" contains the **user-provided** callback
    # and a RefAny to **user-provided** data
    #
    # In this example, we you can now choose to
    # invoke or not invoke the callback depending on whether
    # the text of the object was hit

    text = callbackinfo.get_inline_text(callbackinfo.get_hit_node())
    if text is None:
        return Update.DoNothing

    text_hit = None
    hits = text.hit_test(info.get_hit_relative_to_item())
    if len(text_hits) != 0:
        text_hit = hits[0]
    else:
        return Update.DoNothing

    # do some default processing here
    print("before invoking user callback: text was hit")

    # invoke the user-provided function with additional information
    result = (data[1])(data[0], callbackinfo, text_hit)

    print("after invoking user callback: user returned" + str(result))

    # return the result of the user-provided callback to Azul
    return result
</code>

<p>
    The important part is that you can now customize the callback type
    (including the return type) and invoke the callback with extra data
    in order to do pre- and post-processing of user callbacks.
</p>

<br/>
<h2>Backreferences</h2>

<p>
    The programming pattern that naturally emerges from this architecture
    are "backreferences" or: the lower-level widget (<code>Dom.div</code>,
    <code>TextInput</code>, etc.) can take optional function pointers and
    data references to higher-level widgets (such as <code>NumberInput</code>,
    <code>EMailInput</code>), which then perform validation for you.
</p>

<code class="expand">class TextInput:
    text: String
    # "backreference" to user-provided callback + data
    user_focus_lost_callback: Optional[tuple(data, callback)]

    def __init__(text):
        self.text = text
        self.user_focus_lost_callback = None

    def set_on_focus_lost(data, callback):
        self.user_focus_lost_callback = tuple(data, callback)

    def dom():
        dom = Dom.text(self.text)
        refany = RefAny(self)
        dom.set_callback(On.TextInput, refany, _on_text_input)
        dom.add_callback(On.FocusLost, refany, _on_focus_lost)
        return dom

# callback knows that "data" is of type TextInput
def _on_text_input(data, callbackinfo):
    data.text += callbackinfo.get_keyboard_input().current_char
    callbackinfo.set_text_contents(callbackinfo.get_hit_node_id(), data.text)
    return Update.DoNothing

def _on_focus_lost(data, callbackinfo):
    user_cb = None
    if data.user_focus_lost_callback is None:
        return Update.DoNothing
    else:
        user_cb = data.user_focus_lost_callback

    # invoke the user-provided callback with the extra information
    return (user_cb[1])(user_cb[0], self.string)
</code>

<code class="expand">class NumberInput:
    number: Integer
    # "backreference" to user-provided callback + data
    on_number_input: Optional[tuple(data, callback)]

    def __init__(number):
        self.number = number
        self.on_number_input = None

    def set_on_number_input(data, callback):
        self.on_number_input = tuple(data, callback)

    def dom():
        ti = TextInput("{}".format(self.number))
        ti.set_on_focus_lost(RefAny(self), _validate_text_input_as_number)
        return

def _validate_text_input_as_number(data, callbackinfo, string):
    if data.on_number_input is None:
        return Update.DoNothing

    number = string_to_number(string)
    if number is None:
        return Update.DoNothing # input string is not a number

    # string has now been validated as a number
    # optionally can also validate min / max input, etc.
    (data.on_number_input[1])(data.on_number_input[0], callbackinfo, number)
</code>

<code class="expand">class MyApplication:
    user_age: None

    def __init__(initial_age):
        self.user_age = initial_age

def layoutFunc(data, layoutinfo):
    ni = NumberInput(initial_age)
    ni.set_on_number_input(data, _on_age_input)
    return ni.dom().style(Css.empty())

# will only get called when the text input is a valid number
def _on_age_input(data, callbackinfo, new_age):
    if new_age < 18:
        MsgBox.ok("You must be older than 18 to proceed")
        return Update.DoNothing
    else:
        data.user_age = new_age
        return Update.RefreshDom</code>
<code class="expand">app = App(MyApplication(18), AppConfig(LayoutSolver.Default))
app.run(WindowCreateOptions(layoutFunc))
</code>

<p>For the sake of brevity the code uses <code>RefAny(self)</code> to create
    new <code>RefAny</code> references. All built-in widgets use extra
    classes such as <code>TextInputOnClickedData</code>, <code>TextInputOnFocusLostData</code>
    to separate the <code>TextInput</code> from the data it has to carry
    even further.
</p>

<p>
    When the user presses the "TAB" or "shift + TAB" key, the current
    field loses focus and thereby tirgges an <code>On.FocusLost</code>
    and <code>On.FocusReceived</code> event. If it isn't already
    clear from reading the code, here is how the event "travels" through
    the classes (from the low-level <code>on_focus_lost</code> to the
    high-level <code>on_age_input</code>):
</p>

<code class="expand">1. _on_focus_lost(&mut RefAny&lt;TextInput&gt;)
2. _validate_text_input_as_number(&mut RefAny&lt;NumberInput&gt;, string)
3. _on_age_input(&mut RefAny&lt;MyApplication&gt;, number)
</code>

<p>
    The closer you get to the application, the more custom your function callback
    types will be. Note that the only "state" that gets passed into callbacks is
    via function arguments. Functional programmers can see this as an
    "application" of function parameters: the actual core state management
    is hidden completely.
</p>

<br/>
<h2>Non-linear widget hierarchies</h2>

<p>
    Let's take the example of a node graph: Draggable nodes can be added and removed,
    contain text, number and color inputs, check- and radio boxes, contain inputs and outputs
    (which need to validate their input and output types when they are connected).
</p>

<p>
    Node graphs are pretty much a litmus test to how flexible your UI toolkit is:
    In any object-oriented toolkit you will get problems designing the hierachy here.
</p>

<br/>
<img src="https://docs.unrealengine.com/Images/ProgrammingAndScripting/Blueprints/UserGuide/Nodes/SelectNode.jpg" style="width:100%;"/>

<p>
    Azul includess a built-in <code>NodeGraph</code> as a default widget, so let's
    take a look at the implementation:
</p>

<br/>
<br/>

<a href="$$ROOT_RELATIVE$$/guide">Back to overview</a>
