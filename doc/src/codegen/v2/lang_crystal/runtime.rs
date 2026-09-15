//! The hand-written part of the idiomatic Crystal layer.
//!
//! Everything here is independent of api.json: the ownership base class every
//! wrapper inherits, the handle table that keeps Crystal objects and closures
//! alive while libazul holds a `RefAny` naming them, and the `String`
//! conversions. The generated classes (`wrappers.rs`) build on these names.
//!
//! Stdlib names are always written `::String`, `::Pointer`, ... because the
//! generated `Azul::String` (and `Azul::Thread`, ...) would otherwise shadow
//! them inside `module Azul`.

pub const RUNTIME: &str = r##"# ----------------------------------------------------------------------------
# Runtime: ownership base class, handle table, conversions.
# ----------------------------------------------------------------------------

module Azul
  # Raised when an azul object is used after it was moved into a call.
  #
  # Passing a non-Copy azul object BY VALUE (e.g. `body.with_child(label)`)
  # hands it to libazul, the same as a Rust move. Using `label` afterwards
  # raises this. Call `#clone` first to keep a copy.
  class MovedError < ::Exception
  end

  # Raised by a method whose Rust return type is a `Result` that came back
  # `Err`; `#error` is the converted error value.
  class ResultError(E) < ::Exception
    getter error : E

    def initialize(@error : E)
      super(@error.to_s)
    end
  end

  # Base of every generated wrapper class.
  #
  # A wrapper either OWNS its value (`@__root` is nil; the value lives in a
  # GC-allocated cell and the finalizer calls the type's `_delete`) or is a
  # VIEW into a field of another wrapper (`@__root` keeps that owner alive,
  # writes go straight into the owner's memory, the finalizer does nothing).
  abstract class Wrapper
    # :nodoc:
    @__root : Wrapper? = nil
    # :nodoc:
    @__moved = false

    # There is no public zero-argument constructor: every wrapper is built
    # by a generated factory that initializes its storage.
    private def initialize(__unused : ::NoReturn)
    end

    # :nodoc:
    def __root : Wrapper?
      @__root
    end

    # :nodoc:
    def __moved? : ::Bool
      if r = @__root
        r.__moved?
      else
        @__moved
      end
    end

    # :nodoc:
    def __mark_moved : ::Nil
      @__moved = true
    end

    # :nodoc:
    def __set_root(root : Wrapper) : ::Nil
      @__root = root.__root || root
    end

    # :nodoc:
    def __check_alive : ::Nil
      if __moved?
        raise MovedError.new("#{self.class} was moved into an earlier call and can no longer be used (call #clone before passing it on to keep a copy)")
      end
    end
  end

  # Storage shared by every wrapper of the lib type `L`.
  abstract class Storage(L) < Wrapper
    # :nodoc:
    @__ptr : ::Pointer(L) = ::Pointer(L).null

    # :nodoc:
    def __init(ptr : ::Pointer(L)) : ::Nil
      @__ptr = ptr
    end

    # The address of the underlying lib value, for calls into `LibAzul`.
    def to_unsafe : ::Pointer(L)
      __check_alive
      @__ptr
    end

    # :nodoc:
    def __value : L
      __check_alive
      @__ptr.value
    end

    # :nodoc:
    def __replace(raw : L) : ::Nil
      @__ptr.value = raw
    end
  end

  # Keeps Crystal objects and closures alive while libazul holds a RefAny
  # that names them. The RefAny's payload is the table key; its destructor
  # (run by libazul when the last clone drops) removes the entry.
  module Handles
    # :nodoc:
    abstract class Held
    end

    # :nodoc:
    class HeldValue(S) < Held
      def initialize(@object : S)
      end

      def get(type : T.class) : T? forall T
        o = @object
        o.is_a?(T) ? o : nil
      end
    end

    # RTTI id of every RefAny this binding creates ("CRYSTAL1").
    TYPE_ID = 0x4352_5953_5441_4C31_u64
    @@table = {} of ::UInt64 => {Held?, Held?}
    @@next = 1_u64

    # :nodoc:
    DESTRUCTOR = ->(ptr : ::Pointer(::Void)) {
      Azul::Handles.release(ptr.as(::Pointer(::UInt64)).value)
    }

    # :nodoc:
    def self.release(id : ::UInt64) : ::Nil
      @@table.delete(id)
    end

    # Number of live handles (objects + closures libazul still references).
    def self.size : ::Int32
      @@table.size
    end

    # :nodoc:
    def self.hold(x : S) : Held forall S
      HeldValue(S).new(x)
    end

    # :nodoc:
    # A new RefAny (refcount 1) naming `object` and/or `closure`.
    def self.refany(object : Held?, closure : Held?) : LibAzul::AzRefAny
      id = @@next
      @@next += 1
      @@table[id] = {object, closure}
      payload = id
      ptr = LibAzul::AzGlVoidPtrConst.new
      ptr.ptr = pointerof(payload).as(::Pointer(::Void))
      ptr.run_destructor = false
      name = Native.az_string("Crystal")
      LibAzul.azRefAny_newC(ptr, ::LibC::SizeT.new(8), ::LibC::SizeT.new(8), TYPE_ID, name, DESTRUCTOR, ::LibC::SizeT.new(0), ::LibC::SizeT.new(0))
    end

    # :nodoc:
    def self.entry(refany : ::Pointer(LibAzul::AzRefAny)) : {Held?, Held?}?
      return nil unless LibAzul.azRefAny_isType(refany, TYPE_ID)
      data = LibAzul.azRefAny_getDataPtr(refany)
      return nil if data.null?
      @@table[data.as(::Pointer(::UInt64)).value]?
    end

    # :nodoc:
    # The Crystal object a callback's data RefAny names, as `T`.
    def self.object(refany : ::Pointer(LibAzul::AzRefAny), type : T.class) : T forall T
      if e = entry(refany)
        if held = e[0]
          if o = held.get(T)
            return o
          end
        end
      end
      {% if T <= Azul::RefAny %}
        __raw = LibAzul.azRefAny_clone(refany)
        Azul::RefAny.__own(__raw)
      {% else %}
        raise ::TypeCastError.new("callback data is not a #{T} (pass the object itself as the data argument)")
      {% end %}
    end

    # :nodoc:
    def self.closure(refany : ::Pointer(LibAzul::AzRefAny), type : T.class) : T? forall T
      if e = entry(refany)
        if held = e[1]
          return held.get(T)
        end
      end
      nil
    end
  end

  module Native
    # Copies a Crystal String into a new AzString (the caller owns it).
    def self.az_string(s : ::String) : LibAzul::AzString
      LibAzul.azString_fromUtf8(s.to_unsafe, ::LibC::SizeT.new(s.bytesize))
    end

    # Copies the bytes of a borrowed AzString.
    def self.string(s : ::Pointer(LibAzul::AzString)) : ::String
      v = s.value
      ::String.new(v.vec.ptr, v.vec.len)
    end

    # Copies the bytes of an owned AzString and frees it.
    def self.take_string(s : LibAzul::AzString) : ::String
      v = s
      r = ::String.new(v.vec.ptr, v.vec.len)
      LibAzul.azString_delete(pointerof(v))
      r
    end

    # A RefAny for application data: an `Azul::RefAny` is shared (refcount
    # +1), any other object is kept alive by the handle table.
    def self.refany(x : Azul::RefAny) : LibAzul::AzRefAny
      LibAzul.azRefAny_clone(x.to_unsafe)
    end

    # :ditto:
    def self.refany(x : ::Reference) : LibAzul::AzRefAny
      Handles.refany(Handles.hold(x), nil)
    end

    # Runs a callback body. A Crystal exception must never unwind into
    # libazul's frames, so an escaping one is reported and ends the process.
    def self.guard(kind : ::String, &) : ::Nil
      yield
    rescue ex
      ::STDERR.puts "azul: unhandled exception in a #{kind}:"
      ex.inspect_with_backtrace(::STDERR)
      ::STDERR.flush
      ::exit(1)
    end
  end
end

"##;
