<h2>List of supported CSS attributes</h2>

<style type="text/css">
    .css-table {
        min-width: 700px !important;
        margin-top: 20px;
        margin-bottom: 20px;
        padding-left: 10px;
        font-family: monospace;
    }
    .css-row {
        display: block;
        padding-left:  5px;
    }
    .css-row:nth-child(even) {
        background:  #efefef;
    }
    .css-row:nth-child(odd) {
        background:  #fff;
    }
    .css-row-header {
        background: black !important;
        color: white;
        box-shadow: 0px 2px 2px #efefef;
    }
    .css-col, .css-col-header {
        font-family: monospace;
        display: inline-block;
        min-width: 200px;
    }
</style>

<p>
    This is a list of CSS attributes that are currently implemented. They work in
    the same way as on a regular web page, except if noted otherwise:
</p>

<div class="css-table">
    <div class="css-row css-row-header">
        <div class="css-col-header">name</div>
        <div class="css-col-header">example values</div>
    </div>
    <div class="css-row">
        <div class="css-col">display</div>
        <div class="css-col">block, inline-block, flex (default)</div>
    </div>
    <div class="css-row">
        <div class="css-col">float</div>
        <div class="css-col">left, right, both</div>
    </div>
    <div class="css-row">
        <div class="css-col">box-sizing</div>
        <div class="css-col">border-box, content-box</div>
    </div>
    <div class="css-row">
        <div class="css-col">color</div>
        <div class="css-col">red, green, ..., #efefef, rgb(), rgba(), hsl(), hsla()</div>
    </div>
    <div class="css-row">
        <div class="css-col">font-size</div>
        <div class="css-col">10px, 5pt, 40%, 10em, 5rem</div>
    </div>
    <div class="css-row">
        <div class="css-col">font-family</div>
        <div class="css-col">sans-serif, serif, ..., "Times New Roman"</div>
    </div>
    <div class="css-row">
        <div class="css-col">text-align</div>
        <div class="css-col">left, center, right</div>
    </div>
    <div class="css-row">
        <div class="css-col">letter-spacing</div>
        <div class="css-col">0.0 - infinite</div>
    </div>
    <div class="css-row">
        <div class="css-col">line-height</div>
        <div class="css-col">0.0 - infinite</div>
    </div>
    <div class="css-row">
        <div class="css-col">word-spacing</div>
        <div class="css-col">0.0 - infinite</div>
    </div>
    <div class="css-row">
        <div class="css-col">tab-width</div>
        <div class="css-col">0.0 - infinite</div>
    </div>
    <div class="css-row">
        <div class="css-col">cursor</div>
        <div class="css-col">help, wait, crosshair, grab, default, ...</div>
    </div>
    <div class="css-row">
        <div class="css-col">width</div>
        <div class="css-col">10px, 5%, 10rem, 5em</div>
    </div>
    <div class="css-row">
        <div class="css-col">height</div>
        <div class="css-col">10px, 5%, 10rem, 5em</div>
    </div>
    <div class="css-row">
        <div class="css-col">min-width</div>
        <div class="css-col">10px, 5%, 10rem, 5em</div>
    </div>
    <div class="css-row">
        <div class="css-col">min-height</div>
        <div class="css-col">10px, 5%, 10rem, 5em</div>
    </div>
    <div class="css-row">
        <div class="css-col">max-width</div>
        <div class="css-col">10px, 5%, 10rem, 5em</div>
    </div>
    <div class="css-row">
        <div class="css-col">max-height</div>
        <div class="css-col">10px, 5%, 10rem, 5em</div>
    </div>
    <div class="css-row">
        <div class="css-col">position</div>
        <div class="css-col">static (default), relative, absolute, fixed</div>
    </div>
    <div class="css-row">
        <div class="css-col">top</div>
        <div class="css-col">10px, 5%, 10rem, 5em (+position:absolute / fixed)</div>
    </div>
    <div class="css-row">
        <div class="css-col">right</div>
        <div class="css-col">10px, 5%, 10rem, 5em (+position:absolute / fixed)</div>
    </div>
    <div class="css-row">
        <div class="css-col">left</div>
        <div class="css-col">10px, 5%, 10rem, 5em (+position:absolute / fixed)</div>
    </div>
    <div class="css-row">
        <div class="css-col">bottom</div>
        <div class="css-col">10px, 5%, 10rem, 5em (+position:absolute / fixed)</div>
    </div>
    <div class="css-row">
        <div class="css-col">flex-wrap</div>
        <div class="css-col">wrap, no-wrap</div>
    </div>
    <div class="css-row">
        <div class="css-col">flex-direction</div>
        <div class="css-col">row, column, row-reverse, column-reverse</div>
    </div>
    <div class="css-row">
        <div class="css-col">flex-grow</div>
        <div class="css-col">0.0 - infinite</div>
    </div>
    <div class="css-row">
        <div class="css-col">flex-shrink</div>
        <div class="css-col">0.0 - infinite</div>
    </div>
    <div class="css-row">
        <div class="css-col">justify-content</div>
        <div class="css-col">stretch, center, flex-start, flex-end, space-between, space-around</div>
    </div>
    <div class="css-row">
        <div class="css-col">align-items</div>
        <div class="css-col">stretch, center, flex-start, flex-end</div>
    </div>
    <div class="css-row">
        <div class="css-col">align-content</div>
        <div class="css-col">stretch, center, flex-start, flex-end, space-between, space-around</div>
    </div>
    <div class="css-row">
        <div class="css-col">overflow, overflow[-x, -y]</div>
        <div class="css-col">auto (default), scroll, hidden, visible</div>
    </div>
    <div class="css-row">
        <div class="css-col">padding[-top, ...]</div>
        <div class="css-col">10px, 5%, 10rem, 5em </div>
    </div>
    <div class="css-row">
        <div class="css-col">margin[-top, ...]</div>
        <div class="css-col">10px, 5%, 10rem, 5em </div>
    </div>
    <div class="css-row">
        <div class="css-col">background</div>
        <div class="css-col">red, [linear-, radial-, conic-]gradient(), image(id)</div>
    </div>
    <div class="css-row">
        <div class="css-col">background-position</div>
        <div class="css-col">10% 10%, 10px 10px, left top</div>
    </div>
    <div class="css-row">
        <div class="css-col">background-size</div>
        <div class="css-col">auto, cover, contain, 10% 40%, 100px 200px</div>
    </div>
    <div class="css-row">
        <div class="css-col">background-repeat</div>
        <div class="css-col">repeat, no-repeat</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-radius</div>
        <div class="css-col">10px, 5%, 10rem, 5e</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-top-left-radius</div>
        <div class="css-col">10px, 5%, 10rem, 5em</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-top-right-radius</div>
        <div class="css-col">10px, 5%, 10rem, 5em </div>
    </div>
    <div class="css-row">
        <div class="css-col">border-bottom-left-radius</div>
        <div class="css-col">10px, 5%, 10rem, 5em </div>
    </div>
    <div class="css-row">
        <div class="css-col">border-bottom-right-radius</div>
        <div class="css-col">10px, 5%, 10rem, 5em </div>
    </div>
    <div class="css-row">
        <div class="css-col">border, border-[top, ...]</div>
        <div class="css-col">1px solid red, 10px dotted #efefef</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-top-width</div>
        <div class="css-col">10px, 10rem, 5em (NO PERCENTAGE)</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-right-width</div>
        <div class="css-col">10px, 10rem, 5em (NO PERCENTAGE)</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-left-width</div>
        <div class="css-col">10px, 10rem, 5em (NO PERCENTAGE)</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-bottom-width</div>
        <div class="css-col">10px, 10rem, 5em (NO PERCENTAGE)</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-top-style</div>
        <div class="css-col">solid, dashed, dotted, ...</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-right-style</div>
        <div class="css-col">solid, dashed, dotted, ...</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-left-style</div>
        <div class="css-col">solid, dashed, dotted, ...</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-bottom-style</div>
        <div class="css-col">solid, dashed, dotted, ...</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-top-color</div>
        <div class="css-col">red, green, ..., #efefef, rgb(), rgba(), hsl(), hsla()</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-right-color</div>
        <div class="css-col">red, green, ..., #efefef, rgb(), rgba(), hsl(), hsla()</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-left-color</div>
        <div class="css-col">red, green, ..., #efefef, rgb(), rgba(), hsl(), hsla()</div>
    </div>
    <div class="css-row">
        <div class="css-col">border-bottom-color</div>
        <div class="css-col">red, green, ..., #efefef, rgb(), rgba(), hsl(), hsla()</div>
    </div>
    <div class="css-row">
        <div class="css-col">opacity</div>
        <div class="css-col">0.0 - 1.0</div>
    </div>
    <div class="css-row">
        <div class="css-col">transform</div>
        <div class="css-col">matrix(), translate(), scale(), rotate(), ...</div>
    </div>
    <div class="css-row">
        <div class="css-col">perspective-origin</div>
        <div class="css-col">100px 100px, 50% 50%</div>
    </div>
    <div class="css-row">
        <div class="css-col">transform-origin</div>
        <div class="css-col">100px 100px, 50% 50%</div>
    </div>
    <div class="css-row">
        <div class="css-col">backface-visibility</div>
        <div class="css-col">visible (default), hidden</div>
    </div>
    <div class="css-row">
        <div class="css-col">box-shadow</div>
        <div class="css-col">0px 0px 10px black inset</div>
    </div>
    <div class="css-row">
        <div class="css-col">background-color</div>
        <div class="css-col">red, green, #efefefaa, rgb(), rgba(), hsl(), hsla()</div>
    </div>
    <div class="css-row">
        <div class="css-col">background-image</div>
        <div class="css-col">id("my-id")</div>
    </div>

</div>

<p>
    You can limit the inheritance of properties either to direct children only (using <code>></code>) or to all children
    (using <code> </code>). I.e. <code>div#my_div.class</code> has a different effect than <code>div #my_div .class</code>
    or <code>div > #my_div > .class</code>.
</p><br/>

<p>
    If you want to add images, you need to add them to the application first
    (via <code>app.add_image(id, ImageRef)</code>), then you can reference the <code>id</code>
    in your CSS background-content:
</p>
<code class="expand">const IMAGE: &[u8] = include_bytes!("my-image.png");

struct Data { }

extern "C" fn myLayoutFn(data: &mut RefAny, _: LayoutInfo) -> StyledDom {
    let mut css = Css::from_str("div { background-image: id('my-id'); }");
    Dom::div().style(&mut css)
}

fn main() {
    let config = AppConfig::new(LayoutSolver::Default);
    let mut app = App::new(RefAny::new(Data { }), config);

    // load the image before the app starts
    let decoded = RawImage::decode_image_bytes_any(IMAGE).unwrap();
    let imageref = ImageRef::raw_image(image).unwrap();
    app.add_image("my-id", imageref);

    app.run(WindowCreateOptions::new(myLayoutFn));
}

// or load it dynamically inside of a callback:
extern "C" fn loadImageOnClick(data: &mut RefAny, mut callbackinfo: CallbackInfo) -> Update {
    let decoded = RawImage::decode_image_bytes_any(IMAGE).unwrap();
    let imageref = ImageRef::raw_image(image).unwrap();
    callbackinfo.add_image("my-id", imageref);
    Update::DoNothing
}</code>
<br/>

<p>If Azul can't find an image, the background will be rendered as transparent.</p>

<br/>
<br/>

<h2>DOM styling</h2>

<br/>

<p>
    A <code>Dom</code> object + a <code>Css</code> object results in a <code>StyledDom</code>.
    A <code>StyledDom</code> can be restyled via <code>.restyle(Css)</code> if necessary,
    however, this only exists so that the application can be restyled via global themes.
</p>
<p>
    CSS properties can be set either on the nodes themselves (as inline properties),
    or via a CSS stylesheet:
</p>

<code class="expand">let mut dom = Dom::div().with_id("my_id").with_class("my_class");
let styled_dom = dom.style(&mut Css::from_str("
    #my_id { width: 100px; height: 100px; background: red; }
"));
</code>
<br/>
<br/>

<h2>Builtin CSS classes</h2>

<p>
    CSS classes starting with <code>.__azul-native-</code> are reserved for builtin widgets.
    You can use these classes in the <code>.restyle()</code> method to restyle native buttons
    while retaining all the behaviour of the code:
</p>

<code class="expand">.__azul-native-button-container { } // style the container of the Button widget
.__azul-native-button-content { } // style the text of the Button widget

.__azul-native-checkbox-container { } // style the container of the CheckBox
.__azul-native-checkbox-content { } // style the content of the CheckBox

.__azul_native_color_input { } // style the content of the ColorInput

.__azul-native-label { } // style the text of the Label widget

.__azul-native-text-input-container { } // style the container of the TextInput widget
.__azul-native-text-input-label { } // style the text of the TextInput widget

__azul-native-progressbar-container { } // style the ProgressBar container
__azul-native-progressbar-bar { } // style the ProgressBar bar
__azul-native-progressbar-label { } // style the ProgressBar label
</code><br/>
<br/>

<h2>Animating CSS properties</h2>

<p>
    In any callback you can animate CSS properties if they are animatable using
    <code>callbackinfo.start_animation(nodeid, Animation)</code>. Azul does not
    support animations via CSS. The <code>start_animation</code> function
    returns a <code>TimerId</code>, which you can use to call <code>stop_timer</code>
    when you want to stop the animation again. For UI transitions it generally
    makes sense to set <code>relayout_on_finish</code>, which will call your
    main <code>layout()</code> function again when the animation / transition
    is finished.
</p>

<code class="expand">def on_click(data, info):
    # TODO: ugly - need to create helper functions!
    anim = Animation(
        from=CssProperty.Opacity(OpacityValue.Exact(StyleOpacity(PercentageValue(FloatValue(0))))),
        to=CssProperty.Opacity(OpacityValue.Exact(StyleOpacity(PercentageValue(FloatValue(1000))))),
        duration=Duration.System(SystemTimeDiff(secs=1,nanos=0)),
        repeat=AnimationRepeat.NoRepeat,
        easing=AnimationEasing.EaseInOut,
        relayout_on_finish=True
    )
    timer = info.start_animation(info.get_hit_node(), anim)</code><br/>

<p>Additionally, you can also use <code>set_css_property(nodeid, CssProperty)</code> in order
to change a CSS property once from a callback:</p>

<code class="expand">def on_drag_start(data, info):
    drag = info.get_mouse_state().current_drag
    new_prop = CssProperty.Transform(TransformValue.Exact(StyleTransform.Position(new_x, new_y)))
    info.set_css_property(info.get_hit_node(), new_prop)</code><br/>

<p>Internally, Azul does nothing other than to start a timer with a callback function that only
interpolates between the <code>Animation.from</code> and <code>Animation.to</code> CSS values.</p>
<br/>

<br/>

<h2>Layout system</h2>

<p>The layout system roughly follows the CSS flexbox model, although not quite:</p>

<br/>
<ul>
    <li><p>First, all items are set to either their min-width, the intrinsic content width (text / image) or 0px.</p></li>
    <li><p>Then, flex items are recursively expanded according to their flex-grow values, depending on how much space the parent has available</p></li>
    <li><p><code>position:absolute</code> items are not expanded</p></li>
    <li><p>After all sizes have been calculated, text is reflown if it doesn't fit in the bounds of the parent</p></li>
    <li><p>Items are positioned on the screen, depending on the <code>flex-direction</code> value.</p></li>
</ul>

<p>
    Azul is completely HiDPI aware, however, there is no em-cascading done yet.
    Azul calculates <code>1em = 16px</code>.
</p>

<br/>
<br/>

<a href="$$ROOT_RELATIVE$$/guide">Back to overview</a>
