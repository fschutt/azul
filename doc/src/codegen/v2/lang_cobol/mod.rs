//! COBOL (GnuCOBOL) binding generator.
//!
//! Generates a single `azul.cpy` copybook that COBOL programs include via
//! `COPY "azul.cpy".` from inside their `WORKING-STORAGE SECTION`. The
//! copybook contains:
//!
//! 1. A header banner pinning the GnuCOBOL dialect (>= 3.0) and recording
//!    that the example must be compiled with `cobc -x` (executable, not a
//!    callable subprogram).
//! 2. A block of level-78 constants for every unit-only enum variant
//!    (`78 AZ-BUTTON-TYPE-PRIMARY VALUE 0.`).
//! 3. A block of level-78 constants holding the canonical, case-sensitive
//!    C symbol name for every C-ABI function (`78 FN-AZ-APP-CREATE
//!    VALUE "AzApp_create".`). The user calls `CALL FN-AZ-APP-CREATE
//!    USING ... RETURNING ...` rather than typing the literal C symbol —
//!    this preserves the case the C linker expects without forcing the
//!    COBOL author to remember it.
//! 4. A block of level-01 typedef records for every plain struct and
//!    every tagged union. Tagged unions use `REDEFINES` to overlay each
//!    variant payload on the same memory after the discriminant tag, so
//!    the COBOL record matches the Rust `#[repr(C)]` enum layout.
//! 5. A block of level-01 typedef records for callback function-pointer
//!    typedefs, declared as `USAGE PROGRAM-POINTER`.
//!
//! No procedure-division code is emitted: COBOL has no native concept of
//! a class with a destructor, and OO-COBOL is too rare to target. Users
//! manage native lifetimes manually by calling the matching
//! `FN-AZ-FOO-DELETE` symbol from their own `PROCEDURE DIVISION`. A
//! suggested pattern is documented in the trailing comment block emitted
//! by [`wrappers::generate_wrapper_docs`].
//!
//! # COBOL identifier mangling
//!
//! COBOL is case-insensitive but uppercase-with-hyphens is canonical, and
//! the ANSI 85 / GnuCOBOL 3 default identifier limit is 30 / 31 chars
//! respectively. We therefore:
//!
//! - Convert `Az_App_create` -> `AZ-APP-CREATE` ([`to_cobol_case`]).
//! - Truncate names longer than 30 chars to 30, with collisions resolved
//!   by appending a 4-char hex suffix derived from a stable hash
//!   ([`mangle_identifier`]). The original C symbol is still preserved
//!   verbatim inside the level-78 string literal so the linker can find
//!   it; only the COBOL-side identifier is shortened.
//!
//! # Wiring
//!
//! Like the other Wave-2 generators (Pascal/Ada/FreeBASIC) this module
//! is NOT wired from `v2/mod.rs`. The orchestrator will add a
//! `pub mod lang_cobol;` declaration plus a top-level
//! `generate_cobol(api_data) -> Result<String>` helper that mirrors
//! `generate_python` / `generate_pascal`. See
//! `scripts/api-json-additions/cobol.json` for the api.json patches the
//! orchestrator will merge.

use anyhow::Result;

use super::config::CodegenConfig;
use super::generator::CodeBuilder;
use super::ir::CodegenIR;

pub mod functions;
pub mod managed;
pub mod types;
pub mod wrappers;

/// Library name. Matches the prebuilt artifact's name without extension;
/// `cobc -lazul` resolves it to `azul.dll` / `libazul.so` /
/// `libazul.dylib` per platform.
pub const LIB_NAME: &str = "azul";

/// Maximum identifier length tolerated for the COBOL side. ANSI 85 caps
/// user-defined names at 30 characters; GnuCOBOL extends this to 31 by
/// default but we stay at 30 for portability with legacy tooling.
pub const MAX_ID_LEN: usize = 30;

/// Public entry point. Produces the full `azul.cpy` copybook source.
///
/// The caller writes the result to disk (e.g.
/// `target/codegen/v2/azul.cpy`). COBOL programs include it via
/// `COPY "azul.cpy"` from inside `WORKING-STORAGE SECTION`. There is no
/// separate `.cob` file emitted — the host program is expected to provide
/// its own `IDENTIFICATION DIVISION` and `PROCEDURE DIVISION`.
pub fn generate(ir: &CodegenIR, config: &CodegenConfig) -> Result<String> {
    let mut builder = CodeBuilder::new("    ");

    emit_header(&mut builder);

    // 1. Enum constants (level-78).
    // Callback typedefs FIRST — they're referenced by struct fields and
    // GnuCOBOL needs the TYPEDEF visible before any `USAGE TYAZ-FOO`
    // mention. Without this ordering, field declarations like
    // `05 CB USAGE TYAZ-GET-SYSTEM-TIME-CALL-XXXX` fail with
    // "unknown USAGE".
    types::generate_callback_typedefs(&mut builder, ir, config)?;
    types::generate_enum_constants(&mut builder, ir, config)?;

    // 2. Struct + tagged-union typedefs (level-01).
    types::generate_records(&mut builder, ir, config)?;

    // (Callback typedefs are emitted above so structs referencing them
    // by USAGE see the TYPEDEF first.)

    // 4. Function-name constants (level-78 STRING).
    functions::generate_function_constants(&mut builder, ir, config)?;

    // Managed-FFI host-invoker level-78 aliases (releaser, refany_new/get,
    // and per-kind setter / createFromHostHandle for every host-invoker
    // wrapper kind in HOST_INVOKER_KINDS).
    managed::emit_managed_aliases(&mut builder, ir);

    // 5. Wrapper-pattern documentation (no executable COBOL emitted).
    wrappers::generate_wrapper_docs(&mut builder, ir, config)?;

    Ok(builder.finish())
}

fn emit_header(builder: &mut CodeBuilder) {
    // GnuCOBOL preprocessor directive: parse the rest of this copybook
    // in free format. Lifts the 72-column line limit (which 17k+ of
    // our FN-* level-78 constants exceeded) and lets us use `*>` for
    // inline comments without a column-7 indicator area.
    builder.line(">>SOURCE FORMAT IS FREE");
    builder.blank();
    builder.line("*> ============================================================");
    builder.line("*> AZUL-GUI COBOL BINDINGS - GnuCOBOL >= 3.0");
    builder.line("*> Auto-generated by azul-doc codegen v2 (lang_cobol).");
    builder.line("*> DO NOT EDIT MANUALLY.");
    builder.line("*>");
    builder.line("*> Usage:");
    builder.line("*>   IDENTIFICATION DIVISION.");
    builder.line("*>   PROGRAM-ID. MY-AZUL-PROGRAM.");
    builder.line("*>   DATA DIVISION.");
    builder.line("*>   WORKING-STORAGE SECTION.");
    builder.line("*>   COPY \"azul.cpy\".");
    builder.line("*>");
    builder.line("*> Build (executable program):");
    builder.line("*>   cobc -x -free hello-world.cob -L. -lazul -o hello-world");
    builder.line("*> ============================================================");
    builder.blank();
}

// ============================================================================
// Identifier mangling (shared across types/functions/wrappers)
// ============================================================================

/// Convert a Rust/C identifier to canonical COBOL form: uppercase,
/// hyphens between camel-case word boundaries, hyphens replacing
/// underscores. `Az_App_create` -> `AZ-APP-CREATE`,
/// `MyDataModel` -> `MY-DATA-MODEL`.
pub fn to_cobol_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == '_' {
            // Underscore -> hyphen, but collapse runs.
            if !out.ends_with('-') {
                out.push('-');
            }
        } else if c.is_uppercase() && i > 0 {
            let prev = chars[i - 1];
            // Insert a hyphen on a camel boundary: lower->upper, or
            // letter->upper-followed-by-lower (e.g. "HTTPServer" ->
            // "HTTP-SERVER", "AzApp" -> "AZ-APP").
            let next = chars.get(i + 1).copied();
            let boundary = prev.is_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_uppercase() && next.map_or(false, |n| n.is_lowercase()));
            if boundary && !out.ends_with('-') {
                out.push('-');
            }
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c.to_ascii_uppercase());
        }
    }
    // Collapse double hyphens, trim hyphens at edges.
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

/// Truncate `name` to [`MAX_ID_LEN`] characters. If the name already fits,
/// return it unchanged; otherwise append a deterministic 4-char hex
/// suffix derived from the FNV-1a hash of the original name to keep
/// truncated identifiers globally unique. The truncated section retains
/// `MAX_ID_LEN - 5` characters so the dash + 4 hex digits fit.
pub fn mangle_identifier(name: &str) -> String {
    if name.len() <= MAX_ID_LEN {
        return name.to_string();
    }
    let hash = fnv1a_hex(name);
    // Reserve 5 chars for "-XXXX" suffix.
    let keep = MAX_ID_LEN.saturating_sub(5);
    let mut out: String = name.chars().take(keep).collect();
    // Strip a trailing hyphen so we don't double up.
    while out.ends_with('-') {
        out.pop();
    }
    out.push('-');
    out.push_str(&hash);
    out
}

/// Build the canonical COBOL identifier for a Rust/C name in one step:
/// case-convert, then truncate-and-mangle. Use this for every name that
/// will appear in a level-01 / level-05 / level-78 declaration.
pub fn cobol_identifier(name: &str) -> String {
    mangle_identifier(&to_cobol_case(name))
}

/// 4-char lowercase hex FNV-1a hash. Stable across runs and platforms.
fn fnv1a_hex(s: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("{:04x}", h & 0xffff)
}

// ============================================================================
// COBOL reserved-word table
// ============================================================================
//
// The list below covers the ANSI-85 + GnuCOBOL extension reserved words
// that are most likely to collide with field / argument names from
// api.json. A complete COBOL reserved-word list is several hundred
// entries; this is the practical subset. Anything in here gets a
// trailing `X` appended so it stops being a keyword while still being
// stable and short.

pub fn is_cobol_reserved(name_upper: &str) -> bool {
    matches!(
        name_upper,
        // Procedure / control-flow verbs
        "ACCEPT" | "ADD" | "ALTER" | "CALL" | "CANCEL" | "CLOSE" | "COMPUTE"
        | "CONTINUE" | "DELETE" | "DISPLAY" | "DIVIDE" | "ELSE" | "END" | "EVALUATE"
        | "EXIT" | "GOBACK" | "GO" | "IF" | "INITIALIZE" | "INSPECT" | "INVOKE"
        | "MERGE" | "MOVE" | "MULTIPLY" | "OPEN" | "PERFORM" | "READ" | "RELEASE"
        | "RETURN" | "REWRITE" | "SEARCH" | "SET" | "SORT" | "START" | "STOP"
        | "STRING" | "SUBTRACT" | "TERMINATE" | "UNSTRING" | "USE" | "WHEN"
        | "WRITE"
        // Data-division reserved
        | "BLANK" | "BLOCK" | "BY" | "CHARACTER" | "CLASS" | "CODE" | "COLUMN"
        | "COMMA" | "COMP" | "COMPUTATIONAL" | "COPY" | "CORRESPONDING"
        | "CURRENCY" | "DATA" | "DATE" | "DAY" | "DECIMAL-POINT" | "DEFAULT"
        | "DEPENDING" | "DESCENDING" | "DIVISION" | "EQUAL" | "ERROR" | "EVERY"
        | "EXCEPTION" | "EXTERNAL" | "FALSE" | "FILE" | "FILLER" | "FIRST"
        | "FOR" | "FROM" | "FUNCTION" | "GIVING" | "GLOBAL" | "GREATER" | "GROUP"
        | "HIGH-VALUE" | "HIGH-VALUES" | "I-O" | "IN" | "INDEX" | "INDEXED"
        | "INDICATE" | "INPUT" | "INPUT-OUTPUT" | "INTO" | "IS" | "JUST"
        | "JUSTIFIED" | "KEY" | "LABEL" | "LEADING" | "LEFT" | "LENGTH" | "LESS"
        | "LIB" | "LINAGE" | "LINE" | "LINES" | "LINKAGE" | "LOCAL-STORAGE"
        | "LOCK" | "LOW-VALUE" | "LOW-VALUES" | "MEMORY" | "MODE" | "MODULES"
        | "NATIVE" | "NEGATIVE" | "NEXT" | "NO" | "NOT" | "NULL" | "NULLS"
        | "NUMBER" | "NUMERIC" | "OBJECT" | "OCCURS" | "OF" | "OFF" | "OMITTED"
        | "ON" | "OR" | "ORDER" | "ORGANIZATION" | "OTHER" | "OUTPUT"
        | "OVERFLOW" | "PADDING" | "PAGE" | "PIC" | "PICTURE" | "PLUS"
        | "POINTER" | "POSITION" | "POSITIVE" | "PRESENT" | "PROCEDURE"
        | "PROCEDURES" | "PROCEED" | "PROGRAM" | "PROGRAM-ID" | "QUOTE"
        | "QUOTES" | "RAISE" | "RANDOM" | "RD" | "RECORD" | "RECORDS"
        | "RECURSIVE" | "REDEFINES" | "REEL" | "REFERENCE" | "RELATIVE"
        | "REMAINDER" | "REMOVAL" | "RENAMES" | "REPLACE" | "REPLACING"
        | "REPORT" | "REPORTING" | "REPORTS" | "REPOSITORY" | "RESERVE" | "RESET"
        | "RESUME" | "REVERSED" | "REWIND" | "RIGHT" | "ROUNDED" | "RUN"
        | "SAME" | "SCREEN" | "SD" | "SECTION" | "SECURITY" | "SELECT"
        | "SEPARATE" | "SEQUENCE" | "SEQUENTIAL" | "SHARING" | "SIGN" | "SIZE"
        | "SOURCE" | "SOURCE-COMPUTER" | "SPACE" | "SPACES" | "SPECIAL-NAMES"
        | "STANDARD" | "STANDARD-1" | "STANDARD-2" | "STATUS" | "SUCCESS"
        | "SUPPRESS" | "SYMBOLIC" | "SYNC" | "SYNCHRONIZED" | "TABLE" | "TALLY"
        | "TALLYING" | "TAPE" | "TEST" | "THAN" | "THEN" | "THROUGH" | "THRU"
        | "TIME" | "TIMES" | "TO" | "TOP" | "TRAILING" | "TRUE" | "TYPE"
        | "TYPEDEF" | "UNIT" | "UNTIL" | "UP" | "UPON" | "USAGE" | "USING"
        | "VALUE" | "VALUES" | "VARYING" | "WITH" | "WORDS" | "WORKING-STORAGE"
        | "ZERO" | "ZEROES" | "ZEROS"
        // GnuCOBOL screen / extension reserved words that field names
        // in api.json regularly collide with.
        | "WINDOW" | "ID" | "LAST" | "COL" | "ROW" | "TAB" | "BEEP"
        | "BACKGROUND" | "FOREGROUND" | "BACKGROUND-COLOR" | "FOREGROUND-COLOR"
        | "ALPHABETIC" | "ALPHABETIC-LOWER" | "ALPHABETIC-UPPER"
        | "ASCENDING" | "AT" | "AUTHOR" | "BELL" | "BINARY" | "BOTTOM"
        | "COLOR" | "COLUMNS" | "CONTENT" | "CONTROL" | "CONTROLS"
        | "CURSOR" | "DATE-COMPILED" | "DATE-WRITTEN" | "DEBUGGING"
        | "DECLARATIVES" | "ENVIRONMENT" | "EOL" | "EOS" | "EQUALS"
        | "ERASE" | "EXCEPTION-OBJECT" | "FOOTING" | "HIGHLIGHT"
        | "INHERITS" | "INSTALLATION" | "LIMIT" | "LIMITS" | "LINKAGE"
        | "LOCALE" | "LOWLIGHT" | "MORE-LABELS" | "NESTED" | "OBJECT-COMPUTER"
        | "OPTIONAL" | "OPTIONS" | "OVERRIDE" | "PROMPT" | "PROPERTY"
        | "PROTECTED" | "PUSH" | "RAISING" | "READY" | "REDIRECT"
        | "REVERSE" | "REVERSE-VIDEO" | "SCROLL" | "SEND" | "SENTENCE"
        | "SIGNED" | "STEP" | "TALLY-COUNT" | "TEXT" | "THREAD"
        | "TIME-OUT" | "TIMEOUT" | "TRACK" | "TRACKS" | "TYPE-OBJECT"
        | "UNSIGNED" | "VOID" | "WAIT"
        | "CENTER" | "TRANSFORM" | "STYLE" | "PROGRAM-POINTER"
        | "HEADER" | "FOOTER" | "TITLE" | "BUTTON" | "BORDER" | "MARGIN"
        | "PADDING" | "WIDTH" | "HEIGHT" | "X" | "Y" | "Z"
        | "POINT" | "POSITION" | "RECTANGLE" | "SIZE" | "BOUNDS"
        | "UPDATE" | "VALID" | "INVALID" | "ARGUMENT" | "ARGUMENTS"
        | "ACTIVE" | "ACTION" | "ASSIGN" | "AUTOMATIC" | "AWAY-FROM-ZERO"
        | "BIT" | "BOOLEAN" | "CHAINING" | "CHAINED" | "CHILD"
        | "COMPLEX" | "CONDITION" | "CONFIG" | "CRT" | "DECIMAL"
        | "ENABLED" | "ENTRY" | "FORMAT" | "FUNCTIONS" | "INDEX-1"
        | "INDEX-2" | "INVOKE" | "ITEM" | "LIMITS" | "LOCK-HOLD"
        | "LOCKING" | "MANUAL" | "MAX" | "METHOD" | "METHODS"
        | "MIN" | "MULTIPLE" | "NAMED" | "NUMERIC-EDITED"
        | "OK" | "ONLY" | "OUTPUT-FORMAT" | "PARSE" | "PHYSICAL"
        | "PRESENT-WHEN" | "PRINT" | "PROGRAM-NAME" | "RANGE"
        | "READ-ONLY" | "ROUNDING" | "SCROLL-X" | "SHADOW" | "STAGE"
        | "STANDARD-BINARY" | "STANDARD-DECIMAL" | "STATEMENT"
        | "STREAM" | "SYMBOL" | "TEXT-FILE" | "USER-DEFAULT" | "VAL"
        | "VARIANT" | "VOLATILE" | "VS" | "YYYYDDD" | "YYYYMMDD"
        | "CH" | "VS-9" | "DDD" | "DD" | "MM" | "YY" | "YYYY"
        | "NULL-FILE" | "FILE-CONTROL" | "FILE-STATUS"
        | "AREA" | "AREAS" | "EVENT" | "EVENTS" | "PRIORITY"
        | "ACTIVATING" | "ADJUSTABLE" | "BACKWARD" | "FORWARD"
        | "TIMER" | "TIMERS" | "TARGET" | "DETAIL" | "DETAILS"
        | "GROUP" | "LOCALES" | "MODULE-NAME" | "STATIC"
        | "DYNAMIC-TABLE" | "TASK" | "TASKS" | "WORD" | "WORDS"
        | "BYTES" | "MESSAGE" | "CELL" | "CELLS" | "BUTTONS"
        | "DEBUG" | "DEBUGGING-DUMP" | "ENGINE" | "FLAG" | "FLAGS"
        | "PAINT" | "VIEW" | "VIEWS" | "WIDGETS" | "WIDGET"
        | "GROUPS" | "ITEMS" | "MENU" | "MENUS" | "NODE" | "NODES"
        | "REFERENCED" | "REFERENCING" | "SECONDARY" | "PRIMARY"
        | "USER" | "USERS" | "AUTH" | "ROLE" | "ROLES"
        | "QUERY" | "QUEUE" | "INDEX-VALUE" | "STORE" | "STORES"
        | "VERIFY" | "VERIFIED"
        | "PIXEL" | "PIXELS" | "SMALL-FONT" | "SMALL" | "LARGE"
        | "AREA-WIDTH" | "FONT" | "FONTS" | "CAPTION"
        | "SCREEN-LINE" | "VISIBLE" | "INVISIBLE" | "ENGINE-NAME"
        | "MARK" | "MARKED" | "OUTPUT-BUFFER"
    )
}

/// Sanitize a COBOL identifier against the reserved-word list. A trailing
/// `X` is appended to keywords (the result is still a valid COBOL name
/// and remains within the 30-char budget for everything in api.json).
pub fn sanitize_cobol_identifier(name: &str) -> String {
    let upper = name.to_ascii_uppercase();
    // COBOL identifiers must start with a letter (not a digit). Tuple
    // struct fields from Rust come through with numeric names `0`,
    // `1`, ... — prefix them with `FIELD-` so the result is valid.
    let upper = if upper.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        format!("FIELD-{}", upper)
    } else {
        upper
    };
    if is_cobol_reserved(&upper) {
        format!("{}-X", upper)
    } else {
        upper
    }
}

// ============================================================================
// Comment helpers
// ============================================================================

/// Sanitize a documentation string for inclusion in a COBOL line comment.
/// COBOL line comments start with `*` in column 7 (fixed format) or
/// `*>` anywhere (free format). We use the fixed-format form throughout
/// the copybook for maximum tooling compatibility.
pub fn sanitize_doc(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}

/// Wrap a long doc string into multiple fixed-format COBOL comment lines.
/// Fixed-format source must be ≤ 72 characters total (cols 7–72 are the
/// content area), so each emitted line is `"      * "` plus up to
/// `MAX_COBOL_DOC_BODY` characters of content. Very long single-line
/// doc strings (Rust mod-level docs that include Markdown + code
/// examples) would otherwise raise "source text exceeds 512 bytes,
/// will be truncated" and break parsing of the next statement.
pub fn emit_doc_comment(builder: &mut crate::codegen::v2::generator::CodeBuilder, doc: &str) {
    // Free-format COBOL still has a 512-byte source-line limit (the
    // -Wothers warning fires above it and the trailing content is
    // dropped — which knocks out whatever statement follows). Wrap
    // long doc lines onto multiple `*> ...` rows of ≤ 100 body chars
    // so a single Rust mod-level docstring full of Markdown + code
    // examples doesn't blow past the limit.
    const MAX_COBOL_DOC_BODY: usize = 100;
    let cleaned = sanitize_doc(doc);
    if cleaned.is_empty() {
        return;
    }
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let mut current = String::new();
    for w in words {
        if !current.is_empty() && current.len() + 1 + w.len() > MAX_COBOL_DOC_BODY {
            builder.line(&format!("*> {}", current));
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        if w.len() > MAX_COBOL_DOC_BODY {
            builder.line(&format!("*> {}", &w[..MAX_COBOL_DOC_BODY]));
            continue;
        }
        current.push_str(w);
    }
    if !current.is_empty() {
        builder.line(&format!("*> {}", current));
    }
}
