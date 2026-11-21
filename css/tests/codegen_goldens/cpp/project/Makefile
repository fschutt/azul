# AZUL_INCLUDE: the directory holding azul17.hpp + azul.h; AZUL_LIB: the one holding libazul
AZUL_INCLUDE ?= .
AZUL_LIB ?= .

app: main.cpp styles.hpp
	$(CXX) -std=c++17 -o app main.cpp -I$(AZUL_INCLUDE) -L$(AZUL_LIB) -lazul
