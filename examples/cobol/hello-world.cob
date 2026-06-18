>>SOURCE FORMAT IS FREE
*> cobc -x -free hello-world.cob -L. -lazul -o hello-world && LD_LIBRARY_PATH=. ./hello-world

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
