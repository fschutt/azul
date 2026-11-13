>>SOURCE FORMAT IS FREE
*> Copy target/codegen/azul.cpy and libazul here, then:
*>   cobc -x -free main.cob styles.cob -L. -lazul -o main && ./main
IDENTIFICATION DIVISION.
PROGRAM-ID. MAIN-STYLES.
DATA DIVISION.
WORKING-STORAGE SECTION.
COPY "azul.cpy".
01  ws-value-1 USAGE TYAZ-CSS-PROPERTY-WITH-CO-8550.
PROCEDURE DIVISION.
    CALL "STYLE-BTN" USING BY REFERENCE ws-value-1 END-CALL
    DISPLAY "STYLE-BTN: built"
    STOP RUN.
END PROGRAM MAIN-STYLES.
