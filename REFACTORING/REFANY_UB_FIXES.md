# RefAny Undefined Behavior - Gefunden und Behoben

## Zusammenfassung

Miri hat **3 kritische Undefined Behavior (UB) Bugs** in der `RefAny`-Implementierung gefunden, die die SIGSEGV-Abstürze in den IFrame-Callback-Tests verursacht haben. Alle wurden erfolgreich behoben.

## Bug #1: Unaligned Memory References

### Problem
```rust
// In RefAny::new_c - FALSCH
let struct_as_bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
let layout = Layout::for_value(&*struct_as_bytes);  // Layout mit align = 1!
let heap_struct_as_bytes = unsafe { alloc::alloc::alloc(layout) };  // Nur 1-byte aligned!
```

`Layout::for_value(&[u8])` erstellt ein Layout mit Alignment 1, aber Typen wie `i32`, `f64` oder Structs benötigen oft 4- oder 8-byte Alignment. Beim späteren Downcast wurde eine unaligned reference erstellt → **Undefined Behavior**.

### Miri-Fehler
```
error: Undefined Behavior: constructing invalid value: 
encountered an unaligned reference (required 8 byte alignment but found 1)
--> core/src/refany.rs:343:27
```

### Lösung
```rust
// In RefAny::new - Alignment übergeben
let s = Self::new_c(
    (&value as *const T) as *const c_void,
    ::core::mem::size_of::<T>(),
    ::core::mem::align_of::<T>(),  // NEU: Alignment übergeben
    Self::get_type_id_static::<T>(),
    st,
    default_custom_destructor::<T>,
);

// In RefAny::new_c - Korrektes Layout erstellen
let layout = Layout::from_size_align(len, align)
    .expect("Failed to create layout");
let heap_struct_as_bytes = unsafe { alloc::alloc::alloc(layout) };
if heap_struct_as_bytes.is_null() {
    alloc::alloc::handle_alloc_error(layout);
}
```

## Bug #2: Falscher Count-Parameter in `ptr::copy_nonoverlapping`

### Problem
```rust
// In default_custom_destructor - FALSCH
ptr::copy_nonoverlapping(
    (ptr as *mut c_void) as *const U,
    stack_mem.as_mut_ptr(),
    mem::size_of::<U>(),  // FALSCH: Das ist die SIZE, nicht der COUNT!
);
```

`ptr::copy_nonoverlapping(src, dst, count)` erwartet die **Anzahl der Elemente**, nicht die Größe in Bytes! Bei einem Struct mit `size_of::<U>() = 32` würde es versuchen, 32 Structs (1024 bytes) zu kopieren.

### Miri-Fehler
```
error: Undefined Behavior: memory access failed: 
attempting to access 1024 bytes, but got alloc which is only 32 bytes
--> core/src/refany.rs:222:17
```

### Lösung
```rust
ptr::copy_nonoverlapping(
    ptr as *const U,
    stack_mem.as_mut_ptr(),
    1,  // Kopiere 1 Element vom Typ U
);
```

## Bug #3: Inkonsistente Destruktor-Signatur

### Problem
Die Destruktor-Signatur war inkonsistent:
```rust
// In RefCountInner
pub custom_destructor: extern "C" fn(*mut c_void),  // Pointer

// In new_c Parameter - INKONSISTENT
custom_destructor: extern "C" fn(&mut c_void),  // Referenz!

// In default_custom_destructor - INKONSISTENT
extern "C" fn default_custom_destructor<U: 'static>(ptr: &mut c_void) {
    ptr::copy_nonoverlapping(
        (ptr as *mut c_void) as *const U,  // Reinterpret-cast einer Referenz!
        ...
    );
}
```

Das Casten einer Referenz `&mut c_void` zu `*mut c_void` und dann zu `*const U` verletzt Stacked Borrows.

### Miri-Fehler
```
error: Undefined Behavior: attempting a read access using <tag> at alloc[0x1], 
but that tag does not exist in the borrow stack for this location
--> core/src/refany.rs:222:17
```

### Lösung
Überall konsistent `*mut c_void` verwenden:
```rust
// Überall
custom_destructor: extern "C" fn(*mut c_void)

// In default_custom_destructor
extern "C" fn default_custom_destructor<U: 'static>(ptr: *mut c_void) {
    unsafe {
        let mut stack_mem = mem::MaybeUninit::<U>::uninit();
        ptr::copy_nonoverlapping(
            ptr as *const U,  // Direkt vom pointer, nicht von einer Referenz
            stack_mem.as_mut_ptr(),
            1,
        );
        let stack_mem = stack_mem.assume_init();
        mem::drop(stack_mem);
    }
}
```

## Ergebnisse

### Vor den Fixes
- ❌ IFrame-Tests: **SIGSEGV** (Segmentation Fault)
- ❌ Miri-Tests: 0/9 Tests erfolgreich (UB-Fehler)

### Nach den Fixes
- ✅ **Alle 9 RefAny-Tests bestehen mit Miri**
- ✅ **Keine SIGSEGV mehr** in IFrame-Tests
- ⚠️  IFrame-Tests schlagen mit normalen Assertions fehl (Callback wird nicht aufgerufen)
  - Dies ist ein **separates Logik-Problem**, kein Memory-Safety-Problem

## Betroffene Dateien

1. **core/src/refany.rs**
   - `RefAny::new()`: Fügt `align_of::<T>()` Parameter hinzu
   - `RefAny::new_c()`: Fügt `align: usize` Parameter hinzu, verwendet `Layout::from_size_align`
   - `default_custom_destructor`: Ändert Signatur zu `*mut c_void`, korrigiert `copy_nonoverlapping` count
   - Entfernt unsicheres `transmute` in RefCountInner Initialisierung

2. **api.json**
   - Aktualisiert `RefAny.new_c` Binding um `align: usize` Parameter zu inkludieren

## Lektionen Gelernt

1. **Miri ist unverzichtbar** für Unsafe-Code - es fand alle 3 Bugs sofort
2. **Alignment ist kritisch** - niemals `Layout::for_value(&[u8])` für Typ-erased Allokationen verwenden
3. **Pointer-Arithmetik ist subtil** - `copy_nonoverlapping` nimmt Element-Count, nicht Byte-Count
4. **Konsistenz bei Signaturen** - Referenzen und Pointer haben unterschiedliche Semantik in Miri

## Nächste Schritte

Die Memory-Safety-Probleme sind behoben. Der verbleibende IFrame-Callback-Bug ist ein Logik-Problem:
- Callbacks werden erstellt aber nicht aufgerufen
- Test-Assertions schlagen fehl: `left: 0, right: 1` (callback_count ist 0)
- Debuggen mit Print-Statements oder Step-Debugger erforderlich

## Test-Status

```
✅ core/src/refany.rs - Alle 9 Tests bestehen mit Miri
✅ layout/src/solver3/tests.rs - 75/77 Tests bestehen
⚠️  layout/src/solver3/tests.rs - 2 IFrame-Tests schlagen fehl (kein SIGSEGV)
```
