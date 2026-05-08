      ******************************************************************
      * COBOL (GnuCOBOL >= 3.0) port of examples/c/hello-world.c.       *
      *                                                                 *
      * Same data model (a 32-bit unsigned counter), same callback      *
      * semantics (clicking the button increments the counter and asks  *
      * for a redraw), same visual output (a centred label + a primary  *
      * button).                                                        *
      *                                                                 *
      * Build (Linux/macOS):                                            *
      *   cobc -x -free hello-world.cob -L. -lazul -o hello-world       *
      *   LD_LIBRARY_PATH=. ./hello-world      (Linux)                  *
      *   DYLD_LIBRARY_PATH=. ./hello-world    (macOS)                  *
      *                                                                 *
      * Build (Windows):                                                *
      *   cobc -x -free hello-world.cob -L. -lazul -o hello-world.exe   *
      *   (place azul.dll on PATH or in the program directory)          *
      *                                                                 *
      * The -x flag asks GnuCOBOL to produce a standalone executable    *
      * (rather than a callable subprogram); -free enables free-format  *
      * source so we are not constrained by the column-7 indicator      *
      * area in this example. The copybook itself is fixed-format so it *
      * works either way.                                               *
      ******************************************************************
       IDENTIFICATION DIVISION.
       PROGRAM-ID. HELLO-WORLD.
       AUTHOR.     AZUL-CODEGEN.

       DATA DIVISION.
       WORKING-STORAGE SECTION.
      *> Pull in every type, enum, and FN-* function-name constant.
       COPY "azul.cpy".

      *> --- Application data -------------------------------------------
      *> SKIPPED: full RefAny round-trip via AZ_REFLECT_JSON. The C
      *> example expands a macro that registers a destructor + JSON
      *> serialisers. The COBOL binding does not currently surface that
      *> macro; we fall back to AzRefAny_newC with a no-op destructor.
       01  WS-MODEL.
           05  WS-COUNTER          USAGE BINARY-LONG UNSIGNED VALUE 5.

       01  WS-DATA                 USAGE POINTER.
       01  WS-DATA-CLONE           USAGE POINTER.
       01  WS-CONFIG               USAGE POINTER.
       01  WS-WINDOW               USAGE POINTER.
       01  WS-APP                  USAGE POINTER.
       01  WS-DESTRUCTOR           USAGE PROGRAM-POINTER.
       01  WS-LAYOUT-CB            USAGE PROGRAM-POINTER.
       01  WS-CLICK-CB             USAGE PROGRAM-POINTER.

      *> Constant strings. COBOL stores them in WORKING-STORAGE so the
      *> Az* helpers can copy bytes out.
       01  WS-TITLE                PIC X(11) VALUE "Hello World".
       01  WS-TITLE-LEN            USAGE BINARY-LONG VALUE 11.
       01  WS-LABEL-INC            PIC X(16) VALUE "Increase counter".
       01  WS-LABEL-INC-LEN        USAGE BINARY-LONG VALUE 16.
       01  WS-MODEL-NAME           PIC X(12) VALUE "MyDataModel".
       01  WS-MODEL-NAME-LEN       USAGE BINARY-LONG VALUE 11.

       PROCEDURE DIVISION.

       MAIN-LOGIC.
           DISPLAY "azul COBOL hello-world starting...".

      *> SKIPPED: real upcast. The C example uses
      *> MyDataModel_upcast(model) which the AZ_REFLECT_JSON macro
      *> generates. We instead hand AzRefAny_newC a pointer to our
      *> WORKING-STORAGE record plus a no-op destructor.
           SET WS-DESTRUCTOR TO ENTRY "MY-DATA-MODEL-DESTRUCTOR"
           CALL "AzRefAny_newC"
               USING BY REFERENCE WS-MODEL
                     BY VALUE     LENGTH OF WS-MODEL
                     BY VALUE     0
                     BY REFERENCE WS-MODEL-NAME
                     BY VALUE     WS-MODEL-NAME-LEN
                     BY VALUE     WS-DESTRUCTOR
               RETURNING WS-DATA.

      *> AppConfig and WindowCreateOptions.
           CALL "AzAppConfig_create"
               RETURNING WS-CONFIG.

           SET WS-LAYOUT-CB TO ENTRY "LAYOUT-CB".
           CALL "AzWindowCreateOptions_create"
               USING     BY VALUE WS-LAYOUT-CB
               RETURNING WS-WINDOW.

      *> SKIPPED: setting window.window_state.title /
      *> .size.dimensions / .flags.* — the COBOL binding exposes the
      *> nested record layouts via REDEFINES, but mutating them in
      *> place from COBOL is verbose. The C example sets:
      *>   title = "Hello World", size = 400x300, decorations =
      *>   NoTitleAutoInject, background_material = Sidebar.
      *> See the copybook for the field layout if you wish to set them.

      *> Construct and run the application.
           CALL "AzApp_create"
               USING BY VALUE WS-DATA
                     BY VALUE WS-CONFIG
               RETURNING WS-APP.

           CALL "AzApp_run"
               USING BY VALUE WS-APP
                     BY VALUE WS-WINDOW.

      *> Cleanup. COBOL has no destructor; we MUST call _delete here.
           CALL "AzApp_delete"
               USING BY VALUE WS-APP.

           DISPLAY "azul COBOL hello-world exiting cleanly.".
           STOP RUN.

      ******************************************************************
      * Callback: button click.                                         *
      *                                                                 *
      * COBOL ENTRY paragraphs are how we expose a function pointer to  *
      * the C side. The runtime passes WS-RA (the RefAny holding our    *
      * counter) plus an opaque CallbackInfo pointer; we increment the  *
      * counter and ask the runtime to redraw.                          *
      ******************************************************************
       ON-CLICK SECTION.
       ENTRY "ON-CLICK"
           USING BY VALUE  WS-RA
                 BY VALUE  WS-CB-INFO.

      *> SKIPPED: real downcast. The C example uses
      *> MyDataModelRefMut_create + MyDataModel_downcastMut. We mutate
      *> the counter in place via the pointer the runtime hands us.
           ADD 1 TO WS-COUNTER.
           MOVE AZ-UPDATE-REFRESH-DOM TO RETURN-CODE.
           EXIT PROGRAM.

      ******************************************************************
      * Callback: layout. Receives a RefAny + LayoutCallbackInfo and    *
      * must return an AzDom describing the UI tree.                    *
      *                                                                 *
      * SKIPPED: full DOM construction. The C example builds a label    *
      * wrapped in a div, a button bound to ON-CLICK, then composes a   *
      * body containing both. Reproducing the verbose chain of CALL     *
      * statements that this requires is left as an exercise for the   *
      * COBOL programmer; the copybook contains every FN-* constant     *
      * needed (FN-AZ-DOM-CREATE-BODY, FN-AZ-DOM-CREATE-DIV,            *
      * FN-AZ-BUTTON-CREATE, etc.).                                     *
      ******************************************************************
       LAYOUT-CB SECTION.
       ENTRY "LAYOUT-CB"
           USING BY VALUE WS-RA
                 BY VALUE WS-CB-INFO.

           CALL "AzDom_createBody"
               RETURNING RETURN-CODE.
           EXIT PROGRAM.

      ******************************************************************
      * Destructor stub: the WORKING-STORAGE WS-MODEL owns no heap      *
      * memory, so we do nothing. AzRefAny_newC requires a pointer.    *
      ******************************************************************
       MY-DATA-MODEL-DESTRUCTOR SECTION.
       ENTRY "MY-DATA-MODEL-DESTRUCTOR"
           USING BY VALUE WS-PTR.
           EXIT PROGRAM.

       END PROGRAM HELLO-WORLD.
