# AZUL_INCLUDE: the directory holding azul.h; AZUL_LIB: the one holding libazul
AZUL_INCLUDE ?= .
AZUL_LIB ?= .

app: main.c ui.h
	$(CC) -o app main.c -I$(AZUL_INCLUDE) -L$(AZUL_LIB) -lazul
