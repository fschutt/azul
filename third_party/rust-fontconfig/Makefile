CC = gcc
CFLAGS = -Wall -Werror -g
LDFLAGS = -L. -lrust_fontconfig
RUST_FLAGS = --release --features ffi
INCLUDE_DIR = include
LIB_NAME = rust_fontconfig

# Detect OS
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
  STATIC_EXT = .a
  DYNAMIC_EXT = .so
  LIB_PREFIX = lib
endif
ifeq ($(UNAME_S),Darwin)
  STATIC_EXT = .a
  DYNAMIC_EXT = .dylib
  LIB_PREFIX = lib
endif
ifneq ($(findstring MINGW,$(UNAME_S)),)
  STATIC_EXT = .lib
  DYNAMIC_EXT = .dll
  LIB_PREFIX = 
endif

# Default target based on OS
ifeq ($(OS),Windows_NT)
  all: win
else
  ifeq ($(UNAME_S),Darwin)
    all: mac
  else
    all: linux
  endif
endif

.PHONY: all clean linux mac win

linux: $(LIB_PREFIX)$(LIB_NAME)$(STATIC_EXT) $(LIB_PREFIX)$(LIB_NAME)$(DYNAMIC_EXT) example example_registry

mac: $(LIB_PREFIX)$(LIB_NAME)$(STATIC_EXT) $(LIB_PREFIX)$(LIB_NAME)$(DYNAMIC_EXT) example example_registry

win: $(LIB_NAME)$(STATIC_EXT) $(LIB_NAME)$(DYNAMIC_EXT) example.exe example_registry.exe

# Linux build
$(LIB_PREFIX)$(LIB_NAME)$(STATIC_EXT) $(LIB_PREFIX)$(LIB_NAME)$(DYNAMIC_EXT): src/ffi.rs
	cargo build $(RUST_FLAGS)
	cp target/release/$(LIB_PREFIX)$(LIB_NAME)$(STATIC_EXT) .
	cp target/release/$(LIB_PREFIX)$(LIB_NAME)$(DYNAMIC_EXT) .

# Windows build
$(LIB_NAME)$(STATIC_EXT) $(LIB_NAME)$(DYNAMIC_EXT): src/ffi.rs
	cargo build $(RUST_FLAGS)
	copy target\release\$(LIB_NAME)$(STATIC_EXT) .
	copy target\release\$(LIB_NAME)$(DYNAMIC_EXT) .

# Example build - Unix
example: ffi/example.c include/rust_fontconfig.h $(LIB_PREFIX)$(LIB_NAME)$(STATIC_EXT)
	$(CC) $(CFLAGS) -I$(INCLUDE_DIR) -o $@ $< $(LDFLAGS)

example_registry: ffi/example_registry.c include/rust_fontconfig.h $(LIB_PREFIX)$(LIB_NAME)$(STATIC_EXT)
	$(CC) $(CFLAGS) -I$(INCLUDE_DIR) -o $@ $< $(LDFLAGS)

# Example build - Windows
example.exe: ffi/example.c include/rust_fontconfig.h $(LIB_NAME)$(STATIC_EXT)
	cl.exe /W4 /EHsc /Fe:example.exe ffi/example.c /I$(INCLUDE_DIR) /link /LIBPATH:. $(LIB_NAME)$(STATIC_EXT)

example_registry.exe: ffi/example_registry.c include/rust_fontconfig.h $(LIB_NAME)$(STATIC_EXT)
	cl.exe /W4 /EHsc /Fe:example_registry.exe ffi/example_registry.c /I$(INCLUDE_DIR) /link /LIBPATH:. $(LIB_NAME)$(STATIC_EXT)

# Header file
include/rust_fontconfig.h: ffi/rust_fontconfig.h
	mkdir -p include
	cp ffi/rust_fontconfig.h include/

clean:
	rm -f example example.exe example_registry example_registry.exe
	rm -f $(LIB_PREFIX)$(LIB_NAME)$(STATIC_EXT) $(LIB_PREFIX)$(LIB_NAME)$(DYNAMIC_EXT)
	rm -f $(LIB_NAME)$(STATIC_EXT) $(LIB_NAME)$(DYNAMIC_EXT)
	cargo clean