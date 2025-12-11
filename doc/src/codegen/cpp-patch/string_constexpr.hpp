// =============================================================================
// Constexpr String Concatenation Patch for C++17/20/23
// =============================================================================
//
// This header provides compile-time string concatenation for azul's String type.
// It allows users to compose CSS styles at compile time without runtime overhead.
//
// USAGE:
//   // C++17: Use concat() function
//   constexpr auto STYLE = concat("width:100px; ", "height:50px;");
//
//   // C++23: Use String with operator+ and _s literal
//   constexpr String STYLE = "width:100px; "_s + "height:50px;"_s;
//
// =============================================================================

#pragma once

#include <cstddef>
#include <array>
#include <algorithm>

#if __cplusplus >= 201703L  // C++17 or later

namespace azul {

// =============================================================================
// C++17: FixedString for compile-time string operations
// =============================================================================

/// A fixed-size string that can be used in constexpr contexts
template<size_t N>
struct FixedString {
    std::array<char, N> data{};
    
    /// Construct from string literal
    constexpr FixedString(const char (&str)[N]) {
        for (size_t i = 0; i < N; ++i) {
            data[i] = str[i];
        }
    }
    
    /// Concatenate two FixedStrings
    template<size_t M>
    constexpr auto operator+(const FixedString<M>& other) const {
        FixedString<N + M - 1> result;
        // Copy this string (without null terminator)
        for (size_t i = 0; i < N - 1; ++i) {
            result.data[i] = data[i];
        }
        // Copy other string (with null terminator)
        for (size_t i = 0; i < M; ++i) {
            result.data[N - 1 + i] = other.data[i];
        }
        return result;
    }
    
    /// Get C-string pointer
    constexpr const char* c_str() const { return data.data(); }
    
    /// Implicit conversion to const char*
    constexpr operator const char*() const { return data.data(); }
    
    /// Get length (excluding null terminator)
    constexpr size_t size() const { return N - 1; }
    
private:
    // Private default constructor for operator+
    constexpr FixedString() = default;
    template<size_t> friend struct FixedString;
};

/// Deduction guide for FixedString
template<size_t N>
FixedString(const char (&)[N]) -> FixedString<N>;

// =============================================================================
// C++17: concat() function templates
// =============================================================================

/// Concatenate two string literals at compile time
template<size_t N1, size_t N2>
constexpr auto concat(const char(&a)[N1], const char(&b)[N2]) {
    std::array<char, N1 + N2 - 1> result{};
    for (size_t i = 0; i < N1 - 1; ++i) result[i] = a[i];
    for (size_t i = 0; i < N2; ++i) result[N1 - 1 + i] = b[i];
    return result;
}

/// Concatenate three string literals at compile time
template<size_t N1, size_t N2, size_t N3>
constexpr auto concat(const char(&a)[N1], const char(&b)[N2], const char(&c)[N3]) {
    std::array<char, N1 + N2 + N3 - 2> result{};
    size_t pos = 0;
    for (size_t i = 0; i < N1 - 1; ++i) result[pos++] = a[i];
    for (size_t i = 0; i < N2 - 1; ++i) result[pos++] = b[i];
    for (size_t i = 0; i < N3; ++i) result[pos++] = c[i];
    return result;
}

/// Concatenate four string literals at compile time
template<size_t N1, size_t N2, size_t N3, size_t N4>
constexpr auto concat(const char(&a)[N1], const char(&b)[N2], 
                      const char(&c)[N3], const char(&d)[N4]) {
    std::array<char, N1 + N2 + N3 + N4 - 3> result{};
    size_t pos = 0;
    for (size_t i = 0; i < N1 - 1; ++i) result[pos++] = a[i];
    for (size_t i = 0; i < N2 - 1; ++i) result[pos++] = b[i];
    for (size_t i = 0; i < N3 - 1; ++i) result[pos++] = c[i];
    for (size_t i = 0; i < N4; ++i) result[pos++] = d[i];
    return result;
}

/// Concatenate five string literals at compile time
template<size_t N1, size_t N2, size_t N3, size_t N4, size_t N5>
constexpr auto concat(const char(&a)[N1], const char(&b)[N2], 
                      const char(&c)[N3], const char(&d)[N4],
                      const char(&e)[N5]) {
    std::array<char, N1 + N2 + N3 + N4 + N5 - 4> result{};
    size_t pos = 0;
    for (size_t i = 0; i < N1 - 1; ++i) result[pos++] = a[i];
    for (size_t i = 0; i < N2 - 1; ++i) result[pos++] = b[i];
    for (size_t i = 0; i < N3 - 1; ++i) result[pos++] = c[i];
    for (size_t i = 0; i < N4 - 1; ++i) result[pos++] = d[i];
    for (size_t i = 0; i < N5; ++i) result[pos++] = e[i];
    return result;
}

// =============================================================================
// Helper to get .data() from std::array results
// =============================================================================

/// Convert std::array result to usable string pointer
template<size_t N>
constexpr const char* str(const std::array<char, N>& arr) {
    return arr.data();
}

#if __cplusplus >= 202002L  // C++20 or later

// =============================================================================
// C++20: Enhanced FixedString with more features
// =============================================================================

/// String literal operator for FixedString (C++20)
template<FixedString Str>
constexpr auto operator""_fs() {
    return Str;
}

#endif // C++20

#if __cplusplus >= 202302L  // C++23 or later

// =============================================================================
// C++23: String class with constexpr support
// =============================================================================
//
// In C++23, the azul::String class itself supports constexpr operator+
// This is the cleanest API for users.

/// String class with constexpr concatenation support
/// NOTE: This is a simplified version - the real implementation
/// would wrap AzString from the C API
template<size_t MaxSize = 4096>
class ConstexprString {
    std::array<char, MaxSize> data_{};
    size_t len_ = 0;
    
public:
    constexpr ConstexprString() = default;
    
    /// Construct from string literal
    template<size_t N>
    constexpr ConstexprString(const char (&str)[N]) : len_(N - 1) {
        static_assert(N <= MaxSize, "String too long for ConstexprString");
        for (size_t i = 0; i < N; ++i) {
            data_[i] = str[i];
        }
    }
    
    /// Concatenate two strings
    template<size_t OtherMax>
    constexpr auto operator+(const ConstexprString<OtherMax>& other) const {
        ConstexprString<MaxSize + OtherMax> result;
        result.len_ = len_ + other.len_;
        
        for (size_t i = 0; i < len_; ++i) {
            result.data_[i] = data_[i];
        }
        for (size_t i = 0; i < other.len_; ++i) {
            result.data_[len_ + i] = other.data_[i];
        }
        result.data_[result.len_] = '\0';
        
        return result;
    }
    
    constexpr const char* c_str() const { return data_.data(); }
    constexpr operator const char*() const { return data_.data(); }
    constexpr size_t size() const { return len_; }
    
    template<size_t> friend class ConstexprString;
};

/// User-defined literal for azul::String
/// Usage: "hello"_s creates an azul::String
constexpr auto operator""_s(const char* str, size_t len) {
    // In the real implementation, this would return an azul::String
    // For now, we create a ConstexprString by copying
    ConstexprString<256> result;
    for (size_t i = 0; i < len && i < 255; ++i) {
        result.data_[i] = str[i];
    }
    result.len_ = len;
    return result;
}

// Alias for convenience
using String = ConstexprString<4096>;

#endif // C++23

} // namespace azul

#endif // C++17

// =============================================================================
// USAGE EXAMPLES:
// =============================================================================
//
// C++17:
//   #include <azul.hpp>
//   using namespace azul;
//   
//   constexpr auto SIZE   = "width:100px; height:50px; ";
//   constexpr auto BORDER = "border:1px solid black;";
//   constexpr auto STYLE  = concat(SIZE, BORDER);
//   
//   // Use in code:
//   Dom::div().with_inline_style(STYLE.data());
//
// C++23:
//   #include <azul.hpp>
//   using namespace azul;
//   
//   constexpr String SIZE   = "width:100px; height:50px; "_s;
//   constexpr String BORDER = "border:1px solid black;"_s;
//   constexpr String STYLE  = SIZE + BORDER;
//   
//   // Use in code:
//   Dom::div().with_inline_style(STYLE);
//
// =============================================================================
// INTEGRATION NOTES:
// =============================================================================
//
// To integrate with the azul C API:
//
// 1. The String class should wrap AzString internally
// 2. operator+ should call AzString_concat or similar
// 3. The _s literal should call AzString_fromConstStr
// 4. with_inline_style() should accept both const char* and String
//
// For C++17, the concat() results are std::array<char, N> which can be
// passed to with_inline_style() via .data()
//
// =============================================================================
