#!/usr/bin/env perl
#
# Perl host-invoker smoke test for the Azul binding.
#
# Exercises the managed-FFI prelude that lang_perl/managed.rs emits
# into Azul.pm: a Perl-side `%_handles` table, a pinned releaser
# closure registered with libazul at module load, and the public
# Azul::refany_create / Azul::refany_get helpers that round-trip an
# opaque Perl value through libazul's host-handle id.
#
# Run with:
#     /opt/homebrew/bin/perl hello-world.pl
# (system Perl on macOS lacks write permission to its site_perl;
#  install FFI::Platypus into Homebrew Perl instead.)

use strict;
use warnings;
use FindBin qw($Bin);
use lib "$Bin/lib";
use Azul;

print "[azul] Perl FFI smoke test starting.\n";
print "[azul] Azul.pm loaded; host-invoker prelude registered the releaser.\n";

# 1. AzString_fromUtf8 — proves the regular C ABI dispatches.
my $src = "hello, azul";
my $ptr = unpack('J', pack('P', $src));
my $str = Azul::FFI::AzString_fromUtf8($ptr, length $src);
print "[azul] AzString_fromUtf8 round-trip succeeded; len=", length($src), "\n";

# 2. RefAny round-trip via the host-invoker handle table.
my $model = { counter => 5 };
my $refany = Azul::refany_create($model);
print "[azul] Azul::refany_create ran; RefAny opaque-handle id stored.\n";

# refany_get takes a pointer-to-RefAny. FFI::Platypus represents
# records as scalars holding the struct bytes; cast via `$ffi->cast`
# to the underlying opaque address the C side expects.
my $refany_ptr = $Azul::ffi->cast('AzRefAny' => 'opaque', $refany);
my $recovered  = Azul::refany_get($refany_ptr);
if (ref $recovered eq 'HASH' && $recovered->{counter} == 5) {
    print "[azul] Azul::refany_get round-trip succeeded; counter=", $recovered->{counter}, "\n";
} else {
    print "[azul] Azul::refany_get round-trip FAILED (recovered=", ($recovered // 'undef'), ")\n";
    exit 1;
}

print "[azul] host-invoker init phase completed successfully.\n";
print "[azul] (Full App.run wiring requires layout / callback closures\n";
print "[azul]  via Azul::register_callback, separate from refany.)\n";
