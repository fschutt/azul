>>SOURCE FORMAT IS FREE
*> Copy target/codegen/azul.cpy and libazul here, then:
*>   cobc -x -free main.cob styles.cob -L. -lazul -o main && ./main
IDENTIFICATION DIVISION.
PROGRAM-ID. MAIN-STYLES.
DATA DIVISION.
WORKING-STORAGE SECTION.
COPY "azul.cpy".
PROCEDURE DIVISION.
    STOP RUN.
END PROGRAM MAIN-STYLES.
