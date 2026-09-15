//! The hand-written part of the D binding.
//!
//! Everything here is independent of api.json: the refcounted box every handle
//! struct points to, the GC-rooted boxes that keep D objects and delegates
//! alive while libazul holds a `RefAny` naming them, the exceptions, and the
//! `string` conversions. The generated declarations (`wrappers.rs`) build on
//! these names. Internal names start with `_azul` / `_Azul` so they can never
//! collide with a generated member (api.json names are camelCased and never
//! start with `_`).

/// The imports every idiomatic module needs (the single-file binding has them
/// once at the top).
pub const IMPORTS: &str = "public import std.typecons : Nullable;
private import core.memory : GC;
private import core.stdc.stdlib : abort, calloc, free, malloc;
private import core.stdc.string : memcpy;
private import std.traits : isCallable, Parameters;
";

pub const RUNTIME: &str = r##"// ----------------------------------------------------------------------------
// Runtime: handle boxes, closure handles, exceptions, string conversions.
// ----------------------------------------------------------------------------

/// Thrown when an azul value is used after it was moved into a call, or when a
/// handle was never initialized.
///
/// Passing a handle BY VALUE (`body.withChild(label)`) hands its value to
/// libazul, the same as a Rust move; every copy of that handle shares the value,
/// so using `label` (or a copy of it) afterwards throws this. Call `dup()`
/// first to keep an independent copy.
final class AzulMovedError : Error
{
    this(string type, bool empty, string file = __FILE__, size_t line = __LINE__) nothrow @safe pure
    {
        super(empty
            ? "azul: this " ~ type ~ " is empty: it is " ~ type ~ ".init, not a value made by a constructor or factory"
            : "azul: " ~ type ~ " was moved into an earlier call and can no longer be used (call dup() before passing it on to keep a copy)",
            file, line);
    }
}

/// Thrown when a tagged union is read as a variant it does not hold.
final class AzulVariantError : Error
{
    this(string type, string variant, string file = __FILE__, size_t line = __LINE__) nothrow @safe pure
    {
        super("azul: this " ~ type ~ " does not hold `" ~ variant ~ "` (check `tag` first)", file, line);
    }
}

/// Base class of every exception an azul method throws.
abstract class AzulException : Exception
{
    this(string msg, string file = __FILE__, size_t line = __LINE__) nothrow @safe pure
    {
        super(msg, file, line);
    }
}

/// Thrown by a method whose Rust return type is a `Result` that came back
/// `Err`; `error` is the converted `Err` value.
final class ResultException(E) : AzulException
{
    /// The `Err` value.
    E error;

    this(E error, string file = __FILE__, size_t line = __LINE__)
    {
        super(_azulDescribe(error), file, line);
        this.error = error;
    }
}

package string _azulDescribe(E)(ref E e)
{
    static if (is(E : const(char)[]))
        return e.idup;
    else static if (__traits(compiles, e.toString()))
        return e.toString();
    else
        return E.stringof;
}

/// Rust `Default` for any azul type: `defaultValue!Dom()`, `defaultValue!ButtonType()`.
T defaultValue(T)() if (is(T == struct) && __traits(hasMember, T, "defaultValue"))
{
    return T.defaultValue();
}

// ---- handle boxes ------------------------------------------------------------
//
// A handle struct holds one `_AzulRc*`. An OWNING box stores the C value right
// after its header and runs the type's `_delete` when the last handle goes
// away (unless the value was moved into libazul). A VIEW box points into a field
// of another value and keeps that value's root box alive, so
// `window.windowState.title = "x"` writes through and a view never dangles.

package struct _AzulRc
{
    size_t refs;
    _AzulRc* root;
    void* ptr;
    void function(void*) nothrow @nogc drop;
    bool moved;
}

package enum size_t _azulValueOffset = (_AzulRc.sizeof + 15) & ~cast(size_t) 15;

package _AzulRc* _azulOwn(scope const(void)* raw, size_t size, void function(void*) nothrow @nogc drop) nothrow @nogc @trusted
{
    auto rc = cast(_AzulRc*) calloc(1, _azulValueOffset + (size ? size : 1));
    if (rc is null)
        abort();
    rc.refs = 1;
    rc.drop = drop;
    rc.ptr = cast(void*) rc + _azulValueOffset;
    memcpy(rc.ptr, raw, size);
    return rc;
}

package _AzulRc* _azulView(void* field, const(_AzulRc)* owner) nothrow @nogc @trusted
{
    auto rc = cast(_AzulRc*) calloc(1, _AzulRc.sizeof);
    if (rc is null)
        abort();
    auto root = cast(_AzulRc*)(owner.root !is null ? owner.root : owner);
    root.refs++;
    rc.refs = 1;
    rc.root = root;
    rc.ptr = field;
    return rc;
}

package void _azulRetain(const(_AzulRc)* rc) nothrow @nogc @trusted
{
    if (rc !is null)
        (cast(_AzulRc*) rc).refs++;
}

package void _azulRelease(const(_AzulRc)* crc) nothrow @nogc @trusted
{
    auto rc = cast(_AzulRc*) crc;
    if (rc is null || --rc.refs != 0)
        return;
    if (rc.root !is null)
        _azulRelease(rc.root);
    else if (!rc.moved && rc.drop !is null)
        rc.drop(rc.ptr);
    free(rc);
}

/// The address of a handle's value; throws AzulMovedError when there is none.
package void* _azulPtr(const(_AzulRc)* rc, string type) nothrow @trusted
{
    if (rc is null)
        throw new AzulMovedError(type, true);
    if (rc.moved || (rc.root !is null && rc.root.moved))
        throw new AzulMovedError(type, false);
    return cast(void*) rc.ptr;
}

/// Moves an owned value out of its box (the box stops running `_delete`).
package void _azulMarkMoved(_AzulRc* rc) nothrow @nogc @trusted
{
    rc.moved = true;
}

package noreturn _azulNotMovable(string type) nothrow @safe
{
    throw new Error("azul: a " ~ type ~ " that is a field of another value cannot be moved out of it (the type has no dup())");
}

package float _azulPartialOrdering(ubyte o) nothrow @nogc pure @safe
{
    return o == 0 ? -1f : o == 1 ? 0f : o == 2 ? 1f : float.nan;
}

package int _azulOrdering(ubyte o) nothrow @nogc pure @safe
{
    return o == 0 ? -1 : o == 1 ? 0 : 1;
}

// ---- strings -----------------------------------------------------------------

private immutable ubyte _azulZeroByte = 0;

/// Copies a D string into a new AzString (the caller owns it).
package AzString _azulString(scope const(char)[] s) nothrow @nogc @trusted
{
    if (s.length == 0)
        return AzString_fromUtf8(cast(ubyte*) &_azulZeroByte, 0);
    return AzString_fromUtf8(cast(ubyte*) s.ptr, s.length);
}

/// Copies the bytes of a borrowed AzString.
package string _azulStringOf(const(AzString)* s) nothrow @trusted
{
    auto v = &s.vec;
    if (v.ptr is null || v.len == 0)
        return "";
    return (cast(immutable(char)*) v.ptr)[0 .. v.len].idup;
}

/// Copies the bytes of an owned AzString and frees it.
package string _azulTakeString(AzString s) nothrow @trusted
{
    auto r = _azulStringOf(&s);
    AzString_delete(&s);
    return r;
}

// ---- objects and delegates held by libazul -------------------------------------
//
// A RefAny this binding creates carries one pointer: a `_AzulHeld` in GC
// memory, registered with GC.addRoot because the GC cannot see libazul's heap.
// libazul runs `_azulHeldDestructor` when the last clone of the RefAny drops,
// which removes the root.

package final class _AzulHeld
{
    Object object;
    Object closure;
}

package final class _AzulClosure(DG)
{
    DG call;

    this(DG call) nothrow @safe
    {
        this.call = call;
    }
}

/// RTTI id of every RefAny this binding creates ("D_AZUL_1").
package enum ulong _azulTypeId = 0x445F_415A_554C_5F31;

package extern (C) void _azulHeldDestructor(void* ptr) nothrow @nogc
{
    if (ptr !is null)
        GC.removeRoot(*cast(void**) ptr);
}

/// A new RefAny (refcount 1) naming `object` and/or a closure.
package AzRefAny _azulRefAny(Object object, Object closure = null) @trusted
{
    auto held = new _AzulHeld;
    held.object = object;
    held.closure = closure;
    void* box = cast(void*) held;
    GC.addRoot(box);
    AzGlVoidPtrConst data;
    data.ptr = cast(void*)&box;
    data.run_destructor = false;
    return AzRefAny_newC(data, (void*).sizeof, (void*).alignof, _azulTypeId,
        _azulString("D"), &_azulHeldDestructor, 0, 0);
}

package _AzulHeld _azulHeld(AzRefAny* refany) nothrow @trusted
{
    if (refany is null || !AzRefAny_isType(refany, _azulTypeId))
        return null;
    auto data = AzRefAny_getDataPtr(refany);
    if (data is null)
        return null;
    return cast(_AzulHeld)*cast(void**) data;
}

/// The D object a callback's data RefAny names, as `T`.
package T _azulObject(T)(AzRefAny* refany) @trusted
{
    if (auto h = _azulHeld(refany))
        if (auto o = cast(T) h.object)
            return o;
    throw new Error("azul: the callback data is not a " ~ T.stringof
        ~ " (pass the object itself as the data argument)");
}

/// The delegate of type `DG` a RefAny names, if any.
package DG _azulClosure(DG)(AzRefAny* refany) nothrow @trusted
{
    auto h = _azulHeld(refany);
    if (h is null)
        return null;
    auto c = cast(_AzulClosure!DG) h.closure;
    return c is null ? null : c.call;
}

/// Callbacks can arrive on threads libazul started; D code needs the thread
/// registered with druntime before it touches the GC.
package void _azulAttachThread() nothrow
{
    import core.thread : Thread, thread_attachThis;

    try
    {
        if (Thread.getThis() is null)
            thread_attachThis();
    }
    catch (Exception)
    {
    }
}

/// A D exception must never unwind into libazul's frames: an escaping one is
/// reported and ends the process.
package noreturn _azulUncaught(Throwable t, string where) nothrow @trusted
{
    import core.stdc.stdio : fflush, fprintf, stderr;

    try
    {
        auto s = t.toString();
        fprintf(stderr, "azul: unhandled exception in a %.*s callback:\n%.*s\n",
            cast(int) where.length, where.ptr, cast(int) s.length, s.ptr);
    }
    catch (Throwable)
    {
    }
    fflush(stderr);
    abort();
}

package noreturn _azulNoClosure(string where) nothrow @trusted
{
    import core.stdc.stdio : fflush, fprintf, stderr;

    fprintf(stderr, "azul: no D callable is registered for this %.*s\n", cast(int) where.length, where.ptr);
    fflush(stderr);
    abort();
}

"##;
