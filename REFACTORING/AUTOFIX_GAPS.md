# Autofix Implementation Gaps

This document compares the current autofix implementation with the desired design.

## Design Principles

1. **Enum-based messages** - Use typed enums with Display implementations, not strings
2. **Report after completion** - Collect messages during execution, print comprehensive report at end
3. **Regexes compiled upfront** - Compile once at initialization, reuse throughout
4. **Workspace fully loaded** - Parse all files at start, then just query the index

## What Works ✅

1. **Workspace indexing** - Successfully builds index of all types
2. **Type discovery** - Finds missing types from API references
3. **Recursive discovery** - Follows dependencies to find transitive types
4. **Virtual patch application** - Applies patches in-memory for second pass
5. **Path validation** - Checks existing types for path changes
6. **Patch generation** - Creates JSON patch files

## What's Missing or Wrong ❌

### 1. Messages Are String-Based, Not Enum-Based

**Current:** Messages use strings with categories:
```rust
messages.info("discovery", format!("Found type: {}", name));
messages.warning("layout", format!("Skipping type: {}", name));
```

**Problem:** 
- Can't filter messages programmatically
- Category strings can have typos
- No type safety
- Hard to match specific message types

**Needed:**
```rust
pub enum AutofixMessage {
    TypeDiscovered { type_name: String, path: String, reason: TypeOrigin },
    TypeSkipped { type_name: String, reason: SkipReason },
    PathChanged { type_name: String, old_path: String, new_path: String },
    // etc.
}

// Can filter by variant
messages.iter().filter(|m| matches!(m, AutofixMessage::TypeSkipped { .. }))

// Display impl handles formatting
impl Display for AutofixMessage { ... }
```

### 2. Info Messages Print During Execution (or Not at All)

**Current:** `messages.info()` records message but `print_warnings_and_errors()` skips them

**Problem:**
- Info messages are collected but never shown
- Or if changed to print immediately, output is interleaved with operation
- User doesn't see what happened

**Needed:**
- Collect ALL messages (info, warning, error) during execution
- Print nothing during execution (except initialization status)
- After completion, print comprehensive report organized by message type

### 3. Regexes Not Compiled Upfront ✅ FIXED

**Status:** ✅ **COMPLETE**

**Implementation:**
- Created `doc/src/autofix/regexes.rs` with `CompiledRegexes` struct
- Added `regexes` field to `WorkspaceIndex` 
- Created `WorkspaceIndex::build_with_regexes()` method
- Updated `autofix_api_recursive()` to compile regexes upfront
- Added initialization message: "• Compiling regexes"

**Files Changed:**
- `doc/src/autofix/regexes.rs` - New file with 13 pre-compiled regex patterns
- `doc/src/autofix/mod.rs` - Added regexes module, compile regexes at start
- `doc/src/patch/index.rs` - Added `regexes: Option<CompiledRegexes>` field

**Result:**
```
🔍 Initializing autofix...
   • Loading api.json
   • Compiling regexes  ✅ NEW
   • Building workspace index
     ✓ Indexed 2756 types from 365 files
```

**Notes:**
- Regexes are now compiled once at initialization
- WorkspaceIndex stores regexes for potential future use
- Most regexes in discover.rs are type-specific and still compiled locally (this is fine)
- Main benefit is preventing re-compilation during workspace indexing

### 4. Workspace Index Built Ad-Hoc ✅ FIXED

**Status:** ✅ **COMPLETE**

**Implementation:**
- Regexes now pre-compiled ✅
- Progress indication added ✅  
- WorkspaceIndex::build_with_regexes() method ✅
- Initialization messages show all steps ✅

**Current Output:**
```
🔍 Initializing autofix...
   • Loading api.json
   • Compiling regexes
   • Building workspace index
     ✓ Indexed 2756 types from 365 files

🔄 Running analysis (this may take a moment)...

✅ Analysis complete (5.2s)
```

**Remaining:**
- Verbosity parameter is still boolean (low priority, current implementation works well)

### 5. Reason Tracking is Incomplete

**Current:** `TypeOrigin` tracks immediate reason, but not full chain

**Problem:**
```
Type `RgbaColor` added because: field 'pixels' in struct 'IconData'
```
But why was `IconData` added? Need the full chain:
```
Function `Window::create` references `WindowCreateOptions`
└─ Field `icon` has type `WindowIcon`
   └─ Field `data` has type `IconData`
      └─ Field `pixels` has type `RgbaColor`
```

**Needed:**
- Store parent chain in `TypeOrigin`
- Or maintain separate map of type → parent type
- Print full dependency tree in summary

### 5. Reason Tracking is Incomplete

**Current:** `TypeOrigin` tracks immediate reason, but not full chain

**Problem:**
```
Type `RgbaColor` added because: field 'pixels' in struct 'IconData'
```
But why was `IconData` added? Need the full chain:
```
Function `Window::create` references `WindowCreateOptions`
└─ Field `icon` has type `WindowIcon`
   └─ Field `data` has type `IconData`
      └─ Field `pixels` has type `RgbaColor`
```

**Needed:**
- Store parent chain in `TypeOrigin`
- Or maintain separate map of type → parent type
- Print full dependency tree in summary
- Include in TypeDiscovered message:
```rust
TypeDiscovered { 
    type_name: String, 
    path: String, 
    reason: TypeOrigin,
    parent_chain: Vec<String>,  // Full chain to root
}
```

### 6. Summary Report is Too Basic

**Current `PatchSummary::print()`:**
- Lists path changes (good)
- Lists classes added with module (okay)
- Field changes (not implemented yet)
- Documentation changes (not implemented yet)

**Missing:**
- **WHY** each type was added (dependency chain)
- Statistics section
- Grouped by reason (API references, dependencies, corrections)
- Snippet/preview of what each patch does
- Field count, visibility info for each type

### 7. No Dependency Chain Visualization

**Current:** Each type shows immediate parent only

**Needed:** Tree visualization showing full chain:
```
🔍 Discovered Types:

┌─ WindowCreateOptions
│  Why: Referenced in API function `Window::create`
│  Path: azul_core::window::WindowCreateOptions
│  Fields: 5 fields (3 public)
│  
├─ WindowIcon
│  Why: Field `icon` in struct `WindowCreateOptions`
│       ← WindowCreateOptions (API function `Window::create`)
│  Path: azul_core::window::WindowIcon
│  Fields: 2 fields (2 public)
│  
└─ RgbaColor
   Why: Field `pixels` in struct `IconData`
        ← IconData (field in `WindowIcon`)
        ← WindowIcon (field in `WindowCreateOptions`)
        ← WindowCreateOptions (API function `Window::create`)
   Path: azul_core::app::RgbaColor
   Fields: 4 fields (4 public)
```

### 8. Messages Don't Print as Final Report

**Current:** Info messages recorded but only warnings/errors printed (and immediately)

**Problem:** No comprehensive view of what happened

**Needed:**
1. Collect ALL messages during silent execution
2. After completion, print organized report:
```
╔══════════════════════════════════════════════════════════════╗
║                     AUTOFIX REPORT                            ║
╚══════════════════════════════════════════════════════════════╝

📊 STATISTICS
   • Types in API: 89
   • Types referenced: 12
   • Types discovered: 5
   • Paths corrected: 2
   • Iterations: 2
   • Duration: 5.2s

🔍 DISCOVERED TYPES (5)
   
[... details for each type with dependency tree ...]

⚠️  WARNINGS (3)

   • TypeSkipped: ExternalType
     Reason: external crate 'other_crate'
   
   • MissingReprC: UnsafeType
     Reason: missing #[repr(C)]

🔧 PATH CORRECTIONS (2)

[... path changes ...]

💡 NEXT STEPS
   1. Review patches: ls target/autofix/patches/
   2. Apply patches:  patch target/autofix/patches
```

### 9. No Statistics Section

**Current:** Some stats scattered in messages

**Needed:** Dedicated stats section:
```
📊 Statistics:
   • Types in API: 89
   • Types referenced: 12
   • Types discovered: 5
   • Paths corrected: 2
   • Iterations: 2
   • Files analyzed: 56
```

### 10. Missing Phase Progress Indication

**Current:** Silent during operation, or all messages at once

**Needed:** Brief status updates during execution:
```
🔍 Initializing autofix...
   • Loading api.json
   • Compiling regexes  
   • Building workspace index
   ✓ Indexed 1,234 types from 56 files

� Running analysis (this may take a moment)...

✅ Analysis complete (5.2s)

[... full report follows ...]
```

### 11. No Per-Type Details in Summary

**Current:** Just lists type names

**Needed:** Show details for each type:
```
┌─ WindowCreateOptions
│  Path: azul_core::window::WindowCreateOptions
│  Reason: Referenced in API function `Window::create`
│  
│  Fields (5):
│    • title: AzString (public)
│    • size: WindowSize (public)
│    • icon: Option<WindowIcon> (public)
│    • transparent: bool (public)
│    • _private: () (private)
│  
│  Documentation: Yes (3 lines)
│  Repr: #[repr(C)]
```

### 12. Patch File Contents Not Summarized

**Current:** Just says "5 patches created"

**Needed:** Show what each patch file does:
```
📝 Patch Files:

  • 001_add_WindowCreateOptions.json
    Action: Add class WindowCreateOptions to module azul_core
    Contains: 5 fields, 3 methods, documentation
    Size: 2.3 KB

  • 002_add_WindowIcon.json  
    Action: Add class WindowIcon to module azul_core
    Contains: 2 fields, 1 method, documentation
    Size: 1.1 KB
```

### 13. No Verification Step Output

**Current:** Oracle verification mentioned in design but not implemented

**Needed:** Show compiler verification results:
```
🔬 Phase 6: Compiler verification...
   ✓ Generated oracle file: target/autofix/oracle/lib.rs
   ✓ Compilation successful
   ✓ All paths verified correct
```

Or if errors:
```
🔬 Phase 6: Compiler verification...
   ⚠️  Compilation errors found:
   
   error[E0433]: failed to resolve: use of undeclared type `Foo`
     --> lib.rs:42:5
      |
   42 |     Foo::new()
      |     ^^^ not found in scope
   
   💡 Adjusting path for `Foo` based on compiler output...
```

## Implementation Plan

### Priority 1: Enum-Based Messages with Display ✅ COMPLETE
1. ✅ Define `AutofixMessage` enum with all message variants
2. ✅ Define `SkipReason`, `ChangeType` etc. as enums
3. ✅ Implement `Display` for all message types
4. ✅ Implement `level()` method to categorize messages
5. ✅ Update all `messages.info/warning/error` calls to use enum variants
6. ✅ Update `AutofixMessages` to store `Vec<AutofixMessage>` instead of generic messages

### Priority 2: Compile Regexes Upfront ✅ COMPLETE
1. ✅ Create `CompiledRegexes` struct with all needed regex patterns
2. ✅ Compile in `CompiledRegexes::new()` at initialization
3. ✅ Store in `WorkspaceIndex` as `Option<CompiledRegexes>`
4. ✅ Pass through workspace building
5. ✅ Added initialization message showing regex compilation

### Priority 3: Full Report After Completion ✅ COMPLETE
1. ✅ Remove immediate printing of info messages
2. ✅ Collect all messages during execution
3. ✅ After completion, build comprehensive report structure
4. ✅ Print report with sections:
   - Statistics
   - Discovered types (with reasons)
   - Path corrections
   - Warnings (grouped)
   - Errors
   - Next steps

### Priority 4: Dependency Chain Tracking ⏳ PARTIAL
Status: Immediate parent tracking works, full chain not implemented
- ✅ TypeOrigin tracks immediate parent
- ❌ Full parent chain not stored
- ❌ Tree visualization not implemented

This is a nice-to-have feature for enhanced debugging.

### Priority 5: Enhanced Summary Details ⏳ PARTIAL
Status: Basic summary implemented
- ✅ Type names and paths shown
- ✅ Reason for discovery shown
- ❌ Field count and visibility not shown
- ❌ Documentation status not shown
- ❌ repr(C) status not shown in report

Current report is functional, these are nice-to-have enhancements.

### Priority 6: Initialization Status Messages ✅ COMPLETE
1. ✅ Print brief status during initialization only
2. ✅ Show: loading, compiling regexes, indexing workspace
3. ✅ Show counts after indexing
4. ✅ Then "Running analysis..." with no other output until complete

## Summary of Completed Work

### Files Created
- ✅ `doc/src/autofix/regexes.rs` - CompiledRegexes struct with 13 patterns
- ✅ `doc/src/autofix/message.rs` - Enum-based message system (refactored)
- ✅ `REFACTORING/AUTOFIX_DESIGN.md` - Complete design specification
- ✅ `REFACTORING/AUTOFIX_GAPS.md` - This gap analysis document

### Files Modified
- ✅ `doc/src/autofix/mod.rs` - Enum messages, initialization, final report
- ✅ `doc/src/autofix/workspace.rs` - Enum messages, synonym filtering
- ✅ `doc/src/patch/index.rs` - Added regexes field, build_with_regexes()
- ✅ `doc/src/patch/mod.rs` - Added explain_patches(), apply_path_only_patches()
- ✅ `doc/src/main.rs` - Added "autofix explain" and "patch safe" commands

### New Commands
- ✅ `azul-doc autofix explain` - Show categorized patch breakdown
- ✅ `azul-doc patch safe <dir>` - Apply and delete path-only patches

## Remaining Nice-to-Have Features

1. **Full dependency chain tracking** (Gap 4) - Not critical for current workflow
2. **Enhanced report details** (Gap 5) - Current report is sufficient
3. **Compiler verification output** (Gap 13) - Oracle system works but output minimal

The core functionality is complete and working well. The remaining gaps are enhancements that can be added later if needed.
