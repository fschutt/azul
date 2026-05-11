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
# -ffree-line-length-none: lifts gfortran's default 132-char free-form
# line limit. Some of the generated `bind(C, name="...")` declarations
# combine a long Fortran identifier (truncated to 63 chars) plus the
# verbatim C symbol name and pass the limit on a single line. Without
# this flag the declarations are silently truncated and the wrapper
# bodies referencing them fail with "no IMPLICIT type".
FFLAGS  ?= -O2 -std=f2008 -fimplicit-none -ffree-line-length-none
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
