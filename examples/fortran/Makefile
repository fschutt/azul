# ============================================================================
# Makefile for the Fortran hello-world example.
#
# Drop libazul.{so,dylib} (Linux/macOS) or azul.dll (Windows) into this
# directory next to azul.f90 and hello_world.f90, then:
#
#     make            - build hello_world
#     make run        - run the example
#     make clean      - remove build artifacts
#
# Override FC=ifort or FC=ifx to use the Intel Fortran compiler.
# ============================================================================

FC      ?= gfortran
FFLAGS  ?= -O2 -std=f2008 -fimplicit-none
LDFLAGS ?= -L. -Wl,-rpath,'$$ORIGIN'
LIBS    ?= -lazul

EXE     := hello_world

all: $(EXE)

# `azul.f90` produces both `azul.o` and an `azul.mod` (compiler-managed
# module file) that hello_world.f90 USEs.

azul.o azul.mod: azul.f90
	$(FC) $(FFLAGS) -c azul.f90

hello_world.o: hello_world.f90 azul.mod
	$(FC) $(FFLAGS) -c hello_world.f90

$(EXE): hello_world.o azul.o
	$(FC) $(FFLAGS) hello_world.o azul.o $(LDFLAGS) $(LIBS) -o $(EXE)

run: $(EXE)
	./$(EXE)

clean:
	rm -f *.o *.mod $(EXE)

.PHONY: all run clean
