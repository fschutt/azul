---
slug: hello-world/perl
title: Hello World [Perl]
language: en
canonical_slug: hello-world/perl
audience: external
maturity: wip
guide_order: 28
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/perl/hello-world.pl
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - WindowCreateOptions
  - Update
---

# Hello World [Perl]

## Introduction

The Perl binding calls the prebuilt `libazul` native library through
[`FFI::Platypus`](https://metacpan.org/pod/FFI::Platypus) 2.x — no XS or native
compile step. Idiomatic wrappers live under `Azul::<Type>`; the raw C-ABI
surface is under `Azul::FFI::*`. Callbacks route through libazul's host-invoker
plumbing so `FFI::Platypus` never has to synthesize a struct-by-value
trampoline.

## Installation

You need **Perl 5.30+** and **`FFI::Platypus` 2.x**, plus the native `libazul`
library. On macOS use Homebrew Perl (system Perl can't write to its
`site_perl`):

```sh
cpanm FFI::Platypus
cp /path/to/target/release/libazul.dylib examples/perl/
```

## Running

The loader finds the library via `AZ_LIB_DIR` (or the directory holding
`Azul.pm`):

```sh
cd examples/perl
AZ_LIB_DIR=. perl -Ilib hello-world.pl
```

## The program

`examples/perl/hello-world.pl` builds a counter: a `32px` label showing the
count and an "Increase counter" button that bumps it.

```perl
use FindBin qw($Bin);
use lib "$Bin/lib";
use Azul;
use FFI::Platypus::Buffer qw(scalar_to_buffer);

my $model = { counter => 5 };            # any Perl ref works as the data model

my $on_click = sub {
    my ($data, $info) = @_;
    my $m = Azul::refany_get($data);
    return Azul::AzUpdate::DoNothing() unless defined $m;
    $m->{counter}++;
    return Azul::AzUpdate::RefreshDom();
};

my $layout = sub {
    my ($data, $info) = @_;
    my $m = Azul::refany_get($data);
    my $body = Azul::FFI::AzDom_createBody();
    return $body unless defined $m;
    # ... build div{font-size:32px} > text(counter) + Button ...
    return $body;                         # returns a raw AzDom record
};
```

### How callbacks work

* **`Azul::refany_create($value)`** wraps a Perl value in an `AzRefAny` (an
  opaque host handle); **`Azul::refany_get($data)`** recovers it inside a
  callback. Pass the same model to the app and to the button so both see the
  same counter.
* **`Azul::register_callback('<Kind>', $sub)`** returns the matching
  `Az<Kind>` record. The generated invoker fires `$sub` and writes its return
  value back through the callback out-pointer for you — a layout callback just
  returns a `Dom`, an on-click callback returns an `AzUpdate`.
* The `LayoutCallback` is installed into `window_state.layout_callback` before
  `App::run`.

Build the DOM with the raw `Azul::FFI::AzDom_*` / `Azul::FFI::AzButton_*`
calls (as the example does): these are the exact by-value struct calls the
host-invoker thunks expect, and the raw records carry no destructor, so the
moved-out structs are never double-freed.

## Status

The full counter demo passes the headless E2E
(`scripts/e2e_language_matrix.sh perl` → `✓ WORKS`, counter 5 → 6 → 8). See
`examples/perl/README.md` for the internals (sticky-closure trampolines,
real-size return writeback, sentinel-probed layout-callback splice) and the
one remaining codegen limitation (over-sized Perl tagged-union records).
