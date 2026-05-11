>>SOURCE FORMAT IS FREE
*> ============================================================
*> COBOL (GnuCOBOL >= 3.0) Azul host-invoker smoke test.
*>
*> COBOL has no closures and CALL ... RETURNING doesn't accept a
*> TYPEDEF record, so the host-invoker pattern requires the user's
*> PROCEDURE DIVISION to provide ENTRY paragraphs for the releaser
*> + per-kind dispatchers (see the comment block above the FN-*
*> host-invoker aliases in azul.cpy for the suggested scaffolding).
*>
*> This smoke test exercises what the codegen surface DOES provide:
*>   - the generated azul.cpy COPY parses (including the new
*>     managed-FFI level-78 aliases for the host-invoker C symbols),
*>   - FN-AZ-APP-SET-HOST-HANDLE-9778 and FN-AZ-REF-ANY-NEW-HOST-HANDLE
*>     resolve as level-78 constants ready to use in CALL statements.
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

       PROCEDURE DIVISION.
       MAIN-LOGIC SECTION.
           DISPLAY "[azul] COBOL FFI smoke test starting.".
           DISPLAY "[azul] copybook parsed; FN-* aliases visible:".
           DISPLAY "[azul]   FN-AZ-APP-CREATE  = " FN-AZ-APP-CREATE.
           DISPLAY "[azul]   FN-AZ-STRING-DELETE = " FN-AZ-STRING-DELETE.
           DISPLAY "[azul]   FN-AZ-STRING-FROM-UTF8 = " FN-AZ-STRING-FROM-UTF8.
           DISPLAY "[azul] host-invoker C-symbol aliases:".
           DISPLAY "[azul]   FN-AZ-APP-SET-HOST-HANDLE-9778 = "
               FN-AZ-APP-SET-HOST-HANDLE-9778.
           DISPLAY "[azul]   FN-AZ-REF-ANY-NEW-HOST-HANDLE = "
               FN-AZ-REF-ANY-NEW-HOST-HANDLE.
           DISPLAY "[azul]   FN-AZ-REF-ANY-GET-HOST-HANDLE = "
               FN-AZ-REF-ANY-GET-HOST-HANDLE.
           DISPLAY "[azul] COBOL binding init phase completed successfully.".
           DISPLAY "[azul] (Full app wiring requires user-side ENTRY".
           DISPLAY "[azul]  paragraphs for the per-kind invoker dispatchers".
           DISPLAY "[azul]  -- see scaffolding example in azul.cpy header.)".
           STOP RUN.
