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