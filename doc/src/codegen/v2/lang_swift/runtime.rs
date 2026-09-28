//! The hand-written part of the Swift binding.
//!
//! Everything here is independent of api.json: the ownership base classes
//! every generated class inherits, the handle boxes that keep Swift objects
//! and closures alive while libazul holds a `RefAny` naming them, and the
//! `String` conversions. The generated declarations (`wrappers.rs`) build on
//! these names. Internal names start with `_` so they never collide with a
//! generated member.

pub const RUNTIME: &str = r##"// ----------------------------------------------------------------------------
// Runtime: ownership base classes, closure handles, String conversions.
// ----------------------------------------------------------------------------

/// Stops the program when an object is used after it was moved into a call.
///
/// Passing a class instance BY VALUE (`body.withChild(label)`) hands it to
/// libazul, the same as a Rust move. Using `label` afterwards stops here with
/// the type's name. Call `copy()` first to keep one.
internal func _azulMoved(_ type: Any.Type) -> Never {
    preconditionFailure("azul: \(type) was moved into an earlier call and can no longer be used (call copy() before passing it on to keep one)")
}

/// The error a throwing method raises when its Rust `Result` is `Err`.
public struct AzulError<E>: Error, CustomStringConvertible {
    /// The converted `Err` value.
    public let error: E

    public init(_ error: E) {
        self.error = error
    }

    public var description: String {
        return "\(error)"
    }
}

/// Base of every generated class.
///
/// An object either OWNS its value (`_root == nil`: the value lives in its own
/// allocation and `deinit` runs the type's destructor) or is a VIEW into a
/// field of another object (`_root` keeps that owner alive, writes go straight
/// into the owner's memory, `deinit` frees nothing).
public class AzulObject {
    internal let _root: AzulObject?
    internal var _moved: Bool = false

    internal init(_root root: AzulObject?) {
        if let r = root {
            _root = r._root ?? r
        } else {
            _root = nil
        }
    }

    internal final var _isMoved: Bool {
        if let r = _root {
            return r._moved
        }
        return _moved
    }
}

/// Storage shared by every class that wraps the C value `Raw`.
public class AzulValue<Raw>: AzulObject {
    internal let _ptr: UnsafeMutablePointer<Raw>

    /// Takes ownership of `raw`.
    internal required init(_own raw: Raw) {
        _ptr = UnsafeMutablePointer<Raw>.allocate(capacity: 1)
        _ptr.initialize(to: raw)
        super.init(_root: nil)
    }

    /// A view into memory owned by `root`.
    internal required init(_view ptr: UnsafeMutablePointer<Raw>, root: AzulObject) {
        _ptr = ptr
        super.init(_root: root)
    }

    deinit {
        if _root == nil {
            if !_moved {
                Self._drop(_ptr)
            }
            _ptr.deallocate()
        }
    }

    /// Runs the C destructor (Rust `Drop`); overridden by every class that has one.
    internal class func _drop(_ p: UnsafeMutablePointer<Raw>) {}

    /// An independent copy of the value behind `p` (Rust `Clone`), or nil.
    internal class func _copyRaw(_ p: UnsafeMutablePointer<Raw>) -> Raw? {
        return nil
    }

    /// A bitwise copy is a real copy (Rust `Copy`).
    internal class var _isCopy: Bool {
        return false
    }

    /// The address of the value, for calls that borrow it.
    internal final var _address: UnsafeMutablePointer<Raw> {
        if _isMoved {
            _azulMoved(Self.self)
        }
        return _ptr
    }

    /// Moves the value out: an owner is marked moved, a view is copied, a
    /// `Copy` value is copied.
    internal final func _take() -> Raw {
        if _isMoved {
            _azulMoved(Self.self)
        }
        if Self._isCopy {
            return _ptr.pointee
        }
        if _root != nil {
            guard let c = Self._copyRaw(_ptr) else {
                preconditionFailure("azul: \(Self.self) has no copy(), so it cannot be moved out of the field it is borrowed from")
            }
            return c
        }
        _moved = true
        return _ptr.pointee
    }

    /// For builders that consume `self` and return the new value: reads the
    /// value without marking it moved; `_replace` writes the result back.
    internal final func _value() -> Raw {
        if _isMoved {
            _azulMoved(Self.self)
        }
        return _ptr.pointee
    }

    internal final func _replace(_ raw: Raw) {
        _ptr.pointee = raw
    }
}

/// What a `RefAny` created by this binding names: an object, a closure, or
/// both. The RefAny's payload is a retained pointer to the box; libazul runs
/// `_Handles.destructor` when the last clone drops, which releases it.
internal final class _Held {
    let object: AnyObject?
    let closure: AnyObject?

    init(object: AnyObject?, closure: AnyObject?) {
        self.object = object
        self.closure = closure
    }
}

internal final class _Closure<F> {
    let call: F

    init(_ call: F) {
        self.call = call
    }
}

internal enum _Handles {
    /// RTTI id of every RefAny this binding creates ("SWIFTAZ1").
    static let typeId: UInt64 = 0x5357_4946_5441_5A31

    static let destructor: AzRefAnyDestructorType = { (ptr: UnsafeMutableRawPointer?) -> Void in
        guard let ptr = ptr else {
            return
        }
        let box: UnsafeRawPointer = ptr.load(as: UnsafeRawPointer.self)
        Unmanaged<_Held>.fromOpaque(box).release()
    }

    /// A new RefAny (refcount 1) naming `object` and/or `closure`.
    static func refany(object: AnyObject?, closure: AnyObject?) -> AzRefAny {
        let held: _Held = _Held(object: object, closure: closure)
        var payload: UnsafeRawPointer = UnsafeRawPointer(Unmanaged.passRetained(held).toOpaque())
        let size: Int = MemoryLayout<UnsafeRawPointer>.size
        let align: Int = MemoryLayout<UnsafeRawPointer>.alignment
        let name: AzString = _Native.azString("Swift")
        return withUnsafePointer(to: &payload) { (p: UnsafePointer<UnsafeRawPointer>) -> AzRefAny in
            let data: AzGlVoidPtrConst = AzGlVoidPtrConst(ptr: UnsafeRawPointer(p), run_destructor: false)
            return AzRefAny_newC(data, size, align, typeId, name, destructor, 0, 0)
        }
    }

    static func held(_ refany: UnsafeMutablePointer<AzRefAny>) -> _Held? {
        guard AzRefAny_isType(refany, typeId) else {
            return nil
        }
        guard let data: UnsafeRawPointer = AzRefAny_getDataPtr(refany) else {
            return nil
        }
        let box: UnsafeRawPointer = data.load(as: UnsafeRawPointer.self)
        return Unmanaged<_Held>.fromOpaque(box).takeUnretainedValue()
    }

    /// The Swift object a callback's data RefAny names, as `T`, or nil when
    /// it names something else: the application registered a model of a
    /// different type, or the callback was registered from another binding.
    /// The trampoline reports that through libazul's log and hands back the
    /// callback's fallback, rather than stopping the process.
    static func object<T: AnyObject>(_ refany: UnsafeMutablePointer<AzRefAny>, _ type: T.Type) -> T? {
        if let h = held(refany), let o = h.object as? T {
            return o
        }
        return RefAny(_own: AzRefAny_clone(refany)) as? T
    }

    /// The closure of type `F` a RefAny names, if any.
    static func closure<F>(_ refany: UnsafeMutablePointer<AzRefAny>, _ type: F.Type) -> F? {
        guard let h = held(refany), let c = h.closure as? _Closure<F> else {
            return nil
        }
        return c.call
    }
}

internal enum _Native {
    /// Copies a Swift String into a new AzString (the caller owns it).
    static func azString(_ s: String) -> AzString {
        var u: String = s
        return u.withUTF8 { (b: UnsafeBufferPointer<UInt8>) -> AzString in
            if let base = b.baseAddress {
                return AzString_fromUtf8(base, b.count)
            }
            var zero: UInt8 = 0
            return AzString_fromUtf8(&zero, 0)
        }
    }

    /// Copies the bytes of a borrowed AzString.
    static func string(_ p: UnsafePointer<AzString>) -> String {
        let v: AzU8Vec = p.pointee.vec
        guard let base = v.ptr, v.len > 0 else {
            return ""
        }
        return String(decoding: UnsafeBufferPointer<UInt8>(start: base, count: v.len), as: UTF8.self)
    }

    /// Copies the bytes of an owned AzString and frees it.
    static func takeString(_ s: AzString) -> String {
        var v: AzString = s
        let r: String = string(&v)
        AzString_delete(&v)
        return r
    }

    /// A RefAny for application data: a `RefAny` is shared (refcount + 1),
    /// any other object is kept alive by a handle box.
    static func refany(_ x: AnyObject) -> AzRefAny {
        if let r = x as? RefAny {
            return AzRefAny_clone(r._address)
        }
        return _Handles.refany(object: x, closure: nil)
    }
}

/// Native <-> C conversions for the Option / Vec / Result shapes (extended
/// next to each type).
internal enum _Conv {}

/// One non-capturing C entry point per callback typedef (extended below).
internal enum _Trampolines {}

"##;
