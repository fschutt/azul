# C++ Header V2 Migration Summary

## Current Status

The C header (`azul.h`) now compiles successfully with the V2 codegen system.
The C++ header (`azul.hpp`) has several issues that need to be addressed.

## C++ Header Problems

### 1. Reserved Keyword Conflicts
```cpp
extern DLLIMPORT void AzDom_addClass(AzDom* dom, AzString class);
//                                                        ^^^^^ 'class' is a C++ keyword!
```
**Solution:** Parameter names that are C++ keywords need to be renamed (e.g., `class` → `class_` or `className`).

### 2. Missing Default Constructors
```cpp
WasmWindowOptions() : inner(AzWasmWindowOptions_default()) {}
//                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ undeclared
```
The C++ wrapper tries to call `Az<Type>_default()` functions that don't exist.
**Solution:** Only generate default constructors if the type has `Default` derive in api.json.

### 3. Version Compatibility (C++03 to C++23)

The old C++ generator in `cpp_api.rs` supports multiple C++ versions with different features:

| Feature | C++03 | C++11 | C++14 | C++17 | C++20 | C++23 |
|---------|-------|-------|-------|-------|-------|-------|
| `nullptr` | ❌ (use NULL) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `constexpr` | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `noexcept` | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Move semantics | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `= default` | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `= delete` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ (explicit copy) |
| `[[nodiscard]]` | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| `std::optional` | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| Concepts | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| `std::expected` | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

### 4. Current V2 C++ Generator Limitations

The current `lang_cpp.rs` just includes `azul.h` and adds thin C++ wrappers. It doesn't:

- Handle C++ keyword conflicts in parameter names
- Check for `Default` trait before generating default constructors
- Support different C++ versions
- Generate proper move semantics / RAII

## Migration Plan for V2 C++ Generator

### Phase 1: Fix Immediate Issues

1. Add C++ keyword escaping for parameter names (`class` → `class_`)
2. Only generate default constructors if type has `Default` trait
3. Fix missing function declarations

### Phase 2: Add Version Support

1. Add `CppVersion` enum to `CodegenConfig`:
   ```rust
   enum CppVersion { Cpp03, Cpp11, Cpp14, Cpp17, Cpp20, Cpp23 }
   ```

2. Add version-conditional code generation:
   ```rust
   fn maybe_noexcept(&self) -> &str {
       if self.version >= CppVersion::Cpp11 { " noexcept" } else { "" }
   }
   ```

### Phase 3: Proper C++ Wrappers
1. **RAII wrappers** with destructors that call `Az<Type>_delete()`
2. **Move constructors** (C++11+) that transfer ownership
3. **Copy constructors** that call `Az<Type>_deepCopy()`
4. **Optional wrappers** using `std::optional<T>` (C++17+) or custom Optional<T> (older)
5. **Result wrappers** using `std::expected<T,E>` (C++23) or custom Result<T,E> (older)

### Phase 4: Advanced Features
1. **String conversions** (implicit `std::string` ↔ `AzString`)
2. **Iterator support** for Vec types
3. **Smart pointer support** (`std::unique_ptr`, `std::shared_ptr`)
4. **Exception-safe API** (optional, convert Result → exceptions)

## Files to Modify

- `doc/src/codegen/v2/lang_cpp.rs` - Main C++ generator (needs major rewrite)
- `doc/src/codegen/v2/ir.rs` - Add C++ specific info to IR if needed
- `doc/src/codegen/v2/mod.rs` - Add CppVersion to config

## Reference: Old C++ Generator

The old C++ generator in `doc/src/codegen/cpp_api.rs` has working implementations for:
- Version-specific code generation
- RAII wrapper generation
- Move/copy constructor generation
- String conversion helpers
- Iterator helpers for Vec types

Key functions to port:
- `generate_cpp_wrapper_class()` - Generates wrapper class with RAII
- `generate_constructors()` - Version-aware constructor generation
- `generate_destructor()` - Calls Az<Type>_delete
- `generate_copy_move()` - Copy/move semantics

## Testing Strategy

1. Test C++ header syntax with each version:
   ```bash
   clang++ -std=c++03 -fsyntax-only azul.hpp
   clang++ -std=c++11 -fsyntax-only azul.hpp
   clang++ -std=c++14 -fsyntax-only azul.hpp
   clang++ -std=c++17 -fsyntax-only azul.hpp
   clang++ -std=c++20 -fsyntax-only azul.hpp
   clang++ -std=c++23 -fsyntax-only azul.hpp
   ```

2. Test compilation of example programs
3. Test runtime behavior (linking with libazul)
