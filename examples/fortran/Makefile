ifeq ($(origin FC),default)
FC      = gfortran
endif
FFLAGS  ?= -O2 -std=f2008 -ffree-line-length-none -fimplicit-none
LDFLAGS ?= -L. -Wl,-rpath,'$$ORIGIN'
LIBS    ?= -lazul

EXE     := hello_world

all: $(EXE)

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
