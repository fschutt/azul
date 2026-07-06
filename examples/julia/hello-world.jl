# Azul counter example — Julia.
#
# Run:  AZUL_LIB=$PWD/libazul.so julia hello-world.jl
# See README.md for the per-OS library name and PowerShell invocation.
#
# `@cfunction` mints real C function pointers from on_click/layout, passed
# C-direct to the setters — no host-invoker.

include(joinpath(@__DIR__, "azul", "azul.jl"))
using .Azul

# isbits model + a runtime-unique type id: the address of a module-global
# `Ref` we never read or write. POD model → no-op destructor.

struct MyDataModel
    counter::UInt32
end

const MY_DATA_TOKEN = Ref{UInt8}(0)

my_data_type_id() = UInt64(UInt(pointer_from_objref(MY_DATA_TOKEN)))

# `@cfunction`-compatible destructor (does nothing — the model is POD).
my_data_destructor(::Ptr{Cvoid})::Cvoid = nothing

# A raw void pointer to the storage of a `Ref` (valid under GC.@preserve).
vptr(r::Ref) = Ptr{Cvoid}(pointer_from_objref(r))

function my_data_upcast(model::MyDataModel)
    # AzRefAny_newC copies the bytes into its own heap allocation, so a
    # pointer to a Ref-boxed local is fine; run_destructor=false ⇒ libazul
    # won't free ours.
    local_ref = Ref(model)
    return GC.@preserve local_ref begin
        wrapper = Azul.AzGlVoidPtrConst(vptr(local_ref), false)
        dtor = @cfunction(my_data_destructor, Cvoid, (Ptr{Cvoid},))
        Azul.AzRefAny_newC(
            wrapper,
            Csize_t(sizeof(MyDataModel)),
            Csize_t(Base.datatype_alignment(MyDataModel)),
            my_data_type_id(),
            Azul.az_string("MyDataModel"),
            dtor,
            C_NULL, # no serialize_fn
            C_NULL, # no deserialize_fn
        )
    end
end

# Returns a `Ptr{MyDataModel}` into the refcounted allocation, or a null
# pointer if the type-id check fails. `dref` must be kept alive by the
# caller (GC.@preserve) for the duration of the pointer's use.
function my_data_ptr(dref::Ref{Azul.AzRefAny})
    p = vptr(dref)
    Azul.AzRefAny_isType(p, my_data_type_id()) || return Ptr{MyDataModel}(C_NULL)
    raw = Azul.AzRefAny_getDataPtr(p)
    return Ptr{MyDataModel}(raw)
end

# ── Callback: button click ────────────────────────────────────────────

function on_click(data::Azul.AzRefAny, info::Azul.AzCallbackInfo)::Azul.AzUpdate
    dref = Ref(data)
    return GC.@preserve dref begin
        mp = my_data_ptr(dref)
        if mp == Ptr{MyDataModel}(C_NULL)
            Azul.AzUpdate_DoNothing
        else
            m = unsafe_load(mp)
            unsafe_store!(mp, MyDataModel(m.counter + UInt32(1)))
            Azul.AzUpdate_RefreshDom
        end
    end
end

# ── Layout callback ───────────────────────────────────────────────────

function layout(data::Azul.AzRefAny, info::Azul.AzLayoutCallbackInfo)::Azul.AzDom
    dref = Ref(data)
    counter = GC.@preserve dref begin
        mp = my_data_ptr(dref)
        mp == Ptr{MyDataModel}(C_NULL) ? nothing : unsafe_load(mp).counter
    end
    counter === nothing && return Azul.AzDom_createBody()

    # Counter label (wrapped in a div so the font-size sticks).
    label = Azul.AzDom_createText(Azul.az_string(string(counter)))

    label_wrapper = Ref(Azul.AzDom_createDiv())
    font_size = Azul.AzStyleFontSize_px(32.0f0)
    css_prop = Azul.AzCssProperty_fontSize(font_size)
    cond = Azul.AzCssPropertyWithConditions_simple(css_prop)
    GC.@preserve label_wrapper begin
        Azul.AzDom_addCssProperty(vptr(label_wrapper), cond)
        Azul.AzDom_addChild(vptr(label_wrapper), label)
    end

    # AzButton_setOnClick takes the bare fn-pointer typedef directly.
    button = Ref(Azul.AzButton_create(Azul.az_string("Increase counter")))
    on_click_ptr = @cfunction(on_click, Azul.AzUpdate, (Azul.AzRefAny, Azul.AzCallbackInfo))
    button_dom = GC.@preserve button dref begin
        Azul.AzButton_setButtonType(vptr(button), Azul.AzButtonType_Primary)
        data_clone = Azul.AzRefAny_clone(vptr(dref))
        Azul.AzButton_setOnClick(vptr(button), data_clone, on_click_ptr)
        Azul.AzButton_dom(button[])
    end

    # Body.
    body = Ref(Azul.AzDom_createBody())
    GC.@preserve body begin
        Azul.AzDom_addChild(vptr(body), label_wrapper[])
        Azul.AzDom_addChild(vptr(body), button_dom)
    end
    return body[]
end

# ── Main ──────────────────────────────────────────────────────────────

function main()
    model = MyDataModel(UInt32(5))
    data = my_data_upcast(model)

    layout_ptr = @cfunction(layout, Azul.AzDom, (Azul.AzRefAny, Azul.AzLayoutCallbackInfo))
    window = Azul.AzWindowCreateOptions_create(layout_ptr)

    # isbits structs are immutable — customize the window with functional
    # updates (`setfields`) instead of field assignment.
    ws = window.window_state
    window = Azul.setfields(window;
        window_state = Azul.setfields(ws;
            title = Azul.az_string("Hello World"),
            size = Azul.setfields(ws.size;
                dimensions = Azul.setfields(ws.size.dimensions; width = 400.0f0, height = 300.0f0)),
            # NoTitleAutoInject: OS draws the window buttons; framework
            # injects a draggable Titlebar.
            flags = Azul.setfields(ws.flags;
                decorations = Azul.AzWindowDecorations_NoTitleAutoInject,
                background_material = Azul.AzWindowBackgroundMaterial_Sidebar)))

    app = Ref(Azul.AzApp_create(data, Azul.AzAppConfig_create()))
    GC.@preserve app begin
        Azul.AzApp_run(vptr(app), window)
    end
end

main()
