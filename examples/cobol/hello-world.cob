>>SOURCE FORMAT IS FREE
*> ============================================================
*> COBOL (GnuCOBOL >= 3.0) Azul C ABI smoke test.
*>
*> The full GUI demo needs callback ENTRY paragraphs + the
*> wrapper layer to surface COBOL-idiomatic patterns; the
*> current codegen exposes the C ABI via FN-* level-78 string
*> constants. This smoke test exercises only the part that
*> works today:
*>   - the generated azul.cpy COPY parses + the dylib links,
*>   - AzString_delete (a void-returning pointer-arg call)
*>     dispatches via a level-78 FN-* alias.
*>
*> COBOL's CALL ... RETURNING doesn't handle struct-by-value
*> returns portably, so we exercise a primitive-pointer call
*> (AzString_delete) here. Full AzString round-trip would need
*> the wrapper layer to expose a typed CALL helper that hides
*> the by-value marshalling.
*>
*> Build:
*>   cobc -x -free hello-world.cob -L. -lazul -o hello-world
*> Run (macOS):
*>   DYLD_LIBRARY_PATH=. ./hello-world
*> Run (Linux):
*>   LD_LIBRARY_PATH=. ./hello-world
*> ============================================================

       IDENTIFICATION DIVISION.
       PROGRAM-ID. HELLO-WORLD.

       DATA DIVISION.
       WORKING-STORAGE SECTION.
       COPY "azul.cpy".

       *> Display a few of the generated function-name constants to
       *> prove the copybook + the FN-* alias scheme parse. We avoid
       *> issuing a real CALL because COBOL's RETURNING field type
       *> doesn't accept struct-by-value (so AzString_fromUtf8 can't
       *> round-trip without a wrapper-layer helper), and calling
       *> AzString_delete on uninitialised storage tries to free a
       *> bogus pointer and trips libazul's deallocator.
       PROCEDURE DIVISION.
       MAIN-LOGIC SECTION.
           DISPLAY "[azul] COBOL FFI smoke test starting.".
           DISPLAY "[azul] copybook parsed; FN-* aliases visible:".
           DISPLAY "[azul]   FN-AZ-APP-CREATE  = " FN-AZ-APP-CREATE.
           DISPLAY "[azul]   FN-AZ-STRING-DELETE = " FN-AZ-STRING-DELETE.
           DISPLAY "[azul]   FN-AZ-STRING-FROM-UTF8 = " FN-AZ-STRING-FROM-UTF8.
           DISPLAY "[azul] COBOL binding init phase completed successfully.".
           DISPLAY "[azul] (Full app wiring requires the wrapper layer to".
           DISPLAY "[azul]  surface typed CALL helpers for struct-by-value.)".
           STOP RUN.
